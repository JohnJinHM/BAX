//! 请求失败的分类。
//!
//! 调用方需要区分几种情况来决定下一步：稍后重试、退回缓存、还是告诉用户站点没有这条数据。

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// 一次请求失败的原因。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ApiError {
    /// 在限定时间内没有得到响应。
    #[error("请求超时")]
    Timeout,
    /// 连接不上，或者连接中途断开。
    #[error("网络错误：{0}")]
    Network(String),
    /// 站点要求放慢请求（HTTP 429）。
    #[error("请求过于频繁，被站点限流")]
    RateLimited { retry_after: Option<Duration> },
    /// 站点拒绝了请求（HTTP 401、403 等）。
    #[error("请求被站点拒绝（HTTP {status}）")]
    Rejected { status: u16 },
    /// 状态码正常，但返回的不是 JSON。通常是人机验证页面。
    #[error("站点返回了非 JSON 内容，可能是人机验证")]
    Challenge,
    /// 站点没有这条数据：玩家不存在，或者比赛尚未收录。
    #[error("站点没有收录这条数据")]
    NotFound,
    /// 站点自身出错（HTTP 5xx）。
    #[error("站点出错（HTTP {status}）")]
    Server { status: u16 },
    /// 响应是 JSON，但结构与预期不符。
    #[error("响应无法解析：{0}")]
    Decode(String),
    /// 客户端自身的配置有问题，例如地址不合法。
    #[error("客户端配置错误：{0}")]
    Config(String),
}

/// [`ApiError`] 去掉细节之后的类别，可以直接交给界面。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum ErrorKind {
    Timeout,
    Network,
    RateLimited,
    Rejected,
    NotFound,
    Server,
    Decode,
    Config,
}

impl ApiError {
    pub fn kind(&self) -> ErrorKind {
        match self {
            ApiError::Timeout => ErrorKind::Timeout,
            ApiError::Network(_) => ErrorKind::Network,
            ApiError::RateLimited { .. } => ErrorKind::RateLimited,
            ApiError::Rejected { .. } | ApiError::Challenge => ErrorKind::Rejected,
            ApiError::NotFound => ErrorKind::NotFound,
            ApiError::Server { .. } => ErrorKind::Server,
            ApiError::Decode(_) => ErrorKind::Decode,
            ApiError::Config(_) => ErrorKind::Config,
        }
    }

    /// 过一会儿再试有没有可能成功。
    ///
    /// 超时、网络错误、限流和站点出错是暂时的。“没有收录”对刚结束的比赛来说也是暂时的，
    /// 但那需要调用方按自己的节奏重试，不算在这里。
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            ApiError::Timeout
                | ApiError::Network(_)
                | ApiError::RateLimited { .. }
                | ApiError::Server { .. }
        )
    }

    /// 站点指定的等待时间。
    pub fn retry_after(&self) -> Option<Duration> {
        match self {
            ApiError::RateLimited { retry_after } => *retry_after,
            _ => None,
        }
    }
}
