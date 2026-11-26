//! 对局会话：引擎里唯一维护当前比赛状态的任务。
//!
//! 所有输入（日志事件、接口结果、界面命令、计时）都以消息形式进来，按顺序处理，
//! 状态因此不需要加锁。会话的职责：
//!
//! - 把日志事件交给对局跟踪器，把变化写进长期库；
//! - 为当前比赛的玩家准备资料：先用本地已有的，再按需从接口刷新；
//! - 每次状态变化后发布一份快照。
//!
//! 刷新策略：一名玩家的资料如果不是在这场比赛开始之后取得的，就重新取一次（“每场动态更新”）。
//! 等待室里先到的玩家立即开始查，名单到了再补齐其余的人。启动时补读出来的历史比赛不发请求，
//! 只有补读结束时仍在进行的那一场会刷新。
//!
//! 请求调度的做法来自 BA-ELO-Overlay（`src/ba_elo_overlay.py`）：`EloService` 的有限并发、
//! 先用旧资料兜底和换局后丢弃旧结果，`_warm` 的等待室预热，`_fill_levels` 的延时重试。

use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bax_batrace::model::PlayerInfoResponse;
use bax_batrace::{ApiError, BatraceClient, Fetched};
use bax_core::reminder::{self, ReminderOutcome, ReminderRule};
use bax_core::{
    BattleClock, ClockSource, Connection, LogEvent, MatchId, Phase, PlayerId, Team, Timestamp,
    Tracker, Update,
};
use bax_gamelog::{LogTime, Origin, WatchItem};
use bax_store::{EncounterSummary, PlayerHit, PlayerRecord, ProfileUpdate, Store};
use tokio::sync::{Notify, mpsc, oneshot, watch};
use tokio::time::MissedTickBehavior;

use crate::clock::Clock;
use crate::config::EngineConfig;
use crate::logthread::{LogMsg, LogThread, LogTiming};
use crate::player::{PlayerDetail, ProfileLookup};
use crate::profile::ProfileSummary;
use crate::reports::ReportEvent;
use crate::snapshot::{
    AlertView, ApiErrorView, ApiStatus, ArchiveStatus, FetchState, LogStatus, MatchView,
    PlayerView, ReminderView, Snapshot, TeamSummary,
};

/// 设置项：本机玩家的 ID。
pub(crate) const SELF_ID_KEY: &str = "self.playerId";
/// 设置项：用户指定的日志目录。
pub(crate) const LOG_DIR_KEY: &str = "log.dir";
/// 设置项：提醒规则。
pub(crate) const REMINDERS_KEY: &str = "reminders";
/// 界面自己的设置都以这个前缀开头。引擎只负责保存和分发，不解释内容。
pub(crate) const UI_PREFIX: &str = "ui.";
/// 一项界面设置序列化之后的长度上限。
const UI_VALUE_LIMIT: usize = 64 * 1024;
/// 一条备注的字数上限。
const NOTE_LIMIT: usize = 2000;
/// 玩家档案里列出的同场比赛数上限。
const DETAIL_MATCH_LIMIT: u32 = 200;

pub(crate) const LOG_CHANNEL_CAPACITY: usize = 256;
/// 详细档位下，多久写一行运行摘要。
const SUMMARY_INTERVAL: Duration = Duration::from_secs(5 * 60);

/// 界面发给会话的命令。
#[derive(Debug)]
pub(crate) enum Command {
    /// 重新获取当前比赛全部玩家的资料，不管是否已经取过。
    RefreshPlayers,
    /// 换一个日志目录；`None` 表示改回自动定位。
    SetLogDir {
        dir: Option<PathBuf>,
        done: oneshot::Sender<Result<(), String>>,
    },
    /// 替换全部提醒规则。
    SetReminders {
        rules: Vec<ReminderRule>,
        done: oneshot::Sender<Result<(), String>>,
    },
    /// 手动校准：当前比赛此刻处于正式开局后 `elapsed_ms` 毫秒，准备阶段里为负数。
    Calibrate {
        elapsed_ms: i64,
        done: oneshot::Sender<Result<(), String>>,
    },
    /// 保存一项界面设置，键必须以 `ui.` 开头。
    SetUiSetting {
        key: String,
        value: serde_json::Value,
        done: oneshot::Sender<Result<(), String>>,
    },
    /// 一名玩家的完整档案。
    PlayerDetail {
        id: PlayerId,
        done: oneshot::Sender<Result<Option<PlayerDetail>, String>>,
    },
    /// 按名字或 ID 查找玩家；查询为空时列出最近见过的。
    SearchPlayers {
        query: String,
        limit: u32,
        offset: u32,
        sort: String,
        filter: String,
        done: oneshot::Sender<Result<Vec<PlayerHit>, String>>,
    },
    /// 保存备注；内容为空时删除。
    SetNote {
        id: PlayerId,
        body: String,
        done: oneshot::Sender<Result<(), String>>,
    },
    /// 保存一份单独取得的资料（主窗口里“刷新这名玩家”）。请求由调用方发出，会话只负责写入。
    SaveProfile {
        id: PlayerId,
        lookup: Box<ProfileLookup>,
        done: oneshot::Sender<Result<(), String>>,
    },
    /// 长期库被别处改过（例如导入了历史日志）：重新读取当前比赛玩家的同场次数与备注。
    ReloadArchive,
}

#[derive(Debug)]
struct FetchResult {
    player: PlayerId,
    /// 发起请求时的比赛代次。比赛换了之后到达的结果不再更新界面。
    generation: u64,
    at: Timestamp,
    elapsed_ms: u64,
    lookup: ProfileLookup,
}

/// 上一次记录诊断日志时的比赛状态，用来发现变化。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Seen {
    match_id: MatchId,
    phase: Phase,
    connection: Connection,
    clock: Option<BattleClock>,
}

/// 一场比赛里资料请求的统计，比赛结束或被取代时写一行汇总。
#[derive(Debug, Default)]
struct FetchStats {
    requests: u32,
    ok: u32,
    failures: BTreeMap<String, u32>,
    total_ms: u64,
}

/// 当前比赛里的一名玩家。
#[derive(Debug)]
struct Slot {
    id: PlayerId,
    log_name: Option<String>,
    team: Option<Team>,
    record: Option<PlayerRecord>,
    profile: Option<ProfileSummary>,
    encounters: EncounterSummary,
    note: Option<String>,
    fetch: FetchState,
    failures: usize,
}

impl Slot {
    fn fetched_at(&self) -> Option<Timestamp> {
        self.record.as_ref().and_then(|r| r.fetched_at)
    }

    /// 资料不是在 `since` 之后取得的，而且没有请求在排队或进行。
    fn needs_fetch(&self, since: Timestamp) -> bool {
        self.fetch == FetchState::Idle && self.fetched_at().is_none_or(|at| at < since)
    }
}

