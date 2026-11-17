//! 前端可以调用的命令。参数名在前端写作 camelCase（Tauri 默认转换）。

use std::path::PathBuf;

use bax_core::ReminderRule;
use bax_engine::Snapshot;
use tauri::{AppHandle, State};

use crate::app::{AppState, AppStatus, show_main_window};
use crate::overlay;

type CommandResult = Result<(), String>;

#[tauri::command]
pub fn get_snapshot(state: State<'_, AppState>) -> Snapshot {
    (*state.engine.snapshot()).clone()
}

#[tauri::command]
pub fn get_app_status(state: State<'_, AppState>) -> AppStatus {
    state.status()
}

#[tauri::command]
pub async fn refresh_players(state: State<'_, AppState>) -> CommandResult {
    state
        .engine
        .refresh_players()
        .await
        .map_err(|e| e.to_string())
}

/// `dir` 为空表示改回自动定位。
#[tauri::command]
pub async fn set_log_dir(state: State<'_, AppState>, dir: Option<String>) -> CommandResult {
    let dir = dir
        .map(|d| d.trim().to_owned())
        .filter(|d| !d.is_empty())
        .map(PathBuf::from);
    state
        .engine
        .set_log_dir(dir)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_reminders(state: State<'_, AppState>, rules: Vec<ReminderRule>) -> CommandResult {
    state
        .engine
        .set_reminders(rules)
        .await
        .map_err(|e| e.to_string())
}

/// `elapsed_ms`：此刻处于正式开局后多少毫秒，准备阶段里为负数。
#[tauri::command]
pub async fn calibrate(state: State<'_, AppState>, elapsed_ms: i64) -> CommandResult {
    state
        .engine
        .calibrate(elapsed_ms)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_ui_setting(
    state: State<'_, AppState>,
    key: String,
    value: serde_json::Value,
) -> CommandResult {
    // 悬浮窗的位置与锁定状态由外壳维护，不接受前端直接写
    if key == overlay::WINDOW_KEY {
        return Err("这项设置由程序自己维护".to_owned());
    }
    state
        .engine
        .set_ui_setting(key, value)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_overlay_locked(app: AppHandle, locked: bool) -> CommandResult {
    overlay::set_locked(&app, locked).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_overlay_visible(app: AppHandle, visible: bool) {
    overlay::set_visible(&app, visible);
}

/// 窗口要在异步命令里创建：同步命令跑在主线程上，在那里建窗口会卡住事件循环。
#[tauri::command]
pub async fn open_main_window(app: AppHandle) -> CommandResult {
    show_main_window(&app);
    Ok(())
}
