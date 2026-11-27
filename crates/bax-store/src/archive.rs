//! 玩家档案：备注、按名字或 ID 查找、与某名玩家同场过的比赛。

use bax_core::{MatchId, Phase, PlayerId, Team, Timestamp};
use rusqlite::{OptionalExtension, params};
use serde::Serialize;

use crate::Result;
use crate::store::{Store, id_from_db, id_to_db, phase_from_db, team_from_db};

/// 一条备注。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Note {
    pub body: String,
    pub updated_at: Timestamp,
}

/// 查找结果里的一名玩家。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct PlayerHit {
    pub id: PlayerId,
    /// 接口资料里的名字。
    pub name: Option<String>,
    /// 最近一次在日志名单里见到的名字。
    pub log_name: Option<String>,
    pub rating: Option<f64>,
    pub level: Option<i64>,
    pub last_seen_at: Option<Timestamp>,
    /// 一共同场过多少次。
    pub encounters: u32,
    pub has_note: bool,
    pub is_favorite: bool,
}

/// 与本机玩家的关系。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum Relation {
    Teammate,
    Opponent,
    /// 观战，或者某一方的队伍未知。
    Other,
}

/// 与某名玩家同场过的一场比赛。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct EncounterRow {
    pub match_id: MatchId,
    pub at: Timestamp,
    pub map: Option<String>,
    pub relation: Relation,
    /// 对方在这场比赛里的队伍。
    pub team: Option<Team>,
    pub phase: Phase,
    /// 对方在这场比赛日志里的名字。
    pub log_name: Option<String>,
    /// 从开局到结束的毫秒数；不知道开局或结束时刻时为 `None`。
    pub duration_ms: Option<i64>,
}

/// 把用户输入变成 LIKE 的模式：前后加 `%`，转义通配符。
pub(crate) fn like_pattern(query: &str) -> String {
    let mut pattern = String::with_capacity(query.len() + 2);
    pattern.push('%');
    for c in query.chars() {
        if matches!(c, '%' | '_' | '\\') {
            pattern.push('\\');
        }
        pattern.push(c);
    }
    pattern.push('%');
    pattern
}

