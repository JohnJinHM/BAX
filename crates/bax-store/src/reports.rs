//! 战后战报：原文归档、待获取队列、参考数据（单位目录），以及比赛记录列表。
//!
//! 战报原文按接口返回的样子整份保存，口径变了可以重新解析。待获取队列跨重启保留：
//! 应用在战报出来之前退出，下次启动接着等。

use std::collections::{BTreeMap, BTreeSet};

use bax_core::{MatchId, Phase, PlayerId, Team, Timestamp};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

use crate::Result;
use crate::store::{Store, id_from_db, id_to_db, phase_from_db, team_from_db, team_to_db};

/// 归档的一份原文。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredRaw {
    pub raw: String,
    pub fetched_at: Timestamp,
}

/// 同一数据库快照中的战报原文和出兵单位。
#[derive(Debug)]
pub struct IndexedReport {
    pub report: StoredRaw,
    pub units: BTreeMap<PlayerId, BTreeSet<i64>>,
}

/// 从战报里摘出来、列表要用的几项。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReportMeta {
    pub start_time: Option<Timestamp>,
    pub duration_secs: Option<i64>,
    pub map_id: Option<i64>,
    pub winner: Option<Team>,
}

/// 一份战报是什么时候取到的。用于验证 3：战报在 `last_miss_at` 之后、`fetched_at` 之前出现。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReportTiming {
    pub fetched_at: Timestamp,
    /// 开始等待的时刻（通常是比赛结束）；没经过队列直接取到的为 `None`。
    pub queued_at: Option<Timestamp>,
    /// 取到之前失败了几次。
    pub attempts: u32,
    /// 最后一次没取到的时刻。
    pub last_miss_at: Option<Timestamp>,
}

/// 待获取队列里的一场比赛。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedReport {
    pub match_id: MatchId,
    /// 开始等待的时刻：比赛结束，或者手动请求的时刻。重试间隔从这里算起。
    pub queued_at: Timestamp,
    pub attempts: u32,
    /// 下一次尝试的时刻；`None` 表示已经放弃。
    pub next_at: Option<Timestamp>,
    pub last_error: Option<String>,
    /// 从计时起点到比赛结束有多久，毫秒；两者缺一时为 `None`。很短说明比赛没有真正打起来。
    pub battle_ms: Option<i64>,
}

/// 一场比赛的战报状态。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "state",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum ReportState {
    /// 有结束信号且在准备阶段附近取消，不请求战报。
    Cancelled,
    /// 没有战报，也不在等。
    None,
    /// 在等，`next_at` 再试。
    Pending {
        attempts: u32,
        next_at: Timestamp,
    },
    /// 试了足够多次仍然没有，不再自动尝试。
    GaveUp {
        attempts: u32,
        last_error: Option<String>,
    },
    Ready {
        fetched_at: Timestamp,
    },
}

/// 比赛记录列表里的一行。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct HistoryRow {
    pub match_id: MatchId,
    /// 发现这场比赛的时刻。
    pub at: Timestamp,
    pub map: Option<String>,
    pub phase: Phase,
    pub self_team: Option<Team>,
    /// 本方是否获胜；没有战报或者不知道本方队伍时为 `None`。
    pub won: Option<bool>,
    /// 名单人数。
    pub players: u32,
    /// 从开局到结束的毫秒数；有战报时以战报为准。
    pub duration_ms: Option<i64>,
    pub report: ReportState,
}

const INDEX_UNITS: &str = include_str!("reports/index-units.sql");

fn indexed_units(row: &rusqlite::Row<'_>) -> Result<BTreeSet<i64>> {
    serde_json::from_str(&row.get::<_, String>(1)?)
        .map_err(|e| crate::StoreError::Corrupt(e.to_string()))
}

pub(crate) fn backfill_player_units(conn: &Connection) -> Result<()> {
    let mut reports = conn.prepare("SELECT match_id, start_time, raw FROM match_reports")?;
    let mut rows = reports.query([])?;
    let mut index = conn.prepare(INDEX_UNITS)?;
    while let Some(row) = rows.next()? {
        let id: i64 = row.get(0)?;
        let start: Option<i64> = row.get(1)?;
        let raw: String = row.get(2)?;
        index.execute(params![id, start, raw])?;
    }
    Ok(())
}

