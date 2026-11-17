use super::*;

fn parse_all(text: &str) -> Vec<TimedEvent> {
    let mut parser = LogParser::new();
    let mut out = Vec::new();
    for line in text.lines() {
        parser.push_line(line, &mut out);
    }
    parser.flush(&mut out);
    out
}

fn events(text: &str) -> Vec<LogEvent> {
    parse_all(text).into_iter().map(|e| e.event).collect()
}

/// 把一条消息包成一条完整的记录。
fn record(message: &str) -> String {
    format!("[2026-11-16 18:52:05.319]\n{message}\n\n")
}

fn one(message: &str) -> Option<LogEvent> {
    let mut all = events(&record(message));
    assert!(all.len() <= 1, "一条记录解析出了多个事件：{all:?}");
    all.pop()
}

fn entry(id: u64, name: Option<&str>, team: Team) -> RosterEntry {
    RosterEntry {
        id: PlayerId(id),
        name: name.map(str::to_owned),
        team,
    }
}

#[test]
fn lifecycle_line_gives_room_entry_with_status() {
    let line = "Log: [NET_ROOM] Network lifecycle | Stage=Game | Action=Entered | ServerIp=192.0.2.10 | ServerPort=30214 | ConnectionIp=192.0.2.10 | ConnectionPort=30214 | Route=Direct | RoomType=WaitingRoom | FightId=9000001 | Status=WaitClients";
    assert_eq!(
        one(line),
        Some(LogEvent::RoomEntered {
            room: RoomKind::WaitingRoom,
            match_id: MatchId(9_000_001),
            status: Some(RoomStatus::WaitClients),
        })
    );

    let rejoin = "Log: [NET_ROOM] Network lifecycle | Stage=Game | Action=Entered | Route=Proxy(192.0.2.7) | RoomType=GameRoom | FightId=9000001 | Status=InProcess";
    assert_eq!(
        one(rejoin),
        Some(LogEvent::RoomEntered {
            room: RoomKind::GameRoom,
            match_id: MatchId(9_000_001),
            status: Some(RoomStatus::InProcess),
        })
    );
}

#[test]
fn lifecycle_line_gives_room_exit_with_cause() {
    let line = "Log: [NET_ROOM] Network lifecycle | Stage=Game | Action=Exited | Cause=ConnectionLost | Reason=Connection timed out | ServerIp=192.0.2.10 | ServerPort=30214 | ConnectionIp=192.0.2.10 | ConnectionPort=30214 | Route=Direct | RoomType=GameRoom | FightId=9000001 | Status=InProcess";
    assert_eq!(
        one(line),
        Some(LogEvent::RoomExited {
            room: RoomKind::GameRoom,
            match_id: Some(MatchId(9_000_001)),
            cause: Some(ExitCause::ConnectionLost),
            status: Some(RoomStatus::InProcess),
        })
    );
}

#[test]
fn lifecycle_lines_outside_the_game_stage_are_ignored() {
    for line in [
        "Log: [LOBBY] Network lifecycle | Stage=Lobby | Action=Entered | Route=Direct | LobbyId=2147 | Status=Pending",
        "Log: [NET_ROOM] Network lifecycle | Stage=Game | Action=Connecting | Attempt=1 | ServerIp=192.0.2.10 | Route=Direct",
        "Log: [AUTH] Network lifecycle | Stage=Authorization | Action=Entered | Endpoint=example.invalid:443",
    ] {
        assert_eq!(one(line), None, "{line}");
    }
}

#[test]
fn unknown_status_and_cause_values_survive_as_unknown() {
    let line = "Log: [NET_ROOM] Network lifecycle | Stage=Game | Action=Exited | Cause=SomethingNew | RoomType=GameRoom | FightId=5 | Status=Overtime";
    assert_eq!(
        one(line),
        Some(LogEvent::RoomExited {
            room: RoomKind::GameRoom,
            match_id: Some(MatchId(5)),
            cause: Some(ExitCause::Unknown),
            status: Some(RoomStatus::Unknown),
        })
    );
}

