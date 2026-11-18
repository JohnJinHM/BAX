//! 战报的解读：把接口模型转成核心库的输入，加上日志里的信息，交给界面。

use std::collections::HashMap;

use bax_batrace::model::{MatchInfo, UnitsResponse};
use bax_core::report::{self, MatchAnalysis, PlayerInput, ReportInput, UnitInput, UnitKind};
use bax_core::{MatchId, PlayerId, Team, Timestamp};
use bax_store::{ReportMeta, Store};
use serde::Serialize;

/// 参考数据里单位目录的键。
pub(crate) const UNITS_KEY: &str = "batrace.units";

/// 一场比赛的战后分析，界面按需读取。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct MatchReportView {
    pub analysis: MatchAnalysis,
    /// 日志里的地图名。
    pub map: Option<String>,
    pub self_id: Option<PlayerId>,
    pub self_team: Option<Team>,
    pub fetched_at: Timestamp,
    /// 用到的单位目录的获取时间；还没有目录时为 `None`。
    pub catalog_fetched_at: Option<Timestamp>,
}

/// 摘出列表要用的几项。
pub(crate) fn report_meta(info: &MatchInfo) -> ReportMeta {
    ReportMeta {
        start_time: info.start_time.map(Timestamp::from_secs),
        duration_secs: info.total_play_time_in_sec,
        map_id: info.map_id,
        winner: info.winner(),
    }
}

pub(crate) fn report_input(id: MatchId, info: &MatchInfo) -> ReportInput {
    let players = info
        .players()
        .map(|(pid, p)| PlayerInput {
            name: p.name.clone(),
            team: p.team(),
            old_rating: p.old_rating,
            new_rating: p.new_rating,
            spawned_score: p.total_spawned_unit_score,
            refunded_score: p.total_refunded_unit_score,
            destruction: p.destruction,
            losses: p.losses,
            destruction_score: p.destruction_score,
            losses_score: p.losses_score,
            damage_dealt: p.damage_dealt,
            damage_received: p.damage_received,
            objectives: p.objectives_captured,
            supply_consumed: p.supply_points_consumed,
            supply_from_allies: p.supply_points_consumed_from_allies,
            supply_by_allies: p.supply_points_consumed_by_allies,
            supply_captured: p.supply_captured,
            supply_airdropped: p.supply_airdropped,
            deserter: p.deserter.unwrap_or(false),
            units: p
                .unit_data
                .iter()
                .filter_map(|(key, u)| {
                    Some(UnitInput {
                        key: key.clone(),
                        unit_id: u.id?,
                        spawn_time: u.spawn_time,
                        death_time: u.death_time,
                        kills: u.killed_count,
                        damage_dealt: u.total_damage_dealt,
                        refunded: u.was_refunded.unwrap_or(false),
                        airdropped: u.was_airdropped.unwrap_or(false),
                    })
                })
                .collect(),
            transferred_out: p.transferred_units.keys().cloned().collect(),
            ..PlayerInput::new(pid)
        })
        .collect();
    ReportInput {
        start_time: info.start_time,
        end_time: info.end_time,
        duration_secs: info.total_play_time_in_sec,
        map_id: info.map_id,
        winner: info.winner(),
        alpha_points: info.team_match_points.and_then(|p| p.alpha),
        bravo_points: info.team_match_points.and_then(|p| p.bravo),
        players,
        ..ReportInput::new(id)
    }
}

pub(crate) fn catalog(units: &UnitsResponse) -> HashMap<i64, UnitKind> {
    units
        .units
        .iter()
        .filter_map(|u| {
            let name = u
                .hud_name
                .clone()
                .filter(|n| !n.trim().is_empty())
                .or_else(|| u.name.clone());
            Some((
                u.id?,
                UnitKind {
                    name,
                    category: u.category_type,
                    role: u.role,
                    base_cost: u.cost,
                },
            ))
        })
        .collect()
}

