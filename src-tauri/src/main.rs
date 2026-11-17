//! BAX 桌面应用：Tauri 外壳。
//!
//! 业务逻辑都在 `bax-engine` 里。这里只做三件事：把引擎的快照转发给窗口、把窗口的命令转给引擎，
//! 以及管理悬浮窗、托盘和全局快捷键（ADR-0003、ADR-0004）。

// 发布版不弹出控制台窗口
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod commands;
mod hotkeys;
mod overlay;
mod tray;
#[cfg(windows)]
mod win32;

fn main() {
    app::run();
}
