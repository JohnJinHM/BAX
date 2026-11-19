//! 战后归档：比赛结束后等 BATrace 收录战报，取到就存进长期库。
//!
//! 战报几乎是实时收录的（验证 3：结束后 2 分钟内都已取到），但偶尔取不到：站点延迟、宕机，
//! 或者比赛没打起来就结束了（部署阶段结束时有人没进来之类），这种比赛站点通常不收录。
//! 所以按从比赛结束算起的一串时间点各试一次，过了最后一个时间点就放弃，可以在界面上手动再试。
//! 没打起来的比赛用一张短得多的时间表。应用关着时结束的比赛，下次启动时立刻试一次，
//! 不等时间表里的下一个点。
//!
//! 归档在单独的任务里进行，用自己的数据库连接，不占用对局会话。待获取的比赛记在长期库里，
//! 应用在战报出来之前退出，下次启动接着等。每次只发一个请求，两次之间至少隔一小段时间，
//! 不给站点添负担。

use std::sync::Arc;
use std::time::Duration;

use bax_batrace::{ApiError, BatraceClient};
use bax_core::{BattleClock, ClockSource, MatchId, Timestamp};
use bax_store::{QueuedReport, Store};
use tokio::sync::{Notify, mpsc};

use crate::clock::Clock;
use crate::config::EngineConfig;
use crate::postmatch::{UNITS_KEY, report_meta};

/// 归档状态的变化，发给对局会话放进快照。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ReportEvent {
    pub pending: u32,
    /// 刚刚存进来的战报。
    pub archived: Option<MatchId>,
}

/// 归档任务的设置，从 [`EngineConfig`] 里取。
#[derive(Debug, Clone)]
pub(crate) struct ReportTiming {
    /// 从比赛结束算起的尝试时间点。
    pub schedule: Vec<Duration>,
    /// 没打起来的比赛用的时间点。
    pub unplayed_schedule: Vec<Duration>,
    /// 没有新消息时多久检查一次队列。
    pub poll: Duration,
    /// 两次请求之间至少隔多久。
    pub spacing: Duration,
    /// 启动时补上多久以内结束的比赛。
    pub catch_up: Duration,
    /// 单位目录多久刷新一次。
    pub catalog_max_age: Duration,
}

impl ReportTiming {
    pub(crate) fn from_config(config: &EngineConfig) -> ReportTiming {
        ReportTiming {
            schedule: config.report_schedule.clone(),
            unplayed_schedule: config.report_schedule_unplayed.clone(),
            poll: config.report_poll_interval,
            spacing: config.report_spacing,
            catch_up: config.report_catch_up,
            catalog_max_age: config.catalog_max_age,
        }
    }

    /// 按这场比赛打了多久选时间表。
    fn schedule_for(&self, battle_ms: Option<i64>) -> &[Duration] {
        if is_unplayed(battle_ms) {
            &self.unplayed_schedule
        } else {
            &self.schedule
        }
    }
}

/// 计时起点之后不到这么久就结束的比赛算没打起来。
const UNPLAYED_BATTLE_MS: i64 = 60_000;
/// 没打起来的比赛放弃时记下的原因。
const UNPLAYED_GAVE_UP: &str = "比赛没打起来就结束了，站点通常不收录这种比赛";

/// 被限流时至少等多久。
const RATE_LIMIT_PAUSE: Duration = Duration::from_secs(5 * 60);
/// 被拒绝（403、人机验证）时至少等多久。
const REJECTED_PAUSE: Duration = Duration::from_secs(30 * 60);
/// 单位目录取失败后多久再试。
const CATALOG_RETRY: Duration = Duration::from_secs(30 * 60);
/// 一轮最多处理几场。
const BATCH: u32 = 10;
/// 启动时最多补几场。
const CATCH_UP_LIMIT: u32 = 20;

