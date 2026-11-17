//! 玩家档案：主窗口按需读取的完整资料，不放进快照（ADR-0004）。

use bax_core::{PlayerId, Timestamp};
use bax_store::{EncounterRow, EncounterSummary, Note, RatingPoint};
use serde::Serialize;

use crate::profile::ProfileSummary;

/// 一名玩家的完整档案。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct PlayerDetail {
    pub id: PlayerId,
    /// 接口资料里的名字。
    pub name: Option<String>,
    /// 最近一次在日志名单里见到的名字。
    pub log_name: Option<String>,
    pub steam_id: Option<String>,
    pub profile: Option<ProfileSummary>,
    /// 最近一次取得资料的时刻；从未取到过时为 `None`。
    pub fetched_at: Option<Timestamp>,
    pub first_seen_at: Option<Timestamp>,
    pub last_seen_at: Option<Timestamp>,
    pub note: Option<Note>,
    /// 同场次数汇总，含当前这场。
    pub encounters: EncounterSummary,
    /// ELO、等级等的变化历史，从旧到新。
    pub history: Vec<RatingPoint>,
    /// 同场过的比赛，新的在前，最多 200 场。
    pub matches: Vec<EncounterRow>,
    /// 这是本机玩家自己。
    pub is_self: bool,
}
