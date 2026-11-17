use super::*;

const MATCH: MatchId = MatchId(9_000_001);
const OTHER: MatchId = MatchId(9_000_002);

fn t(secs: i64) -> Timestamp {
    Timestamp::from_secs(secs)
}

fn entered(room: RoomKind, id: MatchId, status: Option<RoomStatus>) -> LogEvent {
    LogEvent::RoomEntered {
        room,
        match_id: id,
        status,
    }
}

fn exited(room: RoomKind, id: MatchId, cause: ExitCause, status: RoomStatus) -> LogEvent {
    LogEvent::RoomExited {
        room,
        match_id: Some(id),
        cause: Some(cause),
        status: Some(status),
    }
}

fn loading() -> LogEvent {
    LogEvent::LoadingStarted {
        map: "Airport".into(),
        scenario: Some("Airport_network".into()),
    }
}

fn player(id: u64, name: &str, team: Team) -> RosterEntry {
    RosterEntry {
        id: PlayerId(id),
        name: (!name.is_empty()).then(|| name.to_owned()),
        team,
    }
}

fn roster() -> Vec<RosterEntry> {
    vec![
        player(101, "Ash", Team::Alpha),
        player(102, "", Team::Alpha),
        player(103, "Cedar", Team::Alpha),
        player(201, "Dune", Team::Bravo),
        player(202, "Me", Team::Bravo),
        player(203, "Fern", Team::Bravo),
    ]
}

/// 正常开局到场景加载完成为止的事件，带上游戏对同一件事写出的新旧两种日志。
fn fresh_until_loaded(tr: &mut Tracker) -> Vec<Update> {
    let mut all = Vec::new();
    let mut feed = |at: i64, ev: LogEvent| all.extend(tr.apply(t(at), &ev));
    feed(
        0,
        LogEvent::Persona {
            name: "MySteamName".into(),
        },
    );
    feed(
        5,
        LogEvent::DeckSelected {
            name: Some("my deck".into()),
        },
    );
    feed(
        10,
        entered(RoomKind::WaitingRoom, MATCH, Some(RoomStatus::WaitClients)),
    );
    feed(10, entered(RoomKind::WaitingRoom, MATCH, None));
    feed(
        20,
        LogEvent::RoomClient {
            room: RoomKind::WaitingRoom,
            player: PlayerId(101),
            name: Some("Ash".into()),
        },
    );
    feed(40, loading());
    feed(
        48,
        LogEvent::FightInfo {
            match_id: MATCH,
            player_name: Some("Me".into()),
        },
    );
    feed(
        50,
        exited(
            RoomKind::WaitingRoom,
            MATCH,
            ExitCause::ServerExitInstance,
            RoomStatus::WaitClients,
        ),
    );
    feed(
        50,
        LogEvent::RoomExited {
            room: RoomKind::WaitingRoom,
            match_id: None,
            cause: None,
            status: None,
        },
    );
    feed(
        50,
        entered(RoomKind::GameRoom, MATCH, Some(RoomStatus::WaitClients)),
    );
    feed(50, entered(RoomKind::GameRoom, MATCH, None));
    feed(50, LogEvent::PlayerList { players: roster() });
    feed(80, LogEvent::ScenarioLoaded);
    all
}

fn current(tr: &Tracker) -> &MatchState {
    tr.current().expect("应当有当前比赛")
}

