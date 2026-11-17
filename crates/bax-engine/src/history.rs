//! 回放与导入历史日志。
//!
//! 引擎运行时只读最新的一份日志。以前的日志里也记着大量比赛和同场玩家，
//! 导入之后“以前遇到过几次”才有意义。

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::{Path, PathBuf};

use bax_core::{MatchId, MatchState, Tracker, TrackerConfig, Update};
use bax_gamelog::{LogParser, LogZone, TimedEvent};
use bax_store::Store;

/// 一次回放读了多少内容。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReplayStats {
    pub lines: u64,
    pub events: u64,
}

/// 把一份日志从头到尾过一遍解析器和对局跟踪器。
///
/// 每处理一个事件调用一次 `on_event`，参数是事件、跟踪器报告的变化和跟踪器本身。
/// 准备阶段结束这类由时间决定的变化，按事件自己的时间推进。
pub fn replay_file(
    path: &Path,
    zone: LogZone,
    config: &TrackerConfig,
    mut on_event: impl FnMut(&TimedEvent, &[Update], &Tracker),
) -> io::Result<ReplayStats> {
    let file = File::open(path)?;
    let mut reader = BufReader::with_capacity(1 << 16, file);
    let mut parser = LogParser::new();
    let mut tracker = Tracker::new(config.clone());
    let mut stats = ReplayStats::default();
    let mut raw = Vec::new();
    let mut parsed = Vec::new();

    let mut feed =
        |parsed: &mut Vec<TimedEvent>, tracker: &mut Tracker, stats: &mut ReplayStats| {
            for timed in parsed.drain(..) {
                stats.events += 1;
                let at = timed.at.to_utc(zone);
                let mut updates = tracker.apply(at, &timed.event);
                updates.extend(tracker.advance(at));
                on_event(&timed, &updates, tracker);
            }
        };

    loop {
        raw.clear();
        if reader.read_until(b'\n', &mut raw)? == 0 {
            break;
        }
        stats.lines += 1;
        if raw.last() == Some(&b'\n') {
            raw.pop();
        }
        parser.push_line(&String::from_utf8_lossy(&raw), &mut parsed);
        if !parsed.is_empty() {
            feed(&mut parsed, &mut tracker, &mut stats);
        }
    }
    parser.flush(&mut parsed);
    feed(&mut parsed, &mut tracker, &mut stats);
    Ok(stats)
}

/// 一份日志里出现过的每场比赛的最终状态，按出现顺序排列。
pub fn matches_in_file(
    path: &Path,
    zone: LogZone,
    config: &TrackerConfig,
) -> io::Result<Vec<MatchState>> {
    let mut order = Vec::new();
    let mut latest: HashMap<MatchId, MatchState> = HashMap::new();
    replay_file(path, zone, config, |_, updates, tracker| {
        if updates.is_empty() {
            return;
        }
        if let Some(state) = tracker.current() {
            if !latest.contains_key(&state.match_id) {
                order.push(state.match_id);
            }
            latest.insert(state.match_id, state.clone());
        }
    })?;
    Ok(order
        .into_iter()
        .filter_map(|id| latest.remove(&id))
        .collect())
}

/// 导入的结果。
#[derive(Debug, Default)]
pub struct ImportReport {
    /// 成功读完的文件数。
    pub files: usize,
    /// 写入长期库的不同比赛数。
    pub matches: usize,
    /// 读不了或写不进去的文件，以及原因。
    pub failed: Vec<(PathBuf, String)>,
}

