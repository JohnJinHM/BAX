//! 对局悬浮窗。
//!
//! 悬浮窗是一个透明、置顶、不出现在任务栏、不能获得焦点的窗口。两种模式：
//! 锁定时鼠标穿透到下面的游戏，只有锁按钮还能点；解锁时可以拖动、点全部按钮。
//! 显示和隐藏都不激活窗口，免得把焦点从游戏抢走（独占全屏时抢焦点会让游戏最小化）。
//! 这些行为是否在各种显示模式下都成立，由验证 1 在实机上确认。
//!
//! 开启吸附边缘（界面设置 `ui.overlay` 的 `snapEdges`，默认开启）时，拖动松手后离屏幕工作区的边不远就贴过去；
//! 页面按内容改变窗口大小时保持贴着的边，并留在工作区里。几何计算见 `snap` 模块。

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, PhysicalPosition, PhysicalSize,
    WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent,
};

use crate::app::AppState;
use crate::snap;

pub const LABEL: &str = "overlay";
/// 悬浮窗的位置与锁定状态，由外壳维护。
pub const WINDOW_KEY: &str = "ui.overlay.window";
const ALERTS_KEY: &str = "ui.alerts";
/// 悬浮窗外观，由界面维护；外壳只读其中的 `snapEdges`。
const APPEARANCE_KEY: &str = "ui.overlay";
/// 拖动松手后离工作区边缘多近就贴过去，逻辑像素。与原型相同。
const SNAP_DISTANCE: f64 = 18.0;
/// 移动停下多久后看是否松手、是否贴边。
const SNAP_DELAY: Duration = Duration::from_millis(150);
/// 移动停下多久后保存位置。
const SAVE_DELAY: Duration = Duration::from_millis(600);
/// 锁定期间鼠标进入或离开锁按钮。载荷是 `bool`，悬浮窗据此显示按钮的悬停样式：
/// 窗口恢复穿透后页面收不到鼠标离开的事件，不能靠 CSS 的 `:hover`。
pub const LOCK_HOVER_EVENT: &str = "bax://overlay-lock-hover";
/// 锁定期间检查鼠标位置的间隔。
const POLL_INTERVAL: Duration = Duration::from_millis(40);
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

/// `ui.overlay` 里外壳要读的部分。默认值与界面一侧（`src/shared/settings.ts`）保持一致。
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Appearance {
    snap_edges: bool,
}

impl Default for Appearance {
    fn default() -> Self {
        Appearance { snap_edges: true }
    }
}

