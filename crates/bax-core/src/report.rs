//! 战后战报的客观统计。
//!
//! 输入是一场比赛的战报（已经从接口模型转换过来）和单位目录，输出是界面要画的东西：
//! 双方与每名玩家的官方数据、按分钟的出兵价值、兵种构成、单位明细和每次出兵的时间。
//! 纯函数，不做 IO。只做客观统计，不对玩家下任何判断（ADR-0002）。
//!
//! 口径（依据见验证 4 与 `docs/reference/batrace-api.md`）：
//!
//! - **出兵者。** `UnitData` 是比赛结束时各自名下的单位；转交出去的实例在接收者名下，
//!   转出者的 `TransferredUnits` 里留有同一个键。这里统计“谁出的兵”，转交的实例算回转出者。
//! - **时间。** `SpawnTime`、`DeathTime` 是 Unix 秒，相对开局的时间要减去战报的开始时间。
//! - **回收。** `WasRefunded` 是单位被回收（飞机返航、开局卖掉等），官方全额退款。
//!   每条记录都算一次出兵（飞机就是一个架次），但回收的不计入花费。
//! - **单位成本是估算。** 目录只有基础价格，不含改装选项。每名玩家的实例按基础价格加总后，
//!   等比例缩放到他官方的“出兵价值 − 回收价值”，这样玩家与队伍的总数和官方一致，
//!   单个单位仍然是估算。缩放的做法参考了 ba-dragon（见 NOTICE.md）。
//! - **无效值。** 接口用 -1 表示缺失的伤害，按缺失处理。

use std::collections::{BTreeMap, HashMap};

use serde::Serialize;

use crate::{MatchId, PlayerId, Team, Timestamp};

/// 单位目录里的一项。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UnitKind {
    pub name: Option<String>,
    pub category: Option<i64>,
    pub role: Option<i64>,
    /// 基础价格，不含改装选项。
    pub base_cost: Option<i64>,
}

/// 兵种。由目录的 `category_type` 与 `role` 组合得出，名称是按目录里的单位名推断的。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum UnitClass {
    Recon,
    Infantry,
    Tank,
    /// 步战车与反坦克车辆。
    Ifv,
    /// 运输车与其他车辆。
    Transport,
    Supply,
    AirDefense,
    Artillery,
    Helicopter,
    Jet,
    /// 非作战单位，或者目录里查不到。
    Other,
}

impl UnitClass {
    pub const ALL: [UnitClass; 11] = [
        UnitClass::Recon,
        UnitClass::Infantry,
        UnitClass::Tank,
        UnitClass::Ifv,
        UnitClass::Transport,
        UnitClass::Supply,
        UnitClass::AirDefense,
        UnitClass::Artillery,
        UnitClass::Helicopter,
        UnitClass::Jet,
        UnitClass::Other,
    ];

    /// 按目录的分类与 role 归类。
    pub fn of(category: Option<i64>, role: Option<i64>) -> UnitClass {
        match (category, role) {
            (Some(0), _) => UnitClass::Recon,
            (Some(1), _) => UnitClass::Infantry,
            (Some(2), Some(11)) => UnitClass::Tank,
            (Some(2), Some(10 | 13)) => UnitClass::Ifv,
            (Some(2), _) => UnitClass::Transport,
            (Some(3), Some(14)) => UnitClass::Supply,
            (Some(3), Some(15 | 16)) => UnitClass::AirDefense,
            (Some(3), _) => UnitClass::Artillery,
            (Some(5), _) => UnitClass::Helicopter,
            (Some(6), _) => UnitClass::Jet,
            _ => UnitClass::Other,
        }
    }
}

