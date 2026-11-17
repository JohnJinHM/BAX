//! 全局快捷键。默认值沿用原型：Ctrl+Alt+B 显示或隐藏悬浮窗，Ctrl+Alt+L 锁定或解锁。

use std::collections::BTreeMap;
use std::sync::Mutex;

use serde::Deserialize;
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutEvent, ShortcutState};

use crate::app::AppState;
use crate::overlay;

/// 快捷键设置。前端的对应定义在 `src/shared/settings.ts`。
pub const SETTINGS_KEY: &str = "ui.hotkeys";

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Hotkeys {
    toggle_overlay: String,
    toggle_lock: String,
}

impl Default for Hotkeys {
    fn default() -> Self {
        Hotkeys {
            toggle_overlay: "Ctrl+Alt+B".to_owned(),
            toggle_lock: "Ctrl+Alt+L".to_owned(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    ToggleOverlay,
    ToggleLock,
}

/// 当前注册成功的快捷键。
#[derive(Debug, Default)]
pub struct HotkeyState {
    bound: Mutex<Vec<(Shortcut, Action)>>,
}

/// 按设置重新注册全部快捷键，把失败的原因写进外壳状态。
pub fn apply(app: &AppHandle, ui: &BTreeMap<String, serde_json::Value>) {
    let hotkeys: Hotkeys = ui
        .get(SETTINGS_KEY)
        .and_then(|value| serde_json::from_value(value.clone()).ok())
        .unwrap_or_default();
    let shortcuts = app.global_shortcut();
    if let Err(err) = shortcuts.unregister_all() {
        tracing::warn!(error = %err, "注销快捷键失败");
    }

    let mut bound = Vec::new();
    let mut errors = Vec::new();
    for (action, text, name) in [
        (
            Action::ToggleOverlay,
            &hotkeys.toggle_overlay,
            "显示/隐藏悬浮窗",
        ),
        (Action::ToggleLock, &hotkeys.toggle_lock, "锁定/解锁悬浮窗"),
    ] {
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        let shortcut = match text.parse::<Shortcut>() {
            Ok(shortcut) => shortcut,
            Err(err) => {
                errors.push(format!("{name}：无法识别“{text}”（{err}）"));
                continue;
            }
        };
        if bound.iter().any(|(s, _)| *s == shortcut) {
            errors.push(format!("{name}：“{text}”与另一个快捷键重复"));
            continue;
        }
        match shortcuts.register(shortcut) {
            Ok(()) => bound.push((shortcut, action)),
            // 最常见的原因是被别的程序占用了
            Err(err) => errors.push(format!(
                "{name}：“{text}”注册失败，可能已被其他程序占用（{err}）"
            )),
        }
    }

    if let Ok(mut state) = app.state::<HotkeyState>().bound.lock() {
        *state = bound;
    }
    app.state::<AppState>()
        .update_status(app, |s| s.hotkey_errors = errors);
}

/// 全局快捷键插件的回调。
pub fn handle(app: &AppHandle, shortcut: &Shortcut, event: ShortcutEvent) {
    if event.state != ShortcutState::Pressed {
        return;
    }
    let action = app
        .state::<HotkeyState>()
        .bound
        .lock()
        .ok()
        .and_then(|bound| {
            bound
                .iter()
                .find(|(s, _)| s == shortcut)
                .map(|(_, action)| *action)
        });
    match action {
        Some(Action::ToggleOverlay) => overlay::toggle_visible(app),
        Some(Action::ToggleLock) => overlay::toggle_locked(app),
        None => {}
    }
}