/// 把若干份日志里的比赛导入长期库。重复导入不会产生重复记录。
///
/// `progress` 在开始处理每份文件前调用，参数是序号和路径。
pub fn import_logs(
    store: &mut Store,
    files: &[PathBuf],
    zone: LogZone,
    mut progress: impl FnMut(usize, &Path),
) -> ImportReport {
    let config = TrackerConfig::default();
    let mut report = ImportReport::default();
    let mut seen = HashSet::new();
    for (index, path) in files.iter().enumerate() {
        progress(index, path);
        let states = match matches_in_file(path, zone, &config) {
            Ok(states) => states,
            Err(err) => {
                report.failed.push((path.clone(), err.to_string()));
                continue;
            }
        };
        report.files += 1;
        for state in &states {
            match store.record_match(state) {
                Ok(()) => {
                    seen.insert(state.match_id);
                }
                Err(err) => report.failed.push((path.clone(), err.to_string())),
            }
        }
    }
    report.matches = seen.len();
    report
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use bax_core::{Phase, PlayerId, Team};

    use super::*;

    const LOG: &str = "[2026-11-16 18:52:05.318]\n\
Log: [NET_ROOM] Network lifecycle | Stage=Game | Action=Entered | RoomType=WaitingRoom | FightId=9000001 | Status=WaitClients\n\
\n\
[2026-11-16 18:52:52.324]\n\
Log: Start loading battle... map: Airport, scenario: - Airport_network\n\
\n\
[2026-11-16 18:53:00.733]\n\
Log: FID:9000001 v1.2.2.1 - Me (2026-11-17 02:52)\n\
\n\
[2026-11-16 18:53:01.978]\n\
Log: [NET_ROOM] Network lifecycle | Stage=Game | Action=Entered | RoomType=GameRoom | FightId=9000001 | Status=WaitClients\n\
\n\
[2026-11-16 18:53:01.979]\n\
Log: Player list: \n\
ID: 101, Name: Ash, Team: Alpha\n\
ID: 500, Name: Me, Team: Bravo\n\
\n\
[2026-11-16 18:53:33.861]\n\
Log: Scenario controller loaded\n\
\n\
[2026-11-16 19:06:20.100]\n\
Log: [NET_ROOM] Network traffic history | Cause=MatchEnded | FightId=9000001 | Samples=27\n\
\n\
[2026-11-16 19:10:00.000]\n\
Log: [NET_ROOM] Network lifecycle | Stage=Game | Action=Entered | RoomType=WaitingRoom | FightId=9000002 | Status=WaitClients\n\
\n";

    fn write_log(dir: &Path, name: &str, text: &str) -> PathBuf {
        let path = dir.join(name);
        let mut file = File::create(&path).expect("建文件");
        file.write_all(text.as_bytes()).expect("写入");
        path
    }

    const UTC: LogZone = LogZone::Fixed { offset_seconds: 0 };

    #[test]
    fn collects_the_final_state_of_each_match_in_order() {
        let dir = tempfile::tempdir().expect("临时目录");
        let path = write_log(dir.path(), "Gamelog__2026_11_16__18_51.log", LOG);
        let states = matches_in_file(&path, UTC, &TrackerConfig::default()).expect("读取");
        assert_eq!(
            states.iter().map(|s| s.match_id.get()).collect::<Vec<_>>(),
            vec![9_000_001, 9_000_002]
        );
        let first = &states[0];
        assert_eq!(first.phase, Phase::Ended);
        assert_eq!(first.self_id, Some(PlayerId(500)));
        assert_eq!(first.my_team(), Some(Team::Bravo));
        assert_eq!(first.roster.len(), 2);
        assert_eq!(states[1].phase, Phase::WaitingRoom);
    }

    #[test]
    fn importing_twice_records_each_match_once() {
        let dir = tempfile::tempdir().expect("临时目录");
        let a = write_log(dir.path(), "Gamelog__2026_11_16__18_51.log", LOG);
        let missing = dir.path().join("Gamelog__2026_11_16__19_00.log");
        let mut store = Store::open_in_memory().expect("打开长期库");

        let mut seen = Vec::new();
        let report = import_logs(&mut store, &[a.clone(), missing.clone()], UTC, |i, _| {
            seen.push(i)
        });
        assert_eq!(seen, vec![0, 1]);
        assert_eq!(report.files, 1);
        assert_eq!(report.matches, 2);
        assert_eq!(report.failed.len(), 1);
        assert_eq!(report.failed[0].0, missing);

        let again = import_logs(&mut store, &[a], UTC, |_, _| {});
        assert_eq!(again.matches, 2);
        assert_eq!(store.recent_matches(10).expect("读取").len(), 2);
        assert_eq!(
            store
                .encounters(PlayerId(101), None)
                .expect("统计")
                .as_opponent,
            1
        );
    }
}
