use bax_core::{
    BattleClock, ClockSource, LogEvent, PlayerId, RoomKind, RoomStatus, RosterEntry, Tracker,
};

use super::*;

fn t(secs: i64) -> Timestamp {
    Timestamp::from_secs(secs)
}

#[test]
fn cancelled_matches_block_old_queues_manual_requests_and_catch_up() {
    let mut store = Store::open_in_memory().expect("库");
    let id = MatchId(1);
    record(&mut store, 1, 100, true, true);
    store.queue_report(id, t(100), t(100)).expect("旧队列");
    store
        .conn
        .execute(
            "UPDATE matches SET ended_at = start_at + 49 WHERE id = 1",
            [],
        )
        .expect("准备结束即取消");
    assert!(store.report_cancelled(id).expect("取消"));
    assert_eq!(
        store.match_history(10, 0).expect("历史")[0].report,
        ReportState::Cancelled
    );
    assert!(!store.request_report(id, t(200)).expect("手动"));
    assert!(!store.queue_report(id, t(200), t(200)).expect("自动"));
    assert!(store.due_reports(t(10000), 10).expect("到期").is_empty());
    assert_eq!(store.pending_reports().expect("计数"), 0);
    assert_eq!(store.next_report_due().expect("调度"), None);
    store
        .conn
        .execute("DELETE FROM report_queue", [])
        .expect("去掉旧队列");
    assert!(store.unreported_matches(t(0), 10).expect("补录").is_empty());
    // 已有官方战报是更强的证据，不因本地时钟误差屏蔽它。
    store
        .save_report(id, "{}", meta(Team::Alpha), t(200))
        .expect("官方战报");
    assert!(!store.report_cancelled(id).expect("有战报"));
    assert!(matches!(
        store.match_history(10, 0).expect("历史")[0].report,
        ReportState::Ready { .. }
    ));
}

#[test]
fn cancellation_requires_a_normal_start_and_an_end_signal() {
    let mut store = Store::open_in_memory().expect("库");
    record(&mut store, 1, 100, true, true);
    for (source, cause, delta, expected) in [
        ("logEstimate", "matchEnded", -30_000, true),
        ("logEstimate", "matchEnded", 2_000, true),
        ("logEstimate", "matchEnded", 2_001, false),
        ("logEstimate", "connectionLost", 0, false),
        ("manual", "matchEnded", 0, false),
        ("report", "matchEnded", 0, false),
        ("logEstimate", "matchEnded", -61_000, false),
    ] {
        store
            .conn
            .execute(
                "UPDATE matches SET start_source = ?1, end_cause = ?2, ended_at = start_at + ?3",
                params![source, cause, delta],
            )
            .expect("情形");
        assert_eq!(store.report_cancelled(MatchId(1)).expect("分类"), expected);
    }
    store
        .conn
        .execute(
            "UPDATE matches SET start_at = NULL, start_source = NULL",
            [],
        )
        .expect("重连无起点");
    assert!(!store.report_cancelled(MatchId(1)).expect("未知时长"));
}

/// 记录一场比赛：本机（500，名字 Me）在 Bravo，对面一人。`battle` 为假时没打起来（没有计时基准）。
fn record(store: &mut Store, match_id: u64, at: i64, battle: bool, ended: bool) {
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
    tracker.apply(
        t(at + 40),
        &LogEvent::PlayerList {
            players: vec![
                RosterEntry {
                    id: PlayerId(500),
                    name: Some("Me".into()),
                    team: Team::Bravo,
                },
                RosterEntry {
                    id: PlayerId(101),
                    name: Some("Ash".into()),
                    team: Team::Alpha,
                },
            ],
        },
    );
    if battle {
        tracker.apply(t(at + 70), &LogEvent::ScenarioLoaded);
    }
    if ended {
        tracker.apply(t(at + 1330), &LogEvent::MatchEnded { match_id: id });
    }
    store
        .record_match(tracker.current().expect("有比赛"))
        .expect("记录比赛");
}

fn meta(winner: Team) -> ReportMeta {
    ReportMeta {
        start_time: Some(t(1130)),
        duration_secs: Some(1200),
        map_id: Some(7),
        winner: Some(winner),
    }
}

