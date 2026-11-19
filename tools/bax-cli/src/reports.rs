//! `bax reports`：列出最近比赛的战报获取情况，给验证 3（数据更新延迟）用。
//!
//! 只读打开长期库，不做结构升级：桌面应用正开着、或者库还是旧版本时都可以运行。
//! 战报在“最后一次没取到”与“取到”之间的某个时刻被 BATrace 收录。

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OpenFlags};

struct Row {
    id: i64,
    map: Option<String>,
    ended_at: Option<i64>,
    fetched_at: Option<i64>,
    queued_at: Option<i64>,
    attempts: Option<i64>,
    last_miss_at: Option<i64>,
    pending_attempts: Option<i64>,
    next_at: Option<i64>,
    last_error: Option<String>,
}

pub fn list(data_dir: &Path, limit: u32) -> Result<()> {
    let path = bax_engine::store_path(data_dir);
    let conn = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("打不开 {}", path.display()))?;
    let version: u32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version < 4 {
        bail!("长期库是第 {version} 版结构，还没有战报归档；用 0.4 以后的 BAX 运行过一次才有");
    }
    let mut stmt = conn.prepare(
        "SELECT m.id, m.map, m.ended_at,
                r.fetched_at, r.queued_at, r.attempts, r.last_miss_at,
                q.attempts, q.next_at, q.last_error
         FROM matches m
         LEFT JOIN match_reports r ON r.match_id = m.id
         LEFT JOIN report_queue q ON q.match_id = m.id
         WHERE m.start_at IS NOT NULL
         ORDER BY m.discovered_at DESC LIMIT ?1",
    )?;
    let rows = stmt
        .query_map([limit], |row| {
            Ok(Row {
                id: row.get(0)?,
                map: row.get(1)?,
                ended_at: row.get(2)?,
                fetched_at: row.get(3)?,
                queued_at: row.get(4)?,
                attempts: row.get(5)?,
                last_miss_at: row.get(6)?,
                pending_attempts: row.get(7)?,
                next_at: row.get(8)?,
                last_error: row.get(9)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    println!("比赛ID\t结束于\t地图\t状态\t尝试\t收录时刻（结束后）");
    let mut windows = Vec::new();
    for r in &rows {
        let ended = r.ended_at.map_or("–".to_owned(), |at| ago(now - at));
        let map = r.map.as_deref().unwrap_or("–");
        let (state, attempts, window) = match (r.fetched_at, r.pending_attempts) {
            (Some(fetched), _) => {
                let base = r.queued_at.or(r.ended_at);
                let window = match base {
                    Some(base) => {
                        let hi = fetched - base;
                        let lo = r.last_miss_at.map(|m| m - base);
                        windows.push((lo, hi));
                        match lo {
                            Some(lo) => format!("{} ～ {}", secs(lo), secs(hi)),
                            None => format!("≤ {}", secs(hi)),
                        }
                    }
                    None => "–".to_owned(),
                };
                ("已归档", r.attempts.unwrap_or(0) + 1, window)
            }
            (None, Some(n)) => {
                let state = if r.next_at.is_some() {
                    "等待中"
                } else {
                    "已放弃"
                };
                let detail = match (r.next_at, &r.last_error) {
                    (Some(next), _) => format!("下次 {}", until(next - now)),
                    (None, Some(err)) => err.clone(),
                    (None, None) => "–".to_owned(),
                };
                (state, n, detail)
            }
            (None, None) => ("没有排队", 0, "–".to_owned()),
        };
        println!("{}\t{ended}\t{map}\t{state}\t{attempts}\t{window}", r.id);
    }

    if !windows.is_empty() {
        let mut upper: Vec<i64> = windows.iter().map(|(_, hi)| *hi).collect();
        upper.sort_unstable();
        let lower: Vec<i64> = windows.iter().filter_map(|(lo, _)| *lo).collect();
        println!();
        println!(
            "已归档 {} 场：取到的时刻在结束后 {}～{}，中位数 {}",
            upper.len(),
            secs(upper[0]),
            secs(upper[upper.len() - 1]),
            secs(upper[upper.len() / 2])
        );
        if let Some(max_miss) = lower.iter().max() {
            println!("最晚一次还没收录：结束后 {}", secs(*max_miss));
        }
        println!(
            "收录发生在“最后一次没取到”与“取到”之间；尝试时间点见 EngineConfig::report_schedule（没打起来的比赛用 report_schedule_unplayed）。"
        );
    }
    Ok(())
}

fn secs(ms: i64) -> String {
    let s = ms.max(0) / 1000;
    format!("{}:{:02}", s / 60, s % 60)
}

fn ago(ms: i64) -> String {
    let minutes = ms.max(0) / 60_000;
    match minutes {
        0 => "刚刚".to_owned(),
        m if m < 60 => format!("{m} 分钟前"),
        m if m < 48 * 60 => format!("{} 小时前", m / 60),
        m => format!("{} 天前", m / (24 * 60)),
    }
}

fn until(ms: i64) -> String {
    if ms <= 0 {
        "马上".to_owned()
    } else {
        format!("{} 后", secs(ms))
    }
}
