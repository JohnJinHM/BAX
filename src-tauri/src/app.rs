//! 应用的组装：启动引擎、创建窗口与托盘、转发快照。

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use bax_engine::diag::{self, DiagConfig, DiagHandle, DiagLevel, DiagSettings, StderrOutput};
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
    pub update: crate::updater::UpdateStatus,
    pub ocr_supported: bool,
    /// 系统观测到的显示且未最小化状态；不表示未被其他窗口遮挡。
    pub overlay_visible: bool,
    pub overlay_locked: bool,
    /// 注册失败的快捷键及原因。
    pub hotkey_errors: Vec<String>,
    pub data_dir: String,
    /// 诊断日志目录；不写诊断日志时为空。
    pub diag_dir: String,
    /// 诊断档位被什么锁定（环境变量或启动参数）；没有锁定时为 `None`。
    pub diag_locked_by: Option<String>,
    /// 诊断日志写不了时的原因。
    pub diag_error: Option<String>,
}

#[derive(Debug)]
pub struct AppState {
    pub engine: Engine,
    pub diag: Arc<DiagHandle>,
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
    // --hidden：启动时不显示悬浮窗，只有托盘图标（开机自启或调试时用）
    let start_hidden = std::env::args().any(|arg| arg == "--hidden");
    // --verbose：本次运行使用详细诊断档位，不受设置影响
    let verbose = std::env::args().any(|arg| arg == "--verbose");
    let data_dir = default_data_dir();
    let diag = Arc::new(init_diag(data_dir.as_deref(), verbose, start_hidden));
    tracing::info!(hidden = start_hidden, verbose, "BAX 启动");

    let setup_diag = Arc::clone(&diag);
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(crate::updater::PendingUpdate::default())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // 已经在运行时再次启动：打开主窗口
            tracing::info!("再次启动，打开主窗口");
            show_main_window(app);
        }))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(hotkeys::handle)
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            commands::check_data_sources,
            commands::get_snapshot,
            crate::updater::check_update,
            crate::updater::install_update,
            crate::updater::open_releases,
            commands::get_app_status,
            commands::refresh_players,
            commands::set_log_dir,
            commands::set_reminders,
            commands::reserve_reminder_id,
            commands::calibrate,
            commands::set_ui_setting,
            commands::set_overlay_locked,
            commands::set_overlay_hotspot,
            commands::fit_overlay,
            commands::set_overlay_visible,
            commands::open_main_window,
            commands::open_batrace_player,
            commands::report_ui_error,
            commands::open_diag_dir,
            commands::export_diag,
            commands::player_detail,
            commands::search_players,
            commands::set_note,
            commands::refresh_player,
            commands::import_history,
            commands::match_history,
            commands::match_report,
            commands::fetch_report,
        ])
        .setup(move |app| {
            let data_dir = data_dir.ok_or("无法确定数据目录")?;
            let config = EngineConfig::new(&data_dir);
            #[cfg(feature = "ocr")]
            let config = {
                let mut config = config;
                let bundled_ocr = app.path().resource_dir()?.join("ocr-runtime");
                config.ocr_runtime = Some(
                    if cfg!(debug_assertions) && !bundled_ocr.join("bax-ocr.exe").is_file() {
                        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                            .join("../.local/ocr-dist/bax-ocr")
                    } else {
                        bundled_ocr
                    },
                );
                config
            };
            // 引擎需要在异步运行时里启动；Tauri 自带的就是 tokio
            let engine = tauri::async_runtime::block_on(async {
                Engine::start(config, Arc::new(SystemClock))
            })?;
            let snapshots = engine.subscribe();
            let ui = engine.snapshot().ui.clone();
            apply_diag_setting(&setup_diag, &ui);
            app.manage(AppState {
                engine,
                status: Mutex::new(AppStatus {
                    ocr_supported: cfg!(feature = "ocr"),
                    data_dir: data_dir.display().to_string(),
                    diag_dir: setup_diag
                        .dir()
                        .map(|d| d.display().to_string())
                        .unwrap_or_default(),
                    diag_locked_by: locked_by(&setup_diag, verbose),
                    diag_error: setup_diag.file_error().map(str::to_owned),
                    ..AppStatus::default()
                }),
                diag: setup_diag,
            });
            app.manage(hotkeys::HotkeyState::default());
            app.manage(commands::UiErrorLimiter::default());

            let handle = app.handle();
            overlay::create(handle, &ui, !start_hidden)?;
            tray::create(handle)?;
            hotkeys::apply(handle, &ui);
            forward_snapshots(handle.clone(), snapshots);
            crate::updater::start(handle.clone());
            Ok(())
        })
        .build(tauri::generate_context!());

    let app = match app {
        Ok(app) => app,
        Err(err) => {
            tracing::error!(error = %err, "BAX 启动失败");
            diag.flush();
            std::process::exit(1);
        }
    };
    app.run(move |_app, event| match event {
        RunEvent::ExitRequested { api, code, .. } => {
            // 关掉主窗口不退出：BAX 常驻托盘，只有托盘菜单的“退出”才结束
            if code.is_none() {
                api.prevent_exit();
            }
        }
        RunEvent::Exit => {
            tracing::info!("BAX 退出");
            diag.flush();
        }
        _ => {}
    });
}