#[test]
fn a_queued_match_waits_until_its_time() {
    let mut store = Store::open_in_memory().expect("打开长期库");
    let id = MatchId(1);
    assert!(store.queue_report(id, t(100), t(220)).expect("排队"));
    assert!(
        !store.queue_report(id, t(100), t(220)).expect("再排"),
        "重复排队不改变已有的记录"
    );
    assert_eq!(store.pending_reports().expect("计数"), 1);
    assert_eq!(store.next_report_due().expect("下一次"), Some(t(220)));
    assert!(store.due_reports(t(219), 10).expect("未到").is_empty());

    let due = store.due_reports(t(220), 10).expect("到了");
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].attempts, 0);
    assert_eq!(due[0].queued_at, t(100));

    store
        .report_attempt_failed(id, 1, Some(t(400)), "站点没有收录这条数据", t(220))
        .expect("记失败");
    let queued = store.queued_report(id).expect("读").expect("在队列里");
    assert_eq!(queued.attempts, 1);
    assert_eq!(queued.next_at, Some(t(400)));
    assert_eq!(queued.last_error.as_deref(), Some("站点没有收录这条数据"));
}

#[test]
fn queued_matches_carry_how_long_the_battle_lasted() {
    let mut store = Store::open_in_memory().expect("打开长期库");
    record(&mut store, 1, 100, true, true);
    let (id, ended_at, _) = store.unreported_matches(t(0), 10).expect("查")[0];
    store.queue_report(id, ended_at, ended_at).expect("排队");
    // 计时基准以战报为准，固定在结束前 5 分钟
    let clock = BattleClock {
        start_at: ended_at.plus_millis(-300_000),
        source: ClockSource::Report,
    };
    store.set_match_clock(id, clock).expect("修正计时基准");
    let queued = store.queued_report(id).expect("读").expect("在队列里");
    assert_eq!(queued.battle_ms, Some(300_000));
    assert_eq!(
        store.due_reports(ended_at, 10).expect("到了")[0].battle_ms,
        queued.battle_ms
    );

    // 长期库里没有这场比赛（不该发生，但不能因此出错）
    store
        .queue_report(MatchId(2), t(100), t(100))
        .expect("排队");
    assert_eq!(
        store
            .queued_report(MatchId(2))
            .expect("读")
            .expect("在")
            .battle_ms,
        None
    );
}

#[test]
fn the_archive_remembers_when_the_report_was_still_missing() {
    let mut store = Store::open_in_memory().expect("打开长期库");
    let id = MatchId(1);
    store.queue_report(id, t(100), t(220)).expect("排队");
    store
        .report_attempt_failed(id, 2, Some(t(700)), "站点没有收录这条数据", t(400))
        .expect("记失败");
    store
        .save_report(id, "{}", meta(Team::Alpha), t(700))
        .expect("保存");
    let timing = store.report_timing(id).expect("读").expect("有");
    assert_eq!(timing.queued_at, Some(t(100)));
    assert_eq!(timing.attempts, 2);
    assert_eq!(timing.last_miss_at, Some(t(400)));
    assert_eq!(timing.fetched_at, t(700));

    // 不经过队列再存一次（例如手动重取）：时间记录保留
    store
        .save_report(id, "{}", meta(Team::Alpha), t(900))
        .expect("覆盖");
    let timing = store.report_timing(id).expect("读").expect("有");
    assert_eq!(timing.last_miss_at, Some(t(400)));
    assert_eq!(timing.fetched_at, t(900));
}

#[test]
fn giving_up_leaves_the_match_out_of_the_pending_count() {
    let mut store = Store::open_in_memory().expect("打开长期库");
    let id = MatchId(1);
    store.queue_report(id, t(100), t(220)).expect("排队");
    store
        .report_attempt_failed(id, 9, None, "请求超时", t(900))
        .expect("放弃");
    assert_eq!(store.pending_reports().expect("计数"), 0);
    assert_eq!(store.next_report_due().expect("下一次"), None);

    // 手动请求重新开始
    assert!(store.request_report(id, t(1000)).expect("手动"));
    let queued = store.queued_report(id).expect("读").expect("在队列里");
    assert_eq!(queued.attempts, 0);
    assert_eq!(queued.next_at, Some(t(1000)));
    assert_eq!(queued.queued_at, t(1000));
}