#[test]
fn room_markers_with_and_without_tag() {
    let tagged = one("Log: [NET_ROOM] WaitingRoom entered, fight id: 9000001");
    let untagged = one("Log: WaitingRoom entered, fight id: 9000001");
    let expected = Some(LogEvent::RoomEntered {
        room: RoomKind::WaitingRoom,
        match_id: MatchId(9_000_001),
        status: None,
    });
    assert_eq!(tagged, expected);
    assert_eq!(untagged, expected);

    assert_eq!(
        one("Log: [NET_ROOM] GameRoom entered, fight id: 12"),
        Some(LogEvent::RoomEntered {
            room: RoomKind::GameRoom,
            match_id: MatchId(12),
            status: None,
        })
    );
    assert_eq!(
        one("Log: [NET_ROOM] WaitingRoom exited"),
        Some(LogEvent::RoomExited {
            room: RoomKind::WaitingRoom,
            match_id: None,
            cause: None,
            status: None,
        })
    );
    assert_eq!(one("Log: [NET_ROOM] Connected 0"), None);
    assert_eq!(
        one("Log: [NET_ROOM] WaitingRoom entered, fight id: abc"),
        None
    );
}

#[test]
fn match_end_comes_from_the_traffic_history_line() {
    let ended = "Log: [NET_ROOM] Network traffic history | Cause=MatchEnded | FightId=9000001 | ServerIp=192.0.2.10 | ServerPort=30214 | Route=Direct | SampleInterval=30s | Samples=27 | IncomingBps=▁▁▃▃█▅▆ | MaxIncomingBps=15466 | OutgoingBps=▁▃▄▄▅ | MaxOutgoingBps=4451";
    assert_eq!(
        one(ended),
        Some(LogEvent::MatchEnded {
            match_id: MatchId(9_000_001)
        })
    );
    // 断线和主动退出也会打印流量历史，但不代表比赛结束
    let lost = "Log: [NET_ROOM] Network traffic history | Cause=ConnectionLost | FightId=9000001 | Samples=3";
    assert_eq!(one(lost), None);
}

#[test]
fn player_list_keeps_empty_and_awkward_names() {
    let text = "[2026-11-16 18:53:01.979]\n\
Log: Player list: \n\
ID: 101, Name: Ash, Team: Alpha\n\
ID: 102, Name: , Team: Alpha\n\
ID: 103, Name: 示例 なまえ, Team: Alpha\n\
ID: 104, Name: a, b, Team: Bravo\n\
ID: 105, Name: Team: Rocket, Team: Bravo\n\
ID: 106, Name: comma,, Team: Bravo\n\
ID: 107, Name: Watcher, Team: Spectators\n\
\n\
[2026-11-16 18:53:02.783]\n\
Log: Client name - Ash\n\
\n";
    let parsed = parse_all(text);
    assert_eq!(parsed.len(), 1);
    assert_eq!(
        parsed[0].at,
        LogTime::from_parts(2026, 11, 16, 18, 53, 1, 979).expect("合法时间")
    );
    assert_eq!(
        parsed[0].event,
        LogEvent::PlayerList {
            players: vec![
                entry(101, Some("Ash"), Team::Alpha),
                entry(102, None, Team::Alpha),
                entry(103, Some("示例 なまえ"), Team::Alpha),
                entry(104, Some("a, b"), Team::Bravo),
                entry(105, Some("Team: Rocket"), Team::Bravo),
                entry(106, Some("comma,"), Team::Bravo),
                entry(107, Some("Watcher"), Team::Spectators),
            ]
        }
    );
}

#[test]
fn player_list_in_the_old_format_without_separators() {
    assert_eq!(
        parse_roster_line("ID: 7001 Name: 测试玩家Team: Alpha"),
        Some(RosterLine::Player(entry(
            7001,
            Some("测试玩家"),
            Team::Alpha
        )))
    );
    assert_eq!(
        parse_roster_line("ID: 7 Name: Team: Bravo"),
        Some(RosterLine::Player(entry(7, None, Team::Bravo)))
    );
}

#[test]
fn roster_lines_that_cannot_be_trusted_are_rejected() {
    for line in [
        "",
        "ID: , Name: x, Team: Alpha",
        "ID: -, Name: x, Team: Alpha",
        "ID: --3, Name: x, Team: Alpha",
        "ID: 12, Name: x",
        "ID: -12, Name: Bot 4",
        "ID: 12, Name: x, Team: Charlie",
        "Name: x, Team: Alpha",
        "IDs: 12, Name: x, Team: Alpha",
    ] {
        assert_eq!(parse_roster_line(line), None, "{line:?}");
    }
}