/// 从长期库读出一场比赛的战报并统计。没有战报时为 `None`。
pub(crate) fn load_view(store: &Store, id: MatchId) -> Result<Option<MatchReportView>, String> {
    let Some(stored) = store.report(id).map_err(|e| e.to_string())? else {
        return Ok(None);
    };
    let info: MatchInfo =
        serde_json::from_str::<bax_batrace::model::MatchReportResponse>(&stored.raw)
            .map_err(|e| format!("战报无法解析：{e}"))?
            .match_info
            .ok_or_else(|| "战报是空的".to_owned())?;

    let units = store.reference(UNITS_KEY).map_err(|e| e.to_string())?;
    let (kinds, catalog_fetched_at) = match &units {
        Some(stored) => match serde_json::from_str::<UnitsResponse>(&stored.raw) {
            Ok(parsed) => (catalog(&parsed), Some(stored.fetched_at)),
            Err(err) => {
                tracing::warn!(error = %err, "单位目录无法解析");
                (HashMap::new(), None)
            }
        },
        None => (HashMap::new(), None),
    };

    let mut analysis = report::analyze(&report_input(id, &info), &kinds);
    // 战报里没有名字的，用档案或日志里的名字补上
    for p in &mut analysis.players {
        if p.name.is_none()
            && let Ok(Some(record)) = store.player(p.id)
        {
            p.name = record.display_name().map(str::to_owned);
        }
    }
    let record = store.match_record(id).map_err(|e| e.to_string())?;
    Ok(Some(MatchReportView {
        analysis,
        map: record.as_ref().and_then(|m| m.map.clone()),
        self_id: record.as_ref().and_then(|m| m.self_id),
        self_team: record.as_ref().and_then(|m| m.self_team),
        fetched_at: stored.fetched_at,
        catalog_fetched_at,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPORT: &str = include_str!("../../../fixtures/batrace/match_report.json");
    const UNITS: &str = include_str!("../../../fixtures/batrace/units.json");

    #[test]
    fn the_fixture_report_converts_and_analyzes() {
        let info = serde_json::from_str::<bax_batrace::model::MatchReportResponse>(REPORT)
            .expect("战报")
            .match_info
            .expect("有内容");
        let units: UnitsResponse = serde_json::from_str(UNITS).expect("目录");
        let input = report_input(MatchId(1), &info);
        assert!(!input.players.is_empty());
        let analysis = report::analyze(&input, &catalog(&units));
        assert_eq!(analysis.players.len(), input.players.len());
        let sorties: u32 = analysis.players.iter().map(|p| p.sorties).sum();
        let instances: usize = input.players.iter().map(|p| p.units.len()).sum();
        assert_eq!(sorties as usize, instances, "每个实例都算到某个出兵者名下");
    }

    #[test]
    fn a_stored_report_loads_into_a_view() {
        let mut store = Store::open_in_memory().expect("长期库");
        let info = serde_json::from_str::<bax_batrace::model::MatchReportResponse>(REPORT)
            .expect("战报")
            .match_info
            .expect("有内容");
        store
            .save_report(
                MatchId(1),
                REPORT,
                report_meta(&info),
                Timestamp::from_secs(10),
            )
            .expect("存战报");
        let view = load_view(&store, MatchId(1)).expect("读").expect("有战报");
        assert_eq!(view.fetched_at, Timestamp::from_secs(10));
        assert_eq!(view.catalog_fetched_at, None);
        assert!(!view.analysis.catalog_complete, "没有目录时成本不完整");

        store
            .set_reference(UNITS_KEY, UNITS, Timestamp::from_secs(20))
            .expect("存目录");
        let view = load_view(&store, MatchId(1)).expect("读").expect("有战报");
        assert_eq!(view.catalog_fetched_at, Some(Timestamp::from_secs(20)));
        assert!(load_view(&store, MatchId(2)).expect("读").is_none());
    }
}
