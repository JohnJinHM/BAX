//! 时间表示。

use serde::{Deserialize, Serialize};

/// UTC 时间戳，单位毫秒，自 Unix 纪元起算。
///
/// 核心库只做时间的加减和比较。把日志里的本地时间换算成 UTC、读取当前时间，
/// 都是调用方的事。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Timestamp(pub i64);

impl Timestamp {
    pub const fn from_millis(ms: i64) -> Self {
        Timestamp(ms)
    }

    pub const fn from_secs(secs: i64) -> Self {
        Timestamp(secs * 1000)
    }

    pub const fn as_millis(self) -> i64 {
        self.0
    }

    /// 向后推 `ms` 毫秒；`ms` 为负时向前。
    pub const fn plus_millis(self, ms: i64) -> Self {
        Timestamp(self.0.saturating_add(ms))
    }

    /// `self - earlier`，单位毫秒。
    pub const fn millis_since(self, earlier: Timestamp) -> i64 {
        self.0.saturating_sub(earlier.0)
    }
}
