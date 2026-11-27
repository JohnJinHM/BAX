use super::*;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

fn client(primary: &MockServer, fallback: &MockServer) -> BatraceClient {
    BatraceClient::new(ClientConfig {
        base_url: fallback.uri(),
        upstream: Some(UpstreamConfig {
            api_url: primary.uri(),
            reports_url: primary.uri(),
        }),
        ..Default::default()
    })
    .expect("valid fixture or successful mock response")
}

async fn mock(server: &MockServer, route: &str, value: Value, calls: u64) {
    Mock::given(method("GET"))
        .and(path(route))
        .respond_with(ResponseTemplate::new(200).set_body_json(value))
        .expect(calls)
        .mount(server)
        .await;
}

fn report() -> Value {
    json!({"Type":1,"StartTime":1000,"Data":{
        "12":{"Id":12,"UnitData":{"a":{"Id":10,"SpawnTime":1001,"TotalSupplyPointsConsumed":267.0,"TotalDamageDealt":3.25}},"TransferredUnits":{"b":{"Id":11,"SpawnTime":1002}}},
        "-2":{"Id":-2,"TeamId":1},"13":{"Id":13,"TeamId":100}
    }})
}

#[tokio::test]
async fn optional_trace_route_requires_a_count_and_its_outage_leaves_search_available() {
    let primary = MockServer::start().await;
    let fallback = MockServer::start().await;
    let client = client(&primary, &fallback);
    mock(&fallback, "/api/players/10/trace", json!({"logs":null}), 1).await;
    mock(
        &fallback,
        "/api/players/11/trace",
        json!({"traceCount":-1}),
        1,
    )
    .await;
    for id in [10, 11] {
        assert!(matches!(
            client.player_trace_count(PlayerId(id)).await,
            Err(ApiError::Decode(_))
        ));
    }
    Mock::given(path("/api/players/12/trace"))
        .respond_with(ResponseTemplate::new(503))
        .expect(1)
        .mount(&fallback)
        .await;
    assert!(client.player_trace_count(PlayerId(12)).await.is_err());
    mock(
        &fallback,
        "/api/players/search",
        json!({"players":[],"count":0}),
        1,
    )
    .await;
    assert!(client.search_players("Raven").await.is_ok());
}

