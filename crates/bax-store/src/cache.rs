//! 缓存库：可以随时重新获取的接口响应。
//!
//! 这里的任何内容都可以丢。整个文件被删掉之后应用应当照常启动。

use std::path::Path;

use bax_core::Timestamp;
use rusqlite::{Connection, OptionalExtension, params};

use crate::Result;
use crate::db;

const MIGRATIONS: &[&str] = &["CREATE TABLE api_cache (
        key        TEXT PRIMARY KEY,
        fetched_at INTEGER NOT NULL,
        body       TEXT NOT NULL
    ) STRICT, WITHOUT ROWID;
    CREATE INDEX api_cache_by_time ON api_cache(fetched_at);"];

/// 缓存里的一条响应。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedEntry {
    pub body: String,
    pub fetched_at: Timestamp,
}

impl CachedEntry {
    /// 这条缓存在 `now` 时已经存在了多少毫秒。
    pub fn age_ms(&self, now: Timestamp) -> i64 {
        now.millis_since(self.fetched_at)
    }
}

/// 缓存库。
#[derive(Debug)]
pub struct CacheStore {
    conn: Connection,
}

impl CacheStore {
    /// 打开缓存库。文件损坏到打不开时删掉重建：缓存不值得为它让应用启动失败。
    pub fn open(path: &Path) -> Result<CacheStore> {
        match db::open(Some(path), MIGRATIONS, false) {
            Ok(conn) => Ok(CacheStore { conn }),
            Err(first) => {
                tracing::warn!(error = %first, "缓存库打不开，删除后重建");
                for suffix in ["", "-wal", "-shm"] {
                    let mut name = path.as_os_str().to_os_string();
                    name.push(suffix);
                    let _ = std::fs::remove_file(name);
                }
                Ok(CacheStore {
                    conn: db::open(Some(path), MIGRATIONS, false)?,
                })
            }
        }
    }

    pub fn open_in_memory() -> Result<CacheStore> {
        Ok(CacheStore {
            conn: db::open(None, MIGRATIONS, false)?,
        })
    }

    /// 读取一条缓存，不论新旧。请求失败时用它兜底。
    pub fn get(&self, key: &str) -> Result<Option<CachedEntry>> {
        Ok(self
            .conn
            .query_row(
                "SELECT body, fetched_at FROM api_cache WHERE key = ?1",
                [key],
                |row| {
                    Ok(CachedEntry {
                        body: row.get(0)?,
                        fetched_at: Timestamp::from_millis(row.get(1)?),
                    })
                },
            )
            .optional()?)
    }

    /// 读取一条不超过 `max_age_ms` 的缓存。
    pub fn get_fresh(
        &self,
        key: &str,
        now: Timestamp,
        max_age_ms: i64,
    ) -> Result<Option<CachedEntry>> {
        Ok(self
            .get(key)?
            .filter(|entry| entry.age_ms(now) <= max_age_ms))
    }

    pub fn put(&self, key: &str, body: &str, now: Timestamp) -> Result<()> {
        self.conn.execute(
            "INSERT INTO api_cache(key, fetched_at, body) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET fetched_at = excluded.fetched_at, body = excluded.body",
            params![key, now.as_millis(), body],
        )?;
        Ok(())
    }

    /// 删除早于 `cutoff` 的缓存，返回删掉的条数。
    pub fn purge_older_than(&self, cutoff: Timestamp) -> Result<usize> {
        Ok(self.conn.execute(
            "DELETE FROM api_cache WHERE fetched_at < ?1",
            [cutoff.as_millis()],
        )?)
    }

    /// 清空缓存，返回删掉的条数。
    pub fn clear(&self) -> Result<usize> {
        Ok(self.conn.execute("DELETE FROM api_cache", [])?)
    }

    pub fn len(&self) -> Result<u64> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM api_cache", [], |row| row.get(0))?;
        Ok(count.max(0) as u64)
    }

    pub fn is_empty(&self) -> Result<bool> {
        Ok(self.len()? == 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(ms: i64) -> Timestamp {
        Timestamp::from_millis(ms)
    }

    #[test]
    fn stores_and_overwrites_entries() {
        let cache = CacheStore::open_in_memory().expect("打开缓存库");
        assert_eq!(cache.get("matches:1:0").expect("读取"), None);

        cache
            .put("matches:1:0", "{\"a\":1}", t(1000))
            .expect("写入");
        cache
            .put("matches:1:0", "{\"a\":2}", t(2000))
            .expect("覆盖");
        let entry = cache.get("matches:1:0").expect("读取").expect("有内容");
        assert_eq!(entry.body, "{\"a\":2}");
        assert_eq!(entry.fetched_at, t(2000));
        assert_eq!(cache.len().expect("计数"), 1);
    }

    #[test]
    fn freshness_is_judged_against_the_given_time() {
        let cache = CacheStore::open_in_memory().expect("打开缓存库");
        cache.put("k", "v", t(10_000)).expect("写入");
        assert!(
            cache
                .get_fresh("k", t(15_000), 5_000)
                .expect("读取")
                .is_some()
        );
        assert!(
            cache
                .get_fresh("k", t(15_001), 5_000)
                .expect("读取")
                .is_none()
        );
        assert!(
            cache.get("k").expect("读取").is_some(),
            "过期的缓存仍可用来兜底"
        );
    }

    #[test]
    fn purge_removes_only_old_entries() {
        let cache = CacheStore::open_in_memory().expect("打开缓存库");
        cache.put("old", "1", t(100)).expect("写入");
        cache.put("edge", "2", t(500)).expect("写入");
        cache.put("new", "3", t(900)).expect("写入");
        assert_eq!(cache.purge_older_than(t(500)).expect("清理"), 1);
        assert!(cache.get("old").expect("读取").is_none());
        assert!(cache.get("edge").expect("读取").is_some());
        assert_eq!(cache.clear().expect("清空"), 2);
        assert!(cache.is_empty().expect("计数"));
    }

    #[test]
    fn a_corrupt_cache_file_is_rebuilt() {
        let dir = tempfile::tempdir().expect("临时目录");
        let path = dir.path().join("cache.db");
        std::fs::write(
            &path,
            b"this is not a sqlite database at all, just garbage bytes",
        )
        .expect("写入垃圾数据");

        let cache = CacheStore::open(&path).expect("损坏的缓存库应当被重建");
        cache.put("k", "v", t(1)).expect("写入");
        drop(cache);

        let reopened = CacheStore::open(&path).expect("再次打开");
        assert_eq!(
            reopened.get("k").expect("读取").map(|e| e.body),
            Some("v".to_owned())
        );
    }

    #[test]
    fn content_survives_reopening() {
        let dir = tempfile::tempdir().expect("临时目录");
        let path = dir.path().join("nested").join("cache.db");
        {
            let cache = CacheStore::open(&path).expect("打开并建目录");
            cache.put("units", "[]", t(42)).expect("写入");
        }
        let cache = CacheStore::open(&path).expect("再次打开");
        assert_eq!(
            cache.get("units").expect("读取").map(|e| e.fetched_at),
            Some(t(42))
        );
    }
}