pub(crate) struct Parts {
    pub config: EngineConfig,
    pub clock: Arc<dyn Clock>,
    pub store: Store,
    pub client: BatraceClient,
    pub log_dir: Option<PathBuf>,
    pub self_hint: Option<PlayerId>,
    pub rules: Vec<ReminderRule>,
    pub ui: BTreeMap<String, serde_json::Value>,
    pub snapshot_tx: watch::Sender<Arc<Snapshot>>,
    /// 比赛结束时叫醒战后归档任务。
    pub report_notify: Arc<Notify>,
    pub report_rx: mpsc::UnboundedReceiver<ReportEvent>,
}

pub(crate) struct Session {
    pregame: crate::pregame::PregameView,
    pregame_cutoff: u64,
    config: EngineConfig,
    clock: Arc<dyn Clock>,
    store: Store,
    client: BatraceClient,
    tracker: Tracker,
    self_hint: Option<PlayerId>,

    snapshot_tx: watch::Sender<Arc<Snapshot>>,
    revision: u64,
    dirty: bool,

    log: LogStatus,
    log_thread: Option<LogThread>,
    log_rx: mpsc::Receiver<LogMsg>,

    fetch_tx: mpsc::UnboundedSender<FetchResult>,
    fetch_rx: mpsc::UnboundedReceiver<FetchResult>,
    generation: u64,
    slots: Vec<Slot>,
    queue: VecDeque<PlayerId>,
    in_flight: usize,
    paused_until: Option<Timestamp>,
    last_error: Option<ApiErrorView>,

    rules: Vec<ReminderRule>,
    /// 当前比赛里各条规则的结果。
    outcomes: Vec<(u32, ReminderOutcome, Timestamp)>,
    alert: Option<AlertView>,
    ui: BTreeMap<String, serde_json::Value>,

    report_notify: Arc<Notify>,
    report_rx: mpsc::UnboundedReceiver<ReportEvent>,
    archive: ArchiveStatus,

    // 诊断日志（ADR-0007）
    /// 当前比赛的 span。比赛相关的记录都在它里面，按 match_id 可以过滤出一场。
    match_span: tracing::Span,
    seen: Option<Seen>,
    /// 正在处理的游戏日志事件，记阶段变化的起因。
    cause: Option<(&'static str, LogTime)>,
    fetch_stats: FetchStats,
    backfill_events: u64,
    log_started: Instant,
    last_summary: Instant,
}

impl Session {
    pub(crate) fn new(parts: Parts) -> Session {
        let Parts {
            config,
            clock,
            store,
            client,
            log_dir,
            self_hint,
            rules,
            ui,
            snapshot_tx,
            report_notify,
            report_rx,
        } = parts;
        let mut tracker = Tracker::new(config.tracker.clone());
        tracker.set_self_hint(self_hint);
        let (fetch_tx, fetch_rx) = mpsc::unbounded_channel();
        // 先放一个空的接收端，马上由 start_log_thread 换成真正的
        let (_, log_rx) = mpsc::channel(1);
        let mut session = Session {
            pregame: Default::default(),
            pregame_cutoff: 0,
            config,
            clock,
            store,
            client,
            tracker,
            self_hint,
            snapshot_tx,
            revision: 0,
            dirty: false,
            log: LogStatus::default(),
            log_thread: None,
            log_rx,
            fetch_tx,
            fetch_rx,
            generation: 0,
            slots: Vec::new(),
            queue: VecDeque::new(),
            in_flight: 0,
            paused_until: None,
            last_error: None,
            rules,
            outcomes: Vec::new(),
            alert: None,
            ui,
            report_notify,
            report_rx,
            archive: ArchiveStatus::default(),
            match_span: tracing::Span::none(),
            seen: None,
            cause: None,
            fetch_stats: FetchStats::default(),
            backfill_events: 0,
            log_started: Instant::now(),
            last_summary: Instant::now(),
        };
        session.start_log_thread(log_dir);
        session
    }

    pub(crate) async fn run(mut self, mut commands: mpsc::Receiver<Command>) {
        let mut tick = tokio::time::interval(self.config.tick_interval);
        tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
        tracing::info!(
            rules = self.rules.len(),
            self_known = self.self_hint.is_some(),
            "对局会话启动"
        );
        self.publish();
        let (ocr_tx, mut ocr_rx) = mpsc::channel(4);
        #[cfg(feature = "ocr")]
        let ocr_task = self.config.ocr_runtime.clone().map(|root| {
            tokio::spawn(crate::pregame::worker::run(
                root,
                self.config.store_path.clone(),
                self.client.clone(),
                self.snapshot_tx.subscribe(),
                ocr_tx,
            ))
        });
        #[cfg(feature = "ocr")]
        let _ocr_guard = OcrGuard(ocr_task);
        #[cfg(not(feature = "ocr"))]
        drop(ocr_tx);
        let mut trace_tasks = tokio::task::JoinSet::new();
        let mut trace_ticket = None;
        let mut trace_attempted = std::collections::BTreeSet::new();
        loop {
            let input = tokio::select! {
                command = commands.recv() => match command {
                    Some(command) => Input::Command(command),
                    // 引擎句柄被丢弃
                    None => break,
                },
                Some(msg) = self.log_rx.recv() => Input::Log(msg),
                Some(result) = self.fetch_rx.recv() => Input::Fetched(Box::new(result)),
                Some(event) = self.report_rx.recv() => Input::Report(event),
                Some(message) = ocr_rx.recv() => Input::Pregame(message),
                Some(Ok((key, id, at, count))) = trace_tasks.join_next(), if !trace_tasks.is_empty() =>
                    Input::PregameTrace(key, id, at, count),
                _ = tick.tick() => Input::Tick,
            };
            // 处理过程中的诊断记录都挂在当前比赛的 span 下
            let span = self.match_span.clone();
            let _entered = span.enter();
            match input {
                Input::Pregame(message) => self.on_pregame(message),
                Input::PregameTrace(key, id, at, count) => {
                    if trace_ticket.as_ref() == Some(&key)
                        && self
                            .pregame
                            .players
                            .iter()
                            .any(|p| p.matched_id() == Some(id))
                        && let Some(count) = count
                        && let Err(err) = self.save_trace_count(id, count, at)
                    {
                        tracing::warn!(player = %id, error = %err, "保存 BATrace 标记次数失败");
                    }
                }
                Input::Command(command) => self.on_command(command),
                Input::Log(msg) => self.on_log(msg),
                Input::Fetched(result) => self.on_fetched(*result),
                Input::Report(event) => self.on_report_event(event),
                Input::Tick => self.on_tick(),
            }
            let ticket = crate::pregame::ticket(&self.build_snapshot());
            if ticket != trace_ticket {
                trace_tasks.abort_all();
                trace_attempted.clear();
                trace_ticket = ticket.clone();
            }
            if let Some(key) = ticket {
                // 单独串行查询已匹配的 ID，不占用 OCR 身份查询额度。
                if trace_tasks.is_empty()
                    && let Some(id) = self
                        .pregame
                        .players
                        .iter()
                        .filter_map(|p| p.matched_id())
                        .find(|id| trace_attempted.insert(*id))
                {
                    let client = self.client.clone();
                    let at = self.clock.now();
                    trace_tasks.spawn(async move {
                        (key, id, at, client.player_trace_count(id).await.ok())
                    });
                }
            } else {
                if self.pregame != Default::default() {
                    self.dirty = true;
                }
                self.pregame = Default::default();
                self.pregame_cutoff = self.revision + 1;
            }
            self.dispatch();
            if self.dirty && self.log.ready {
                self.publish();
            }
        }
        self.log_fetch_summary();
        tracing::info!("对局会话结束");
        // 唤醒可能正在向满队列发送补读事件的日志线程，再等待它退出。
        self.log_rx.close();
        if let Some(handle) = self.log_thread.as_mut().and_then(LogThread::stop) {
            let _ = tokio::task::spawn_blocking(move || handle.join()).await;
        }
    }

