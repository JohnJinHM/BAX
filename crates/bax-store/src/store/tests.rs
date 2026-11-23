use bax_core::{LogEvent, RoomKind, RoomStatus, RosterEntry, Tracker, TrackerConfig};
use serde::{Deserialize, Serialize};

use super::*;

fn t(secs: i64) -> Timestamp {
    Timestamp::from_secs(secs)
}

fn entry(id: u64, name: &str, team: Team) -> RosterEntry {
    RosterEntry {
        id: PlayerId(id),
        name: (!name.is_empty()).then(|| name.to_owned()),
        team,
    }
}

const ME: u64 = 500;

fn row(id: u64, team: Option<Team>, name: Option<&str>) -> MatchPlayerRow {
    MatchPlayerRow {
        player: PlayerId(id),
        team,
        log_name: name.map(str::to_owned),
    }
}

/// 用对局跟踪器走一遍正常开局，得到一个真实形状的比赛状态。
fn played(match_id: u64, start: i64, roster: Vec<RosterEntry>) -> Tracker {
    let id = MatchId(match_id);
    let mut tr = Tracker::new(TrackerConfig::default());
    tr.apply(
        t(start),
        &LogEvent::GameVersion {
            version: "1.2.2.1".into(),
        },
    );
    tr.apply(t(start), &LogEvent::Persona { name: "Me".into() });
    tr.apply(
        t(start),
        &LogEvent::DeckSelected {
            name: Some("my deck".into()),
        },
    );
    tr.apply(
        t(start),
        &LogEvent::RoomEntered {
            room: RoomKind::WaitingRoom,
            match_id: id,
            status: Some(RoomStatus::WaitClients),
        },
    );
    tr.apply(
        t(start + 30),
        &LogEvent::LoadingStarted {
            map: "Airport".into(),
            scenario: Some("Airport_network".into()),
        },
    );
    tr.apply(
        t(start + 40),
        &LogEvent::RoomEntered {
            room: RoomKind::GameRoom,
            match_id: id,
            status: Some(RoomStatus::WaitClients),
        },
    );
    tr.apply(t(start + 40), &LogEvent::PlayerList { players: roster });
    tr.apply(t(start + 70), &LogEvent::ScenarioLoaded);
    tr
}

fn standard_roster() -> Vec<RosterEntry> {
    vec![
        entry(101, "Ash", Team::Alpha),
        entry(102, "", Team::Alpha),
        entry(201, "Dune", Team::Bravo),
        entry(ME, "Me", Team::Bravo),
    ]
}

fn state(tr: &Tracker) -> &MatchState {
    tr.current().expect("应当有当前比赛")
}

fn profile(id: u64, name: &str, rating: f64) -> ProfileUpdate {
    ProfileUpdate {
        id: PlayerId(id),
        name: Some(name.to_owned()),
        steam_id: Some(format!("7656119{id:010}")),
        level: Some(40),
        rating: Some(rating),
        rank: Some(1000),
        rating_games: Some(200),
        raw_json: format!("{{\"info\":{{\"id\":{id},\"rating\":{rating}}}}}"),
    }
}

#[test]
fn a_new_database_is_at_the_latest_schema_version() {
    let store = Store::open_in_memory().expect("打开长期库");
    assert_eq!(
        store.schema_version().expect("读取版本"),
        MIGRATIONS.len() as u32
    );
}

#[test]
fn profiles_round_trip() {
    let mut store = Store::open_in_memory().expect("打开长期库");
    assert_eq!(store.player(PlayerId(101)).expect("读取"), None);

    store
        .save_profile(&profile(101, "Ash", 2100.5), t(1000))
        .expect("写入");
    let got = store.player(PlayerId(101)).expect("读取").expect("有记录");
    assert_eq!(got.name.as_deref(), Some("Ash"));
    assert_eq!(got.rating, Some(2100.5));
    assert_eq!(got.level, Some(40));
    assert_eq!(got.fetched_at, Some(t(1000)));
    assert!(
        got.profile_json
            .as_deref()
            .is_some_and(|j| j.contains("2100.5"))
    );
    assert_eq!(got.first_seen_at, None, "只查过资料、没在比赛里遇到过");
    assert_eq!(got.display_name(), Some("Ash"));
}

#[test]
fn a_refresh_replaces_external_data_but_an_older_fetch_does_not() {
    let mut store = Store::open_in_memory().expect("打开长期库");
    store
        .save_profile(&profile(101, "Ash", 2100.0), t(1000))
        .expect("写入");
    store
        .save_profile(&profile(101, "Ash Renamed", 2150.0), t(2000))
        .expect("刷新");
    // 一个迟到的旧请求结果
    store
        .save_profile(&profile(101, "Stale", 1900.0), t(1500))
        .expect("旧结果");

    let got = store.player(PlayerId(101)).expect("读取").expect("有记录");
    assert_eq!(got.name.as_deref(), Some("Ash Renamed"));
    assert_eq!(got.rating, Some(2150.0));
    assert_eq!(got.fetched_at, Some(t(2000)));
}

