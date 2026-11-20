//! 从接口资料里提炼出的玩家摘要。
//!
//! 只包含客观数值和由它们直接算出的比率，不对玩家下任何结论（见 ADR-0002）。

use bax_batrace::model::{PlayerInfoResponse, PlayerStats};
use serde::Serialize;
use std::collections::BTreeMap;
use std::sync::LazyLock;

/// 一名玩家的资料摘要。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ProfileSummary {
    /// 接口资料里的名字；空串记为 `None`。
    pub name: Option<String>,
    pub level: Option<i64>,
    /// ELO。
    pub rating: Option<f64>,
    pub rank: Option<i64>,
    pub rating_games: Option<i64>,
    /// 排位统计；部分玩家的累计统计为空，此时为 `None`。
    pub ranked: Option<RankedStats>,
    /// 站点更新这份统计的时间，原样保留站点给出的文本。
    pub stats_updated_at: Option<String>,
    /// 试运行：按累计经验排序，不代表使用场次或本场选择。
    pub favorite_specs: Vec<SpecPreference>,
    pub favorite_spec_pairs: Vec<SpecPreference>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SpecPreference {
    pub ids: Vec<u32>,
    pub name: String,
    pub experience: i64,
    pub experience_share: f64,
}

static SPEC_NAMES: LazyLock<BTreeMap<String, String>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("specializations.json")).expect("内置专精目录有效")
});

fn preferences(values: &BTreeMap<String, i64>, pairs: bool) -> Vec<SpecPreference> {
    let mut normalized = BTreeMap::<Vec<u32>, i64>::new();
    for (key, &experience) in values {
        if experience <= 0 {
            continue;
        }
        let Some(mut ids) = key
            .split('_')
            .map(|v| v.parse::<u32>().ok().filter(|id| *id > 0))
            .collect::<Option<Vec<_>>>()
        else {
            continue;
        };
        ids.sort_unstable();
        ids.dedup();
        if ids.len() != if pairs { 2 } else { 1 } {
            continue;
        }
        // 有些响应同时带正反组合键，只计较大的值，避免重复累加。
        normalized
            .entry(ids)
            .and_modify(|value| *value = (*value).max(experience))
            .or_insert(experience);
    }
    let total: f64 = normalized.values().map(|v| *v as f64).sum();
    let mut result: Vec<_> = normalized
        .into_iter()
        .map(|(ids, experience)| {
            let name = ids
                .iter()
                .map(|id| {
                    SPEC_NAMES
                        .get(&id.to_string())
                        .cloned()
                        .unwrap_or_else(|| format!("专精 #{id}"))
                })
                .collect::<Vec<_>>()
                .join(" + ");
            SpecPreference {
                ids,
                name,
                experience,
                experience_share: experience as f64 / total,
            }
        })
        .collect();
    result.sort_by(|a, b| {
        b.experience
            .cmp(&a.experience)
            .then_with(|| a.ids.cmp(&b.ids))
    });
    result.truncate(3);
    result
}

/// 排位赛的累计统计。比率在分母为零或数据缺失时为 `None`。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct RankedStats {
    pub fights: Option<i64>,
    pub wins: Option<i64>,
    pub losses: Option<i64>,
    pub leaves: Option<i64>,
    /// 胜场 ÷（胜场 + 负场）。
    pub win_rate: Option<f64>,
    /// 击毁单位数 ÷ 损失单位数。
    pub kill_loss_ratio: Option<f64>,
    /// 击毁价值 ÷ 损失价值。
    pub cost_ratio: Option<f64>,
    /// 平均每场时长，分钟。
    pub average_minutes: Option<f64>,
}

impl ProfileSummary {
    /// 从接口响应提炼摘要。响应里没有基本资料时返回 `None`。
    pub fn from_response(response: &PlayerInfoResponse) -> Option<ProfileSummary> {
        let info = response.info.as_ref()?;
        Some(ProfileSummary {
            name: info.name.clone().filter(|n| !n.trim().is_empty()),
            level: info.level,
            rating: info.rating,
            rank: info.rank,
            rating_games: info.rating_games,
            ranked: response.stat_info.as_ref().map(RankedStats::from_stats),
            stats_updated_at: response
                .stat_info
                .as_ref()
                .and_then(|s| s.updated_at.clone()),
            favorite_specs: response
                .stat_info
                .as_ref()
                .map(|s| preferences(&s.spec_exp, false))
                .unwrap_or_default(),
            favorite_spec_pairs: response
                .stat_info
                .as_ref()
                .map(|s| preferences(&s.combined_specs_exp, true))
                .unwrap_or_default(),
        })
    }

    /// 从保存下来的响应原文提炼摘要。原文无法解析时返回 `None`。
    pub fn from_json(raw: &str) -> Option<ProfileSummary> {
        let response: PlayerInfoResponse = serde_json::from_str(raw).ok()?;
        ProfileSummary::from_response(&response)
    }
}

