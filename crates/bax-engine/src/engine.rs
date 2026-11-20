//! 引擎的对外句柄。

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use bax_batrace::{ApiError, BatraceClient};
use bax_core::reminder::{self, ReminderRule};
use bax_core::{MatchId, PlayerId};
use bax_gamelog::LogZone;
use bax_store::{HistoryRow, PlayerHit, Store, StoreError};
use serde::Serialize;
use tokio::sync::{Notify, mpsc, oneshot, watch};
use tokio::task::JoinHandle;

use crate::clock::Clock;
use crate::config::EngineConfig;
use crate::player::PlayerDetail;
use crate::postmatch::{self, MatchReportView};
use crate::reports::{ReportTiming, ReportWorker};
use crate::session::{Command, LOG_DIR_KEY, Parts, REMINDERS_KEY, SELF_ID_KEY, Session, UI_PREFIX};
use crate::snapshot::Snapshot;

/// 启动失败的原因。
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("无法打开长期库：{0}")]
    Store(#[from] StoreError),
    #[error("接口客户端配置错误：{0}")]
    Api(#[from] ApiError),
}

/// 命令没有执行。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CommandError {
    #[error("引擎已经停止")]
    Stopped,
    /// 命令被拒绝，内容是给使用者看的原因。
    #[error("{0}")]
    Rejected(String),
}

/// 一次历史日志导入的结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ImportSummary {
    /// 读完的日志文件数。
    pub files: u32,
    /// 写入长期库的不同比赛数。
    pub matches: u32,
    /// 读不了或写不进去的文件数。
    pub failed: u32,
}

/// 正在运行的引擎。丢弃句柄会让引擎停下；需要等它收尾时用 [`Engine::shutdown`]。
#[derive(Debug)]
pub struct Engine {
    commands: mpsc::Sender<Command>,
    snapshots: watch::Receiver<Arc<Snapshot>>,
    task: JoinHandle<()>,
    /// 单独刷新一名玩家时直接用它发请求，不占用会话。
    client: BatraceClient,
    store_path: PathBuf,
    log_zone: LogZone,
    /// 战后归档任务。
    reports: JoinHandle<()>,
    spec_reports: JoinHandle<()>,
    report_notify: Arc<Notify>,
    /// 按需读取比赛记录与战报用的连接，不经过对局会话。
    reader: Arc<Mutex<Store>>,
    clock: Arc<dyn Clock>,
}

impl Engine {
    /// 打开长期库并启动引擎。必须在 tokio 运行时里调用。
    pub fn start(config: EngineConfig, clock: Arc<dyn Clock>) -> Result<Engine, EngineError> {
        let started = Instant::now();
        let store = Store::open(&config.store_path).inspect_err(|err| {
            tracing::error!(store = %config.store_path.display(), error = %err, "无法打开长期库");
        })?;
        let store_path = config.store_path.clone();
        let log_zone = config.log_zone;
        let client = BatraceClient::new(config.api.clone())?;

        let saved_dir = store
            .setting::<String>(LOG_DIR_KEY)
            .ok()
            .flatten()
            .map(PathBuf::from);
        let log_dir_source = match (&config.log_dir, &saved_dir) {
            (Some(_), _) => "启动参数",
            (None, Some(_)) => "设置",
            (None, None) => "自动定位",
        };
        let log_dir = config.log_dir.clone().or(saved_dir);
        let self_hint = remembered_self(&store);
        let rules = saved_rules(&store);
        let ui = saved_ui_settings(&store);

        let config_store = config.store_path.clone();
        let (snapshot_tx, snapshots) = watch::channel(Arc::new(Snapshot::initial()));
        let (commands, command_rx) = mpsc::channel(16);

        // 战后归档与按需读取各用自己的连接；WAL 下读写互不阻塞
        let report_notify = Arc::new(Notify::new());
        let (report_tx, report_rx) = mpsc::unbounded_channel();
        let spec_store = Store::open(&config.store_path)?;
        let spec_events = report_tx.clone();
        let worker = ReportWorker {
            store: Store::open(&config.store_path)?,
            client: client.clone(),
            clock: Arc::clone(&clock),
            timing: ReportTiming::from_config(&config),
            notify: Arc::clone(&report_notify),
            events: report_tx,
        };
        let reader = Arc::new(Mutex::new(Store::open(&config.store_path)?));

        let session = Session::new(Parts {
            config,
            clock: Arc::clone(&clock),
            store,
            client: client.clone(),
            log_dir,
            self_hint,
            rules,
            ui,
            snapshot_tx,
            report_notify: Arc::clone(&report_notify),
            report_rx,
        });
        let task = tokio::spawn(session.run(command_rx));
        let reports = tokio::spawn(worker.run());
        let spec_reports = tokio::spawn(crate::spec_worker::run(
            spec_store,
            client.clone(),
            Arc::clone(&clock),
            snapshots.clone(),
            spec_events,
        ));
        tracing::info!(
            store = %config_store.display(),
            log_dir_source = %log_dir_source,
            elapsed_ms = started.elapsed().as_millis() as u64,
            "引擎启动"
        );
        Ok(Engine {
            commands,
            snapshots,
            task,
            client,
            store_path,
            log_zone,
            reports,
            spec_reports,
            report_notify,
            reader,
            clock,
        })
    }

