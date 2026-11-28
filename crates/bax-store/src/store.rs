//! 长期库：玩家档案、比赛、相遇记录和设置。
//!
//! 写入都是幂等的合并：同一场比赛、同一份资料写多少遍，结果都一样；
//! 用一份不完整的状态再写一遍，不会把已有的信息抹掉。
//! 重读日志、重连、应用重启都会让同样的数据再来一遍，存储层必须经得起。

use std::path::Path;

use bax_core::{
    BattleClock, ClockSource, ExitCause, MatchId, MatchState, Phase, PlayerId, ReminderOutcome,
    Team, Timestamp,
};
use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::Result;
use crate::db;
use crate::error::StoreError;

const MIGRATIONS: &[&str] = &[
    // 第 1 版：对局识别与玩家查询
    "CREATE TABLE players (
        id            INTEGER PRIMARY KEY,
        name          TEXT,
        steam_id      TEXT,
        level         INTEGER,
        rating        REAL,
        rank          INTEGER,
        rating_games  INTEGER,
        profile_json  TEXT,
        fetched_at    INTEGER,
        last_log_name TEXT,
        first_seen_at INTEGER,
        last_seen_at  INTEGER
    ) STRICT;

    CREATE TABLE player_ratings (
        player_id    INTEGER NOT NULL,
        at           INTEGER NOT NULL,
        rating       REAL,
        level        INTEGER,
        rank         INTEGER,
        rating_games INTEGER,
        PRIMARY KEY (player_id, at)
    ) STRICT, WITHOUT ROWID;

    CREATE TABLE matches (
        id                 INTEGER PRIMARY KEY,
        discovered_at      INTEGER NOT NULL,
        phase              TEXT NOT NULL,
        rejoined           INTEGER NOT NULL DEFAULT 0,
        map                TEXT,
        scenario           TEXT,
        deck               TEXT,
        game_version       TEXT,
        self_id            INTEGER,
        self_team          TEXT,
        start_at           INTEGER,
        start_source       TEXT,
        scenario_loaded_at INTEGER,
        ended_at           INTEGER,
        end_cause          TEXT
    ) STRICT;
    CREATE INDEX matches_by_time ON matches(discovered_at);

    CREATE TABLE match_players (
        match_id  INTEGER NOT NULL REFERENCES matches(id) ON DELETE CASCADE,
        player_id INTEGER NOT NULL,
        position  INTEGER NOT NULL,
        team      TEXT,
        log_name  TEXT,
        PRIMARY KEY (match_id, player_id)
    ) STRICT, WITHOUT ROWID;
    CREATE INDEX match_players_by_player ON match_players(player_id);

    CREATE TABLE settings (
        key   TEXT PRIMARY KEY,
        value TEXT NOT NULL
    ) STRICT, WITHOUT ROWID;",
    // 第 2 版：计时提醒在每场比赛里的结果
    "CREATE TABLE reminder_log (
        match_id INTEGER NOT NULL,
        rule_id  INTEGER NOT NULL,
        outcome  TEXT NOT NULL,
        at       INTEGER NOT NULL,
        PRIMARY KEY (match_id, rule_id)
    ) STRICT, WITHOUT ROWID;",
    // 第 3 版：玩家备注。备注是用户写的，与接口资料分开存放，接口更新永远不会碰它
    "CREATE TABLE notes (
        player_id  INTEGER PRIMARY KEY,
        body       TEXT NOT NULL,
        updated_at INTEGER NOT NULL
    ) STRICT;
    CREATE INDEX players_by_last_seen ON players(last_seen_at);",
    // 第 4 版：战报归档、待获取队列与参考数据（单位目录）
    "CREATE TABLE match_reports (
        match_id      INTEGER PRIMARY KEY,
        raw           TEXT NOT NULL,
        fetched_at    INTEGER NOT NULL,
        start_time    INTEGER,
        duration_secs INTEGER,
        map_id        INTEGER,
        winner        TEXT,
        queued_at     INTEGER,
        attempts      INTEGER,
        last_miss_at  INTEGER
    ) STRICT;

    CREATE TABLE report_queue (
        match_id   INTEGER PRIMARY KEY,
        queued_at  INTEGER NOT NULL,
        attempts   INTEGER NOT NULL DEFAULT 0,
        next_at    INTEGER,
        last_error TEXT,
        updated_at INTEGER NOT NULL
    ) STRICT;
    CREATE INDEX report_queue_by_next ON report_queue(next_at);

    CREATE TABLE reference_data (
        key        TEXT PRIMARY KEY,
        raw        TEXT NOT NULL,
        fetched_at INTEGER NOT NULL
    ) STRICT, WITHOUT ROWID;",
    // 第 5 版：动态识别准备阶段取消；旧记录和后续导入适用同一口径。
    "CREATE VIEW cancelled_matches AS
     SELECT m.id FROM matches m
     WHERE m.phase = 'ended' AND m.start_source = 'logEstimate'
       AND m.end_cause IN ('matchEnded', 'serverExitInstance')
       AND m.scenario_loaded_at IS NOT NULL
       AND m.ended_at >= m.scenario_loaded_at
       AND m.ended_at <= m.start_at + 2000
       AND NOT EXISTS (SELECT 1 FROM match_reports r WHERE r.match_id = m.id);",
    // 第 6 版：索引每场每人的原出兵单位类型；旧战报逐场投影。
    "CREATE TABLE report_player_units (
        match_id INTEGER NOT NULL REFERENCES match_reports(match_id) ON DELETE CASCADE,
        player_id INTEGER NOT NULL,
        start_time INTEGER,
        unit_ids TEXT NOT NULL,
        PRIMARY KEY (match_id, player_id)
    ) STRICT, WITHOUT ROWID;
    CREATE INDEX report_units_by_player ON report_player_units(player_id, start_time DESC, match_id DESC);",
    // 第 7 版：BATrace 标记能独立于基本资料更新，旧档案的次数为未知。
    "ALTER TABLE players ADD COLUMN trace_count INTEGER CHECK(trace_count >= 0);
     ALTER TABLE players ADD COLUMN trace_checked_at INTEGER;",
    // Version 8 indexes the participant perspective for archived player history.
    "ALTER TABLE report_player_units ADD COLUMN team_id INTEGER;
     ALTER TABLE report_player_units ADD COLUMN rating_delta REAL;",
    // 第 9 版：手动刷新过往对局的时间独立于资料更新和战报收录。
    "CREATE TABLE player_fight_queries (
        player_id INTEGER PRIMARY KEY,
        queried_at INTEGER NOT NULL
    ) STRICT;",
    "ALTER TABLE report_player_units ADD COLUMN deployment TEXT NOT NULL DEFAULT '[]';",
    // 归档列表和雷达图只读取小索引，保留战报当时的昵称与官方计数。
    "ALTER TABLE report_player_units ADD COLUMN report_name TEXT;
     ALTER TABLE report_player_units ADD COLUMN kills INTEGER NOT NULL DEFAULT 0;
     ALTER TABLE report_player_units ADD COLUMN losses INTEGER NOT NULL DEFAULT 0;
     ALTER TABLE report_player_units ADD COLUMN objectives INTEGER NOT NULL DEFAULT 0;
     ALTER TABLE report_player_units ADD COLUMN supplied_allies INTEGER NOT NULL DEFAULT 0;",
    // 玩家效率使用战报的官方累计出兵分，缺失时保留 NULL。
    "ALTER TABLE report_player_units ADD COLUMN spawned_score INTEGER;",
    // 原文的模式只解析一次，日志后到时通过视图即时修正。
    "ALTER TABLE matches ADD COLUMN has_ai INTEGER NOT NULL DEFAULT 0;
     CREATE TABLE report_modes (
         match_id INTEGER PRIMARY KEY REFERENCES match_reports(match_id) ON DELETE CASCADE,
         raw_mode TEXT NOT NULL
     ) STRICT;
     CREATE VIEW classified_reports AS
     SELECT r.match_id, CASE
         WHEN r.raw_mode = 'pve' OR m.has_ai OR lower(substr(m.scenario, -9)) = '_skirmish' THEN 'pve'
         WHEN r.raw_mode = 'custom' OR m.self_team = 'spectators' OR EXISTS (
             SELECT 1 FROM match_players p WHERE p.match_id = r.match_id AND p.team = 'spectators'
         ) THEN 'custom'
         ELSE r.raw_mode END AS mode
     FROM report_modes r LEFT JOIN matches m ON m.id = r.match_id;",
];