#[test]
fn ai_players_are_skipped_without_ending_the_list() {
    assert_eq!(
        parse_roster_line("ID: -12, Name: Bot 4, Team: Alpha"),
        Some(RosterLine::Ai)
    );
    // 遭遇战与合作模式：AI 夹在真人中间，后面的真人不能丢
    let text = "[2026-11-16 18:53:01.979]\n\
Log: Player list: \n\
ID: 101, Name: Ash, Team: Alpha\n\
ID: -12, Name: Bot 4, Team: Alpha\n\
ID: -3, Name: Bot 1, Team: Alpha\n\
ID: 102, Name: Birch, Team: Alpha\n\
ID: -15, Name: Bot 4, Team: Bravo\n\
\n";
    assert_eq!(
        events(text),
        vec![LogEvent::PlayerList {
            players: vec![
                entry(101, Some("Ash"), Team::Alpha),
                entry(102, Some("Birch"), Team::Alpha),
            ]
        }]
    );
    // 只有 AI 的名单不产生事件
    let bots_only =
        "[2026-11-16 18:53:01.979]\nLog: Player list: \nID: -5, Name: Bot 0, Team: Bravo\n\n";
    assert_eq!(events(bots_only), vec![]);
}

#[test]
fn player_list_is_flushed_at_end_of_input_and_by_the_next_record() {
    // 文件在名单中途结束：flush 之后仍然得到已读到的部分
    let cut = "[2026-11-16 18:53:01.979]\nLog: Player list: \nID: 1, Name: A, Team: Alpha\nID: 2, Name: B, Team: Bravo";
    assert_eq!(
        events(cut),
        vec![LogEvent::PlayerList {
            players: vec![
                entry(1, Some("A"), Team::Alpha),
                entry(2, Some("B"), Team::Bravo)
            ]
        }]
    );

    // 名单后面没有空行，直接是下一条记录
    let tight = "[2026-11-16 18:53:01.979]\nLog: Player list: \nID: 1, Name: A, Team: Alpha\n[2026-11-16 18:53:02.000]\nLog: Scenario controller loaded\n\n";
    assert_eq!(
        events(tight),
        vec![
            LogEvent::PlayerList {
                players: vec![entry(1, Some("A"), Team::Alpha)]
            },
            LogEvent::ScenarioLoaded,
        ]
    );
}

#[test]
fn empty_player_list_produces_no_event() {
    assert_eq!(
        events("[2026-11-16 18:53:01.979]\nLog: Player list: \n\n"),
        vec![]
    );
}

#[test]
fn pending_state_is_visible_while_a_list_is_incomplete() {
    let mut parser = LogParser::new();
    let mut out = Vec::new();
    for line in ["[2026-11-16 18:53:01.979]", "Log: Player list: "] {
        parser.push_line(line, &mut out);
    }
    assert!(!parser.has_pending(), "还没有任何名单行");
    parser.push_line("ID: 1, Name: A, Team: Alpha", &mut out);
    assert!(parser.has_pending());
    assert!(out.is_empty());
    parser.push_line("", &mut out);
    assert!(!parser.has_pending());
    assert_eq!(out.len(), 1);
}

#[test]
fn room_client_names_may_contain_separators() {
    assert_eq!(
        one("Log: Room: (WaitingRoom|1), Client: (Ash|101)"),
        Some(LogEvent::RoomClient {
            room: RoomKind::WaitingRoom,
            player: PlayerId(101),
            name: Some("Ash".into()),
        })
    );
    assert_eq!(
        one("Log: Room: (GameRoom|2), Client: (a|b), Client: (c)|77)"),
        Some(LogEvent::RoomClient {
            room: RoomKind::GameRoom,
            player: PlayerId(77),
            name: Some("a|b), Client: (c)".into()),
        })
    );
    assert_eq!(
        one("Log: Room: (WaitingRoom|1), Client: (|9)"),
        Some(LogEvent::RoomClient {
            room: RoomKind::WaitingRoom,
            player: PlayerId(9),
            name: None,
        })
    );
    assert_eq!(one("Log: Room: (Lobby|1), Client: (x|9)"), None);
    assert_eq!(one("Log: Room: (WaitingRoom|1), Client: (x|nine)"), None);
}

