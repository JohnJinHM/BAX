//! 引擎的端到端测试：真实的日志文件、本地模拟的 BATrace、手动拨动的时钟。

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use bax_batrace::{ClientConfig, ErrorKind};
use bax_core::{
    BattleClock, ClockSource, LogEvent, MatchId, Phase, PlayerId, ReminderOutcome, ReminderRule,
    RoomKind, RoomStatus, RosterEntry, Team, Timestamp, Tracker,
};
use bax_engine::{
    CommandError, Engine, EngineConfig, FetchState, ManualClock, PlayerView, ReportState, Snapshot,
};
use bax_gamelog::{LogTime, LogZone};
use bax_store::{ProfileUpdate, Relation, Store};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

const LOG_FILE: &str = "Gamelog__2026_11_16__18_51.log";
const UTC: LogZone = LogZone::Fixed { offset_seconds: 0 };
const ME: u64 = 500;

/// 按玩家 ID 生成资料：名字 `API-<id>`，ELO 为 1000 + ID。
#[derive(Debug, Clone, Default)]
struct ProfileResponder {
    delay: Duration,
}

impl Respond for ProfileResponder {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let id: u64 = request
            .url
            .query_pairs()
            .find(|(k, _)| k == "stbid")
            .and_then(|(_, v)| v.parse().ok())
            .unwrap_or(0);
        let body = serde_json::json!({
            "info": {
                "id": id,
                "name": format!("API-{id}"),
                "level": 10,
                "rating": 1000.0 + id as f64,
                "rank": 5,
                "ratingGames": 20
            },
            "statInfo": {
                "fightsCountRt": 20,
                "winCountRt": 12,
                "lossCountRt": 8,
                "leaveCountRt": 0,
                "unitsKilledCountRt": 300,
                "unitsLostCountRt": 200
            },
            "last_fights_data": []
        });
        ResponseTemplate::new(200)
            .set_body_raw(body.to_string(), "application/json")
            .set_delay(self.delay)
    }
}

struct Fixture {
    dir: tempfile::TempDir,
    logs: PathBuf,
    server: MockServer,
    clock: ManualClock,
}

impl Fixture {
    async fn new() -> Fixture {
        let dir = tempfile::tempdir().expect("临时目录");
        let logs = dir.path().join("GameLogs");
        std::fs::create_dir_all(&logs).expect("建日志目录");
        Fixture {
            dir,
            logs,
            server: MockServer::start().await,
            clock: ManualClock::new(ts("18:55:00.000")),
        }
    }

    async fn serve_profiles(&self) {
        Mock::given(method("GET"))
            .and(path("/api/players/info"))
            .respond_with(ProfileResponder::default())
            .mount(&self.server)
            .await;
    }

    fn store_path(&self) -> PathBuf {
        bax_engine::store_path(self.dir.path())
    }

    fn config(&self) -> EngineConfig {
        let mut config = EngineConfig::new(self.dir.path());
        config.log_dir = Some(self.logs.clone());
        config.api = ClientConfig {
            base_url: self.server.uri(),
            timeout: Duration::from_secs(3),
            player_info_timeout: Duration::from_secs(3),
            ..ClientConfig::default()
        };
        config.log_zone = UTC;
        config.log_poll_interval = Duration::from_millis(20);
        config.tick_interval = Duration::from_millis(20);
        config.retry_delays = vec![Duration::from_secs(5)];
        config
    }

    fn start(&self) -> Engine {
        self.start_with(self.config())
    }

    fn start_with(&self, config: EngineConfig) -> Engine {
        Engine::start(config, Arc::new(self.clock.clone())).expect("引擎启动")
    }

    fn append(&self, text: &str) {
        append_to(&self.logs.join(LOG_FILE), text);
    }

    /// 每名玩家被请求了几次，按 ID 排序。
    async fn requests(&self) -> Vec<(u64, usize)> {
        let mut counts = std::collections::BTreeMap::new();
        for request in self.server.received_requests().await.unwrap_or_default() {
            let id: u64 = request
                .url
                .query_pairs()
                .find(|(k, _)| k == "stbid")
                .and_then(|(_, v)| v.parse().ok())
                .unwrap_or(0);
            *counts.entry(id).or_default() += 1;
        }
        counts.into_iter().collect()
    }
}

fn append_to(path: &Path, text: &str) {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .expect("打开日志");
    file.write_all(text.as_bytes()).expect("写入日志");
}

/// 2026-11-16 当天的时刻，按 UTC 换算。
fn ts(time: &str) -> Timestamp {
    LogTime::parse_bracketed(&format!("[2026-11-16 {time}]"))
        .expect("合法时间")
        .to_utc(UTC)
}

fn rec(time: &str, message: &str) -> String {
    format!("[2026-11-16 {time}]\n{message}\n\n")
}

fn entered(id: u64, room: &str, status: &str, time: &str) -> String {
    rec(
        time,
        &format!(
            "Log: [NET_ROOM] Network lifecycle | Stage=Game | Action=Entered | Route=Direct | RoomType={room} | FightId={id} | Status={status}"
        ),
    )
}

