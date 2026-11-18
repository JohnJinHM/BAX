//! 对局悬浮窗。
//!
//! 悬浮窗是一个透明、置顶、不出现在任务栏、不能获得焦点的窗口。两种模式：
//! 锁定时鼠标穿透到下面的游戏；解锁时可以拖动、点按钮。
//! 显示和隐藏都不激活窗口，免得把焦点从游戏抢走（独占全屏时抢焦点会让游戏最小化）。
//! 这些行为是否在各种显示模式下都成立，由验证 1 在实机上确认。

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{
    AppHandle, LogicalPosition, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
    WindowEvent,
};

use crate::app::AppState;

pub const LABEL: &str = "overlay";
/// 悬浮窗的位置与锁定状态，由外壳维护。
pub const WINDOW_KEY: &str = "ui.overlay.window";
const ALERTS_KEY: &str = "ui.alerts";
const WIDTH: f64 = 340.0;
const INITIAL_HEIGHT: f64 = 120.0;

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct WindowSettings {
    locked: bool,
    /// 逻辑像素。没有保存过时放在主显示器右上角。
    x: Option<f64>,
    y: Option<f64>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct AlertSettings {
    sound: bool,
    show_overlay: bool,
}

impl Default for AlertSettings {
    fn default() -> Self {
        AlertSettings {
            sound: true,
            show_overlay: true,
        }
    }
}

/// 悬浮窗自己的状态。
#[derive(Debug, Default)]
struct OverlayState {
    settings: Mutex<WindowSettings>,
    /// 每次移动加一；停下一会儿之后只保存最后一次的位置。
    moves: Arc<AtomicU64>,
}

fn read<T: for<'de> Deserialize<'de> + Default>(
    ui: &BTreeMap<String, serde_json::Value>,
    key: &str,
) -> T {
    ui.get(key)
        .and_then(|value| serde_json::from_value(value.clone()).ok())
        .unwrap_or_default()
}

pub fn create(
    app: &AppHandle,
    ui: &BTreeMap<String, serde_json::Value>,
    show: bool,
) -> tauri::Result<()> {
    let settings: WindowSettings = read(ui, WINDOW_KEY);
    let window = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("overlay.html".into()))
        .title("BAX 悬浮窗")
        .inner_size(WIDTH, INITIAL_HEIGHT)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focusable(false)
        .focused(false)
        .visible(false)
        .build()?;

    let saved = matches!((settings.x, settings.y), (Some(_), Some(_)));
    let position = match (settings.x, settings.y) {
        (Some(x), Some(y)) => Some(LogicalPosition::new(x, y)),
        _ => default_position(&window),
    };
    if let Some(position) = position {
        window.set_position(position)?;
    }
    log_placement(&window, saved);
    window.set_ignore_cursor_events(settings.locked)?;

    let state = OverlayState {
        settings: Mutex::new(settings),
        moves: Arc::default(),
    };
    app.manage(state);
    app.state::<AppState>()
        .update_status(app, |s| s.overlay_locked = settings.locked);

    let handle = app.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::Moved(_) = event {
            remember_position(&handle);
        }
    });

    if show {
        set_visible(app, true, "启动");
    }
    Ok(())
}

/// 悬浮窗所在的显示器与缩放，诊断多显示器和缩放问题时用。
fn log_placement(window: &WebviewWindow, saved: bool) {
    let monitor = window.current_monitor().ok().flatten();
    tracing::debug!(
        saved_position = saved,
        monitor = %monitor
            .as_ref()
            .and_then(|m| m.name())
            .map(String::as_str)
            .unwrap_or("-"),
        scale = monitor.as_ref().map(|m| m.scale_factor()),
        monitors = window.available_monitors().map(|m| m.len()).ok(),
        "悬浮窗位置"
    );
}

/// 主显示器右上角，留出一点边距。
fn default_position(window: &WebviewWindow) -> Option<LogicalPosition<f64>> {
    let monitor = window.primary_monitor().ok().flatten()?;
    let scale = monitor.scale_factor();
    let size = monitor.size().to_logical::<f64>(scale);
    let origin = monitor.position().to_logical::<f64>(scale);
    Some(LogicalPosition::new(
        origin.x + size.width - WIDTH - 24.0,
        origin.y + 80.0,
    ))
}

