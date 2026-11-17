use std::time::Duration;

use bax_core::{MatchId, PlayerId, Team};
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::error::ErrorKind;

const PLAYER_INFO: &str = include_str!("../../../../fixtures/batrace/player_info.json");
const MATCH_REPORT: &str = include_str!("../../../../fixtures/batrace/match_report.json");
const PLAYER_MATCHES: &str = include_str!("../../../../fixtures/batrace/player_matches.json");
const UNITS: &str = include_str!("../../../../fixtures/batrace/units.json");

fn client(server: &MockServer) -> BatraceClient {
    BatraceClient::new(ClientConfig {
        base_url: server.uri(),
        timeout: Duration::from_millis(600),
        player_info_timeout: Duration::from_millis(300),
        ..ClientConfig::default()
    })
    .expect("配置合法")
}

fn json(body: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(body.to_owned(), "application/json; charset=utf-8")
}

async fn serve(server: &MockServer, at: &str, response: ResponseTemplate) {
    Mock::given(method("GET"))
        .and(path(at))
        .respond_with(response)
        .mount(server)
        .await;
}

#[tokio::test]
async fn player_info_is_parsed_and_the_request_identifies_the_app() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/players/info"))
        .and(query_param("stbid", "100202"))
        .and(header("accept", "application/json"))
        .and(header("x-client", "BAX"))
        .and(header(
            "user-agent",
            format!("BAX/{} (+local tool)", env!("CARGO_PKG_VERSION")).as_str(),
        ))
        .respond_with(json(PLAYER_INFO))
        .expect(1)
        .mount(&server)
        .await;

    let fetched = client(&server)
        .player_info(PlayerId(100_202))
        .await
        .expect("请求成功");

    let info = fetched.data.info.as_ref().expect("有基本资料");
    assert_eq!(info.id, Some(100_202));
    assert_eq!(info.name.as_deref(), Some("Starling"));
    assert_eq!(info.steam_id.as_deref(), Some("76561190000000001"));
    assert_eq!(info.level, Some(87));
    assert_eq!(info.rating, Some(2143.5));
    assert_eq!(info.rank, Some(512));
    assert_eq!(info.rating_games, Some(640));

    let stats = fetched.data.stat_info.as_ref().expect("有累计统计");
    assert_eq!(stats.fights_count_rt, Some(640));
    assert_eq!(stats.win_count_rt, Some(352));
    assert_eq!(stats.leave_count_rt, Some(7));
    assert_eq!(stats.units_killed_cost_rt, Some(4_310_000));
    assert_eq!(stats.zone_captured_count, Some(1890));
    assert_eq!(stats.supply_pts_consumed_by_allies, Some(95_000));
    assert_eq!(stats.spec_levels.get("5"), Some(&14));
    assert_eq!(stats.combined_specs_exp.get("3_5"), Some(&120_000));
    assert_eq!(stats.map_play_counts.len(), 3);
    assert_eq!(
        stats.updated_at.as_deref(),
        Some("2026-11-16T10:21:07.000Z")
    );

    assert_eq!(
        fetched.data.recent_match_ids().collect::<Vec<_>>(),
        vec![MatchId(9_000_003), MatchId(9_000_002), MatchId(9_000_001)]
    );
    assert_eq!(fetched.raw, PLAYER_INFO, "原文原样保留，供归档");
}

