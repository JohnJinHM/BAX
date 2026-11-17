//! 时间来源。
//!
//! 引擎里所有“什么时候”的判断（资料是否过期、何时重试、阶段推进）都通过 [`Clock`] 取当前时间，
//! 测试因此可以用 [`ManualClock`] 精确控制。

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use bax_core::Timestamp;

/// 当前时间的来源。
pub trait Clock: Send + Sync + fmt::Debug + 'static {
    fn now(&self) -> Timestamp;
}

/// 系统时间。
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        let ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        Timestamp::from_millis(ms)
    }
}

/// 手动拨动的时钟，克隆出来的副本共用同一个时间。
#[derive(Debug, Clone)]
pub struct ManualClock(Arc<AtomicI64>);

impl ManualClock {
    pub fn new(start: Timestamp) -> Self {
        ManualClock(Arc::new(AtomicI64::new(start.as_millis())))
    }

    pub fn set(&self, at: Timestamp) {
        self.0.store(at.as_millis(), Ordering::SeqCst);
    }

    pub fn advance_millis(&self, ms: i64) {
        self.0.fetch_add(ms, Ordering::SeqCst);
    }
}

impl Clock for ManualClock {
    fn now(&self) -> Timestamp {
        Timestamp::from_millis(self.0.load(Ordering::SeqCst))
    }
}