/// 从接口取得的一份玩家资料，准备写入长期库。
#[derive(Debug, Clone, PartialEq)]
pub struct ProfileUpdate {
    pub id: PlayerId,
    pub name: Option<String>,
    pub steam_id: Option<String>,
    pub level: Option<i64>,
    pub rating: Option<f64>,
    pub rank: Option<i64>,
    pub rating_games: Option<i64>,
    /// 接口响应的原文。
    pub raw_json: String,
}

/// 长期库里的一名玩家。
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerRecord {
    pub id: PlayerId,
    /// 接口资料里的名字。
    pub name: Option<String>,
    pub steam_id: Option<String>,
    pub level: Option<i64>,
    pub rating: Option<f64>,
    pub rank: Option<i64>,
    pub rating_games: Option<i64>,
    /// 按字段合并的接口资料；缺失字段保留上次有效值。
    pub profile_json: Option<String>,
    pub trace_count: Option<u32>,
    pub trace_checked_at: Option<Timestamp>,
    /// 最近一次成功取得资料的时刻；从未取到过时为 `None`。
    pub fetched_at: Option<Timestamp>,
    /// 最近一次在日志名单里见到的名字。
    pub last_log_name: Option<String>,
    pub first_seen_at: Option<Timestamp>,
    pub last_seen_at: Option<Timestamp>,
}

impl PlayerRecord {
    /// 用来显示的名字：接口资料优先，其次是日志里的名字。空串视为没有。
    pub fn display_name(&self) -> Option<&str> {
        [self.name.as_deref(), self.last_log_name.as_deref()]
            .into_iter()
            .flatten()
            .find(|n| !n.trim().is_empty())
    }
}

/// 玩家某一时刻的 ELO 等数据。只在数值变化时记一条。
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct RatingPoint {
    pub at: Timestamp,
    pub rating: Option<f64>,
    pub level: Option<i64>,
    pub rank: Option<i64>,
    pub rating_games: Option<i64>,
}

