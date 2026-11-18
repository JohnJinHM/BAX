//! 计时提醒。
//!
//! 提醒规则写的是“正式开局后第几秒”。到点之后一小段时间内为“已提醒”；
//! 过了这段时间才第一次检查到的（例如对局中途才打开 BAX，或者手动校准把开局时刻往前拨），
//! 记为“已错过”，不补发。每条规则在每场比赛里只有一个结果，由调用方保存，重启后不会重复提醒。

use serde::{Deserialize, Serialize};

use crate::lifecycle::BattleClock;
use crate::time::Timestamp;

/// 一条提醒规则。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ReminderRule {
    /// 稳定的编号，用来记录每场比赛里这条规则是否已经提醒过。改时间或文字不应换编号。
    pub id: u32,
    /// 正式开局后多少秒提醒。
    pub at_secs: u32,
    pub label: String,
    pub enabled: bool,
}

/// 默认没有提醒。计时提醒是可选功能，由用户自己添加。
pub fn default_rules() -> Vec<ReminderRule> {
    Vec::new()
}

/// 规则在一场比赛里的结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum ReminderOutcome {
    Fired,
    Missed,
}

/// 到点后多长时间内仍算按时提醒，毫秒。
pub const DEFAULT_GRACE_MS: i64 = 30_000;

/// 规则检查不通过的原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleError {
    DuplicateId(u32),
    /// 提醒时间超过 4 小时，多半是输错了。
    TooLate(u32),
    EmptyLabel(u32),
}

impl std::fmt::Display for RuleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RuleError::DuplicateId(id) => write!(f, "提醒编号 {id} 重复"),
            RuleError::TooLate(id) => write!(f, "提醒 {id} 的时间超过 4 小时"),
            RuleError::EmptyLabel(id) => write!(f, "提醒 {id} 没有文字"),
        }
    }
}

impl std::error::Error for RuleError {}

/// 检查一组规则。
pub fn validate(rules: &[ReminderRule]) -> Result<(), RuleError> {
    let mut seen = std::collections::HashSet::new();
    for rule in rules {
        if !seen.insert(rule.id) {
            return Err(RuleError::DuplicateId(rule.id));
        }
        if rule.at_secs > 4 * 3600 {
            return Err(RuleError::TooLate(rule.id));
        }
        if rule.label.trim().is_empty() {
            return Err(RuleError::EmptyLabel(rule.id));
        }
    }
    Ok(())
}

/// 规则到点的时刻。
pub fn fire_at(rule: &ReminderRule, clock: BattleClock) -> Timestamp {
    clock
        .start_at
        .plus_millis(i64::from(rule.at_secs).saturating_mul(1000))
}

/// 找出在 `now` 这一刻有了结果的规则。
///
/// `settled` 判断某条规则在这场比赛里是否已经有结果；有结果的不再处理。
/// 调用方负责只在比赛进行中（有计时基准、尚未结束）时调用。
pub fn due(
    rules: &[ReminderRule],
    clock: BattleClock,
    now: Timestamp,
    grace_ms: i64,
    settled: impl Fn(u32) -> bool,
) -> Vec<(u32, ReminderOutcome)> {
    rules
        .iter()
        .filter(|rule| rule.enabled && !settled(rule.id))
        .filter_map(|rule| {
            let late = now.millis_since(fire_at(rule, clock));
            if late < 0 {
                None
            } else if late <= grace_ms {
                Some((rule.id, ReminderOutcome::Fired))
            } else {
                Some((rule.id, ReminderOutcome::Missed))
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lifecycle::ClockSource;

    fn clock() -> BattleClock {
        BattleClock {
            start_at: Timestamp::from_secs(1000),
            source: ClockSource::LogEstimate,
        }
    }

    fn rule(id: u32, at_secs: u32) -> ReminderRule {
        ReminderRule {
            id,
            at_secs,
            label: format!("规则 {id}"),
            enabled: true,
        }
    }

    #[test]
    fn fires_on_time_and_misses_when_first_seen_too_late() {
        let rules = [rule(1, 600), rule(2, 900)];
        let none = |_: u32| false;
        assert!(
            due(
                &rules,
                clock(),
                Timestamp::from_secs(1599),
                DEFAULT_GRACE_MS,
                none
            )
            .is_empty()
        );
        assert_eq!(
            due(
                &rules,
                clock(),
                Timestamp::from_secs(1600),
                DEFAULT_GRACE_MS,
                none
            ),
            vec![(1, ReminderOutcome::Fired)]
        );
        assert_eq!(
            due(
                &rules,
                clock(),
                Timestamp::from_secs(1630),
                DEFAULT_GRACE_MS,
                none
            ),
            vec![(1, ReminderOutcome::Fired)],
            "宽限期以内仍算按时"
        );
        assert_eq!(
            due(
                &rules,
                clock(),
                Timestamp::from_millis(1_630_001),
                DEFAULT_GRACE_MS,
                none
            ),
            vec![(1, ReminderOutcome::Missed)]
        );
        // 很晚才打开：两条都错过，不补发
        assert_eq!(
            due(
                &rules,
                clock(),
                Timestamp::from_secs(2000),
                DEFAULT_GRACE_MS,
                none
            ),
            vec![(1, ReminderOutcome::Missed), (2, ReminderOutcome::Missed)]
        );
    }

    #[test]
    fn settled_and_disabled_rules_are_skipped() {
        let mut off = rule(2, 600);
        off.enabled = false;
        let rules = [rule(1, 600), off];
        assert!(
            due(
                &rules,
                clock(),
                Timestamp::from_secs(1600),
                DEFAULT_GRACE_MS,
                |id| id == 1
            )
            .is_empty()
        );
    }

    #[test]
    fn validation_catches_common_mistakes() {
        assert_eq!(validate(&default_rules()), Ok(()));
        assert_eq!(
            validate(&[rule(1, 60), rule(1, 120)]),
            Err(RuleError::DuplicateId(1))
        );
        assert_eq!(validate(&[rule(3, 5 * 3600)]), Err(RuleError::TooLate(3)));
        let mut blank = rule(4, 60);
        blank.label = "  ".into();
        assert_eq!(validate(&[blank]), Err(RuleError::EmptyLabel(4)));
    }

    #[test]
    fn rules_serialize_in_camel_case() {
        let json = serde_json::to_value(rule(1, 600)).expect("可序列化");
        assert_eq!(json["atSecs"], 600);
        let back: ReminderRule = serde_json::from_value(json).expect("可反序列化");
        assert_eq!(back, rule(1, 600));
    }
}