#[tokio::test]
async fn empty_report_is_not_cached_and_later_archive_can_succeed() {
    let primary = MockServer::start().await;
    let fallback = MockServer::start().await;
    Mock::given(path("/fights/fight_88.json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"Data":{}})))
        .up_to_n_times(1)
        .with_priority(1)
        .expect(1)
        .mount(&primary)
        .await;
    Mock::given(path("/fights/fight_88.json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(report()))
        .with_priority(2)
        .expect(1)
        .mount(&primary)
        .await;
    mock(&fallback, "/api/match", json!({"matchInfo":null}), 0).await;
    let client = client(&primary, &fallback);
    assert!(matches!(
        client.match_report(MatchId(88)).await,
        Err(ApiError::NotFound)
    ));
    assert!(client.match_report(MatchId(88)).await.is_ok());
}

#[tokio::test]
async fn missing_primary_report_does_not_request_batrace() {
    let primary = MockServer::start().await;
    let fallback = MockServer::start().await;
    assert!(
        client(&primary, &fallback)
            .match_report(MatchId(88))
            .await
            .is_err()
    );
    assert!(
        fallback
            .received_requests()
            .await
            .expect("recorded requests")
            .is_empty()
    );
}

#[tokio::test]
async fn expired_cache_is_refreshed_and_primary_recovers_after_cooldown() {
    let primary = MockServer::start().await;
    let fallback = MockServer::start().await;
    Mock::given(path("/user/12"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(1)
        .with_priority(1)
        .expect(1)
        .mount(&primary)
        .await;
    mock(&primary, "/user/12", json!({"id":12,"rt":2000}), 2).await;
    mock(&primary, "/user/12/last_fights", json!([]), 1).await;
    mock(
        &fallback,
        "/api/players/info",
        json!({"info":{"id":12,"rating":1234}}),
        0,
    )
    .await;
    let client = client(&primary, &fallback);
    assert!(client.player_info(PlayerId(12)).await.is_err());
    client
        .shared
        .lock()
        .expect("valid fixture or successful mock response")
        .cooldowns
        .clear();
    assert_eq!(
        client
            .player_info(PlayerId(12))
            .await
            .expect("valid fixture or successful mock response")
            .data
            .info
            .expect("valid fixture or successful mock response")
            .rating,
        Some(2000.0)
    );
    let slot = client
        .shared
        .lock()
        .expect("valid fixture or successful mock response")
        .cache[&format!("{}/user/12", primary.uri())]
        .clone();
    slot.lock()
        .await
        .as_mut()
        .expect("valid fixture or successful mock response")
        .0 = Instant::now() - Duration::from_secs(61);
    assert_eq!(
        client
            .player_info(PlayerId(12))
            .await
            .expect("valid fixture or successful mock response")
            .data
            .info
            .expect("valid fixture or successful mock response")
            .rating,
        Some(2000.0)
    );
}

#[tokio::test]
async fn report_normalization_cache_and_concurrent_dedup_preserve_modes() {
    let primary = MockServer::start().await;
    let fallback = MockServer::start().await;
    mock(&primary, "/fights/fight_88.json", report(), 1).await;
    let client = client(&primary, &fallback);
    let (a, b) = tokio::join!(
        client.match_report(MatchId(88)),
        client.match_report(MatchId(88))
    );
    let a = a.expect("valid fixture or successful mock response");
    assert_eq!(a, b.expect("valid fixture or successful mock response"));
    assert_eq!(
        a.data.data["12"].unit_data["a"].total_supply_points_consumed,
        Some(267)
    );
    assert_eq!(
        a.data.data["12"].unit_data["a"].total_damage_dealt,
        Some(3.25)
    );
    assert_eq!(a.data.data["12"].team(), Some(bax_core::Team::Alpha));
    assert_eq!(a.data.winner(), Some(bax_core::Team::Alpha));
    assert_eq!(a.data.players().count(), 2); // AI excluded; spectator retained for engine separation.
    assert_eq!(a.data.match_type, Some(1));
    assert_eq!(a.data.data["12"].transferred_units.len(), 1);
    assert!(
        serde_json::from_str::<MatchReportResponse>(&a.raw)
            .expect("valid fixture or successful mock response")
            .match_info
            .is_some()
    );
    assert!(
        fallback
            .received_requests()
            .await
            .expect("valid fixture or successful mock response")
            .is_empty()
    );
}

#[tokio::test]
async fn fractional_integer_target_is_rejected_without_fallback() {
    let primary = MockServer::start().await;
    let fallback = MockServer::start().await;
    let mut data = report();
    data["Data"]["12"]["UnitData"]["a"]["TotalSupplyPointsConsumed"] = json!(1.5);
    mock(&primary, "/fights/fight_88.json", data, 1).await;
    mock(
        &fallback,
        "/api/match",
        json!({"matchInfo":{"Data":{"12":{"Id":12}}}}),
        0,
    )
    .await;
    let result = client(&primary, &fallback).match_report(MatchId(88)).await;
    assert!(matches!(result, Err(ApiError::Decode(_))));
}

#[tokio::test]
async fn primary_profile_uses_only_stb_endpoints() {
    let primary = MockServer::start().await;
    let fallback = MockServer::start().await;
    mock(
        &primary,
        "/user/12",
        json!({"id":12,"name":"New","lvl":4,"rt":2300.5,"rk":9,"rtgms":11,"marketId":"steam12"}),
        1,
    )
    .await;
    mock(&primary, "/user/12/last_fights", json!(["88"]), 1).await;
    mock(&primary,"/statistic/personal/steam12",json!({"statisticByLobbyType":{"Rating":{"winsCount":0,"killsCount":30},"Custom":{}},"updateDate":"unverified response time"}),1).await;
    mock(&fallback,"/api/players/info",json!({"info":{"id":12,"name":"Old","rating":1000},"statInfo":{"winCountRt":5,"unitsKilledCostRt":9000,"fightsCountCt":20,"specExp":{"3":400}},"last_fights_data":["77"]}),0).await;
    let client = client(&primary, &fallback);
    let a = client
        .player_info(PlayerId(12))
        .await
        .expect("valid fixture or successful mock response");
    assert_eq!(
        a.data
            .info
            .as_ref()
            .expect("valid fixture or successful mock response")
            .rating,
        Some(2300.5)
    );
    assert_eq!(
        a.data
            .info
            .as_ref()
            .expect("valid fixture or successful mock response")
            .name
            .as_deref(),
        Some("New")
    );
    let stats = a
        .data
        .stat_info
        .expect("valid fixture or successful mock response");
    assert_eq!(stats.win_count_rt, Some(0));
    assert_eq!(stats.units_killed_cost_rt, None);
    assert_eq!(stats.fights_count_ct, None);
    assert!(stats.spec_exp.is_empty());
    assert_eq!(stats.updated_at, None);
    assert_eq!(a.data.last_fights_data, vec!["88"]);
    client
        .player_info(PlayerId(12))
        .await
        .expect("valid fixture or successful mock response"); // TTL cache covers all four requests.
}

#[tokio::test]
async fn missing_supplements_do_not_hide_primary_profile() {
    let primary = MockServer::start().await;
    let fallback = MockServer::start().await;
    mock(&primary, "/user/12", json!({"id":12,"rt":2000}), 1).await;
    mock(&fallback, "/api/players/info", json!({"info":null}), 0).await;
    let result = client(&primary, &fallback)
        .player_info(PlayerId(12))
        .await
        .expect("valid fixture or successful mock response");
    assert_eq!(
        result
            .data
            .info
            .expect("valid fixture or successful mock response")
            .rating,
        Some(2000.0)
    );
    assert!(result.data.stat_info.is_none());
    let raw: Value =
        serde_json::from_str(&result.raw).expect("valid fixture or successful mock response");
    assert!(raw.get("last_fights_data").is_none()); // missing must not erase durable recent IDs.
}

#[tokio::test]
async fn independent_cooldowns_keep_reports_available() {
    let primary = MockServer::start().await;
    let fallback = MockServer::start().await;
    Mock::given(path("/user/12"))
        .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "90"))
        .expect(1)
        .mount(&primary)
        .await;
    mock(
        &fallback,
        "/api/players/info",
        json!({"info":{"id":12,"rating":1234}}),
        0,
    )
    .await;
    mock(&primary, "/fights/fight_88.json", report(), 1).await;
    let client = client(&primary, &fallback);
    for _ in 0..2 {
        assert!(client.player_info(PlayerId(12)).await.is_err());
    }
    assert!(client.match_report(MatchId(88)).await.is_ok());
}

#[tokio::test]
async fn primary_outage_is_not_masked_as_missing_by_fallback() {
    let primary = MockServer::start().await;
    let fallback = MockServer::start().await;
    Mock::given(path("/user/12"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&primary)
        .await;
    mock(&fallback, "/api/players/info", json!({"info":null}), 0).await;
    assert!(matches!(
        client(&primary, &fallback).player_info(PlayerId(12)).await,
        Err(ApiError::Server { status: 503 })
    ));
}

#[tokio::test]
async fn empty_primary_user_returns_not_found() {
    let primary = MockServer::start().await;
    let fallback = MockServer::start().await;
    mock(&primary, "/user/12", json!({}), 1).await;
    Mock::given(path("/api/players/info"))
        .respond_with(ResponseTemplate::new(404).set_body_string("404 page not found"))
        .expect(0)
        .mount(&fallback)
        .await;
    assert!(matches!(
        client(&primary, &fallback).player_info(PlayerId(12)).await,
        Err(ApiError::NotFound)
    ));
}

#[tokio::test]
async fn source_checks_validate_payloads_and_throttle_button_presses() {
    let primary = MockServer::start().await;
    let fallback = MockServer::start().await;
    mock(&primary, "/user/367171", json!({"id":367171}), 1).await;
    mock(&primary, "/fights/fight_9845804.json", report(), 1).await;
    mock(&fallback, "/api/players/info", json!({"info":null}), 0).await;
    mock(&fallback, "/api/players/search", json!({"players":[]}), 1).await;
    mock(&fallback, "/api/units", json!({"units":[{"id":1}]}), 1).await;
    let client = client(&primary, &fallback);
    let a = client.check_sources().await;
    let b = client.check_sources().await;
    assert_eq!(a.len(), 5);
    assert_eq!(b.len(), 5);
    assert!(
        a[0].available && a[1].available && !a[2].available && a[3].available && a[4].available
    );
}
