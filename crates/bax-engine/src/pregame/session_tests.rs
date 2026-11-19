use super::*;
use crate::pregame::{Candidate, Observation, PregamePlayer, PregameView, ticket, worker::Message};
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
fn message(s: &Session) -> Message {
    Message {
        ticket: ticket(&s.build_snapshot()).expect("eligible"),
        revision: s.revision,
        view: PregameView {
            status: "候选".into(),
            players: vec![PregamePlayer {
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