fn waiting_room(id: u64, time: &str) -> String {
    entered(id, "WaitingRoom", "WaitClients", time)
        + &rec(
            time,
            &format!("Log: [NET_ROOM] WaitingRoom entered, fight id: {id}"),
        )
}

fn early_client(id: u64, name: &str, time: &str) -> String {
    rec(
        time,
        &format!("Log: Room: (WaitingRoom|1), Client: ({name}|{id})"),
    )
}

fn player_list(time: &str, roster: &[(u64, &str, &str)]) -> String {
    let mut text = format!("[2026-11-16 {time}]\nLog: Player list: \n");
    for (id, name, team) in roster {
        text += &format!("ID: {id}, Name: {name}, Team: {team}\n");
    }
    text + "\n"
}

fn game_room(id: u64, time: &str, roster: &[(u64, &str, &str)]) -> String {
    rec(
        time,
        "Log: Start loading battle... map: Airport, scenario: - Airport_network",
    ) + &rec(
        time,
        &format!("Log: FID:{id} v1.2.2.1 - Me (2026-11-17 02:52)"),
    ) + &entered(id, "GameRoom", "WaitClients", time)
        + &player_list(time, roster)
}

fn scenario_loaded(time: &str) -> String {
    rec(time, "Log: Scenario controller loaded")
}

fn match_ended(id: u64, time: &str) -> String {
    rec(
        time,
        &format!(
            "Log: [NET_ROOM] Network traffic history | Cause=MatchEnded | FightId={id} | Samples=27"
        ),
    )
}

const ROSTER: [(u64, &str, &str); 4] = [
    (101, "Ash", "Alpha"),
    (102, "", "Alpha"),
    (201, "Dune", "Bravo"),
    (ME, "Me", "Bravo"),
];

fn finished_match(id: u64) -> String {
    waiting_room(id, "18:52:05.318")
        + &game_room(id, "18:53:01.979", &ROSTER)
        + &scenario_loaded("18:53:33.861")
        + &match_ended(id, "19:06:20.100")
}

async fn wait_for(
    engine: &Engine,
    what: &str,
    predicate: impl Fn(&Snapshot) -> bool,
) -> Arc<Snapshot> {
    let mut rx = engine.subscribe();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let snapshot = rx.borrow_and_update().clone();
        if predicate(&snapshot) {
            return snapshot;
        }
        if tokio::time::timeout_at(deadline, rx.changed())
            .await
            .is_err()
        {
            panic!("等待超时：{what}\n最后的快照：{snapshot:#?}");
        }
    }
}

fn player(snapshot: &Snapshot, id: u64) -> Option<&PlayerView> {
    snapshot
        .current
        .as_ref()?
        .players
        .iter()
        .find(|p| p.id == PlayerId(id))
}

