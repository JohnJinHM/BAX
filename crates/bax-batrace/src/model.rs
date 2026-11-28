//! 接口响应的结构。
//!
//! 字段名与接口保持一致，只做反序列化，不加业务解释。
//! 所有字段都可以缺省：接口省略默认值，部分玩家的 `statInfo` 整体为空，
//! 站点升级后还可能增减字段。缺一个字段不应该让整条响应作废。

use std::collections::BTreeMap;

use bax_core::{MatchId, PlayerId, Team};
use serde::{Deserialize, Deserializer};

/// 把 JSON 里的 `null` 当作缺省值。
fn null_as_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

// ---------------------------------------------------------------- 玩家资料

/// `/api/players/info?stbid=…`
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct PlayerInfoResponse {
    pub info: Option<PlayerInfo>,
    #[serde(rename = "statInfo")]
    pub stat_info: Option<PlayerStats>,
    /// 近期比赛的 ID，新的在前。
    #[serde(deserialize_with = "null_as_default")]
    pub last_fights_data: Vec<String>,
}

impl PlayerInfoResponse {
    /// 近期比赛的 ID，丢掉无法解析的项。
    pub fn recent_match_ids(&self) -> impl Iterator<Item = MatchId> + '_ {
        self.last_fights_data
            .iter()
            .filter_map(|s| s.trim().parse().ok())
    }
}

/// 玩家的基本资料。
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PlayerInfo {
    pub id: Option<u64>,
    pub name: Option<String>,
    pub steam_id: Option<String>,
    pub level: Option<i64>,
    /// ELO。
    pub rating: Option<f64>,
    pub rank: Option<i64>,
    pub rating_games: Option<i64>,
}

/// 玩家的累计统计。后缀 `Rt` 是排位，`Ct` 是自定义，`Sk` 是遭遇战。
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PlayerStats {
    pub player_id: Option<u64>,
    pub login_count: Option<i64>,
    pub exp_count: Option<i64>,
    pub level_count: Option<i64>,

    pub fights_count_rt: Option<i64>,
    pub win_count_rt: Option<i64>,
    pub loss_count_rt: Option<i64>,
    pub leave_count_rt: Option<i64>,
    pub units_killed_count_rt: Option<i64>,
    pub units_lost_count_rt: Option<i64>,
    pub units_killed_cost_rt: Option<i64>,
    pub units_lost_cost_rt: Option<i64>,
    pub match_duration_time_sec_rt: Option<i64>,

    pub fights_count_ct: Option<i64>,
    pub win_count_ct: Option<i64>,
    pub loss_count_ct: Option<i64>,
    pub units_killed_count_ct: Option<i64>,
    pub units_lost_count_ct: Option<i64>,
    pub units_killed_cost_ct: Option<i64>,
    pub units_lost_cost_ct: Option<i64>,
    pub match_duration_time_sec_ct: Option<i64>,

    pub fights_count_sk: Option<i64>,
    pub win_count_sk: Option<i64>,
    pub units_killed_count_sk: Option<i64>,
    pub units_lost_count_sk: Option<i64>,
    pub units_killed_cost_sk: Option<i64>,
    pub units_lost_cost_sk: Option<i64>,
    pub match_duration_time_sec_sk: Option<i64>,

    pub supply_pts_consumed: Option<i64>,
    pub supply_pts_consumed_by_allies: Option<i64>,
    pub supply_pts_consumed_from_allies: Option<i64>,
    pub supply_captured_count: Option<i64>,
    pub supply_captured_by_enemy_count: Option<i64>,
    pub zone_captured_count: Option<i64>,
    pub units_refunded_count: Option<i64>,
    pub units_self_destruction_count: Option<i64>,
    pub units_lost_friendly_fire_count: Option<i64>,
    pub units_killed_friendly_fire_count: Option<i64>,

    /// 地图 ID → 次数。
    #[serde(deserialize_with = "null_as_default")]
    pub map_play_counts: BTreeMap<String, i64>,
    /// 专精 ID → 等级。
    #[serde(deserialize_with = "null_as_default")]
    pub spec_levels: BTreeMap<String, i64>,
    /// 专精 ID → 经验。
    #[serde(deserialize_with = "null_as_default")]
    pub spec_exp: BTreeMap<String, i64>,
    /// 国家 ID → 经验。
    #[serde(deserialize_with = "null_as_default")]
    pub nation_exp: BTreeMap<String, i64>,
    /// 专精组合（形如 `4_5`）→ 经验。
    #[serde(deserialize_with = "null_as_default")]
    pub combined_specs_exp: BTreeMap<String, i64>,
    /// 专精组合 → 等级。
    #[serde(deserialize_with = "null_as_default")]
    pub combined_specs_level: BTreeMap<String, i64>,

    pub updated_at: Option<String>,
}

// ---------------------------------------------------------------- 战报

/// `/api/match?matchid=…`
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct MatchReportResponse {
    #[serde(rename = "matchInfo")]
    pub match_info: Option<MatchInfo>,
}

