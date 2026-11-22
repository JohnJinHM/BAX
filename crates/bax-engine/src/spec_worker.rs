//! 身份查询完成后，最多补取五份敌方上一场战报。
use crate::reports::ReportEvent;
use crate::{Clock, Snapshot};
use bax_batrace::BatraceClient;
use bax_core::{Connection, MatchId, Phase, PlayerId, Team, Timestamp};
use bax_store::Store;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, watch};

fn identity(snapshot: &Snapshot) -> Option<(MatchId, Timestamp)> {
    let m = snapshot.current.as_ref()?;
    (snapshot.log.ready
        && m.phase != Phase::Ended
        && m.mode != crate::MatchMode::Pve
        && m.connection == Connection::Connected)
        .then_some((m.match_id, m.discovered_at))
}

fn targets(snapshot: &Snapshot) -> Vec<(PlayerId, MatchId)> {
    let Some(m) = &snapshot.current else {
        return vec![];
    };
    if snapshot.api.in_flight + snapshot.api.queued > 0 || snapshot.api.paused_until.is_some() {
        return vec![];
    }
    if m.roster_known {
        let Some(team @ (Team::Alpha | Team::Bravo)) = m.self_team else {
            return vec![];
        };
        m.players
            .iter()
            .filter(|p| {
                matches!(p.team, Some(Team::Alpha | Team::Bravo))
                    && p.team != Some(team)
                    && !p.is_self
            })
            .filter_map(|p| Some((p.id, p.profile.as_ref()?.recent_match_id?)))
            .filter(|(_, id)| *id != m.match_id)
            .collect()
    } else {
        let enemies: Vec<_> = m
            .pregame
            .players
            .iter()
            .filter(|p| p.observed.friendly == Some(false))
            .collect();
        // 单个玩家未匹配成功，不阻塞其他已完成的身份。
        if !m.pregame.queries_complete {
            return vec![];
        }
        enemies
            .into_iter()
            .filter(|p| !p.ambiguous)
            .filter_map(|p| {
                let c = p.candidates.first()?;
                Some((c.id, c.profile.as_ref()?.recent_match_id?))
            })
            .filter(|(_, id)| *id != m.match_id)
            .collect()
    }
}

#[derive(Default)]
struct Budget {
    match_key: Option<(MatchId, Timestamp)>,
    attempted: HashSet<MatchId>,
    requests: u8,
    stopped: bool,
}
impl Budget {
    fn select(&mut self, key: (MatchId, Timestamp)) {
        if self.match_key != Some(key) {
            *self = Self {
                match_key: Some(key),
                ..Self::default()
            };
        }
    }
    fn take(&mut self, id: MatchId, cached: bool) -> bool {
        if self.stopped || self.attempted.contains(&id) {
            return false;
        }
        if cached {
            self.attempted.insert(id);
            return false;
        }
        if self.requests >= 5 {
            return false;
        }
        self.attempted.insert(id);
        self.requests += 1;
        true
    }
}

