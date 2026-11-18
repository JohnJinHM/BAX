//! 诊断日志的隐私约定（ADR-0007）：跑一整场比赛，截获全部诊断输出，
//! 检查玩家名称和卡组名没有出现，ID 只出现在 `player`、`self_player`、`match_id` 字段里，
//! 导出的诊断包里一个 ID 也不剩。
//!
//! 全局订阅器每个进程只能设一次，所以这个文件只有一个测试。

use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use bax_batrace::ClientConfig;
use bax_core::{Phase, Timestamp};
use bax_engine::diag::{self, DiagConfig, DiagLevel, ExportOptions};
use bax_engine::{Engine, EngineConfig, ManualClock, Snapshot};
use bax_gamelog::{LogTime, LogZone};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

const LOG_FILE: &str = "Gamelog__2026_11_16__18_51.log";
const UTC: LogZone = LogZone::Fixed { offset_seconds: 0 };

// 虚构的 ID 都取得足够长，不会和时间、耗时之类的数字撞上
const MATCH: u64 = 77_700_000_001;
const ME: u64 = 88_800_000_500;
const ROSTER: [(u64, &str, &str); 4] = [
    (88_800_000_101, "Zephyrine", "Alpha"),
    (88_800_000_102, "", "Alpha"),
    (88_800_000_201, "Quillon", "Bravo"),
    (ME, "Marlowe", "Bravo"),
];
const DECK: &str = "Glasswing Vanguard";
const FORBIDDEN: [&str; 5] = ["Zephyrine", "Quillon", "Marlowe", "Profile-", DECK];

#[derive(Debug, Clone)]
struct ProfileResponder;

impl Respond for ProfileResponder {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let id: u64 = request
            .url
            .query_pairs()
            .find(|(k, _)| k == "stbid")
            .and_then(|(_, v)| v.parse().ok())
            .unwrap_or(0);
        let body = serde_json::json!({
            "info": { "id": id, "name": format!("Profile-{id}"), "level": 10, "rating": 1200.0 },
            "statInfo": {},
            "last_fights_data": []
        });
        ResponseTemplate::new(200).set_body_raw(body.to_string(), "application/json")
    }
}

fn rec(time: &str, message: &str) -> String {
    format!("[2026-11-16 {time}]\n{message}\n\n")
}

fn entered(room: &str, status: &str, time: &str) -> String {
    rec(
        time,
        &format!(
            "Log: [NET_ROOM] Network lifecycle | Stage=Game | Action=Entered | Route=Direct | RoomType={room} | FightId={MATCH} | Status={status}"
        ),
    )
}

fn append(path: &Path, text: &str) {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .expect("打开日志");
    file.write_all(text.as_bytes()).expect("写入日志");
}

fn ts(time: &str) -> Timestamp {
    LogTime::parse_bracketed(&format!("[2026-11-16 {time}]"))
        .expect("合法时间")
        .to_utc(UTC)
}

async fn wait_for(engine: &Engine, what: &str, predicate: impl Fn(&Snapshot) -> bool) {
    let mut rx = engine.subscribe();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        if predicate(&rx.borrow_and_update()) {
            return;
        }
        if tokio::time::timeout_at(deadline, rx.changed())
            .await
            .is_err()
        {
            panic!("等待超时：{what}");
        }
    }
}