pub(crate) struct ReportWorker {
    pub store: Store,
    pub client: BatraceClient,
    pub clock: Arc<dyn Clock>,
    pub timing: ReportTiming,
    pub notify: Arc<Notify>,
    pub events: mpsc::UnboundedSender<ReportEvent>,
}

impl ReportWorker {
    pub(crate) async fn run(mut self) {
        tracing::info!(
            pending = self.store.pending_reports().unwrap_or(0),
            "战后归档启动"
        );
        let mut catalog_retry_at: Option<Timestamp> = None;
        self.publish(None);
        loop {
            self.catch_up();
            let now = self.clock.now();
            let due = self.store.due_reports(now, BATCH).unwrap_or_else(|err| {
                tracing::warn!(error = %err, "读取待获取队列失败");
                Vec::new()
            });
            for item in due {
                if self.events.is_closed() {
                    return;
                }
                self.attempt(item).await;
                tokio::time::sleep(self.timing.spacing).await;
            }
            if catalog_retry_at.is_none_or(|at| at <= self.clock.now()) {
                catalog_retry_at = self.ensure_catalog().await;
            }

            let now = self.clock.now();
            let wait = match self.store.next_report_due().ok().flatten() {
                Some(next) => {
                    let ms = next.millis_since(now).max(0) as u64;
                    Duration::from_millis(ms).min(self.timing.poll)
                }
                None => self.timing.poll,
            };
            tokio::select! {
                _ = self.notify.notified() => {}
                _ = tokio::time::sleep(wait) => {}
                _ = self.events.closed() => break,
            }
        }
        tracing::info!("战后归档结束");
    }

    /// 把最近结束、还没有战报也不在队列里的比赛排进队列。
    fn catch_up(&mut self) {
        let now = self.clock.now();
        let since = now.plus_millis(-(self.timing.catch_up.as_millis() as i64));
        let found = match self.store.unreported_matches(since, CATCH_UP_LIMIT) {
            Ok(found) => found,
            Err(err) => {
                tracing::warn!(error = %err, "查找待归档比赛失败");
                return;
            }
        };
        let mut queued = 0;
        for (id, ended_at, battle_ms) in found {
            let next = first_attempt(self.timing.schedule_for(Some(battle_ms)), ended_at, now);
            match self.store.queue_report(id, ended_at, next) {
                Ok(true) => {
                    queued += 1;
                    tracing::info!(
                        match_id = %id,
                        next_in_secs = next.millis_since(now) / 1000,
                        unplayed = is_unplayed(Some(battle_ms)),
                        "等待战报"
                    );
                }
                Ok(false) => {}
                Err(err) => tracing::warn!(error = %err, match_id = %id, "排队等待战报失败"),
            }
        }
        if queued > 0 {
            self.publish(None);
        }
    }

    async fn attempt(&mut self, item: QueuedReport) {
        let id = item.match_id;
        let attempts = item.attempts + 1;
        let result = self.client.match_report(id).await;
        let now = self.clock.now();
        match result {
            Ok(fetched) => {
                let meta = report_meta(&fetched.data);
                if let Err(err) = self.store.save_report(id, &fetched.raw, meta, now) {
                    tracing::warn!(error = %err, match_id = %id, "保存战报失败");
                    return;
                }
                // 战报的开始时间最可信，修正这场比赛的计时基准
                if let Some(start) = meta.start_time {
                    let clock = BattleClock {
                        start_at: start,
                        source: ClockSource::Report,
                    };
                    if let Err(err) = self.store.set_match_clock(id, clock) {
                        tracing::warn!(error = %err, match_id = %id, "修正计时基准失败");
                    }
                }
                tracing::info!(
                    match_id = %id,
                    attempts,
                    after_end_secs = now.millis_since(item.queued_at) / 1000,
                    players = fetched.data.data.len(),
                    "战报已归档"
                );
                self.publish(Some(id));
            }
            Err(err) => {
                let unplayed = is_unplayed(item.battle_ms);
                let schedule = self.timing.schedule_for(item.battle_ms);
                let (counted, next) = retry_plan(schedule, &item, now, &err);
                let reason = match (&err, next) {
                    (ApiError::NotFound, None) if unplayed => UNPLAYED_GAVE_UP.to_owned(),
                    _ => err.to_string(),
                };
                if let Err(store_err) = self
                    .store
                    .report_attempt_failed(id, counted, next, &reason, now)
                {
                    tracing::warn!(error = %store_err, match_id = %id, "记录战报重试失败");
                }
                tracing::info!(
                    match_id = %id,
                    attempts = counted,
                    error = %err,
                    gave_up = next.is_none(),
                    unplayed,
                    next_in_secs = next.map(|n| n.millis_since(now) / 1000),
                    "还没有取到战报"
                );
                self.publish(None);
            }
        }
    }