    // ------------------------------------------------------------ 输入

    fn on_pregame(&mut self, mut message: crate::pregame::Message) {
        if message.revision >= self.pregame_cutoff
            && crate::pregame::ticket(&self.build_snapshot()).as_ref() == Some(&message.ticket)
        {
            for p in &mut message.view.players {
                let Some(id) = p.matched_id() else {
                    p.trace_count = None;
                    continue;
                };
                p.trace_count = self
                    .pregame
                    .players
                    .iter()
                    .find(|old| old.matched_id() == Some(id))
                    .map(|old| old.trace_count)
                    .unwrap_or_else(|| {
                        self.store
                            .player(id)
                            .ok()
                            .flatten()
                            .and_then(|r| r.trace_count)
                    });
                if let Some(candidate) = p.candidates.first_mut()
                    && let Some(profile) = candidate.profile.as_mut()
                {
                    let previous = self
                        .pregame
                        .players
                        .iter()
                        .flat_map(|p| &p.candidates)
                        .find(|c| c.id == candidate.id)
                        .and_then(|c| c.profile.as_ref());
                    if let Some(old) = previous {
                        crate::specs::copy_usage(old, profile);
                    } else {
                        crate::specs::enrich(profile, &self.store, candidate.id);
                    }
                }
            }
            if self.pregame == message.view {
                return;
            }
            self.pregame = message.view;
            self.dirty = true;
        }
    }

    fn on_command(&mut self, command: Command) {
        if matches!(&command, Command::SetUiSetting { key, .. } if key == "ui.ocr") {
            self.pregame = Default::default();
            self.pregame_cutoff = self.revision + 1;
        }
        match command {
            Command::RefreshPlayers => {
                let ids: Vec<PlayerId> = self.slots.iter().map(|s| s.id).collect();
                for id in ids {
                    if let Some(slot) = self.slot_mut(id) {
                        slot.failures = 0;
                        if matches!(slot.fetch, FetchState::Failed { .. }) {
                            slot.fetch = FetchState::Idle;
                        }
                    }
                    self.enqueue(id);
                }
                self.dirty = true;
            }
            Command::SetLogDir { dir, done } => {
                let saved = match &dir {
                    Some(dir) => self
                        .store
                        .set_setting(LOG_DIR_KEY, &dir.to_string_lossy().into_owned()),
                    None => self.store.remove_setting(LOG_DIR_KEY).map(|_| ()),
                };
                if let Err(err) = saved {
                    tracing::warn!(error = %err, "保存日志目录设置失败");
                    let _ = done.send(Err(format!("保存日志目录失败：{err}")));
                    return;
                }
                match &dir {
                    Some(dir) => tracing::info!(dir = %dir.display(), "切换游戏日志目录"),
                    None => tracing::info!("游戏日志目录改回自动定位"),
                }
                self.reset_match_state();
                self.start_log_thread(dir);
                self.publish();
                let _ = done.send(Ok(()));
            }
            Command::SetReminders { rules, done } => {
                let _ = done.send(self.set_reminders(rules));
            }
            Command::Calibrate { elapsed_ms, done } => {
                let _ = done.send(self.calibrate(elapsed_ms));
            }
            Command::SetUiSetting { key, value, done } => {
                let _ = done.send(self.set_ui_setting(key, value));
            }
            Command::PlayerDetail { id, done } => {
                let _ = done.send(self.player_detail(id).map_err(|e| e.to_string()));
            }
            Command::SearchPlayers {
                query,
                limit,
                offset,
                sort,
                filter,
                done,
            } => {
                let _ = done.send(
                    self.store
                        .search_players_in_match(
                            &query,
                            limit.min(500),
                            offset,
                            &sort,
                            &filter,
                            (
                                self.self_hint,
                                self.tracker
                                    .current()
                                    .filter(|m| {
                                        m.phase != Phase::Ended
                                            && m.my_team().is_some_and(|team| team.is_playing())
                                    })
                                    .map(|m| m.match_id),
                            ),
                        )
                        .map_err(|e| e.to_string()),
                );
            }
            Command::SetNote { id, body, done } => {
                let _ = done.send(self.set_note(id, &body));
            }
            Command::SaveProfile { id, lookup, done } => {
                let result = self.save_single_profile(id, *lookup);
                let _ = done.send(result);
            }
            Command::ReloadArchive => {
                let current = self.tracker.current().map(|m| m.match_id);
                for index in 0..self.slots.len() {
                    let id = self.slots[index].id;
                    if let Ok(encounters) = self.store.encounters(id, current) {
                        self.slots[index].encounters = encounters;
                    }
                    if let Ok(note) = self.store.note(id) {
                        self.slots[index].note = note.map(|n| n.body);
                    }
                }
                self.dirty = true;
                self.publish();
            }
        }
    }

    fn player_detail(&self, id: PlayerId) -> bax_store::Result<Option<PlayerDetail>> {
        let record = self.store.player(id)?;
        let note = self.store.note(id)?;
        if record.is_none() && note.is_none() {
            return Ok(None);
        }
        let profile = crate::specs::from_store(
            record.as_ref().and_then(|r| r.profile_json.as_deref()),
            &self.store,
            id,
        );
        let self_id = self
            .tracker
            .current()
            .and_then(|m| m.self_id)
            .or(self.self_hint);
        Ok(Some(PlayerDetail {
            id,
            name: record.as_ref().and_then(|r| r.name.clone()),
            log_name: record.as_ref().and_then(|r| r.last_log_name.clone()),
            steam_id: record.as_ref().and_then(|r| r.steam_id.clone()),
            trace_count: record.as_ref().and_then(|r| r.trace_count),
            trace_checked_at: record.as_ref().and_then(|r| r.trace_checked_at),
            profile,
            fetched_at: record.as_ref().and_then(|r| r.fetched_at),
            first_seen_at: record.as_ref().and_then(|r| r.first_seen_at),
            last_seen_at: record.as_ref().and_then(|r| r.last_seen_at),
            note,
            encounters: self.store.encounters(id, None)?,
            history: self.store.rating_history(id)?,
            matches: self.store.encounter_list(id, DETAIL_MATCH_LIMIT)?,
            is_self: self_id == Some(id),
        }))
    }