#[tokio::test]
async fn player_info_tolerates_missing_stats_nulls_and_unknown_fields() {
    let server = MockServer::start().await;
    serve(
        &server,
        "/api/players/info",
        json(r#"{"info":{"id":7,"name":"","rating":1500,"brandNewField":[1,2]},"statInfo":null,"ban_info":{"x":1},"last_fights_data":null,"extra":true}"#),
    )
    .await;

    let fetched = client(&server)
        .player_info(PlayerId(7))
        .await
        .expect("请求成功");
    let info = fetched.data.info.expect("有基本资料");
    assert_eq!(info.name.as_deref(), Some(""));
    assert_eq!(info.rating, Some(1500.0), "整数形式的 ELO 也能读");
    assert_eq!(info.level, None);
    assert_eq!(fetched.data.stat_info, None);
    assert!(fetched.data.last_fights_data.is_empty());
}

#[tokio::test]
async fn player_without_info_is_not_found() {
    let server = MockServer::start().await;
    serve(
        &server,
        "/api/players/info",
        json(r#"{"info":null,"statInfo":null}"#),
    )
    .await;
    let err = client(&server)
        .player_info(PlayerId(1))
        .await
        .expect_err("应当失败");
    assert_eq!(err, ApiError::NotFound);
    assert!(!err.is_transient());
}

#[tokio::test]
async fn match_report_is_parsed_with_teams_winner_and_optional_unit_fields() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/match"))
        .and(query_param("matchid", "9000003"))
        .respond_with(json(MATCH_REPORT))
        .mount(&server)
        .await;

    let fetched = client(&server)
        .match_report(MatchId(9_000_003))
        .await
        .expect("请求成功");
    let info = &fetched.data;

    assert_eq!(info.start_time, Some(1_794_883_993));
    assert_eq!(info.total_play_time_in_sec, Some(1200));
    assert_eq!(info.winner(), Some(Team::Bravo));
    assert_eq!(
        info.team_match_points,
        Some(crate::model::TeamMatchPoints {
            alpha: Some(420),
            bravo: Some(1000)
        })
    );

    let players: Vec<_> = info.players().collect();
    assert_eq!(players.len(), 5);
    let team_of = |id: u64| {
        players
            .iter()
            .find(|(pid, _)| *pid == PlayerId(id))
            .and_then(|(_, p)| p.team())
    };
    // 没有 TeamId 键的是 Alpha
    assert_eq!(team_of(100_101), Some(Team::Alpha));
    assert_eq!(team_of(100_102), Some(Team::Alpha));
    assert_eq!(team_of(100_201), Some(Team::Bravo));
    assert_eq!(team_of(100_900), Some(Team::Spectators));

    let ash = &info.data["100101"];
    assert_eq!(ash.old_rating, Some(2100.5));
    assert_eq!(ash.new_rating, Some(2088.0));
    assert_eq!(ash.total_spawned_unit_score, Some(5200));
    assert_eq!(ash.dl_ratio, Some(1.24));
    assert_eq!(ash.medals, vec![3, 7]);
    assert_eq!(ash.unit_data.len(), 3);

    let killed = &ash.unit_data["1"];
    assert_eq!(killed.id, Some(169));
    assert_eq!(killed.option_ids, vec![11, 12, 40]);
    assert_eq!(killed.spawn_time, Some(0));
    assert_eq!(killed.death_time, Some(340));
    assert_eq!(killed.killed_count, Some(3));
    assert_eq!(killed.total_damage_dealt, Some(812.5));

    // 被省略的字段保持“没有这个键”，不被当成零
    let refunded = &ash.unit_data["2"];
    assert_eq!(refunded.was_refunded, Some(true));
    assert_eq!(refunded.death_time, None);
    assert_eq!(refunded.killed_count, None);
    assert_eq!(refunded.total_damage_dealt, None);
    assert!(refunded.option_ids.is_empty());

    let airdropped = &ash.unit_data["9"];
    assert_eq!(airdropped.was_airdropped, Some(true));
    assert_eq!(airdropped.total_supply_points_consumed, Some(350));

    let nameless = &info.data["100102"];
    assert_eq!(nameless.name.as_deref(), Some(""));
    assert_eq!(nameless.deserter, Some(true));
    assert_eq!(nameless.destruction, None);
    assert_eq!(nameless.unit_data["3"].self_destruction, Some(true));

    let dune = &info.data["100201"];
    assert_eq!(dune.transferred_units["31"].id, Some(77));
    assert_eq!(dune.transferred_units["31"].spawn_time, Some(400));
    assert_eq!(dune.unit_data["20"].building_destroyed_count, Some(1));
    assert_eq!(
        dune.unit_data["20"].total_damage_friendly_fire_dealt,
        Some(35.5)
    );
}

#[tokio::test]
async fn winner_defaults_to_alpha_when_the_key_is_omitted() {
    let server = MockServer::start().await;
    serve(
        &server,
        "/api/match",
        json(r#"{"matchInfo":{"Data":{"5":{"Id":5,"Name":"x"}},"VictoryLevel":70}}"#),
    )
    .await;
    let fetched = client(&server)
        .match_report(MatchId(1))
        .await
        .expect("请求成功");
    assert_eq!(fetched.data.winner(), Some(Team::Alpha));

    // 见过的其他编号含义不明，不猜
    let odd: crate::model::MatchInfo =
        serde_json::from_str(r#"{"WinnerTeam":101,"Data":{"5":{}}}"#).expect("可解析");
    assert_eq!(odd.winner(), None);
    assert_eq!(odd.players().count(), 1, "缺少 Id 字段时用键补上");
}

#[tokio::test]
async fn empty_or_missing_match_report_is_not_found() {
    for body in [
        r#"{"matchInfo":{"Data":{},"MapId":3}}"#,
        r#"{"matchInfo":null}"#,
        r#"{}"#,
    ] {
        let server = MockServer::start().await;
        serve(&server, "/api/match", json(body)).await;
        let err = client(&server)
            .match_report(MatchId(1))
            .await
            .expect_err("应当失败");
        assert_eq!(err, ApiError::NotFound, "{body}");
    }
}

#[tokio::test]
async fn player_matches_are_parsed_with_paging_parameters() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/players/matches"))
        .and(query_param("stbid", "100202"))
        .and(query_param("offset", "15"))
        .and(query_param("limit", "30"))
        .respond_with(json(PLAYER_MATCHES))
        .mount(&server)
        .await;

    let fetched = client(&server)
        .player_matches(PlayerId(100_202), 15, 30)
        .await
        .expect("请求成功");
    assert_eq!(fetched.data.total_count, Some(100));
    assert_eq!(fetched.data.matches.len(), 2);

    let first = &fetched.data.matches[0];
    assert_eq!(first.id(), Some(MatchId(9_000_003)));
    let summary = first.data.as_ref().expect("有摘要");
    assert_eq!(summary.winner(), Some(Team::Bravo));
    assert_eq!(summary.match_type, None);
    let me = summary
        .players()
        .find(|(id, _)| *id == PlayerId(100_202))
        .map(|(_, p)| p)
        .expect("列表里有本人");
    assert_eq!(me.team(), Some(Team::Bravo));
    assert_eq!(me.new_rating, Some(2155.0));

    let second = fetched.data.matches[1].data.as_ref().expect("有摘要");
    assert_eq!(second.winner(), Some(Team::Alpha), "缺省的胜方是 Alpha");
    assert_eq!(second.match_type, Some(2));
}

#[tokio::test]
async fn unit_catalogue_is_parsed() {
    let server = MockServer::start().await;
    serve(&server, "/api/units", json(UNITS)).await;
    let fetched = client(&server).units().await.expect("请求成功");
    assert_eq!(fetched.data.count, Some(3));
    let jet = &fetched.data.units[0];
    assert_eq!(jet.id, Some(169));
    assert_eq!(jet.name.as_deref(), Some("Su-35S"));
    assert_eq!(jet.category_type, Some(6));
    assert_eq!(jet.unit_type, Some(4));
    assert_eq!(jet.cost, Some(330));
    assert_eq!(
        jet.country.as_ref().and_then(|c| c.ui_name.as_deref()),
        Some("RU")
    );
    let sparse = &fetched.data.units[2];
    assert_eq!(sparse.hud_name, None);
    assert_eq!(sparse.is_unit_modification, None);
}

#[tokio::test]
async fn http_statuses_map_to_error_kinds() {
    let cases = [
        (404, ErrorKind::NotFound, false),
        (401, ErrorKind::Rejected, false),
        (403, ErrorKind::Rejected, false),
        (400, ErrorKind::Rejected, false),
        (429, ErrorKind::RateLimited, true),
        (500, ErrorKind::Server, true),
        (502, ErrorKind::Server, true),
        (503, ErrorKind::Server, true),
    ];
    for (status, kind, transient) in cases {
        let server = MockServer::start().await;
        serve(
            &server,
            "/api/players/info",
            ResponseTemplate::new(status).set_body_string("nope"),
        )
        .await;
        let err = client(&server)
            .player_info(PlayerId(1))
            .await
            .expect_err("应当失败");
        assert_eq!(err.kind(), kind, "HTTP {status}");
        assert_eq!(err.is_transient(), transient, "HTTP {status}");
    }
}

#[tokio::test]
async fn rate_limit_carries_the_retry_after_delay() {
    let server = MockServer::start().await;
    serve(
        &server,
        "/api/units",
        ResponseTemplate::new(429).insert_header("retry-after", "7"),
    )
    .await;
    let err = client(&server).units().await.expect_err("应当失败");
    assert_eq!(
        err,
        ApiError::RateLimited {
            retry_after: Some(Duration::from_secs(7))
        }
    );
    assert_eq!(err.retry_after(), Some(Duration::from_secs(7)));

    // 没有这个头，或者写的是日期而不是秒数
    let server = MockServer::start().await;
    serve(
        &server,
        "/api/units",
        ResponseTemplate::new(429).insert_header("retry-after", "Wed, 21 Oct 2026 07:28:00 GMT"),
    )
    .await;
    let err = client(&server).units().await.expect_err("应当失败");
    assert_eq!(err, ApiError::RateLimited { retry_after: None });
}

#[tokio::test]
async fn a_successful_non_json_page_is_a_challenge() {
    let server = MockServer::start().await;
    serve(
        &server,
        "/api/players/info",
        ResponseTemplate::new(200).set_body_raw(
            "<html><body>Checking your browser…</body></html>",
            "text/html",
        ),
    )
    .await;
    let err = client(&server)
        .player_info(PlayerId(1))
        .await
        .expect_err("应当失败");
    assert_eq!(err, ApiError::Challenge);
    assert_eq!(err.kind(), ErrorKind::Rejected);
    assert!(!err.is_transient());
}

#[tokio::test]
async fn malformed_json_is_a_decode_error() {
    for body in [
        r#"{"info": {"id": "#,
        r#"[1,2,3]"#,
        r#"{"info": {"id": "not a number"}}"#,
    ] {
        let server = MockServer::start().await;
        serve(&server, "/api/players/info", json(body)).await;
        let err = client(&server)
            .player_info(PlayerId(1))
            .await
            .expect_err("应当失败");
        assert_eq!(err.kind(), ErrorKind::Decode, "{body}");
    }
}

#[tokio::test]
async fn a_slow_response_times_out() {
    let server = MockServer::start().await;
    serve(
        &server,
        "/api/players/info",
        json(PLAYER_INFO).set_delay(Duration::from_secs(5)),
    )
    .await;
    let started = std::time::Instant::now();
    let err = client(&server)
        .player_info(PlayerId(1))
        .await
        .expect_err("应当失败");
    assert_eq!(err, ApiError::Timeout);
    assert!(err.is_transient());
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "玩家资料请求用的是较短的超时"
    );
}

#[tokio::test]
async fn an_unreachable_server_is_a_network_error() {
    // 先占一个端口再释放，得到一个大概率没人监听的地址
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("绑定端口");
    let addr = listener.local_addr().expect("取得地址");
    drop(listener);

    // Windows 对本机已关闭的端口会重试约两秒才报告连接被拒，超时要留得比这更长
    let client = BatraceClient::new(ClientConfig {
        base_url: format!("http://{addr}"),
        timeout: Duration::from_secs(20),
        ..ClientConfig::default()
    })
    .expect("配置合法");
    let err = client.units().await.expect_err("应当失败");
    assert_eq!(err.kind(), ErrorKind::Network);
    assert!(err.is_transient());
    assert!(
        !err.to_string().contains(&addr.to_string()),
        "错误信息里不带请求地址：{err}"
    );
}

#[test]
fn invalid_base_url_is_rejected_up_front() {
    for base in ["", "bot.batrace.top", "ftp://example.invalid"] {
        let err = BatraceClient::new(ClientConfig {
            base_url: base.to_owned(),
            ..ClientConfig::default()
        })
        .expect_err("应当失败");
        assert_eq!(err.kind(), ErrorKind::Config, "{base:?}");
    }
    // 末尾的斜杠不影响拼接
    assert!(
        BatraceClient::new(ClientConfig {
            base_url: "https://example.invalid/".to_owned(),
            ..ClientConfig::default()
        })
        .is_ok()
    );
}