impl Store {
    /// 玩家最近已有的完整战报，不依赖同场日志是否存在。
    pub fn recent_player_reports(
        &self,
        id: bax_core::PlayerId,
        limit: u32,
    ) -> Result<Vec<(MatchId, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT i.match_id, r.raw FROM report_player_units i JOIN match_reports r USING(match_id)
             WHERE i.player_id = ?1 ORDER BY i.start_time DESC, i.match_id DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![id_to_db(id.get()), limit], |r| {
            Ok((MatchId(id_from_db(r.get(0)?)), r.get(1)?))
        })?;
        rows.map(|r| r.map_err(Into::into)).collect()
    }

    /// 最近各场的原出兵单位类型，包含没有可识别单位的场次。
    pub fn recent_player_units(
        &self,
        id: PlayerId,
        limit: u32,
    ) -> Result<Vec<(MatchId, BTreeSet<i64>)>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT match_id, unit_ids FROM report_player_units WHERE player_id = ?1
             ORDER BY start_time DESC, match_id DESC LIMIT ?2",
        )?;
        let mut rows = stmt.query(params![id_to_db(id.get()), limit])?;
        let mut reports = Vec::new();
        while let Some(row) = rows.next()? {
            reports.push((MatchId(id_from_db(row.get(0)?)), indexed_units(row)?));
        }
        Ok(reports)
    }

    pub fn indexed_report(&self, id: MatchId) -> Result<Option<IndexedReport>> {
        let tx = self.conn.unchecked_transaction()?;
        let Some(report) = self.report(id)? else {
            return Ok(None);
        };
        let mut stmt = self.conn.prepare_cached(
            "SELECT player_id, unit_ids FROM report_player_units WHERE match_id = ?1",
        )?;
        let mut rows = stmt.query([id_to_db(id.get())])?;
        let mut players = BTreeMap::new();
        while let Some(row) = rows.next()? {
            players.insert(PlayerId(id_from_db(row.get(0)?)), indexed_units(row)?);
        }
        drop(rows);
        tx.commit()?;
        Ok(Some(IndexedReport {
            report,
            units: players,
        }))
    }
    pub fn report_cancelled(&self, id: MatchId) -> Result<bool> {
        Ok(self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM cancelled_matches WHERE id = ?1)",
            [id_to_db(id.get())],
            |row| row.get(0),
        )?)
    }

    /// 保存一份战报原文，并把这场比赛移出待获取队列。
    pub fn save_report(
        &mut self,
        id: MatchId,
        raw: &str,
        meta: ReportMeta,
        at: Timestamp,
    ) -> Result<()> {
        self.save_report_with_players(id, raw, meta, at).map(|_| ())
    }

    /// 返回本次写入影响的玩家，包括替换旧战报时移除的成员。
    pub fn save_report_with_players(
        &mut self,
        id: MatchId,
        raw: &str,
        meta: ReportMeta,
        at: Timestamp,
    ) -> Result<BTreeSet<PlayerId>> {
        let tx = self.conn.transaction()?;
        // 队列里记着等了多久、失败过几次，一并留下（验证 3 要用）
        let queued: Option<(i64, u32, i64)> = tx
            .query_row(
                "SELECT queued_at, attempts, updated_at FROM report_queue WHERE match_id = ?1",
                [id_to_db(id.get())],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        let (queued_at, attempts, last_miss_at) = match queued {
            Some((queued_at, attempts, updated_at)) => (
                Some(queued_at),
                attempts,
                (attempts > 0).then_some(updated_at),
            ),
            None => (None, 0, None),
        };
        tx.execute(
            "INSERT INTO match_reports(match_id, raw, fetched_at, start_time, duration_secs, map_id,
                                       winner, queued_at, attempts, last_miss_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(match_id) DO UPDATE SET
                 raw = excluded.raw, fetched_at = excluded.fetched_at,
                 start_time = excluded.start_time, duration_secs = excluded.duration_secs,
                 map_id = excluded.map_id, winner = excluded.winner,
                 queued_at = COALESCE(excluded.queued_at, match_reports.queued_at),
                 attempts = CASE WHEN excluded.queued_at IS NULL THEN match_reports.attempts
                                 ELSE excluded.attempts END,
                 last_miss_at = CASE WHEN excluded.queued_at IS NULL THEN match_reports.last_miss_at
                                     ELSE excluded.last_miss_at END",
            params![
                id_to_db(id.get()),
                raw,
                at.as_millis(),
                meta.start_time.map(Timestamp::as_millis),
                meta.duration_secs,
                meta.map_id,
                meta.winner.map(team_to_db),
                queued_at,
                attempts,
                last_miss_at,
            ],
        )?;
        tx.execute(
            "DELETE FROM report_queue WHERE match_id = ?1",
            [id_to_db(id.get())],
        )?;
        let mut players = BTreeSet::new();
        {
            let mut remove = tx.prepare_cached(
                "DELETE FROM report_player_units WHERE match_id = ?1 RETURNING player_id",
            )?;
            let mut removed = remove.query([id_to_db(id.get())])?;
            while let Some(row) = removed.next()? {
                players.insert(PlayerId(id_from_db(row.get(0)?)));
            }
            let mut index = tx.prepare_cached(concat!(
                include_str!("reports/index-units.sql"),
                " RETURNING player_id"
            ))?;
            let mut added = index.query(params![
                id_to_db(id.get()),
                meta.start_time.map(Timestamp::as_millis),
                raw
            ])?;
            while let Some(row) = added.next()? {
                players.insert(PlayerId(id_from_db(row.get(0)?)));
            }
        }
        tx.commit()?;
        Ok(players)
    }

    /// 一场比赛的战报原文。
    pub fn report(&self, id: MatchId) -> Result<Option<StoredRaw>> {
        Ok(self
            .conn
            .query_row(
                "SELECT raw, fetched_at FROM match_reports WHERE match_id = ?1",
                [id_to_db(id.get())],
                |row| {
                    Ok(StoredRaw {
                        raw: row.get(0)?,
                        fetched_at: Timestamp::from_millis(row.get(1)?),
                    })
                },
            )
            .optional()?)
    }

    /// 一份战报是什么时候取到的。
    pub fn report_timing(&self, id: MatchId) -> Result<Option<ReportTiming>> {
        Ok(self
            .conn
            .query_row(
                "SELECT fetched_at, queued_at, attempts, last_miss_at FROM match_reports
                 WHERE match_id = ?1",
                [id_to_db(id.get())],
                |row| {
                    Ok(ReportTiming {
                        fetched_at: Timestamp::from_millis(row.get(0)?),
                        queued_at: row.get::<_, Option<i64>>(1)?.map(Timestamp::from_millis),
                        attempts: row.get::<_, Option<u32>>(2)?.unwrap_or(0),
                        last_miss_at: row.get::<_, Option<i64>>(3)?.map(Timestamp::from_millis),
                    })
                },
            )
            .optional()?)
    }

    /// 归档的战报数。
    pub fn report_count(&self) -> Result<u32> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM match_reports", [], |row| row.get(0))?)
    }

    pub fn has_report(&self, id: MatchId) -> Result<bool> {
        Ok(self
            .conn
            .query_row(
                "SELECT 1 FROM match_reports WHERE match_id = ?1",
                [id_to_db(id.get())],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
    }

    /// 比赛结束后排队等战报。已经有战报或已经在队列里时不做任何事，返回假。
    pub fn queue_report(
        &mut self,
        id: MatchId,
        queued_at: Timestamp,
        next_at: Timestamp,
    ) -> Result<bool> {
        if self.has_report(id)? || self.report_cancelled(id)? {
            return Ok(false);
        }
        let inserted = self.conn.execute(
            "INSERT OR IGNORE INTO report_queue(match_id, queued_at, attempts, next_at, updated_at)
             VALUES (?1, ?2, 0, ?3, ?2)",
            params![
                id_to_db(id.get()),
                queued_at.as_millis(),
                next_at.as_millis()
            ],
        )?;
        Ok(inserted > 0)
    }

    /// 手动请求：立刻再试，重试间隔从现在重新算。已经有战报时返回假。
    pub fn request_report(&mut self, id: MatchId, now: Timestamp) -> Result<bool> {
        if self.has_report(id)? || self.report_cancelled(id)? {
            return Ok(false);
        }
        self.conn.execute(
            "INSERT INTO report_queue(match_id, queued_at, attempts, next_at, updated_at)
             VALUES (?1, ?2, 0, ?2, ?2)
             ON CONFLICT(match_id) DO UPDATE SET
                 queued_at = excluded.queued_at, attempts = 0,
                 next_at = excluded.next_at, updated_at = excluded.updated_at",
            params![id_to_db(id.get()), now.as_millis()],
        )?;
        Ok(true)
    }

    /// 到时间该试的比赛，最早的在前。
    pub fn due_reports(&self, now: Timestamp, limit: u32) -> Result<Vec<QueuedReport>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT q.match_id, q.queued_at, q.attempts, q.next_at, q.last_error,
                    m.ended_at - m.start_at
             FROM report_queue q LEFT JOIN matches m ON m.id = q.match_id
             WHERE q.next_at IS NOT NULL AND q.next_at <= ?1
               AND q.match_id NOT IN (SELECT id FROM cancelled_matches)
             ORDER BY q.next_at, q.match_id LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![now.as_millis(), limit], queued_from_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// 队列里最早的下一次尝试。
    pub fn next_report_due(&self) -> Result<Option<Timestamp>> {
        Ok(self
            .conn
            .query_row(
                "SELECT MIN(next_at) FROM report_queue WHERE next_at IS NOT NULL
                 AND match_id NOT IN (SELECT id FROM cancelled_matches)",
                [],
                |row| row.get::<_, Option<i64>>(0),
            )?
            .map(Timestamp::from_millis))
    }

    /// 还在等的比赛数，不含已经放弃的。
    pub fn pending_reports(&self) -> Result<u32> {
        Ok(self.conn.query_row(
            "SELECT COUNT(*) FROM report_queue WHERE next_at IS NOT NULL
                 AND match_id NOT IN (SELECT id FROM cancelled_matches)",
            [],
            |row| row.get(0),
        )?)
    }

    pub fn queued_report(&self, id: MatchId) -> Result<Option<QueuedReport>> {
        Ok(self
            .conn
            .query_row(
                "SELECT q.match_id, q.queued_at, q.attempts, q.next_at, q.last_error,
                        m.ended_at - m.start_at
                 FROM report_queue q LEFT JOIN matches m ON m.id = q.match_id
                 WHERE q.match_id = ?1",
                [id_to_db(id.get())],
                queued_from_row,
            )
            .optional()?)
    }

    /// 记下一次没有拿到战报。`next_at` 为 `None` 表示放弃。
    pub fn report_attempt_failed(
        &mut self,
        id: MatchId,
        attempts: u32,
        next_at: Option<Timestamp>,
        error: &str,
        now: Timestamp,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE report_queue SET attempts = ?2, next_at = ?3, last_error = ?4, updated_at = ?5
             WHERE match_id = ?1",
            params![
                id_to_db(id.get()),
                attempts,
                next_at.map(Timestamp::as_millis),
                error,
                now.as_millis()
            ],
        )?;
        Ok(())
    }

    /// 打起来了、已经结束、却既没有战报也不在队列里的比赛，结束得晚的在前：
    /// 比赛、结束时刻、从计时起点到结束的毫秒数。
    /// 用于启动时补上应用没开着时结束的比赛。
    pub fn unreported_matches(
        &self,
        ended_since: Timestamp,
        limit: u32,
    ) -> Result<Vec<(MatchId, Timestamp, i64)>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT m.id, m.ended_at, m.ended_at - m.start_at FROM matches m
             WHERE m.ended_at IS NOT NULL AND m.ended_at >= ?1 AND m.start_at IS NOT NULL
               AND m.id NOT IN (SELECT id FROM cancelled_matches)
               AND NOT EXISTS (SELECT 1 FROM match_reports r WHERE r.match_id = m.id)
               AND NOT EXISTS (SELECT 1 FROM report_queue q WHERE q.match_id = m.id)
             ORDER BY m.ended_at DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![ended_since.as_millis(), limit], |row| {
            Ok((
                MatchId(id_from_db(row.get(0)?)),
                Timestamp::from_millis(row.get(1)?),
                row.get(2)?,
            ))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// 参考数据（例如单位目录）的原文。
    pub fn reference(&self, key: &str) -> Result<Option<StoredRaw>> {
        Ok(self
            .conn
            .query_row(
                "SELECT raw, fetched_at FROM reference_data WHERE key = ?1",
                [key],
                |row| {
                    Ok(StoredRaw {
                        raw: row.get(0)?,
                        fetched_at: Timestamp::from_millis(row.get(1)?),
                    })
                },
            )
            .optional()?)
    }

    pub fn set_reference(&mut self, key: &str, raw: &str, at: Timestamp) -> Result<()> {
        self.conn.execute(
            "INSERT INTO reference_data(key, raw, fetched_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET raw = excluded.raw, fetched_at = excluded.fetched_at",
            params![key, raw, at.as_millis()],
        )?;
        Ok(())
    }

    /// 比赛记录，新的在前。
    pub fn match_history(&self, limit: u32, offset: u32) -> Result<Vec<HistoryRow>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT m.id, m.discovered_at, m.map, m.phase, m.self_team, m.start_at, m.ended_at,
                    (SELECT COUNT(*) FROM match_players mp WHERE mp.match_id = m.id),
                    r.fetched_at, r.duration_secs, r.winner,
                    q.attempts, q.next_at, q.last_error,
                    m.id IN (SELECT id FROM cancelled_matches)
             FROM matches m
             LEFT JOIN match_reports r ON r.match_id = m.id
             LEFT JOIN report_queue q ON q.match_id = m.id
             ORDER BY m.discovered_at DESC, m.id DESC
             LIMIT ?1 OFFSET ?2",
        )?;
        let rows = stmt.query_map(params![limit, offset], |row| {
            Ok(RawHistory {
                id: row.get(0)?,
                at: row.get(1)?,
                map: row.get(2)?,
                phase: row.get(3)?,
                self_team: row.get(4)?,
                start_at: row.get(5)?,
                ended_at: row.get(6)?,
                players: row.get(7)?,
                fetched_at: row.get(8)?,
                report_secs: row.get(9)?,
                winner: row.get(10)?,
                attempts: row.get(11)?,
                next_at: row.get(12)?,
                last_error: row.get(13)?,
                cancelled: row.get(14)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?.decode()?);
        }
        Ok(out)
    }
}

fn queued_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<QueuedReport> {
    Ok(QueuedReport {
        match_id: MatchId(id_from_db(row.get(0)?)),
        queued_at: Timestamp::from_millis(row.get(1)?),
        attempts: row.get(2)?,
        next_at: row.get::<_, Option<i64>>(3)?.map(Timestamp::from_millis),
        last_error: row.get(4)?,
        battle_ms: row.get(5)?,
    })
}

struct RawHistory {
    cancelled: bool,
    id: i64,
    at: i64,
    map: Option<String>,
    phase: String,
    self_team: Option<String>,
    start_at: Option<i64>,
    ended_at: Option<i64>,
    players: u32,
    fetched_at: Option<i64>,
    report_secs: Option<i64>,
    winner: Option<String>,
    attempts: Option<u32>,
    next_at: Option<i64>,
    last_error: Option<String>,
}

impl RawHistory {
    fn decode(self) -> Result<HistoryRow> {
        let self_team = self.self_team.as_deref().map(team_from_db).transpose()?;
        let winner = self.winner.as_deref().map(team_from_db).transpose()?;
        let won = match (self_team, winner) {
            (Some(me), Some(w)) if me.is_playing() => Some(me == w),
            _ => None,
        };
        let report = if self.cancelled {
            ReportState::Cancelled
        } else {
            match (self.fetched_at, self.attempts) {
                (Some(at), _) => ReportState::Ready {
                    fetched_at: Timestamp::from_millis(at),
                },
                (None, Some(attempts)) => match self.next_at {
                    Some(next) => ReportState::Pending {
                        attempts,
                        next_at: Timestamp::from_millis(next),
                    },
                    None => ReportState::GaveUp {
                        attempts,
                        last_error: self.last_error,
                    },
                },
                (None, None) => ReportState::None,
            }
        };
        let duration_ms =
            self.report_secs
                .map(|s| s * 1000)
                .or(match (self.start_at, self.ended_at) {
                    (Some(start), Some(end)) if end >= start => Some(end - start),
                    _ => None,
                });
        Ok(HistoryRow {
            match_id: MatchId(id_from_db(self.id)),
            at: Timestamp::from_millis(self.at),
            map: self.map,
            phase: phase_from_db(&self.phase)?,
            self_team,
            won,
            players: self.players,
            duration_ms,
            report,
        })
    }
}

#[cfg(test)]
mod tests;
