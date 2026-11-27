//! 战报的解读：把接口模型转成核心库的输入，加上日志里的信息，交给界面。

use std::collections::{BTreeMap, HashMap};

use bax_batrace::model::{MatchInfo, UnitsResponse};
use bax_core::report::{self, MatchAnalysis, PlayerInput, ReportInput, UnitInput, UnitKind};
use bax_core::{MatchId, PlayerId, Team, Timestamp};
use bax_store::{ReportMeta, Store};
use serde::Serialize;

mod supply;

/// 参考数据里单位目录的键。
pub(crate) const UNITS_KEY: &str = "batrace.units";

/// 一场比赛的战后分析，界面按需读取。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct MatchReportView {
    pub match_id: MatchId,
    pub supply: Vec<supply::SupplyPlayer>,
    pub mode: crate::MatchMode,
    pub spectators: Vec<ReportSpectator>,
    pub ai_players: [u32; 2],
    /// 仅由这一场战报推导出的专精。
    pub specs: BTreeMap<PlayerId, Vec<crate::specs::SpecPreference>>,
    pub analysis: Option<MatchAnalysis>,
    /// 日志里的地图名。
    pub map: Option<String>,
    pub self_id: Option<PlayerId>,
    pub self_team: Option<Team>,
    pub fetched_at: Timestamp,
    /// 用到的单位目录的获取时间；还没有目录时为 `None`。
    pub catalog_fetched_at: Option<Timestamp>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ReportSpectator {
    pub id: PlayerId,
    pub name: Option<String>,
}

