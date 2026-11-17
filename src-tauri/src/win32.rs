//! Tauri 没有提供的 Win32 能力。这是整个项目里唯一允许 `unsafe` 的地方。
//!
//! Tauri（tao）对已经创建的窗口调用 `show()` 时用的是 `SW_SHOW`，会激活窗口；
//! 用快捷键显示悬浮窗时，Windows 允许收到快捷键的程序抢前台，于是焦点会从游戏移走。
//! 这里改用 `SW_SHOWNOACTIVATE`。

#![allow(unsafe_code)]

use std::ffi::c_void;

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::System::Diagnostics::Debug::Beep;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    HWND_TOPMOST, SW_HIDE, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetWindowPos,
    ShowWindow,
};

/// 显示窗口但不激活它，并保持在最上层。
pub fn show_without_activating(hwnd: *mut c_void) {
    let hwnd = hwnd as HWND;
    // SAFETY: 句柄来自 Tauri 窗口，窗口存活期间有效。即使句柄失效，这两个函数也只返回失败，不会访问无效内存。
    unsafe {
        ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
        );
    }
}

pub fn hide(hwnd: *mut c_void) {
    // SAFETY: 同上。
    unsafe {
        ShowWindow(hwnd as HWND, SW_HIDE);
    }
}

/// 响一声。会阻塞到响完，不要在界面线程上调用。
pub fn beep(frequency: u32, duration_ms: u32) {
    // SAFETY: 只接受两个整数参数，没有指针。
    unsafe {
        Beep(frequency, duration_ms);
    }
}