pub(crate) async fn run(
    mut store: Store,
    client: BatraceClient,
    clock: Arc<dyn Clock>,
    mut snapshots: watch::Receiver<Arc<Snapshot>>,
    events: mpsc::UnboundedSender<ReportEvent>,
) {
    let mut budget = Budget::default();
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = interval.tick() => {},
            result = snapshots.changed() => { if result.is_err() { break; } continue; }
        }
        let snapshot = snapshots.borrow().clone();
        let Some(key) = identity(&snapshot) else {
            continue;
        };
        budget.select(key);
        let target = targets(&snapshot)
            .into_iter()
            .find(|(_, id)| budget.take(*id, store.has_report(*id).unwrap_or(false)));
        let Some((player, match_id)) = target else {
            continue;
        };
        let request = client.match_report(match_id);
        tokio::pin!(request);
        let response = loop {
            tokio::select! {
                result = &mut request => break Some(result),
                changed = snapshots.changed() => {
                    if changed.is_err() || identity(&snapshots.borrow()) != Some(key) { break None; }
                }
            }
        };
        match response {
            Some(Ok(fetched)) => {
                if fetched.data.players().any(|(id, _)| id == player)
                    && store
                        .save_report(
                            match_id,
                            &fetched.raw,
                            crate::postmatch::report_meta(&fetched.data),
                            clock.now(),
                        )
                        .is_ok()
                {
                    let pending = store.pending_reports().unwrap_or(0);
                    let _ = events.send(ReportEvent {
                        pending,
                        archived: Some(match_id),
                    });
                }
                tracing::debug!(player = %player, match_id = %match_id, requests = budget.requests, "上一场专精战报已获取");
            }
            Some(Err(_)) => {
                budget.stopped = true;
                tracing::info!(requests = budget.requests, "上一场专精查询停止，本场不重试");
            }
            None => {}
        }
        // 以请求完成时刻计间隔，不在慢请求结束后补发累计 tick。
        interval.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FetchState, MatchView, PlayerView, ProfileSummary};

    fn snapshot() -> Snapshot {
        let mut s = Snapshot::initial();
        s.log.ready = true;
        let players = (101..=106)
            .map(|id| PlayerView {
                id: PlayerId(id),
                name: None,
                log_name: None,
                team: Some(if id == 101 { Team::Alpha } else { Team::Bravo }),
                is_self: id == 101,
                profile: ProfileSummary::from_json(&format!(
                    r#"{{"info":{{"id":{id}}},"last_fights_data":["{}"]}}"#,
                    900100 + id
                )),
                profile_fetched_at: None,
                profile_current: true,
                fetch: FetchState::Idle,
                encounters: Default::default(),
                note: None,
            })
            .collect();
        s.current = Some(MatchView {
            mode: crate::MatchMode::Unknown,
            pregame: Default::default(),
            match_id: MatchId(900001),
            phase: Phase::Deployment,
            connection: Connection::Connected,
            rejoined: false,
            map: None,
            scenario: None,
            deck: None,
            clock: None,
            discovered_at: Timestamp::from_millis(1),
            ended_at: None,
            self_team: Some(Team::Alpha),
            roster_known: true,
            players,
            teams: vec![],
        });
        s
    }

    #[test]
    fn only_resolved_enemies_after_identity_requests_are_selected() {
        let mut s = snapshot();
        assert_eq!(targets(&s).len(), 5);
        assert!(targets(&s).iter().all(|(id, _)| *id != PlayerId(101)));
        s.api.in_flight = 1;
        assert!(targets(&s).is_empty());
        s.api.in_flight = 0;
        s.current.as_mut().expect("match").self_team = None;
        assert!(targets(&s).is_empty());
        let mut s = snapshot();
        s.current.as_mut().expect("match").phase = Phase::Ended;
        assert!(identity(&s).is_none());
        let mut s = snapshot();
        s.current.as_mut().expect("match").roster_known = false;
        assert!(targets(&s).is_empty());
        for n in 0..5 {
            s.current.as_mut().expect("match").pregame.players.push(
                crate::pregame::PregamePlayer {
                    observed: crate::pregame::Observation {
                        side: "right".into(),
                        row: n,
                        friendly: Some(false),
                        name: "Raven".into(),
                        level: 50,
                        alternatives: vec![],
                    },
                    candidates: vec![crate::pregame::Candidate {
                        id: PlayerId(102 + n as u64),
                        name: "Raven".into(),
                        level: Some(50),
                        local: true,
                        profile: ProfileSummary::from_json(
                            r#"{"info":{"id":102},"last_fights_data":["900202"]}"#,
                        ),
                    }],
                    ambiguous: false,
                },
            );
        }
        assert!(targets(&s).is_empty());
        s.current.as_mut().expect("match").pregame.queries_complete = true;
        assert_eq!(targets(&s).len(), 5);
        s.current.as_mut().expect("match").pregame.players[0].ambiguous = true;
        assert_eq!(targets(&s).len(), 4);
        s.current.as_mut().expect("match").pregame.players[1].candidates[0].profile = None;
        assert_eq!(targets(&s).len(), 3);
    }

    #[tokio::test]
    async fn throttling_stops_extra_requests_without_retries() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/match"))
            .respond_with(ResponseTemplate::new(429))
            .expect(1)
            .mount(&server)
            .await;
        let client = BatraceClient::new(bax_batrace::ClientConfig {
            upstream: None,
            base_url: server.uri(),
            ..Default::default()
        })
        .expect("client");
        let (tx, rx) = watch::channel(Arc::new(snapshot()));
        let (events, _) = mpsc::unbounded_channel();
        let task = tokio::spawn(run(
            Store::open_in_memory().expect("store"),
            client,
            Arc::new(crate::ManualClock::new(Timestamp::from_millis(1))),
            rx,
            events,
        ));
        tokio::time::sleep(Duration::from_millis(2200)).await;
        drop(tx);
        task.await.expect("worker");
        server.verify().await;
    }

    #[tokio::test]
    async fn shared_latest_match_is_fetched_once_and_archived() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/match"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "matchInfo":{"StartTime":1000,"Data":{"102":{"Id":102}}}
            })))
            .expect(1)
            .mount(&server)
            .await;
        let client = BatraceClient::new(bax_batrace::ClientConfig {
            upstream: None,
            base_url: server.uri(),
            ..Default::default()
        })
        .expect("client");
        let mut s = snapshot();
        for p in &mut s.current.as_mut().expect("match").players {
            p.profile.as_mut().expect("profile").recent_match_id = Some(MatchId(900100));
        }
        let (tx, rx) = watch::channel(Arc::new(s));
        let (events, mut observed) = mpsc::unbounded_channel();
        let dir = tempfile::tempdir().expect("dir");
        let db = dir.path().join("store.db");
        let task = tokio::spawn(run(
            Store::open(&db).expect("store"),
            client,
            Arc::new(crate::ManualClock::new(Timestamp::from_millis(1))),
            rx,
            events,
        ));
        let event = tokio::time::timeout(Duration::from_secs(3), observed.recv())
            .await
            .expect("event")
            .expect("archived");
        assert_eq!(event.archived, Some(MatchId(900100)));
        assert!(
            Store::open(&db)
                .expect("reader")
                .has_report(MatchId(900100))
                .expect("cached")
        );
        tokio::time::sleep(Duration::from_millis(1200)).await;
        drop(tx);
        task.await.expect("worker");
        server.verify().await;
    }
    #[test]
    fn budget_deduplicates_caches_caps_and_survives_roster_transition() {
        let mut budget = Budget::default();
        let key = (MatchId(900001), Timestamp::from_millis(1));
        budget.select(key);
        assert!(!budget.take(MatchId(1), true));
        for id in 2..7 {
            assert!(budget.take(MatchId(id), false));
        }
        budget.select(key);
        assert!(!budget.take(MatchId(2), false));
        assert!(!budget.take(MatchId(7), false));
        budget.select((MatchId(900002), Timestamp::from_millis(2)));
        assert!(budget.take(MatchId(7), false));
        budget.stopped = true;
        assert!(!budget.take(MatchId(8), false));
    }
}