/// STB fight IDs from BA Hub backend/src/data/constants.ts and locally matched logs.
/// Display strings use the game's Simplified Chinese map_loc entries.
pub(crate) fn map_name(id: Option<i64>, logged: Option<String>) -> Option<String> {
    let Some(id) = id else { return logged };
    let name = match id {
        1 => "测试地图",
        3 => "波罗的斯克半岛",
        4 => "黑海岸",
        5 => "里加机场",
        6 => "蜿蜒之河",
        7 => "考纳斯水坝",
        8 => "寒水港",
        9 => "契卡洛夫斯克空军基地",
        10 => "死亡公路",
        11 => "死亡中心",
        12 => "石油热潮",
        13 => "苏瓦乌基走廊",
        14 => "叶尔加瓦",
        15 => "纳尔瓦冰雪城堡",
        16 => "克莱佩达裂谷",
        17 => "鲁达森林",
        20 => "风啸湾",
        21 => "切尔尼亚霍夫斯克",
        22 => "伊格纳利纳核电站",
        23 => "加里宁格勒",
        25 => "卡达加军事基地",
        28 => "煤山",
        _ => return logged,
    };
    Some(name.to_owned())
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

fn unit_metrics(unit: &bax_batrace::model::UnitInstance) -> report::UnitMetrics {
    let damage = |v: Option<f64>| v.unwrap_or_default().max(0.0);
    report::UnitMetrics {
        buildings: unit.building_destroyed_count.unwrap_or_default().max(0),
        damage_received: damage(unit.total_damage_received),
        friendly_damage_dealt: damage(unit.total_damage_friendly_fire_dealt),
        friendly_damage_received: damage(unit.total_damage_received_by_friendly_fire),
        self_damage: damage(unit.total_self_damage_dealt),
        self_destroyed: u32::from(unit.self_destruction.unwrap_or_default()),
        airdropped: u32::from(unit.was_airdropped.unwrap_or_default()),
        supply: unit.total_supply_points_consumed.unwrap_or_default().max(0),
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
                        metrics: unit_metrics(u),
                        key: key.clone(),
                        option_ids: u.option_ids.clone(),
                        appearance: crate::variants::resolve(u.id?, &u.option_ids),
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
    if store.report_cancelled(id).map_err(|e| e.to_string())? {
        return Ok(None);
    }
    let Some(bax_store::IndexedReport {
        report: stored,
        mode,
        units: deployed,
    }) = store.indexed_report(id).map_err(|e| e.to_string())?
    else {
        return Ok(None);
    };
    let info: MatchInfo =
        serde_json::from_str::<bax_batrace::model::MatchReportResponse>(&stored.raw)
            .map_err(|e| format!("战报无法解析：{e}"))?
            .match_info
            .ok_or_else(|| "战报是空的".to_owned())?;

    let record = store.match_record(id).map_err(|e| e.to_string())?;
    let roster = store.match_players(id).map_err(|e| e.to_string())?;
    let mut spectators: BTreeMap<_, _> = info
        .players()
        .filter(|(_, p)| p.team() == Some(Team::Spectators))
        .map(|(id, p)| (id, p.name.clone()))
        .collect();
    for p in roster.iter().filter(|p| p.team == Some(Team::Spectators)) {
        let name = spectators.entry(p.player).or_default();
        if name.is_none() {
            *name = p.log_name.clone();
        }
    }
    let mut ai_players = [0; 2];
    for (key, player) in &info.data {
        if player
            .id
            .or_else(|| key.parse().ok())
            .is_some_and(|id| id < 0)
        {
            match player.team() {
                Some(Team::Alpha) => ai_players[0] += 1,
                Some(Team::Bravo) => ai_players[1] += 1,
                _ => {}
            }
        }
    }
    let mut view = MatchReportView {
        match_id: id,
        mode,
        spectators: spectators
            .into_iter()
            .map(|(id, name)| ReportSpectator { id, name })
            .collect(),
        ai_players,
        specs: BTreeMap::new(),
        analysis: None,
        supply: Vec::new(),
        map: record.as_ref().and_then(|m| m.map.clone()),
        self_id: record.as_ref().and_then(|m| m.self_id),
        self_team: record.as_ref().and_then(|m| m.self_team),
        fetched_at: stored.fetched_at,
        catalog_fetched_at: None,
    };
    if mode != crate::MatchMode::Ranked {
        return Ok(Some(view));
    }
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

    let mut input = report_input(id, &info);
    for p in &mut input.players {
        if let Some(team) = roster
            .iter()
            .find(|r| r.player == p.id)
            .and_then(|r| r.team)
        {
            p.team = Some(team);
        }
    }
    input.players.retain(|p| p.team != Some(Team::Spectators));
    view.specs = input
        .players
        .iter()
        .filter_map(|p| {
            Some((
                p.id,
                crate::specs::infer(deployed.get(&p.id)?)?
                    .into_iter()
                    .map(|id| crate::specs::entry(vec![id], 1))
                    .collect(),
            ))
        })
        .collect();
    let mut analysis = report::analyze(&input, &kinds);
    // 战报里没有名字的，用档案或日志里的名字补上
    for p in &mut analysis.players {
        if p.name.is_none()
            && let Ok(Some(record)) = store.player(p.id)
        {
            p.name = record.display_name().map(str::to_owned);
        }
    }
    view.supply = supply::consumption(&info, &analysis, &kinds);
    view.analysis = Some(analysis);
    view.catalog_fetched_at = catalog_fetched_at;
    Ok(Some(view))
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPORT: &str = include_str!("../../../fixtures/batrace/match_report.json");
    const UNITS: &str = include_str!("../../../fixtures/batrace/units.json");

    #[test]
    fn supplies_follow_the_consumer_even_without_a_spawn_time() {
        let mut store = Store::open_in_memory().expect("库");
        let raw = serde_json::json!({"matchInfo":{"Data":{
            "11":{"TransferredUnits":{"a":{"Id":3}}},
            "22":{"SupplyPointsConsumed":90,"SupplyPointsConsumedFromAllies":15,
                  "SupplyPointsConsumedByAllies":12,"UnitData":{
                      "a":{"Id":3,"TotalSupplyPointsConsumed":60,"KilledCount":3,"WasRefunded":true,
                           "WasAirdropped":true,"BuildingDestroyedCount":2,"TotalDamageReceived":23.75},
                      "b":{"Id":3,"TotalSupplyPointsConsumed":20,"DeathTime":1040,"TotalDamageReceived":-1},
                      "c":{"Id":3,"KilledCount":5}}},
            "33":{"TeamId":1,"UnitData":{"b":{"Id":3,"TotalSupplyPointsConsumed":35}}}
        }}}).to_string();
        store
            .save_report(
                MatchId(77),
                &raw,
                Default::default(),
                Timestamp::from_secs(10),
            )
            .expect("归档");
        let view = load_view(&store, MatchId(77)).expect("读取").expect("战报");
        let by_id: BTreeMap<_, _> = view.supply.iter().map(|p| (p.id, p)).collect();
        assert_eq!(by_id.len(), 3);
        assert_eq!((by_id[&11].consumed, by_id[&33].consumed), (0, 35));
        let p = by_id[&22];
        assert_eq!((p.consumed, p.from_allies, p.by_allies), (90, 15, 12));
        let model = p.models.iter().find(|m| m.id == Some(3)).expect("具体型号");
        assert_eq!(model.points, 80);
        let usage = model.activity.as_ref().expect("参与消耗的实例");
        assert_eq!(
            (usage.count, usage.kills, usage.refunded, usage.dead),
            (2, 3, 1, 1)
        );
        assert_eq!(usage.metrics.damage_received, 23.75);
        assert_eq!(
            (
                usage.metrics.buildings,
                usage.metrics.airdropped,
                usage.metrics.supply
            ),
            (2, 1, 80)
        );
        let sender = view
            .analysis
            .as_ref()
            .expect("多人战报")
            .players
            .iter()
            .find(|p| p.id.get() == 11)
            .expect("原出兵者");
        let sent = &sender.units[0];
        assert_eq!(
            (
                sent.count,
                sent.transferred,
                sent.kills,
                sent.refunded,
                sent.metrics.supply
            ),
            (1, 1, 3, 1, 60)
        );
        assert!(
            p.models
                .iter()
                .find(|m| m.id.is_none())
                .expect("未归类")
                .activity
                .is_none()
        );
        assert_eq!(
            p.models
                .iter()
                .find(|m| m.id.is_none())
                .expect("未归类")
                .points,
            10
        );
    }

    #[test]
    fn custom_reports_keep_metadata_and_skip_analysis() {
        let mut store = Store::open_in_memory().expect("验证数据");
        let raw = serde_json::json!({"matchInfo": {"Type": 1, "Data": {
            "101": {"Name": "Ash", "TeamId": 0},
            "102": {"Name": "Birch", "TeamId": 1},
            "103": {"Name": "Cedar", "TeamId": 100}
        }}})
        .to_string();
        store
            .save_report(
                MatchId(9_000_001),
                &raw,
                ReportMeta::default(),
                Timestamp::from_secs(1),
            )
            .expect("验证数据");
        let view = load_view(&store, MatchId(9_000_001))
            .expect("验证数据")
            .expect("验证数据");
        assert_eq!(view.mode, crate::MatchMode::Custom);
        assert!(view.analysis.is_none() && view.specs.is_empty() && view.supply.is_empty());
        assert_eq!(
            view.spectators,
            vec![ReportSpectator {
                id: PlayerId(103),
                name: Some("Cedar".into())
            }]
        );
        assert!(!view.specs.contains_key(&PlayerId(103)));
    }

    #[test]
    fn pve_reports_count_ai_without_creating_human_players() {
        let mut store = Store::open_in_memory().expect("验证数据");
        let raw = serde_json::json!({"matchInfo": {"Data": {
            "101": {"Name": "Ash"}, "-2": {"Id": -2, "TeamId": 1}, "-3": {"TeamId": 1}
        }}})
        .to_string();
        store
            .save_report(
                MatchId(9_000_002),
                &raw,
                ReportMeta::default(),
                Timestamp::from_secs(1),
            )
            .expect("验证数据");
        let view = load_view(&store, MatchId(9_000_002))
            .expect("验证数据")
            .expect("验证数据");
        assert_eq!(view.mode, crate::MatchMode::Pve);
        assert_eq!(view.ai_players, [0, 2]);
        assert!(view.analysis.is_none() && view.supply.is_empty() && view.specs.is_empty());
    }

    #[test]
    fn cached_option_ids_reach_the_report_without_refetching() {
        let raw = serde_json::json!({"matchInfo": {
            "StartTime": 1000, "TotalPlayTimeInSec": 600,
            "Data": {"101": {"Id": 101, "UnitData": {
                "base": {"Id": 3, "SpawnTime": 1003, "OptionIds": []},
                "upgrade": {"Id": 3, "SpawnTime": 1003, "OptionIds": [471]}
            }}}
        }})
        .to_string();
        let mut store = Store::open_in_memory().expect("库");
        store
            .save_report(
                MatchId(1),
                &raw,
                ReportMeta::default(),
                Timestamp::from_secs(2000),
            )
            .expect("缓存");
        let view = load_view(&store, MatchId(1)).expect("解析").expect("战报");
        let analysis = view.analysis.expect("多人战报");
        let units = &analysis.players[0].units;
        assert_eq!(units.len(), 2);
        assert!(units.iter().any(|u| u.unit_name == "BMP-3"));
        assert!(units.iter().any(|u| u.unit_name == "BMP-3M Epokha"));
        assert!(
            analysis
                .sorties
                .iter()
                .any(|u| u.thumbnail.as_deref() == Some("RU_BMP3_EPOKHA-Label.webp"))
        );
    }

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
        let raw = r#"{"matchInfo":{"Data":{"101":{"UnitData":{"a":{"Id":3,"SpawnTime":100}}}}}}"#;
        let info = serde_json::from_str::<bax_batrace::model::MatchReportResponse>(raw)
            .expect("战报")
            .match_info
            .expect("有内容");
        store
            .save_report(
                MatchId(1),
                raw,
                report_meta(&info),
                Timestamp::from_secs(10),
            )
            .expect("存战报");
        let view = load_view(&store, MatchId(1)).expect("读").expect("有战报");
        assert_eq!(view.fetched_at, Timestamp::from_secs(10));
        assert_eq!(view.catalog_fetched_at, None);
        assert!(
            !view.analysis.expect("多人战报").catalog_complete,
            "没有目录时成本不完整"
        );

        store
            .set_reference(UNITS_KEY, UNITS, Timestamp::from_secs(20))
            .expect("存目录");
        let view = load_view(&store, MatchId(1)).expect("读").expect("有战报");
        assert_eq!(view.catalog_fetched_at, Some(Timestamp::from_secs(20)));
        assert!(load_view(&store, MatchId(2)).expect("读").is_none());
    }
}