#[test]
fn loading_line_gives_map_and_scenario() {
    assert_eq!(
        one(
            "Log: Start loading battle... map: Central Village, scenario: - Central Village_network"
        ),
        Some(LogEvent::LoadingStarted {
            map: "Central Village".into(),
            scenario: Some("Central Village_network".into()),
        })
    );
    assert_eq!(
        one("Log: Start loading battle... map: Airport, scenario: - （1-3p）机场爽图"),
        Some(LogEvent::LoadingStarted {
            map: "Airport".into(),
            scenario: Some("（1-3p）机场爽图".into()),
        })
    );
    assert_eq!(
        one("Log: Start loading battle... map: Dam"),
        Some(LogEvent::LoadingStarted {
            map: "Dam".into(),
            scenario: None,
        })
    );
}

#[test]
fn fight_info_gives_the_local_player_name() {
    assert_eq!(
        one("Log: FID:9000001 v1.2.2.1 - Starling (2026-11-17 02:52)"),
        Some(LogEvent::FightInfo {
            match_id: MatchId(9_000_001),
            player_name: Some("Starling".into()),
        })
    );
    // 名字里带括号和连字符
    assert_eq!(
        one("Log: FID:5 v1.2.2 - a - b (c) (2026-11-17 02:52)"),
        Some(LogEvent::FightInfo {
            match_id: MatchId(5),
            player_name: Some("a - b (c)".into()),
        })
    );
    assert_eq!(
        one("Log: FID:5 v1.2.2 -  (2026-11-17 02:52)"),
        Some(LogEvent::FightInfo {
            match_id: MatchId(5),
            player_name: None,
        })
    );
    // 另一种以 SID 开头的行不是对局信息
    assert_eq!(one("Log: SID:1000001 v1.2.2.1"), None);
}

#[test]
fn single_line_markers() {
    assert_eq!(
        one("Log: GetPersonaName   Starling"),
        Some(LogEvent::Persona {
            name: "Starling".into()
        })
    );
    assert_eq!(
        one("Log: Deck set to: _my+deck"),
        Some(LogEvent::DeckSelected {
            name: Some("_my+deck".into())
        })
    );
    assert_eq!(
        one("Log: Deck set to: null"),
        Some(LogEvent::DeckSelected { name: None })
    );
    assert_eq!(
        one("Log: Start reconnect to battle"),
        Some(LogEvent::ReconnectStarted)
    );
    assert_eq!(
        one("Log: Scenario controller loaded"),
        Some(LogEvent::ScenarioLoaded)
    );
}

#[test]
fn similar_looking_lines_are_not_events() {
    for line in [
        "Log: Scenario controller start post load",
        "Log: Scenario controller end post load",
        "Log: Scenario controller wait for all clients connected",
        "Log: Scenario controller loaded twice",
        "Log: Request reconnect data",
        "Log: Client name - Ash",
        "Log: GalaxyNetworkController-> Connect",
        "Warning: Scenario controller loaded",
        "Error: Start reconnect to battle",
        "Log: ",
        "Log",
        "Scenario controller loaded",
    ] {
        assert_eq!(one(line), None, "{line:?}");
    }
}

#[test]
fn game_version_comes_from_the_startup_record() {
    let text = "[2026-11-16 18:31:26.199]\n\
Log: BrokenArrow startup info:\n\
Version: 1.2.2.1\n\
Platform: WindowsPlayer\n\
Market: Steam\n\
\n\
\n\
[2026-11-16 18:31:26.199]\n\
Log: [service] reg service SettingsService.\n\
\n";
    assert_eq!(
        events(text),
        vec![LogEvent::GameVersion {
            version: "1.2.2.1".into()
        }]
    );
}

#[test]
fn continuation_lines_of_other_records_are_never_read_as_messages() {
    // 多行警告的后续行即使长得像事件也不算数
    let text = "[2026-11-16 18:55:07.942]\n\
Warning: Has death component: Entity: Entity 1:2645.0, IsAlive: True, \n\
Log: Scenario controller loaded\n\
ID: 5, Name: ghost, Team: Alpha\n\
\n";
    assert_eq!(events(text), vec![]);
}

