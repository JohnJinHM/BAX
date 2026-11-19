use super::*;

const START: i64 = 1_800_000_000;

#[test]
fn variants_stay_separate_and_transferred_units_keep_their_appearance() {
    let base = unit("base", 1, 10);
    let mut modified = unit("modified", 1, 20);
    modified.option_ids = vec![20, 10];
    modified.appearance = Some(UnitAppearance {
        name: "Tank A upgraded".into(),
        thumbnail: Some("tank.webp".into()),
    });
    let mut same = modified.clone();
    same.key = "same".into();
    same.option_ids = vec![10, 20, 20];
    let mut sender = player(1, Team::Alpha, vec![base, same]);
    sender.transferred_out.push("modified".into());
    let analysis = analyze(
        &input(vec![sender, player(2, Team::Alpha, vec![modified])]),
        &catalog(),
    );
    let sender = find(&analysis, 1);
    assert_eq!(sender.units.len(), 2);
    assert_eq!(
        sender
            .units
            .iter()
            .find(|u| u.unit_name == "Tank A upgraded")
            .expect("改型")
            .count,
        2
    );
    let transferred = analysis
        .sorties
        .iter()
        .find(|s| s.transferred_to.is_some())
        .expect("转交");
    assert_eq!(transferred.player, PlayerId(1));
    assert_eq!(transferred.unit_id, 1);
    assert_eq!(transferred.unit_name, "Tank A upgraded");
    assert_eq!(transferred.thumbnail.as_deref(), Some("tank.webp"));
}

fn kind(name: &str, category: i64, role: i64, cost: i64) -> UnitKind {
    UnitKind {
        name: Some(name.to_owned()),
        category: Some(category),
        role: Some(role),
        base_cost: Some(cost),
    }
}

fn catalog() -> HashMap<i64, UnitKind> {
    HashMap::from([
        (1, kind("Tank A", 2, 11, 200)),
        (2, kind("Rifles", 1, 30, 100)),
        (3, kind("Jet B", 6, 160, 300)),
        (4, kind("Supply C", 3, 14, 50)),
    ])
}

fn unit(key: &str, unit_id: i64, spawn: i64) -> UnitInput {
    UnitInput {
        key: key.to_owned(),
        unit_id,
        spawn_time: Some(START + spawn),
        ..UnitInput::default()
    }
}

fn player(id: u64, team: Team, units: Vec<UnitInput>) -> PlayerInput {
    PlayerInput {
        name: Some(format!("P{id}")),
        team: Some(team),
        units,
        ..PlayerInput::new(PlayerId(id))
    }
}

fn input(players: Vec<PlayerInput>) -> ReportInput {
    ReportInput {
        start_time: Some(START),
        end_time: Some(START + 600),
        duration_secs: Some(600),
        winner: Some(Team::Bravo),
        alpha_points: Some(1200),
        bravo_points: Some(1800),
        players,
        ..ReportInput::new(MatchId(9_000_001))
    }
}

fn find(analysis: &MatchAnalysis, id: u64) -> &PlayerReport {
    analysis
        .players
        .iter()
        .find(|p| p.id == PlayerId(id))
        .expect("玩家")
}

#[test]
fn classes_follow_category_and_role() {
    assert_eq!(UnitClass::of(Some(2), Some(11)), UnitClass::Tank);
    assert_eq!(UnitClass::of(Some(2), Some(10)), UnitClass::Ifv);
    assert_eq!(UnitClass::of(Some(2), Some(12)), UnitClass::Transport);
    assert_eq!(UnitClass::of(Some(3), Some(14)), UnitClass::Supply);
    assert_eq!(UnitClass::of(Some(3), Some(16)), UnitClass::AirDefense);
    assert_eq!(UnitClass::of(Some(3), Some(130)), UnitClass::Artillery);
    assert_eq!(UnitClass::of(Some(0), Some(100)), UnitClass::Recon);
    assert_eq!(UnitClass::of(Some(4), None), UnitClass::Other);
    assert_eq!(UnitClass::of(None, None), UnitClass::Other);
}

#[test]
fn transferred_units_count_for_the_player_who_deployed_them() {
    let mut a = player(1, Team::Alpha, vec![unit("10", 1, 5)]);
    a.transferred_out = vec!["11".to_owned()];
    // 11 是 A 出的，转交给了 B，比赛结束时在 B 名下
    let b = player(2, Team::Alpha, vec![unit("11", 4, 30), unit("12", 2, 70)]);
    let analysis = analyze(&input(vec![a, b]), &catalog());

    let a = find(&analysis, 1);
    assert_eq!(a.sorties, 2);
    assert_eq!(a.transferred_out, 1);
    let b = find(&analysis, 2);
    assert_eq!(b.sorties, 1);
    assert_eq!(b.units.len(), 1);
    assert_eq!(b.units[0].unit_id, 2);

    let moved = analysis
        .sorties
        .iter()
        .find(|s| s.unit_id == 4)
        .expect("转交的单位");
    assert_eq!(moved.player, PlayerId(1));
    assert_eq!(moved.transferred_to, Some(PlayerId(2)));
}

