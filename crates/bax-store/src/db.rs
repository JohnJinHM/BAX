//! 打开数据库与结构迁移。
//!
//! 结构版本记在 SQLite 的 `user_version` 里。迁移脚本只增不改：
//! 已经发布过的脚本不能再编辑，结构要变就在末尾追加一条。

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{Connection, OpenFlags, TransactionBehavior};

use crate::Result;
use crate::error::StoreError;

/// 打开数据库文件并迁移到最新结构。`path` 为 `None` 时使用内存数据库。
///
/// `backup_before_migrating` 为真时，已有数据的库在迁移前先复制一份放在旁边。
pub(crate) fn open(
    path: Option<&Path>,
    migrations: &[&str],
    backup_before_migrating: bool,
) -> Result<Connection> {
    let mut conn = match path {
        Some(path) => {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|source| StoreError::Io {
                    path: parent.to_path_buf(),
                    source,
                })?;
            }
            Connection::open(path)?
        }
        None => Connection::open_in_memory()?,
    };
    conn.busy_timeout(Duration::from_secs(5))?;
    // WAL 让读写互不阻塞；NORMAL 在 WAL 下断电也不会损坏数据库，只可能丢最后一次提交
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;

    let supported = migrations.len() as u32;
    let found = checked_version(&conn, supported)?;
    if found < supported {
        // 多个进程同时打开旧库时，由拿到写锁的连接执行备份和升级。
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let found = checked_version(&tx, supported)?;
        if found == supported {
            tx.commit()?;
            return Ok(conn);
        }
        tracing::info!(from = found, to = supported, "数据库结构升级");
        if backup_before_migrating
            && found > 0
            && let Some(path) = path
        {
            let reader = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
            reader.backup(rusqlite::MAIN_DB, backup_path(path, found), None)?;
        }
        for (index, script) in migrations.iter().enumerate().skip(found as usize) {
            tx.execute_batch(script)?;
            tx.pragma_update(None, "user_version", (index + 1) as u32)?;
        }
        tx.commit()?;
    }
    Ok(conn)
}

pub(crate) fn schema_version(conn: &Connection) -> Result<u32> {
    Ok(conn.query_row("PRAGMA user_version", [], |row| row.get(0))?)
}

fn checked_version(conn: &Connection, supported: u32) -> Result<u32> {
    let found = schema_version(conn)?;
    if found > supported {
        return Err(StoreError::SchemaTooNew { found, supported });
    }
    Ok(found)
}

/// 迁移前的备份文件路径：`bax.db` 的第 1 版备份为 `bax.db.v1.bak`。
pub(crate) fn backup_path(path: &Path, version: u32) -> PathBuf {
    let mut name = path
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    name.push(format!(".v{version}.bak"));
    path.with_file_name(name)
}