#[test]
fn rating_history_records_changes_only() {
    let mut store = Store::open_in_memory().expect("打开长期库");
    store
        .save_profile(&profile(101, "Ash", 2100.0), t(1000))
        .expect("写入");
    store
        .save_profile(&profile(101, "Ash", 2100.0), t(2000))
        .expect("数值没变");
    store
        .save_profile(&profile(101, "Ash", 2112.5), t(3000))
        .expect("赢了一场");
    store
        .save_profile(&profile(101, "Ash", 1900.0), t(2500))
        .expect("迟到的旧结果");

    let history = store.rating_history(PlayerId(101)).expect("读取");
    assert_eq!(
        history.iter().map(|p| (p.at, p.rating)).collect::<Vec<_>>(),
        vec![(t(1000), Some(2100.0)), (t(3000), Some(2112.5))]
    );

    // 资料里什么数值都没有时不记历史
    let empty = ProfileUpdate {
        id: PlayerId(7),
        name: None,
        steam_id: None,
        level: None,
        rating: None,
        rank: None,
        rating_games: None,
        raw_json: "{}".into(),
    };
    store.save_profile(&empty, t(10)).expect("写入");
    assert!(store.rating_history(PlayerId(7)).expect("读取").is_empty());
}

#[test]
fn partial_profile_refresh_preserves_missing_fields_and_merges_statistics() {
    let mut store = Store::open_in_memory().expect("store");
    let mut full = profile(101, "Ash", 2100.0);
    full.raw_json = r#"{"info":{"id":101,"name":"Ash","rating":2100},"statInfo":{"winCountRt":12,"specExp":{"12":100,"15":200},"futureField":true},"last_fights_data":["900001"]}"#.into();
    store.save_profile(&full, t(1000)).expect("full");
    let mut partial = ProfileUpdate {
        id: PlayerId(101),
        name: None,
        steam_id: None,
        level: None,
        rating: Some(2120.0),
        rank: None,
        rating_games: None,
        raw_json: r#"{"info":{"id":101,"name":"  ","rating":2120},"statInfo":null}"#.into(),
    };
    store.save_profile(&partial, t(2000)).expect("partial");
    let saved = store.player(PlayerId(101)).expect("read").expect("exists");
    assert_eq!(saved.name.as_deref(), Some("Ash"));
    assert_eq!(saved.level, Some(40));
    assert_eq!(saved.rating, Some(2120.0));
    let raw: serde_json::Value =
        serde_json::from_str(saved.profile_json.as_deref().expect("raw")).expect("json");
    assert_eq!(raw["statInfo"]["specExp"]["15"], 200);
    assert_eq!(raw["statInfo"]["futureField"], true);
    assert_eq!(raw["last_fights_data"][0], "900001");
    assert_eq!(raw["info"]["level"], 40);
    partial.rating = None;
    partial.raw_json = r#"{"statInfo":{"winCountRt":0,"specExp":{"12":300},"futureField":false},"last_fights_data":[]}"#.into();
    store
        .save_profile(&partial, t(3000))
        .expect("update values");
    let saved = store.player(PlayerId(101)).expect("read").expect("exists");
    let raw: serde_json::Value =
        serde_json::from_str(saved.profile_json.as_deref().expect("raw")).expect("json");
    assert_eq!(raw["statInfo"]["winCountRt"], 0);
    assert_eq!(raw["statInfo"]["specExp"]["12"], 300);
    assert_eq!(raw["statInfo"]["specExp"]["15"], 200);
    assert_eq!(raw["statInfo"]["futureField"], false);
    assert_eq!(raw["last_fights_data"], serde_json::json!([]));
    assert_eq!(
        store.rating_history(PlayerId(101)).expect("history").len(),
        2
    );
    store.save_profile(&full, t(1500)).expect("late ignored");
    assert_eq!(
        store.player(PlayerId(101)).expect("read").expect("exists"),
        saved
    );
    partial.raw_json = "not json".into();
    assert!(store.save_profile(&partial, t(4000)).is_err());
    assert_eq!(
        store.player(PlayerId(101)).expect("read").expect("exists"),
        saved
    );
}