/// 一场比赛的战报。
#[derive(Debug, Clone, PartialEq)]
pub struct ReportInput {
    pub match_id: MatchId,
    /// 正式开局，Unix 秒。
    pub start_time: Option<i64>,
    /// 比赛结束，Unix 秒。
    pub end_time: Option<i64>,
    pub duration_secs: Option<i64>,
    pub map_id: Option<i64>,
    pub winner: Option<Team>,
    pub alpha_points: Option<i64>,
    pub bravo_points: Option<i64>,
    pub players: Vec<PlayerInput>,
}

/// 战报里的一名玩家。
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerInput {
    pub id: PlayerId,
    pub name: Option<String>,
    pub team: Option<Team>,
    pub old_rating: Option<f64>,
    pub new_rating: Option<f64>,
    pub spawned_score: Option<i64>,
    pub refunded_score: Option<i64>,
    /// 击毁数。
    pub destruction: Option<i64>,
    /// 损失数。
    pub losses: Option<i64>,
    pub destruction_score: Option<i64>,
    pub losses_score: Option<i64>,
    pub damage_dealt: Option<f64>,
    pub damage_received: Option<f64>,
    pub objectives: Option<i64>,
    pub supply_consumed: Option<i64>,
    pub supply_from_allies: Option<i64>,
    pub supply_by_allies: Option<i64>,
    pub supply_captured: Option<i64>,
    pub supply_airdropped: Option<i64>,
    /// 游戏自己的离场标记。
    pub deserter: bool,
    /// 比赛结束时在他名下的单位实例（`UnitData`）。
    pub units: Vec<UnitInput>,
    /// 他转交出去的实例的键（`TransferredUnits`）。
    pub transferred_out: Vec<String>,
}

impl ReportInput {
    /// 只有比赛 ID、其余全部缺失的战报。
    pub fn new(match_id: MatchId) -> ReportInput {
        ReportInput {
            match_id,
            start_time: None,
            end_time: None,
            duration_secs: None,
            map_id: None,
            winner: None,
            alpha_points: None,
            bravo_points: None,
            players: Vec::new(),
        }
    }
}

impl PlayerInput {
    /// 只有玩家 ID、其余全部缺失的一名玩家。
    pub fn new(id: PlayerId) -> PlayerInput {
        PlayerInput {
            id,
            name: None,
            team: None,
            old_rating: None,
            new_rating: None,
            spawned_score: None,
            refunded_score: None,
            destruction: None,
            losses: None,
            destruction_score: None,
            losses_score: None,
            damage_dealt: None,
            damage_received: None,
            objectives: None,
            supply_consumed: None,
            supply_from_allies: None,
            supply_by_allies: None,
            supply_captured: None,
            supply_airdropped: None,
            deserter: false,
            units: Vec::new(),
            transferred_out: Vec::new(),
        }
    }
}

/// 已按战报选项解析的名称与图标。
#[derive(Debug, Clone, PartialEq)]
pub struct UnitAppearance {
    pub name: String,
    pub thumbnail: Option<String>,
}

/// 一个单位实例。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UnitInput {
    pub option_ids: Vec<i64>,
    pub appearance: Option<UnitAppearance>,
    /// 实例在 `UnitData` 里的键，一场比赛内唯一。
    pub key: String,
    /// 单位类型，对应单位目录。
    pub unit_id: i64,
    /// Unix 秒。
    pub spawn_time: Option<i64>,
    /// Unix 秒；活到比赛结束时为 `None`。
    pub death_time: Option<i64>,
    pub kills: Option<i64>,
    pub damage_dealt: Option<f64>,
    pub refunded: bool,
    pub airdropped: bool,
}

