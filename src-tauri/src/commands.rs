//! 前端可以调用的命令。参数名在前端写作 camelCase（Tauri 默认转换）。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use bax_core::{MatchId, PlayerId, ReminderRule};
use bax_engine::diag::{self, ExportOptions};
use bax_engine::{
    CommandError, HistoryRow, ImportSummary, MatchReportView, PlayerDetail, PlayerHit, Snapshot,
};
use tauri::{AppHandle, State, Window};

use crate::app::{AppState, AppStatus, reveal_in_explorer, show_main_window};
use crate::overlay;

#[tauri::command]
pub async fn check_data_sources(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    serde_json::to_value(state.engine.check_data_sources().await).map_err(|e| e.to_string())
}

type CommandResult = Result<(), String>;

/// 命令被拒绝或失败时记一条诊断日志，再把原因交给前端。
fn rejected(command: &'static str) -> impl FnOnce(CommandError) -> String {
    move |err| {
        tracing::info!(command = %command, reason = %err, "命令没有执行");
        err.to_string()
    }
}

#[tauri::command]
pub fn get_snapshot(state: State<'_, AppState>) -> Snapshot {
    (*state.engine.snapshot()).clone()
}

#[tauri::command]
pub fn get_app_status(state: State<'_, AppState>) -> AppStatus {
    state.status()
}

/// 只接受玩家 ID，由外壳组装固定站点地址并交给默认浏览器。
#[tauri::command]
pub fn open_batrace_player(id: PlayerId) -> CommandResult {
    if id.0 == 0 {
        return Err("玩家 ID 无效".to_owned());
    }
    std::process::Command::new("explorer.exe")
        .arg(format!("https://dash.batrace.top/player/{}", id.0))
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("无法打开 BATrace 玩家页面：{e}"))
}

#[tauri::command]
pub async fn player_fights(
    state: State<'_, AppState>,
    id: PlayerId,
) -> Result<bax_engine::PlayerFightArchive, String> {
    state
        .engine
        .player_fights(id)
        .await
        .map_err(rejected("player_fights"))
}
#[tauri::command]
pub fn stop_player_fights(state: State<'_, AppState>, id: PlayerId) {
    state.engine.stop_player_fights(id);
}
#[tauri::command]
pub async fn sync_player_fights(
    state: State<'_, AppState>,
    id: PlayerId,
    limit: u32,
) -> Result<String, String> {
    state
        .engine
        .sync_player_fights(id, limit)
        .await
        .map_err(rejected("sync_player_fights"))
}
#[tauri::command]
pub async fn refresh_players(state: State<'_, AppState>) -> CommandResult {
    tracing::info!(via = %"界面", "刷新玩家资料");
    state
        .engine
        .refresh_players()
        .await
        .map_err(rejected("refresh_players"))
}

/// 打开所属窗口的系统目录选择器；取消时保留当前日志来源。
#[tauri::command]
pub async fn pick_log_dir(window: Window) -> Result<Option<PathBuf>, String> {
    #[cfg(windows)]
    {
        tauri::async_runtime::spawn_blocking(move || {
            rfd::FileDialog::new()
                .set_parent(&window)
                .set_title("选择 Broken Arrow 的 GameLogs 目录")
                .pick_folder()
        })
        .await
        .map_err(|e| format!("无法打开目录选择器：{e}"))
    }
    #[cfg(not(windows))]
    {
        let _ = window;
        Err("请在设置中输入 GameLogs 目录路径".into())
    }
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
        .map_err(rejected("set_log_dir"))
}

#[tauri::command]
pub async fn reserve_reminder_id(state: State<'_, AppState>) -> Result<u32, String> {
    state
        .engine
        .reserve_reminder_id()
        .await
        .map_err(rejected("reserve_reminder_id"))
}

#[tauri::command]
pub async fn set_reminders(state: State<'_, AppState>, rules: Vec<ReminderRule>) -> CommandResult {
    state
        .engine
        .set_reminders(rules)
        .await
        .map_err(rejected("set_reminders"))
}

/// `elapsed_ms`：此刻处于正式开局后多少毫秒，准备阶段里为负数。
#[tauri::command]
pub async fn calibrate(state: State<'_, AppState>, elapsed_ms: i64) -> CommandResult {
    state
        .engine
        .calibrate(elapsed_ms)
        .await
        .map_err(rejected("calibrate"))
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
        .map_err(rejected("set_ui_setting"))
}