#[test]
fn recording_a_match_stores_its_details_roster_and_sightings() {
    let mut store = Store::open_in_memory().expect("打开长期库");
    let tr = played(9_000_001, 1000, standard_roster());
    store.record_match(state(&tr)).expect("记录比赛");

    let m = store
        .match_record(MatchId(9_000_001))
        .expect("读取")
        .expect("有记录");
    assert_eq!(m.discovered_at, t(1000));
    assert_eq!(m.phase, Phase::Deployment);
    assert!(!m.rejoined);
    assert_eq!(m.map.as_deref(), Some("Airport"));
    assert_eq!(m.scenario.as_deref(), Some("Airport_network"));
    assert_eq!(m.deck.as_deref(), Some("my deck"));
    assert_eq!(m.game_version.as_deref(), Some("1.2.2.1"));
    assert_eq!(m.self_id, Some(PlayerId(ME)));
    assert_eq!(m.self_team, Some(Team::Bravo));
    assert_eq!(
        m.clock,
        Some(BattleClock {
            start_at: t(1130),
            source: ClockSource::LogEstimate
        })
    );
    assert_eq!(m.scenario_loaded_at, Some(t(1070)));
    assert_eq!(m.ended_at, None);

    let players = store.match_players(MatchId(9_000_001)).expect("读取名单");
    assert_eq!(
        players,
        vec![
            row(101, Some(Team::Alpha), Some("Ash")),
            row(102, Some(Team::Alpha), None),
            row(201, Some(Team::Bravo), Some("Dune")),
            row(ME, Some(Team::Bravo), Some("Me")),
        ],
        "保持日志里的顺序，空名字存为空值"
    );

    let ash = store.player(PlayerId(101)).expect("读取").expect("有记录");
    assert_eq!(ash.last_log_name.as_deref(), Some("Ash"));
    assert_eq!(ash.first_seen_at, Some(t(1000)));
    assert_eq!(ash.last_seen_at, Some(t(1000)));
    assert_eq!(ash.fetched_at, None, "只是遇到过，还没有接口资料");
    assert_eq!(ash.display_name(), Some("Ash"));

    let nameless = store.player(PlayerId(102)).expect("读取").expect("有记录");
    assert_eq!(nameless.display_name(), None);
}

#[test]
fn recording_the_same_state_again_changes_nothing() {
    let mut store = Store::open_in_memory().expect("打开长期库");
    let tr = played(9_000_001, 1000, standard_roster());
    store.record_match(state(&tr)).expect("第一次");
    let before = (
        store.match_record(MatchId(9_000_001)).expect("读取"),
        store.match_players(MatchId(9_000_001)).expect("读取"),
        store.player(PlayerId(101)).expect("读取"),
    );
    for _ in 0..3 {
        store.record_match(state(&tr)).expect("重复记录");
    }
    let after = (
        store.match_record(MatchId(9_000_001)).expect("读取"),
        store.match_players(MatchId(9_000_001)).expect("读取"),
        store.player(PlayerId(101)).expect("读取"),
    );
    assert_eq!(before, after);
    assert_eq!(store.recent_matches(10).expect("读取").len(), 1);
}

#[test]
fn a_less_complete_state_does_not_erase_what_is_known() {
    let mut store = Store::open_in_memory().expect("打开长期库");
    let mut tr = played(9_000_001, 1000, standard_roster());
    tr.advance(t(1200));
    store.record_match(state(&tr)).expect("记录比赛");

    // 游戏崩溃后重启并重连：新进程里的跟踪器不知道地图以外的任何事
    let id = MatchId(9_000_001);
    let mut rejoin = Tracker::default();
    rejoin.apply(t(1500), &LogEvent::ReconnectStarted);
    rejoin.apply(
        t(1501),
        &LogEvent::RoomEntered {
            room: RoomKind::WaitingRoom,
            match_id: id,
            status: Some(RoomStatus::InProcess),
        },
    );
    store.record_match(state(&rejoin)).expect("记录重连");

    let m = store.match_record(id).expect("读取").expect("有记录");
    assert_eq!(m.discovered_at, t(1000), "保留最早的发现时刻");
    assert!(m.rejoined);
    assert_eq!(m.phase, Phase::InBattle);
    assert_eq!(m.deck.as_deref(), Some("my deck"));
    assert_eq!(m.map.as_deref(), Some("Airport"));
    assert_eq!(m.self_team, Some(Team::Bravo));
    assert_eq!(
        m.clock.map(|c| c.start_at),
        Some(t(1130)),
        "计时基准没有被空值覆盖，重连后可以恢复"
    );
    assert_eq!(m.scenario_loaded_at, Some(t(1070)));
    assert_eq!(
        store.match_players(id).expect("读取").len(),
        4,
        "没有名单的状态不会清掉已有名单"
    );
}

#[test]
fn the_end_of_a_match_is_recorded_and_kept() {
    let mut store = Store::open_in_memory().expect("打开长期库");
    let id = MatchId(9_000_001);
    let mut tr = played(9_000_001, 1000, standard_roster());
    tr.apply(t(2300), &LogEvent::MatchEnded { match_id: id });
    store.record_match(state(&tr)).expect("记录结束");
    tr.apply(
        t(2330),
        &LogEvent::RoomExited {
            room: RoomKind::GameRoom,
            match_id: Some(id),
            cause: Some(ExitCause::ServerExitInstance),
            status: Some(RoomStatus::End),
        },
    );
    store.record_match(state(&tr)).expect("记录离开");

    let m = store.match_record(id).expect("读取").expect("有记录");
    assert_eq!(m.phase, Phase::Ended);
    assert_eq!(m.ended_at, Some(t(2300)));
    assert_eq!(m.end_cause, Some(ExitCause::MatchEnded));
}