/// 一场比赛的统计结果。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct MatchAnalysis {
    pub match_id: MatchId,
    pub start_time: Option<Timestamp>,
    pub duration_secs: Option<i64>,
    pub map_id: Option<i64>,
    pub winner: Option<Team>,
    pub alpha_points: Option<i64>,
    pub bravo_points: Option<i64>,
    /// 时间线的分钟数，至少为 1。
    pub minutes: u32,
    /// 是否每个实例都在单位目录里查到了基础价格。为假时单位成本更不可靠，界面要提示。
    pub catalog_complete: bool,
    /// Alpha 在前。
    pub teams: Vec<TeamReport>,
    /// 按队伍分组，组内按官方出兵价值从高到低。
    pub players: Vec<PlayerReport>,
    /// 每次出兵，按出生时间排序。
    pub sorties: Vec<Sortie>,
    /// 出现过的单位类型 → 名字。
    pub unit_names: BTreeMap<i64, String>,
}

/// 一支队伍的合计，都是官方数据的加总，只有 `value_by_minute` 是估算。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct TeamReport {
    pub team: Team,
    pub players: u32,
    pub won: Option<bool>,
    pub points: Option<i64>,
    /// 赛前 ELO 的平均值，只算有 ELO 的玩家。
    pub avg_rating: Option<f64>,
    pub deployed_value: i64,
    pub destruction_score: i64,
    pub losses_score: i64,
    pub destruction: i64,
    pub losses: i64,
    pub objectives: i64,
    pub supply_consumed: i64,
    /// 每分钟出兵价值（估算，总数与官方一致）。
    pub value_by_minute: Vec<i64>,
}

/// 一名玩家。官方字段原样给出，缺失为 `None`。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct PlayerReport {
    pub id: PlayerId,
    pub name: Option<String>,
    pub team: Option<Team>,
    pub old_rating: Option<f64>,
    pub new_rating: Option<f64>,
    /// 官方的出兵价值减回收价值。
    pub deployed_value: Option<i64>,
    pub destruction_score: Option<i64>,
    pub losses_score: Option<i64>,
    pub destruction: Option<i64>,
    pub losses: Option<i64>,
    pub damage_dealt: Option<f64>,
    pub damage_received: Option<f64>,
    pub objectives: Option<i64>,
    pub supply_consumed: Option<i64>,
    pub supply_from_allies: Option<i64>,
    pub supply_by_allies: Option<i64>,
    pub supply_captured: Option<i64>,
    pub supply_airdropped: Option<i64>,
    pub deserter: bool,
    /// 以下按出兵者统计：出兵次数（含回收的）。
    pub sorties: u32,
    pub refunded: u32,
    pub dead: u32,
    /// 转交给别人的实例数。
    pub transferred_out: u32,
    /// 最后一次出兵距开局的秒数。
    pub last_spawn_secs: Option<i64>,
    /// 每分钟出兵价值（估算）。
    pub value_by_minute: Vec<i64>,
    /// 按兵种，价值从高到低，只列出现过的。
    pub composition: Vec<ClassShare>,
    /// 按单位类型，估算价值从高到低。
    pub units: Vec<UnitRow>,
}

/// 一个兵种在某名玩家出兵里的份额。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ClassShare {
    pub class: UnitClass,
    pub count: u32,
    /// 累计出兵估算价值，包含后来回收的单位。
    pub value: i64,
    /// 单位记录的有效伤害合计，按出兵者归属。
    pub damage: f64,
    /// 已阵亡且未回收单位的估算成本，按出兵者归属。
    pub loss_value: i64,
}

/// 某名玩家出过的一种单位。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct UnitRow {
    pub variant_key: String,
    pub unit_name: String,
    pub thumbnail: Option<String>,
    pub unit_id: i64,
    pub class: UnitClass,
    pub count: u32,
    pub refunded: u32,
    pub dead: u32,
    pub kills: i64,
    pub damage: f64,
    /// 估算价值，不含回收的。
    pub value: i64,
    pub first_spawn_secs: Option<i64>,
}