/// 一场比赛的完整战报。
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct MatchInfo {
    pub map_id: Option<i64>,
    /// 正式开局的时刻，Unix 秒。
    pub start_time: Option<i64>,
    /// 比赛结束的时刻，Unix 秒。
    pub end_time: Option<i64>,
    pub total_play_time_in_sec: Option<i64>,
    pub total_objective_zones_count: Option<i64>,
    /// 胜方的队伍编号。键不出现表示 0。
    pub winner_team: Option<i64>,
    pub victory_level: Option<i64>,
    pub end_match_reason: Option<i64>,
    #[serde(rename = "Type")]
    pub match_type: Option<i64>,
    pub team_match_points: Option<TeamMatchPoints>,
    /// 玩家 ID → 该玩家的战报。
    #[serde(deserialize_with = "null_as_default")]
    pub data: BTreeMap<String, MatchPlayer>,
}

impl MatchInfo {
    /// 胜方。接口省略默认值，所以没有 `WinnerTeam` 键时胜方是 Alpha；
    /// 其他编号（见过 101）含义不明，返回 `None`。
    pub fn winner(&self) -> Option<Team> {
        winner_of(self.winner_team)
    }

    /// 按玩家遍历，不含 AI 玩家。
    pub fn players(&self) -> impl Iterator<Item = (PlayerId, &MatchPlayer)> {
        self.data
            .iter()
            .filter_map(|(key, player)| Some((human_id(player.id, key)?, player)))
    }

