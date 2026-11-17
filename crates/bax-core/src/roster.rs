//! 队伍与名单。

use serde::{Deserialize, Serialize};

use crate::ids::PlayerId;

/// 队伍。日志里写作 `Alpha`、`Bravo`、`Spectators`；
/// BATrace 用 `TeamId`，缺省（0）是 Alpha，1 是 Bravo，100 是观战。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum Team {
    Alpha,
    Bravo,
    Spectators,
}

impl Team {
    /// 解析日志里的队伍名。
    pub fn from_log_name(name: &str) -> Option<Team> {
        match name {
            "Alpha" => Some(Team::Alpha),
            "Bravo" => Some(Team::Bravo),
            "Spectators" => Some(Team::Spectators),
            _ => None,
        }
    }

    /// 解析 BATrace 的 `TeamId`。
    pub fn from_api_id(id: i64) -> Option<Team> {
        match id {
            0 => Some(Team::Alpha),
            1 => Some(Team::Bravo),
            100 => Some(Team::Spectators),
            _ => None,
        }
    }

    /// 对面的队伍；观战没有对手。
    pub fn opponent(self) -> Option<Team> {
        match self {
            Team::Alpha => Some(Team::Bravo),
            Team::Bravo => Some(Team::Alpha),
            Team::Spectators => None,
        }
    }

    pub fn is_playing(self) -> bool {
        !matches!(self, Team::Spectators)
    }
}

/// 名单里的一行。
///
/// 名字可能为空：真实日志里有玩家的名字是空串。空名字记为 `None`，
/// 之后用接口资料补全。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RosterEntry {
    pub id: PlayerId,
    pub name: Option<String>,
    pub team: Team,
}