/// 一次出兵。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Sortie {
    pub variant_key: String,
    pub unit_name: String,
    pub thumbnail: Option<String>,
    /// 出兵者。
    pub player: PlayerId,
    pub unit_id: i64,
    pub class: UnitClass,
    /// 距开局的秒数，不小于 0。
    pub spawn_secs: i64,
    pub death_secs: Option<i64>,
    /// 估算成本。
    pub cost: i64,
    pub refunded: bool,
    pub airdropped: bool,
    /// 转交给了谁。
    pub transferred_to: Option<PlayerId>,
    pub kills: i64,
}

/// 缩放系数的范围。超出范围多半是数据有问题，宁可按基础价格算。
const SCALE_RANGE: (f64, f64) = (0.5, 4.0);

/// 统计一场比赛。
pub fn analyze(input: &ReportInput, catalog: &HashMap<i64, UnitKind>) -> MatchAnalysis {
    // 开局时刻：战报的开始时间；没有时用结束时间倒推；再没有就用最早的出生时间
    let start = input
        .start_time
        .or_else(|| Some(input.end_time? - input.duration_secs?))
        .or_else(|| {
            input
                .players
                .iter()
                .flat_map(|p| &p.units)
                .filter_map(|u| u.spawn_time)
                .min()
        });
    let duration = input
        .duration_secs
        .filter(|d| *d > 0)
        .or_else(|| Some(input.end_time? - start?).filter(|d| *d > 0));
    let offset = |t: Option<i64>| -> Option<i64> { Some((t? - start?).max(0)) };

    // 转交：实例键 → 原来出兵的人
    let mut origin: HashMap<&str, PlayerId> = HashMap::new();
    for p in &input.players {
        for key in &p.transferred_out {
            origin.entry(key.as_str()).or_insert(p.id);
        }
    }
    // 实例所在的人（比赛结束时的持有者）
    let mut holder: HashMap<&str, PlayerId> = HashMap::new();
    for p in &input.players {
        for u in &p.units {
            holder.insert(u.key.as_str(), p.id);
        }
    }

    // 按出兵者收集实例
    let mut by_spawner: BTreeMap<PlayerId, Vec<(&UnitInput, Option<PlayerId>)>> = BTreeMap::new();
    for p in &input.players {
        for u in &p.units {
            match origin.get(u.key.as_str()) {
                Some(&from) if from != p.id => {
                    by_spawner.entry(from).or_default().push((u, Some(p.id)));
                }
                _ => by_spawner.entry(p.id).or_default().push((u, None)),
            }
        }
    }

    let max_offset = input
        .players
        .iter()
        .flat_map(|p| &p.units)
        .flat_map(|u| [offset(u.spawn_time), offset(u.death_time)])
        .flatten()
        .max()
        .unwrap_or(0);
    let span = duration.unwrap_or(max_offset).max(max_offset).max(1);
    let minutes = span.div_euclid(60) as u32 + u32::from(span % 60 != 0);
    let minutes = minutes.max(1);
    let minute_of = |secs: i64| -> usize { (secs / 60).clamp(0, i64::from(minutes) - 1) as usize };

    let mut catalog_complete = true;
    let mut unit_names = BTreeMap::new();
    let mut sorties = Vec::new();
    let mut players = Vec::with_capacity(input.players.len());

    for p in &input.players {
        let own: &[(&UnitInput, Option<PlayerId>)] =
            by_spawner.get(&p.id).map(Vec::as_slice).unwrap_or(&[]);
        let base = |u: &UnitInput| -> i64 {
            catalog
                .get(&u.unit_id)
                .and_then(|k| k.base_cost)
                .unwrap_or(0)
                .max(0)
        };
        let base_total: i64 = own
            .iter()
            .filter(|(u, _)| !u.refunded)
            .map(|(u, _)| base(u))
            .sum();
        let official = deployed_value(p);
        let scale = match official {
            Some(v) if v > 0 && base_total > 0 => {
                let k = v as f64 / base_total as f64;
                if (SCALE_RANGE.0..=SCALE_RANGE.1).contains(&k) {
                    k
                } else {
                    1.0
                }
            }
            _ => 1.0,
        };

        let mut value_by_minute = vec![0i64; minutes as usize];
        let mut classes: BTreeMap<UnitClass, ClassShare> = BTreeMap::new();
        let mut units: BTreeMap<String, UnitRow> = BTreeMap::new();
        let (mut refunded, mut dead, mut transferred) = (0u32, 0u32, 0u32);
        let mut last_spawn = None;

        for &(u, transferred_to) in own {
            let kind = catalog.get(&u.unit_id);
            if kind.and_then(|k| k.base_cost).is_none() {
                catalog_complete = false;
            }
            let class = kind.map_or(UnitClass::Other, |k| UnitClass::of(k.category, k.role));
            unit_names.entry(u.unit_id).or_insert_with(|| {
                kind.and_then(|k| k.name.clone())
                    .unwrap_or_else(|| format!("单位 #{}", u.unit_id))
            });
            let mut options = u.option_ids.clone();
            options.sort_unstable();
            options.dedup();
            let variant_key = format!("{}:{options:?}", u.unit_id);
            let unit_name = u
                .appearance
                .as_ref()
                .map(|a| a.name.clone())
                .unwrap_or_else(|| unit_names[&u.unit_id].clone());
            let thumbnail = u.appearance.as_ref().and_then(|a| a.thumbnail.clone());
            let cost = (base(u) as f64 * scale).round() as i64;
            let spawn_secs = offset(u.spawn_time);
            let death_secs = offset(u.death_time);
            let kills = u.kills.unwrap_or(0).max(0);
            let damage = u.damage_dealt.filter(|d| *d >= 0.0).unwrap_or(0.0);

            if u.refunded {
                refunded += 1;
            }
            if u.death_time.is_some() {
                dead += 1;
            }
            if transferred_to.is_some() {
                transferred += 1;
            }
            let value = if u.refunded { 0 } else { cost };
            if let Some(s) = spawn_secs {
                value_by_minute[minute_of(s)] += value;
                last_spawn = last_spawn.max(Some(s));
            }
            let entry = classes.entry(class).or_insert(ClassShare {
                class,
                count: 0,
                value: 0,
                damage: 0.0,
                loss_value: 0,
            });
            entry.count += 1;
            entry.value += cost;
            entry.damage += damage;
            if u.death_time.is_some() && !u.refunded {
                entry.loss_value += cost;
            }

            let row = units.entry(variant_key.clone()).or_insert(UnitRow {
                variant_key: variant_key.clone(),
                unit_name: unit_name.clone(),
                thumbnail: thumbnail.clone(),
                unit_id: u.unit_id,
                class,
                count: 0,
                refunded: 0,
                dead: 0,
                kills: 0,
                damage: 0.0,
                value: 0,
                first_spawn_secs: None,
            });
            row.count += 1;
            row.refunded += u32::from(u.refunded);
            row.dead += u32::from(u.death_time.is_some());
            row.kills += kills;
            row.damage += damage;
            row.value += value;
            row.first_spawn_secs = match (row.first_spawn_secs, spawn_secs) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            };

            if let Some(spawn_secs) = spawn_secs {
                sorties.push(Sortie {
                    variant_key,
                    unit_name,
                    thumbnail,
                    player: p.id,
                    unit_id: u.unit_id,
                    class,
                    spawn_secs,
                    death_secs,
                    cost,
                    refunded: u.refunded,
                    airdropped: u.airdropped,
                    transferred_to,
                    kills,
                });
            }
        }

        let mut composition: Vec<ClassShare> = classes.into_values().collect();
        composition.sort_by(|a, b| b.value.cmp(&a.value).then(b.count.cmp(&a.count)));
        let mut units: Vec<UnitRow> = units.into_values().collect();
        units.sort_by(|a, b| b.value.cmp(&a.value).then(b.count.cmp(&a.count)));

        players.push(PlayerReport {
            id: p.id,
            name: p.name.clone().filter(|n| !n.trim().is_empty()),
            team: p.team,
            old_rating: p.old_rating,
            new_rating: p.new_rating,
            deployed_value: official,
            destruction_score: p.destruction_score,
            losses_score: p.losses_score,
            destruction: p.destruction,
            losses: p.losses,
            damage_dealt: p.damage_dealt.filter(|d| *d >= 0.0),
            damage_received: p.damage_received.filter(|d| *d >= 0.0),
            objectives: p.objectives,
            supply_consumed: p.supply_consumed,
            supply_from_allies: p.supply_from_allies,
            supply_by_allies: p.supply_by_allies,
            supply_captured: p.supply_captured,
            supply_airdropped: p.supply_airdropped,
            deserter: p.deserter,
            sorties: own.len() as u32,
            refunded,
            dead,
            transferred_out: transferred,
            last_spawn_secs: last_spawn,
            value_by_minute,
            composition,
            units,
        });
    }

    let team_order = |t: Option<Team>| match t {
        Some(Team::Alpha) => 0,
        Some(Team::Bravo) => 1,
        _ => 2,
    };
    players.sort_by(|a, b| {
        team_order(a.team)
            .cmp(&team_order(b.team))
            .then(
                b.deployed_value
                    .unwrap_or(0)
                    .cmp(&a.deployed_value.unwrap_or(0)),
            )
            .then(a.id.cmp(&b.id))
    });
    sorties.sort_by(|a, b| {
        a.spawn_secs
            .cmp(&b.spawn_secs)
            .then(a.player.cmp(&b.player))
    });

    let teams = [Team::Alpha, Team::Bravo]
        .into_iter()
        .map(|team| team_report(team, input, &players, minutes))
        .collect();

    MatchAnalysis {
        match_id: input.match_id,
        start_time: start.map(Timestamp::from_secs),
        duration_secs: duration,
        map_id: input.map_id,
        winner: input.winner,
        alpha_points: input.alpha_points,
        bravo_points: input.bravo_points,
        minutes,
        catalog_complete,
        teams,
        players,
        sorties,
        unit_names,
    }
}

