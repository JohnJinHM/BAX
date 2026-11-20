//! 签名更新只属于桌面外壳，不进入玩家资料引擎。
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Manager, Window};
use tauri_plugin_updater::{Update, UpdaterExt};
use tokio::sync::Mutex;

use crate::app::{AppState, MAIN_LABEL};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    pub current_version: String,
    pub edition: String,
    pub can_install: bool,
    pub state: String,
    pub version: Option<String>,
    pub notes: Option<String>,
    pub downloaded: u64,
    pub total: Option<u64>,
    pub message: String,
}

impl Default for UpdateStatus {
    fn default() -> Self {
        Self {
            current_version: env!("CARGO_PKG_VERSION").into(),
            edition: if cfg!(feature = "ocr") {
                "Full"
            } else {
                "Lite"
            }
            .into(),
            can_install: cfg!(all(
                feature = "installed-updater",
                not(debug_assertions),
                windows
            )),
            state: "idle".into(),
            version: None,
            notes: None,
            downloaded: 0,
            total: None,
            message: "尚未检查更新".into(),
        }
    }
}

#[derive(Default)]
pub struct PendingUpdate(Mutex<Option<Update>>);

impl std::fmt::Debug for PendingUpdate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PendingUpdate").finish_non_exhaustive()
    }
}

fn publish(app: &AppHandle, change: impl FnOnce(&mut UpdateStatus)) {
    app.state::<AppState>()
        .update_status(app, |s| change(&mut s.update));
}

pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(10)).await;
        let _ = check(&app).await;
    });
}

async fn check(app: &AppHandle) -> Result<(), String> {
    let pending = app.state::<PendingUpdate>();
    let mut pending = pending.0.try_lock().map_err(|_| "更新操作正在进行")?;
    *pending = None;
    publish(app, |s| {
        s.state = "checking".into();
        s.message = "正在检查 GitHub 更新…".into();
        s.version = None;
        s.notes = None;
        s.downloaded = 0;
        s.total = None;
    });
    let result = async {
        let target = if cfg!(feature = "ocr") {
            "windows-x86_64-full"
        } else {
            "windows-x86_64-lite"
        };
        app.updater_builder()
            .target(target)
            .timeout(Duration::from_secs(20))
            .build()?
            .check()
            .await
    }
    .await;
    match result {
        Ok(Some(mut update)) => {
            update.timeout = Some(Duration::from_secs(300));
            publish(app, |s| {
                s.state = "available".into();
                s.version = Some(update.version.clone());
                s.notes = update.body.clone();
                s.message = format!("发现新版本 {}", update.version);
            });
            *pending = Some(update);
            Ok(())
        }
        Ok(None) => {
            publish(app, |s| {
                s.state = "current".into();
                s.message = "当前已是最新版本".into();
            });
            Ok(())
        }
        Err(error) => {
            let message = if matches!(error, tauri_plugin_updater::Error::ReleaseNotFound) {
                "更新源尚不可用：可能还没有公开 Release，或 GitHub 暂时不可达。可稍后重试。".into()
            } else {
                format!("检查更新失败，请稍后重试：{error}")
            };
            publish(app, |s| {
                s.state = "error".into();
                s.message = message;
            });
            Err("检查更新失败".into())
        }
    }
}

fn main_only(window: &Window) -> Result<(), String> {
    if window.label() == MAIN_LABEL {
        Ok(())
    } else {
        Err("请在主窗口设置页操作更新".into())
    }
}

#[tauri::command]
pub async fn check_update(app: AppHandle, window: Window) -> Result<(), String> {
    main_only(&window)?;
    check(&app).await
}

fn match_active(app: &AppHandle) -> bool {
    app.state::<AppState>()
        .engine
        .snapshot()
        .current
        .as_ref()
        .is_some_and(|m| blocks_install(m.phase, m.connection))
}

fn blocks_install(phase: bax_core::Phase, connection: bax_core::Connection) -> bool {
    phase != bax_core::Phase::Ended && connection != bax_core::Connection::Disconnected
}

#[tauri::command]
pub async fn install_update(app: AppHandle, window: Window) -> Result<(), String> {
    main_only(&window)?;
    if !UpdateStatus::default().can_install {
        return Err("此构建请从发布页手动更新".into());
    }
    if match_active(&app) {
        return Err("请在本场比赛结束后安装更新".into());
    }
    let pending = app.state::<PendingUpdate>();
    let pending = pending.0.try_lock().map_err(|_| "更新操作正在进行")?;
    let update = pending.as_ref().ok_or("请先检查更新")?;
    publish(&app, |s| {
        s.state = "downloading".into();
        s.downloaded = 0;
        s.total = None;
        s.message = "正在下载并验证签名…".into();
    });
    let result: Result<(), String> = async {
        let mut downloaded = 0u64;
        let bytes = update
            .download(
                |chunk, total| {
                    downloaded = downloaded.saturating_add(chunk as u64);
                    publish(&app, |s| {
                        s.downloaded = downloaded;
                        s.total = total;
                    });
                },
                || {},
            )
            .await
            .map_err(|e| format!("下载或签名验证失败：{e}"))?;
        if match_active(&app) {
            return Err("已进入对局，暂缓安装；比赛结束后请重试".into());
        }
        publish(&app, |s| {
            s.state = "installing".into();
            s.message = "正在退出 BAX 并启动安装程序…".into();
        });
        app.state::<AppState>().diag.flush();
        update.install(bytes).map_err(|e| format!("安装失败：{e}"))
    }
    .await;
    if let Err(message) = &result {
        publish(&app, |s| {
            s.state = "error".into();
            s.message = message.clone();
        });
    }
    result
}

#[tauri::command]
pub fn open_releases(window: Window) -> Result<(), String> {
    main_only(&window)?;
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("rundll32.exe")
            .args([
                "url.dll,FileProtocolHandler",
                "https://github.com/JohnJinHM/BAX/releases",
            ])
            .creation_flags(0x08000000)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installation_waits_for_match_end_or_room_exit() {
        use bax_core::{Connection::*, Phase::*};
        for phase in [WaitingRoom, Loading, Deployment, InBattle] {
            assert!(blocks_install(phase, Connected));
            assert!(blocks_install(phase, Reconnecting));
            assert!(!blocks_install(phase, Disconnected));
        }
        assert!(!blocks_install(Ended, Connected));
    }

    #[test]
    fn updater_protocol_keeps_editions_separate_and_rejects_missing_targets() {
        let release: tauri_plugin_updater::RemoteRelease = serde_json::from_value(serde_json::json!({
            "version": "0.2.1", "notes": "fixture", "platforms": {
                "windows-x86_64-full": { "url": "https://github.com/JohnJinHM/BAX/releases/download/v0.2.1/BAX-Full_0.2.1_x64-setup.exe", "signature": "full-fixture" },
                "windows-x86_64-lite": { "url": "https://github.com/JohnJinHM/BAX/releases/download/v0.2.1/BAX-Lite_0.2.1_x64-setup.exe", "signature": "lite-fixture" }
            }
        })).expect("有效更新协议");
        assert!(
            release
                .download_url("windows-x86_64-full")
                .expect("Full")
                .path()
                .contains("BAX-Full")
        );
        assert_eq!(
            release.signature("windows-x86_64-lite").expect("Lite"),
            "lite-fixture"
        );
        assert!(release.download_url("windows-aarch64-full").is_err());
    }
}
