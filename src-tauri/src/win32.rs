//! Tauri 没有提供的 Win32 能力。这是整个项目里唯一允许 `unsafe` 的地方。
//!
//! Tauri（tao）对已经创建的窗口调用 `show()` 时用的是 `SW_SHOW`，会激活窗口；
//! 用快捷键显示悬浮窗时，Windows 允许收到快捷键的程序抢前台，于是焦点会从游戏移走。
//! 这里改用 `SW_SHOWNOACTIVATE`。

#![allow(unsafe_code)]

use std::ffi::c_void;

use windows_sys::Win32::Foundation::{CloseHandle, HWND};
use windows_sys::Win32::System::Diagnostics::Debug::Beep;
use windows_sys::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowThreadProcessId, HWND_TOPMOST, SW_HIDE, SW_SHOWNOACTIVATE,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetWindowPos, ShowWindow,
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

/// 前台窗口所属程序的文件名，例如 `BrokenArrow.exe`。取不到时为 `None`。只用于诊断日志。
pub fn foreground_process_name() -> Option<String> {
    // SAFETY: 这几个函数只读取系统状态。缓冲区的长度如实传给 QueryFullProcessImageNameW，
    // 它写入不超过这个长度；打开的进程句柄在返回前关闭。
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == 0 {
            return None;
        }
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return None;
        }
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok =
            QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len);
        CloseHandle(process);
        if ok == 0 {
            return None;
        }
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        // 只要文件名：完整路径里有用户目录
        path.rsplit(['\\', '/']).next().map(str::to_owned)
    }
}

/// 响一声。会阻塞到响完，不要在界面线程上调用。
pub fn beep(frequency: u32, duration_ms: u32) {
    // SAFETY: 只接受两个整数参数，没有指针。
    unsafe {
        Beep(frequency, duration_ms);
    }
}