#[test]
fn fresh_match_reaches_deployment_with_estimated_clock() {
    let mut tr = Tracker::default();
    let updates = fresh_until_loaded(&mut tr);

    let m = current(&tr);
    assert_eq!(m.match_id, MATCH);
    assert_eq!(m.phase, Phase::Deployment);
    assert_eq!(m.connection, Connection::Connected);
    assert!(!m.rejoined);
    assert_eq!(m.map.as_deref(), Some("Airport"));
    assert_eq!(m.scenario.as_deref(), Some("Airport_network"));
    assert_eq!(m.deck.as_deref(), Some("my deck"));
    assert_eq!(m.roster.len(), 6);
    assert_eq!(m.times.discovered_at, t(10));
    assert_eq!(m.times.loading_at, Some(t(40)));
    assert_eq!(m.times.game_room_at, Some(t(50)));
    assert_eq!(m.times.scenario_loaded_at, Some(t(80)));
    assert_eq!(
        m.clock,
        Some(BattleClock {
            start_at: t(140),
            source: ClockSource::LogEstimate
        })
    );

    // 每件事只报告一次，新旧两种日志写法不会造成重复
    let discovered = updates
        .iter()
        .filter(|u| matches!(u, Update::MatchDiscovered { .. }))
        .count();
    let rosters = updates
        .iter()
        .filter(|u| matches!(u, Update::RosterReady { .. }))
        .count();
    assert_eq!((discovered, rosters), (1, 1));
    assert!(updates.contains(&Update::EarlyPlayers {
        match_id: MATCH,
        players: vec![PlayerId(101)]
    }));
}

#[test]
fn advance_moves_from_deployment_to_battle_at_clock_start() {
    let mut tr = Tracker::default();
    fresh_until_loaded(&mut tr);

    assert!(tr.advance(t(139)).is_empty());
    assert_eq!(current(&tr).phase, Phase::Deployment);
    assert_eq!(current(&tr).elapsed_ms(t(139)), Some(-1000));

    assert_eq!(
        tr.advance(t(140)),
        vec![Update::Changed { match_id: MATCH }]
    );
    assert_eq!(current(&tr).phase, Phase::InBattle);
    assert!(tr.advance(t(141)).is_empty(), "阶段不变时不再报告");
    assert_eq!(current(&tr).elapsed_ms(t(740)), Some(600_000));
}

#[test]
fn match_end_is_reported_once_across_both_signals() {
    let mut tr = Tracker::default();
    fresh_until_loaded(&mut tr);
    tr.advance(t(140));

    let first = tr.apply(t(2000), &LogEvent::MatchEnded { match_id: MATCH });
    assert_eq!(first, vec![Update::MatchEnded { match_id: MATCH }]);
    assert_eq!(current(&tr).phase, Phase::Ended);
    assert_eq!(current(&tr).times.ended_at, Some(t(2000)));

    // 半分钟后离开结算界面：只是连接状态变化，不再报告一次结束
    let second = tr.apply(
        t(2030),
        &exited(
            RoomKind::GameRoom,
            MATCH,
            ExitCause::ServerExitInstance,
            RoomStatus::End,
        ),
    );
    assert_eq!(second, vec![Update::Changed { match_id: MATCH }]);
    let m = current(&tr);
    assert_eq!(m.connection, Connection::Disconnected);
    assert_eq!(m.times.ended_at, Some(t(2000)), "结束时刻以最早的信号为准");
    assert_eq!(m.times.left_at, Some(t(2030)));
    assert_eq!(m.end_cause, Some(ExitCause::MatchEnded));
}

#[test]
fn exit_with_end_status_alone_ends_the_match() {
    let mut tr = Tracker::default();
    fresh_until_loaded(&mut tr);
    let out = tr.apply(
        t(2000),
        &exited(
            RoomKind::GameRoom,
            MATCH,
            ExitCause::ServerExitInstance,
            RoomStatus::End,
        ),
    );
    assert_eq!(out, vec![Update::MatchEnded { match_id: MATCH }]);
    assert_eq!(current(&tr).phase, Phase::Ended);
}