/// 长期库里的一场比赛。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchRecord {
    pub id: MatchId,
    pub discovered_at: Timestamp,
    pub phase: Phase,
    pub rejoined: bool,
    pub map: Option<String>,
    pub scenario: Option<String>,
    pub deck: Option<String>,
    pub game_version: Option<String>,
    pub self_id: Option<PlayerId>,
    pub self_team: Option<Team>,
    pub clock: Option<BattleClock>,
    pub scenario_loaded_at: Option<Timestamp>,
    pub ended_at: Option<Timestamp>,
    pub end_cause: Option<ExitCause>,
}

/// 比赛名单里的一行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchPlayerRow {
    pub player: PlayerId,
    /// 只在等待室见过、还没有完整名单时为 `None`。
    pub team: Option<Team>,
    /// 日志里的名字；空名字为 `None`。
    pub log_name: Option<String>,
}

/// 与某名玩家的相遇情况。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct EncounterSummary {
    /// 一共同场多少次。
    pub total: u32,
    /// 其中作为队友的次数。
    pub as_teammate: u32,
    /// 其中作为对手的次数。
    pub as_opponent: u32,
    pub first_at: Option<Timestamp>,
    pub last_at: Option<Timestamp>,
}

/// 长期库。
#[derive(Debug)]
pub struct Store {
    pub(crate) conn: Connection,
}

impl Store {
    /// 打开长期库，必要时建库或迁移结构。迁移已有数据之前会在旁边留一份备份。
    pub fn open(path: &Path) -> Result<Store> {
        Self::open_at(Some(path))
    }

    pub fn open_in_memory() -> Result<Store> {
        Self::open_at(None)
    }

    fn open_at(path: Option<&Path>) -> Result<Store> {
        let conn = db::open_with_data_migrations(path, MIGRATIONS, true, |conn, version| {
            // 在完整索引结构就绪后统一回填，包括已经使用第 11 版的库。
            if version == 12 {
                crate::reports::backfill_player_units(conn)?;
            }
            if version == 13 {
                conn.execute_batch(include_str!("reports/index-modes.sql"))?;
            }
            Ok(())
        })?;
        Ok(Store { conn })
    }

    pub fn schema_version(&self) -> Result<u32> {
        db::schema_version(&self.conn)
    }

    // ------------------------------------------------------------ 玩家