#[test]
fn a_partial_rejoin_preserves_confirmed_encounters_in_order() {
    let mut store = Store::open_in_memory().expect("长期库");
    let original = played(1, 1000, standard_roster());
    store.record_match(state(&original)).expect("正式名单");
    let mut rejoin = state(&original).clone();
    rejoin.rejoined = true;
    rejoin.roster = vec![
        entry(201, "", Team::Bravo),
        entry(301, "Birch", Team::Spectators),
    ];
    for _ in 0..2 {
        store.record_match(&rejoin).expect("合并重连名单");
    }
    let rows = store.match_players(MatchId(1)).expect("名单");
    assert_eq!(
        rows.iter().map(|r| r.player.get()).collect::<Vec<_>>(),
        [101, 102, 201, ME, 301]
    );
    assert_eq!(rows[2].log_name.as_deref(), Some("Dune"));
    assert_eq!(
        store
            .encounters(PlayerId(101), None)
            .expect("相遇")
            .as_opponent,
        1
    );
}

#[test]
fn a_complete_log_imported_after_a_partial_roster_restores_the_original_order() {
    let mut store = Store::open_in_memory().expect("长期库");
    let full = played(1, 1000, standard_roster());
    let mut partial = state(&full).clone();
    partial.rejoined = true;
    partial.roster.drain(..2);
    store.record_match(&partial).expect("先导入重连");
    store.record_match(state(&full)).expect("后导入完整日志");
    store.record_match(&partial).expect("再次导入重连");
    let rows = store.match_players(MatchId(1)).expect("名单");
    assert_eq!(
        rows.iter().map(|r| r.player.get()).collect::<Vec<_>>(),
        [101, 102, 201, ME]
    );
}

#[test]
fn early_players_are_replaced_by_the_full_roster() {
    let mut store = Store::open_in_memory().expect("打开长期库");
    let id = MatchId(9_000_001);
    let mut tr = Tracker::default();
    tr.apply(
        t(1000),
        &LogEvent::RoomEntered {
            room: RoomKind::WaitingRoom,
            match_id: id,
            status: Some(RoomStatus::WaitClients),
        },
    );
    for (pid, name) in [(101, Some("Ash")), (999, Some("Dodger")), (201, None)] {
        tr.apply(
            t(1010),
            &LogEvent::RoomClient {
                room: RoomKind::WaitingRoom,
                player: PlayerId(pid),
                name: name.map(str::to_owned),
            },
        );
    }
    let early = state(&tr).clone();
    store.record_match(&early).expect("记录等待室");
    assert_eq!(
        store.match_players(id).expect("读取"),
        vec![
            row(101, None, Some("Ash")),
            row(999, None, Some("Dodger")),
            row(201, None, None),
        ]
    );

    tr.apply(
        t(1040),
        &LogEvent::RoomEntered {
            room: RoomKind::GameRoom,
            match_id: id,
            status: Some(RoomStatus::WaitClients),
        },
    );
    tr.apply(
        t(1040),
        &LogEvent::PlayerList {
            players: standard_roster(),
        },
    );
    store.record_match(state(&tr)).expect("记录名单");
    store.record_match(&early).expect("重放早到的等待室记录");

    let ids: Vec<u64> = store
        .match_players(id)
        .expect("读取")
        .into_iter()
        .map(|r| r.player.get())
        .collect();
    assert_eq!(ids, vec![101, 102, 201, ME], "没进名单的人不算同场");
    assert!(
        store.player(PlayerId(999)).expect("读取").is_some(),
        "但见过他这件事仍然保留"
    );
}

#[test]
fn encounters_count_teammates_and_opponents_separately() {
    let mut store = Store::open_in_memory().expect("打开长期库");
    // 第一场：Ash 是对手
    store
        .record_match(state(&played(1, 1000, standard_roster())))
        .expect("记录");
    // 第二场：Ash 是队友
    store
        .record_match(state(&played(
            2,
            5000,
            vec![
                entry(101, "Ash", Team::Alpha),
                entry(ME, "Me", Team::Alpha),
                entry(301, "Elm", Team::Bravo),
            ],
        )))
        .expect("记录");
    // 第三场：Ash 在观战
    store
        .record_match(state(&played(
            3,
            9000,
            vec![
                entry(ME, "Me", Team::Alpha),
                entry(301, "Elm", Team::Bravo),
                entry(101, "Ash", Team::Spectators),
            ],
        )))
        .expect("记录");

    let ash = store.encounters(PlayerId(101), None).expect("统计");
    assert_eq!(
        ash,
        EncounterSummary {
            total: 3,
            as_teammate: 1,
            as_opponent: 1,
            first_at: Some(t(1000)),
            last_at: Some(t(9000)),
        }
    );

    // 正在进行的这一场不算“以前遇到过”
    let before_third = store
        .encounters(PlayerId(101), Some(MatchId(3)))
        .expect("统计");
    assert_eq!(before_third.total, 2);
    assert_eq!(before_third.last_at, Some(t(5000)));

    let elm = store.encounters(PlayerId(301), None).expect("统计");
    assert_eq!((elm.total, elm.as_teammate, elm.as_opponent), (2, 0, 2));

    assert_eq!(
        store.encounters(PlayerId(ME), None).expect("统计"),
        EncounterSummary::default(),
        "自己不算遇到自己"
    );
    assert_eq!(
        store.encounters(PlayerId(424242), None).expect("统计"),
        EncounterSummary::default()
    );
}