    /// 最新的快照。
    pub fn snapshot(&self) -> Arc<Snapshot> {
        Arc::clone(&self.snapshots.borrow())
    }

    /// 订阅快照。接收端只保留最新的一份，处理得慢不会积压。
    pub fn subscribe(&self) -> watch::Receiver<Arc<Snapshot>> {
        self.snapshots.clone()
    }

    /// 重新获取当前比赛全部玩家的资料。
    pub async fn refresh_players(&self) -> Result<(), CommandError> {
        self.send(Command::RefreshPlayers).await
    }

    /// 改用另一个日志目录并记住这个选择；`None` 表示改回自动定位。
    /// 返回时新的日志线程已经启动，当前比赛的状态已经清空。
    pub async fn set_log_dir(&self, dir: Option<PathBuf>) -> Result<(), CommandError> {
        let (done, wait) = oneshot::channel();
        self.send(Command::SetLogDir { dir, done }).await?;
        wait.await.map_err(|_| CommandError::Stopped)
    }

    /// 替换全部提醒规则并保存。
    pub async fn set_reminders(&self, rules: Vec<ReminderRule>) -> Result<(), CommandError> {
        let (done, wait) = oneshot::channel();
        self.send(Command::SetReminders { rules, done }).await?;
        reply(wait).await
    }

    /// 手动校准当前比赛的计时：此刻处于正式开局后 `elapsed_ms` 毫秒，准备阶段里为负数。
    pub async fn calibrate(&self, elapsed_ms: i64) -> Result<(), CommandError> {
        let (done, wait) = oneshot::channel();
        self.send(Command::Calibrate { elapsed_ms, done }).await?;
        reply(wait).await
    }

    /// 保存一项界面设置。键必须以 `ui.` 开头；值为 `null` 时删除这一项。
    pub async fn set_ui_setting(
        &self,
        key: impl Into<String>,
        value: serde_json::Value,
    ) -> Result<(), CommandError> {
        let (done, wait) = oneshot::channel();
        self.send(Command::SetUiSetting {
            key: key.into(),
            value,
            done,
        })
        .await?;
        reply(wait).await
    }

    /// 一名玩家的完整档案；长期库里没有这名玩家时为 `None`。
    pub async fn player_detail(&self, id: PlayerId) -> Result<Option<PlayerDetail>, CommandError> {
        let (done, wait) = oneshot::channel();
        self.send(Command::PlayerDetail { id, done }).await?;
        reply_with(wait).await
    }

    /// 按名字或 ID 查找玩家；查询为空时列出最近见过的玩家。
    pub async fn search_players(
        &self,
        query: impl Into<String>,
        limit: u32,
    ) -> Result<Vec<PlayerHit>, CommandError> {
        self.search_players_filtered(query, limit, 0, "default".into(), "all".into())
            .await
    }

    pub async fn search_players_filtered(
        &self,
        query: impl Into<String>,
        limit: u32,
        offset: u32,
        sort: String,
        filter: String,
    ) -> Result<Vec<PlayerHit>, CommandError> {
        let (done, wait) = oneshot::channel();
        self.send(Command::SearchPlayers {
            query: query.into(),
            limit,
            offset,
            sort,
            filter,
            done,
        })
        .await?;
        reply_with(wait).await
    }

    /// 保存一名玩家的备注；内容为空时删除。
    pub async fn set_note(
        &self,
        id: PlayerId,
        body: impl Into<String>,
    ) -> Result<(), CommandError> {
        let (done, wait) = oneshot::channel();
        self.send(Command::SetNote {
            id,
            body: body.into(),
            done,
        })
        .await?;
        reply(wait).await
    }

    /// 立即从接口取一次这名玩家的资料并保存，不管他在不在当前比赛里。
    pub async fn refresh_player(&self, id: PlayerId) -> Result<(), CommandError> {
        let fetched = self
            .client
            .player_info(id)
            .await
            .map_err(|err| CommandError::Rejected(err.to_string()))?;
        let (done, wait) = oneshot::channel();
        self.send(Command::SaveProfile {
            id,
            fetched: Box::new(fetched),
            done,
        })
        .await?;
        reply(wait).await
    }

