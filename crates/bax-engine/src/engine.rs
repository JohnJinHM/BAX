//! 引擎的对外句柄。

use std::path::PathBuf;
use std::sync::Arc;

use bax_batrace::{ApiError, BatraceClient};
use bax_core::PlayerId;
use bax_store::{Store, StoreError};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinHandle;

use crate::clock::Clock;
use crate::config::EngineConfig;
use crate::session::{Command, LOG_DIR_KEY, Parts, SELF_ID_KEY, Session};
use crate::snapshot::Snapshot;

/// 启动失败的原因。
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("无法打开长期库：{0}")]
    Store(#[from] StoreError),
    #[error("接口客户端配置错误：{0}")]
    Api(#[from] ApiError),
}

/// 引擎已经停止，命令没有送达。
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("引擎已经停止")]
pub struct EngineStopped;

/// 正在运行的引擎。丢弃句柄会让引擎停下；需要等它收尾时用 [`Engine::shutdown`]。
#[derive(Debug)]
pub struct Engine {
    commands: mpsc::Sender<Command>,
    snapshots: watch::Receiver<Arc<Snapshot>>,
    task: JoinHandle<()>,
}

impl Engine {
    /// 打开长期库并启动引擎。必须在 tokio 运行时里调用。
    pub fn start(config: EngineConfig, clock: Arc<dyn Clock>) -> Result<Engine, EngineError> {
        let store = Store::open(&config.store_path)?;
        let client = BatraceClient::new(config.api.clone())?;

        let log_dir = config.log_dir.clone().or_else(|| {
            store
                .setting::<String>(LOG_DIR_KEY)
                .ok()
                .flatten()
                .map(PathBuf::from)
        });
        let self_hint = remembered_self(&store);

        let (snapshot_tx, snapshots) = watch::channel(Arc::new(Snapshot::initial()));
        let (commands, command_rx) = mpsc::channel(16);
        let session = Session::new(Parts {
            config,
            clock,
            store,
            client,
            log_dir,
            self_hint,
            snapshot_tx,
        });
        let task = tokio::spawn(session.run(command_rx));
        Ok(Engine {
            commands,
            snapshots,
            task,
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
    pub async fn refresh_players(&self) -> Result<(), EngineStopped> {
        self.commands
            .send(Command::RefreshPlayers)
            .await
            .map_err(|_| EngineStopped)
    }

    /// 改用另一个日志目录并记住这个选择；`None` 表示改回自动定位。
    /// 返回时新的日志线程已经启动，当前比赛的状态已经清空。
    pub async fn set_log_dir(&self, dir: Option<PathBuf>) -> Result<(), EngineStopped> {
        let (done, wait) = oneshot::channel();
        self.commands
            .send(Command::SetLogDir { dir, done })
            .await
            .map_err(|_| EngineStopped)?;
        wait.await.map_err(|_| EngineStopped)
    }

    /// 停下引擎，等它收尾。
    pub async fn shutdown(self) {
        let Engine { commands, task, .. } = self;
        drop(commands);
        let _ = task.await;
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