/// 锁按钮在窗口里的位置，物理像素，相对窗口左上角。由悬浮窗页面报告。
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct Hotspot {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

impl Hotspot {
    #[cfg_attr(not(windows), allow(dead_code))]
    fn contains(&self, (x, y): (i32, i32)) -> bool {
        let (x, y) = (f64::from(x), f64::from(y));
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }
}

/// 锁定期间的鼠标穿透状态。只在主线程上改窗口样式，轮询线程只读鼠标位置。
#[derive(Debug, Default)]
struct Pointer {
    locked: bool,
    hotspot: Option<Hotspot>,
    /// 当前是否穿透。锁定时为 `true`，鼠标在锁按钮上时暂时为 `false`。
    passthrough: bool,
    /// 每次锁定加一；旧的轮询线程看到编号变了就退出。
    generation: u64,
}

/// 悬浮窗自己的状态。
#[derive(Debug, Default)]
struct OverlayState {
    settings: Mutex<WindowSettings>,
    /// 每次移动加一；停下一会儿之后只保存最后一次的位置。
    moves: Arc<AtomicU64>,
    pointer: Arc<Mutex<Pointer>>,
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

    let state = OverlayState {
        settings: Mutex::new(settings),
        moves: Arc::default(),
        pointer: Arc::default(),
    };
    app.manage(state);
    apply_lock(app, &window, settings.locked);
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

/// 锁定时鼠标穿透。不用 tao 的 `set_ignore_cursor_events`，原因见 `win32` 模块的说明。
fn apply_click_through(window: &WebviewWindow, locked: bool) {
    #[cfg(windows)]
    match window.hwnd() {
        Ok(hwnd) => {
            if !crate::win32::set_click_through(hwnd.0, locked) {
                tracing::warn!(locked, "设置悬浮窗鼠标穿透失败");
            }
        }
        Err(err) => tracing::warn!(error = %err, "取不到悬浮窗的窗口句柄"),
    }
    #[cfg(not(windows))]
    if let Err(err) = window.set_ignore_cursor_events(locked) {
        tracing::warn!(error = %err, "设置悬浮窗鼠标穿透失败");
    }
}

/// 设置穿透并开始或停止轮询鼠标。在主线程上调用。
fn apply_lock(app: &AppHandle, window: &WebviewWindow, locked: bool) {
    let pointer = Arc::clone(&app.state::<OverlayState>().pointer);
    let generation = {
        let Ok(mut p) = pointer.lock() else {
            return;
        };
        p.locked = locked;
        p.passthrough = locked;
        p.generation += 1;
        p.generation
    };
    apply_click_through(window, locked);
    let _ = window.emit_to(LABEL, LOCK_HOVER_EVENT, false);
    #[cfg(windows)]
    if locked {
        match window.hwnd() {
            Ok(hwnd) => watch_lock_button(app.clone(), pointer, hwnd.0 as isize, generation),
            Err(err) => tracing::warn!(error = %err, "取不到悬浮窗的窗口句柄"),
        }
    }
    #[cfg(not(windows))]
    let _ = generation;
}

/// 锁定期间每 40 毫秒看一次鼠标是否在锁按钮上，在上面时让窗口接收鼠标，
/// 离开后恢复穿透。改样式交给主线程做：样式变化的消息要由窗口所在的线程处理，
/// 在这里改会等主线程，而主线程可能正等着 `pointer` 这把锁。
#[cfg(windows)]
fn watch_lock_button(app: AppHandle, pointer: Arc<Mutex<Pointer>>, hwnd: isize, generation: u64) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(POLL_INTERVAL);
            let (hotspot, passthrough) = match pointer.lock() {
                Ok(p) if p.locked && p.generation == generation => (p.hotspot, p.passthrough),
                _ => return,
            };
            let over = hotspot.is_some_and(|h| {
                crate::win32::cursor_in_window(hwnd as *mut std::ffi::c_void)
                    .is_some_and(|at| h.contains(at))
            });
            // 鼠标在按钮上时不穿透
            let wanted = !over;
            if wanted == passthrough {
                continue;
            }
            let pointer = Arc::clone(&pointer);
            let handle = app.clone();
            let _ = app.run_on_main_thread(move || {
                let Ok(mut p) = pointer.lock() else {
                    return;
                };
                if !p.locked || p.generation != generation || p.passthrough == wanted {
                    return;
                }
                crate::win32::set_passthrough(hwnd as *mut std::ffi::c_void, wanted);
                p.passthrough = wanted;
                drop(p);
                let _ = handle.emit_to(LABEL, LOCK_HOVER_EVENT, over);
            });
        }
    });
}

/// 悬浮窗页面报告锁按钮的位置；按钮不显示时为 `None`。
pub fn set_hotspot(app: &AppHandle, hotspot: Option<Hotspot>) {
    if let Ok(mut p) = app.state::<OverlayState>().pointer.lock() {
        p.hotspot = hotspot;
    }
}