fn is_current_profile(snapshot: &Snapshot, id: u64) -> bool {
    player(snapshot, id).is_some_and(|p| p.profile_current)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn backfill_of_a_finished_match_makes_no_requests() {
    let fx = Fixture::new().await;
    fx.serve_profiles().await;
    fx.append(&finished_match(9_000_001));

    let engine = fx.start();
    let snapshot = wait_for(&engine, "补读完成", |s| {
        s.log.ready && s.current.is_some()
    })
    .await;
    let m = snapshot.current.as_ref().expect("有当前比赛");
    assert_eq!(m.match_id, MatchId(9_000_001));
    assert_eq!(m.phase, Phase::Ended);
    assert!(m.roster_known);
    assert_eq!(m.map.as_deref(), Some("Airport"));
    assert_eq!(m.self_team, Some(Team::Bravo));
    let names: Vec<Option<&str>> = m.players.iter().map(|p| p.name.as_deref()).collect();
    assert_eq!(names, vec![Some("Ash"), None, Some("Dune"), Some("Me")]);
    assert!(m.players.iter().all(|p| p.fetch == FetchState::Idle));
    assert_eq!(snapshot.self_id, Some(PlayerId(ME)));
    assert!(
        snapshot
            .log
            .file
            .as_deref()
            .is_some_and(|f| f.ends_with(LOG_FILE))
    );

    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(fx.requests().await.is_empty(), "已经结束的历史比赛不发请求");
    engine.shutdown().await;

    let store = Store::open(&fx.store_path()).expect("打开长期库");
    let record = store
        .match_record(MatchId(9_000_001))
        .expect("读取")
        .expect("比赛已记录");
    assert_eq!(record.phase, Phase::Ended);
    assert_eq!(
        store.setting::<PlayerId>("self.playerId").expect("读取"),
        Some(PlayerId(ME)),
        "记住了本机玩家"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_live_match_fetches_each_player_once() {
    let fx = Fixture::new().await;
    fx.serve_profiles().await;
    let engine = fx.start();
    wait_for(&engine, "空目录补读完成", |s| s.log.ready).await;

    // 等待室里先到的两个人立即开始查
    fx.append(
        &(waiting_room(9_000_001, "18:52:05.318")
            + &early_client(101, "Ash", "18:52:20.000")
            + &early_client(201, "Dune", "18:52:21.000")),
    );
    let snapshot = wait_for(&engine, "先到的玩家查完", |s| {
        is_current_profile(s, 101) && is_current_profile(s, 201)
    })
    .await;
    let m = snapshot.current.as_ref().expect("有当前比赛");
    assert!(!m.roster_known);
    assert_eq!(m.phase, Phase::WaitingRoom);

    // 名单到了，只补查剩下的人
    fx.append(&(game_room(9_000_001, "18:53:01.979", &ROSTER) + &scenario_loaded("18:53:33.861")));
    let snapshot = wait_for(&engine, "全部玩家查完并进入对局", |s| {
        ROSTER.iter().all(|(id, _, _)| is_current_profile(s, *id))
            && s.current
                .as_ref()
                .is_some_and(|m| m.phase == Phase::InBattle)
    })
    .await;
    assert_eq!(
        fx.requests().await,
        vec![(101, 1), (102, 1), (201, 1), (ME, 1)],
        "每人只查一次"
    );

    let m = snapshot.current.as_ref().expect("有当前比赛");
    assert_eq!(
        m.clock,
        Some(BattleClock {
            start_at: ts("18:54:33.861"),
            source: ClockSource::LogEstimate
        })
    );
    let ash = player(&snapshot, 101).expect("有 Ash");
    assert_eq!(ash.name.as_deref(), Some("API-101"), "接口资料里的名字优先");
    assert_eq!(ash.log_name.as_deref(), Some("Ash"));
    assert_eq!(ash.team, Some(Team::Alpha));
    let profile = ash.profile.as_ref().expect("有资料");
    assert_eq!(profile.rating, Some(1101.0));
    assert_eq!(
        profile.ranked.as_ref().and_then(|r| r.win_rate),
        Some(12.0 / 20.0)
    );
    assert_eq!(ash.encounters.total, 0);

    let nameless = player(&snapshot, 102).expect("有空名字的玩家");
    assert_eq!(nameless.log_name, None);
    assert_eq!(
        nameless.name.as_deref(),
        Some("API-102"),
        "空名字由接口资料补全"
    );

    assert!(player(&snapshot, ME).is_some_and(|p| p.is_self));
    let alpha = &m.teams[0];
    assert_eq!(alpha.team, Team::Alpha);
    assert_eq!((alpha.players, alpha.rated), (2, 2));
    assert_eq!(alpha.average_rating, Some(1101.5));
    assert_eq!(snapshot.api.in_flight, 0);
    assert_eq!(snapshot.api.last_error, None);
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cached_profiles_show_first_and_failures_are_retried() {
    let fx = Fixture::new().await;
    {
        let mut store = Store::open(&fx.store_path()).expect("建库");
        store
            .save_profile(
                &ProfileUpdate {
                    id: PlayerId(101),
                    name: Some("Old Ash".into()),
                    steam_id: None,
                    level: Some(9),
                    rating: Some(1500.0),
                    rank: None,
                    rating_games: None,
                    raw_json: r#"{"info":{"id":101,"name":"Old Ash","level":9,"rating":1500}}"#
                        .into(),
                },
                ts("12:00:00.000"),
            )
            .expect("写入旧资料");
    }
    Mock::given(method("GET"))
        .and(path("/api/players/info"))
        .and(query_param("stbid", "101"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&fx.server)
        .await;
    fx.serve_profiles().await;

    let engine = fx.start();
    wait_for(&engine, "补读完成", |s| s.log.ready).await;
    fx.append(
        &(waiting_room(9_000_001, "18:52:05.318")
            + &game_room(
                9_000_001,
                "18:53:01.979",
                &[(101, "Ash", "Alpha"), (201, "Dune", "Bravo")],
            )),
    );

    let snapshot = wait_for(&engine, "第一次请求失败", |s| {
        player(s, 101).is_some_and(|p| matches!(p.fetch, FetchState::Failed { .. }))
    })
    .await;
    let ash = player(&snapshot, 101).expect("有 Ash");
    assert_eq!(
        ash.fetch,
        FetchState::Failed {
            kind: ErrorKind::Server,
            retry_at: Some(ts("18:55:05.000"))
        }
    );
    // 失败期间照样显示以前的资料，并标明不是这场比赛取得的
    assert_eq!(ash.profile.as_ref().and_then(|p| p.rating), Some(1500.0));
    assert_eq!(ash.name.as_deref(), Some("Old Ash"));
    assert!(!ash.profile_current);
    assert_eq!(
        snapshot.api.last_error.as_ref().map(|e| e.kind),
        Some(ErrorKind::Server)
    );

    fx.clock.advance_millis(5_000);
    let snapshot = wait_for(&engine, "重试成功", |s| is_current_profile(s, 101)).await;
    let ash = player(&snapshot, 101).expect("有 Ash");
    assert_eq!(ash.profile.as_ref().and_then(|p| p.rating), Some(1101.0));
    assert_eq!(ash.fetch, FetchState::Idle);
    assert_eq!(snapshot.api.last_error, None);
    assert_eq!(fx.requests().await, vec![(101, 2), (201, 1)]);
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn results_for_a_previous_match_do_not_leak_into_the_next() {
    let fx = Fixture::new().await;
    Mock::given(method("GET"))
        .and(path("/api/players/info"))
        .and(query_param("stbid", "101"))
        .respond_with(ProfileResponder {
            delay: Duration::from_millis(700),
        })
        .with_priority(1)
        .mount(&fx.server)
        .await;
    fx.serve_profiles().await;

    let engine = fx.start();
    wait_for(&engine, "补读完成", |s| s.log.ready).await;
    fx.append(
        &(waiting_room(9_000_001, "18:52:05.318")
            + &game_room(
                9_000_001,
                "18:53:01.979",
                &[(101, "Ash", "Alpha"), (ME, "Me", "Bravo")],
            )),
    );
    wait_for(&engine, "慢请求已发出", |s| {
        player(s, 101).is_some_and(|p| p.fetch == FetchState::Fetching)
    })
    .await;

    // 慢请求还没回来，下一场就开始了
    fx.append(
        &(waiting_room(9_000_002, "18:54:00.000")
            + &game_room(
                9_000_002,
                "18:54:30.000",
                &[(201, "Dune", "Alpha"), (ME, "Me", "Bravo")],
            )),
    );
    wait_for(&engine, "第二场的玩家查完", |s| {
        s.current
            .as_ref()
            .is_some_and(|m| m.match_id == MatchId(9_000_002))
            && is_current_profile(s, 201)
    })
    .await;
    tokio::time::sleep(Duration::from_millis(900)).await;

    let snapshot = engine.snapshot();
    let m = snapshot.current.as_ref().expect("有当前比赛");
    assert_eq!(m.match_id, MatchId(9_000_002));
    let ids: Vec<u64> = m.players.iter().map(|p| p.id.get()).collect();
    assert_eq!(ids, vec![201, ME], "上一场的结果不会出现在这一场");
    assert_eq!(snapshot.api.in_flight, 0);
    engine.shutdown().await;

    let store = Store::open(&fx.store_path()).expect("打开长期库");
    let ash = store.player(PlayerId(101)).expect("读取").expect("有记录");
    assert!(ash.fetched_at.is_some(), "迟到的结果照样保存");
    assert_eq!(ash.rating, Some(1101.0));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_rejoin_after_restart_restores_the_saved_clock() {
    let fx = Fixture::new().await;
    fx.serve_profiles().await;
    let id = MatchId(9_000_001);

    // 上一次运行时记下的比赛
    let saved = {
        let mut tracker = Tracker::default();
        let roster = vec![
            RosterEntry {
                id: PlayerId(101),
                name: Some("Ash".into()),
                team: Team::Alpha,
            },
            RosterEntry {
                id: PlayerId(ME),
                name: Some("Me".into()),
                team: Team::Bravo,
            },
        ];
        tracker.apply(
            ts("18:52:05.318"),
            &LogEvent::RoomEntered {
                room: RoomKind::WaitingRoom,
                match_id: id,
                status: Some(RoomStatus::WaitClients),
            },
        );
        tracker.apply(
            ts("18:52:52.324"),
            &LogEvent::LoadingStarted {
                map: "Airport".into(),
                scenario: None,
            },
        );
        tracker.apply(
            ts("18:53:01.979"),
            &LogEvent::RoomEntered {
                room: RoomKind::GameRoom,
                match_id: id,
                status: Some(RoomStatus::WaitClients),
            },
        );
        tracker.apply(
            ts("18:53:01.979"),
            &LogEvent::PlayerList { players: roster },
        );
        tracker.apply(ts("18:53:33.861"), &LogEvent::ScenarioLoaded);
        let state = tracker.current().expect("有比赛");
        let mut store = Store::open(&fx.store_path()).expect("建库");
        store.record_match(state).expect("记录");
        state.clock.expect("有计时基准")
    };

    // 游戏崩溃重启，新日志里只有重连
    fx.clock.set(ts("19:01:00.000"));
    fx.append(
        &(rec("18:59:00.000", "Log: Start reconnect to battle")
            + &entered(9_000_001, "WaitingRoom", "InProcess", "18:59:01.000")
            + &rec(
                "18:59:01.000",
                "Log: Start loading battle... map: Airport, scenario: - Airport_network",
            )
            + &scenario_loaded("18:59:08.000")
            + &entered(9_000_001, "GameRoom", "InProcess", "18:59:08.000")
            + &player_list(
                "18:59:08.000",
                &[(101, "Ash", "Alpha"), (ME, "Me", "Bravo")],
            )),
    );

    let engine = fx.start();
    let snapshot = wait_for(&engine, "补读完成", |s| {
        s.log.ready && s.current.is_some()
    })
    .await;
    let m = snapshot.current.as_ref().expect("有当前比赛");
    assert!(m.rejoined);
    assert_eq!(m.phase, Phase::InBattle);
    assert_eq!(m.clock, Some(saved), "沿用上次记下的计时基准");

    // 补读结束时比赛仍在进行：刷新玩家资料
    wait_for(&engine, "刷新玩家资料", |s| {
        is_current_profile(s, 101) && is_current_profile(s, ME)
    })
    .await;
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn switching_log_dirs_resets_the_match_and_is_remembered() {
    let fx = Fixture::new().await;
    fx.serve_profiles().await;
    fx.append(&finished_match(9_000_001));
    let other = fx.dir.path().join("OtherLogs");
    std::fs::create_dir_all(&other).expect("建目录");
    append_to(&other.join(LOG_FILE), &finished_match(9_000_002));

    let engine = fx.start();
    wait_for(&engine, "读到第一个目录的比赛", |s| {
        s.current
            .as_ref()
            .is_some_and(|m| m.match_id == MatchId(9_000_001))
    })
    .await;

    engine
        .set_log_dir(Some(other.clone()))
        .await
        .expect("引擎在运行");
    let snapshot = wait_for(&engine, "读到第二个目录的比赛", |s| {
        s.log.ready
            && s.current
                .as_ref()
                .is_some_and(|m| m.match_id == MatchId(9_000_002))
    })
    .await;
    let other_text = other.to_string_lossy().into_owned();
    assert_eq!(snapshot.log.dir.as_deref(), Some(other_text.as_str()));
    engine.shutdown().await;

    // 下次启动不指定目录时，用记住的那个
    let mut config = fx.config();
    config.log_dir = None;
    let engine = fx.start_with(config);
    let snapshot = wait_for(&engine, "重启后读到记住的目录", |s| {
        s.log.ready && s.current.is_some()
    })
    .await;
    assert_eq!(snapshot.log.dir.as_deref(), Some(other_text.as_str()));
    assert_eq!(
        snapshot.current.as_ref().map(|m| m.match_id),
        Some(MatchId(9_000_002))
    );
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_missing_log_dir_is_reported_and_picked_up_once_created() {
    let fx = Fixture::new().await;
    fx.serve_profiles().await;
    let later = fx.dir.path().join("NotYet");
    let mut config = fx.config();
    config.log_dir = Some(later.clone());

    let engine = fx.start_with(config);
    let snapshot = wait_for(&engine, "报告目录不存在", |s| s.log.error.is_some()).await;
    assert!(!snapshot.log.ready);
    assert!(snapshot.current.is_none());

    std::fs::create_dir_all(&later).expect("建目录");
    append_to(&later.join(LOG_FILE), &finished_match(9_000_001));
    let snapshot = wait_for(&engine, "目录出现后读到比赛", |s| {
        s.log.ready && s.current.is_some()
    })
    .await;
    assert_eq!(snapshot.log.error, None);
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn refresh_command_refetches_everyone() {
    let fx = Fixture::new().await;
    fx.serve_profiles().await;
    fx.append(&finished_match(9_000_001));
    let engine = fx.start();
    wait_for(&engine, "补读完成", |s| {
        s.log.ready && s.current.is_some()
    })
    .await;
    assert!(fx.requests().await.is_empty());

    engine.refresh_players().await.expect("引擎在运行");
    wait_for(&engine, "刷新完成", |s| {
        ROSTER.iter().all(|(id, _, _)| is_current_profile(s, *id))
    })
    .await;
    assert_eq!(
        fx.requests().await,
        vec![(101, 1), (102, 1), (201, 1), (ME, 1)]
    );
    engine.shutdown().await;
}

fn rule(id: u32, at_secs: u32, label: &str) -> ReminderRule {
    ReminderRule {
        id,
        at_secs,
        label: label.to_owned(),
        enabled: true,
    }
}

/// 一场正在进行的比赛：场景加载完成于 18:53:33.861，开局时刻推算为 18:54:33.861。
fn match_in_progress(id: u64) -> String {
    waiting_room(id, "18:52:05.318")
        + &game_room(id, "18:53:01.979", &ROSTER)
        + &scenario_loaded("18:53:33.861")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_reminder_fires_once_and_not_again_after_a_restart() {
    let fx = Fixture::new().await;
    fx.serve_profiles().await;
    fx.append(&match_in_progress(9_000_001));
    let engine = fx.start();
    wait_for(&engine, "补读完成", |s| {
        s.log.ready && s.current.is_some()
    })
    .await;
    engine
        .set_reminders(vec![rule(7, 120, "两分钟")])
        .await
        .expect("规则合法");
    let snapshot = engine.snapshot();
    assert_eq!(snapshot.reminders.len(), 1);
    assert_eq!(snapshot.reminders[0].fire_at, Some(ts("18:56:33.861")));
    assert_eq!(snapshot.reminders[0].outcome, None);
    assert_eq!(snapshot.alert, None);

    fx.clock.set(ts("18:56:34.000"));
    let snapshot = wait_for(&engine, "到点提醒", |s| s.alert.is_some()).await;
    let alert = snapshot.alert.as_ref().expect("有提醒");
    assert_eq!(alert.seq, 1);
    assert_eq!(alert.rule_id, 7);
    assert_eq!(alert.label, "两分钟");
    assert_eq!(alert.match_id, MatchId(9_000_001));
    assert_eq!(snapshot.reminders[0].outcome, Some(ReminderOutcome::Fired));
    engine.shutdown().await;

    // 重启后补读同一场比赛：规则还在，已经提醒过的不再提醒
    let engine = fx.start();
    let snapshot = wait_for(&engine, "重启后补读完成", |s| {
        s.log.ready && s.current.is_some()
    })
    .await;
    assert_eq!(snapshot.reminders[0].id, 7, "规则保存在长期库里");
    assert_eq!(snapshot.reminders[0].outcome, Some(ReminderOutcome::Fired));
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(engine.snapshot().alert, None);
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reminders_passed_before_bax_started_are_marked_missed() {
    let fx = Fixture::new().await;
    fx.serve_profiles().await;
    // 先在没有比赛的时候设好第 10 分钟的提醒；默认没有任何提醒
    let engine = fx.start();
    let snapshot = wait_for(&engine, "补读完成", |s| s.log.ready).await;
    assert!(snapshot.reminders.is_empty(), "默认不设提醒");
    engine
        .set_reminders(vec![rule(1, 600, "第 10 分钟")])
        .await
        .expect("规则合法");
    engine.shutdown().await;

    // 开局 20 分钟后才打开 BAX，第 10 分钟的提醒已经过去
    fx.append(&match_in_progress(9_000_001));
    fx.clock.set(ts("19:14:40.000"));
    let engine = fx.start();
    let snapshot = wait_for(&engine, "检查过提醒", |s| {
        s.reminders.first().is_some_and(|r| r.outcome.is_some())
    })
    .await;
    assert_eq!(snapshot.reminders[0].at_secs, 600);
    assert_eq!(snapshot.reminders[0].outcome, Some(ReminderOutcome::Missed));
    assert_eq!(snapshot.alert, None, "错过的不补发");
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn manual_calibration_moves_the_clock() {
    let fx = Fixture::new().await;
    fx.serve_profiles().await;
    fx.append(&match_in_progress(9_000_001));
    let engine = fx.start();
    wait_for(&engine, "补读完成", |s| {
        s.log.ready && s.current.is_some()
    })
    .await;

    // 画面上显示开局 5 分钟整
    engine.calibrate(300_000).await.expect("比赛进行中");
    let snapshot = engine.snapshot();
    let clock = snapshot
        .current
        .as_ref()
        .and_then(|m| m.clock)
        .expect("有计时基准");
    assert_eq!(clock.source, ClockSource::Manual);
    assert_eq!(clock.start_at, ts("18:50:00.000"));

    // 比赛结束后不再接受校准
    fx.append(&match_ended(9_000_001, "18:55:30.000"));
    wait_for(&engine, "比赛结束", |s| {
        s.current.as_ref().is_some_and(|m| m.phase == Phase::Ended)
    })
    .await;
    let err = engine.calibrate(0).await.expect_err("应当拒绝");
    assert!(matches!(err, CommandError::Rejected(_)));
    engine.shutdown().await;

    let store = Store::open(&fx.store_path()).expect("打开长期库");
    let record = store
        .match_record(MatchId(9_000_001))
        .expect("读取")
        .expect("有记录");
    assert_eq!(record.clock.map(|c| c.source), Some(ClockSource::Manual));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ui_settings_are_saved_and_shared() {
    let fx = Fixture::new().await;
    let engine = fx.start();
    wait_for(&engine, "补读完成", |s| s.log.ready).await;

    let overlay = serde_json::json!({ "opacity": 0.85, "locked": true });
    engine
        .set_ui_setting("ui.overlay", overlay.clone())
        .await
        .expect("合法的键");
    assert_eq!(engine.snapshot().ui.get("ui.overlay"), Some(&overlay));
    let err = engine
        .set_ui_setting("overlay", serde_json::json!(1))
        .await
        .expect_err("键必须以 ui. 开头");
    assert!(matches!(err, CommandError::Rejected(_)));
    engine
        .set_ui_setting("ui.temp", serde_json::json!("x"))
        .await
        .expect("合法的键");
    engine
        .set_ui_setting("ui.temp", serde_json::Value::Null)
        .await
        .expect("删除");
    engine.shutdown().await;

    let engine = fx.start();
    let snapshot = wait_for(&engine, "重启", |s| s.log.ready).await;
    assert_eq!(snapshot.ui.get("ui.overlay"), Some(&overlay));
    assert!(!snapshot.ui.contains_key("ui.temp"));
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn invalid_reminder_rules_are_rejected_and_nothing_changes() {
    let fx = Fixture::new().await;
    let engine = fx.start();
    wait_for(&engine, "补读完成", |s| s.log.ready).await;
    let before = engine.snapshot().reminders.clone();
    let err = engine
        .set_reminders(vec![rule(1, 60, "a"), rule(1, 120, "b")])
        .await
        .expect_err("编号重复");
    assert!(matches!(err, CommandError::Rejected(_)));
    assert_eq!(engine.snapshot().reminders, before);
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn notes_show_up_in_the_match_and_in_the_player_detail() {
    let fx = Fixture::new().await;
    fx.serve_profiles().await;
    fx.append(&finished_match(9_000_001));
    let engine = fx.start();
    wait_for(&engine, "补读完成", |s| {
        s.log.ready && s.current.is_some()
    })
    .await;

    engine
        .set_note(PlayerId(101), "爱推坦克")
        .await
        .expect("写入备注");
    let snapshot = wait_for(&engine, "快照里有备注", |s| {
        player(s, 101).is_some_and(|p| p.note.as_deref() == Some("爱推坦克"))
    })
    .await;
    assert_eq!(player(&snapshot, 201).and_then(|p| p.note.clone()), None);

    let detail = engine
        .player_detail(PlayerId(101))
        .await
        .expect("查询")
        .expect("有档案");
    assert_eq!(detail.note.map(|n| n.body), Some("爱推坦克".to_owned()));
    assert_eq!(detail.log_name.as_deref(), Some("Ash"));
    assert_eq!(detail.matches.len(), 1);
    assert_eq!(detail.matches[0].relation, Relation::Opponent);
    assert_eq!(detail.encounters.total, 1);
    assert!(!detail.is_self);
    let me = engine
        .player_detail(PlayerId(ME))
        .await
        .expect("查询")
        .expect("有档案");
    assert!(me.is_self);
    assert!(
        engine
            .player_detail(PlayerId(424_242))
            .await
            .expect("查询")
            .is_none()
    );

    let hits = engine.search_players("ash", 10).await.expect("查找");
    assert_eq!(
        hits.iter().map(|h| h.id.get()).collect::<Vec<_>>(),
        vec![101]
    );
    assert!(hits[0].has_note);

    engine.set_note(PlayerId(101), "").await.expect("删除备注");
    wait_for(&engine, "备注已删除", |s| {
        player(s, 101).is_some_and(|p| p.note.is_none())
    })
    .await;
    let err = engine
        .set_note(PlayerId(101), "字".repeat(2001))
        .await
        .expect_err("太长");
    assert!(matches!(err, CommandError::Rejected(_)));
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_single_player_can_be_refreshed_on_demand() {
    let fx = Fixture::new().await;
    fx.serve_profiles().await;
    fx.append(&finished_match(9_000_001));
    let engine = fx.start();
    wait_for(&engine, "补读完成", |s| {
        s.log.ready && s.current.is_some()
    })
    .await;

    engine.refresh_player(PlayerId(201)).await.expect("刷新");
    let snapshot = wait_for(&engine, "当前比赛里的资料更新", |s| {
        is_current_profile(s, 201)
    })
    .await;
    assert_eq!(
        player(&snapshot, 201)
            .and_then(|p| p.profile.as_ref())
            .and_then(|p| p.rating),
        Some(1201.0)
    );
    let detail = engine
        .player_detail(PlayerId(201))
        .await
        .expect("查询")
        .expect("有档案");
    assert_eq!(detail.history.len(), 1);
    assert_eq!(detail.name.as_deref(), Some("API-201"));
    assert_eq!(fx.requests().await, vec![(201, 1)]);
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn importing_history_updates_encounter_counts() {
    let fx = Fixture::new().await;
    fx.serve_profiles().await;
    // 一份更早的日志：与 101 等人同场过一次
    append_to(
        &fx.logs.join("Gamelog__2026_11_15__18_00.log"),
        &finished_match(8_000_001),
    );
    fx.append(&finished_match(9_000_001));
    let engine = fx.start();
    let snapshot = wait_for(&engine, "补读完成", |s| {
        s.log.ready && s.current.is_some()
    })
    .await;
    assert_eq!(
        player(&snapshot, 101).map(|p| p.encounters.total),
        Some(0),
        "补读只读最新的一份日志"
    );

    let summary = engine.import_history().await.expect("导入");
    assert_eq!((summary.files, summary.matches, summary.failed), (2, 2, 0));
    wait_for(&engine, "同场次数更新", |s| {
        player(s, 101).is_some_and(|p| p.encounters.total == 1)
    })
    .await;

    let again = engine.import_history().await.expect("再导入一次");
    assert_eq!(again.matches, 2);
    assert_eq!(
        player(&engine.snapshot(), 101).map(|p| p.encounters.total),
        Some(1),
        "重复导入不会重复计数"
    );
    engine.shutdown().await;
}

// ---------------------------------------------------------------- 战后归档

const REPORT: &str = include_str!("../../../fixtures/batrace/match_report.json");
const UNITS: &str = include_str!("../../../fixtures/batrace/units.json");

fn json(body: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(body.to_owned(), "application/json")
}

/// 战报请求了几次。
async fn report_requests(fx: &Fixture) -> usize {
    fx.server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|r| r.url.path() == "/api/match")
        .count()
}

fn quick_reports(mut config: EngineConfig) -> EngineConfig {
    config.report_schedule = vec![
        Duration::from_secs(1),
        Duration::from_secs(2),
        Duration::from_secs(60),
    ];
    config.report_schedule_unplayed = vec![Duration::from_secs(1)];
    config.report_poll_interval = Duration::from_millis(30);
    config.report_spacing = Duration::from_millis(5);
    config
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_finished_match_is_archived_once_the_report_appears() {
    let fx = Fixture::new().await;
    fx.serve_profiles().await;
    // 第一次还没收录，第二次有了
    Mock::given(method("GET"))
        .and(path("/api/match"))
        .and(query_param("matchid", "9000001"))
        .respond_with(ResponseTemplate::new(404))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&fx.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/match"))
        .and(query_param("matchid", "9000001"))
        .respond_with(json(REPORT))
        .mount(&fx.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/units"))
        .respond_with(json(UNITS))
        .mount(&fx.server)
        .await;

    let engine = fx.start_with(quick_reports(fx.config()));
    fx.append(&finished_match(9_000_001));
    wait_for(&engine, "比赛结束", |s| {
        s.current.as_ref().is_some_and(|m| m.phase == Phase::Ended)
    })
    .await;
    wait_for(&engine, "排队等战报", |s| s.archive.pending == 1).await;
    assert_eq!(report_requests(&fx).await, 0, "没到时间不请求");

    // 结束后 1 秒：第一次尝试，还没有
    fx.clock.set(ts("19:06:21.200"));
    let mut tries = 0;
    while report_requests(&fx).await == 0 {
        tries += 1;
        assert!(tries < 200, "应该已经试过一次");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let history = engine.match_history(10, 0).await.expect("比赛记录");
    assert!(
        matches!(history[0].report, ReportState::Pending { attempts: 1, .. }),
        "{:?}",
        history[0].report
    );
    assert!(
        engine
            .match_report(MatchId(9_000_001))
            .await
            .expect("读")
            .is_none()
    );

    // 结束后 2 秒：第二次，取到了
    fx.clock.set(ts("19:06:22.200"));
    let snapshot = wait_for(&engine, "战报归档", |s| {
        s.archive.last_archived == Some(MatchId(9_000_001))
    })
    .await;
    assert_eq!(snapshot.archive.pending, 0);
    assert_eq!(report_requests(&fx).await, 2);

    let history = engine.match_history(10, 0).await.expect("比赛记录");
    assert_eq!(history[0].match_id, MatchId(9_000_001));
    assert!(matches!(history[0].report, ReportState::Ready { .. }));

    // 有了战报之后取单位目录
    let mut view = None;
    for _ in 0..200 {
        let v = engine
            .match_report(MatchId(9_000_001))
            .await
            .expect("读")
            .expect("有战报");
        if v.catalog_fetched_at.is_some() {
            view = Some(v);
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let view = view.expect("单位目录应该已经取到");
    assert_eq!(view.map.as_deref(), Some("Airport"));
    assert_eq!(view.self_id, Some(PlayerId(ME)));
    assert!(!view.analysis.players.is_empty());

    // 已经有战报：手动请求什么也不做
    engine
        .fetch_report(MatchId(9_000_001))
        .await
        .expect("手动请求");
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(report_requests(&fx).await, 2);
    engine.shutdown().await;

    // 战报的开始时间修正了计时基准
    let store = Store::open(&fx.store_path()).expect("长期库");
    let record = store
        .match_record(MatchId(9_000_001))
        .expect("读")
        .expect("有比赛");
    assert_eq!(record.clock.map(|c| c.source), Some(ClockSource::Report));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_report_that_never_appears_is_given_up_and_can_be_requested_again() {
    let fx = Fixture::new().await;
    fx.serve_profiles().await;
    Mock::given(method("GET"))
        .and(path("/api/match"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&fx.server)
        .await;

    // 应用关着的时候比赛就结束了，而且已经过了全部时间点：启动后只试一次
    fx.append(&finished_match(9_000_002));
    fx.clock.set(ts("19:30:00.000"));
    let engine = fx.start_with(quick_reports(fx.config()));
    let mut tries = 0;
    loop {
        let history = engine.match_history(10, 0).await.expect("比赛记录");
        if history
            .first()
            .is_some_and(|r| matches!(r.report, ReportState::GaveUp { .. }))
        {
            break;
        }
        tries += 1;
        assert!(tries < 200, "应该已经放弃：{history:?}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(report_requests(&fx).await, 1);

    // 手动再试：马上请求一次
    engine
        .fetch_report(MatchId(9_000_002))
        .await
        .expect("手动请求");
    let mut tries = 0;
    while report_requests(&fx).await < 2 {
        tries += 1;
        assert!(tries < 200, "手动请求应该马上发出");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_match_that_ends_at_deployment_gives_up_quickly_and_says_why() {
    let fx = Fixture::new().await;
    fx.serve_profiles().await;
    Mock::given(method("GET"))
        .and(path("/api/match"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&fx.server)
        .await;

    // 准备阶段一结束比赛就结束了（计时起点是场景加载后 60 秒）
    let id = 9_000_003;
    fx.append(
        &(waiting_room(id, "18:52:05.318")
            + &game_room(id, "18:53:01.979", &ROSTER)
            + &scenario_loaded("18:53:33.861")
            + &match_ended(id, "18:54:33.910")),
    );
    fx.clock.set(ts("18:54:34.000"));
    let engine = fx.start_with(quick_reports(fx.config()));
    wait_for(&engine, "排队等战报", |s| s.archive.pending == 1).await;

    // 短时间表只有一个点：试一次就放弃，并说明原因
    fx.clock.set(ts("18:54:35.000"));
    let mut tries = 0;
    let last_error = loop {
        let history = engine.match_history(10, 0).await.expect("比赛记录");
        if let Some(ReportState::GaveUp { last_error, .. }) = history.first().map(|r| &r.report) {
            break last_error.clone();
        }
        tries += 1;
        assert!(tries < 200, "应该已经放弃：{history:?}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    assert_eq!(report_requests(&fx).await, 1);
    assert!(
        last_error
            .as_deref()
            .is_some_and(|e| e.contains("没打起来")),
        "{last_error:?}"
    );
    engine.shutdown().await;
}
