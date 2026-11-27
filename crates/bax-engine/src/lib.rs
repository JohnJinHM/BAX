//! BAX 的后台运行时。
//!
//! 引擎把四个库接起来：日志线程读取并解析游戏日志；对局会话按顺序处理日志事件、
//! 接口结果和界面命令，维护当前比赛的状态，写入长期库，并把状态快照发布出去。
//! 它不依赖 Tauri，桌面应用和命令行工具用的是同一套逻辑（见 ADR-0003）。
//!
//! ```no_run
//! # async fn demo() -> Result<(), Box<dyn std::error::Error>> {
//! use std::sync::Arc;
//! use bax_engine::{Engine, EngineConfig, SystemClock, default_data_dir};
//!
//! let data_dir = default_data_dir().expect("找不到数据目录");
//! let engine = Engine::start(EngineConfig::new(&data_dir), Arc::new(SystemClock))?;
//! let mut updates = engine.subscribe();
//! while updates.changed().await.is_ok() {
//!     let snapshot = updates.borrow_and_update().clone();
//!     println!("修订 {}", snapshot.revision);
//! }
//! # Ok(())
//! # }
//! ```

mod clock;
mod config;
pub mod diag;
mod engine;
pub mod history;
mod label_worker;
mod labels;
mod logthread;
mod match_mode;
mod player;
mod postmatch;
pub mod pregame;
mod profile;
mod reports;
mod session;
mod snapshot;
mod spec_worker;
mod specs;

pub use bax_core::report::{
    ClassShare, MatchAnalysis, PlayerReport, Sortie, TeamReport, UnitClass, UnitRow,
};
pub use bax_store::MatchMode;
pub use bax_store::{
    EncounterRow, HistoryRow, Note, PlayerHit, RatingPoint, Relation, ReportState,
};
pub use clock::{Clock, ManualClock, SystemClock};
pub use config::{EngineConfig, cache_path, default_data_dir, store_path};
pub use engine::{
    CommandError, Engine, EngineError, ImportSummary, PlayerFight, PlayerFightArchive,
};
pub use player::PlayerDetail;
pub use postmatch::MatchReportView;
pub use profile::{ProfileSummary, RankedStats};
pub use snapshot::{
    AlertView, ApiErrorView, ApiStatus, ArchiveStatus, FetchState, LogStatus, MatchView,
    PlayerView, ReminderView, Snapshot, TeamSummary,
};

mod variants;
