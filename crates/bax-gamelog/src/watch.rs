//! 把读取与解析接起来，并区分补读与实时。
//!
//! BAX 启动时游戏可能已经在运行，所以先把最新一份日志从头读一遍，恢复出当前状态。
//! 这部分事件标为 [`Origin::Backfill`]：它们描述的是已经发生的事，
//! 调用方据此恢复状态，但不应该为它们补发提醒或重新发起一轮请求。
//! 补读结束后的事件标为 [`Origin::Live`]。

use std::io;
use std::path::{Path, PathBuf};

use crate::parse::{LogParser, TimedEvent};
use crate::tail::{LogTailer, TailItem};

/// 事件是补读出来的，还是实时发生的。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Origin {
    /// 启动时从已有日志里读出的历史事件。
    Backfill,
    /// 运行期间游戏新写入的事件。
    Live,
}

/// [`LogWatcher::poll`] 交出的内容。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatchItem {
    /// 开始读一份日志文件。
    FileOpened {
        path: PathBuf,
        origin: Origin,
    },
    /// 当前文件被截断并从头重读。之前从这份文件得到的状态应当作废。
    Truncated {
        path: PathBuf,
    },
    Event {
        origin: Origin,
        event: TimedEvent,
    },
    /// 启动时的补读已经完成。只出现一次。
    BackfillDone,
}

/// 日志监视器。
#[derive(Debug)]
pub struct LogWatcher {
    tailer: LogTailer,
    parser: LogParser,
    scratch: Vec<TimedEvent>,
    backfill_done: bool,
}

impl LogWatcher {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        LogWatcher::with_tailer(LogTailer::new(dir))
    }

    pub fn with_tailer(tailer: LogTailer) -> Self {
        LogWatcher {
            tailer,
            parser: LogParser::new(),
            scratch: Vec::new(),
            backfill_done: false,
        }
    }

    pub fn dir(&self) -> &Path {
        self.tailer.dir()
    }

    pub fn current_file(&self) -> Option<&Path> {
        self.tailer.current_file()
    }

    /// 读出新内容并解析。目录不可读时返回错误，可以稍后再试。
    pub fn poll(&mut self, sink: &mut dyn FnMut(WatchItem)) -> io::Result<()> {
        let origin = if self.backfill_done {
            Origin::Live
        } else {
            Origin::Backfill
        };
        let LogWatcher {
            tailer,
            parser,
            scratch,
            ..
        } = self;

        let read = tailer.poll(&mut |item| match item {
            TailItem::FileOpened { path } => {
                // 上一份文件里没收完的名单到此为止
                parser.flush(scratch);
                drain(scratch, origin, sink);
                parser.reset();
                sink(WatchItem::FileOpened {
                    path: path.to_path_buf(),
                    origin,
                });
            }
            TailItem::Truncated { path } => {
                parser.reset();
                scratch.clear();
                sink(WatchItem::Truncated {
                    path: path.to_path_buf(),
                });
            }
            TailItem::Line(line) => {
                parser.push_line(line, scratch);
                if !scratch.is_empty() {
                    drain(scratch, origin, sink);
                }
            }
        })?;

        // 名单以空行结束。补读到文件末尾、或者这一轮没有任何新内容时，
        // 不再等那个空行，把已经读到的名单交出去
        if (read == 0 || !self.backfill_done) && self.parser.has_pending() {
            self.parser.publish_pending(&mut self.scratch);
            drain(&mut self.scratch, origin, sink);
        }
        if !self.backfill_done {
            self.backfill_done = true;
            sink(WatchItem::BackfillDone);
        }
        Ok(())
    }
}

fn drain(scratch: &mut Vec<TimedEvent>, origin: Origin, sink: &mut dyn FnMut(WatchItem)) {
    for event in scratch.drain(..) {
        sink(WatchItem::Event { origin, event });
    }
}

#[cfg(test)]
mod tests {
    use std::fs::OpenOptions;
    use std::io::Write;
    use std::time::Duration;

