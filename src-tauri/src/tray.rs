//! 托盘图标与菜单。BAX 常驻托盘，关掉主窗口不退出。

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

use crate::app::{AppState, show_main_window};
use crate::overlay;

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let main = MenuItem::with_id(app, "main", "打开主窗口", true, None::<&str>)?;
    let toggle = MenuItem::with_id(app, "toggle", "显示 / 隐藏悬浮窗", true, None::<&str>)?;
    let lock = MenuItem::with_id(app, "lock", "锁定 / 解锁悬浮窗", true, None::<&str>)?;
    let refresh = MenuItem::with_id(app, "refresh", "刷新玩家资料", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "退出 BAX", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&main, &toggle, &lock, &refresh, &separator, &quit])?;

    let mut builder = TrayIconBuilder::with_id("bax")
        .tooltip("BAX")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "main" => show_main_window(app),
            "toggle" => overlay::toggle_visible(app),
            "lock" => overlay::toggle_locked(app),
            "refresh" => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = app.state::<AppState>().engine.refresh_players().await;
                });
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}