/// 官方的出兵价值减回收价值。没有出兵价值时为 `None`。
fn deployed_value(p: &PlayerInput) -> Option<i64> {
    let spawned = p.spawned_score?;
    Some((spawned - p.refunded_score.unwrap_or(0)).max(0))
}

fn team_report(
    team: Team,
    input: &ReportInput,
    players: &[PlayerReport],
    minutes: u32,
) -> TeamReport {
    let members: Vec<&PlayerReport> = players.iter().filter(|p| p.team == Some(team)).collect();
    let sum = |f: fn(&PlayerReport) -> Option<i64>| members.iter().filter_map(|p| f(p)).sum();
    let ratings: Vec<f64> = members.iter().filter_map(|p| p.old_rating).collect();
    let mut value_by_minute = vec![0i64; minutes as usize];
    for p in &members {
        for (slot, v) in value_by_minute.iter_mut().zip(&p.value_by_minute) {
            *slot += v;
        }
    }
    TeamReport {
        team,
        players: members.len() as u32,
        won: input.winner.map(|w| w == team),
        points: match team {
            Team::Alpha => input.alpha_points,
            Team::Bravo => input.bravo_points,
            Team::Spectators => None,
        },
        avg_rating: (!ratings.is_empty())
            .then(|| ratings.iter().sum::<f64>() / ratings.len() as f64),
        deployed_value: sum(|p| p.deployed_value),
        destruction_score: sum(|p| p.destruction_score),
        losses_score: sum(|p| p.losses_score),
        destruction: sum(|p| p.destruction),
        losses: sum(|p| p.losses),
        objectives: sum(|p| p.objectives),
        supply_consumed: sum(|p| p.supply_consumed),
        value_by_minute,
    }
}

#[cfg(test)]
mod tests;