    /// 搜索摘要只补充身份，不覆盖完整资料或产生同场记录。
    pub fn save_player_identity(
        &self,
        id: PlayerId,
        name: &str,
        steam_id: Option<&str>,
        level: Option<i64>,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO players(id, name, steam_id, level) VALUES (?1, NULLIF(?2, ''), NULLIF(?3, ''), ?4)
             ON CONFLICT(id) DO UPDATE SET
                name = COALESCE(players.name, excluded.name),
                steam_id = COALESCE(players.steam_id, excluded.steam_id),
                level = COALESCE(players.level, excluded.level)",
            params![id_to_db(id.get()), name, steam_id, level],
        )?;
        Ok(())
    }

    pub fn player(&self, id: PlayerId) -> Result<Option<PlayerRecord>> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {PLAYER_COLUMNS} FROM players WHERE id = ?1"),
                [id_to_db(id.get())],
                player_from_row,
            )
            .optional()?)
    }

    /// 批量读取。库里没有的玩家不出现在结果里。
    pub fn players(&self, ids: &[PlayerId]) -> Result<Vec<PlayerRecord>> {
        let mut out = Vec::with_capacity(ids.len());
        let mut stmt = self.conn.prepare_cached(&format!(
            "SELECT {PLAYER_COLUMNS} FROM players WHERE id = ?1"
        ))?;
        for id in ids {
            if let Some(record) = stmt
                .query_row([id_to_db(id.get())], player_from_row)
                .optional()?
            {
                out.push(record);
            }
        }
        Ok(out)
    }

    /// 按请求开始时间排序，迟到的旧次数不能覆盖新次数。
    pub fn save_trace_count(&self, player: PlayerId, count: u32, at: Timestamp) -> Result<()> {
        self.conn.execute(
            "INSERT INTO players(id, trace_count, trace_checked_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET trace_count = excluded.trace_count,
                 trace_checked_at = excluded.trace_checked_at
             WHERE players.trace_checked_at IS NULL OR players.trace_checked_at <= excluded.trace_checked_at",
            params![id_to_db(player.get()), count, at.as_millis()],
        )?;
        Ok(())
    }

    /// 写入一份从接口取得的资料。只更新外部资料的列，不动相遇记录。
    ///
    /// ELO、等级、排名或排位场次与上一次记录不同时，追加一条历史。
    pub fn save_profile(&mut self, profile: &ProfileUpdate, at: Timestamp) -> Result<()> {
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let id = id_to_db(profile.id.get());
        let previous = tx
            .query_row(
                &format!("SELECT {PLAYER_COLUMNS} FROM players WHERE id = ?1"),
                [id],
                player_from_row,
            )
            .optional()?;
        if previous
            .as_ref()
            .and_then(|p| p.fetched_at)
            .is_some_and(|old| old > at)
        {
            return Ok(());
        }
        let mut profile = profile.clone();
        profile.name = profile.name.filter(|v| !v.trim().is_empty());
        profile.steam_id = profile.steam_id.filter(|v| !v.trim().is_empty());
        if let Some(old) = &previous {
            profile.name = profile.name.or_else(|| old.name.clone());
            profile.steam_id = profile.steam_id.or_else(|| old.steam_id.clone());
            profile.level = profile.level.or(old.level);
            profile.rating = profile.rating.or(old.rating);
            profile.rank = profile.rank.or(old.rank);
            profile.rating_games = profile.rating_games.or(old.rating_games);
        }
        let incoming: serde_json::Value = serde_json::from_str(&profile.raw_json)
            .map_err(|_| StoreError::Corrupt("玩家资料 JSON 无效".into()))?;
        if !incoming.is_object() {
            return Err(StoreError::Corrupt("玩家资料必须是 JSON 对象".into()));
        }
        let mut merged = previous
            .as_ref()
            .and_then(|p| p.profile_json.as_deref())
            .and_then(|raw| serde_json::from_str(raw).ok())
            .unwrap_or_else(|| serde_json::json!({}));
        merge_profile_fields(&mut merged, incoming);
        // 查询列和资料摘要保持一致，旧资料中缺失的基本字段也能补齐。
        merge_profile_fields(
            &mut merged,
            serde_json::json!({ "info": {
                "id": profile.id.get(), "name": profile.name, "steamId": profile.steam_id,
                "level": profile.level, "rating": profile.rating, "rank": profile.rank, "ratingGames": profile.rating_games,
            }}),
        );
        profile.raw_json = merged.to_string();
        tx.execute(
            "INSERT INTO players(id, name, steam_id, level, rating, rank, rating_games, profile_json, fetched_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(id) DO UPDATE SET
                 name = excluded.name,
                 steam_id = COALESCE(excluded.steam_id, steam_id),
                 level = excluded.level,
                 rating = excluded.rating,
                 rank = excluded.rank,
                 rating_games = excluded.rating_games,
                 profile_json = excluded.profile_json,
                 fetched_at = excluded.fetched_at
             WHERE excluded.fetched_at >= COALESCE(players.fetched_at, excluded.fetched_at)",
            params![
                id,
                profile.name,
                profile.steam_id,
                profile.level,
                profile.rating,
                profile.rank,
                profile.rating_games,
                profile.raw_json,
                at.as_millis(),
            ],
        )?;

        type Point = (Option<f64>, Option<i64>, Option<i64>, Option<i64>);
        let latest: Option<(i64, Point)> = tx
            .query_row(
                "SELECT at, rating, level, rank, rating_games FROM player_ratings
                 WHERE player_id = ?1 ORDER BY at DESC LIMIT 1",
                [id],
                |row| {
                    Ok((
                        row.get(0)?,
                        (row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?),
                    ))
                },
            )
            .optional()?;
        let current: Point = (
            profile.rating,
            profile.level,
            profile.rank,
            profile.rating_games,
        );
        let has_values = current != (None, None, None, None);
        // 只在数值变化时记一条；迟到的旧资料不插进历史中间
        let changed =
            latest.is_none_or(|(latest_at, point)| at.as_millis() > latest_at && point != current);
        if has_values && changed {
            tx.execute(
                "INSERT OR IGNORE INTO player_ratings(player_id, at, rating, level, rank, rating_games)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    id,
                    at.as_millis(),
                    profile.rating,
                    profile.level,
                    profile.rank,
                    profile.rating_games
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// 玩家的 ELO 等数据的变化历史，从旧到新。
    pub fn rating_history(&self, id: PlayerId) -> Result<Vec<RatingPoint>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT at, rating, level, rank, rating_games FROM player_ratings
             WHERE player_id = ?1 ORDER BY at",
        )?;
        let rows = stmt.query_map([id_to_db(id.get())], |row| {
            Ok(RatingPoint {
                at: Timestamp::from_millis(row.get(0)?),
                rating: row.get(1)?,
                level: row.get(2)?,
                rank: row.get(3)?,
                rating_games: row.get(4)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    // ------------------------------------------------------------ 比赛

    /// 把一场比赛的当前状态合并进长期库，连同名单和玩家的相遇时间。
    pub fn record_match(&mut self, state: &MatchState) -> Result<()> {
        // 导入和实时会话可能同时合并同一场比赛，读取旧状态前先取得写锁。
        let tx = rusqlite::Transaction::new_unchecked(
            &self.conn,
            rusqlite::TransactionBehavior::Immediate,
        )?;
        let existing = self.match_record(state.match_id)?;
        let merged = merge_match(existing, state);
        let match_id = id_to_db(merged.id.get());
        tx.execute(
            "INSERT INTO matches(id, discovered_at, phase, rejoined, map, scenario, deck, game_version,
                                 self_id, self_team, start_at, start_source, scenario_loaded_at,
                                 ended_at, end_cause)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
             ON CONFLICT(id) DO UPDATE SET
                 discovered_at = excluded.discovered_at, phase = excluded.phase,
                 rejoined = excluded.rejoined, map = excluded.map, scenario = excluded.scenario,
                 deck = excluded.deck, game_version = excluded.game_version,
                 self_id = excluded.self_id, self_team = excluded.self_team,
                 start_at = excluded.start_at, start_source = excluded.start_source,
                 scenario_loaded_at = excluded.scenario_loaded_at,
                 ended_at = excluded.ended_at, end_cause = excluded.end_cause",
            params![
                match_id,
                merged.discovered_at.as_millis(),
                phase_to_db(merged.phase),
                merged.rejoined,
                merged.map,
                merged.scenario,
                merged.deck,
                merged.game_version,
                merged.self_id.map(|id| id_to_db(id.get())),
                merged.self_team.map(team_to_db),
                merged.clock.map(|c| c.start_at.as_millis()),
                merged.clock.map(|c| source_to_db(c.source)),
                merged.scenario_loaded_at.map(Timestamp::as_millis),
                merged.ended_at.map(Timestamp::as_millis),
                merged.end_cause.map(cause_to_db),
            ],
        )?;
        if state.has_ai {
            tx.execute("UPDATE matches SET has_ai = 1 WHERE id = ?1", [match_id])?;
        }

        let seen_at = merged.discovered_at.as_millis();
        if !state.roster.is_empty() {
            // 只有等待室线索是临时的。重连或分段写入的名单可能只含部分正式成员。
            tx.execute(
                "DELETE FROM match_players WHERE match_id = ?1 AND team IS NULL",
                [match_id],
            )?;
            let confirmed = self.match_players(state.match_id)?;
            let covers_confirmed = confirmed
                .iter()
                .all(|old| state.roster.iter().any(|p| p.id == old.player));
            for (position, entry) in state.roster.iter().enumerate() {
                tx.execute(
                    "INSERT INTO match_players(match_id, player_id, position, team, log_name)
                     SELECT ?1, ?2, COALESCE(?5, MAX(position) + 1, 0), ?3, ?4
                     FROM match_players WHERE match_id = ?1
                     ON CONFLICT(match_id, player_id) DO UPDATE SET
                         team = excluded.team, log_name = COALESCE(excluded.log_name, log_name),
                         position = COALESCE(?5, position)",
                    params![
                        match_id,
                        id_to_db(entry.id.get()),
                        team_to_db(entry.team),
                        entry.name,
                        covers_confirmed.then_some(position as i64),
                    ],
                )?;
                touch_player(&tx, entry.id, entry.name.as_deref(), seen_at)?;
            }
        } else if !state.early.is_empty() {
            let roster_known: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM match_players WHERE match_id = ?1 AND team IS NOT NULL)",
                [match_id], |r| r.get(0),
            )?;
            for (position, early) in state.early.iter().enumerate() {
                touch_player(&tx, early.id, early.name.as_deref(), seen_at)?;
                if roster_known {
                    continue;
                }
                tx.execute(
                    "INSERT INTO match_players(match_id, player_id, position, team, log_name)
                     VALUES (?1, ?2, ?3, NULL, ?4)
                     ON CONFLICT(match_id, player_id) DO UPDATE SET
                         log_name = COALESCE(excluded.log_name, log_name)",
                    params![
                        match_id,
                        id_to_db(early.id.get()),
                        position as i64,
                        early.name
                    ],
                )?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn match_record(&self, id: MatchId) -> Result<Option<MatchRecord>> {
        self.conn
            .query_row(
                &format!("SELECT {MATCH_COLUMNS} FROM matches WHERE id = ?1"),
                [id_to_db(id.get())],
                RawMatch::from_row,
            )
            .optional()?
            .map(RawMatch::decode)
            .transpose()
    }

    /// 最近的比赛，新的在前。
    pub fn recent_matches(&self, limit: u32) -> Result<Vec<MatchRecord>> {
        let mut stmt = self.conn.prepare_cached(&format!(
            "SELECT {MATCH_COLUMNS} FROM matches ORDER BY discovered_at DESC, id DESC LIMIT ?1"
        ))?;
        let rows = stmt.query_map([limit], RawMatch::from_row)?;
        rows.map(|row| row.map_err(StoreError::from).and_then(RawMatch::decode))
            .collect()
    }

    /// 一场比赛的名单，按日志里的顺序。
    pub fn match_players(&self, id: MatchId) -> Result<Vec<MatchPlayerRow>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT player_id, team, log_name FROM match_players WHERE match_id = ?1 ORDER BY position",
        )?;
        let rows = stmt.query_map([id_to_db(id.get())], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (player, team, log_name) = row?;
            out.push(MatchPlayerRow {
                player: PlayerId(id_from_db(player)),
                team: team.as_deref().map(team_from_db).transpose()?,
                log_name,
            });
        }
        Ok(out)
    }

    /// 修改一场比赛的计时基准。用于手动校准和战报修正；来源不如现有基准可信时不生效。
    pub fn set_match_clock(&mut self, id: MatchId, clock: BattleClock) -> Result<bool> {
        let Some(existing) = self.match_record(id)? else {
            return Ok(false);
        };
        if existing.clock.is_some_and(|old| old.source > clock.source) {
            return Ok(false);
        }
        self.conn.execute(
            "UPDATE matches SET start_at = ?2, start_source = ?3 WHERE id = ?1",
            params![
                id_to_db(id.get()),
                clock.start_at.as_millis(),
                source_to_db(clock.source)
            ],
        )?;
        Ok(true)
    }

    /// 与一名玩家的相遇情况。`exclude` 通常是当前这场比赛，免得把它也算进“以前遇到过”。
    pub fn encounters(
        &self,
        player: PlayerId,
        exclude: Option<MatchId>,
    ) -> Result<EncounterSummary> {
        let playing = "mp.team IN ('alpha', 'bravo') AND m.self_team IN ('alpha', 'bravo')";
        let sql = format!(
            "SELECT COUNT(*),
                    COALESCE(SUM(CASE WHEN {playing} AND mp.team = m.self_team THEN 1 ELSE 0 END), 0),
                    COALESCE(SUM(CASE WHEN {playing} AND mp.team <> m.self_team THEN 1 ELSE 0 END), 0),
                    MIN(m.discovered_at),
                    MAX(m.discovered_at)
             FROM match_players mp JOIN matches m ON m.id = mp.match_id
             WHERE mp.player_id = ?1
               AND (m.self_id IS NULL OR m.self_id <> mp.player_id)
               AND (?2 IS NULL OR m.id <> ?2)"
        );
        Ok(self.conn.query_row(
            &sql,
            params![id_to_db(player.get()), exclude.map(|id| id_to_db(id.get()))],
            |row| {
                Ok(EncounterSummary {
                    total: row.get(0)?,
                    as_teammate: row.get(1)?,
                    as_opponent: row.get(2)?,
                    first_at: row.get::<_, Option<i64>>(3)?.map(Timestamp::from_millis),
                    last_at: row.get::<_, Option<i64>>(4)?.map(Timestamp::from_millis),
                })
            },
        )?)
    }

    /// 出现场次最多的玩家，以及有名单的比赛总数。
    ///
    /// 本机玩家出现在自己的每一场比赛里，所以场次接近总数的那个 ID 就是本机玩家。
    /// 日志不直接给出本机玩家的 ID，这是按名字匹配之外的另一条线索。
    pub fn most_common_player(&self) -> Result<Option<(PlayerId, u32, u32)>> {
        let total: u32 = self.conn.query_row(
            "SELECT COUNT(DISTINCT match_id) FROM match_players WHERE team IS NOT NULL",
            [],
            |row| row.get(0),
        )?;
        let top: Option<(i64, u32)> = self
            .conn
            .query_row(
                "SELECT player_id, COUNT(*) AS n FROM match_players WHERE team IS NOT NULL
                 GROUP BY player_id ORDER BY n DESC, player_id LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        Ok(top.map(|(id, count)| (PlayerId(id_from_db(id)), count, total)))
    }

    // ------------------------------------------------------------ 设置

    /// 读取一项设置。没有设置过时为 `None`。
    pub fn setting<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        let raw: Option<String> = self
            .conn
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| {
                row.get(0)
            })
            .optional()?;
        raw.map(|raw| {
            serde_json::from_str(&raw).map_err(|source| StoreError::Setting {
                key: key.to_owned(),
                source,
            })
        })
        .transpose()
    }

    pub fn set_setting<T: Serialize>(&mut self, key: &str, value: &T) -> Result<()> {
        let raw = serde_json::to_string(value).map_err(|source| StoreError::Setting {
            key: key.to_owned(),
            source,
        })?;
        self.conn.execute(
            "INSERT INTO settings(key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, raw],
        )?;
        Ok(())
    }

    pub fn remove_setting(&mut self, key: &str) -> Result<bool> {
        Ok(self
            .conn
            .execute("DELETE FROM settings WHERE key = ?1", [key])?
            > 0)
    }

    /// 键以 `prefix` 开头的全部设置，值为原样的 JSON 文本，按键排序。
    pub fn settings_with_prefix(&self, prefix: &str) -> Result<Vec<(String, String)>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT key, value FROM settings WHERE substr(key, 1, length(?1)) = ?1 ORDER BY key",
        )?;
        let rows = stmt.query_map([prefix], |row| Ok((row.get(0)?, row.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    // ------------------------------------------------------------ 提醒

    /// 分配稳定编号。首次分配时兼容旧版规则和已经删除的提醒的触发记录。
    pub fn reserve_reminder_id(&mut self) -> Result<u32> {
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let last: u32 = tx.query_row(
            "SELECT max(
                coalesce((SELECT CAST(value AS INTEGER) FROM settings WHERE key = 'reminders.lastId'), 0),
                coalesce((SELECT max(rule_id) FROM reminder_log), 0),
                coalesce((SELECT max(json_extract(r.value, '$.id'))
                          FROM settings s, json_each(s.value) r WHERE s.key = 'reminders'), 0)
            )", [], |r| r.get(0),
        )?;
        let id = last
            .checked_add(1)
            .ok_or_else(|| StoreError::Corrupt("提醒编号已用尽".into()))?;
        tx.execute(
            "INSERT INTO settings(key, value) VALUES ('reminders.lastId', ?1)
                    ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [id.to_string()],
        )?;
        tx.commit()?;
        Ok(id)
    }

    /// 记下一条提醒规则在一场比赛里的结果。每条规则每场只记第一次，返回这次是否记下了。
    pub fn record_reminder(
        &mut self,
        match_id: MatchId,
        rule_id: u32,
        outcome: ReminderOutcome,
        at: Timestamp,
    ) -> Result<bool> {
        Ok(self.conn.execute(
            "INSERT OR IGNORE INTO reminder_log(match_id, rule_id, outcome, at) VALUES (?1, ?2, ?3, ?4)",
            params![
                id_to_db(match_id.get()),
                rule_id,
                outcome_to_db(outcome),
                at.as_millis()
            ],
        )? > 0)
    }

    /// 一场比赛里各条提醒规则的结果：规则编号、结果、时刻。
    pub fn reminder_outcomes(
        &self,
        match_id: MatchId,
    ) -> Result<Vec<(u32, ReminderOutcome, Timestamp)>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT rule_id, outcome, at FROM reminder_log WHERE match_id = ?1 ORDER BY rule_id",
        )?;
        let rows = stmt.query_map([id_to_db(match_id.get())], |row| {
            Ok((
                row.get::<_, u32>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (rule, outcome, at) = row?;
            out.push((rule, outcome_from_db(&outcome)?, Timestamp::from_millis(at)));
        }
        Ok(out)
    }
}

fn merge_profile_fields(current: &mut serde_json::Value, incoming: serde_json::Value) {
    if incoming.is_null() || incoming.as_str().is_some_and(|s| s.trim().is_empty()) {
        return;
    }
    if let serde_json::Value::Object(fields) = incoming {
        if !current.is_object() {
            *current = serde_json::json!({});
        }
        for (key, value) in fields {
            merge_profile_fields(&mut current[&key], value);
        }
    } else {
        *current = incoming;
    }
}

fn outcome_to_db(outcome: ReminderOutcome) -> &'static str {
    match outcome {
        ReminderOutcome::Fired => "fired",
        ReminderOutcome::Missed => "missed",
    }
}

fn outcome_from_db(value: &str) -> Result<ReminderOutcome> {
    match value {
        "fired" => Ok(ReminderOutcome::Fired),
        "missed" => Ok(ReminderOutcome::Missed),
        other => Err(StoreError::Corrupt(format!("提醒结果 {other:?}"))),
    }
}

/// 记下在某场比赛里见到了这名玩家。
fn touch_player(
    conn: &Connection,
    id: PlayerId,
    log_name: Option<&str>,
    seen_at: i64,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO players(id, last_log_name, first_seen_at, last_seen_at) VALUES (?1, ?2, ?3, ?3)
         ON CONFLICT(id) DO UPDATE SET
             last_log_name = CASE
                 WHEN excluded.last_log_name IS NOT NULL
                      AND excluded.last_seen_at >= COALESCE(players.last_seen_at, excluded.last_seen_at)
                 THEN excluded.last_log_name ELSE players.last_log_name END,
             first_seen_at = MIN(COALESCE(players.first_seen_at, excluded.first_seen_at), excluded.first_seen_at),
             last_seen_at = MAX(COALESCE(players.last_seen_at, excluded.last_seen_at), excluded.last_seen_at)",
        params![id_to_db(id.get()), log_name, seen_at],
    )?;
    Ok(())
}

/// 把新观察到的状态并入已有记录。已有的信息只会被更可信或更晚的信息取代，不会被空值覆盖。
fn merge_match(existing: Option<MatchRecord>, state: &MatchState) -> MatchRecord {
    let fresh = MatchRecord {
        id: state.match_id,
        discovered_at: state.times.discovered_at,
        phase: state.phase,
        rejoined: state.rejoined,
        map: state.map.clone(),
        scenario: state.scenario.clone(),
        deck: state.deck.clone(),
        game_version: state.game_version.clone(),
        self_id: state.self_id,
        self_team: state.my_team(),
        clock: state.clock,
        scenario_loaded_at: state.times.scenario_loaded_at,
        ended_at: state.times.ended_at,
        end_cause: state.end_cause,
    };
    let Some(old) = existing else {
        return fresh;
    };
    MatchRecord {
        id: fresh.id,
        discovered_at: old.discovered_at.min(fresh.discovered_at),
        phase: old.phase.max(fresh.phase),
        rejoined: old.rejoined || fresh.rejoined,
        map: fresh.map.or(old.map),
        scenario: fresh.scenario.or(old.scenario),
        deck: fresh.deck.or(old.deck),
        game_version: fresh.game_version.or(old.game_version),
        self_id: fresh.self_id.or(old.self_id),
        self_team: fresh.self_team.or(old.self_team),
        clock: match (old.clock, fresh.clock) {
            (Some(a), Some(b)) => Some(if b.source >= a.source { b } else { a }),
            (a, b) => b.or(a),
        },
        scenario_loaded_at: old.scenario_loaded_at.or(fresh.scenario_loaded_at),
        ended_at: old.ended_at.or(fresh.ended_at),
        end_cause: old.end_cause.or(fresh.end_cause),
    }
}

const PLAYER_COLUMNS: &str = "id, name, steam_id, level, rating, rank, rating_games, profile_json, \
                              fetched_at, last_log_name, first_seen_at, last_seen_at, trace_count, trace_checked_at";

fn player_from_row(row: &Row<'_>) -> rusqlite::Result<PlayerRecord> {
    let ts = |i: usize| -> rusqlite::Result<Option<Timestamp>> {
        Ok(row.get::<_, Option<i64>>(i)?.map(Timestamp::from_millis))
    };
    Ok(PlayerRecord {
        id: PlayerId(id_from_db(row.get(0)?)),
        name: row.get(1)?,
        steam_id: row.get(2)?,
        level: row.get(3)?,
        rating: row.get(4)?,
        rank: row.get(5)?,
        rating_games: row.get(6)?,
        profile_json: row.get(7)?,
        fetched_at: ts(8)?,
        last_log_name: row.get(9)?,
        first_seen_at: ts(10)?,
        last_seen_at: ts(11)?,
        trace_count: row.get(12)?,
        trace_checked_at: ts(13)?,
    })
}

const MATCH_COLUMNS: &str = "id, discovered_at, phase, rejoined, map, scenario, deck, game_version, \
                             self_id, self_team, start_at, start_source, scenario_loaded_at, \
                             ended_at, end_cause";

/// `matches` 表的一行，枚举列还是文本。读行与解释取值分两步，各自只处理一种错误。
struct RawMatch {
    id: i64,
    discovered_at: i64,
    phase: String,
    rejoined: bool,
    map: Option<String>,
    scenario: Option<String>,
    deck: Option<String>,
    game_version: Option<String>,
    self_id: Option<i64>,
    self_team: Option<String>,
    start_at: Option<i64>,
    start_source: Option<String>,
    scenario_loaded_at: Option<i64>,
    ended_at: Option<i64>,
    end_cause: Option<String>,
}

impl RawMatch {
    fn from_row(row: &Row<'_>) -> rusqlite::Result<RawMatch> {
        Ok(RawMatch {
            id: row.get(0)?,
            discovered_at: row.get(1)?,
            phase: row.get(2)?,
            rejoined: row.get(3)?,
            map: row.get(4)?,
            scenario: row.get(5)?,
            deck: row.get(6)?,
            game_version: row.get(7)?,
            self_id: row.get(8)?,
            self_team: row.get(9)?,
            start_at: row.get(10)?,
            start_source: row.get(11)?,
            scenario_loaded_at: row.get(12)?,
            ended_at: row.get(13)?,
            end_cause: row.get(14)?,
        })
    }

    fn decode(self) -> Result<MatchRecord> {
        let clock = match (self.start_at, self.start_source.as_deref()) {
            (Some(start_at), Some(source)) => Some(BattleClock {
                start_at: Timestamp::from_millis(start_at),
                source: source_from_db(source)?,
            }),
            _ => None,
        };
        Ok(MatchRecord {
            id: MatchId(id_from_db(self.id)),
            discovered_at: Timestamp::from_millis(self.discovered_at),
            phase: phase_from_db(&self.phase)?,
            rejoined: self.rejoined,
            map: self.map,
            scenario: self.scenario,
            deck: self.deck,
            game_version: self.game_version,
            self_id: self.self_id.map(|id| PlayerId(id_from_db(id))),
            self_team: self.self_team.as_deref().map(team_from_db).transpose()?,
            clock,
            scenario_loaded_at: self.scenario_loaded_at.map(Timestamp::from_millis),
            ended_at: self.ended_at.map(Timestamp::from_millis),
            end_cause: self.end_cause.as_deref().map(cause_from_db).transpose()?,
        })
    }
}

// 写进数据库的枚举取值。它们是存储格式的一部分，改名需要迁移脚本，所以不跟着类型的序列化名走。

pub(crate) fn id_to_db(id: u64) -> i64 {
    id as i64
}

pub(crate) fn id_from_db(id: i64) -> u64 {
    id as u64
}

pub(crate) fn team_to_db(team: Team) -> &'static str {
    match team {
        Team::Alpha => "alpha",
        Team::Bravo => "bravo",
        Team::Spectators => "spectators",
    }
}

pub(crate) fn team_from_db(value: &str) -> Result<Team> {
    match value {
        "alpha" => Ok(Team::Alpha),
        "bravo" => Ok(Team::Bravo),
        "spectators" => Ok(Team::Spectators),
        other => Err(StoreError::Corrupt(format!("队伍 {other:?}"))),
    }
}

fn phase_to_db(phase: Phase) -> &'static str {
    match phase {
        Phase::WaitingRoom => "waitingRoom",
        Phase::Loading => "loading",
        Phase::Deployment => "deployment",
        Phase::InBattle => "inBattle",
        Phase::Ended => "ended",
    }
}

