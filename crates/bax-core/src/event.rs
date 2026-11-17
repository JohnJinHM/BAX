//! 从游戏日志里识别出的事件。
//!
//! 这里只定义事件的形状。把日志文本解析成事件由 `bax-gamelog` 负责，
//! 解释事件对一场比赛意味着什么由 [`crate::lifecycle`] 负责。

use serde::{Deserialize, Serialize};

use crate::ids::{MatchId, PlayerId};
use crate::roster::RosterEntry;

/// 游戏的两种房间。匹配成功后先进等待室，加载完成后转入对局房间。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RoomKind {
    WaitingRoom,
    GameRoom,
}

/// 房间状态，来自日志 `Network lifecycle` 行的 `Status` 字段。
///
/// 进入房间时的状态说明了这是一场新比赛还是中途重连：
/// `WaitClients` 是正常开局，`InProcess` 表示比赛已经在进行。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RoomStatus {
    WaitClients,
    DeploymentPhase,
    InProcess,
    End,
    /// 日志里出现了这里没有列出的取值。
    Unknown,
}

impl RoomStatus {
    pub fn from_log_name(name: &str) -> RoomStatus {
        match name {
            "WaitClients" => RoomStatus::WaitClients,
            "DeploymentPhase" => RoomStatus::DeploymentPhase,
            "InProcess" => RoomStatus::InProcess,
            "End" => RoomStatus::End,
            _ => RoomStatus::Unknown,
        }
    }
}

/// 离开房间的原因，来自 `Network lifecycle` 行的 `Cause` 字段。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExitCause {
    /// 比赛正常结束。
    MatchEnded,
    /// 服务器关闭了这个房间实例。从等待室转入对局房间时是这个原因，
    /// 比赛结束后被服务器请出对局房间时也可能是这个原因。
    ServerExitInstance,
    /// 连接超时。
    ConnectionLost,
    /// 客户端一侧的网络服务被销毁，通常是玩家主动退出或关闭游戏。
    ServiceDisposed,
    Manual,
    Unknown,
}

impl ExitCause {
    pub fn from_log_name(name: &str) -> ExitCause {
        match name {
            "MatchEnded" => ExitCause::MatchEnded,
            "ServerExitInstance" => ExitCause::ServerExitInstance,
            "ConnectionLost" => ExitCause::ConnectionLost,
            "ServiceDisposed" => ExitCause::ServiceDisposed,
            "Manual" => ExitCause::Manual,
            _ => ExitCause::Unknown,
        }
    }
}

/// 一条与对局有关的日志事件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum LogEvent {
    /// 游戏启动时报告的版本号，例如 `1.2.2.1`。
    GameVersion { version: String },
    /// 本机的 Steam 昵称（`GetPersonaName`）。
    Persona { name: String },
    /// 选定的卡组名；`None` 表示卡组被清空。卡组名可由玩家自定义，只作辅助信息。
    DeckSelected { name: Option<String> },
    /// 游戏开始重连到一场进行中的比赛（`Start reconnect to battle`）。
    ReconnectStarted,
    /// 进入房间。旧版日志没有 `Network lifecycle` 行，此时 `status` 为 `None`。
    RoomEntered {
        room: RoomKind,
        match_id: MatchId,
        status: Option<RoomStatus>,
    },
    /// 离开房间。旧版日志只有 `WaitingRoom exited` 这样的标记，其余字段为 `None`。
    RoomExited {
        room: RoomKind,
        match_id: Option<MatchId>,
        cause: Option<ExitCause>,
        status: Option<RoomStatus>,
    },
    /// 房间里出现了一名客户端（`Room: (WaitingRoom|…), Client: (名字|ID)`）。
    /// 在等待室里它早于完整名单，可以用来提前查询玩家资料。
    RoomClient {
        room: RoomKind,
        player: PlayerId,
        name: Option<String>,
    },
    /// 开始加载战场。
    LoadingStarted {
        map: String,
        scenario: Option<String>,
    },
    /// 本机在这场比赛里的身份（`FID:比赛ID 版本 - 名字 (UTC 时间)`）。
    FightInfo {
        match_id: MatchId,
        player_name: Option<String>,
    },
    /// 完整名单，包含队伍。
    PlayerList { players: Vec<RosterEntry> },
    /// 场景加载完成（`Scenario controller loaded`）。正常开局时，准备阶段从这里开始。
    ScenarioLoaded,
    /// 比赛结束（`Network traffic history | Cause=MatchEnded`）。
    MatchEnded { match_id: MatchId },
}