/// 诊断日志写到数据目录下的 `diag\`；开发版同时写到标准错误（ADR-0007）。
fn init_diag(data_dir: Option<&Path>, verbose: bool, hidden: bool) -> DiagHandle {
    let mut config = DiagConfig::new("bax-app");
    config.file_dir = data_dir.map(diag::diag_dir);
    config.stderr = if cfg!(debug_assertions) {
        StderrOutput::FollowLevel
    } else {
        StderrOutput::Off
    };
    if verbose {
        config.level = DiagLevel::Verbose;
        config.level_forced = true;
    }
    let webview = tauri::webview_version().unwrap_or_else(|err| format!("未知（{err}）"));
    config.extra = vec![
        ("WebView2".to_owned(), webview),
        (
            "数据目录".to_owned(),
            data_dir.map_or_else(|| "未知".to_owned(), |d| d.display().to_string()),
        ),
        (
            "启动参数".to_owned(),
            match (hidden, verbose) {
                (false, false) => "无".to_owned(),
                _ => [(hidden, "--hidden"), (verbose, "--verbose")]
                    .iter()
                    .filter(|(on, _)| *on)
                    .map(|(_, flag)| *flag)
                    .collect::<Vec<_>>()
                    .join(" "),
            },
        ),
    ];
    diag::init(config)
}

fn locked_by(diag: &DiagHandle, verbose: bool) -> Option<String> {
    if let Some(directives) = diag.env_directives() {
        return Some(format!("环境变量 {}={directives}", diag::ENV_VAR));
    }
    verbose.then(|| "启动参数 --verbose".to_owned())
}

/// 按界面设置 `ui.diag` 切换诊断档位。档位被锁定时不变。
fn apply_diag_setting(diag: &DiagHandle, ui: &BTreeMap<String, serde_json::Value>) {
    let settings: DiagSettings = ui
        .get(diag::SETTINGS_KEY)
        .and_then(|value| serde_json::from_value(value.clone()).ok())
        .unwrap_or_default();
    diag.set_level(settings.level);
}

/// 把引擎的每份快照转发给所有窗口，并处理需要外壳动作的变化（提醒、快捷键与诊断设置）。
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
                tracing::info!(rule_id = alert.rule_id, "提醒触发");
                overlay::on_alert(&app, &snapshot.ui);
            }
            if snapshot.ui != last_ui {
                if snapshot.ui.get(hotkeys::SETTINGS_KEY) != last_ui.get(hotkeys::SETTINGS_KEY) {
                    hotkeys::apply(&app, &snapshot.ui);
                }
                if snapshot.ui.get(diag::SETTINGS_KEY) != last_ui.get(diag::SETTINGS_KEY) {
                    apply_diag_setting(&app.state::<AppState>().diag, &snapshot.ui);
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
    tracing::info!("打开主窗口");
    let built = WebviewWindowBuilder::new(app, MAIN_LABEL, WebviewUrl::App("index.html".into()))
        .title("BAX")
        .inner_size(1120.0, 720.0)
        .min_inner_size(800.0, 480.0)
        .build();
    if let Err(err) = built {
        tracing::error!(error = %err, "无法打开主窗口");
    }
}

/// 在资源管理器里打开目录，或者打开所在目录并选中文件。
pub fn reveal_in_explorer(path: &Path, select: bool) -> std::io::Result<()> {
    let mut command = std::process::Command::new("explorer.exe");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // explorer 自己解析参数，路径要整体加引号，不能交给标准库按常规规则转义
        if select {
            command.raw_arg(format!("/select,\"{}\"", path.display()));
        } else {
            command.raw_arg(format!("\"{}\"", path.display()));
        }
    }
    #[cfg(not(windows))]
    {
        let _ = select;
        command.arg(path);
    }
    // explorer 的退出码不表示成败，不等它
    command.spawn().map(|_| ())
}
