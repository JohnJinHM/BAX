//! 恢复时机独立于 Win32，便于验证快速切窗、主动隐藏和失败重试。

use std::time::{Duration, Instant};

/// 前台句柄与矩形；同一窗口切换分辨率或全屏尺寸也需要重新置顶。
pub type Foreground = (usize, [i32; 4]);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    WindowState,
    ForegroundChanged,
}

#[derive(Debug, Default)]
pub struct Recovery {
    foreground: Option<Foreground>,
    followups: [Option<Instant>; 2],
    retry_after: Option<Instant>,
}

impl Recovery {
    pub fn poll(
        &mut self,
        now: Instant,
        requested: bool,
        foreground: Foreground,
        healthy: bool,
    ) -> Option<Reason> {
        if !requested {
            *self = Self::default();
            return None;
        }
        // 切窗期间 GetForegroundWindow 可能短暂返回 NULL，不用它重置恢复计时。
        if foreground.0 != 0 && self.foreground != Some(foreground) {
            self.foreground = Some(foreground);
            self.followups = [
                Some(now + Duration::from_millis(250)),
                Some(now + Duration::from_secs(1)),
            ];
        }
        let mut due = false;
        for deadline in &mut self.followups {
            if deadline.is_some_and(|at| now >= at) {
                *deadline = None;
                due = true;
            }
        }
        if !healthy {
            if self.retry_after.is_some_and(|at| now < at) {
                return None;
            }
            self.retry_after = Some(now + Duration::from_secs(2));
            return Some(Reason::WindowState);
        }
        self.retry_after = None;
        due.then_some(Reason::ForegroundChanged)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GAME: Foreground = (1, [0, 0, 1920, 1080]);
    const OTHER: Foreground = (2, [0, 0, 1920, 1080]);

    #[test]
    fn reasserts_after_switch_even_when_topmost_flag_survives_then_stops() {
        let mut recovery = Recovery::default();
        let now = Instant::now();
        assert_eq!(recovery.poll(now, true, GAME, true), None);
        for ms in [250, 1000] {
            assert_eq!(
                recovery.poll(now + Duration::from_millis(ms), true, GAME, true),
                Some(Reason::ForegroundChanged)
            );
        }
        assert_eq!(
            recovery.poll(now + Duration::from_secs(60), true, GAME, true),
            None
        );
    }

    #[test]
    fn hiding_cancels_pending_recovery_and_does_not_restore_a_hidden_window() {
        let mut recovery = Recovery::default();
        let now = Instant::now();
        recovery.poll(now, true, GAME, true);
        assert_eq!(
            recovery.poll(now + Duration::from_millis(100), false, GAME, false),
            None
        );
        assert_eq!(
            recovery.poll(now + Duration::from_secs(5), false, OTHER, false),
            None
        );
        assert_eq!(
            recovery.poll(now + Duration::from_secs(6), true, OTHER, true),
            None
        );
    }

    #[test]
    fn rapid_switches_replace_old_deadlines() {
        let mut recovery = Recovery::default();
        let now = Instant::now();
        recovery.poll(now, true, GAME, true);
        recovery.poll(now + Duration::from_millis(200), true, OTHER, true);
        assert_eq!(
            recovery.poll(now + Duration::from_millis(250), true, OTHER, true),
            None
        );
        assert_eq!(
            recovery.poll(now + Duration::from_millis(450), true, OTHER, true),
            Some(Reason::ForegroundChanged)
        );
        assert_eq!(
            recovery.poll(now + Duration::from_millis(1000), true, OTHER, true),
            None
        );
    }

    #[test]
    fn repairs_bad_state_without_a_foreground_change_and_throttles_failures() {
        let mut recovery = Recovery::default();
        let now = Instant::now();
        assert_eq!(
            recovery.poll(now, true, GAME, false),
            Some(Reason::WindowState)
        );
        for ms in [250, 1000, 1750] {
            assert_eq!(
                recovery.poll(now + Duration::from_millis(ms), true, GAME, false),
                None
            );
        }
        assert_eq!(
            recovery.poll(now + Duration::from_secs(2), true, GAME, false),
            Some(Reason::WindowState)
        );
        recovery.poll(now + Duration::from_secs(3), true, GAME, true);
        assert_eq!(
            recovery.poll(now + Duration::from_millis(3250), true, GAME, false),
            Some(Reason::WindowState)
        );
    }

    #[test]
    fn null_foreground_does_not_restart_the_switch_timer() {
        let mut recovery = Recovery::default();
        let now = Instant::now();
        recovery.poll(now, true, GAME, true);
        recovery.poll(now + Duration::from_millis(100), true, (0, [0; 4]), true);
        assert_eq!(
            recovery.poll(now + Duration::from_millis(250), true, GAME, true),
            Some(Reason::ForegroundChanged)
        );
    }

    #[test]
    fn changing_fullscreen_bounds_rearms_recovery_for_the_same_window() {
        let mut recovery = Recovery::default();
        let now = Instant::now();
        recovery.poll(now, true, GAME, true);
        recovery.poll(now + Duration::from_secs(2), true, GAME, true);
        let resized = (GAME.0, [0, 0, 2560, 1440]);
        assert_eq!(
            recovery.poll(now + Duration::from_secs(3), true, resized, true),
            None
        );
        assert_eq!(
            recovery.poll(now + Duration::from_millis(3250), true, resized, true),
            Some(Reason::ForegroundChanged)
        );
    }
}