    fn set_note(&mut self, id: PlayerId, body: &str) -> Result<(), String> {
        if body.chars().count() > NOTE_LIMIT {
            return Err(format!("备注不能超过 {NOTE_LIMIT} 个字"));
        }
        let now = self.clock.now();
        self.store
            .set_note(id, body, now)
            .map_err(|e| e.to_string())?;
        // 备注正文是使用者写的，可能含名字，诊断日志只记字数（ADR-0007）
        tracing::info!(player = %id, chars = body.chars().count(), "保存备注");
        let note = self.store.note(id).ok().flatten().map(|n| n.body);
        if let Some(slot) = self.slot_mut(id) {
            slot.note = note;
            self.dirty = true;
            self.publish();
        }
        Ok(())
    }

    fn save_trace_count(
        &mut self,
        id: PlayerId,
        count: u32,
        at: Timestamp,
    ) -> bax_store::Result<()> {
        self.store.save_trace_count(id, count, at)?;
        let record = self.store.player(id)?;
        let saved = record.as_ref().and_then(|r| r.trace_count);
        if let Some(slot) = self.slot_mut(id) {
            slot.record = record;
            self.dirty = true;
        }
        for p in &mut self.pregame.players {
            if p.matched_id() == Some(id) && p.trace_count != saved {
                p.trace_count = saved;
                self.dirty = true;
            }
        }
        Ok(())
    }

    fn save_single_profile(&mut self, id: PlayerId, lookup: ProfileLookup) -> Result<(), String> {
        if let Some(count) = lookup.trace_count {
            self.save_trace_count(id, count, lookup.started_at)
                .map_err(|e| e.to_string())?;
        }
        let fetched = lookup.profile.map_err(|err| err.to_string())?;
        let now = self.clock.now();
        self.store
            .save_profile(&profile_update(id, &fetched), now)
            .map_err(|e| e.to_string())?;
        if self.slots.iter().any(|s| s.id == id) {
            let record = self.store.player(id).ok().flatten();
            let profile = crate::specs::from_store(
                record.as_ref().and_then(|r| r.profile_json.as_deref()),
                &self.store,
                id,
            );
            if let Some(slot) = self.slot_mut(id) {
                slot.profile = profile;
                slot.record = record;
                slot.failures = 0;
                if matches!(slot.fetch, FetchState::Failed { .. }) {
                    slot.fetch = FetchState::Idle;
                }
            }
            self.dirty = true;
            self.publish();
        }
        Ok(())
    }

    fn set_reminders(&mut self, rules: Vec<ReminderRule>) -> Result<(), String> {
        reminder::validate(&rules).map_err(|err| err.to_string())?;
        self.store
            .set_setting(REMINDERS_KEY, &rules)
            .map_err(|err| err.to_string())?;
        self.rules = rules;
        tracing::info!(rules = self.rules.len(), "提醒规则已更新");
        self.dirty = true;
        // 不等日志就绪：设置界面需要立刻看到结果
        self.publish();
        Ok(())
    }

    fn calibrate(&mut self, elapsed_ms: i64) -> Result<(), String> {
        let Some(m) = self.tracker.current() else {
            return Err("当前没有比赛".to_owned());
        };
        if m.phase == Phase::Ended {
            return Err("比赛已经结束".to_owned());
        }
        let match_id = m.match_id;
        let previous = m.clock;
        let clock = BattleClock {
            start_at: self.clock.now().plus_millis(-elapsed_ms),
            source: ClockSource::Manual,
        };
        tracing::info!(
            elapsed_ms,
            previous_source = previous.map(|c| tracing::field::debug(c.source)),
            shift_ms = previous.map(|c| clock.start_at.millis_since(c.start_at)),
            "手动校准计时"
        );
        self.tracker.set_clock(match_id, clock);
        self.store
            .set_match_clock(match_id, clock)
            .map_err(|err| err.to_string())?;
        let now = self.clock.now();
        let updates = self.tracker.advance(now);
        self.handle_updates(updates, true);
        self.dirty = true;
        self.publish();
        Ok(())
    }

    fn set_ui_setting(&mut self, key: String, value: serde_json::Value) -> Result<(), String> {
        if !key.starts_with(UI_PREFIX) || key.len() <= UI_PREFIX.len() {
            return Err(format!("界面设置的键必须以 {UI_PREFIX} 开头"));
        }
        if value.to_string().len() > UI_VALUE_LIMIT {
            return Err("设置内容过长".to_owned());
        }
        if self.ui.get(&key) == Some(&value) {
            return Ok(());
        }
        tracing::debug!(key = %key, "保存界面设置");
        let saved = if value.is_null() {
            self.store.remove_setting(&key).map(|_| ())
        } else {
            self.store.set_setting(&key, &value)
        };
        saved.map_err(|err| err.to_string())?;
        if value.is_null() {
            self.ui.remove(&key);
        } else {
            self.ui.insert(key, value);
        }
        self.dirty = true;
        self.publish();
        Ok(())
    }

    /// 检查提醒是否到点。每条规则每场比赛只有一个结果，记进长期库，重启后不会重复。
    fn check_reminders(&mut self, now: Timestamp) {
        let Some(m) = self.tracker.current() else {
            return;
        };
        if !matches!(m.phase, Phase::Deployment | Phase::InBattle) {
            return;
        }
        let Some(clock) = m.clock else {
            return;
        };
        let match_id = m.match_id;
        let outcomes = &self.outcomes;
        let due = reminder::due(
            &self.rules,
            clock,
            now,
            self.config.reminder_grace_ms,
            |id| outcomes.iter().any(|(rule, _, _)| *rule == id),
        );
        for (rule_id, outcome) in due {
            let rule = self.rules.iter().find(|r| r.id == rule_id);
            tracing::info!(
                rule_id,
                outcome = ?outcome,
                at_secs = rule.map(|r| r.at_secs),
                late_ms = rule.map(|r| now.millis_since(reminder::fire_at(r, clock))),
                "提醒结果"
            );
            if let Err(err) = self.store.record_reminder(match_id, rule_id, outcome, now) {
                tracing::warn!(error = %err, "保存提醒结果失败");
            }
            self.outcomes.push((rule_id, outcome, now));
            if outcome == ReminderOutcome::Fired {
                let label = self
                    .rules
                    .iter()
                    .find(|r| r.id == rule_id)
                    .map(|r| r.label.clone())
                    .unwrap_or_default();
                let seq = self.alert.as_ref().map_or(1, |a| a.seq + 1);
                self.alert = Some(AlertView {
                    seq,
                    match_id,
                    rule_id,
                    label,
                    at: now,
                });
            }
            self.dirty = true;
        }
    }

    fn on_log(&mut self, msg: LogMsg) {
        match msg {
            LogMsg::NotFound => {
                self.log.searching = false;
                self.publish();
            }
            LogMsg::Watching { dir } => {
                self.log = LogStatus {
                    dir: Some(dir.to_string_lossy().into_owned()),
                    ..LogStatus::default()
                };
                self.publish();
            }
            LogMsg::Failed { message } => {
                self.log.error = Some(message);
                self.publish();
            }
            LogMsg::Recovered => {
                self.log.error = None;
                self.dirty = true;
            }
            LogMsg::Item(item) => self.on_watch_item(item),
        }
    }

