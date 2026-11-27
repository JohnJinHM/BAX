use super::*;
use crate::pregame::{Candidate, Message, Observation, PregamePlayer, PregameView, ticket};
use bax_core::{RoomKind, RoomStatus, RosterEntry};

fn setup(dir: &std::path::Path) -> Session {
    let config = EngineConfig::new(dir);
    let (snapshot_tx, _) = watch::channel(Arc::new(Snapshot::initial()));
    let (_, report_rx) = mpsc::unbounded_channel();
    let mut session = Session::new(Parts {
        client: BatraceClient::new(config.api.clone()).expect("client"),
        config,
        clock: Arc::new(crate::ManualClock::new(Timestamp::from_millis(1000))),
        store: Store::open_in_memory().expect("store"),
        log_dir: Some(dir.to_owned()),
        self_hint: None,
        rules: vec![],
        ui: BTreeMap::new(),
        snapshot_tx,
        report_notify: Arc::new(Notify::new()),
        report_rx,
    });
    session.log.ready = true;
    enter(&mut session, 7001);
    session.publish();
    session
}
fn enter(s: &mut Session, id: u64) {
    let updates = s.tracker.apply(
        Timestamp::from_millis(1000),
        &LogEvent::RoomEntered {
            room: RoomKind::WaitingRoom,
            match_id: MatchId(id),
            status: Some(RoomStatus::WaitClients),
        },
    );
    s.handle_updates(updates, false);
}

#[test]
#[cfg(feature = "ocr")]
fn pve_cancels_ocr_and_late_candidates_and_next_pvp_resumes() {
    let dir = tempfile::tempdir().expect("验证数据");
    let mut s = setup(dir.path());
    let stale = message(&s);
    s.tracker.apply(
        Timestamp::from_millis(2000),
        &LogEvent::LoadingStarted {
            map: "Example".into(),
            scenario: Some("Example_Skirmish".into()),
        },
    );
    assert_eq!(
        s.build_snapshot().current.expect("验证数据").mode,
        crate::MatchMode::Pve
    );
    assert!(ticket(&s.build_snapshot()).is_none());
    s.on_pregame(stale);
    assert!(
        s.build_snapshot()
            .current
            .expect("验证数据")
            .pregame
            .players
            .is_empty()
    );
    enter(&mut s, 7002);
    assert!(ticket(&s.build_snapshot()).is_some());
    s.tracker
        .apply(Timestamp::from_millis(3000), &LogEvent::AiDetected);
    assert!(ticket(&s.build_snapshot()).is_none());
    assert!(!s.ui.contains_key("ui.ocr"), "只暂停本局，不修改用户开关");
}
fn message(s: &Session) -> Message {
    Message {
        ticket: ticket(&s.build_snapshot()).expect("eligible"),
        revision: s.revision,
        view: PregameView {
            status: "候选".into(),
            queries_complete: true,
            players: vec![PregamePlayer {
                trace_count: None,
                observed: Observation {
                    side: "left".into(),
                    row: 0,
                    country: Some(1),
                    friendly: Some(false),
                    name: "Raven".into(),
                    level: 57,
                    alternatives: vec![],
                },
                candidates: vec![Candidate {
                    id: PlayerId(101),
                    name: "Raven".into(),
                    level: Some(57),
                    local: true,
                    profile: None,
                }],
                ambiguous: false,
            }],
        },
    }
}
#[test]
fn log_identity_replaces_candidate_without_polluting_history() {
    let dir = tempfile::tempdir().expect("dir");
    let mut s = setup(dir.path());
    let late = message(&s);
    s.on_pregame(message(&s));
    assert_eq!(
        s.build_snapshot()
            .current
            .expect("match")
            .pregame
            .players
            .len(),
        1
    );
    assert!(
        s.store
            .match_players(MatchId(7001))
            .expect("read")
            .is_empty()
    );
    assert!(s.store.player(PlayerId(101)).expect("read").is_none());
    let updates = s.tracker.apply(
        Timestamp::from_millis(2000),
        &LogEvent::RoomEntered {
            room: RoomKind::GameRoom,
            match_id: MatchId(7001),
            status: Some(RoomStatus::WaitClients),
        },
    );
    s.handle_updates(updates, false);
    let updates = s.tracker.apply(
        Timestamp::from_millis(2100),
        &LogEvent::PlayerList {
            players: vec![RosterEntry {
                id: PlayerId(202),
                name: Some("Raven".into()),
                team: Team::Alpha,
            }],
        },
    );
    s.handle_updates(updates, false);
    s.on_pregame(late);
    let current = s.build_snapshot().current.expect("match");
    assert!(current.roster_known);
    assert!(current.pregame.players.is_empty());
    assert_eq!(current.players[0].id, PlayerId(202));
    assert_eq!(current.players[0].country, None, "同名不同账号不沿用国旗");
    assert!(s.store.player(PlayerId(101)).expect("read").is_none());
    assert_eq!(
        s.store.match_players(MatchId(7001)).expect("read")[0].player,
        PlayerId(202)
    );
}