impl RankedStats {
    fn from_stats(stats: &PlayerStats) -> RankedStats {
        let wins = stats.win_count_rt;
        let losses = stats.loss_count_rt;
        RankedStats {
            fights: stats.fights_count_rt,
            wins,
            losses,
            leaves: stats.leave_count_rt,
            win_rate: match (wins, losses) {
                (Some(w), Some(l)) => ratio(Some(w), Some(w + l)),
                _ => None,
            },
            kill_loss_ratio: ratio(stats.units_killed_count_rt, stats.units_lost_count_rt),
            cost_ratio: ratio(stats.units_killed_cost_rt, stats.units_lost_cost_rt),
            average_minutes: ratio(stats.match_duration_time_sec_rt, stats.fights_count_rt)
                .map(|secs| secs / 60.0),
        }
    }
}

fn ratio(numerator: Option<i64>, denominator: Option<i64>) -> Option<f64> {
    match (numerator, denominator) {
        (Some(n), Some(d)) if d > 0 => Some(n as f64 / d as f64),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAYER_INFO: &str = include_str!("../../../fixtures/batrace/player_info.json");

    #[test]
    fn preferences_rank_experience_and_keep_unknown_ids() {
        let summary = ProfileSummary::from_json(PLAYER_INFO).expect("fixture");
        assert_eq!(summary.favorite_specs[0].ids, vec![5]);
        assert!((summary.favorite_specs[0].experience_share - 240000.0 / 360000.0).abs() < 1e-9);
        assert_eq!(summary.favorite_spec_pairs[0].ids, vec![3, 5]);
        let values =
            serde_json::from_str(r#"{"3":10,"4":10,"5":10,"999":20,"7":-1,"8":0,"oops":99}"#)
                .expect("fixture");
        let ranked = preferences(&values, false);
        assert_eq!(ranked.len(), 3);
        assert_eq!(ranked[0].name, "专精 #999");
        assert_eq!(ranked[0].experience_share, 0.4);
        assert_eq!(ranked[1].ids, vec![3]);
    }

    #[test]
    fn preferences_handle_missing_and_malformed_pairs() {
        let values =
            serde_json::from_str(r#"{"3_5":20,"5_3":30,"4_4":50,"0_3":80,"3_4_5":90,"bad":100}"#)
                .expect("fixture");
        let ranked = preferences(&values, true);
        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].experience, 30);
        assert_eq!(ranked[0].experience_share, 1.0);
        let summary =
            ProfileSummary::from_json(r#"{"info":{"id":1},"statInfo":null}"#).expect("fixture");
        assert!(summary.favorite_specs.is_empty());
        assert!(summary.favorite_spec_pairs.is_empty());
    }

    #[test]
    fn summarizes_a_full_profile() {
        let summary = ProfileSummary::from_json(PLAYER_INFO).expect("可以解析");
        assert_eq!(summary.name.as_deref(), Some("Starling"));
        assert_eq!(summary.level, Some(87));
        assert_eq!(summary.rating, Some(2143.5));
        assert_eq!(summary.rank, Some(512));
        assert_eq!(summary.rating_games, Some(640));
        assert_eq!(
            summary.stats_updated_at.as_deref(),
            Some("2026-11-16T10:21:07.000Z")
        );

        let ranked = summary.ranked.expect("有排位统计");
        assert_eq!(ranked.fights, Some(640));
        assert_eq!(ranked.leaves, Some(7));
        let close = |a: Option<f64>, b: f64| a.is_some_and(|a| (a - b).abs() < 1e-9);
        assert!(close(ranked.win_rate, 352.0 / 633.0));
        assert!(close(ranked.kill_loss_ratio, 21500.0 / 19800.0));
        assert!(close(ranked.cost_ratio, 4_310_000.0 / 3_980_000.0));
        assert!(close(ranked.average_minutes, 1_180_000.0 / 640.0 / 60.0));
    }

    #[test]
    fn missing_stats_and_zero_denominators_give_no_ratio() {
        let summary = ProfileSummary::from_json(
            r#"{"info":{"id":1,"name":"  ","rating":1500},"statInfo":null}"#,
        )
        .expect("可以解析");
        assert_eq!(summary.name, None, "空白名字视为没有");
        assert_eq!(summary.ranked, None);

        let summary = ProfileSummary::from_json(
            r#"{"info":{"id":1},"statInfo":{"fightsCountRt":0,"winCountRt":0,"lossCountRt":0,"unitsKilledCountRt":5,"unitsLostCountRt":0}}"#,
        )
        .expect("可以解析");
        let ranked = summary.ranked.expect("有排位统计");
        assert_eq!(ranked.win_rate, None);
        assert_eq!(ranked.kill_loss_ratio, None);
        assert_eq!(ranked.average_minutes, None);
    }

    #[test]
    fn unusable_input_gives_none() {
        assert_eq!(ProfileSummary::from_json("not json"), None);
        assert_eq!(ProfileSummary::from_json(r#"{"info":null}"#), None);
    }
}