    fn on_watch_item(&mut self, item: WatchItem) {
        match item {
            WatchItem::FileOpened { path, origin } => {
                tracing::info!(
                    file = %path.display(),
                    bytes = std::fs::metadata(&path).map(|m| m.len()).ok(),
                    backfill = origin == Origin::Backfill,
                    "开始读取游戏日志文件"
                );
                self.log.file = Some(path.to_string_lossy().into_owned());
                self.dirty = true;
            }
            WatchItem::Truncated { path } => {
                tracing::warn!(file = %path.display(), "日志文件被截断，从头重读");
            }
            WatchItem::BackfillDone => {
                tracing::info!(
                    events = self.backfill_events,
                    elapsed_ms = self.log_started.elapsed().as_millis() as u64,
                    "补读完成，转入实时"
                );
                self.log.ready = true;
                let now = self.clock.now();
                let updates = self.tracker.advance(now);
                self.handle_updates(updates, true);
                // 补读结束时还在进行的比赛：用户是在对局中途打开的 BAX
                if self
                    .tracker
                    .current()
                    .is_some_and(|m| m.phase != Phase::Ended)
                {
                    self.enqueue_stale();
                }
                self.dirty = true;
            }
            WatchItem::Event { origin, event } => {
                let live = origin == Origin::Live;
                let kind = event_kind(&event.event);
                if live {
                    tracing::debug!(event = %kind, log_time = %event.at, "游戏日志事件");
                } else {
                    self.backfill_events += 1;
                    tracing::trace!(event = %kind, log_time = %event.at, "补读的游戏日志事件");
                }
                let at = event.at.to_utc(self.config.log_zone);
                self.cause = Some((kind, event.at));
                let mut updates = self.tracker.apply(at, &event.event);
                if let LogEvent::RoomClient { room, player, .. } = &event.event {
                    let current = self.tracker.current();
                    let accepted = updates.iter().any(|u| {
                        matches!(u,
                        Update::EarlyPlayers { players, .. } if players.contains(player))
                    });
                    let reason = if accepted {
                        "accepted"
                    } else if *room != bax_core::RoomKind::WaitingRoom {
                        "not_waiting_room"
                    } else if let Some(m) = current {
                        if m.phase > Phase::Loading {
                            "phase"
                        } else if m.connection != bax_core::Connection::Connected {
                            "connection"
                        } else if !m.roster.is_empty() {
                            "roster_known"
                        } else if m.early.iter().any(|p| p.id == *player) {
                            "duplicate"
                        } else {
                            "ignored"
                        }
                    } else {
                        "no_match"
                    };
                    tracing::debug!(
                        match_id = current.map(|m| tracing::field::display(m.match_id)),
                        player = %player, room = ?room, accepted, reason = %reason, live,
                        log_time = %event.at,
                        early_players = current.map(|m| m.early.len()),
                        "房间玩家线索处理"
                    );
                }
                if !live {
                    updates.extend(self.tracker.advance(at));
                }
                self.handle_updates(updates, live);
                self.cause = None;
            }
        }
    }

    fn on_tick(&mut self) {
        if !self.log.ready {
            return;
        }
        let now = self.clock.now();
        let updates = self.tracker.advance(now);
        self.handle_updates(updates, true);
        self.check_reminders(now);
        let due: Vec<PlayerId> = self
            .slots
            .iter()
            .filter(|slot| {
                matches!(slot.fetch, FetchState::Failed { retry_at: Some(at), .. } if at <= now)
            })
            .map(|slot| slot.id)
            .collect();
        for id in due {
            if let Some(slot) = self.slot_mut(id) {
                slot.fetch = FetchState::Idle;
            }
            self.enqueue(id);
        }
        if self.paused_until.is_some_and(|until| until <= now) {
            self.paused_until = None;
            tracing::info!("限流暂停结束，恢复请求");
            self.dirty = true;
        }
        if self.last_summary.elapsed() >= SUMMARY_INTERVAL {
            self.last_summary = Instant::now();
            tracing::debug!(
                file = %self.log.file.as_deref().unwrap_or("-"),
                players = self.slots.len(),
                queued = self.queue.len(),
                in_flight = self.in_flight,
                revision = self.revision,
                "运行摘要"
            );
        }
    }

    fn on_fetched(&mut self, result: FetchResult) {
        self.in_flight = self.in_flight.saturating_sub(1);
        self.dirty = true;
        let FetchResult {
            player,
            generation,
            at,
            elapsed_ms,
            lookup,
        } = result;
        if let Some(count) = lookup.trace_count
            && let Err(err) = self.save_trace_count(player, count, lookup.started_at)
        {
            tracing::warn!(player = %player, error = %err, "保存 BATrace 标记次数失败");
        }
        let current = generation == self.generation;
        if current {
            self.fetch_stats.requests += 1;
            self.fetch_stats.total_ms += elapsed_ms;
        } else {
            tracing::debug!(player = %player, "比赛已经切换，资料只入库，不更新界面");
        }
        match lookup.profile {
            Ok(fetched) => {
                tracing::debug!(player = %player, elapsed_ms, "玩家资料已获取");
                if current {
                    self.fetch_stats.ok += 1;
                }
                self.last_error = None;
                // 比赛已经换了也照样保存：这份资料本身是有效的
                if let Err(err) = self
                    .store
                    .save_profile(&profile_update(player, &fetched), at)
                {
                    tracing::warn!(error = %err, "保存玩家资料失败");
                }
                if current {
                    let record = self.store.player(player).ok().flatten();
                    let profile = crate::specs::from_store(
                        record.as_ref().and_then(|r| r.profile_json.as_deref()),
                        &self.store,
                        player,
                    );
                    if let Some(slot) = self.slot_mut(player) {
                        slot.failures = 0;
                        slot.fetch = FetchState::Idle;
                        slot.profile = profile;
                        slot.record = record;
                    }
                }
            }
            Err(err) => {
                let kind = format!("{:?}", err.kind());
                tracing::debug!(player = %player, elapsed_ms, kind = %kind, error = %err, "获取玩家资料失败");
                if current {
                    *self.fetch_stats.failures.entry(kind).or_default() += 1;
                }
                if let ApiError::RateLimited { retry_after } = &err {
                    let pause = retry_after.unwrap_or(self.config.rate_limit_pause);
                    tracing::warn!(pause_ms = millis(pause), "被限流，暂停全部请求");
                    self.paused_until = Some(at.plus_millis(millis(pause)));
                }
                self.last_error = Some(ApiErrorView {
                    kind: err.kind(),
                    message: err.to_string(),
                    at,
                });
                let delays = self.config.retry_delays.clone();
                let paused_until = self.paused_until;
                if current && let Some(slot) = self.slot_mut(player) {
                    let retry_at =
                        (err.is_transient() && slot.failures < delays.len()).then(|| {
                            let planned = at.plus_millis(millis(delays[slot.failures]));
                            paused_until.map_or(planned, |until| planned.max(until))
                        });
                    slot.failures += 1;
                    slot.fetch = FetchState::Failed {
                        kind: err.kind(),
                        retry_at,
                    };
                }
            }
        }
    }