#[test]
fn unit_costs_are_scaled_to_the_official_total() {
    let mut refunded = unit("3", 3, 200);
    refunded.refunded = true;
    let mut p = player(
        1,
        Team::Alpha,
        vec![unit("1", 1, 3), unit("2", 2, 3), refunded],
    );
    // 基础价格合计 300（回收的不算），官方 450 − 回收 50 = 400
    p.spawned_score = Some(450);
    p.refunded_score = Some(50);
    let analysis = analyze(&input(vec![p]), &catalog());
    let p = find(&analysis, 1);

    assert_eq!(p.deployed_value, Some(400));
    let total: i64 = p.value_by_minute.iter().sum();
    assert!((total - 400).abs() <= 1, "估算总数应与官方一致：{total}");
    assert_eq!(p.sorties, 3);
    assert_eq!(p.refunded, 1);
    let tank = p.units.iter().find(|u| u.unit_id == 1).expect("坦克");
    assert_eq!(tank.value, 267);
    let jet = p.units.iter().find(|u| u.unit_id == 3).expect("飞机");
    assert_eq!(jet.value, 0, "回收的不计入花费");
    assert_eq!(jet.refunded, 1);
}

#[test]
fn implausible_scale_falls_back_to_base_prices() {
    let mut p = player(1, Team::Alpha, vec![unit("1", 2, 3)]);
    p.spawned_score = Some(100_000);
    let analysis = analyze(&input(vec![p]), &catalog());
    assert_eq!(find(&analysis, 1).units[0].value, 100);
}

#[test]
fn times_are_relative_to_the_start_and_bucketed_by_minute() {
    let mut early = unit("1", 2, 3);
    early.death_time = Some(START + 95);
    let late = unit("2", 1, 125);
    let analysis = analyze(
        &input(vec![player(1, Team::Alpha, vec![early, late])]),
        &catalog(),
    );

    assert_eq!(analysis.minutes, 10);
    let p = find(&analysis, 1);
    assert_eq!(p.value_by_minute[0], 100);
    assert_eq!(p.value_by_minute[2], 200);
    assert_eq!(p.dead, 1);
    assert_eq!(p.last_spawn_secs, Some(125));
    assert_eq!(analysis.sorties[0].spawn_secs, 3);
    assert_eq!(analysis.sorties[0].death_secs, Some(95));
}

#[test]
fn start_falls_back_to_end_minus_duration() {
    let mut data = input(vec![player(1, Team::Alpha, vec![unit("1", 2, 60)])]);
    data.start_time = None;
    let analysis = analyze(&data, &catalog());
    assert_eq!(analysis.start_time, Some(Timestamp::from_secs(START)));
    assert_eq!(analysis.sorties[0].spawn_secs, 60);
}

#[test]
fn invalid_and_unknown_values_are_treated_as_missing() {
    let mut odd = unit("1", 999, 10);
    odd.damage_dealt = Some(-1.0);
    let mut p = player(1, Team::Alpha, vec![odd]);
    p.damage_received = Some(-1.0);
    let analysis = analyze(&input(vec![p]), &catalog());

    assert!(!analysis.catalog_complete);
    assert_eq!(
        analysis.unit_names.get(&999).map(String::as_str),
        Some("单位 #999")
    );
    let p = find(&analysis, 1);
    assert_eq!(p.damage_received, None);
    assert_eq!(p.units[0].damage, 0.0);
    assert_eq!(p.units[0].class, UnitClass::Other);
}

#[test]
fn team_totals_add_up_official_numbers() {
    let mut a1 = player(1, Team::Alpha, vec![]);
    a1.destruction_score = Some(1000);
    a1.old_rating = Some(2000.0);
    let mut a2 = player(2, Team::Alpha, vec![]);
    a2.destruction_score = Some(500);
    a2.old_rating = Some(2100.0);
    let b1 = player(3, Team::Bravo, vec![]);
    let analysis = analyze(&input(vec![a1, b1, a2]), &catalog());

    let alpha = &analysis.teams[0];
    assert_eq!(alpha.team, Team::Alpha);
    assert_eq!(alpha.players, 2);
    assert_eq!(alpha.destruction_score, 1500);
    assert_eq!(alpha.avg_rating, Some(2050.0));
    assert_eq!(alpha.won, Some(false));
    assert_eq!(alpha.points, Some(1200));
    assert_eq!(analysis.teams[1].won, Some(true));
    // 先 Alpha 后 Bravo
    assert_eq!(analysis.players[2].id, PlayerId(3));
}

#[test]
fn composition_is_sorted_by_value() {
    let p = player(
        1,
        Team::Alpha,
        vec![unit("1", 2, 3), unit("2", 2, 3), unit("3", 1, 3)],
    );
    let analysis = analyze(&input(vec![p]), &catalog());
    let p = find(&analysis, 1);
    assert_eq!(p.composition[0].class, UnitClass::Infantry);
    assert_eq!(p.composition[0].value, 200);
    assert_eq!(p.composition[0].count, 2);
    assert_eq!(p.composition[1].class, UnitClass::Tank);
}
