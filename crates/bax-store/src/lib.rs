//! BAX 的本地存储。
//!
//! 有两个互相独立的 SQLite 文件（见 ADR-0005）：
//!
//! - [`Store`] 是长期库，保存玩家档案、比赛、相遇记录、战报和设置。
//!   它没有任何自动过期或批量删除的操作。
//! - [`CacheStore`] 是缓存库，保存可以随时重新获取的接口响应，可以按时间清理，也可以整个删掉。
//!
//! 两者是不同的类型、不同的连接。清理缓存的代码拿不到长期库，误删长期数据的路径在结构上不存在。
//!
//! 这个库不发起网络请求，也不读系统时钟；时间由调用方传入。

mod archive;
mod cache;
mod db;
mod error;
mod reports;
mod store;

pub use archive::{EncounterRow, Note, PlayerHit, Relation};
pub use cache::{CacheStore, CachedEntry};
pub use error::StoreError;
pub use reports::{HistoryRow, QueuedReport, ReportMeta, ReportState, StoredRaw};
pub use store::{
    EncounterSummary, MatchPlayerRow, MatchRecord, PlayerRecord, ProfileUpdate, RatingPoint, Store,
};

pub type Result<T> = std::result::Result<T, StoreError>;

/// 长期库的文件名。
pub const STORE_FILE_NAME: &str = "bax.db";
/// 缓存库的文件名。
pub const CACHE_FILE_NAME: &str = "cache.db";