#[tokio::test]
async fn matched_ocr_row_refreshes_cached_marks_and_saves_them_without_an_encounter() {
    use wiremock::{Mock, MockServer, ResponseTemplate, matchers::path};
    let dir = tempfile::tempdir().expect("dir");
    let mut session = setup(dir.path());
    // Drive the waiting-room state directly, without filesystem events.
    session.log_rx = mpsc::channel(1).1;
    let server = MockServer::start().await;
    Mock::given(path("/api/players/101/trace"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"traceCount": 0})),
        )
        .expect(1)
        .mount(&server)
        .await;
    session.client = BatraceClient::new(bax_batrace::ClientConfig {
        base_url: server.uri(),
        upstream: None,
        ..Default::default()
    })
    .expect("client");
    let id = PlayerId(101);
    session
        .store
        .save_trace_count(id, 3, Timestamp::from_millis(1))
        .expect("old count");
    let mut roster = message(&session);
    let mut ambiguous = roster.view.players[0].clone();
    ambiguous.candidates[0].id = PlayerId(102);
    ambiguous.ambiguous = true;
    roster.view.players.push(ambiguous);
    session.on_pregame(roster);
    assert_eq!(session.pregame.players[0].trace_count, Some(3));
    let mut snapshots = session.snapshot_tx.subscribe();
    let (commands, receiver) = mpsc::channel(2);
    let task = tokio::spawn(session.run(receiver));
    tokio::time::timeout(
        Duration::from_secs(3),
        snapshots.wait_for(|s| {
            s.current.as_ref().is_some_and(|m| {
                m.pregame.players.len() == 2
                    && m.pregame.players[0].trace_count == Some(0)
                    && m.pregame.players[1].trace_count.is_none()
            })
        }),
    )
    .await
    .expect("trace refresh")
    .expect("snapshot");
    let (done, read) = oneshot::channel();
    commands
        .send(Command::PlayerDetail { id, done })
        .await
        .expect("command");
    let detail = read.await.expect("reply").expect("db").expect("player");
    assert_eq!(detail.trace_count, Some(0));
    assert_eq!(detail.encounters.total, 0);
    assert!(detail.fetched_at.is_none());
    drop(commands);
    task.await.expect("shutdown");
    assert_eq!(server.received_requests().await.expect("requests").len(), 1);
}
#[tokio::test]
async fn temporary_trace_failure_retries_without_blocking_other_ocr_players() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use wiremock::{Mock, MockServer, Request, ResponseTemplate, matchers::path};
    let dir = tempfile::tempdir().expect("日志目录");
    let mut s = setup(dir.path());
    s.log_rx = mpsc::channel(1).1;
    let clock = crate::ManualClock::new(Timestamp::from_secs(20));
    s.clock = Arc::new(clock.clone());
    s.config.tick_interval = Duration::from_millis(10);
    let server = MockServer::start().await;
    let attempts = AtomicUsize::new(0);
    Mock::given(path("/api/players/101/trace"))
        .respond_with(move |_: &Request| {
            if attempts.fetch_add(1, Ordering::SeqCst) == 0 {
                ResponseTemplate::new(503)
            } else {
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"traceCount": 2}))
            }
        })
        .expect(2)
        .mount(&server)
        .await;
    Mock::given(path("/api/players/102/trace"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"traceCount": 0})),
        )
        .expect(1)
        .mount(&server)
        .await;
    s.client = BatraceClient::new(bax_batrace::ClientConfig {
        base_url: server.uri(),
        upstream: None,
        ..Default::default()
    })
    .expect("客户端");
    let mut rows = message(&s);
    let mut second = rows.view.players[0].clone();
    second.observed.row = 1;
    second.candidates[0].id = PlayerId(102);
    rows.view.players.push(second);
    s.on_pregame(rows);
    let mut snapshots = s.snapshot_tx.subscribe();
    let (commands, receiver) = mpsc::channel(1);
    let task = tokio::spawn(s.run(receiver));
    tokio::time::timeout(Duration::from_secs(4), async {
        snapshots
            .wait_for(|s| {
                s.current.as_ref().is_some_and(|m| {
                    m.pregame.players.len() == 2
                        && m.pregame.players[0].trace_count.is_none()
                        && m.pregame.players[1].trace_count == Some(0)
                })
            })
            .await
            .expect("其他玩家先收到结果");
        clock.advance_millis(60_001);
        snapshots
            .wait_for(|s| {
                s.current
                    .as_ref()
                    .is_some_and(|m| m.pregame.players[0].trace_count == Some(2))
            })
            .await
            .expect("冷却后重新取得标记");
    })
    .await
    .expect("标记补查");
    drop(commands);
    task.await.expect("会话退出");
}

