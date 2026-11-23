use super::*;
use serde::Serialize;
use serde_json::{Value, json};

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpstreamConfig {
    pub api_url: String,
    pub reports_url: String,
}

impl Default for UpstreamConfig {
    fn default() -> Self {
        Self {
            api_url: "https://api.brokenarrowgame.tech".into(),
            reports_url: "https://s3.brokenarrowgame.tech".into(),
        }
    }
}

impl UpstreamConfig {
    pub(super) fn validate(&self) -> Result<(), ApiError> {
        for base in [&self.api_url, &self.reports_url] {
            let url =
                reqwest::Url::parse(base).map_err(|_| ApiError::Config("主源地址不合法".into()))?;
            if !matches!(url.scheme(), "https" | "http") || url.host_str().is_none() {
                return Err(ApiError::Config("主源地址不合法".into()));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceCheck {
    pub name: String,
    pub available: bool,
    pub message: String,
    pub latency_ms: u64,
    pub checked_at: u64,
}

pub(super) type CheckCache = Arc<tokio::sync::Mutex<Option<(Instant, Vec<SourceCheck>)>>>;

pub(super) type CacheSlot = Arc<tokio::sync::Mutex<Option<(Instant, String)>>>;

fn decode<T: DeserializeOwned>(value: Value) -> Result<Fetched<T>, ApiError> {
    let raw = value.to_string();
    let data = serde_json::from_str(&raw).map_err(|e| ApiError::Decode(e.to_string()))?;
    Ok(Fetched { data, raw })
}

// Only this integer target is emitted as a floating point number by the upstream.
// Preserve fractional damage/rating values; never silently truncate supply points.
fn normalize_report(value: &mut Value) -> Result<(), ApiError> {
    if let Some(players) = value.get_mut("Data").and_then(Value::as_object_mut) {
        for player in players.values_mut() {
            if let Some(units) = player.get_mut("UnitData").and_then(Value::as_object_mut) {
                for unit in units.values_mut() {
                    if let Some(v) = unit.get_mut("TotalSupplyPointsConsumed")
                        && !v.is_null()
                        && v.as_i64().is_none()
                    {
                        let n = v
                            .as_f64()
                            .filter(|n| {
                                n.is_finite()
                                    && n.fract() == 0.0
                                    && *n >= i64::MIN as f64
                                    && *n < 9_223_372_036_854_775_808.0
                            })
                            .ok_or_else(|| ApiError::Decode("单位补给点数不是有效整数".into()))?;
                        *v = json!(n as i64);
                    }
                }
            }
        }
    }
    Ok(())
}

fn merge(base: &mut Value, incoming: Value) {
    if incoming.is_null() {
        return;
    }
    if let Value::Object(fields) = incoming {
        if !base.is_object() {
            *base = json!({});
        }
        for (key, value) in fields {
            merge(&mut base[&key], value);
        }
    } else {
        *base = incoming;
    }
}

fn profile(user: &Value, id: PlayerId) -> Result<Value, ApiError> {
    if user.as_object().is_some_and(|o| o.is_empty()) {
        return Err(ApiError::NotFound);
    }
    if user.get("id").and_then(Value::as_u64) != Some(id.get()) {
        return Err(ApiError::Decode("主源玩家 ID 不匹配".into()));
    }
    let mut info = json!({});
    for (from, to) in [
        ("id", "id"),
        ("name", "name"),
        ("steamId", "steamId"),
        ("lvl", "level"),
        ("rt", "rating"),
        ("rk", "rank"),
        ("rtgms", "ratingGames"),
    ] {
        if let Some(v) = user.get(from).filter(|v| !v.is_null()) {
            info[to] = v.clone();
        }
    }
    // Reject wrong field types before accepting the upstream as a successful profile.
    let _: crate::model::PlayerInfo =
        serde_json::from_value(info.clone()).map_err(|e| ApiError::Decode(e.to_string()))?;
    Ok(json!({"info": info}))
}

fn statistics(value: &Value) -> Value {
    let mut stats = json!({});
    // Empty mode objects mean unavailable, not zero. Map only observed integer counters.
    for (mode, suffix) in [("Rating", "Rt"), ("Custom", "Ct"), ("Skirmish", "Sk")] {
        for (from, to) in [
            ("fightsCount", "fightsCount"),
            ("winsCount", "winCount"),
            ("losesCount", "lossCount"),
            ("leavesCount", "leaveCount"),
            ("killsCount", "unitsKilledCount"),
            ("deathsCount", "unitsLostCount"),
            ("totalMatchTimeSec", "matchDurationTimeSec"),
        ] {
            if let Some(n) = value["statisticByLobbyType"][mode][from].as_i64() {
                stats[format!("{to}{suffix}")] = json!(n);
            }
        }
    }
    for (from, to) in [
        ("experienceCount", "expCount"),
        ("level", "levelCount"),
        ("killsFriendlyFireCount", "unitsKilledFriendlyFireCount"),
        ("deathsByFriendlyFireCount", "unitsLostFriendlyFireCount"),
        ("capturedZonesCount", "zoneCapturedCount"),
        ("supplyPointsConsumed", "supplyPtsConsumed"),
        ("supplyCapturedCount", "supplyCapturedCount"),
        ("supplyCapturedByEnemyCount", "supplyCapturedByEnemyCount"),
    ] {
        if let Some(n) = value[from].as_i64() {
            stats[to] = json!(n);
        }
    }
    // updateDate is a response-generation time, not a verified snapshot timestamp.
    stats
}

fn fallback_error(primary: ApiError, secondary: ApiError) -> ApiError {
    // A missing record on the fallback must not turn a primary outage into permanent absence.
    if matches!(secondary, ApiError::NotFound) {
        primary
    } else {
        secondary
    }
}

impl BatraceClient {
    pub(super) async fn fetch_at<T: DeserializeOwned>(
        &self,
        source: &str,
        base: &str,
        endpoint: &'static str,
        path: &str,
        timeout: Option<Duration>,
    ) -> Result<Fetched<T>, ApiError> {
        self.fetch_parsed(source, base, endpoint, path, timeout, |raw| {
            serde_json::from_str(raw).map_err(|e| ApiError::Decode(e.to_string()))
        })
        .await
    }

    async fn fetch_parsed<T>(
        &self,
        source: &str,
        base: &str,
        endpoint: &'static str,
        path: &str,
        timeout: Option<Duration>,
        parse: impl Fn(&str) -> Result<T, ApiError>,
    ) -> Result<Fetched<T>, ApiError> {
        let url = format!("{}{path}", base.trim_end_matches('/'));
        let ttl = match endpoint {
            "match" => 24 * 3600,
            "players.info" if source == "batrace" => 15 * 60,
            "players.stats" => 15 * 60,
            "units" | "specializations" => 24 * 3600,
            _ => 60,
        };
        let slot = if self.upstream.is_some() {
            let mut state = self.shared.lock().expect("request state");
            if state.cache.len() >= 128 {
                state.cache.retain(|_, slot| Arc::strong_count(slot) > 1);
            }
            Some(state.cache.entry(url.clone()).or_default().clone())
        } else {
            None
        };
        // Coalesce concurrent requests for the same object; unrelated players run independently.
        let mut cached = match &slot {
            Some(slot) => Some(slot.lock().await),
            None => None,
        };
        if let Some(guard) = &cached
            && let Some((at, raw)) = &**guard
            && at.elapsed() < Duration::from_secs(ttl)
        {
            let data = parse(raw)?;
            return Ok(Fetched {
                data,
                raw: raw.clone(),
            });
        }
        if let Some((until, err)) = self
            .shared
            .lock()
            .expect("request state")
            .cooldowns
            .get(source)
            && *until > Instant::now()
        {
            return Err(err.clone());
        }
        let result = async {
            tracing::debug!(endpoint, source, "API 请求开始");
            let response = self
                .http
                .get(&url)
                .timeout(timeout.unwrap_or(self.timeout))
                .send()
                .await
                .map_err(transport_error)?;
            let is_json = response
                .headers()
                .get(CONTENT_TYPE)
                .and_then(|h| h.to_str().ok())
                .is_some_and(|h| h.to_ascii_lowercase().contains("json"));
            let status = response.status();
            if !status.is_success() {
                // Legacy host's plain 404 indicates a missing route, not a missing player/report.
                if self.upstream.is_some()
                    && source == "batrace"
                    && status == StatusCode::NOT_FOUND
                    && !is_json
                {
                    return Err(ApiError::Server { status: 404 });
                }
                return Err(status_error(status, &response));
            }
            let raw = response.text().await.map_err(transport_error)?;
            if !is_json {
                return Err(ApiError::Challenge);
            }
            let data = parse(&raw)?;
            Ok(Fetched { data, raw })
        }
        .await;
        match &result {
            Ok(fetched) => {
                if let Some(guard) = &mut cached {
                    // Bound memory for large reports. Persistent archives remain in bax-store.
                    if fetched.raw.len() <= 512 * 1024 {
                        **guard = Some((Instant::now(), fetched.raw.clone()));
                    }
                }
            }
            Err(err) => {
                let delay = if let ApiError::RateLimited { retry_after } = err {
                    Some(
                        retry_after
                            .unwrap_or(Duration::from_secs(30))
                            .max(Duration::from_secs(30)),
                    )
                } else if self.upstream.is_some()
                    && !matches!(err, ApiError::NotFound | ApiError::Decode(_))
                {
                    Some(Duration::from_secs(60))
                } else {
                    None
                };
                if let Some(delay) = delay {
                    self.shared
                        .lock()
                        .expect("request state")
                        .cooldowns
                        .insert(source.into(), (Instant::now() + delay, err.clone()));
                }
                tracing::debug!(endpoint, source, error = %err, "API 请求失败");
            }
        }
        result
    }

    async fn batrace_player(&self, id: PlayerId) -> Result<Fetched<Value>, ApiError> {
        let r: Fetched<Value> = self
            .get(
                "players.info",
                &format!("/api/players/info?stbid={id}"),
                Some(self.player_info_timeout),
            )
            .await?;
        let parsed: Fetched<PlayerInfoResponse> = decode(r.data.clone())?;
        if parsed.data.info.as_ref().and_then(|p| p.id) != Some(id.get()) {
            return Err(ApiError::NotFound);
        }
        Ok(r)
    }

    pub(super) async fn combined_player(
        &self,
        id: PlayerId,
    ) -> Result<Fetched<PlayerInfoResponse>, ApiError> {
        let upstream = self.upstream.as_ref().expect("upstream enabled");
        let primary = self
            .fetch_at::<Value>(
                "upstream",
                &upstream.api_url,
                "players.info",
                &format!("/user/{id}"),
                Some(self.player_info_timeout),
            )
            .await
            .and_then(|u| profile(&u.data, id).map(|p| (u.data, p)));
        let (user, mut primary) = match primary {
            Ok(value) => value,
            Err(err) => {
                self.shared
                    .lock()
                    .expect("request state")
                    .cache
                    .remove(&format!(
                        "{}/user/{id}",
                        upstream.api_url.trim_end_matches('/')
                    ));
                return match self.batrace_player(id).await {
                    Ok(r) => decode(r.data),
                    Err(other) => Err(fallback_error(err, other)),
                };
            }
        };
        let recent_path = format!("/user/{id}/last_fights");
        let stats_path = user.get("marketId").and_then(Value::as_str).map(|market| {
            let mut url = reqwest::Url::parse("https://localhost").expect("static URL");
            url.path_segments_mut()
                .expect("base URL")
                .extend(["statistic", "personal", market]);
            url.path().to_owned()
        });
        let stats_request = async {
            match stats_path {
                Some(path) => self
                    .fetch_at::<Value>(
                        "upstream",
                        &upstream.api_url,
                        "players.stats",
                        &path,
                        Some(self.player_info_timeout),
                    )
                    .await
                    .ok(),
                None => None,
            }
        };
        let (supplement, recent, stats) = tokio::join!(
            self.batrace_player(id),
            self.fetch_at::<Vec<String>>(
                "upstream",
                &upstream.api_url,
                "players.recent",
                &recent_path,
                Some(self.player_info_timeout)
            ),
            stats_request
        );
        let mut result = supplement.map(|r| r.data).unwrap_or_else(|_| json!({}));
        if let Some(stats) = stats {
            primary["statInfo"] = statistics(&stats.data);
        }
        if let Ok(recent) = recent {
            primary["last_fights_data"] = json!(recent.data);
        }
        merge(&mut result, primary);
        decode(result)
    }

    pub(super) async fn combined_report(
        &self,
        id: MatchId,
    ) -> Result<Fetched<MatchInfo>, ApiError> {
        let upstream = self.upstream.as_ref().expect("upstream enabled");
        let primary = self
            .fetch_parsed(
                "reports",
                &upstream.reports_url,
                "match",
                &format!("/fights/fight_{id}.json"),
                None,
                |raw| {
                    let mut value =
                        serde_json::from_str(raw).map_err(|e| ApiError::Decode(e.to_string()))?;
                    normalize_report(&mut value)?;
                    let info: MatchInfo = serde_json::from_value(value.clone())
                        .map_err(|e| ApiError::Decode(e.to_string()))?;
                    if info.is_empty() {
                        return Err(ApiError::NotFound);
                    }
                    Ok((info, value))
                },
            )
            .await;
        match primary {
            Ok(report) => {
                let (data, value) = report.data;
                Ok(Fetched {
                    data,
                    raw: json!({"matchInfo": value}).to_string(),
                })
            }
            Err(err) => self
                .fetch_parsed(
                    "batrace",
                    &self.base_url,
                    "match",
                    &format!("/api/match?matchid={id}"),
                    None,
                    |raw| {
                        let response: MatchReportResponse = serde_json::from_str(raw)
                            .map_err(|e| ApiError::Decode(e.to_string()))?;
                        response
                            .match_info
                            .filter(|info| !info.is_empty())
                            .ok_or(ApiError::NotFound)
                    },
                )
                .await
                .map_err(|other| fallback_error(err, other)),
        }
    }

    /// Manual, endpoint-specific checks. Fresh requests, shared 30s cooldown between button presses.
    /// A report object may expire: its absence is shown as a sample failure, not a global outage.
    pub async fn check_sources(&self) -> Vec<SourceCheck> {
        let mut last = self.checks.lock().await;
        if let Some((at, checks)) = &*last
            && at.elapsed() < Duration::from_secs(30)
        {
            return checks.clone();
        }
        let mut targets = vec![
            (
                "BATrace · 玩家补充",
                format!("{}/api/players/info?stbid=367171", self.base_url),
                2,
            ),
            (
                "BATrace · 昵称搜索",
                format!("{}/api/players/search?q=JohnLemon&limit=1", self.base_url),
                3,
            ),
            (
                "BATrace · 单位目录",
                format!("{}/api/units", self.base_url),
                4,
            ),
        ];
        if let Some(upstream) = &self.upstream {
            targets.insert(
                0,
                (
                    "主源 · 玩家资料",
                    format!("{}/user/367171", upstream.api_url.trim_end_matches('/')),
                    0,
                ),
            );
            targets.insert(
                1,
                (
                    "主源 · 战报样本",
                    format!(
                        "{}/fights/fight_9845804.json",
                        upstream.reports_url.trim_end_matches('/')
                    ),
                    1,
                ),
            );
        }
        let mut jobs = tokio::task::JoinSet::new();
        for (index, (name, url, kind)) in targets.into_iter().enumerate() {
            let http = self.http.clone();
            jobs.spawn(async move {
                let started = Instant::now();
                let result = async {
                    let response = http
                        .get(url)
                        .timeout(Duration::from_secs(8))
                        .send()
                        .await
                        .map_err(transport_error)?;
                    if !response.status().is_success() {
                        return Err(status_error(response.status(), &response));
                    }
                    let mut value: Value = response
                        .json()
                        .await
                        .map_err(|_| ApiError::Decode("返回内容不是有效 JSON".into()))?;
                    let valid = match kind {
                        0 => profile(&value, PlayerId(367171)).is_ok(),
                        1 => {
                            normalize_report(&mut value).is_ok()
                                && decode::<MatchInfo>(value.clone())
                                    .is_ok_and(|r| !r.data.is_empty())
                        }
                        2 => decode::<PlayerInfoResponse>(value.clone())
                            .is_ok_and(|r| r.data.info.is_some()),
                        3 => value.get("players").is_some_and(Value::is_array),
                        _ => decode::<UnitsResponse>(value).is_ok_and(|r| !r.data.units.is_empty()),
                    };
                    if valid {
                        Ok(())
                    } else {
                        Err(ApiError::Decode("响应缺少所需数据".into()))
                    }
                }
                .await;
                (
                    index,
                    SourceCheck {
                        name: name.into(),
                        available: result.is_ok(),
                        message: match result {
                            Ok(()) => "样本查询成功".into(),
                            Err(ApiError::NotFound) if kind == 1 => {
                                "样本战报不存在或已过期，服务状态未确认".into()
                            }
                            Err(ApiError::NotFound) => "接口返回 HTTP 404，未通过样本检查".into(),
                            Err(e) => e.to_string(),
                        },
                        latency_ms: started.elapsed().as_millis() as u64,
                        checked_at: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs(),
                    },
                )
            });
        }
        let mut checks = Vec::new();
        while let Some(Ok(row)) = jobs.join_next().await {
            checks.push(row);
        }
        checks.sort_by_key(|(index, _)| *index);
        let checks: Vec<_> = checks.into_iter().map(|(_, row)| row).collect();
        *last = Some((Instant::now(), checks.clone()));
        checks
    }
}