pub(crate) fn phase_from_db(value: &str) -> Result<Phase> {
    match value {
        "waitingRoom" => Ok(Phase::WaitingRoom),
        "loading" => Ok(Phase::Loading),
        "deployment" => Ok(Phase::Deployment),
        "inBattle" => Ok(Phase::InBattle),
        "ended" => Ok(Phase::Ended),
        other => Err(StoreError::Corrupt(format!("阶段 {other:?}"))),
    }
}

fn source_to_db(source: ClockSource) -> &'static str {
    match source {
        ClockSource::LogEstimate => "logEstimate",
        ClockSource::Manual => "manual",
        ClockSource::Report => "report",
    }
}

fn source_from_db(value: &str) -> Result<ClockSource> {
    match value {
        "logEstimate" => Ok(ClockSource::LogEstimate),
        "manual" => Ok(ClockSource::Manual),
        "report" => Ok(ClockSource::Report),
        other => Err(StoreError::Corrupt(format!("计时来源 {other:?}"))),
    }
}

fn cause_to_db(cause: ExitCause) -> &'static str {
    match cause {
        ExitCause::MatchEnded => "matchEnded",
        ExitCause::ServerExitInstance => "serverExitInstance",
        ExitCause::ConnectionLost => "connectionLost",
        ExitCause::ServiceDisposed => "serviceDisposed",
        ExitCause::Manual => "manual",
        ExitCause::Unknown => "unknown",
    }
}

fn cause_from_db(value: &str) -> Result<ExitCause> {
    match value {
        "matchEnded" => Ok(ExitCause::MatchEnded),
        "serverExitInstance" => Ok(ExitCause::ServerExitInstance),
        "connectionLost" => Ok(ExitCause::ConnectionLost),
        "serviceDisposed" => Ok(ExitCause::ServiceDisposed),
        "manual" => Ok(ExitCause::Manual),
        "unknown" => Ok(ExitCause::Unknown),
        other => Err(StoreError::Corrupt(format!("结束原因 {other:?}"))),
    }
}

#[cfg(test)]
mod tests;
