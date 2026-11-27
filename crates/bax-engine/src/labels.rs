//! Compact per-match evidence shared by all roster members.
use bax_batrace::model::MatchInfo;
use bax_core::{
    MatchId, PlayerId, Team,
    report::{UnitClass, UnitKind},
};
use chrono::{DateTime, Local};
use serde::Serialize;
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, HashMap};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct PlayerLabel {
    pub symbol: String,
    pub detail: String,
}
pub(crate) type Labels = BTreeMap<PlayerId, Vec<PlayerLabel>>;
#[derive(Default)]
pub(crate) struct Facts {
    pub(crate) players: BTreeMap<PlayerId, Stats>,
    duration: i64,
    pub(crate) start: Option<i64>,
    pub(crate) map: Option<String>,
}
#[derive(Default)]
pub(crate) struct Stats {
    team: Option<Team>,
    kills: i64,
    spawns: usize,
    art: i64,
    wing: i64,
    fuel: i64,
    jet_fuel: i64,
    class_known: bool,
    score: Option<f64>,
}
impl Facts {
    pub(crate) fn ranked(m: &MatchInfo, catalog: &HashMap<i64, UnitKind>) -> Option<Self> {
        if m.match_type.unwrap_or(0) != 0 || m.data.len() != 10 {
            return None;
        }
        let mut players = BTreeMap::new();
        for (id, p) in m.players() {
            let mut s = Stats {
                team: p.team(),
                spawns: p.unit_data.len(),
                class_known: !p.unit_data.is_empty(),
                score: p
                    .total_spawned_unit_score
                    .map(|v| v as f64 - 0.8 * p.total_refunded_unit_score.unwrap_or(0) as f64),
                ..Default::default()
            };
            for unit in p.unit_data.values() {
                let k = unit.killed_count.unwrap_or(0);
                let fuel = unit.total_supply_points_consumed.unwrap_or(0);
                s.kills += k;
                s.fuel += fuel;
                match unit
                    .id
                    .and_then(|id| catalog.get(&id))
                    .filter(|u| u.category.is_some())
                    .map(|u| UnitClass::of(u.category, u.role))
                {
                    Some(UnitClass::Artillery) => s.art += k,
                    Some(UnitClass::Jet) => {
                        s.wing += k;
                        s.jet_fuel += fuel
                    }
                    None if k > 0 || fuel > 0 => s.class_known = false,
                    _ => {}
                }
            }
            players.insert(id, s);
        }
        if [Team::Alpha, Team::Bravo]
            .into_iter()
            .any(|t| players.values().filter(|s| s.team == Some(t)).count() != 5)
        {
            return None;
        }
        Some(Self {
            players,
            duration: m.total_play_time_in_sec.unwrap_or(0),
            start: m.start_time.filter(|t| *t > 0),
            map: crate::postmatch::map_name(m.map_id, None),
        })
    }

