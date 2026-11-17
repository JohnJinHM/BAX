//! 存储层的错误。

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("数据库操作失败：{0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("无法访问 {path}：{source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// 数据库是更新版本的 BAX 创建的。为了不损坏数据，旧版本拒绝打开它。
    #[error("数据库的结构版本是 {found}，这个版本的 BAX 只认识到 {supported}")]
    SchemaTooNew { found: u32, supported: u32 },
    #[error("设置项 {key} 的内容无法解析：{source}")]
    Setting {
        key: String,
        #[source]
        source: serde_json::Error,
    },
    /// 数据库里存着这个版本不认识的取值，例如未知的队伍名。
    #[error("数据库里有无法识别的取值：{0}")]
    Corrupt(String),
}