    fn on_report_event(&mut self, event: ReportEvent) {
        let archive = &mut self.archive;
        archive.pending = event.pending;
        archive.seq += 1;
        if let Some((match_id, affected)) = event.archived {
            archive.last_archived = Some(match_id);
            for slot in &mut self.slots {
                if affected.contains(&slot.id)
                    && let Some(profile) = &mut slot.profile
                {
                    crate::specs::enrich(profile, &self.store, slot.id);
                }
            }
            for candidate in self
                .pregame
                .players
                .iter_mut()
                .flat_map(|p| &mut p.candidates)
            {
                if affected.contains(&candidate.id)
                    && let Some(profile) = &mut candidate.profile
                {
                    crate::specs::enrich(profile, &self.store, candidate.id);
                }
            }
        }
        self.dirty = true;
    }

    // ------------------------------------------------------------ 对局变化

    fn handle_updates(&mut self, updates: Vec<Update>, live: bool) {
        if updates.is_empty() {
            return;
        }
        let mut ended = false;
        for update in updates {
            // 发现新比赛之后，同一批里余下的记录挂在新比赛的 span 下
            let span = self.match_span.clone();
            let _entered = span.enter();
            match update {
                Update::MatchDiscovered { match_id } => self.begin_match(match_id, live),
                Update::EarlyPlayers { match_id, players } => {
                    if self.is_current(match_id) {
                        tracing::debug!(count = players.len(), "等待室玩家线索增加");
                        for player in players {
                            self.add_early_player(player, live);
                        }
                    }
                }
                Update::RosterReady { match_id } => {
                    if self.is_current(match_id) {
                        if let Some(m) = self.tracker.current() {
                            let covered = m
                                .roster
                                .iter()
                                .filter(|p| m.early.iter().any(|e| e.id == p.id))
                                .count();
                            tracing::info!(
                                early_players = m.early.len(),
                                roster_players = m.roster.len(),
                                covered,
                                missing = m.roster.len() - covered,
                                "等待室线索覆盖情况"
                            );
                        }
                        self.apply_roster(live);
                        let count =
                            |team| self.slots.iter().filter(|s| s.team == Some(team)).count();
                        tracing::info!(
                            players = self.slots.len(),
                            alpha = count(Team::Alpha),
                            bravo = count(Team::Bravo),
                            "名单已知"
                        );
                    }
                }
                Update::MatchSuperseded { match_id } => {
                    tracing::info!(
                        match_id = %match_id,
                        "上一场比赛没有等到结束信号就被新比赛取代"
                    );
                }
                Update::ClockSet { clock, .. } => {
                    let previous = self.seen.and_then(|s| s.clock);
                    tracing::info!(
                        source = ?clock.source,
                        start = %local_time(clock.start_at),
                        previous_source = previous.map(|c| tracing::field::debug(c.source)),
                        shift_ms = previous.map(|c| clock.start_at.millis_since(c.start_at)),
                        live,
                        "计时基准已设定"
                    );
                }
                Update::MatchEnded { .. } => {
                    ended = true;
                    let cause = self.tracker.current().and_then(|m| m.end_cause);
                    tracing::info!(cause = cause.map(tracing::field::debug), live, "比赛结束");
                    self.log_fetch_summary();
                }
                Update::Disconnected { cause, .. } => {
                    tracing::info!(cause = ?cause, live, "与房间断开");
                }
                Update::Changed { .. } => {}
            }
        }
        let span = self.match_span.clone();
        let _entered = span.enter();
        self.learn_self();
        self.record_current();
        if ended {
            // 结束时刻已经写进长期库，归档任务据此排队等战报
            self.report_notify.notify_one();
        }
        self.note_changes(live);
        self.dirty = true;
    }

    /// 把阶段与连接的变化写进诊断日志，并记下这次看到的状态。
    fn note_changes(&mut self, live: bool) {
        let Some(m) = self.tracker.current() else {
            return;
        };
        let now = Seen {
            match_id: m.match_id,
            phase: m.phase,
            connection: m.connection,
            clock: m.clock,
        };
        let before = self.seen.filter(|s| s.match_id == now.match_id);
        let (event, log_time) = match self.cause {
            Some((kind, at)) => (kind, Some(at)),
            None => ("计时", None),
        };
        if before.is_none_or(|b| b.phase != now.phase) {
            tracing::info!(
                from = before.map(|b| tracing::field::debug(b.phase)),
                to = ?now.phase,
                event = %event,
                log_time = log_time.map(tracing::field::display),
                live,
                "比赛阶段变化"
            );
        }
        if let Some(before) = before
            && before.connection != now.connection
        {
            tracing::info!(
                from = ?before.connection,
                to = ?now.connection,
                event = %event,
                log_time = log_time.map(tracing::field::display),
                live,
                "连接状态变化"
            );
        }
        self.seen = Some(now);
    }

    /// 写一行本场请求的汇总，然后清零。没有请求时不写。
    fn log_fetch_summary(&mut self) {
        let stats = std::mem::take(&mut self.fetch_stats);
        if stats.requests == 0 {
            return;
        }
        tracing::info!(
            requests = stats.requests,
            ok = stats.ok,
            failures = ?stats.failures,
            avg_ms = stats.total_ms / u64::from(stats.requests),
            "本场资料请求汇总"
        );
    }

    fn is_current(&self, match_id: MatchId) -> bool {
        self.tracker
            .current()
            .is_some_and(|m| m.match_id == match_id)
    }

    fn begin_match(&mut self, match_id: MatchId, live: bool) {
        self.log_fetch_summary();
        // 不挂在上一场比赛的 span 下面
        self.match_span = tracing::info_span!(parent: None, "match", match_id = %match_id);
        if let Some(m) = self.tracker.current().filter(|m| m.match_id == match_id) {
            let (rejoined, phase) = (m.rejoined, m.phase);
            self.match_span
                .in_scope(|| tracing::info!(phase = ?phase, rejoined, live, "发现新比赛"));
        }
        self.pregame = Default::default();
        self.pregame_cutoff = self.revision + 1;
        self.generation += 1;
        self.slots.clear();
        self.queue.clear();
        // 这场比赛以前（上一次运行时）已经提醒过或错过的，不再重复
        self.outcomes = self
            .store
            .reminder_outcomes(match_id)
            .unwrap_or_else(|err| {
                tracing::warn!(error = %err, "读取提醒记录失败");
                Vec::new()
            });
        // 游戏重启后重连进来的比赛，日志给不出计时起点；用当初保存的基准
        let needs_clock = self
            .tracker
            .current()
            .is_some_and(|m| m.match_id == match_id && m.clock.is_none());
        if needs_clock {
            match self.store.match_record(match_id) {
                Ok(Some(record)) => {
                    if let Some(clock) = record.clock {
                        self.match_span.in_scope(|| {
                            tracing::info!(source = ?clock.source, "从长期库恢复计时基准");
                        });
                        self.tracker.set_clock(match_id, clock);
                    }
                }
                Ok(None) => {}
                Err(err) => tracing::warn!(error = %err, "读取比赛记录失败"),
            }
        }
    }