    fn excess(&self, s: &Stats) -> Option<String> {
        let cap = 1000.0 + 5.0 * self.duration as f64;
        let real = s.score?;
        if self.duration <= 0
            || real <= cap
            || self.players.values().any(|s| s.score.is_none())
            || self
                .players
                .values()
                .filter(|s| s.score.is_some_and(|v| v > cap))
                .count()
                >= 5
        {
            return None;
        }
        Some(format!("净出兵 {real:.1} / 上限 {cap:.0}"))
    }
    fn remote(&self, id: PlayerId, s: &Stats) -> Option<String> {
        if self.duration < 1500 || !s.class_known || s.kills == 0 {
            return None;
        }
        let weighted = s.art as f64 + 0.7 * s.wing as f64;
        let share = weighted / s.kills as f64;
        if weighted >= 30. && share >= 0.45 {
            return Some(format!(
                "远程折算击杀 {weighted:.1}，占 {:.0}%",
                share * 100.
            ));
        }
        let ratio = s.art as f64 / s.kills as f64;
        if s.kills < 10 || ratio <= 0.4 {
            return None;
        }
        let mut best = 0.0_f64;
        for (_, mate) in self
            .players
            .iter()
            .filter(|(other, p)| **other != id && p.team == s.team)
        {
            if !mate.class_known || mate.kills == 0 {
                return None;
            }
            best = best.max(mate.art as f64 / mate.kills as f64);
        }
        (best <= 0.2 || ratio > 2. * best).then(|| {
            format!(
                "炮兵击杀占 {:.0}%，队友最高 {:.0}%",
                ratio * 100.,
                best * 100.
            )
        })
    }
    fn efficiency(&self, id: PlayerId, s: &Stats) -> Option<(f64, f64)> {
        if s.spawns < 5 {
            return None;
        }
        let others = self.players.iter().filter(|(other, _)| **other != id);
        let mut k = 0;
        let mut n = 0;
        for (_, p) in others {
            if p.spawns == 0 {
                return None;
            }
            k += p.kills;
            n += p.spawns
        }
        (k > 0).then(|| (s.kills as f64 / s.spawns as f64, k as f64 / n as f64))
    }
    fn air(&self, s: &Stats) -> Option<f64> {
        let mut total = 0;
        for p in self.players.values().filter(|p| p.team == s.team) {
            if p.spawns == 0 {
                return None;
            }
            total += p.fuel
        }
        (s.class_known && total > 0).then(|| s.jet_fuel as f64 / total as f64)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Member {
    pub id: PlayerId,
    pub side: Option<String>,
    pub enemy: bool,
    pub recent: Vec<MatchId>,
    pub name: String,
}
pub(crate) struct History {
    pub matches: Vec<MatchId>,
    pub first_three_known: bool,
}
fn add(labels: &mut Labels, id: PlayerId, symbol: &str, detail: String) {
    labels.entry(id).or_default().push(PlayerLabel {
        symbol: symbol.into(),
        detail: format!("仅统计多人对战（5v5）。\n{detail}"),
    })
}

fn with_games(rule: &str, mut games: Vec<(&Facts, String)>) -> String {
    games.sort_by_key(|(game, _)| Reverse(game.start));
    let mut detail = rule.to_owned();
    if !games.is_empty() {
        detail.push('\n');
    }
    for (game, value) in games {
        let date = game
            .start
            .and_then(|secs| DateTime::from_timestamp(secs, 0))
            .map(|t| t.with_timezone(&Local).format("%m/%d %H:%M").to_string())
            .unwrap_or_else(|| "日期未知".to_owned());
        detail.push_str(&format!(
            "\n{date}  {}",
            game.map.as_deref().unwrap_or("地图未知")
        ));
        if !value.is_empty() {
            detail.push_str("  ·  ");
            detail.push_str(&value);
        }
    }
    detail
}

pub(crate) fn evaluate(
    members: &[Member],
    histories: &BTreeMap<PlayerId, History>,
    facts: &BTreeMap<MatchId, Option<Facts>>,
) -> Labels {
    let mut out = Labels::new();
    let mut air = Vec::new();
    let mut groups = BTreeMap::<Vec<PlayerId>, BTreeSet<MatchId>>::new();
    for member in members {
        let id = member.id;
        let Some(history) = histories.get(&id) else {
            continue;
        };
        let mut remote = vec![];
        let mut eff = vec![];
        let mut jets = vec![];
        let mut scores = vec![];
        for (i, mid) in history.matches.iter().enumerate() {
            let Some(f) = facts.get(mid).and_then(Option::as_ref) else {
                continue;
            };
            let Some(s) = f.players.get(&id) else {
                continue;
            };
            if let Some(v) = f.excess(s) {
                scores.push((f, v));
            }
            if let Some(v) = f.remote(id, s) {
                remote.push((f, v))
            }
            if let Some((mine, base)) = f.efficiency(id, s)
                && mine >= 0.8
                && mine >= 1.6 * base
            {
                eff.push((f, format!("本人 {mine:.2} / 其余九人 {base:.2}")))
            }
            if i < 3
                && let Some(r) = f.air(s)
                && r > 0.4
            {
                jets.push((f, format!("{:.1}%", 100. * r)))
            }
            if member.side.is_none() {
                continue;
            }
            let mates: Vec<_> = members
                .iter()
                .filter(|m| {
                    m.id != id
                        && m.side == member.side
                        && f.players.get(&m.id).is_some_and(|p| p.team == s.team)
                })
                .collect();
            for mask in 1..1usize << mates.len() {
                let mut group = vec![id];
                group.extend(
                    mates
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| mask & (1 << i) != 0)
                        .map(|(_, m)| m.id),
                );
                group.sort();
                groups.entry(group).or_default().insert(*mid);
            }
        }
        if !scores.is_empty() {
            add(
                &mut out,
                id,
                "挂",
                with_games(
                    "净出兵分 = 出兵分 − 退款分 × 80%\n净出兵分超过 1000 ＋ 5 × 对局秒数时显示。",
                    scores,
                ),
            )
        }
        if remote.len() >= 3 {
            add(
                &mut out,
                id,
                "远",
                with_games(
                    concat!(
                        "疑似透视挂。至少 3 场排位，每场至少 25 分钟。\n",
                        "每场满足以下任一项：\n",
                        "① 折算击杀至少 30，占本人击杀至少 45%。\n",
                        "　 折算击杀 = 炮兵击杀 ＋ 固定翼击杀 × 0.7\n",
                        "② 本人至少 10 杀，炮兵击杀占比超过 40%。\n",
                        "　 同时满足：队友最高炮兵占比不超过 20%，\n",
                        "　 或本人占比超过队友最高值的两倍。",
                    ),
                    remote,
                ),
            )
        }
        if eff.len() >= 5 {
            add(
                &mut out,
                id,
                "效",
                with_games(
                    concat!(
                        "至少 5 场排位，每场同时满足：\n",
                        "① 派出至少 5 个单位。\n",
                        "② 每单位平均击杀至少 0.8 个敌人，\n",
                        "　 且达到同场其余九人平均值的 1.6 倍。",
                    ),
                    eff,
                ),
            )
        }
        if member.enemy && history.first_three_known && jets.len() == 3 {
            air.push((member, jets))
        }
    }
    let mut cars: Vec<_> = groups
        .into_iter()
        .filter(|(g, ms)| ms.len() >= if g.len() == 5 { 2 } else { 3 })
        .collect();
    cars.sort_by_key(|(g, m)| {
        (
            std::cmp::Reverse(g.len()),
            std::cmp::Reverse(m.len()),
            g.clone(),
        )
    });
    for member in members {
        if let Some((g, ms)) = cars.iter().find(|(g, _)| g.contains(&member.id)) {
            let names = members
                .iter()
                .filter(|m| g.contains(&m.id))
                .map(|m| m.name.as_str())
                .collect::<Vec<_>>()
                .join("\n• ");
            let games = ms
                .iter()
                .filter_map(|id| facts.get(id)?.as_ref())
                .map(|f| (f, String::new()))
                .collect();
            add(
                &mut out,
                member.id,
                "车",
                with_games(
                    &format!(
                        "本局同队，且曾整组同队：\n2–4 人：至少 3 场\n5 人：至少 2 场\n\n疑似 {} 人车队 · 整组同队 {} 场\n• {names}",
                        g.len(),
                        ms.len()
                    ),
                    games,
                ),
            );
        }
    }
    for (member, jets) in air {
        if let Some((g, _)) = cars.iter().find(|(g, _)| {
            g.len() >= 3
                && members
                    .iter()
                    .any(|m| m.side == member.side && g.contains(&m.id))
        }) {
            add(
                &mut out,
                member.id,
                "空",
                with_games(
                    &format!(
                        "最近连续 3 场，本人固定翼飞机每场消耗全队超过 40% 的补给。\n本局敌方有疑似 {} 人车队（至少 3 人）。",
                        g.len()
                    ),
                    jets,
                ),
            );
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game() -> Facts {
        let players = (1..=10)
            .map(|n| {
                (
                    PlayerId(n),
                    Stats {
                        team: Some(if n <= 5 { Team::Alpha } else { Team::Bravo }),
                        spawns: 10,
                        kills: if n == 1 { 40 } else { 3 },
                        art: if n == 1 { 32 } else { 0 },
                        class_known: true,
                        score: Some(if n == 1 { 10001.0 } else { 1000.0 }),
                        fuel: if n == 1 { 800 } else { 100 },
                        jet_fuel: if n == 1 { 720 } else { 0 },
                        ..Default::default()
                    },
                )
            })
            .collect();
        Facts {
            players,
            duration: 1800,
            ..Default::default()
        }
    }
    #[test]
    fn evidence_lists_every_hit_in_date_order_without_match_ids() {
        let ids = (810001..=810005).map(MatchId).collect::<Vec<_>>();
        let facts = ids
            .iter()
            .enumerate()
            .map(|(i, id)| {
                let mut f = game();
                f.start = Some(1_750_000_000 + i as i64 * 86_400);
                f.map = Some(format!("地图{i}"));
                (*id, Some(f))
            })
            .collect();
        let members: Vec<_> = (1..=3)
            .map(|n| Member {
                id: PlayerId(n),
                side: Some("enemy".to_owned()),
                enemy: true,
                name: format!("玩家{n}"),
                recent: ids.clone(),
            })
            .collect();
        let histories = members
            .iter()
            .map(|m| {
                (
                    m.id,
                    History {
                        matches: ids.iter().rev().copied().collect(),
                        first_three_known: true,
                    },
                )
            })
            .collect();
        let labels = evaluate(&members, &histories, &facts);
        for b in &labels[&PlayerId(1)] {
            let rows: Vec<_> = b
                .detail
                .lines()
                .filter(|line| line.contains("地图"))
                .collect();
            assert_eq!(rows.len(), if b.symbol == "空" { 3 } else { 5 });
            assert!(rows[0].contains("地图4"));
            assert!(rows[1].contains("地图3"));
            assert!(ids.iter().all(|id| !b.detail.contains(&id.to_string())));
        }
        let game = game();
        assert!(with_games("规则", vec![(&game, String::new())]).contains("日期未知  地图未知"));
    }

    #[test]
    fn thresholds_use_distinct_matches_and_the_historical_team() {
        let ids = (1..=5).map(MatchId).collect::<Vec<_>>();
        let facts = ids.iter().map(|id| (*id, Some(game()))).collect();
        let members = (1..=3)
            .map(|n| Member {
                id: PlayerId(n),
                side: Some("enemy".into()),
                enemy: true,
                recent: ids.clone(),
                name: n.to_string(),
            })
            .collect::<Vec<_>>();
        let mut histories = members
            .iter()
            .map(|m| {
                (
                    m.id,
                    History {
                        matches: ids.clone(),
                        first_three_known: true,
                    },
                )
            })
            .collect();
        let symbols = |out: Labels| {
            out[&PlayerId(1)]
                .iter()
                .map(|b| b.symbol.as_str())
                .collect::<String>()
        };
        assert_eq!(
            symbols(evaluate(&members, &histories, &facts)),
            "挂远效车空"
        );
        histories
            .get_mut(&PlayerId(1))
            .expect("history")
            .matches
            .truncate(4);
        histories
            .get_mut(&PlayerId(1))
            .expect("history")
            .first_three_known = false;
        assert_eq!(symbols(evaluate(&members, &histories, &facts)), "挂远车");
        let mut split = members;
        split[2].side = Some("friendly".into());
        let result = evaluate(&split, &histories, &facts);
        assert!(
            result[&PlayerId(1)]
                .iter()
                .any(|b| b.symbol == "车" && b.detail.contains("2 人"))
        );
        assert!(!result.contains_key(&PlayerId(3)));
    }
    #[test]
    fn crowd_boundary_and_missing_team_stats_suppress_alerts() {
        let mut f = game();
        for id in 2..=5 {
            f.players.get_mut(&PlayerId(id)).expect("player").score = Some(10001.0);
        }
        assert!(f.excess(&f.players[&PlayerId(1)]).is_none());
        let mut f = game();
        let me = f.players.get_mut(&PlayerId(1)).expect("player");
        me.score = Some(10000.0);
        me.kills = 2;
        me.art = 1;
        assert!(f.excess(&f.players[&PlayerId(1)]).is_none());
        assert!(f.remote(PlayerId(1), &f.players[&PlayerId(1)]).is_none());
        let me = f.players.get_mut(&PlayerId(1)).expect("player");
        me.kills = 11;
        me.art = 6;
        assert!(f.remote(PlayerId(1), &f.players[&PlayerId(1)]).is_some());
        f.players.get_mut(&PlayerId(2)).expect("player").class_known = false;
        assert!(f.remote(PlayerId(1), &f.players[&PlayerId(1)]).is_none());
    }
}