#[test]
fn rejoin_in_a_new_process_has_no_clock_until_restored() {
    let mut tr = Tracker::default();
    let mut all = Vec::new();
    let mut feed = |at: i64, ev: LogEvent| all.extend(tr.apply(t(at), &ev));
    feed(0, LogEvent::ReconnectStarted);
    feed(
        1,
        entered(RoomKind::WaitingRoom, MATCH, Some(RoomStatus::InProcess)),
    );
    feed(1, entered(RoomKind::WaitingRoom, MATCH, None));
    feed(1, loading());
    // 重连时场景加载完成早于进入对局房间
    feed(8, LogEvent::ScenarioLoaded);
    feed(
        8,
        exited(
            RoomKind::WaitingRoom,
            MATCH,
            ExitCause::ServerExitInstance,
            RoomStatus::InProcess,
        ),
    );
    feed(
        8,
        entered(RoomKind::GameRoom, MATCH, Some(RoomStatus::InProcess)),
    );
    feed(8, LogEvent::PlayerList { players: roster() });

    let m = current(&tr);
    assert!(m.rejoined);
    assert_eq!(m.phase, Phase::InBattle);
    assert_eq!(m.connection, Connection::Connected);
    assert_eq!(m.clock, None, "重连时日志给不出计时起点");
    assert_eq!(m.times.scenario_loaded_at, None);
    assert_eq!(m.map.as_deref(), Some("Airport"));
    assert_eq!(m.roster.len(), 6);
    assert!(!all.iter().any(|u| matches!(u, Update::ClockSet { .. })));

    // 从存储里恢复当初保存的基准
    let saved = BattleClock {
        start_at: t(-600),
        source: ClockSource::LogEstimate,
    };
    assert_eq!(
        tr.set_clock(MATCH, saved),
        vec![Update::ClockSet {
            match_id: MATCH,
            clock: saved
        }]
    );
    assert_eq!(current(&tr).elapsed_ms(t(0)), Some(600_000));
    assert!(tr.advance(t(10)).is_empty());
    assert_eq!(current(&tr).phase, Phase::InBattle);
}

#[test]
fn disconnect_and_rejoin_in_the_same_process_keeps_the_clock() {
    let mut tr = Tracker::default();
    fresh_until_loaded(&mut tr);
    tr.advance(t(140));
    let clock = current(&tr).clock;

    let out = tr.apply(
        t(500),
        &exited(
            RoomKind::GameRoom,
            MATCH,
            ExitCause::ConnectionLost,
            RoomStatus::InProcess,
        ),
    );
    assert_eq!(
        out,
        vec![Update::Disconnected {
            match_id: MATCH,
            cause: ExitCause::ConnectionLost
        }]
    );
    assert_eq!(current(&tr).connection, Connection::Disconnected);
    assert_eq!(current(&tr).phase, Phase::InBattle, "断线不改变比赛阶段");

    tr.apply(t(520), &LogEvent::ReconnectStarted);
    let out = tr.apply(
        t(521),
        &entered(RoomKind::WaitingRoom, MATCH, Some(RoomStatus::InProcess)),
    );
    assert_eq!(out, vec![Update::Changed { match_id: MATCH }]);
    assert!(
        tr.apply(t(521), &entered(RoomKind::WaitingRoom, MATCH, None))
            .is_empty()
    );
    assert_eq!(current(&tr).connection, Connection::Reconnecting);

    tr.apply(t(522), &loading());
    assert!(tr.apply(t(528), &LogEvent::ScenarioLoaded).is_empty());
    tr.apply(
        t(528),
        &entered(RoomKind::GameRoom, MATCH, Some(RoomStatus::InProcess)),
    );

    let m = current(&tr);
    assert_eq!(m.connection, Connection::Connected);
    assert_eq!(m.clock, clock);
    assert_eq!(m.times.scenario_loaded_at, Some(t(80)));
    assert!(m.rejoined);
}