/// `via` 写进诊断日志，说明是谁要求的：快捷键、托盘、界面、提醒或启动。
pub fn set_visible(app: &AppHandle, visible: bool, via: &'static str) {
    let Some(window) = app.get_webview_window(LABEL) else {
        return;
    };
    tracing::info!(visible, via = %via, "悬浮窗显示状态");
    // 显示前后各看一次前台程序，确认显示悬浮窗不抢游戏的焦点（验收清单的要求）
    #[cfg(windows)]
    let focus_before = (visible && tracing::enabled!(tracing::Level::DEBUG))
        .then(crate::win32::foreground_process_name);
    #[cfg(windows)]
    match window.hwnd() {
        Ok(hwnd) => {
            if visible {
                crate::win32::show_without_activating(hwnd.0);
            } else {
                crate::win32::hide(hwnd.0);
            }
        }
        Err(err) => tracing::warn!(error = %err, "取不到悬浮窗的窗口句柄"),
    }
    #[cfg(not(windows))]
    {
        let _ = if visible {
            window.show()
        } else {
            window.hide()
        };
    }
    app.state::<AppState>()
        .update_status(app, |s| s.overlay_visible = visible);
    #[cfg(windows)]
    if let Some(before) = focus_before {
        check_focus_after_show(before);
    }
}

/// 显示悬浮窗 0.3 秒后，看前台窗口是否还是显示之前的那个程序。
#[cfg(windows)]
fn check_focus_after_show(before: Option<String>) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        let after = crate::win32::foreground_process_name();
        tracing::debug!(
            before = %before.as_deref().unwrap_or("-"),
            after = %after.as_deref().unwrap_or("-"),
            kept = before == after,
            "显示悬浮窗后的前台程序"
        );
    });
}

pub fn toggle_visible(app: &AppHandle, via: &'static str) {
    let visible = app.state::<AppState>().status().overlay_visible;
    set_visible(app, !visible, via);
}

pub fn set_locked(app: &AppHandle, locked: bool, via: &'static str) -> tauri::Result<()> {
    tracing::info!(locked, via = %via, "悬浮窗锁定状态");
    if let Some(window) = app.get_webview_window(LABEL) {
        window.set_ignore_cursor_events(locked)?;
    }
    app.state::<AppState>()
        .update_status(app, |s| s.overlay_locked = locked);
    let overlay = app.state::<OverlayState>();
    if let Ok(mut settings) = overlay.settings.lock() {
        settings.locked = locked;
        save(app, *settings);
    }
    Ok(())
}

pub fn toggle_locked(app: &AppHandle, via: &'static str) {
    let locked = app.state::<AppState>().status().overlay_locked;
    if let Err(err) = set_locked(app, !locked, via) {
        tracing::warn!(error = %err, "切换悬浮窗锁定失败");
    }
}

/// 有提醒按时触发：按设置响铃、把隐藏的悬浮窗显示出来。闪烁由悬浮窗页面自己处理。
pub fn on_alert(app: &AppHandle, ui: &BTreeMap<String, serde_json::Value>) {
    let alerts: AlertSettings = read(ui, ALERTS_KEY);
    tracing::info!(
        sound = alerts.sound,
        show_overlay = alerts.show_overlay,
        "提醒响应"
    );
    if alerts.show_overlay && !app.state::<AppState>().status().overlay_visible {
        set_visible(app, true, "提醒");
    }
    #[cfg(windows)]
    if alerts.sound {
        // 与原型相同的提示音：1050 Hz，两声
        std::thread::spawn(|| {
            crate::win32::beep(1050, 180);
            std::thread::sleep(Duration::from_millis(120));
            crate::win32::beep(1050, 180);
        });
    }
}

/// 拖动会连续产生很多移动事件，停下 0.6 秒后再保存位置。
fn remember_position(app: &AppHandle) {
    let overlay = app.state::<OverlayState>();
    let ticket = overlay.moves.fetch_add(1, Ordering::SeqCst) + 1;
    let moves = Arc::clone(&overlay.moves);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(600)).await;
        if moves.load(Ordering::SeqCst) != ticket {
            return;
        }
        let Some(window) = app.get_webview_window(LABEL) else {
            return;
        };
        let (Ok(position), Ok(scale)) = (window.outer_position(), window.scale_factor()) else {
            return;
        };
        let logical = position.to_logical::<f64>(scale);
        let overlay = app.state::<OverlayState>();
        let Ok(mut settings) = overlay.settings.lock() else {
            return;
        };
        settings.x = Some(logical.x);
        settings.y = Some(logical.y);
        save(&app, *settings);
        drop(settings);
        log_placement(&window, true);
    });
}

fn save(app: &AppHandle, settings: WindowSettings) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let Ok(value) = serde_json::to_value(settings) else {
            return;
        };
        let engine = &app.state::<AppState>().engine;
        if let Err(err) = engine.set_ui_setting(WINDOW_KEY, value).await {
            tracing::warn!(error = %err, "保存悬浮窗位置失败");
        }
    });
}