    /// 把当前日志目录里的全部历史日志导入长期库，用于统计以前同场的次数。
    ///
    /// 在后台线程里用另一个数据库连接导入，不阻塞对局会话；完成后刷新当前比赛的同场次数。
    /// 重复导入不会产生重复记录。
    pub async fn import_history(&self) -> Result<ImportSummary, CommandError> {
        let dir = self
            .snapshot()
            .log
            .dir
            .clone()
            .map(PathBuf::from)
            .ok_or_else(|| CommandError::Rejected("还没有找到日志目录".to_owned()))?;
        let store_path = self.store_path.clone();
        let zone = self.log_zone;
        let summary = tokio::task::spawn_blocking(move || -> Result<ImportSummary, String> {
            let files = bax_gamelog::tail::list_log_files(&dir).map_err(|e| e.to_string())?;
            let mut store = Store::open(&store_path).map_err(|e| e.to_string())?;
            let report = crate::history::import_logs(&mut store, &files, zone, |_, _| {});
            Ok(ImportSummary {
                files: report.files as u32,
                matches: report.matches as u32,
                failed: report.failed.len() as u32,
            })
        })
        .await
        .map_err(|err| CommandError::Rejected(err.to_string()))?
        .map_err(CommandError::Rejected)?;
        tracing::info!(
            files = summary.files,
            matches = summary.matches,
            failed = summary.failed,
            "历史日志导入完成"
        );
        self.send(Command::ReloadArchive).await?;
        Ok(summary)
    }

    /// 比赛记录，新的在前，带战报状态。
    pub async fn match_history(
        &self,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<HistoryRow>, CommandError> {
        self.read(move |store| {
            store
                .match_history(limit.min(500), offset)
                .map_err(|e| e.to_string())
        })
        .await
    }

    /// 一场比赛的战后分析；还没有战报时为 `None`。
    pub async fn match_report(&self, id: MatchId) -> Result<Option<MatchReportView>, CommandError> {
        self.read(move |store| postmatch::load_view(store, id))
            .await
    }

    /// 立即去取一场比赛的战报；取不到时按时间表继续等。已经有战报时什么也不做。
    pub async fn fetch_report(&self, id: MatchId) -> Result<(), CommandError> {
        let now = self.clock.now();
        let queued = self
            .read(move |store| store.request_report(id, now).map_err(|e| e.to_string()))
            .await?;
        if queued {
            tracing::info!(match_id = %id, "手动请求战报");
            self.report_notify.notify_one();
        }
        Ok(())
    }

    /// 在阻塞线程池里用读取连接做一件事。
    async fn read<T, F>(&self, f: F) -> Result<T, CommandError>
    where
        T: Send + 'static,
        F: FnOnce(&mut Store) -> Result<T, String> + Send + 'static,
    {
        let reader = Arc::clone(&self.reader);
        tokio::task::spawn_blocking(move || {
            let mut store = reader.lock().map_err(|_| "读取连接不可用".to_owned())?;
            f(&mut store)
        })
        .await
        .map_err(|err| CommandError::Rejected(err.to_string()))?
        .map_err(CommandError::Rejected)
    }

    /// 停下引擎，等它收尾。
    pub async fn shutdown(self) {
        let Engine {
            commands,
            task,
            reports,
            spec_reports,
            ..
        } = self;
        drop(commands);
        let _ = task.await;
        // 会话停下后归档任务会自己退出；正在等网络的请求不必等完
        reports.abort();
        spec_reports.abort();
        let _ = reports.await;
        let _ = spec_reports.await;
    }

    async fn send(&self, command: Command) -> Result<(), CommandError> {
        self.commands
            .send(command)
            .await
            .map_err(|_| CommandError::Stopped)
    }
}

async fn reply(wait: oneshot::Receiver<Result<(), String>>) -> Result<(), CommandError> {
    match wait.await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(reason)) => Err(CommandError::Rejected(reason)),
        Err(_) => Err(CommandError::Stopped),
    }
}

async fn reply_with<T>(wait: oneshot::Receiver<Result<T, String>>) -> Result<T, CommandError> {
    match wait.await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(reason)) => Err(CommandError::Rejected(reason)),
        Err(_) => Err(CommandError::Stopped),
    }
}

/// 以前记住的本机玩家 ID；没有记过时，从历史名单里推断。
///
/// 本机玩家出现在自己的每一场比赛里，所以出现场次接近比赛总数的那个 ID 就是本机玩家。
fn remembered_self(store: &Store) -> Option<PlayerId> {
    if let Ok(Some(id)) = store.setting::<PlayerId>(SELF_ID_KEY) {
        return Some(id);
    }
    let (id, count, total) = store.most_common_player().ok().flatten()?;
    (count >= 5 && u64::from(count) * 10 >= u64::from(total) * 8).then_some(id)
}

/// 保存的提醒规则；没有保存过，或者保存的内容不合法时用默认规则。
fn saved_rules(store: &Store) -> Vec<ReminderRule> {
    match store.setting::<Vec<ReminderRule>>(REMINDERS_KEY) {
        Ok(Some(rules)) if reminder::validate(&rules).is_ok() => rules,
        Ok(None) => reminder::default_rules(),
        Ok(Some(_)) | Err(_) => {
            tracing::warn!("保存的提醒规则无法使用，改用默认规则");
            reminder::default_rules()
        }
    }
}

fn saved_ui_settings(store: &Store) -> BTreeMap<String, serde_json::Value> {
    let rows = store.settings_with_prefix(UI_PREFIX).unwrap_or_else(|err| {
        tracing::warn!(error = %err, "读取界面设置失败");
        Vec::new()
    });
    rows.into_iter()
        .filter_map(|(key, raw)| Some((key, serde_json::from_str(&raw).ok()?)))
        .collect()
}