#[test]
fn the_most_common_player_points_at_the_local_player() {
    let mut store = Store::open_in_memory().expect("打开长期库");
    assert_eq!(store.most_common_player().expect("统计"), None);

    for (id, others) in [(1, [101, 102]), (2, [101, 103]), (3, [104, 105])] {
        store
            .record_match(state(&played(
                id,
                1000 * id as i64,
                vec![
                    entry(others[0], "a", Team::Alpha),
                    entry(others[1], "b", Team::Alpha),
                    entry(ME, "Me", Team::Bravo),
                ],
            )))
            .expect("记录");
    }
    assert_eq!(
        store.most_common_player().expect("统计"),
        Some((PlayerId(ME), 3, 3))
    );
}

#[test]
fn a_name_from_an_older_log_does_not_replace_a_newer_one() {
    let mut store = Store::open_in_memory().expect("打开长期库");
    store
        .record_match(state(&played(
            2,
            5000,
            vec![
                entry(101, "NewName", Team::Alpha),
                entry(ME, "Me", Team::Bravo),
            ],
        )))
        .expect("记录新比赛");
    // 之后才导入更早的一场
    store
        .record_match(state(&played(
            1,
            1000,
            vec![
                entry(101, "OldName", Team::Alpha),
                entry(ME, "Me", Team::Bravo),
            ],
        )))
        .expect("记录旧比赛");

    let p = store.player(PlayerId(101)).expect("读取").expect("有记录");
    assert_eq!(p.last_log_name.as_deref(), Some("NewName"));
    assert_eq!(p.first_seen_at, Some(t(1000)));
    assert_eq!(p.last_seen_at, Some(t(5000)));

    let recent: Vec<u64> = store
        .recent_matches(10)
        .expect("读取")
        .into_iter()
        .map(|m| m.id.get())
        .collect();
    assert_eq!(recent, vec![2, 1], "按比赛时间排序，与写入顺序无关");
}

#[test]
fn profile_updates_do_not_touch_sightings_and_vice_versa() {
    let mut store = Store::open_in_memory().expect("打开长期库");
    store
        .record_match(state(&played(1, 1000, standard_roster())))
        .expect("记录");
    store
        .save_profile(&profile(101, "Ash (API)", 2100.0), t(1100))
        .expect("写入资料");
    store
        .record_match(state(&played(2, 5000, standard_roster())))
        .expect("再遇到一次");

    let p = store.player(PlayerId(101)).expect("读取").expect("有记录");
    assert_eq!(p.name.as_deref(), Some("Ash (API)"));
    assert_eq!(p.rating, Some(2100.0));
    assert_eq!(p.fetched_at, Some(t(1100)));
    assert_eq!(p.last_log_name.as_deref(), Some("Ash"));
    assert_eq!(p.first_seen_at, Some(t(1000)));
    assert_eq!(p.last_seen_at, Some(t(5000)));
    assert_eq!(p.display_name(), Some("Ash (API)"), "接口资料里的名字优先");
}

#[test]
fn batch_lookup_returns_only_known_players() {
    let mut store = Store::open_in_memory().expect("打开长期库");
    store
        .save_profile(&profile(1, "a", 1.0), t(1))
        .expect("写入");
    store
        .save_profile(&profile(3, "c", 3.0), t(1))
        .expect("写入");
    let got: Vec<u64> = store
        .players(&[PlayerId(3), PlayerId(2), PlayerId(1)])
        .expect("读取")
        .into_iter()
        .map(|p| p.id.get())
        .collect();
    assert_eq!(got, vec![3, 1]);
}