    /// 战报是否还没有内容。刚结束的比赛可能先返回一个空壳。
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct TeamMatchPoints {
    pub alpha: Option<i64>,
    pub bravo: Option<i64>,
}

/// 战报里一名玩家的数据。
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct MatchPlayer {
    /// AI 玩家为负数。
    pub id: Option<i64>,
    pub name: Option<String>,
    /// 队伍编号。键不出现表示 0。
    pub team_id: Option<i64>,
    pub old_rating: Option<f64>,
    pub new_rating: Option<f64>,
    pub total_spawned_unit_score: Option<i64>,
    pub total_refunded_unit_score: Option<i64>,
    pub destruction: Option<i64>,
    pub losses: Option<i64>,
    pub destruction_score: Option<i64>,
    pub losses_score: Option<i64>,
    pub damage_dealt: Option<f64>,
    pub damage_received: Option<f64>,
    pub damage_friendly_fire_dealt: Option<f64>,
    pub damage_friendly_fire_received: Option<f64>,
    pub destruction_friendly_fire: Option<i64>,
    pub destruction_friendly_fire_cost: Option<i64>,
    pub losses_by_friendly_fire: Option<i64>,
    pub losses_by_friendly_fire_score: Option<i64>,
    pub objectives_captured: Option<i64>,
    pub supply_points_consumed: Option<i64>,
    pub supply_points_consumed_by_allies: Option<i64>,
    pub supply_points_consumed_from_allies: Option<i64>,
    pub supply_captured: Option<i64>,
    pub supply_captured_by_enemy: Option<i64>,
    pub supply_destroyed: Option<i64>,
    pub supply_airdropped: Option<i64>,
    pub deserter: Option<bool>,
    #[serde(deserialize_with = "null_as_default")]
    pub medals: Vec<i64>,
    pub raw_exp: Option<i64>,
    pub raw_exp_with_multipliers: Option<i64>,
    pub total_exp: Option<i64>,
    #[serde(rename = "DLRatio")]
    pub dl_ratio: Option<f64>,
    /// 单位实例 ID → 实例。
    #[serde(deserialize_with = "null_as_default")]
    pub unit_data: BTreeMap<String, UnitInstance>,
    /// 转交的单位：实例 ID → 实例的基本信息。
    #[serde(deserialize_with = "null_as_default")]
    pub transferred_units: BTreeMap<String, TransferredUnit>,
}

impl MatchPlayer {
    /// 所在队伍。没有 `TeamId` 键时是 Alpha。
    pub fn team(&self) -> Option<Team> {
        Team::from_api_id(self.team_id.unwrap_or(0))
    }
}

/// 战报里的一个单位实例。
///
/// 实例在 `UnitData` 里的键在一场比赛内唯一。单位被转交后出现在接收者的 `UnitData` 里，
/// 转出者的 `TransferredUnits` 里留有同一个键。
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct UnitInstance {
    /// 单位类型 ID，对应 `/api/units` 里的 `id`。
    pub id: Option<i64>,
    #[serde(deserialize_with = "null_as_default")]
    pub option_ids: Vec<i64>,
    /// 出生时刻，Unix 秒（不是相对开局的秒数）。相对开局的时间是它减去 `MatchInfo::start_time`；
    /// 准备阶段部署的单位在开局后两三秒出现。
    pub spawn_time: Option<i64>,
    /// 阵亡时刻，Unix 秒。键不出现表示活到了比赛结束。
    pub death_time: Option<i64>,
    pub killed_count: Option<i64>,
    pub building_destroyed_count: Option<i64>,
    pub total_damage_dealt: Option<f64>,
    pub total_damage_received: Option<f64>,
    pub total_damage_friendly_fire_dealt: Option<f64>,
    pub total_damage_received_by_friendly_fire: Option<f64>,
    pub total_self_damage_dealt: Option<f64>,
    pub total_supply_points_consumed: Option<i64>,
    pub was_refunded: Option<bool>,
    pub was_airdropped: Option<bool>,
    pub self_destruction: Option<bool>,
}

/// 转出的单位：出现在转出者（原本出兵的人）名下，实例本身在接收者的 `UnitData` 里，键相同。
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct TransferredUnit {
    pub id: Option<i64>,
    #[serde(deserialize_with = "null_as_default")]
    pub option_ids: Vec<i64>,
    /// 出生时刻，Unix 秒，与接收者那边的实例相同。
    pub spawn_time: Option<i64>,
}

// ---------------------------------------------------------------- 比赛列表

/// `/api/players/matches?stbid=…&offset=…&limit=…`
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PlayerMatchesResponse {
    #[serde(deserialize_with = "null_as_default")]
    pub matches: Vec<MatchListEntry>,
    pub total_count: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MatchListEntry {
    pub match_id: Option<String>,
    pub data: Option<MatchSummary>,
}

impl MatchListEntry {
    pub fn id(&self) -> Option<MatchId> {
        self.match_id.as_deref()?.trim().parse().ok()
    }
}

/// 比赛列表里的摘要，没有单位数据。
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct MatchSummary {
    pub map_id: Option<i64>,
    pub end_time: Option<i64>,
    pub total_play_time_in_sec: Option<i64>,
    pub winner_team: Option<i64>,
    pub victory_level: Option<i64>,
    #[serde(rename = "Type")]
    pub match_type: Option<i64>,
    #[serde(deserialize_with = "null_as_default")]
    pub data: BTreeMap<String, MatchSummaryPlayer>,
}

impl MatchSummary {
    pub fn winner(&self) -> Option<Team> {
        winner_of(self.winner_team)
    }

    /// 按玩家遍历，不含 AI 玩家。
    pub fn players(&self) -> impl Iterator<Item = (PlayerId, &MatchSummaryPlayer)> {
        self.data
            .iter()
            .filter_map(|(key, player)| Some((human_id(player.id, key)?, player)))
    }
}

/// 真人玩家的 ID。键里的 `Id` 缺失时用外层的键补上。
///
/// 遭遇战与合作模式里的 AI 玩家 ID 是负数，每场比赛重新编号，不对应任何玩家档案，返回 `None`。
fn human_id(id: Option<i64>, key: &str) -> Option<PlayerId> {
    let id = id.or_else(|| key.trim().parse().ok())?;
    u64::try_from(id).ok().map(PlayerId)
}

fn winner_of(winner_team: Option<i64>) -> Option<Team> {
    match winner_team.unwrap_or(0) {
        id @ (0 | 1) => Team::from_api_id(id),
        _ => None,
    }
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct MatchSummaryPlayer {
    /// AI 玩家为负数。
    pub id: Option<i64>,
    pub name: Option<String>,
    pub team_id: Option<i64>,
    pub old_rating: Option<f64>,
    pub new_rating: Option<f64>,
    pub destruction_score: Option<i64>,
    pub losses_score: Option<i64>,
    pub objectives_captured: Option<i64>,
}

impl MatchSummaryPlayer {
    pub fn team(&self) -> Option<Team> {
        Team::from_api_id(self.team_id.unwrap_or(0))
    }
}

// ---------------------------------------------------------------- 单位目录

/// `/api/units`
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct UnitsResponse {
    #[serde(deserialize_with = "null_as_default")]
    pub units: Vec<UnitInfo>,
    pub count: Option<i64>,
}

/// 单位目录里的一项。
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct UnitInfo {
    pub id: Option<i64>,
    pub name: Option<String>,
    pub hud_name: Option<String>,
    pub country_id: Option<i64>,
    /// 基础价格。单位的实际成本还取决于改装选项，这个值只能用于明确标注的估算。
    pub cost: Option<i64>,
    pub original_cost: Option<i64>,
    #[serde(rename = "type")]
    pub unit_type: Option<i64>,
    /// 分类：1 步兵，2 坦克，3 火炮与防空，5 直升机，6 固定翼。
    pub category_type: Option<i64>,
    pub role: Option<i64>,
    pub owner_infantry_id: Option<i64>,
    pub is_unit_modification: Option<bool>,
    pub display_in_armory: Option<bool>,
    pub country: Option<UnitCountry>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct UnitCountry {
    pub id: Option<i64>,
    pub name: Option<String>,
    #[serde(rename = "uiName")]
    pub ui_name: Option<String>,
}

/// BATrace 网站搜索的结果，输入可以是昵称、玩家 ID 或 Steam64。
#[derive(Debug, Clone, Default, PartialEq, Deserialize, serde::Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(default)]
pub struct PlayerSearchResponse {
    pub players: Vec<PlayerSearchHit>,
    pub count: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize, serde::Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(default)]
pub struct PlayerSearchHit {
    pub id: u64,
    pub name: String,
    pub level: Option<i64>,
    pub steam_id: Option<String>,
}