#[test]
fn new_match_supersedes_an_unfinished_one() {
    let mut tr = Tracker::default();
    fresh_until_loaded(&mut tr);
    tr.apply(
        t(500),
        &exited(
            RoomKind::GameRoom,
            MATCH,
            ExitCause::ServiceDisposed,
            RoomStatus::InProcess,
        ),
    );

    let out = tr.apply(
        t(900),
        &entered(RoomKind::WaitingRoom, OTHER, Some(RoomStatus::WaitClients)),
    );
    assert_eq!(
        out,
        vec![
            Update::MatchSuperseded { match_id: MATCH },
            Update::MatchDiscovered { match_id: OTHER },
        ]
    );
    let m = current(&tr);
    assert_eq!(m.match_id, OTHER);
    assert_eq!(m.phase, Phase::WaitingRoom);
    assert!(m.roster.is_empty());
    assert_eq!(m.clock, None);
}

#[test]
fn finished_match_is_replaced_without_supersede() {
    let mut tr = Tracker::default();
    fresh_until_loaded(&mut tr);
    tr.apply(t(2000), &LogEvent::MatchEnded { match_id: MATCH });
    let out = tr.apply(
        t(2100),
        &entered(RoomKind::WaitingRoom, OTHER, Some(RoomStatus::WaitClients)),
    );
    assert_eq!(out, vec![Update::MatchDiscovered { match_id: OTHER }]);
}

#[test]
fn noise_after_a_finished_match_is_ignored() {
    let mut tr = Tracker::default();
    fresh_until_loaded(&mut tr);
    tr.apply(t(2000), &LogEvent::MatchEnded { match_id: MATCH });
    tr.apply(
        t(2030),
        &exited(
            RoomKind::GameRoom,
            MATCH,
            ExitCause::MatchEnded,
            RoomStatus::End,
        ),
    );
    let before = current(&tr).clone();

    // 之后玩一局遭遇战：没有比赛 ID，但同样会打印加载、名单和场景加载完成
    let mut noise = Vec::new();
    noise.extend(tr.apply(t(3000), &LogEvent::DeckSelected { name: None }));
    noise.extend(tr.apply(
        t(3001),
        &LogEvent::LoadingStarted {
            map: "Dam".into(),
            scenario: Some("Dam_Skirmish".into()),
        },
    ));
    noise.extend(tr.apply(
        t(3002),
        &LogEvent::PlayerList {
            players: vec![player(202, "Me", Team::Alpha)],
        },
    ));
    noise.extend(tr.apply(t(3003), &LogEvent::ScenarioLoaded));
    noise.extend(tr.advance(t(4000)));

    assert!(noise.is_empty(), "{noise:?}");
    assert_eq!(current(&tr), &before);
}

#[test]
fn entering_a_finished_match_reports_discovery_and_end() {
    let mut tr = Tracker::default();
    tr.apply(t(0), &LogEvent::ReconnectStarted);
    let out = tr.apply(
        t(1),
        &entered(RoomKind::WaitingRoom, MATCH, Some(RoomStatus::End)),
    );
    assert_eq!(
        out,
        vec![
            Update::MatchDiscovered { match_id: MATCH },
            Update::MatchEnded { match_id: MATCH },
        ]
    );
    let m = current(&tr);
    assert_eq!(m.phase, Phase::Ended);
    assert_eq!(m.times.ended_at, None, "没有观察到结束的时刻");

    // 随后经过等待室进入对局房间，状态仍是已结束，不再重复报告
    let mut more = Vec::new();
    more.extend(tr.apply(
        t(2),
        &exited(
            RoomKind::WaitingRoom,
            MATCH,
            ExitCause::ServerExitInstance,
            RoomStatus::End,
        ),
    ));
    more.extend(tr.apply(
        t(2),
        &entered(RoomKind::GameRoom, MATCH, Some(RoomStatus::End)),
    ));
    assert!(!more.iter().any(|u| matches!(u, Update::MatchEnded { .. })));
}