    fn add_early_player(&mut self, player: PlayerId, live: bool) {
        if self.slots.iter().any(|s| s.id == player) {
            return;
        }
        let name = self.tracker.current().and_then(|m| {
            m.early
                .iter()
                .find(|p| p.id == player)
                .and_then(|p| p.name.clone())
        });
        let slot = self.load_slot(player, name, None);
        self.slots.push(slot);
        if live {
            self.enqueue_if_stale(player);
        }
    }

    fn apply_roster(&mut self, live: bool) {
        let Some(roster) = self.tracker.current().map(|m| m.roster.clone()) else {
            return;
        };
        let mut previous = std::mem::take(&mut self.slots);
        let mut slots = Vec::with_capacity(roster.len());
        for entry in roster {
            let slot = match previous.iter().position(|s| s.id == entry.id) {
                Some(index) => {
                    let mut slot = previous.swap_remove(index);
                    slot.team = Some(entry.team);
                    slot.log_name = entry.name.or(slot.log_name);
                    slot
                }
                None => self.load_slot(entry.id, entry.name, Some(entry.team)),
            };
            slots.push(slot);
        }
        self.slots = slots;
        // 没进名单的人：等待室里来过又走了
        let slots = &self.slots;
        self.queue.retain(|id| slots.iter().any(|s| s.id == *id));
        if live {
            self.enqueue_stale();
        }
    }

    fn load_slot(&self, id: PlayerId, log_name: Option<String>, team: Option<Team>) -> Slot {
        let record = self.store.player(id).unwrap_or_else(|err| {
            tracing::warn!(error = %err, "读取玩家记录失败");
            None
        });
        let profile = crate::specs::from_store(
            record.as_ref().and_then(|r| r.profile_json.as_deref()),
            &self.store,
            id,
        );
        let current = self.tracker.current().map(|m| m.match_id);
        let encounters = self.store.encounters(id, current).unwrap_or_else(|err| {
            tracing::warn!(error = %err, "统计相遇次数失败");
            EncounterSummary::default()
        });
        let note = self.store.note(id).unwrap_or_else(|err| {
            tracing::warn!(error = %err, "读取备注失败");
            None
        });
        Slot {
            id,
            log_name,
            team,
            record,
            profile,
            encounters,
            note: note.map(|n| n.body),
            fetch: FetchState::Idle,
            failures: 0,
        }
    }

    /// 记住本机玩家的 ID，下次即使名字对不上也能认出来。
    fn learn_self(&mut self) {
        let Some(found) = self.tracker.current().and_then(|m| m.self_id) else {
            return;
        };
        if self.self_hint == Some(found) {
            return;
        }
        self.self_hint = Some(found);
        tracing::info!(self_player = %found, "识别出本机玩家");
        self.tracker.set_self_hint(Some(found));
        if let Err(err) = self.store.set_setting(SELF_ID_KEY, &found) {
            tracing::warn!(error = %err, "保存本机玩家 ID 失败");
        }
    }

    fn record_current(&mut self) {
        if let Some(state) = self.tracker.current()
            && let Err(err) = self.store.record_match(state)
        {
            tracing::warn!(error = %err, match_id = %state.match_id, "保存比赛记录失败");
        }
    }

    fn reset_match_state(&mut self) {
        self.log_fetch_summary();
        self.match_span = tracing::Span::none();
        self.seen = None;
        let mut tracker = Tracker::new(self.config.tracker.clone());
        tracker.set_self_hint(self.self_hint);
        self.tracker = tracker;
        self.pregame = Default::default();
        self.pregame_cutoff = self.revision + 1;
        self.generation += 1;
        self.slots.clear();
        self.queue.clear();
        self.outcomes.clear();
    }

    fn start_log_thread(&mut self, dir: Option<PathBuf>) {
        if let Some(mut old) = self.log_thread.take() {
            // 不等旧线程结束：它在下一次检查时自己退出，发出的消息没人接收
            old.stop();
        }
        let (tx, rx) = mpsc::channel(LOG_CHANNEL_CAPACITY);
        self.log_rx = rx;
        self.log = LogStatus {
            searching: dir.is_none(),
            dir: dir.as_ref().map(|p| p.to_string_lossy().into_owned()),
            ..LogStatus::default()
        };
        self.backfill_events = 0;
        self.log_started = Instant::now();
        let timing = LogTiming {
            poll: self.config.log_poll_interval,
            discovery: self.config.log_discovery_interval,
        };
        match LogThread::spawn(dir, timing, tx) {
            Ok(thread) => self.log_thread = Some(thread),
            Err(err) => {
                tracing::error!(error = %err, "无法启动日志线程");
                self.log.error = Some(format!("无法启动日志线程：{err}"));
            }
        }
    }

    // ------------------------------------------------------------ 请求调度

    fn slot_mut(&mut self, id: PlayerId) -> Option<&mut Slot> {
        self.slots.iter_mut().find(|s| s.id == id)
    }

    fn enqueue(&mut self, id: PlayerId) {
        if let Some(slot) = self.slot_mut(id)
            && matches!(slot.fetch, FetchState::Idle | FetchState::Failed { .. })
        {
            slot.fetch = FetchState::Queued;
            self.queue.push_back(id);
            self.dirty = true;
        }
    }

    fn enqueue_if_stale(&mut self, id: PlayerId) {
        let Some(since) = self.tracker.current().map(|m| m.times.discovered_at) else {
            return;
        };
        if self
            .slots
            .iter()
            .any(|slot| slot.id == id && slot.needs_fetch(since))
        {
            self.enqueue(id);
        }
    }

    fn enqueue_stale(&mut self) {
        let ids: Vec<PlayerId> = self.slots.iter().map(|s| s.id).collect();
        for id in ids {
            self.enqueue_if_stale(id);
        }
    }

    fn dispatch(&mut self) {
        let now = self.clock.now();
        if self.paused_until.is_some_and(|until| now < until) {
            return;
        }
        while self.in_flight < self.config.fetch_concurrency.max(1) {
            let Some(id) = self.queue.pop_front() else {
                break;
            };
            let Some(slot) = self.slot_mut(id) else {
                continue;
            };
            if slot.fetch != FetchState::Queued {
                continue;
            }
            slot.fetch = FetchState::Fetching;
            self.in_flight += 1;
            self.dirty = true;
            tracing::debug!(player = %id, queued = self.queue.len(), "请求玩家资料");

            let client = self.client.clone();
            let tx = self.fetch_tx.clone();
            let clock = Arc::clone(&self.clock);
            let generation = self.generation;
            tokio::spawn(async move {
                let started = Instant::now();
                let lookup = ProfileLookup::fetch(&client, id, clock.now()).await;
                let _ = tx.send(FetchResult {
                    player: id,
                    generation,
                    at: clock.now(),
                    elapsed_ms: started.elapsed().as_millis() as u64,
                    lookup,
                });
            });
        }
    }