/// 文本里 `id` 的每一次出现（前后都不是数字），前面紧挨着的必须是允许的字段名。
fn assert_only_in_fields(text: &str, id: u64, fields: &[&str]) {
    let id = id.to_string();
    let bytes = text.as_bytes();
    let mut from = 0;
    while let Some(found) = text[from..].find(&id) {
        let at = from + found;
        let end = at + id.len();
        from = end;
        let digit_before = at > 0 && bytes[at - 1].is_ascii_digit();
        let digit_after = end < bytes.len() && bytes[end].is_ascii_digit();
        if digit_before || digit_after {
            continue;
        }
        let allowed = fields
            .iter()
            .any(|field| text[..at].ends_with(&format!("{field}=")));
        let line_start = text[..at].rfind('\n').map_or(0, |i| i + 1);
        let line_end = text[end..].find('\n').map_or(text.len(), |i| end + i);
        assert!(
            allowed,
            "ID {id} 出现在约定字段之外：{}",
            &text[line_start..line_end]
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn diagnostics_keep_names_out_and_ids_in_their_fields() {
    let dir = tempfile::tempdir().expect("临时目录");
    let diag_dir = dir.path().join("diag");
    let mut config = DiagConfig::new("privacy-test");
    config.file_dir = Some(diag_dir.clone());
    config.level = DiagLevel::Verbose;
    config.panic_hook = false;
    let (subscriber, handle) = diag::build(config);
    tracing::subscriber::set_global_default(subscriber).expect("这个测试独占全局订阅器");

    let logs = dir.path().join("GameLogs");
    std::fs::create_dir_all(&logs).expect("建日志目录");
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/players/info"))
        .respond_with(ProfileResponder)
        .mount(&server)
        .await;
    let clock = ManualClock::new(ts("18:52:00.000"));
    let mut engine_config = EngineConfig::new(dir.path());
    engine_config.log_dir = Some(logs.clone());
    engine_config.api = ClientConfig {
        base_url: server.uri(),
        timeout: Duration::from_secs(3),
        player_info_timeout: Duration::from_secs(3),
        ..ClientConfig::default()
    };
    engine_config.log_zone = UTC;
    engine_config.log_poll_interval = Duration::from_millis(20);
    engine_config.tick_interval = Duration::from_millis(20);
    let engine = Engine::start(engine_config, Arc::new(clock.clone())).expect("引擎启动");
    wait_for(&engine, "补读完成", |s| s.log.ready).await;

    let log = logs.join(LOG_FILE);
    let mut text = rec("18:51:30.000", "Log: GetPersonaName Marlowe")
        + &rec("18:51:40.000", &format!("Log: Deck set to: {DECK}"))
        + &entered("WaitingRoom", "WaitClients", "18:52:05.318")
        + &rec(
            "18:52:05.318",
            &format!("Log: [NET_ROOM] WaitingRoom entered, fight id: {MATCH}"),
        );
    for (id, name, _) in &ROSTER[..2] {
        text += &rec(
            "18:52:20.000",
            &format!("Log: Room: (WaitingRoom|1), Client: ({name}|{id})"),
        );
    }
    append(&log, &text);
    wait_for(&engine, "进入等待室", |s| s.current.is_some()).await;

    let mut list = "[2026-11-16 18:53:01.979]\nLog: Player list: \n".to_owned();
    for (id, name, team) in &ROSTER {
        list += &format!("ID: {id}, Name: {name}, Team: {team}\n");
    }
    append(
        &log,
        &(rec(
            "18:53:01.979",
            "Log: Start loading battle... map: Airport, scenario: - Airport_network",
        ) + &rec(
            "18:53:01.979",
            &format!("Log: FID:{MATCH} v1.2.2.1 - Marlowe (2026-11-17 02:52)"),
        ) + &entered("GameRoom", "WaitClients", "18:53:01.979")
            + &list
            + "\n"
            + &rec("18:53:33.861", "Log: Scenario controller loaded")),
    );
    clock.set(ts("18:55:00.000"));
    wait_for(&engine, "进入对局并查完资料", |s| {
        s.current.as_ref().is_some_and(|m| {
            m.phase == Phase::InBattle && m.players.iter().all(|p| p.profile_current)
        })
    })
    .await;
    engine.calibrate(25_000).await.expect("手动校准");
    append(
        &log,
        &rec(
            "19:06:20.100",
            &format!(
                "Log: [NET_ROOM] Network traffic history | Cause=MatchEnded | FightId={MATCH} | Samples=27"
            ),
        ),
    );
    wait_for(&engine, "比赛结束", |s| {
        s.current.as_ref().is_some_and(|m| m.phase == Phase::Ended)
    })
    .await;
    engine.shutdown().await;
    handle.flush();

    let written =
        std::fs::read_to_string(handle.current_file().expect("有诊断日志")).expect("读诊断日志");
    for expected in [
        "match{match_id=",
        "比赛阶段变化",
        "计时基准已设定",
        "手动校准计时",
        "识别出本机玩家",
        "请求玩家资料",
        "本场资料请求汇总",
        "比赛结束",
    ] {
        assert!(
            written.contains(expected),
            "诊断日志里缺少“{expected}”：\n{written}"
        );
    }
    for name in FORBIDDEN {
        assert!(
            !written.contains(name),
            "诊断日志里出现了“{name}”：\n{written}"
        );
    }
    for (id, _, _) in &ROSTER {
        assert_only_in_fields(&written, *id, &["player", "self_player"]);
    }
    assert_only_in_fields(&written, MATCH, &["match_id"]);

    let exported = diag::export(&diag_dir, &ExportOptions::default()).expect("导出");
    let package = std::fs::read_to_string(&exported.path).expect("读诊断包");
    for (id, _, _) in &ROSTER {
        assert!(
            !package.contains(&id.to_string()),
            "诊断包里还有玩家 ID {id}"
        );
    }
    assert!(!package.contains(&MATCH.to_string()), "诊断包里还有比赛 ID");
    assert!(package.contains("self_player=SELF"));
    assert_eq!(exported.matches, 1);
}
