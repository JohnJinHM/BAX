//! 发布给界面的状态快照。
//!
//! 快照描述当前这一刻界面需要的全部状态，体积很小（一场比赛十几名玩家）。
//! 每次变化发布一份完整的快照，带递增的修订号，窗口晚打开或重新加载时拿最新的一份就能对齐
//! （见 ADR-0004）。完整战报这类大块数据不放进快照，由界面按需另外读取。

use bax_batrace::ErrorKind;
use bax_core::{BattleClock, Connection, MatchId, Phase, PlayerId, Team, Timestamp};
use bax_store::EncounterSummary;
use serde::Serialize;

use crate::profile::ProfileSummary;

/// 引擎状态的一份快照。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    /// 每发布一次加一。
    pub revision: u64,
    pub log: LogStatus,
    /// 本机的 Steam 昵称。
    pub persona: Option<String>,
    pub game_version: Option<String>,
    /// 本机玩家的 ID：当前比赛里识别出的，或者以前记住的。
    pub self_id: Option<PlayerId>,
    pub api: ApiStatus,
    /// 当前比赛；还没见过任何比赛时为 `None`。比赛结束后仍然保留，直到下一场开始。
    pub current: Option<MatchView>,
}

impl Snapshot {
    pub(crate) fn initial() -> Snapshot {
        Snapshot {
            revision: 0,
            log: LogStatus::default(),
            persona: None,
            game_version: None,
            self_id: None,
            api: ApiStatus::default(),
            current: None,
        }
    }
}

/// 日志读取的状态。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogStatus {
    /// 正在自动寻找日志目录。
    pub searching: bool,
    pub dir: Option<String>,
    /// 正在读的日志文件。
    pub file: Option<String>,
    /// 启动时对已有日志的补读已经完成，之后的事件都是实时的。
    pub ready: bool,
    /// 最近一次读取失败的原因；恢复后清除。
    pub error: Option<String>,
}

/// 接口请求的整体状态。
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiStatus {
    pub in_flight: u32,
    pub queued: u32,
    /// 被限流后暂停到这个时刻。
    pub paused_until: Option<Timestamp>,
    /// 最近一次失败；之后有请求成功就清除。
    pub last_error: Option<ApiErrorView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiErrorView {
    pub kind: ErrorKind,
    pub message: String,
    pub at: Timestamp,
}

/// 当前比赛。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchView {
    pub match_id: MatchId,
    pub phase: Phase,
    pub connection: Connection,
    /// 中途重连进入。
    pub rejoined: bool,
    pub map: Option<String>,
    pub scenario: Option<String>,
    pub deck: Option<String>,
    /// 正式开局的时刻。界面据此自己计时，不需要引擎每秒推送。
    pub clock: Option<BattleClock>,
    pub discovered_at: Timestamp,
    pub ended_at: Option<Timestamp>,
    pub self_team: Option<Team>,
    /// 已经有完整名单。为 `false` 时 `players` 是等待室里先到的玩家，队伍未知。
    pub roster_known: bool,
    /// 按日志名单的顺序。
    pub players: Vec<PlayerView>,
    /// Alpha 与 Bravo 两队的汇总。
    pub teams: Vec<TeamSummary>,
}

/// 比赛里的一名玩家。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerView {
    pub id: PlayerId,
    /// 显示用的名字：接口资料里的名字优先，其次是这场比赛日志里的名字。
    pub name: Option<String>,
    /// 这场比赛日志里的名字；空名字为 `None`。
    pub log_name: Option<String>,
    pub team: Option<Team>,
    pub is_self: bool,
    /// 本地已有的资料摘要，可能是以前取得的。
    pub profile: Option<ProfileSummary>,
    pub profile_fetched_at: Option<Timestamp>,
    /// 资料是在这场比赛开始之后取得的。
    pub profile_current: bool,
    pub fetch: FetchState,
    /// 以前同场的次数，不含这一场。
    pub encounters: EncounterSummary,
}

/// 一名玩家的资料请求进行到哪一步。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "state"
)]
pub enum FetchState {
    /// 没有请求在进行，也不需要请求。
    Idle,
    Queued,
    Fetching,
    /// 失败。`retry_at` 是计划重试的时刻，不再重试时为 `None`。
    Failed {
        kind: ErrorKind,
        retry_at: Option<Timestamp>,
    },
}

/// 一支队伍的汇总。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamSummary {
    pub team: Team,
    pub players: u32,
    /// 有 ELO 的人数。
    pub rated: u32,
    pub average_rating: Option<f64>,
}

impl TeamSummary {
    pub(crate) fn of(team: Team, players: &[PlayerView]) -> TeamSummary {
        let members = players.iter().filter(|p| p.team == Some(team));
        let mut count = 0u32;
        let mut ratings = Vec::new();
        for p in members {
            count += 1;
            if let Some(rating) = p.profile.as_ref().and_then(|profile| profile.rating) {
                ratings.push(rating);
            }
        }
        TeamSummary {
            team,
            players: count,
            rated: ratings.len() as u32,
            average_rating: (!ratings.is_empty())
                .then(|| ratings.iter().sum::<f64>() / ratings.len() as f64),
        }
    }
}
