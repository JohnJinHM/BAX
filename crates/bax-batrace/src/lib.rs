//! BATrace（<https://bot.batrace.top>）接口客户端。
//!
//! 这个库只管发请求、把响应分门别类、把 JSON 解析成结构。
//! 什么时候刷新、失败后隔多久重试、结果写到哪里，都由调用方决定。
//!
//! 接口路径和请求头的经验来自 BA-ELO-Overlay 的 `fetch_json`（`src/ba_elo_overlay.py`）。
//!
//! # 响应里的缺省值
//!
//! 接口会省略取默认值的字段：`TeamId` 为 0、`WinnerTeam` 为 0、击杀数为 0 时，
//! 对应的键根本不出现。[`model`] 里这些字段都是 `Option`，原样反映“有没有这个键”，
//! 具体含义由各自的访问方法解释。

pub mod client;
pub mod error;
pub mod model;

pub use client::{BatraceClient, ClientConfig, Fetched};
pub use error::{ApiError, ErrorKind};

/// 站点地址。
pub const DEFAULT_BASE_URL: &str = "https://bot.batrace.top";