pub fn set_locked(app: &AppHandle, locked: bool, via: &'static str) -> tauri::Result<()> {
    tracing::info!(locked, via = %via, "悬浮窗锁定状态");
    if let Some(window) = app.get_webview_window(LABEL) {
        apply_lock(app, &window, locked);
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

fn snap_enabled(app: &AppHandle) -> bool {
    let snapshot = app.state::<AppState>().engine.snapshot();
    read::<Appearance>(&snapshot.ui, APPEARANCE_KEY).snap_edges
}

/// 窗口的位置大小与它所在显示器的工作区（去掉任务栏），物理像素。
fn geometry(window: &WebviewWindow) -> Option<(snap::Rect, snap::Rect)> {
    let position = window.outer_position().ok()?;
    let size = window.outer_size().ok()?;
    let monitor = window.current_monitor().ok().flatten()?;
    let area = monitor.work_area();
    Some((
        snap::Rect {
            x: position.x,
            y: position.y,
            width: i32::try_from(size.width).ok()?,
            height: i32::try_from(size.height).ok()?,
        },
        snap::Rect {
            x: area.position.x,
            y: area.position.y,
            width: i32::try_from(area.size.width).ok()?,
            height: i32::try_from(area.size.height).ok()?,
        },
    ))
}

/// 离工作区边缘不远时贴过去。位置变了返回 `true`，之后的移动事件会负责保存。
fn snap_to_edges(app: &AppHandle) -> bool {
    let Some(window) = app.get_webview_window(LABEL) else {
        return false;
    };
    let (Some((rect, area)), Ok(scale)) = (geometry(&window), window.scale_factor()) else {
        return false;
    };
    let distance = (SNAP_DISTANCE * scale).round() as i32;
    let (x, y) = snap::snap(rect, area, distance);
    if (x, y) == (rect.x, rect.y) {
        return false;
    }
    tracing::debug!(dx = x - rect.x, dy = y - rect.y, "悬浮窗贴边");
    window.set_position(PhysicalPosition::new(x, y)).is_ok()
}

/// 页面按内容报告的大小，逻辑像素。开启吸附边缘时，位置和大小一起算、一次改完：
/// 贴着下边展开时窗口不会先往下长再跳回来。
pub fn fit(app: &AppHandle, width: f64, height: f64) -> tauri::Result<()> {
    let Some(window) = app.get_webview_window(LABEL) else {
        return Ok(());
    };
    let logical = LogicalSize::new(width, height);
    let geometry = snap_enabled(app).then(|| geometry(&window)).flatten();
    let Some((rect, area)) = geometry else {
        return window.set_size(logical);
    };
    let size: PhysicalSize<i32> = logical.to_physical(window.scale_factor()?);
    let (x, y) = snap::resize(rect, size.width, size.height, area);
    #[cfg(windows)]
    {
        let hwnd = window.hwnd()?;
        if !crate::win32::set_bounds(hwnd.0, x, y, size.width, size.height) {
            tracing::warn!("调整悬浮窗大小失败");
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        window.set_size(size)?;
        window.set_position(PhysicalPosition::new(x, y))
    }
}

/// 拖动会连续产生很多移动事件。停下一会儿、鼠标键都松开以后按设置贴边；停下 0.6 秒后保存位置。
fn remember_position(app: &AppHandle) {
    let overlay = app.state::<OverlayState>();
    let ticket = overlay.moves.fetch_add(1, Ordering::SeqCst) + 1;
    let moves = Arc::clone(&overlay.moves);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let superseded = || moves.load(Ordering::SeqCst) != ticket;
        tokio::time::sleep(SNAP_DELAY).await;
        if superseded() {
            return;
        }
        if snap_enabled(&app) {
            // 拖动时按住鼠标不动不会产生移动事件，等松手再贴边
            #[cfg(windows)]
            while crate::win32::mouse_button_down() {
                tokio::time::sleep(POLL_INTERVAL).await;
                if superseded() {
                    return;
                }
            }
            if snap_to_edges(&app) {
                return;
            }
        }
        tokio::time::sleep(SAVE_DELAY.saturating_sub(SNAP_DELAY)).await;
        if superseded() {
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
