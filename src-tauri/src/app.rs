//! 应用的组装：启动引擎、创建窗口与托盘、转发快照。

use std::sync::{Arc, Mutex};

use bax_engine::{Engine, EngineConfig, Snapshot, SystemClock, default_data_dir};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder};
use tokio::sync::watch;

use crate::{commands, hotkeys, overlay, tray};

/// 快照事件：载荷是完整的 [`Snapshot`]。
pub const SNAPSHOT_EVENT: &str = "bax://snapshot";
/// 外壳状态事件：载荷是 [`AppStatus`]。
pub const APP_STATUS_EVENT: &str = "bax://app";
pub const MAIN_LABEL: &str = "main";

/// 外壳自己的状态。前端的对应定义在 `src/shared/engine.ts`。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppStatus {
    pub overlay_visible: bool,
    pub overlay_locked: bool,
    /// 注册失败的快捷键及原因。
    pub hotkey_errors: Vec<String>,
    pub data_dir: String,
}

#[derive(Debug)]
pub struct AppState {
    pub engine: Engine,
    status: Mutex<AppStatus>,
}

impl AppState {
    pub fn status(&self) -> AppStatus {
        self.status.lock().map(|s| s.clone()).unwrap_or_default()
    }

    /// 修改外壳状态，有变化时通知所有窗口。
    pub fn update_status(&self, app: &AppHandle, change: impl FnOnce(&mut AppStatus)) {
        let changed = match self.status.lock() {
            Ok(mut status) => {
                let before = status.clone();
                change(&mut status);
                (*status != before).then(|| status.clone())
            }
            Err(_) => None,
        };
        if let Some(status) = changed {
            let _ = app.emit(APP_STATUS_EVENT, status);
        }
    }
}

pub fn run() {
    init_tracing();
    // --hidden：启动时不显示悬浮窗，只有托盘图标（开机自启或调试时用）
    let start_hidden = std::env::args().any(|arg| arg == "--hidden");

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // 已经在运行时再次启动：打开主窗口
            show_main_window(app);
        }))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(hotkeys::handle)
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            commands::get_snapshot,
            commands::get_app_status,
            commands::refresh_players,
            commands::set_log_dir,
            commands::set_reminders,
            commands::calibrate,
            commands::set_ui_setting,
            commands::set_overlay_locked,
            commands::set_overlay_visible,
            commands::open_main_window,
        ])
        .setup(move |app| {
            let data_dir = default_data_dir().ok_or("无法确定数据目录")?;
            let config = EngineConfig::new(&data_dir);
            // 引擎需要在异步运行时里启动；Tauri 自带的就是 tokio
            let engine = tauri::async_runtime::block_on(async {
                Engine::start(config, Arc::new(SystemClock))
            })?;
            let snapshots = engine.subscribe();
            let ui = engine.snapshot().ui.clone();
            app.manage(AppState {
                engine,
                status: Mutex::new(AppStatus {
                    data_dir: data_dir.display().to_string(),
                    ..AppStatus::default()
                }),
            });
            app.manage(hotkeys::HotkeyState::default());

            let handle = app.handle();
            overlay::create(handle, &ui, !start_hidden)?;
            tray::create(handle)?;
            hotkeys::apply(handle, &ui);
            forward_snapshots(handle.clone(), snapshots);
            Ok(())
        })
        .build(tauri::generate_context!());

    let app = match app {
        Ok(app) => app,
        Err(err) => {
            tracing::error!(error = %err, "BAX 启动失败");
            std::process::exit(1);
        }
    };
    app.run(|_app, event| {
        if let RunEvent::ExitRequested { api, code, .. } = event {
            // 关掉主窗口不退出：BAX 常驻托盘，只有托盘菜单的“退出”才结束
            if code.is_none() {
                api.prevent_exit();
            }
        }
    });
}

/// 把引擎的每份快照转发给所有窗口，并处理需要外壳动作的变化（提醒、快捷键设置）。
fn forward_snapshots(app: AppHandle, mut snapshots: watch::Receiver<Arc<Snapshot>>) {
    tauri::async_runtime::spawn(async move {
        let (mut last_alert, mut last_ui) = {
            let first = snapshots.borrow();
            (first.alert.as_ref().map_or(0, |a| a.seq), first.ui.clone())
        };
        while snapshots.changed().await.is_ok() {
            let snapshot = snapshots.borrow_and_update().clone();
            if let Err(err) = app.emit(SNAPSHOT_EVENT, &*snapshot) {
                tracing::warn!(error = %err, "转发快照失败");
            }
            if let Some(alert) = &snapshot.alert
                && alert.seq > last_alert
            {
                last_alert = alert.seq;
                overlay::on_alert(&app, &snapshot.ui);
            }
            if snapshot.ui != last_ui {
                if snapshot.ui.get(hotkeys::SETTINGS_KEY) != last_ui.get(hotkeys::SETTINGS_KEY) {
                    hotkeys::apply(&app, &snapshot.ui);
                }
                last_ui = snapshot.ui.clone();
            }
        }
    });
}

/// 打开主窗口；已经打开时把它带到前台。
pub fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN_LABEL) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }
    let built = WebviewWindowBuilder::new(app, MAIN_LABEL, WebviewUrl::App("index.html".into()))
        .title("BAX")
        .inner_size(1120.0, 720.0)
        .min_inner_size(800.0, 480.0)
        .build();
    if let Err(err) = built {
        tracing::error!(error = %err, "无法打开主窗口");
    }
}

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .with_writer(std::io::stderr)
        .try_init();
}
