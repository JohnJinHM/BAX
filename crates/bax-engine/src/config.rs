//! 引擎配置与数据文件位置。

use std::env;
use std::path::{Path, PathBuf};
use std::time::Duration;

use bax_batrace::ClientConfig;
use bax_core::TrackerConfig;
use bax_gamelog::LogZone;
use bax_store::{CACHE_FILE_NAME, STORE_FILE_NAME};

/// 引擎配置。
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// 游戏日志目录。`None` 时先看用户保存的设置，再自动定位，找不到就每隔一段时间重试。
    pub log_dir: Option<PathBuf>,
    /// 长期库文件。
    pub store_path: PathBuf,
    pub api: ClientConfig,
    pub tracker: TrackerConfig,
    /// 把日志里的本地时间换算成 UTC 时使用的时区。
    pub log_zone: LogZone,
    /// 多久读一次日志。
    pub log_poll_interval: Duration,
    /// 找不到日志目录时，多久重新找一次。
    pub log_discovery_interval: Duration,
    /// 多久检查一次由时间触发的变化：准备阶段结束、失败后的重试、限流暂停结束。
    pub tick_interval: Duration,
    /// 同时进行的玩家资料请求数。原型实测 5 路并发查完 10 人约 6.4 秒，3 路偶尔被单个慢请求拖到 25 秒。
    pub fetch_concurrency: usize,
    /// 暂时性失败后，各次重试前的等待时间；长度就是最多重试的次数。
    pub retry_delays: Vec<Duration>,
    /// 被限流而站点没有说明要等多久时，暂停全部请求的时间。
    pub rate_limit_pause: Duration,
}

impl EngineConfig {
    /// 以数据目录为基础的默认配置。
    pub fn new(data_dir: &Path) -> Self {
        EngineConfig {
            log_dir: None,
            store_path: store_path(data_dir),
            api: ClientConfig::default(),
            tracker: TrackerConfig::default(),
            log_zone: LogZone::System,
            log_poll_interval: Duration::from_millis(500),
            log_discovery_interval: Duration::from_secs(5),
            tick_interval: Duration::from_secs(1),
            fetch_concurrency: 5,
            retry_delays: vec![Duration::from_secs(12), Duration::from_secs(24)],
            rate_limit_pause: Duration::from_secs(30),
        }
    }
}

/// 默认的数据目录：环境变量 `BAX_DATA_DIR`，否则是 `%LOCALAPPDATA%\BAX`。
///
/// 桌面应用和命令行工具用同一个目录，看到的是同一份数据。
pub fn default_data_dir() -> Option<PathBuf> {
    let non_empty = |name: &str| env::var_os(name).filter(|v| !v.is_empty());
    if let Some(dir) = non_empty("BAX_DATA_DIR") {
        return Some(PathBuf::from(dir));
    }
    let base = non_empty("LOCALAPPDATA").map(PathBuf::from).or_else(|| {
        non_empty("HOME").map(|home| PathBuf::from(home).join(".local").join("share"))
    })?;
    Some(base.join("BAX"))
}

/// 数据目录下的长期库文件。
pub fn store_path(data_dir: &Path) -> PathBuf {
    data_dir.join(STORE_FILE_NAME)
}

/// 数据目录下的缓存库文件，单独放在一个子目录里，整个删掉也不影响长期数据。
pub fn cache_path(data_dir: &Path) -> PathBuf {
    data_dir.join("cache").join(CACHE_FILE_NAME)
}
