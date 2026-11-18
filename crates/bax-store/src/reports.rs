//! 战后战报：原文归档、待获取队列、参考数据（单位目录），以及比赛记录列表。
//!
//! 战报原文按接口返回的样子整份保存，口径变了可以重新解析。待获取队列跨重启保留：
//! 应用在战报出来之前退出，下次启动接着等。

use bax_core::{MatchId, Phase, Team, Timestamp};
use rusqlite::{OptionalExtension, params};
use serde::Serialize;

use crate::Result;
use crate::store::{Store, id_from_db, id_to_db, phase_from_db, team_from_db, team_to_db};

/// 归档的一份原文。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredRaw {
    pub raw: String,
    pub fetched_at: Timestamp,
}

/// 从战报里摘出来、列表要用的几项。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReportMeta {
    pub start_time: Option<Timestamp>,
    pub duration_secs: Option<i64>,
    pub map_id: Option<i64>,
    pub winner: Option<Team>,
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

impl Store {
    /// 保存一份战报原文，并把这场比赛移出待获取队列。
    pub fn save_report(
        &mut self,
        id: MatchId,
        raw: &str,
        meta: ReportMeta,
        at: Timestamp,
    ) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO match_reports(match_id, raw, fetched_at, start_time, duration_secs, map_id, winner)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(match_id) DO UPDATE SET
                 raw = excluded.raw, fetched_at = excluded.fetched_at,
                 start_time = excluded.start_time, duration_secs = excluded.duration_secs,
                 map_id = excluded.map_id, winner = excluded.winner",
            params![
                id_to_db(id.get()),
                raw,
                at.as_millis(),
                meta.start_time.map(Timestamp::as_millis),
                meta.duration_secs,
                meta.map_id,
                meta.winner.map(team_to_db),
            ],
        )?;
        tx.execute(
            "DELETE FROM report_queue WHERE match_id = ?1",
            [id_to_db(id.get())],
        )?;
        tx.commit()?;
        Ok(())
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
        if self.has_report(id)? {
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
        if self.has_report(id)? {
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
            "SELECT match_id, queued_at, attempts, next_at, last_error FROM report_queue
             WHERE next_at IS NOT NULL AND next_at <= ?1
             ORDER BY next_at, match_id LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![now.as_millis(), limit], queued_from_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// 队列里最早的下一次尝试。
    pub fn next_report_due(&self) -> Result<Option<Timestamp>> {
        Ok(self
            .conn
            .query_row(
                "SELECT MIN(next_at) FROM report_queue WHERE next_at IS NOT NULL",
                [],
                |row| row.get::<_, Option<i64>>(0),
            )?
            .map(Timestamp::from_millis))
    }

    /// 还在等的比赛数，不含已经放弃的。
    pub fn pending_reports(&self) -> Result<u32> {
        Ok(self.conn.query_row(
            "SELECT COUNT(*) FROM report_queue WHERE next_at IS NOT NULL",
            [],
            |row| row.get(0),
        )?)
    }

    pub fn queued_report(&self, id: MatchId) -> Result<Option<QueuedReport>> {
        Ok(self
            .conn
            .query_row(
                "SELECT match_id, queued_at, attempts, next_at, last_error FROM report_queue
                 WHERE match_id = ?1",
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

    /// 打起来了、已经结束、却既没有战报也不在队列里的比赛，结束得晚的在前。
    /// 用于启动时补上应用没开着时结束的比赛。
    pub fn unreported_matches(
        &self,
        ended_since: Timestamp,
        limit: u32,
    ) -> Result<Vec<(MatchId, Timestamp)>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT m.id, m.ended_at FROM matches m
             WHERE m.ended_at IS NOT NULL AND m.ended_at >= ?1 AND m.start_at IS NOT NULL
               AND NOT EXISTS (SELECT 1 FROM match_reports r WHERE r.match_id = m.id)
               AND NOT EXISTS (SELECT 1 FROM report_queue q WHERE q.match_id = m.id)
             ORDER BY m.ended_at DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![ended_since.as_millis(), limit], |row| {
            Ok((
                MatchId(id_from_db(row.get(0)?)),
                Timestamp::from_millis(row.get(1)?),
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
                    q.attempts, q.next_at, q.last_error
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
    })
}

struct RawHistory {
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
        let report = match (self.fetched_at, self.attempts) {
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