impl Store {
    /// OCR 的本地名称索引，包含历史日志别名；调用方做 Unicode 比较。
    pub fn player_aliases(&self) -> Result<Vec<(PlayerId, String, Option<i64>)>> {
        let mut stmt = self.conn.prepare(
            "SELECT p.id, a.name, p.level FROM players p JOIN (
                SELECT id AS player_id, name FROM players WHERE name IS NOT NULL
                UNION SELECT id, last_log_name FROM players WHERE last_log_name IS NOT NULL
                UNION SELECT player_id, log_name FROM match_players WHERE log_name IS NOT NULL
            ) a ON a.player_id = p.id WHERE a.name <> ''",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((PlayerId(id_from_db(row.get(0)?)), row.get(1)?, row.get(2)?))
        })?;
        rows.map(|r| r.map_err(Into::into)).collect()
    }

    /// 一名玩家的备注。
    pub fn note(&self, player: PlayerId) -> Result<Option<Note>> {
        Ok(self
            .conn
            .query_row(
                "SELECT body, updated_at FROM notes WHERE player_id = ?1",
                [id_to_db(player.get())],
                |row| {
                    Ok(Note {
                        body: row.get(0)?,
                        updated_at: Timestamp::from_millis(row.get(1)?),
                    })
                },
            )
            .optional()?)
    }

    /// 保存备注。去掉首尾空白后为空时删除这条备注。
    pub fn set_note(&mut self, player: PlayerId, body: &str, at: Timestamp) -> Result<()> {
        let body = body.trim();
        if body.is_empty() {
            self.conn.execute(
                "DELETE FROM notes WHERE player_id = ?1",
                [id_to_db(player.get())],
            )?;
        } else {
            self.conn.execute(
                "INSERT INTO notes(player_id, body, updated_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT(player_id) DO UPDATE SET body = excluded.body, updated_at = excluded.updated_at",
                params![id_to_db(player.get()), body, at.as_millis()],
            )?;
        }
        Ok(())
    }

    /// 收藏是本机的个人设置，每个玩家单独写入，取消时删除对应键。
    pub fn set_player_favorite(&mut self, player: PlayerId, favorite: bool) -> Result<()> {
        let key = format!("players.favorite.{}", id_to_db(player.get()));
        if favorite {
            self.set_setting(&key, &true)
        } else {
            self.remove_setting(&key).map(|_| ())
        }
    }

    /// 按名字或 ID 查找玩家。查询为空时列出最近见过的玩家。
    ///
    /// 名字同时在接口资料与日志里的名字中查找，不区分英文大小写。完全匹配 ID 的排在最前，
    /// 其余先列收藏，再按最近一次见到的时间排序。
    pub fn search_players(&self, query: &str, limit: u32) -> Result<Vec<PlayerHit>> {
        self.search_players_filtered(query, limit, 0, "recent", "all", None)
    }

    /// 筛选和排序在 LIMIT 之前执行，分页不会漏掉常见玩家。
    pub fn search_players_filtered(
        &self,
        query: &str,
        limit: u32,
        offset: u32,
        sort: &str,
        filter: &str,
        self_id: Option<PlayerId>,
    ) -> Result<Vec<PlayerHit>> {
        self.search_players_in_match(query, limit, offset, sort, filter, (self_id, None))
    }

    pub fn search_players_in_match(
        &self,
        query: &str,
        limit: u32,
        offset: u32,
        sort: &str,
        filter: &str,
        (self_id, active_match): (Option<PlayerId>, Option<MatchId>),
    ) -> Result<Vec<PlayerHit>> {
        let query = query.trim();
        let exact_id = query.parse::<u64>().ok().map(id_to_db);
        let pattern = like_pattern(query);
        let (priority, ordering) = match sort {
            "ratingDesc" => ("in_match DESC", "rating IS NULL, rating DESC"),
            "ratingAsc" => ("in_match DESC", "rating IS NULL, rating ASC"),
            "name" => (
                "in_match DESC",
                "COALESCE(name, last_log_name, '') COLLATE NOCASE",
            ),
            "encounters" => (
                "in_match DESC, (id = ?5) DESC",
                "encounters DESC, last_seen_at DESC",
            ),
            _ => (
                "CASE WHEN id = ?5 THEN 0 WHEN in_match THEN 1 WHEN is_favorite THEN 2 ELSE 3 END",
                "last_seen_at DESC",
            ),
        };
        let condition = match filter {
            "noted" => "has_note = 1",
            "encountered" => "encounters > 0",
            "rated" => "rating IS NOT NULL",
            _ => "1 = 1",
        };
        let sql = format!("WITH hits AS (SELECT p.id, p.name, p.last_log_name, p.rating, p.level, p.last_seen_at,
                    (SELECT COUNT(*) FROM match_players mp JOIN matches m ON m.id = mp.match_id
                      WHERE mp.player_id = p.id AND (m.self_id IS NULL OR m.self_id <> mp.player_id)) AS encounters,
                    EXISTS (SELECT 1 FROM notes n WHERE n.player_id = p.id) AS has_note,
                    EXISTS (SELECT 1 FROM settings WHERE key = 'players.favorite.' || p.id) AS is_favorite,
                    EXISTS (SELECT 1 FROM match_players mp WHERE mp.match_id = ?7 AND mp.player_id = p.id
                            AND mp.team IN ('alpha', 'bravo') AND mp.player_id IS NOT ?5) AS in_match
             FROM players p
             WHERE ?1 = ''
                OR p.id = ?2
                OR p.name LIKE ?3 ESCAPE '\\'
                OR p.last_log_name LIKE ?3 ESCAPE '\\'
             ) SELECT * FROM hits WHERE {condition}
             ORDER BY (id = ?2) DESC, {priority}, {ordering}, id
             LIMIT ?4 OFFSET ?6");
        let mut stmt = self.conn.prepare_cached(&sql)?;
        let rows = stmt.query_map(
            params![
                query,
                exact_id,
                pattern,
                limit,
                self_id.map(|id| id_to_db(id.get())),
                offset,
                active_match.map(|id| id_to_db(id.get()))
            ],
            |row| {
                Ok(PlayerHit {
                    id: PlayerId(id_from_db(row.get(0)?)),
                    name: row.get(1)?,
                    log_name: row.get(2)?,
                    rating: row.get(3)?,
                    level: row.get(4)?,
                    last_seen_at: row.get::<_, Option<i64>>(5)?.map(Timestamp::from_millis),
                    encounters: row.get(6)?,
                    has_note: row.get(7)?,
                    is_favorite: row.get(8)?,
                })
            },
        )?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// 与一名玩家同场过的比赛，新的在前。不含本机玩家自己。
    pub fn encounter_list(&self, player: PlayerId, limit: u32) -> Result<Vec<EncounterRow>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT m.id, m.discovered_at, m.map, mp.team, m.self_team, m.phase, mp.log_name,
                    m.start_at, m.ended_at
             FROM match_players mp JOIN matches m ON m.id = mp.match_id
             WHERE mp.player_id = ?1 AND (m.self_id IS NULL OR m.self_id <> mp.player_id)
             ORDER BY m.discovered_at DESC, m.id DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![id_to_db(player.get()), limit], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, Option<i64>>(7)?,
                row.get::<_, Option<i64>>(8)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, at, map, team, self_team, phase, log_name, start_at, ended_at) = row?;
            let team = team.as_deref().map(team_from_db).transpose()?;
            let self_team = self_team.as_deref().map(team_from_db).transpose()?;
            let relation = match (team, self_team) {
                (Some(a), Some(b)) if a.is_playing() && b.is_playing() => {
                    if a == b {
                        Relation::Teammate
                    } else {
                        Relation::Opponent
                    }
                }
                _ => Relation::Other,
            };
            out.push(EncounterRow {
                match_id: MatchId(id_from_db(id)),
                at: Timestamp::from_millis(at),
                map,
                relation,
                team,
                phase: phase_from_db(&phase)?,
                log_name,
                duration_ms: match (start_at, ended_at) {
                    (Some(start), Some(end)) if end >= start => Some(end - start),
                    _ => None,
                },
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use bax_core::{LogEvent, RoomKind, RoomStatus, RosterEntry, Tracker};

    use super::*;
    use crate::ProfileUpdate;

    fn t(secs: i64) -> Timestamp {
        Timestamp::from_secs(secs)
    }

    fn entry(id: u64, name: &str, team: Team) -> RosterEntry {
        RosterEntry {
            id: PlayerId(id),
            name: Some(name.to_owned()),
            team,
        }
    }

    /// 记录一场比赛：本机（500，名字 Me）在 Bravo。
    fn record(store: &mut Store, match_id: u64, at: i64, roster: Vec<RosterEntry>, ended: bool) {
        let id = MatchId(match_id);
        let mut tracker = Tracker::default();
        tracker.apply(t(at), &LogEvent::Persona { name: "Me".into() });
        tracker.apply(
            t(at),
            &LogEvent::RoomEntered {
                room: RoomKind::WaitingRoom,
                match_id: id,
                status: Some(RoomStatus::WaitClients),
            },
        );
        tracker.apply(
            t(at + 30),
            &LogEvent::LoadingStarted {
                map: format!("Map{match_id}"),
                scenario: None,
            },
        );
        tracker.apply(
            t(at + 40),
            &LogEvent::RoomEntered {
                room: RoomKind::GameRoom,
                match_id: id,
                status: Some(RoomStatus::WaitClients),
            },
        );
        tracker.apply(t(at + 40), &LogEvent::PlayerList { players: roster });
        tracker.apply(t(at + 70), &LogEvent::ScenarioLoaded);
        if ended {
            tracker.apply(t(at + 1330), &LogEvent::MatchEnded { match_id: id });
        }
        store
            .record_match(tracker.current().expect("有比赛"))
            .expect("记录比赛");
    }

    fn sample() -> Store {
        let mut store = Store::open_in_memory().expect("打开长期库");
        record(
            &mut store,
            1,
            1000,
            vec![
                entry(101, "Ash", Team::Alpha),
                entry(201, "Dune", Team::Bravo),
                entry(500, "Me", Team::Bravo),
            ],
            true,
        );
        record(
            &mut store,
            2,
            5000,
            vec![
                entry(101, "Ash_Renamed", Team::Bravo),
                entry(500, "Me", Team::Bravo),
                entry(301, "100%_Elm", Team::Alpha),
            ],
            false,
        );
        store
            .save_profile(
                &ProfileUpdate {
                    id: PlayerId(101),
                    name: Some("Ashen".into()),
                    steam_id: None,
                    level: Some(40),
                    rating: Some(2100.0),
                    rank: None,
                    rating_games: None,
                    raw_json: "{}".into(),
                },
                t(5100),
            )
            .expect("写入资料");
        store
    }

    #[test]
    fn notes_are_saved_replaced_and_removed() {
        let mut store = sample();
        assert_eq!(store.note(PlayerId(101)).expect("读取"), None);
        store
            .set_note(PlayerId(101), "  爱推坦克\n第二行 ", t(10))
            .expect("写入");
        assert_eq!(
            store.note(PlayerId(101)).expect("读取"),
            Some(Note {
                body: "爱推坦克\n第二行".into(),
                updated_at: t(10)
            })
        );
        store.set_note(PlayerId(101), "改了", t(20)).expect("覆盖");
        assert_eq!(
            store.note(PlayerId(101)).expect("读取").map(|n| n.body),
            Some("改了".into())
        );
        store
            .set_note(PlayerId(101), "   ", t(30))
            .expect("清空即删除");
        assert_eq!(store.note(PlayerId(101)).expect("读取"), None);
        // 没在比赛里见过的玩家也可以写备注
        store
            .set_note(PlayerId(9999), "听说很强", t(40))
            .expect("写入");
        assert!(store.note(PlayerId(9999)).expect("读取").is_some());
    }

    #[test]
    fn notes_survive_profile_updates() {
        let mut store = sample();
        store.set_note(PlayerId(101), "备注", t(10)).expect("写入");
        store
            .save_profile(
                &ProfileUpdate {
                    id: PlayerId(101),
                    name: Some("New".into()),
                    steam_id: None,
                    level: None,
                    rating: Some(1.0),
                    rank: None,
                    rating_games: None,
                    raw_json: "{}".into(),
                },
                t(99_999),
            )
            .expect("刷新资料");
        assert_eq!(
            store.note(PlayerId(101)).expect("读取").map(|n| n.body),
            Some("备注".into())
        );
    }

    #[test]
    fn search_matches_names_ids_and_escapes_wildcards() {
        let mut store = sample();
        store.set_note(PlayerId(201), "x", t(1)).expect("写入");

        let ids = |q: &str| -> Vec<u64> {
            store
                .search_players(q, 20)
                .expect("查找")
                .into_iter()
                .map(|h| h.id.get())
                .collect()
        };
        // 接口资料里的名字和日志里的名字都能找到，不区分英文大小写
        assert_eq!(ids("ashen"), vec![101]);
        assert_eq!(ids("renamed"), vec![101]);
        assert_eq!(ids("dune"), vec![201]);
        // 完全匹配的 ID 排在最前，名字里含这串数字的随后
        assert_eq!(ids("101"), vec![101]);
        assert_eq!(ids("100%"), vec![301], "百分号按字面匹配");
        assert_eq!(ids("_"), vec![101, 301], "下划线按字面匹配");
        assert!(ids("zzz").is_empty());
        // 空查询：最近见过的在前，同一场里见到的按 ID 排
        assert_eq!(ids(""), vec![101, 301, 500, 201]);

        let hits = store.search_players("dune", 5).expect("查找");
        assert_eq!(hits[0].encounters, 1);
        assert!(hits[0].has_note);
        assert_eq!(hits[0].log_name.as_deref(), Some("Dune"));
        let ash = &store.search_players("101", 5).expect("查找")[0];
        assert_eq!(ash.name.as_deref(), Some("Ashen"));
        assert_eq!(ash.rating, Some(2100.0));
        assert_eq!(ash.encounters, 2);
        assert!(!ash.has_note);
    }

    #[test]
    fn filters_and_default_order_apply_before_pagination() {
        let mut store = sample();
        store.set_note(PlayerId(201), "备注", t(1)).expect("备注");
        let ids = |sort, filter, offset, limit| -> Vec<u64> {
            store
                .search_players_filtered("", limit, offset, sort, filter, Some(PlayerId(500)))
                .expect("查询")
                .into_iter()
                .map(|p| p.id.get())
                .collect()
        };
        assert_eq!(ids("default", "all", 0, 2), vec![500, 101]);
        assert_eq!(ids("default", "all", 2, 2), vec![301, 201]);
        assert_eq!(ids("default", "noted", 0, 1), vec![201]);
        assert_eq!(ids("ratingDesc", "rated", 0, 1), vec![101]);
        assert_eq!(ids("ratingAsc", "all", 0, 1), vec![101]);
        assert_eq!(ids("default", "encountered", 0, 10), vec![101, 301, 201]);
        assert_eq!(ids("name", "all", 0, 2), vec![301, 101]);
        assert!(ids("default", "all", 99, 1).is_empty());
        let first = store
            .search_players_in_match(
                "",
                1,
                0,
                "default",
                "all",
                (Some(PlayerId(500)), Some(MatchId(2))),
            )
            .expect("查询本局玩家");
        assert_eq!(first[0].id, PlayerId(500));
    }

    #[test]
    fn favorites_rank_after_self_and_active_players_before_pagination() {
        let mut store = sample();
        let page = |store: &Store, active_match, sort, offset| {
            store
                .search_players_in_match(
                    "",
                    2,
                    offset,
                    sort,
                    "all",
                    (Some(PlayerId(500)), active_match),
                )
                .expect("分页")
                .into_iter()
                .map(|p| (p.id.get(), p.is_favorite))
                .collect::<Vec<_>>()
        };
        for _ in 0..2 {
            store
                .set_player_favorite(PlayerId(201), true)
                .expect("收藏");
        }
        assert_eq!(page(&store, None, "recent", 0), [(500, false), (201, true)]);
        assert_eq!(
            page(&store, Some(MatchId(2)), "recent", 0),
            [(500, false), (101, false)]
        );
        assert_eq!(
            page(&store, Some(MatchId(2)), "recent", 2),
            [(301, false), (201, true)]
        );
        assert_eq!(page(&store, None, "name", 0), [(301, false), (101, false)]);
        for _ in 0..2 {
            store
                .set_player_favorite(PlayerId(201), false)
                .expect("取消收藏");
        }
        assert_eq!(
            page(&store, None, "recent", 0),
            [(500, false), (101, false)]
        );
        assert_eq!(
            page(&store, None, "recent", 2),
            [(301, false), (201, false)]
        );
    }

    #[test]
    fn encounter_list_tells_teammates_from_opponents() {
        let store = sample();
        let rows = store.encounter_list(PlayerId(101), 10).expect("读取");
        assert_eq!(rows.len(), 2);
        // 新的在前：第二场是队友
        assert_eq!(rows[0].match_id, MatchId(2));
        assert_eq!(rows[0].relation, Relation::Teammate);
        assert_eq!(rows[0].log_name.as_deref(), Some("Ash_Renamed"));
        assert_eq!(rows[0].map.as_deref(), Some("Map2"));
        assert_eq!(rows[0].duration_ms, None, "还没结束");
        assert_eq!(rows[1].relation, Relation::Opponent);
        assert_eq!(rows[1].team, Some(Team::Alpha));
        assert_eq!(rows[1].phase, Phase::Ended);
        // 开局时刻是场景加载完成后 60 秒
        assert_eq!(rows[1].duration_ms, Some((1330 - 130) * 1000));

        assert!(
            store
                .encounter_list(PlayerId(500), 10)
                .expect("读取")
                .is_empty(),
            "自己不算"
        );
        assert_eq!(
            store.encounter_list(PlayerId(101), 1).expect("读取").len(),
            1
        );
    }

    #[test]
    fn like_patterns_escape_wildcards() {
        assert_eq!(like_pattern("a%b_c\\"), "%a\\%b\\_c\\\\%");
        assert_eq!(like_pattern(""), "%%");
    }
}