#[test]
fn saving_a_report_removes_it_from_the_queue() {
    let mut store = Store::open_in_memory().expect("打开长期库");
    let id = MatchId(1);
    store.queue_report(id, t(100), t(220)).expect("排队");
    store
        .save_report(id, "{\"x\":1}", meta(Team::Alpha), t(300))
        .expect("保存");
    assert!(store.queued_report(id).expect("读").is_none());
    let timing = store.report_timing(id).expect("读").expect("有");
    assert_eq!(timing.queued_at, Some(t(100)));
    assert_eq!(timing.attempts, 0);
    assert_eq!(timing.last_miss_at, None);
    let stored = store.report(id).expect("读").expect("有战报");
    assert_eq!(stored.raw, "{\"x\":1}");
    assert_eq!(stored.fetched_at, t(300));
    assert!(
        !store.queue_report(id, t(400), t(500)).expect("排队"),
        "有战报就不再排队"
    );
    assert!(!store.request_report(id, t(400)).expect("手动"));

    // 再存一次覆盖旧的
    store
        .save_report(id, "{\"x\":2}", meta(Team::Alpha), t(600))
        .expect("覆盖");
    assert_eq!(store.report(id).expect("读").expect("有").raw, "{\"x\":2}");
}

#[test]
fn unreported_matches_only_include_finished_battles() {
    let mut store = Store::open_in_memory().expect("打开长期库");
    record(&mut store, 1, 1000, true, true);
    record(&mut store, 2, 3000, false, true); // 没打起来
    record(&mut store, 3, 5000, true, false); // 没结束
    record(&mut store, 4, 7000, true, true);
    record(&mut store, 5, 9000, true, true);
    store
        .save_report(MatchId(4), "{}", meta(Team::Bravo), t(9000))
        .expect("已有战报");
    store
        .queue_report(MatchId(5), t(10330), t(10450))
        .expect("已在队列");

    let found = store.unreported_matches(t(0), 10).expect("查");
    assert_eq!(found, vec![(MatchId(1), t(2330), 1_200_000)]);
    assert!(
        store
            .unreported_matches(t(2331), 10)
            .expect("查")
            .is_empty(),
        "太早结束的不算"
    );
}

#[test]
fn history_rows_carry_report_state_and_result() {
    let mut store = Store::open_in_memory().expect("打开长期库");
    record(&mut store, 1, 1000, true, true);
    record(&mut store, 2, 3000, true, true);
    record(&mut store, 3, 5000, true, true);
    record(&mut store, 4, 7000, true, true);
    store
        .save_report(MatchId(1), "{}", meta(Team::Bravo), t(2400))
        .expect("战报");
    store
        .queue_report(MatchId(2), t(4330), t(4450))
        .expect("排队");
    store
        .queue_report(MatchId(3), t(6330), t(6450))
        .expect("排队");
    store
        .report_attempt_failed(MatchId(3), 12, None, "请求超时", t(9000))
        .expect("放弃");

    let rows = store.match_history(10, 0).expect("列表");
    assert_eq!(
        rows.iter().map(|r| r.match_id.get()).collect::<Vec<_>>(),
        vec![4, 3, 2, 1]
    );
    let one = &rows[3];
    assert_eq!(one.won, Some(true), "本方 Bravo，胜方 Bravo");
    assert_eq!(one.players, 2);
    assert_eq!(one.duration_ms, Some(1_200_000), "有战报时用战报的时长");
    assert_eq!(one.map.as_deref(), Some("Map1"));
    assert_eq!(
        one.report,
        ReportState::Ready {
            fetched_at: t(2400)
        }
    );
    assert_eq!(
        rows[2].report,
        ReportState::Pending {
            attempts: 0,
            next_at: t(4450)
        }
    );
    assert_eq!(
        rows[1].report,
        ReportState::GaveUp {
            attempts: 12,
            last_error: Some("请求超时".into())
        }
    );
    assert_eq!(rows[0].report, ReportState::None);
    assert_eq!(rows[0].won, None);

    let page = store.match_history(2, 2).expect("翻页");
    assert_eq!(page[0].match_id, MatchId(2));
}

#[test]
fn reference_data_round_trips() {
    let mut store = Store::open_in_memory().expect("打开长期库");
    assert!(store.reference("units").expect("读").is_none());
    store.set_reference("units", "[1]", t(10)).expect("写");
    store.set_reference("units", "[2]", t(20)).expect("改");
    let stored = store.reference("units").expect("读").expect("有");
    assert_eq!(stored.raw, "[2]");
    assert_eq!(stored.fetched_at, t(20));
}