    use bax_core::{LogEvent, MatchId, RoomKind};

    use super::*;

    fn append(path: &Path, text: &str) {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .expect("打开文件");
        file.write_all(text.as_bytes()).expect("写入");
    }

    fn watcher(dir: &Path) -> LogWatcher {
        LogWatcher::with_tailer(LogTailer::new(dir).with_rescan_interval(Duration::ZERO))
    }

    fn poll(w: &mut LogWatcher) -> Vec<WatchItem> {
        let mut items = Vec::new();
        w.poll(&mut |item| items.push(item)).expect("读取应当成功");
        items
    }

    /// 只留下事件及其来源，方便断言。
    fn events(items: &[WatchItem]) -> Vec<(Origin, &LogEvent)> {
        items
            .iter()
            .filter_map(|i| match i {
                WatchItem::Event { origin, event } => Some((*origin, &event.event)),
                _ => None,
            })
            .collect()
    }

    const ENTERED: &str =
        "[2026-11-16 18:52:05.319]\nLog: [NET_ROOM] WaitingRoom entered, fight id: 9000001\n\n";
    const LOADED: &str = "[2026-11-16 18:53:33.861]\nLog: Scenario controller loaded\n\n";

    fn entered_event() -> LogEvent {
        LogEvent::RoomEntered {
            room: RoomKind::WaitingRoom,
            match_id: MatchId(9_000_001),
            status: None,
        }
    }

    #[test]
    fn existing_content_is_backfill_and_later_content_is_live() {
        let dir = tempfile::tempdir().expect("临时目录");
        let path = dir.path().join("Gamelog__2026_11_16__18_51.log");
        append(&path, ENTERED);

        let mut w = watcher(dir.path());
        let first = poll(&mut w);
        assert!(matches!(
            first.first(),
            Some(WatchItem::FileOpened {
                origin: Origin::Backfill,
                ..
            })
        ));
        assert_eq!(events(&first), vec![(Origin::Backfill, &entered_event())]);
        assert_eq!(first.last(), Some(&WatchItem::BackfillDone));

        append(&path, LOADED);
        let second = poll(&mut w);
        assert_eq!(
            events(&second),
            vec![(Origin::Live, &LogEvent::ScenarioLoaded)]
        );
        assert!(
            !second.contains(&WatchItem::BackfillDone),
            "补读完成只报告一次"
        );
    }

    #[test]
    fn a_log_created_after_startup_is_live_from_its_first_line() {
        let dir = tempfile::tempdir().expect("临时目录");
        let mut w = watcher(dir.path());
        assert_eq!(poll(&mut w), vec![WatchItem::BackfillDone]);

        append(&dir.path().join("Gamelog__2026_11_16__18_51.log"), ENTERED);
        let items = poll(&mut w);
        assert!(matches!(
            items.first(),
            Some(WatchItem::FileOpened {
                origin: Origin::Live,
                ..
            })
        ));
        assert_eq!(events(&items), vec![(Origin::Live, &entered_event())]);
    }

