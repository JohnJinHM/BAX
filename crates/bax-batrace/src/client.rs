//! HTTP 客户端。

use std::time::Duration;

use bax_core::{MatchId, PlayerId};
use reqwest::header::{ACCEPT, CONTENT_TYPE, HeaderMap, HeaderValue, RETRY_AFTER};
use reqwest::{Response, StatusCode};
use serde::de::DeserializeOwned;

use crate::DEFAULT_BASE_URL;
use crate::error::ApiError;
use crate::model::{
    MatchInfo, MatchReportResponse, PlayerInfoResponse, PlayerMatchesResponse, UnitsResponse,
};

/// 客户端配置。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientConfig {
    /// 站点地址，不带末尾的斜杠。
    pub base_url: String,
    /// 应用标识，同时用作 `User-Agent` 的产品名和 `X-Client` 的值。
    ///
    /// 原型发现不带清晰标识的请求会被站点以 403 拒绝，所以每个请求都带上它。
    pub client_name: String,
    pub client_version: String,
    /// 一般请求的超时。
    pub timeout: Duration,
    /// 单个玩家资料请求的超时。开局时要并发查询十名玩家，
    /// 这个值短一些，一个卡住的请求才不会拖住整批。
    pub player_info_timeout: Duration,
    /// 是否走系统代理。默认不走：原型的经验是经过代理访问这个站点容易变慢或被拒绝。
    pub use_system_proxy: bool,
}

impl Default for ClientConfig {
    fn default() -> Self {
        ClientConfig {
            base_url: DEFAULT_BASE_URL.to_owned(),
            client_name: "BAX".to_owned(),
            client_version: env!("CARGO_PKG_VERSION").to_owned(),
            timeout: Duration::from_secs(15),
            player_info_timeout: Duration::from_secs(8),
            use_system_proxy: false,
        }
    }
}

/// 一次成功的响应：解析后的结构，加上原始 JSON 文本。
///
/// 原文留给存储层归档。结构里没有列出的字段因此不会丢，以后口径变了也能重新解析。
#[derive(Debug, Clone, PartialEq)]
pub struct Fetched<T> {
    pub data: T,
    pub raw: String,
}

/// BATrace 客户端。克隆的代价很低，内部共用同一个连接池。
#[derive(Debug, Clone)]
pub struct BatraceClient {
    http: reqwest::Client,
    base_url: String,
    player_info_timeout: Duration,
}

impl BatraceClient {
    pub fn new(config: ClientConfig) -> Result<Self, ApiError> {
        let base_url = config.base_url.trim_end_matches('/').to_owned();
        if !(base_url.starts_with("https://") || base_url.starts_with("http://")) {
            return Err(ApiError::Config(format!("地址不合法：{base_url}")));
        }
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        headers.insert(
            "X-Client",
            HeaderValue::from_str(&config.client_name)
                .map_err(|_| ApiError::Config("应用标识含有非法字符".to_owned()))?,
        );
        let mut builder = reqwest::Client::builder()
            .user_agent(format!(
                "{}/{} (+local tool)",
                config.client_name, config.client_version
            ))
            .default_headers(headers)
            .timeout(config.timeout)
            .connect_timeout(config.timeout.min(Duration::from_secs(10)));
        if !config.use_system_proxy {
            builder = builder.no_proxy();
        }
        let http = builder
            .build()
            .map_err(|err| ApiError::Config(err.to_string()))?;
        Ok(BatraceClient {
            http,
            base_url,
            player_info_timeout: config.player_info_timeout,
        })
    }

    /// 玩家资料、累计统计和近期比赛 ID。站点没有这名玩家时返回 [`ApiError::NotFound`]。
    pub async fn player_info(&self, id: PlayerId) -> Result<Fetched<PlayerInfoResponse>, ApiError> {
        let fetched: Fetched<PlayerInfoResponse> = self
            .get(
                &format!("/api/players/info?stbid={id}"),
                Some(self.player_info_timeout),
            )
            .await?;
        if fetched.data.info.is_none() {
            return Err(ApiError::NotFound);
        }
        Ok(fetched)
    }

    /// 玩家的比赛摘要，按时间从新到旧分页。
    pub async fn player_matches(
        &self,
        id: PlayerId,
        offset: u32,
        limit: u32,
    ) -> Result<Fetched<PlayerMatchesResponse>, ApiError> {
        self.get(
            &format!("/api/players/matches?stbid={id}&offset={offset}&limit={limit}"),
            None,
        )
        .await
    }

    /// 完整战报。比赛尚未收录、或者战报还是空壳时返回 [`ApiError::NotFound`]。
    pub async fn match_report(&self, id: MatchId) -> Result<Fetched<MatchInfo>, ApiError> {
        let fetched: Fetched<MatchReportResponse> =
            self.get(&format!("/api/match?matchid={id}"), None).await?;
        match fetched.data.match_info {
            Some(info) if !info.is_empty() => Ok(Fetched {
                data: info,
                raw: fetched.raw,
            }),
            _ => Err(ApiError::NotFound),
        }
    }

    /// 单位目录。
    pub async fn units(&self) -> Result<Fetched<UnitsResponse>, ApiError> {
        self.get("/api/units", None).await
    }

    /// 专精目录。响应结构尚未确认，先按原样返回。
    pub async fn specializations(&self) -> Result<Fetched<serde_json::Value>, ApiError> {
        self.get("/api/dek/specializations", None).await
    }

    async fn get<T: DeserializeOwned>(
        &self,
        path: &str,
        timeout: Option<Duration>,
    ) -> Result<Fetched<T>, ApiError> {
        let mut request = self.http.get(format!("{}{path}", self.base_url));
        if let Some(timeout) = timeout {
            request = request.timeout(timeout);
        }
        let response = request.send().await.map_err(transport_error)?;
        let status = response.status();
        if !status.is_success() {
            let err = status_error(status, &response);
            tracing::debug!(%status, path, "请求失败");
            return Err(err);
        }
        let is_json = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.to_ascii_lowercase().contains("json"));
        let raw = response.text().await.map_err(transport_error)?;
        if !is_json {
            return Err(ApiError::Challenge);
        }
        let data = serde_json::from_str(&raw).map_err(|err| ApiError::Decode(err.to_string()))?;
        Ok(Fetched { data, raw })
    }
}

fn transport_error(err: reqwest::Error) -> ApiError {
    if err.is_timeout() {
        ApiError::Timeout
    } else {
        // 连接不上，或者响应体读到一半断开。去掉地址，错误信息里不带查询的玩家 ID
        ApiError::Network(err.without_url().to_string())
    }
}

fn status_error(status: StatusCode, response: &Response) -> ApiError {
    match status {
        StatusCode::TOO_MANY_REQUESTS => ApiError::RateLimited {
            retry_after: response
                .headers()
                .get(RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.trim().parse::<u64>().ok())
                .map(Duration::from_secs),
        },
        StatusCode::NOT_FOUND => ApiError::NotFound,
        s if s.is_server_error() => ApiError::Server { status: s.as_u16() },
        s => ApiError::Rejected { status: s.as_u16() },
    }
}

#[cfg(test)]
mod tests;
