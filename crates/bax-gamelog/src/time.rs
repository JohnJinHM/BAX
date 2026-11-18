//! 日志时间。
//!
//! 日志里的时间戳是本机的本地时间，不带时区。[`LogTime`] 原样保存它，
//! 换算成 UTC 时才需要时区，见 [`LogZone`]。

use bax_core::Timestamp;
use chrono::{Local, NaiveDate, NaiveDateTime, TimeZone};

/// 日志记录的本地时间，精确到毫秒。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LogTime {
    /// 把本地时间的各字段当作 UTC 得到的毫秒数。只用于排序、求差和换算。
    naive_ms: i64,
}

impl LogTime {
    pub fn from_parts(
        year: i32,
        month: u32,
        day: u32,
        hour: u32,
        minute: u32,
        second: u32,
        milli: u32,
    ) -> Option<LogTime> {
        let naive = NaiveDate::from_ymd_opt(year, month, day)?
            .and_hms_milli_opt(hour, minute, second, milli)?;
        Some(LogTime {
            naive_ms: naive.and_utc().timestamp_millis(),
        })
    }

    /// 解析记录开头的时间戳行，形如 `[2026-11-16 18:52:05.319]`。
    ///
    /// 这个函数对日志的每一行都会调用，所以先用长度和首尾字符快速排除。
    pub fn parse_bracketed(line: &str) -> Option<LogTime> {
        let b = line.as_bytes();
        if b.len() != 25 || b[0] != b'[' || b[24] != b']' {
            return None;
        }
        if b[5] != b'-'
            || b[8] != b'-'
            || b[11] != b' '
            || b[14] != b':'
            || b[17] != b':'
            || b[20] != b'.'
        {
            return None;
        }
        let num = |range: std::ops::Range<usize>| -> Option<u32> {
            let mut v = 0u32;
            for &c in &b[range] {
                if !c.is_ascii_digit() {
                    return None;
                }
                v = v * 10 + u32::from(c - b'0');
            }
            Some(v)
        };
        LogTime::from_parts(
            num(1..5)? as i32,
            num(6..8)?,
            num(9..11)?,
            num(12..14)?,
            num(15..17)?,
            num(18..20)?,
            num(21..24)?,
        )
    }

    /// 两个日志时间之差，毫秒。同一份日志里的时间可以直接相减，不需要时区。
    pub fn millis_since(self, earlier: LogTime) -> i64 {
        self.naive_ms - earlier.naive_ms
    }

    /// 换算成 UTC。
    pub fn to_utc(self, zone: LogZone) -> Timestamp {
        match zone {
            LogZone::Fixed { offset_seconds } => {
                Timestamp::from_millis(self.naive_ms - i64::from(offset_seconds) * 1000)
            }
            LogZone::System => Timestamp::from_millis(self.system_utc_millis()),
        }
    }

    fn naive(self) -> Option<NaiveDateTime> {
        chrono::DateTime::from_timestamp_millis(self.naive_ms).map(|dt| dt.naive_utc())
    }

    fn system_utc_millis(self) -> i64 {
        let Some(naive) = self.naive() else {
            return self.naive_ms;
        };
        match Local.from_local_datetime(&naive).earliest() {
            Some(dt) => dt.timestamp_millis(),
            // 夏令时切换时被跳过的那一小时：本地时间不存在，按当前偏移量换算
            None => {
                let offset = i64::from(Local::now().offset().local_minus_utc());
                self.naive_ms - offset * 1000
            }
        }
    }
}

/// 与日志里的写法相同，例如 `2026-11-16 18:52:05.319`。
impl std::fmt::Display for LogTime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.naive() {
            Some(naive) => write!(f, "{}", naive.format("%Y-%m-%d %H:%M:%S%.3f")),
            None => write!(f, "{}", self.naive_ms),
        }
    }
}

/// 把日志的本地时间换算成 UTC 时使用的时区。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LogZone {
    /// 本机的系统时区。游戏和 BAX 运行在同一台机器上，这是正常情况。
    #[default]
    System,
    /// 固定偏移量，东区为正。用于测试，以及处理在别的时区录制的日志。
    Fixed { offset_seconds: i32 },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_timestamp_line() {
        let t = LogTime::parse_bracketed("[2026-11-16 18:52:05.319]").expect("应当解析成功");
        let midnight = LogTime::from_parts(2026, 11, 16, 0, 0, 0, 0).expect("合法日期");
        assert_eq!(
            t.millis_since(midnight),
            ((18 * 60 + 52) * 60 + 5) * 1000 + 319
        );
    }

    #[test]
    fn displays_like_the_log() {
        let t = LogTime::parse_bracketed("[2026-11-16 18:52:05.019]").expect("应当解析成功");
        assert_eq!(t.to_string(), "2026-11-16 18:52:05.019");
    }

    #[test]
    fn rejects_lines_that_only_look_similar() {
        for line in [
            "",
            "[2026-11-16 18:52:05.319] trailing",
            "[2026-11-16 18:52:05.31]",
            "[2026-11-16T18:52:05.319]",
            "[2026-13-16 18:52:05.319]",
            "[2026-11-16 24:52:05.319]",
            "[2026-11-16 18:52:05,319]",
            "[abcd-11-16 18:52:05.319]",
            "Log: [2026-11-16 18:52:05]",
            "[2026-11-16 18:52:05.319 ",
        ] {
            assert_eq!(LogTime::parse_bracketed(line), None, "{line:?}");
        }
    }

    #[test]
    fn converts_to_utc_with_a_fixed_offset() {
        // 日志在 UTC-8 录制：本地 18:52 对应 UTC 次日 02:52
        let t = LogTime::parse_bracketed("[2026-11-16 18:52:05.319]").expect("应当解析成功");
        let utc = t.to_utc(LogZone::Fixed {
            offset_seconds: -8 * 3600,
        });
        let expected = LogTime::from_parts(2026, 11, 17, 2, 52, 5, 319)
            .expect("合法日期")
            .to_utc(LogZone::Fixed { offset_seconds: 0 });
        assert_eq!(utc, expected);
    }

    #[test]
    fn system_zone_agrees_with_the_current_offset_for_now() {
        // 不依赖具体时区：用系统时区换算“现在”，结果应当回到同一时刻（精确到毫秒内的舍入）
        let now = Local::now();
        let naive = now.naive_local();
        let t = LogTime {
            naive_ms: naive.and_utc().timestamp_millis(),
        };
        let utc = t.to_utc(LogZone::System).as_millis();
        // 夏令时回拨的那一小时里本地时间有两种解释，相差恰好一小时
        let diff = (utc - now.timestamp_millis()).abs();
        assert!(diff == 0 || diff == 3_600_000, "差值 {diff} 毫秒");
    }
}
