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