    // ------------------------------------------------------------ 输出

    fn publish(&mut self) {
        self.revision += 1;
        tracing::trace!(revision = self.revision, "发布快照");
        let snapshot = self.build_snapshot();
        self.snapshot_tx.send_replace(Arc::new(snapshot));
        self.dirty = false;
    }

    fn build_snapshot(&self) -> Snapshot {
        let current = self.tracker.current().map(|m| {
            let players: Vec<PlayerView> = self
                .slots
                .iter()
                .map(|slot| {
                    let fetched_at = slot.fetched_at();
                    PlayerView {
                        id: slot.id,
                        trace_count: slot.record.as_ref().and_then(|r| r.trace_count),
                        name: slot
                            .profile
                            .as_ref()
                            .and_then(|p| p.name.clone())
                            .or_else(|| slot.log_name.clone()),
                        log_name: slot.log_name.clone(),
                        team: slot.team,
                        is_self: m.self_id == Some(slot.id),
                        profile: slot.profile.clone(),
                        profile_fetched_at: fetched_at,
                        profile_current: fetched_at.is_some_and(|at| at >= m.times.discovered_at),
                        fetch: slot.fetch,
                        encounters: slot.encounters,
                        note: slot.note.clone(),
                    }
                })
                .collect();
            let teams = [Team::Alpha, Team::Bravo]
                .into_iter()
                .map(|team| TeamSummary::of(team, &players))
                .collect();
            MatchView {
                mode: crate::match_mode::live_mode(
                    m.scenario.as_deref(),
                    m.has_ai,
                    m.roster.iter().any(|p| p.team == Team::Spectators),
                ),
                pregame: if m.roster.is_empty()
                    && !m.has_ai
                    && !crate::match_mode::scenario_is_pve(m.scenario.as_deref())
                    && !m.rejoined
                    && m.connection == Connection::Connected
                    && matches!(
                        m.phase,
                        Phase::WaitingRoom | Phase::Loading | Phase::Deployment
                    )
                    && self
                        .ui
                        .get("ui.ocr")
                        .and_then(|v| v.get("enabled"))
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(true)
                {
                    self.pregame.clone()
                } else {
                    Default::default()
                },
                match_id: m.match_id,
                phase: m.phase,
                connection: m.connection,
                rejoined: m.rejoined,
                map: m.map.clone(),
                scenario: m.scenario.clone(),
                deck: m.deck.clone(),
                clock: m.clock,
                discovered_at: m.times.discovered_at,
                ended_at: m.times.ended_at,
                self_team: m.my_team(),
                roster_known: !m.roster.is_empty(),
                players,
                teams,
            }
        });
        Snapshot {
            archive: self.archive,
            revision: self.revision,
            log: self.log.clone(),
            persona: self.tracker.persona().map(str::to_owned),
            game_version: self.tracker.game_version().map(str::to_owned),
            self_id: self
                .tracker
                .current()
                .and_then(|m| m.self_id)
                .or(self.self_hint),
            api: ApiStatus {
                in_flight: self.in_flight as u32,
                queued: self.queue.len() as u32,
                paused_until: self.paused_until,
                last_error: self.last_error.clone(),
            },
            reminders: self.reminder_views(),
            alert: self.alert.clone(),
            ui: self.ui.clone(),
            current,
        }
    }

    fn reminder_views(&self) -> Vec<ReminderView> {
        let clock = self.tracker.current().and_then(|m| m.clock);
        self.rules
            .iter()
            .map(|rule| {
                let outcome = self.outcomes.iter().find(|(id, _, _)| *id == rule.id);
                ReminderView {
                    id: rule.id,
                    label: rule.label.clone(),
                    at_secs: rule.at_secs,
                    enabled: rule.enabled,
                    fire_at: clock.map(|c| reminder::fire_at(rule, c)),
                    outcome: outcome.map(|(_, o, _)| *o),
                    outcome_at: outcome.map(|(_, _, at)| *at),
                }
            })
            .collect()
    }
}

fn profile_update(id: PlayerId, fetched: &Fetched<PlayerInfoResponse>) -> ProfileUpdate {
    let info = fetched.data.info.as_ref();
    ProfileUpdate {
        id,
        name: info
            .and_then(|i| i.name.clone())
            .filter(|n| !n.trim().is_empty()),
        steam_id: info.and_then(|i| i.steam_id.clone()),
        level: info.and_then(|i| i.level),
        rating: info.and_then(|i| i.rating),
        rank: info.and_then(|i| i.rank),
        rating_games: info.and_then(|i| i.rating_games),
        raw_json: fetched.raw.clone(),
    }
}

/// 会话循环一次处理的输入。
enum Input {
    Pregame(crate::pregame::Message),
    PregameTrace(crate::pregame::Ticket, PlayerId, Timestamp, Option<u32>),
    Command(Command),
    Log(LogMsg),
    // 资料结果比其他输入大得多，装箱避免每次循环都搬动整块内存
    Fetched(Box<FetchResult>),
    Report(ReportEvent),
    Tick,
}

/// 事件类型的名字。诊断日志只记类型，不记内容：内容里有玩家名字和卡组名。
fn event_kind(event: &LogEvent) -> &'static str {
    match event {
        LogEvent::GameVersion { .. } => "GameVersion",
        LogEvent::Persona { .. } => "Persona",
        LogEvent::DeckSelected { .. } => "DeckSelected",
        LogEvent::ReconnectStarted => "ReconnectStarted",
        LogEvent::RoomEntered { .. } => "RoomEntered",
        LogEvent::RoomExited { .. } => "RoomExited",
        LogEvent::RoomClient { .. } => "RoomClient",
        LogEvent::LoadingStarted { .. } => "LoadingStarted",
        LogEvent::FightInfo { .. } => "FightInfo",
        LogEvent::PlayerList { .. } => "PlayerList",
        LogEvent::AiDetected => "AiDetected",
        LogEvent::ScenarioLoaded => "ScenarioLoaded",
        LogEvent::MatchEnded { .. } => "MatchEnded",
    }
}

/// 把时刻写成本地时间，与诊断日志的时间戳一致。
fn local_time(at: Timestamp) -> String {
    use chrono::TimeZone;
    match chrono::Local.timestamp_millis_opt(at.0) {
        chrono::LocalResult::Single(t) | chrono::LocalResult::Ambiguous(t, _) => {
            t.format("%H:%M:%S%.3f").to_string()
        }
        chrono::LocalResult::None => at.0.to_string(),
    }
}

fn millis(duration: Duration) -> i64 {
    i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
}

#[cfg(feature = "ocr")]
struct OcrGuard(Option<tokio::task::JoinHandle<()>>);
#[cfg(feature = "ocr")]
impl Drop for OcrGuard {
    fn drop(&mut self) {
        if let Some(task) = &self.0 {
            task.abort();
        }
    }
}

#[cfg(all(test, feature = "ocr"))]
#[path = "pregame/session_tests.rs"]
mod pregame_tests;
