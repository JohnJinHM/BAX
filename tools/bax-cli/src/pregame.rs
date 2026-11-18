//! 对照等待室日志线索、跟踪器接收结果与后来的完整名单。只读文件，不访问接口。

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use anyhow::{Context, Result};
use bax_core::{LogEvent, MatchId, PlayerId, RoomKind, Tracker, TrackerConfig, Update};
use bax_engine::history;
use bax_gamelog::{LogTime, LogZone, TimedEvent, discovery, tail};

#[derive(Debug, Default)]
struct MatchAudit {
    entered: Option<LogTime>,
    loading: Option<LogTime>,
    roster_at: Option<LogTime>,
    room_lines: usize,
    before_loading: BTreeSet<PlayerId>,
    accepted: BTreeSet<PlayerId>,
    roster: BTreeSet<PlayerId>,
}

#[derive(Debug, Default)]
struct Audit {
    matches: BTreeMap<MatchId, MatchAudit>,
    unassociated: usize,
}

impl Audit {
    fn observe(&mut self, timed: &TimedEvent, updates: &[Update], tracker: &Tracker) {
        let Some(state) = tracker.current() else {
            if matches!(
                timed.event,
                LogEvent::RoomClient {
                    room: RoomKind::WaitingRoom,
                    ..
                }
            ) {
                self.unassociated += 1;
            }
            return;
        };
        let audit = self.matches.entry(state.match_id).or_default();
        match &timed.event {
            LogEvent::RoomEntered {
                room: RoomKind::WaitingRoom,
                ..
            } => {
                audit.entered.get_or_insert(timed.at);
            }
            LogEvent::LoadingStarted { .. } if state.times.loading_at.is_some() => {
                audit.loading.get_or_insert(timed.at);
            }
            LogEvent::RoomClient {
                room: RoomKind::WaitingRoom,
                player,
                ..
            } => {
                audit.room_lines += 1;
                if audit.loading.is_none() && audit.entered.is_some() && audit.roster_at.is_none() {
                    audit.before_loading.insert(*player);
                    if updates.iter().any(|u| matches!(u, Update::EarlyPlayers { players, .. } if players.contains(player))) {
                        audit.accepted.insert(*player);
                    }
                }
            }
            LogEvent::PlayerList { .. }
                if updates
                    .iter()
                    .any(|u| matches!(u, Update::RosterReady { .. })) =>
            {
                audit.roster_at.get_or_insert(timed.at);
                audit.roster = state.roster.iter().map(|p| p.id).collect();
            }
            _ => {}
        }
    }
}

pub fn run(file: Option<PathBuf>, dir: Option<PathBuf>, all: bool) -> Result<()> {
    let files = if let Some(file) = file {
        vec![file]
    } else {
        let dir = dir
            .or_else(discovery::find_log_dir)
            .context("没有找到日志目录，请用 --dir 指定")?;
        if all {
            tail::list_log_files(&dir)?
        } else {
            vec![tail::latest_log_file(&dir)?.context("目录里没有游戏日志")?]
        }
    };
    println!(
        "文件\t比赛\t进入等待室\t开始加载\t首次名单\t等待室行数\t加载前唯一ID\t接收ID\t名单人数\t提前覆盖\t缺少线索\t线索未入名单"
    );
    let mut distribution = BTreeMap::<usize, usize>::new();
    let mut complete = 0usize;
    let mut orphaned = 0usize;
    for file in &files {
        let mut audit = Audit::default();
        history::replay_file(
            file,
            LogZone::System,
            &TrackerConfig::default(),
            |event, updates, tracker| {
                audit.observe(event, updates, tracker);
            },
        )
        .with_context(|| format!("读取 {} 失败", file.display()))?;
        orphaned += audit.unassociated;
        for (id, a) in audit.matches {
            let covered = a.roster.intersection(&a.before_loading).count();
            let missing = a.roster.difference(&a.before_loading).count();
            let extra = a.before_loading.difference(&a.roster).count();
            if a.entered.is_some() && a.roster_at.is_some() {
                *distribution.entry(covered).or_default() += 1;
                complete += 1;
            }
            let time =
                |at: Option<LogTime>| at.map(|t| t.to_string()).unwrap_or_else(|| "?".to_owned());
            println!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                file.file_name().unwrap_or_default().to_string_lossy(),
                id,
                time(a.entered),
                time(a.loading),
                time(a.roster_at),
                a.room_lines,
                a.before_loading.len(),
                a.accepted.len(),
                a.roster.len(),
                if a.roster_at.is_some() {
                    covered.to_string()
                } else {
                    "?".to_owned()
                },
                if a.roster_at.is_some() {
                    missing.to_string()
                } else {
                    "?".to_owned()
                },
                if a.roster_at.is_some() {
                    extra.to_string()
                } else {
                    "?".to_owned()
                }
            );
        }
    }
    eprintln!(
        "{} 份文件，{complete} 个有等待室和名单的记录（跨文件未去重）；加载前覆盖人数分布 {distribution:?}；无比赛可关联的等待室事件 {orphaned}",
        files.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bax_gamelog::LogParser;

    #[test]
    fn distinguishes_missing_source_from_duplicate_and_late_events() {
        let mut parser = LogParser::new();
        let mut tracker = Tracker::default();
        let mut audit = Audit::default();
        let mut events = Vec::new();
        let records = [
            "Room: (WaitingRoom|1), Client: (Orphan|99)",
            "[NET_ROOM] WaitingRoom entered, fight id: 9000001",
            "Room: (WaitingRoom|1), Client: (Aster|101)",
            "Room: (WaitingRoom|1), Client: (Aster|101)",
            "Start loading battle... map: Test, scenario: - Test_network",
            "Room: (WaitingRoom|1), Client: (Birch|102)",
            "[NET_ROOM] GameRoom entered, fight id: 9000001",
            "Player list:\nID: 101, Name: Aster, Team: Alpha\nID: 102, Name: Birch, Team: Bravo",
        ];
        for (i, record) in records.iter().enumerate() {
            let text = format!("[2026-11-17 18:00:{i:02}.000]\nLog: {record}\n\n");
            for line in text.lines() {
                parser.push_line(line, &mut events);
            }
            for event in events.drain(..) {
                let updates = tracker.apply(event.at.to_utc(LogZone::System), &event.event);
                audit.observe(&event, &updates, &tracker);
            }
        }
        assert_eq!(audit.unassociated, 1);
        let a = &audit.matches[&MatchId(9_000_001)];
        assert_eq!(a.room_lines, 3);
        assert_eq!(a.before_loading, BTreeSet::from([PlayerId(101)]));
        assert_eq!(a.accepted, a.before_loading);
        assert_eq!(a.roster.len(), 2);
        assert!(a.loading.is_some());
        assert!(a.roster_at.is_some());
    }
}