#[test]
fn old_logs_without_status_fall_back_to_the_reconnect_marker() {
    // 旧版日志：正常开局
    let mut tr = Tracker::default();
    tr.apply(t(0), &entered(RoomKind::WaitingRoom, MATCH, None));
    tr.apply(t(5), &loading());
    tr.apply(t(9), &entered(RoomKind::GameRoom, MATCH, None));
    tr.apply(t(9), &LogEvent::PlayerList { players: roster() });
    tr.apply(t(30), &LogEvent::ScenarioLoaded);
    assert_eq!(current(&tr).phase, Phase::Deployment);
    assert!(current(&tr).clock.is_some());

    // 旧版日志：重连
    let mut tr = Tracker::default();
    tr.apply(t(0), &LogEvent::ReconnectStarted);
    tr.apply(t(1), &entered(RoomKind::WaitingRoom, MATCH, None));
    tr.apply(t(2), &loading());
    tr.apply(t(5), &LogEvent::ScenarioLoaded);
    tr.apply(t(5), &entered(RoomKind::GameRoom, MATCH, None));
    let m = current(&tr);
    assert!(m.rejoined);
    assert_eq!(m.phase, Phase::InBattle);
    assert_eq!(m.clock, None);
}

#[test]
fn entering_during_deployment_becomes_battle_after_one_deployment_length() {
    let mut tr = Tracker::default();
    tr.apply(
        t(100),
        &entered(
            RoomKind::WaitingRoom,
            MATCH,
            Some(RoomStatus::DeploymentPhase),
        ),
    );
    assert_eq!(current(&tr).phase, Phase::Deployment);
    assert!(tr.advance(t(159)).is_empty());
    assert_eq!(
        tr.advance(t(160)),
        vec![Update::Changed { match_id: MATCH }]
    );
    assert_eq!(current(&tr).phase, Phase::InBattle);
    assert_eq!(current(&tr).clock, None);
}

#[test]
fn self_is_found_by_fight_name_then_persona_then_hint() {
    // FID 行给出的名字优先
    let mut tr = Tracker::default();
    fresh_until_loaded(&mut tr);
    assert_eq!(current(&tr).self_id, Some(PlayerId(202)));
    assert_eq!(current(&tr).my_team(), Some(Team::Bravo));

    // 没有 FID 行时用 Steam 昵称
    let mut tr = Tracker::default();
    tr.apply(
        t(0),
        &LogEvent::Persona {
            name: "Cedar".into(),
        },
    );
    tr.apply(t(1), &entered(RoomKind::GameRoom, MATCH, None));
    tr.apply(t(1), &LogEvent::PlayerList { players: roster() });
    assert_eq!(current(&tr).self_id, Some(PlayerId(103)));

    // 已知 ID 时直接采用，即使名字对不上
    let out = tr.set_self_hint(Some(PlayerId(102)));
    assert_eq!(out, vec![Update::Changed { match_id: MATCH }]);
    assert_eq!(current(&tr).self_id, Some(PlayerId(102)));
    assert_eq!(current(&tr).my_team(), Some(Team::Alpha));

    // 提示的 ID 不在名单里时退回按名字找
    tr.set_self_hint(Some(PlayerId(999)));
    assert_eq!(current(&tr).self_id, Some(PlayerId(103)));
}

#[test]
fn ambiguous_names_do_not_identify_anyone() {
    let mut tr = Tracker::default();
    tr.apply(
        t(0),
        &LogEvent::Persona {
            name: "Twin".into(),
        },
    );
    tr.apply(t(1), &entered(RoomKind::GameRoom, MATCH, None));
    tr.apply(
        t(1),
        &LogEvent::PlayerList {
            players: vec![
                player(1, "Twin", Team::Alpha),
                player(2, "Twin", Team::Bravo),
            ],
        },
    );
    assert_eq!(current(&tr).self_id, None);
    assert_eq!(current(&tr).my_team(), None);
}