#[tokio::test]
async fn log_rotation_and_confirmed_roster_keep_the_inflight_mark_query() {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    let dir = tempfile::tempdir().expect("日志目录");
    let mut s = setup(dir.path());
    let (logs, rx) = mpsc::channel(4);
    s.log_rx = rx;
    // 隔离正式名单的资料请求，确保标记只能来自正在进行的 OCR 请求。
    s.paused_until = Some(Timestamp::from_secs(3600));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("监听");
    s.client = BatraceClient::new(bax_batrace::ClientConfig {
        base_url: format!("http://{}", listener.local_addr().expect("地址")),
        upstream: None,
        ..Default::default()
    })
    .expect("客户端");
    let (started, received) = oneshot::channel();
    let (respond, ready) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("连接");
        let mut lines = BufReader::new(&mut stream).lines();
        assert!(
            lines
                .next_line()
                .await
                .expect("读请求")
                .expect("请求行")
                .starts_with("GET /api/players/101/trace ")
        );
        while lines
            .next_line()
            .await
            .expect("读请求头")
            .is_some_and(|line| !line.is_empty())
        {}
        started.send(()).expect("收到请求");
        ready.await.expect("正式名单已到");
        stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"traceCount\":4}").await.expect("响应");
    });
    let pending = message(&s);
    s.on_log(LogMsg::Item(WatchItem::FileOpened {
        path: dir.path().join("next.log"),
        origin: Origin::Live,
    }));
    s.on_pregame(pending);
    assert_eq!(s.pregame.players.len(), 1, "轮换前发出的候选仍属于本局");
    let mut snapshots = s.snapshot_tx.subscribe();
    let (commands, receiver) = mpsc::channel(1);
    let task = tokio::spawn(s.run(receiver));
    tokio::time::timeout(Duration::from_secs(4), async {
        received.await.expect("标记查询已发出");
        for event in [
            LogEvent::RoomEntered {
                room: RoomKind::GameRoom,
                match_id: MatchId(7001),
                status: Some(RoomStatus::WaitClients),
            },
            LogEvent::PlayerList {
                players: vec![RosterEntry {
                    id: PlayerId(101),
                    name: Some("Raven".into()),
                    team: Team::Alpha,
                }],
            },
        ] {
            logs.send(LogMsg::Item(WatchItem::Event {
                origin: Origin::Live,
                event: bax_gamelog::TimedEvent {
                    at: LogTime::from_parts(2026, 1, 1, 1, 0, 0, 0).expect("时间"),
                    event,
                },
            }))
            .await
            .expect("更新日志");
        }
        snapshots
            .wait_for(|s| {
                s.current
                    .as_ref()
                    .is_some_and(|m| m.roster_known && m.pregame.players.is_empty())
            })
            .await
            .expect("正式名单");
        respond.send(()).expect("返回标记");
        snapshots
            .wait_for(|s| {
                s.current.as_ref().is_some_and(|m| {
                    m.players.iter().any(|p| {
                        p.id == PlayerId(101) && p.country == Some(1) && p.trace_count == Some(4)
                    })
                })
            })
            .await
            .expect("同 ID 的正式玩家收到标记");
    })
    .await
    .expect("名单交接");
    drop(commands);
    task.await.expect("会话退出");
    server.await.expect("测试服务退出");
}

#[test]
fn old_match_and_disabled_results_cannot_return() {
    let dir = tempfile::tempdir().expect("dir");
    let mut s = setup(dir.path());
    let old = message(&s);
    enter(&mut s, 7002);
    s.publish();
    s.on_pregame(old);
    assert!(s.pregame.players.is_empty());
    let old = message(&s);
    let (done, _) = oneshot::channel();
    s.on_command(Command::SetUiSetting {
        key: "ui.ocr".into(),
        value: serde_json::json!({"enabled":false}),
        done,
    });
    assert!(ticket(&s.build_snapshot()).is_none());
    let (done, _) = oneshot::channel();
    s.on_command(Command::SetUiSetting {
        key: "ui.ocr".into(),
        value: serde_json::json!({"enabled":true}),
        done,
    });
    s.on_pregame(old);
    assert!(s.pregame.players.is_empty());
    let updates = s
        .tracker
        .apply(Timestamp::from_millis(3000), &LogEvent::ReconnectStarted);
    s.handle_updates(updates, false);
    let updates = s.tracker.apply(
        Timestamp::from_millis(3100),
        &LogEvent::RoomEntered {
            room: RoomKind::WaitingRoom,
            match_id: MatchId(7003),
            status: Some(RoomStatus::InProcess),
        },
    );
    s.handle_updates(updates, false);
    assert!(ticket(&s.build_snapshot()).is_none());
}