#[test]
fn clock_updates_follow_the_trust_order() {
    let mut store = Store::open_in_memory().expect("打开长期库");
    let id = MatchId(1);
    store
        .record_match(state(&played(1, 1000, standard_roster())))
        .expect("记录");
    let manual = BattleClock {
        start_at: t(1128),
        source: ClockSource::Manual,
    };
    let estimate = BattleClock {
        start_at: t(1999),
        source: ClockSource::LogEstimate,
    };
    let report = BattleClock {
        start_at: t(1129),
        source: ClockSource::Report,
    };

    assert!(store.set_match_clock(id, manual).expect("校准"));
    assert!(
        !store
            .set_match_clock(id, estimate)
            .expect("推算值不能覆盖手动校准")
    );
    assert_eq!(
        store.match_record(id).expect("读取").and_then(|m| m.clock),
        Some(manual)
    );

    // 再记录一次带推算基准的状态，也不会把手动校准冲掉
    store
        .record_match(state(&played(1, 1000, standard_roster())))
        .expect("重复记录");
    assert_eq!(
        store.match_record(id).expect("读取").and_then(|m| m.clock),
        Some(manual)
    );

    assert!(store.set_match_clock(id, report).expect("战报修正"));
    assert_eq!(
        store.match_record(id).expect("读取").and_then(|m| m.clock),
        Some(report)
    );
    assert!(
        !store
            .set_match_clock(MatchId(404), manual)
            .expect("没有这场比赛")
    );
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct OverlaySettings {
    opacity: f32,
    theme: String,
}

#[test]
fn settings_round_trip_as_typed_values() {
    let mut store = Store::open_in_memory().expect("打开长期库");
    assert_eq!(store.setting::<String>("log_dir").expect("读取"), None);

    store
        .set_setting("log_dir", &"D:\\Games\\GameLogs")
        .expect("写入");
    store.set_setting("self_id", &PlayerId(ME)).expect("写入");
    let overlay = OverlaySettings {
        opacity: 0.85,
        theme: "深色".into(),
    };
    store.set_setting("overlay", &overlay).expect("写入");
    store.set_setting("overlay", &overlay).expect("覆盖");

    assert_eq!(
        store.setting::<String>("log_dir").expect("读取").as_deref(),
        Some("D:\\Games\\GameLogs")
    );
    assert_eq!(
        store.setting::<PlayerId>("self_id").expect("读取"),
        Some(PlayerId(ME))
    );
    assert_eq!(
        store.setting::<OverlaySettings>("overlay").expect("读取"),
        Some(overlay)
    );

    // 类型对不上时报错，不悄悄返回默认值
    let err = store.setting::<u32>("log_dir").expect_err("应当失败");
    assert!(matches!(err, StoreError::Setting { ref key, .. } if key == "log_dir"));

    assert!(store.remove_setting("log_dir").expect("删除"));
    assert!(!store.remove_setting("log_dir").expect("已经没有了"));
    assert_eq!(store.setting::<String>("log_dir").expect("读取"), None);
}

#[test]
fn data_survives_reopening_the_file() {
    let dir = tempfile::tempdir().expect("临时目录");
    let path = dir.path().join("data").join("bax.db");
    {
        let mut store = Store::open(&path).expect("建库");
        store
            .record_match(state(&played(1, 1000, standard_roster())))
            .expect("记录");
        store
            .save_profile(&profile(101, "Ash", 2100.0), t(1100))
            .expect("写入");
        store.set_setting("self_id", &PlayerId(ME)).expect("写入");
    }
    let store = Store::open(&path).expect("再次打开");
    assert!(store.match_record(MatchId(1)).expect("读取").is_some());
    assert_eq!(
        store
            .player(PlayerId(101))
            .expect("读取")
            .and_then(|p| p.rating),
        Some(2100.0)
    );
    assert_eq!(
        store.setting::<PlayerId>("self_id").expect("读取"),
        Some(PlayerId(ME))
    );
    assert!(
        !db::backup_path(&path, 1).exists(),
        "结构版本没变时不产生备份"
    );
}

#[test]
fn a_database_from_a_newer_version_is_refused_untouched() {
    let dir = tempfile::tempdir().expect("临时目录");
    let path = dir.path().join("bax.db");
    {
        let store = Store::open(&path).expect("建库");
        store
            .conn
            .pragma_update(None, "user_version", 999)
            .expect("伪造未来版本");
    }
    let err = Store::open(&path).expect_err("应当拒绝打开");
    assert!(matches!(
        err,
        StoreError::SchemaTooNew { found: 999, supported } if supported == MIGRATIONS.len() as u32
    ));
}

#[test]
fn migrating_existing_data_leaves_a_backup_beside_the_database() {
    let dir = tempfile::tempdir().expect("临时目录");
    let path = dir.path().join("bax.db");
    let v1 = "CREATE TABLE notes_demo (id INTEGER PRIMARY KEY, body TEXT NOT NULL) STRICT;";
    let v2 = "ALTER TABLE notes_demo ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0;";

    {
        let conn = db::open(Some(&path), &[v1], true).expect("建第 1 版");
        conn.execute("INSERT INTO notes_demo(id, body) VALUES (1, '老数据')", [])
            .expect("写入");
    }
    assert!(!db::backup_path(&path, 1).exists(), "新建的库不需要备份");

    let conn = db::open(Some(&path), &[v1, v2], true).expect("迁移到第 2 版");
    assert_eq!(db::schema_version(&conn).expect("读取版本"), 2);
    let (body, pinned): (String, i64) = conn
        .query_row(
            "SELECT body, pinned FROM notes_demo WHERE id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("读取迁移后的数据");
    assert_eq!((body.as_str(), pinned), ("老数据", 0));

    // 备份是迁移之前的样子：有数据，没有新列
    let backup = db::backup_path(&path, 1);
    assert!(backup.exists());
    let old = Connection::open(&backup).expect("打开备份");
    let count: i64 = old
        .query_row("SELECT COUNT(*) FROM notes_demo", [], |row| row.get(0))
        .expect("读取备份");
    assert_eq!(count, 1);
    assert!(
        old.prepare("SELECT pinned FROM notes_demo").is_err(),
        "备份里不应该有迁移后才加的列"
    );
}

#[test]
fn a_failed_migration_leaves_the_previous_version_in_place() {
    let dir = tempfile::tempdir().expect("临时目录");
    let path = dir.path().join("bax.db");
    let v1 = "CREATE TABLE a (id INTEGER PRIMARY KEY) STRICT;";
    let broken = "CREATE TABLE b (id INTEGER PRIMARY KEY) STRICT; THIS IS NOT SQL;";
    drop(db::open(Some(&path), &[v1], true).expect("建第 1 版"));

    assert!(db::open(Some(&path), &[v1, broken], true).is_err());
    let conn = db::open(Some(&path), &[v1], true).expect("旧版本仍可打开");
    assert_eq!(db::schema_version(&conn).expect("读取版本"), 1);
    assert!(
        conn.prepare("SELECT id FROM b").is_err(),
        "失败的迁移整体回滚，不留下半张表"
    );
}

#[test]
fn migration_backup_includes_commits_after_an_active_read_snapshot() {
    let dir = tempfile::tempdir().expect("临时目录");
    let path = dir.path().join("bax.db");
    let v1 = "CREATE TABLE notes (body TEXT);";
    let writer = db::open(Some(&path), &[v1], true).expect("旧库");
    writer
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
        .expect("检查点");
    let reader = Connection::open(&path).expect("读连接");
    reader
        .execute_batch("BEGIN; SELECT * FROM notes")
        .expect("固定旧快照");
    writer
        .execute("INSERT INTO notes VALUES ('已保存')", [])
        .expect("提交备注");
    let upgraded = db::open(
        Some(&path),
        &[v1, "ALTER TABLE notes ADD COLUMN tag TEXT"],
        true,
    )
    .expect("升级");
    let backup = Connection::open(db::backup_path(&path, 1)).expect("读取备份");
    assert_eq!(db::schema_version(&backup).expect("版本"), 1);
    for conn in [&backup, &upgraded] {
        let body: String = conn
            .query_row("SELECT body FROM notes", [], |r| r.get(0))
            .expect("备注");
        assert_eq!(body, "已保存");
    }
}

#[test]
fn reminder_outcomes_are_recorded_once_per_match_and_rule() {
    let mut store = Store::open_in_memory().expect("打开长期库");
    let id = MatchId(1);
    assert!(store.reminder_outcomes(id).expect("读取").is_empty());

    assert!(
        store
            .record_reminder(id, 1, ReminderOutcome::Fired, t(1600))
            .expect("写入")
    );
    assert!(
        !store
            .record_reminder(id, 1, ReminderOutcome::Missed, t(1700))
            .expect("重复写入"),
        "第一次的结果为准"
    );
    store
        .record_reminder(id, 2, ReminderOutcome::Missed, t(1800))
        .expect("写入");
    store
        .record_reminder(MatchId(2), 1, ReminderOutcome::Fired, t(9000))
        .expect("另一场比赛");

    assert_eq!(
        store.reminder_outcomes(id).expect("读取"),
        vec![
            (1, ReminderOutcome::Fired, t(1600)),
            (2, ReminderOutcome::Missed, t(1800)),
        ]
    );
}

#[test]
fn settings_can_be_listed_by_prefix() {
    let mut store = Store::open_in_memory().expect("打开长期库");
    store
        .set_setting("ui.overlay", &serde_json::json!({"opacity": 0.9}))
        .expect("写入");
    store
        .set_setting("ui.hotkeys", &serde_json::json!({"toggle": "Ctrl+Alt+B"}))
        .expect("写入");
    store.set_setting("uix.other", &1).expect("写入");
    store.set_setting("log.dir", &"D:\\Logs").expect("写入");
    // LIKE 里的通配符不应生效
    store.set_setting("u_.weird", &2).expect("写入");

    let keys: Vec<String> = store
        .settings_with_prefix("ui.")
        .expect("读取")
        .into_iter()
        .map(|(k, _)| k)
        .collect();
    assert_eq!(keys, vec!["ui.hotkeys", "ui.overlay"]);
    let (_, raw) = &store.settings_with_prefix("ui.overlay").expect("读取")[0];
    assert_eq!(raw, r#"{"opacity":0.9}"#);
}

#[test]
fn a_version_one_database_upgrades_with_a_backup() {
    let dir = tempfile::tempdir().expect("临时目录");
    let path = dir.path().join("bax.db");
    {
        // 0.1.0 创建的库只有第 1 版结构
        let conn = db::open(Some(&path), &MIGRATIONS[..1], true).expect("建第 1 版");
        let mut old = Store { conn };
        old.record_match(state(&played(1, 1000, standard_roster())))
            .expect("记录");
    }
    let store = Store::open(&path).expect("升级");
    assert_eq!(
        store.schema_version().expect("读取版本"),
        MIGRATIONS.len() as u32,
        "一次升级到最新版本"
    );
    assert!(store.note(PlayerId(101)).expect("备注表可用").is_none());
    assert!(
        store.match_record(MatchId(1)).expect("读取").is_some(),
        "旧数据还在"
    );
    assert!(
        store
            .reminder_outcomes(MatchId(1))
            .expect("新表可用")
            .is_empty()
    );
    assert!(db::backup_path(&path, 1).exists(), "升级前留了备份");
}

#[test]
fn version_four_migrates_to_five_and_existing_five_reopens_without_data_loss() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("bax.db");
    {
        let conn = db::open(Some(&path), &MIGRATIONS[..4], true).expect("v4");
        let mut old = Store { conn };
        old.set_note(PlayerId(101), "保留备注", Timestamp::from_millis(1000))
            .expect("note");
        old.record_match(state(&played(1, 1000, standard_roster())))
            .expect("match");
    }
    {
        let store = Store {
            conn: db::open(Some(&path), &MIGRATIONS[..5], true).expect("v4 -> v5"),
        };
        assert_eq!(store.schema_version().expect("version"), 5);
        assert!(db::backup_path(&path, 4).exists());
        let _: u32 = store
            .conn
            .query_row("SELECT COUNT(*) FROM cancelled_matches", [], |r| r.get(0))
            .expect("v5 view");
    }
    let store = Store::open(&path).expect("升级已有 v5");
    assert_eq!(
        store
            .note(PlayerId(101))
            .expect("note")
            .expect("exists")
            .body,
        "保留备注"
    );
    assert!(store.match_record(MatchId(1)).expect("match").is_some());
    assert_eq!(
        store.schema_version().expect("version"),
        MIGRATIONS.len() as u32
    );
}

#[test]
fn version_six_indexes_existing_reports_and_replaces_old_participants_atomically() {
    let dir = tempfile::tempdir().expect("临时目录");
    let path = dir.path().join("bax.db");
    let old = db::open(Some(&path), &MIGRATIONS[..5], true).expect("v5");
    let raw = r#"{"matchInfo":{"Data":{
        "101":{"TransferredUnits":{"shared":{"Id":81}},"UnitData":null},
        "102":{"UnitData":{"shared":{"Id":81},"own":{"Id":82},"from_ai":{"Id":99}}},
        "-1":{"TransferredUnits":{"from_ai":{"Id":99}}},
        "103":{"Id":103,"UnitData":{}}
    }}}"#;
    old.execute(
        "INSERT INTO match_reports(match_id, raw, fetched_at, start_time)
                 VALUES (7, ?1, 20, 10), (8, 'legacy invalid JSON', 30, NULL)",
        [raw],
    )
    .expect("旧战报");
    drop(old);
    let mut store = Store::open(&path).expect("升级");
    let id = MatchId(7);
    let units = store
        .indexed_report(id)
        .expect("旧资料的索引")
        .expect("已归档")
        .units;
    assert_eq!(units.len(), 3);
    assert_eq!(units[&PlayerId(101)], [81].into());
    assert_eq!(units[&PlayerId(102)], [82].into());
    assert!(units[&PlayerId(103)].is_empty());
    assert_eq!(
        store.recent_player_reports(PlayerId(101), 1).expect("原文")[0].1,
        raw
    );
    let changed = store
        .save_report_with_players(
            id,
            r#"{"matchInfo":{"Data":{"104":{}}}}"#,
            Default::default(),
            t(40),
        )
        .expect("替换");
    assert_eq!(changed, [101, 102, 103, 104].map(PlayerId).into());
    assert!(
        store
            .recent_player_units(PlayerId(101), 20)
            .expect("已移除")
            .is_empty()
    );
    assert_eq!(
        store
            .recent_player_units(PlayerId(104), 20)
            .expect("新玩家")
            .len(),
        1
    );
    assert!(db::backup_path(&path, 5).exists());
}