    #[test]
    fn an_unterminated_player_list_is_released_at_the_end_of_backfill() {
        let dir = tempfile::tempdir().expect("临时目录");
        let path = dir.path().join("Gamelog__2026_11_16__18_51.log");
        append(
            &path,
            "[2026-11-16 18:53:01.979]\nLog: Player list: \nID: 1, Name: A, Team: Alpha\n",
        );

        let mut w = watcher(dir.path());
        let items = poll(&mut w);
        let got = events(&items);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].0, Origin::Backfill);
        assert!(matches!(got[0].1, LogEvent::PlayerList { players } if players.len() == 1));
        assert_eq!(items.last(), Some(&WatchItem::BackfillDone));
    }

    #[test]
    fn a_live_player_list_waits_one_quiet_poll_before_being_released() {
        let dir = tempfile::tempdir().expect("临时目录");
        let path = dir.path().join("Gamelog__2026_11_16__18_51.log");
        append(&path, ENTERED);
        let mut w = watcher(dir.path());
        poll(&mut w);

        // 名单还在写：这一轮读到了内容，先不交出
        append(
            &path,
            "[2026-11-16 18:53:01.979]\nLog: Player list: \nID: 1, Name: A, Team: Alpha\n",
        );
        assert!(events(&poll(&mut w)).is_empty());

        // 又来了一行
        append(&path, "ID: 2, Name: B, Team: Bravo\n");
        assert!(events(&poll(&mut w)).is_empty());

        // 一轮没有新内容：不再等结尾的空行
        let items = poll(&mut w);
        let got = events(&items);
        assert_eq!(got.len(), 1);
        assert!(matches!(got[0].1, LogEvent::PlayerList { players } if players.len() == 2));
        assert_eq!(got[0].0, Origin::Live);
    }

    #[test]
    fn player_list_continues_after_idle_publication_and_backfill() {
        for during_backfill in [false, true] {
            let dir = tempfile::tempdir().expect("临时目录");
            let path = dir.path().join("Gamelog__2026_11_16__18_51.log");
            append(&path, ENTERED);
            let mut w = watcher(dir.path());
            if !during_backfill {
                poll(&mut w);
            }
            append(
                &path,
                "[2026-11-16 18:53:01.979]\nLog: Player list:\nID: 1, Name: Aster, Team: Alpha\n",
            );
            let first = poll(&mut w);
            let partial = if during_backfill { first } else { poll(&mut w) };
            assert!(events(&partial).iter().any(
                |(_, event)| matches!(event, LogEvent::PlayerList { players } if players.len() == 1)
            ));
            assert!(events(&poll(&mut w)).is_empty(), "空闲期间不重复发布");
            append(&path, "ID: 2, Name: Birch, Team: Bravo\n\n");
            let complete = poll(&mut w);
            assert!(
                matches!(events(&complete)[0].1, LogEvent::PlayerList { players } if players.len() == 2)
            );
            assert_eq!(events(&complete)[0].0, Origin::Live);
            append(&path, LOADED);
            assert_eq!(
                events(&poll(&mut w)),
                vec![(Origin::Live, &LogEvent::ScenarioLoaded)]
            );
        }
    }

    #[test]
    fn truncation_is_reported_and_parsing_restarts() {
        let dir = tempfile::tempdir().expect("临时目录");
        let path = dir.path().join("Gamelog__2026_11_16__18_51.log");
        append(&path, ENTERED);
        append(&path, LOADED);
        let mut w = watcher(dir.path());
        poll(&mut w);

        std::fs::write(&path, LOADED).expect("重写文件");
        let items = poll(&mut w);
        assert!(matches!(items.first(), Some(WatchItem::Truncated { .. })));
        assert_eq!(
            events(&items),
            vec![(Origin::Live, &LogEvent::ScenarioLoaded)]
        );
    }

    #[test]
    fn switching_files_keeps_events_in_order() {
        let dir = tempfile::tempdir().expect("临时目录");
        let first = dir.path().join("Gamelog__2026_11_16__18_31.log");
        append(&first, ENTERED);
        let mut w = watcher(dir.path());
        poll(&mut w);

        append(&first, LOADED);
        append(
            &dir.path().join("Gamelog__2026_11_16__18_51.log"),
            "[2026-11-16 18:51:40.000]\nLog: Start reconnect to battle\n\n",
        );
        let items = poll(&mut w);
        assert_eq!(
            events(&items),
            vec![
                (Origin::Live, &LogEvent::ScenarioLoaded),
                (Origin::Live, &LogEvent::ReconnectStarted),
            ]
        );
        let opened_at = items
            .iter()
            .position(|i| matches!(i, WatchItem::FileOpened { .. }))
            .expect("应当报告打开了新文件");
        assert_eq!(opened_at, 1, "新文件在旧文件的最后一个事件之后打开");
    }
}