#[test]
fn early_players_are_deduplicated_and_stop_once_the_roster_is_known() {
    let mut tr = Tracker::default();
    tr.apply(
        t(0),
        &entered(RoomKind::WaitingRoom, MATCH, Some(RoomStatus::WaitClients)),
    );
    let client = |id: u64| LogEvent::RoomClient {
        room: RoomKind::WaitingRoom,
        player: PlayerId(id),
        name: None,
    };
    assert_eq!(tr.apply(t(1), &client(7)).len(), 1);
    assert!(tr.apply(t(2), &client(7)).is_empty());
    assert_eq!(tr.apply(t(3), &client(8)).len(), 1);
    assert_eq!(
        current(&tr).known_player_ids(),
        vec![PlayerId(7), PlayerId(8)]
    );

    tr.apply(t(4), &loading());
    tr.apply(
        t(5),
        &entered(RoomKind::GameRoom, MATCH, Some(RoomStatus::WaitClients)),
    );
    tr.apply(t(5), &LogEvent::PlayerList { players: roster() });
    assert!(tr.apply(t(6), &client(9)).is_empty());
    assert_eq!(current(&tr).known_player_ids().len(), 6);

    // 对局房间里的同类日志出现在比赛结束之后，与名单无关
    let late = LogEvent::RoomClient {
        room: RoomKind::GameRoom,
        player: PlayerId(10),
        name: None,
    };
    assert!(tr.apply(t(7), &late).is_empty());
}

#[test]
fn clock_sources_override_by_trust_order() {
    let mut tr = Tracker::default();
    fresh_until_loaded(&mut tr);
    let manual = BattleClock {
        start_at: t(138),
        source: ClockSource::Manual,
    };
    let estimate = BattleClock {
        start_at: t(150),
        source: ClockSource::LogEstimate,
    };
    let report = BattleClock {
        start_at: t(139),
        source: ClockSource::Report,
    };

    assert_eq!(tr.set_clock(MATCH, manual).len(), 1);
    assert!(tr.set_clock(MATCH, manual).is_empty(), "相同的值不重复报告");
    assert!(
        tr.set_clock(MATCH, estimate).is_empty(),
        "推算值不能覆盖手动校准"
    );
    assert_eq!(current(&tr).clock, Some(manual));
    assert_eq!(tr.set_clock(MATCH, report).len(), 1);
    assert_eq!(current(&tr).clock, Some(report));
    assert!(tr.set_clock(OTHER, manual).is_empty(), "不是当前比赛");
}

#[test]
fn manual_calibration_to_a_later_start_returns_to_deployment() {
    let mut tr = Tracker::default();
    fresh_until_loaded(&mut tr);
    tr.advance(t(141));
    assert_eq!(current(&tr).phase, Phase::InBattle);

    tr.set_clock(
        MATCH,
        BattleClock {
            start_at: t(150),
            source: ClockSource::Manual,
        },
    );
    assert_eq!(
        tr.advance(t(142)),
        vec![Update::Changed { match_id: MATCH }]
    );
    assert_eq!(current(&tr).phase, Phase::Deployment);
}

#[test]
fn deck_cleared_after_the_match_does_not_erase_the_recorded_deck() {
    let mut tr = Tracker::default();
    fresh_until_loaded(&mut tr);
    tr.apply(t(2000), &LogEvent::MatchEnded { match_id: MATCH });
    assert!(
        tr.apply(t(2031), &LogEvent::DeckSelected { name: None })
            .is_empty()
    );
    assert_eq!(current(&tr).deck.as_deref(), Some("my deck"));
}

#[test]
fn state_serializes_with_camel_case_names() {
    let mut tr = Tracker::default();
    fresh_until_loaded(&mut tr);
    let json = serde_json::to_value(current(&tr)).expect("可序列化");
    assert_eq!(json["matchId"], 9_000_001);
    assert_eq!(json["phase"], "deployment");
    assert_eq!(json["clock"]["source"], "logEstimate");
    assert_eq!(json["roster"][1]["name"], serde_json::Value::Null);
    assert_eq!(json["roster"][0]["team"], "alpha");
}