#[tauri::command]
pub fn set_overlay_locked(app: AppHandle, locked: bool) -> CommandResult {
    overlay::set_locked(&app, locked, "界面").map_err(|e| e.to_string())
}

/// 悬浮窗按内容报告自己的大小（逻辑像素）。开启吸附边缘时保持贴着的边，并留在屏幕里。
#[tauri::command]
pub fn fit_overlay(app: AppHandle, width: f64, height: f64) -> CommandResult {
    overlay::fit(&app, width, height).map_err(|e| e.to_string())
}

/// 悬浮窗报告锁按钮的位置（物理像素，相对窗口左上角），锁定后鼠标在这里时仍可点击。
#[tauri::command]
pub fn set_overlay_hotspot(app: AppHandle, hotspot: Option<overlay::Hotspot>) {
    overlay::set_hotspot(&app, hotspot);
}

#[tauri::command]
pub fn set_overlay_visible(app: AppHandle, visible: bool) {
    overlay::set_visible(&app, visible, "界面");
}

#[tauri::command]
pub async fn player_detail(
    state: State<'_, AppState>,
    id: u64,
) -> Result<Option<PlayerDetail>, String> {
    state
        .engine
        .player_detail(PlayerId(id))
        .await
        .map_err(rejected("player_detail"))
}

/// `query` 为空时列出最近见过的玩家。
#[tauri::command]
pub async fn search_players(
    state: State<'_, AppState>,
    query: String,
    limit: Option<u32>,
    offset: Option<u32>,
    sort: Option<String>,
    filter: Option<String>,
) -> Result<Vec<PlayerHit>, String> {
    state
        .engine
        .search_players_filtered(
            query,
            limit.unwrap_or(50),
            offset.unwrap_or(0),
            sort.unwrap_or_else(|| "recent".into()),
            filter.unwrap_or_else(|| "all".into()),
        )
        .await
        .map_err(rejected("search_players"))
}

#[tauri::command]
pub async fn search_batrace_players(
    state: State<'_, AppState>,
    query: String,
) -> Result<bax_engine::PlayerSearchResponse, String> {
    state
        .engine
        .search_batrace_players(&query)
        .await
        .map_err(rejected("search_batrace_players"))
}

#[tauri::command]
pub async fn save_batrace_player(
    state: State<'_, AppState>,
    hit: bax_engine::PlayerSearchHit,
) -> CommandResult {
    state
        .engine
        .save_batrace_player(hit)
        .await
        .map_err(rejected("save_batrace_player"))
}

#[tauri::command]
pub async fn set_player_favorite(
    state: State<'_, AppState>,
    id: u64,
    favorite: bool,
) -> CommandResult {
    state
        .engine
        .set_player_favorite(PlayerId(id), favorite)
        .await
        .map_err(rejected("set_player_favorite"))
}

/// `body` 为空时删除备注。
#[tauri::command]
pub async fn set_note(state: State<'_, AppState>, id: u64, body: String) -> CommandResult {
    state
        .engine
        .set_note(PlayerId(id), body)
        .await
        .map_err(rejected("set_note"))
}

#[tauri::command]
pub async fn refresh_player(state: State<'_, AppState>, id: u64) -> CommandResult {
    state
        .engine
        .refresh_player(PlayerId(id))
        .await
        .map_err(rejected("refresh_player"))
}

/// 比赛记录，新的在前。
#[tauri::command]
pub async fn match_history(
    state: State<'_, AppState>,
    limit: Option<u32>,
    offset: Option<u32>,
    query: Option<String>,
    player: Option<PlayerId>,
) -> Result<Vec<HistoryRow>, String> {
    state
        .engine
        .search_match_history(
            limit.unwrap_or(100),
            offset.unwrap_or(0),
            query.unwrap_or_default(),
            player,
        )
        .await
        .map_err(rejected("match_history"))
}

/// 一场比赛的战后分析；还没有战报时为 null。
#[tauri::command]
pub async fn match_report(
    state: State<'_, AppState>,
    id: u64,
) -> Result<Option<MatchReportView>, String> {
    state
        .engine
        .match_report(MatchId(id))
        .await
        .map_err(rejected("match_report"))
}

/// 立即去取一场比赛的战报。
#[tauri::command]
pub async fn fetch_report(state: State<'_, AppState>, id: u64) -> CommandResult {
    state
        .engine
        .fetch_report(MatchId(id))
        .await
        .map_err(rejected("fetch_report"))
}