    /// 有战报而单位目录缺失或过期时取一次。返回下一次允许尝试的时刻。
    async fn ensure_catalog(&mut self) -> Option<Timestamp> {
        let now = self.clock.now();
        let has_reports = self.store.report_count().unwrap_or(0) > 0;
        let fresh = self
            .store
            .reference(UNITS_KEY)
            .ok()
            .flatten()
            .is_some_and(|c| {
                now.millis_since(c.fetched_at) < self.timing.catalog_max_age.as_millis() as i64
            });
        if !has_reports || fresh {
            return None;
        }
        match self.client.units().await {
            Ok(fetched) => {
                let count = fetched.data.units.len();
                if let Err(err) = self.store.set_reference(UNITS_KEY, &fetched.raw, now) {
                    tracing::warn!(error = %err, "保存单位目录失败");
                }
                tracing::info!(units = count, "单位目录已更新");
                // 目录影响已有战报的解读，让界面重新读取
                self.publish(None);
                None
            }
            Err(err) => {
                tracing::info!(error = %err, "单位目录获取失败");
                Some(now.plus_millis(CATALOG_RETRY.as_millis() as i64))
            }
        }
    }

    fn publish(&self, archived: Option<MatchId>) {
        let pending = self.store.pending_reports().unwrap_or(0);
        let _ = self.events.send(ReportEvent { pending, archived });
    }
}

fn is_unplayed(battle_ms: Option<i64>) -> bool {
    battle_ms.is_some_and(|ms| ms < UNPLAYED_BATTLE_MS)
}

/// 刚排进队列的比赛第一次什么时候试：时间表的第一个点；已经过了（应用关着时结束的比赛）就马上试。
/// 战报几乎实时收录，没有理由再等到后面的点。
fn first_attempt(schedule: &[Duration], ended_at: Timestamp, now: Timestamp) -> Timestamp {
    schedule
        .first()
        .map(|d| ended_at.plus_millis(d.as_millis() as i64))
        .filter(|at| *at > now)
        .unwrap_or(now)
}

/// 时间表里在 `now` 之后的第一个时间点；全都过了时为 `None`。
fn next_after(schedule: &[Duration], queued_at: Timestamp, now: Timestamp) -> Option<Timestamp> {
    schedule
        .iter()
        .map(|d| queued_at.plus_millis(d.as_millis() as i64))
        .find(|at| *at > now)
}