#[test]
fn text_before_the_first_timestamp_is_ignored() {
    let text = "Log: Scenario controller loaded\n\nID: 5, Name: ghost, Team: Alpha\n";
    assert_eq!(events(text), vec![]);
}

#[test]
fn carriage_returns_are_tolerated() {
    let text = "[2026-11-16 18:52:05.319]\r\nLog: Scenario controller loaded\r\n\r\n";
    assert_eq!(events(text), vec![LogEvent::ScenarioLoaded]);
}

#[test]
fn a_whole_match_parses_in_order_with_times() {
    let text = [
        "[2026-11-16 18:52:05.318]\nLog: [NET_ROOM] Network lifecycle | Stage=Game | Action=Entered | Route=Direct | RoomType=WaitingRoom | FightId=9000001 | Status=WaitClients\n",
        "[2026-11-16 18:52:05.319]\nLog: [NET_ROOM] WaitingRoom entered, fight id: 9000001\n",
        "[2026-11-16 18:52:05.319]\nLog: Deck set to: _my+deck\n",
        "[2026-11-16 18:52:52.324]\nLog: Start loading battle... map: Airport, scenario: - Airport_network\n",
        "[2026-11-16 18:53:00.733]\nLog: FID:9000001 v1.2.2.1 - Starling (2026-11-17 02:52)\n",
        "[2026-11-16 18:53:01.977]\nLog: [NET_ROOM] Network lifecycle | Stage=Game | Action=Exited | Cause=ServerExitInstance | Reason=none | RoomType=WaitingRoom | FightId=9000001 | Status=WaitClients\n",
        "[2026-11-16 18:53:01.978]\nLog: [NET_ROOM] WaitingRoom exited\n",
        "[2026-11-16 18:53:01.978]\nLog: [NET_ROOM] Network lifecycle | Stage=Game | Action=Entered | RoomType=GameRoom | FightId=9000001 | Status=WaitClients\n",
        "[2026-11-16 18:53:01.978]\nLog: [NET_ROOM] GameRoom entered, fight id: 9000001\n",
        "[2026-11-16 18:53:01.979]\nLog: Player list: \nID: 101, Name: Ash, Team: Alpha\nID: 202, Name: Starling, Team: Bravo\n",
        "[2026-11-16 18:53:33.861]\nLog: Scenario controller loaded\n",
        "[2026-11-16 19:06:20.100]\nLog: [NET_ROOM] Network traffic history | Cause=MatchEnded | FightId=9000001 | Samples=27\n",
        "[2026-11-16 19:06:45.200]\nLog: [NET_ROOM] Network lifecycle | Stage=Game | Action=Exited | Cause=MatchEnded | Reason=none | RoomType=GameRoom | FightId=9000001 | Status=End\n",
        "[2026-11-16 19:06:45.300]\nLog: Deck set to: null\n",
    ]
    .join("\n");

    let parsed = parse_all(&text);
    let kinds: Vec<&str> = parsed
        .iter()
        .map(|e| match &e.event {
            LogEvent::RoomEntered { .. } => "entered",
            LogEvent::RoomExited { .. } => "exited",
            LogEvent::DeckSelected { .. } => "deck",
            LogEvent::LoadingStarted { .. } => "loading",
            LogEvent::FightInfo { .. } => "fight",
            LogEvent::PlayerList { .. } => "list",
            LogEvent::ScenarioLoaded => "loaded",
            LogEvent::MatchEnded { .. } => "ended",
            _ => "other",
        })
        .collect();
    assert_eq!(
        kinds,
        [
            "entered", "entered", "deck", "loading", "fight", "exited", "exited", "entered",
            "entered", "list", "loaded", "ended", "exited", "deck"
        ]
    );
    let first = parsed.first().expect("有事件").at;
    let loaded = parsed
        .iter()
        .find(|e| e.event == LogEvent::ScenarioLoaded)
        .expect("有场景加载完成")
        .at;
    assert_eq!(loaded.millis_since(first), 88_543);
}
