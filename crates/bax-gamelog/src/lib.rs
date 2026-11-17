//! 《断箭》游戏日志的发现、增量读取与解析。
//!
//! 游戏把日志写在安装目录的 `GameLogs` 下，每次启动新建一个 `Gamelog__年_月_日__时_分.log`。
//! 文件是 UTF-8、LF 换行，由一条条记录组成：
//!
//! ```text
//! [2026-11-16 18:52:05.319]
//! Log: [NET_ROOM] WaitingRoom entered, fight id: 9000001
//!
//! ```
//!
//! 第一行是本地时间，第二行是级别与消息，消息可以带后续行（名单就是这样），最后是一个空行。
//!
//! 这个库分三层：[`discovery`] 找到日志目录，[`tail`] 把不断增长的文件切成完整的行，
//! [`parse`] 把行解析成 [`bax_core::LogEvent`]。[`watch`] 把三者接起来。
//! 事件对一场比赛意味着什么不在这里处理，那是 `bax_core::lifecycle` 的事。

pub mod discovery;
pub mod parse;
pub mod tail;
pub mod time;
pub mod watch;

pub use parse::{LogParser, TimedEvent};
pub use tail::{LogTailer, TailItem};
pub use time::{LogTime, LogZone};
pub use watch::{LogWatcher, Origin, WatchItem};

/// 游戏日志文件名的前缀与扩展名。
pub const LOG_FILE_PREFIX: &str = "Gamelog__";
pub const LOG_FILE_EXTENSION: &str = "log";

/// 判断文件名是不是游戏日志。
pub fn is_log_file_name(name: &str) -> bool {
    name.strip_prefix(LOG_FILE_PREFIX)
        .and_then(|rest| rest.strip_suffix(".log"))
        .is_some_and(|stem| !stem.is_empty())
}