/// 导入日志目录里的全部历史日志。
#[tauri::command]
pub async fn import_history(state: State<'_, AppState>) -> Result<ImportSummary, String> {
    state
        .engine
        .import_history()
        .await
        .map_err(rejected("import_history"))
}

/// 窗口要在异步命令里创建：同步命令跑在主线程上，在那里建窗口会卡住事件循环。
#[tauri::command]
pub async fn open_main_window(app: AppHandle) -> CommandResult {
    show_main_window(&app);
    Ok(())
}

// ---------------------------------------------------------------- 诊断（ADR-0007）

/// 每个窗口每分钟最多记录的前端错误条数。
const UI_ERRORS_PER_MINUTE: u32 = 20;
const MESSAGE_LIMIT: usize = 2_000;
const STACK_LIMIT: usize = 4_000;

/// 前端错误的限流：按窗口计数，每分钟清零。
#[derive(Debug, Default)]
pub struct UiErrorLimiter {
    windows: Mutex<HashMap<String, Budget>>,
}

#[derive(Debug)]
struct Budget {
    since: Instant,
    count: u32,
    suppressed: u32,
}

impl UiErrorLimiter {
    /// 这一条是否记录；返回值的第二项是上一分钟里因限流没有记录的条数。
    fn admit(&self, window: &str) -> (bool, u32) {
        let Ok(mut windows) = self.windows.lock() else {
            return (false, 0);
        };
        let entry = windows.entry(window.to_owned()).or_insert(Budget {
            since: Instant::now(),
            count: 0,
            suppressed: 0,
        });
        let mut carried = 0;
        if entry.since.elapsed() >= Duration::from_secs(60) {
            carried = std::mem::take(&mut entry.suppressed);
            entry.since = Instant::now();
            entry.count = 0;
        }
        if entry.count < UI_ERRORS_PER_MINUTE {
            entry.count += 1;
            (true, carried)
        } else {
            entry.suppressed += 1;
            (false, carried)
        }
    }
}

fn truncate(text: &str, limit: usize) -> &str {
    match text.char_indices().nth(limit) {
        Some((at, _)) => &text[..at],
        None => text,
    }
}

/// 前端未捕获的异常、未处理的 Promise 拒绝和 Vue 渲染错误。前端不写文件，交给这里记录。
#[tauri::command]
pub fn report_ui_error(
    window: Window,
    limiter: State<'_, UiErrorLimiter>,
    kind: String,
    message: String,
    stack: Option<String>,
) {
    let label = window.label().to_owned();
    let (admitted, suppressed) = limiter.admit(&label);
    if suppressed > 0 {
        tracing::warn!(target: "bax_ui", window = %label, suppressed, "前端错误过多，上一分钟有部分没有记录");
    }
    if !admitted {
        return;
    }
    let kind = truncate(&kind, 32);
    let message = truncate(&message, MESSAGE_LIMIT);
    match stack.as_deref().map(|s| truncate(s, STACK_LIMIT)) {
        Some(stack) => {
            tracing::warn!(target: "bax_ui", window = %label, kind = %kind, stack = %stack, "前端错误：{message}")
        }
        None => {
            tracing::warn!(target: "bax_ui", window = %label, kind = %kind, "前端错误：{message}")
        }
    }
}

#[tauri::command]
pub fn open_diag_dir(state: State<'_, AppState>) -> CommandResult {
    let dir = state.diag.dir().ok_or("没有写诊断日志")?;
    reveal_in_explorer(dir, false).map_err(|e| format!("无法打开资源管理器：{e}"))
}

/// 导出最近两次运行的诊断包，在资源管理器里选中它，返回文件路径。
#[tauri::command]
pub async fn export_diag(state: State<'_, AppState>) -> Result<String, String> {
    let Some(dir) = state.diag.dir().map(PathBuf::from) else {
        return Err("没有写诊断日志".to_owned());
    };
    tracing::info!("导出诊断包");
    state.diag.flush();
    let exported =
        tauri::async_runtime::spawn_blocking(move || diag::export(&dir, &ExportOptions::default()))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| format!("导出失败：{e}"))?;
    if let Err(err) = reveal_in_explorer(&exported.path, true) {
        tracing::warn!(error = %err, "无法在资源管理器里显示诊断包");
    }
    Ok(exported.path.display().to_string())
}