/// 一次失败之后：记几次、下一次什么时候试（`None` 为放弃）。
fn retry_plan(
    schedule: &[Duration],
    item: &QueuedReport,
    now: Timestamp,
    err: &ApiError,
) -> (u32, Option<Timestamp>) {
    let after = |pause: Duration| now.plus_millis(pause.as_millis() as i64);
    match err {
        // 站点要求放慢：这次不算，过一会儿原样再试
        ApiError::RateLimited { retry_after } => (
            item.attempts,
            Some(after(retry_after.unwrap_or_default().max(RATE_LIMIT_PAUSE))),
        ),
        // 被拒绝或要求人机验证：马上再试也没用，至少隔半小时
        ApiError::Rejected { .. } | ApiError::Challenge => (
            item.attempts + 1,
            next_after(schedule, item.queued_at, now).map(|n| n.max(after(REJECTED_PAUSE))),
        ),
        _ => (item.attempts + 1, next_after(schedule, item.queued_at, now)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mins(m: u64) -> Duration {
        Duration::from_secs(m * 60)
    }

    fn at_min(m: i64) -> Timestamp {
        Timestamp::from_secs(m * 60)
    }

    fn item(attempts: u32) -> QueuedReport {
        QueuedReport {
            match_id: MatchId(1),
            queued_at: at_min(0),
            attempts,
            next_at: None,
            last_error: None,
            battle_ms: Some(20 * 60_000),
        }
    }

    #[test]
    fn retries_follow_the_schedule_and_skip_passed_points() {
        let schedule = [mins(2), mins(5), mins(10)];
        let (n, next) = retry_plan(&schedule, &item(0), at_min(2), &ApiError::NotFound);
        assert_eq!((n, next), (1, Some(at_min(5))));
        // 应用关着错过了几个时间点：直接跳到下一个
        let (n, next) = retry_plan(&schedule, &item(1), at_min(7), &ApiError::Timeout);
        assert_eq!((n, next), (2, Some(at_min(10))));
        let (n, next) = retry_plan(&schedule, &item(2), at_min(10), &ApiError::NotFound);
        assert_eq!((n, next), (3, None), "过了最后一个时间点就放弃");
    }

    #[test]
    fn rate_limits_do_not_count_as_attempts() {
        let schedule = [mins(2), mins(5)];
        let err = ApiError::RateLimited {
            retry_after: Some(Duration::from_secs(10)),
        };
        let (n, next) = retry_plan(&schedule, &item(1), at_min(5), &err);
        assert_eq!((n, next), (1, Some(at_min(10))));
    }

    #[test]
    fn rejections_wait_at_least_half_an_hour() {
        let schedule = [mins(2), mins(5), mins(60)];
        let (n, next) = retry_plan(&schedule, &item(0), at_min(2), &ApiError::Challenge);
        assert_eq!((n, next), (1, Some(at_min(32))));
    }

    #[test]
    fn the_next_point_skips_passed_ones() {
        let schedule = [mins(2), mins(5), mins(10)];
        assert_eq!(next_after(&schedule, at_min(0), at_min(1)), Some(at_min(2)));
        assert_eq!(
            next_after(&schedule, at_min(0), at_min(6)),
            Some(at_min(10))
        );
        assert_eq!(next_after(&schedule, at_min(0), at_min(11)), None);
    }

    #[test]
    fn a_match_that_ended_while_the_app_was_closed_is_tried_at_once() {
        let schedule = [mins(1), mins(60), mins(1440)];
        // 刚结束：等第一个点
        assert_eq!(first_attempt(&schedule, at_min(0), at_min(0)), at_min(1));
        // 结束 5 小时后才排进来：马上试，不等到 24 小时那个点
        assert_eq!(
            first_attempt(&schedule, at_min(0), at_min(300)),
            at_min(300)
        );
        // 马上试也没取到：接着按时间表
        let (_, next) = retry_plan(&schedule, &item(0), at_min(300), &ApiError::NotFound);
        assert_eq!(next, Some(at_min(1440)));
    }

    #[test]
    fn matches_that_never_got_going_use_the_short_schedule() {
        let timing = ReportTiming {
            schedule: vec![mins(1), mins(60)],
            unplayed_schedule: vec![mins(1), mins(5)],
            poll: mins(1),
            spacing: mins(1),
            catch_up: mins(1),
            catalog_max_age: mins(1),
        };
        assert_eq!(timing.schedule_for(Some(48)), &[mins(1), mins(5)]);
        assert_eq!(timing.schedule_for(Some(20 * 60_000)), &[mins(1), mins(60)]);
        assert_eq!(
            timing.schedule_for(None),
            &[mins(1), mins(60)],
            "不知道打了多久时照常等"
        );
    }
}
