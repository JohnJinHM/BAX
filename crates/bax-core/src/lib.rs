//! BAX 的领域模型与纯逻辑。
//!
//! 这个库不做任何 IO，也不读系统时钟：所有时间都由调用方以 [`Timestamp`] 传入。
//! 同一串输入因此总是得到同一个结果，日志回放测试依赖这一点。

pub mod event;
pub mod ids;
pub mod lifecycle;
pub mod reminder;
pub mod roster;
pub mod time;

pub use event::{ExitCause, LogEvent, RoomKind, RoomStatus};
pub use ids::{MatchId, PlayerId};
pub use lifecycle::{
    BattleClock, ClockSource, Connection, EarlyPlayer, MatchState, Phase, Timeline, Tracker,
    TrackerConfig, Update,
};
pub use reminder::{ReminderOutcome, ReminderRule};
pub use roster::{RosterEntry, Team};
pub use time::Timestamp;
