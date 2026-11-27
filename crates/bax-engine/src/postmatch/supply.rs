use std::collections::{BTreeMap, HashMap};

use bax_batrace::model::MatchInfo;
use bax_core::Team;
use bax_core::report::{MatchAnalysis, UnitClass, UnitKind};
use serde::Serialize;

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ConsumerActivity {
    pub count: u32,
    pub dead: u32,
    pub refunded: u32,
    pub kills: i64,
    pub damage: f64,
    pub metrics: bax_core::report::UnitMetrics,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SupplyModel {
    /// 只计消耗过补给的实例，无法归类的玩家总量差额没有实例。
    pub activity: Option<ConsumerActivity>,
    pub id: Option<i64>,
    pub name: String,
    pub thumbnail: Option<String>,
    pub class: UnitClass,
    pub points: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SupplyPlayer {
    pub id: i64,
    pub name: String,
    pub team: Team,
    pub consumed: i64,
    pub from_allies: i64,
    pub by_allies: i64,
    pub models: Vec<SupplyModel>,
}

/// UnitData 归属于最终持有者。转交记录仅描述出兵来源，不能再次累加消耗。
pub(super) fn consumption(
    info: &MatchInfo,
    analysis: &MatchAnalysis,
    kinds: &HashMap<i64, UnitKind>,
) -> Vec<SupplyPlayer> {
    info.data
        .iter()
        .filter_map(|(key, p)| {
            let id = p.id.or_else(|| key.parse().ok())?;
            let human = analysis
                .players
                .iter()
                .find(|v| i64::try_from(v.id.get()) == Ok(id));
            let team = human.and_then(|h| h.team).or(p.team())?;
            if !team.is_playing() || (human.is_none() && id >= 0) {
                return None;
            }
            let mut models = BTreeMap::new();
            for unit in p.unit_data.values() {
                let points = unit.total_supply_points_consumed.unwrap_or(0);
                if points <= 0 {
                    continue;
                }
                let kind = unit.id.and_then(|id| kinds.get(&id));
                let appearance = unit
                    .id
                    .and_then(|id| crate::variants::resolve(id, &unit.option_ids));
                let name = appearance
                    .as_ref()
                    .map(|a| a.name.clone())
                    .or_else(|| kind.and_then(|k| k.name.clone()))
                    .unwrap_or_else(|| {
                        unit.id
                            .map_or_else(|| "未知型号".into(), |id| format!("单位 #{id}"))
                    });
                let entry = models
                    .entry((unit.id, name.clone()))
                    .or_insert_with(|| SupplyModel {
                        activity: None,
                        id: unit.id,
                        name,
                        thumbnail: appearance.and_then(|a| a.thumbnail),
                        class: kind.map_or(UnitClass::Other, |k| UnitClass::of(k.category, k.role)),
                        points: 0,
                    });
                entry.points += points;
                let activity = entry.activity.get_or_insert_default();
                activity.count += 1;
                activity.dead += u32::from(unit.death_time.is_some());
                activity.refunded += u32::from(unit.was_refunded.unwrap_or_default());
                activity.kills += unit.killed_count.unwrap_or_default().max(0);
                activity.damage += unit.total_damage_dealt.unwrap_or_default().max(0.0);
                activity.metrics += &super::unit_metrics(unit);
            }
            let mut models: Vec<_> = models.into_values().collect();
            let attributed = models.iter().map(|m| m.points).sum();
            let consumed = p.supply_points_consumed.unwrap_or(attributed);
            if consumed > attributed {
                models.push(SupplyModel {
                    activity: None,
                    id: None,
                    name: "未归类消耗".into(),
                    thumbnail: None,
                    class: UnitClass::Other,
                    points: consumed - attributed,
                });
            }
            Some(SupplyPlayer {
                id,
                name: human
                    .and_then(|h| h.name.clone())
                    .or_else(|| p.name.clone())
                    .unwrap_or_else(|| {
                        if id < 0 {
                            format!("AI {id}")
                        } else {
                            format!("ID {id}")
                        }
                    }),
                team,
                consumed,
                from_allies: p.supply_points_consumed_from_allies.unwrap_or(0),
                by_allies: p.supply_points_consumed_by_allies.unwrap_or(0),
                models,
            })
        })
        .collect()
}
