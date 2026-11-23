//! Tauri 没有提供的 Win32 能力。这是整个项目里唯一允许 `unsafe` 的地方。
//!
//! Tauri（tao）对已经创建的窗口调用 `show()` 时用的是 `SW_SHOW`，会激活窗口；
//! 用快捷键显示悬浮窗时，Windows 允许收到快捷键的程序抢前台，于是焦点会从游戏移走。
//! 这里改用 `SW_SHOWNOACTIVATE`。
//!
//! 鼠标穿透也不用 tao 的 `set_ignore_cursor_events`：它改窗口样式时会按 tao 自己记的可见状态
//! 重新显示或隐藏窗口，而悬浮窗是用上面的 `SW_SHOWNOACTIVATE` 显示的，tao 一直以为它是隐藏的，
//! 于是一锁定窗口就被藏起来；它还加上 `WS_EX_LAYERED` 却不设置分层属性，分层窗口不设置属性时不会绘制。
//!
//! 锁定后悬浮窗上的锁按钮仍要能点：外壳轮询鼠标位置，鼠标在按钮上时暂时去掉 `WS_EX_TRANSPARENT`，
//! 离开后再加上（见 `overlay` 模块）。
//!
//! 悬浮窗贴边时要知道拖动是否结束（鼠标键是否松开），并且一次改完位置和大小。

#![allow(unsafe_code)]

use std::ffi::c_void;

use windows_sys::Win32::Foundation::{CloseHandle, HWND, POINT, RECT};
use windows_sys::Win32::System::Diagnostics::Debug::Beep;
use windows_sys::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON, VK_RBUTTON};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GWL_EXSTYLE, GetCursorPos, GetForegroundWindow, GetWindowLongW, GetWindowRect,
    GetWindowThreadProcessId, HWND_TOPMOST, IsIconic, IsWindow, IsWindowVisible, LWA_ALPHA,
    SW_HIDE, SW_SHOWNOACTIVATE, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOOWNERZORDER,
    SWP_NOSIZE, SWP_NOZORDER, SetLayeredWindowAttributes, SetWindowLongW, SetWindowPos, ShowWindow,
    WS_EX_LAYERED, WS_EX_TOPMOST, WS_EX_TRANSPARENT,
};

#[derive(Debug, Clone, Copy)]
pub struct WindowState {
    pub visible: bool,
    pub minimized: bool,
    pub topmost: bool,
}

impl WindowState {
    pub fn shown(self) -> bool {
        self.visible && !self.minimized
    }

    pub fn healthy(self) -> bool {
        self.shown() && self.topmost
    }
}

/// 直接读取系统状态，不能用 tao 内部缓存的可见标志；不代表窗口未被遮挡。
pub fn window_state(hwnd: *mut c_void) -> Option<WindowState> {
    // SAFETY: 只读系统窗口状态；无效句柄返回 None。
    unsafe {
        (IsWindow(hwnd) != 0).then(|| WindowState {
            visible: IsWindowVisible(hwnd) != 0,
            minimized: IsIconic(hwnd) != 0,
            topmost: GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST != 0,
        })
    }
}

/// 前台句柄与矩形用于检测窗口切换，不读取标题或进程信息。
pub fn foreground_context() -> (usize, [i32; 4]) {
    // SAFETY: RECT 在栈上；无前台窗口或取矩形失败时按空状态返回。
    unsafe {
        let hwnd = GetForegroundWindow();
        let mut rect: RECT = std::mem::zeroed();
        if !hwnd.is_null() {
            GetWindowRect(hwnd, &mut rect);
        }
        (
            hwnd as usize,
            [rect.left, rect.top, rect.right, rect.bottom],
        )
    }
}

/// 显示或恢复窗口并重新置顶，不激活它。已经显示时只调整 Z 顺序。
pub fn show_without_activating(hwnd: *mut c_void) -> bool {
    let hwnd = hwnd as HWND;
    let Some(before) = window_state(hwnd) else {
        return false;
    };
    // SAFETY: 句柄来自 Tauri 窗口，窗口存活期间有效。即使句柄失效，这两个函数也只返回失败，不会访问无效内存。
    unsafe {
        if !before.shown() {
            // ShowWindow 返回的是此前是否可见，不是操作是否成功。
            ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }
        let placed = SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE | SWP_NOOWNERZORDER,
        ) != 0;
        placed && window_state(hwnd).is_some_and(WindowState::healthy)
    }
}

pub fn hide(hwnd: *mut c_void) -> bool {
    // SAFETY: 同上。
    unsafe {
        ShowWindow(hwnd as HWND, SW_HIDE);
    }
    window_state(hwnd).is_some_and(|state| !state.visible)
}

/// 打开或关闭鼠标穿透。打开时窗口成为不透明度 100% 的分层窗口，点击落到下面的窗口上；
/// 不改变窗口是否可见。返回是否成功。
pub fn set_click_through(hwnd: *mut c_void, enabled: bool) -> bool {
    let hwnd = hwnd as HWND;
    // SAFETY: 句柄来自 Tauri 窗口，窗口存活期间有效。这几个函数只读写窗口样式，句柄失效时返回失败。
    unsafe {
        let old = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
        let bits = WS_EX_LAYERED | WS_EX_TRANSPARENT;
        let new = if enabled { old | bits } else { old & !bits };
        if new != old {
            SetWindowLongW(hwnd, GWL_EXSTYLE, new as i32);
        }
        let layered_ok = !enabled || SetLayeredWindowAttributes(hwnd, 0, 255, LWA_ALPHA) != 0;
        SetWindowPos(
            hwnd,
            std::ptr::null_mut(),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
        let applied = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 & bits;
        layered_ok && applied == if enabled { bits } else { 0 }
    }
}

/// 只切换 `WS_EX_TRANSPARENT`：`true` 时点击穿透，`false` 时窗口接收鼠标。
/// 用于锁定期间鼠标经过锁按钮的那一小会儿，分层属性保持不变，窗口不重绘。
/// 会给窗口发样式变化的消息，要在窗口所在的线程（主线程）上调用。
pub fn set_passthrough(hwnd: *mut c_void, enabled: bool) {
    let hwnd = hwnd as HWND;
    // SAFETY: 同 `set_click_through`。
    unsafe {
        let old = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
        let new = if enabled {
            old | WS_EX_TRANSPARENT
        } else {
            old & !WS_EX_TRANSPARENT
        };
        if new != old {
            SetWindowLongW(hwnd, GWL_EXSTYLE, new as i32);
        }
    }
}

/// 鼠标相对窗口左上角的位置，物理像素。悬浮窗没有边框，窗口左上角就是页面左上角。
/// 只读取系统状态，可以在任何线程调用。
pub fn cursor_in_window(hwnd: *mut c_void) -> Option<(i32, i32)> {
    let hwnd = hwnd as HWND;
    // SAFETY: 两个结构体都在栈上，指针只在调用期间使用；句柄失效时函数返回失败。
    unsafe {
        let mut cursor = POINT { x: 0, y: 0 };
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        if GetCursorPos(&mut cursor) == 0 || GetWindowRect(hwnd, &mut rect) == 0 {
            return None;
        }
        Some((cursor.x - rect.left, cursor.y - rect.top))
    }
}

/// 鼠标左键或右键是否按着。看的是物理按键：左右键对调后，系统拖动窗口用的是物理右键。
/// 只读取系统状态，可以在任何线程调用。
pub fn mouse_button_down() -> bool {
    // SAFETY: 只接受一个整数参数，没有指针。
    unsafe {
        [VK_LBUTTON, VK_RBUTTON]
            .into_iter()
            .any(|key| GetAsyncKeyState(i32::from(key)) as u16 & 0x8000 != 0)
    }
}

/// 一次改窗口的位置和大小，物理像素；不激活窗口，不改层次。返回是否成功。
/// 会给窗口发消息，要在主线程上调用。
pub fn set_bounds(hwnd: *mut c_void, x: i32, y: i32, width: i32, height: i32) -> bool {
    // SAFETY: 句柄来自 Tauri 窗口，窗口存活期间有效；句柄失效时函数返回失败，没有指针参数。
    unsafe {
        SetWindowPos(
            hwnd as HWND,
            std::ptr::null_mut(),
            x,
            y,
            width,
            height,
            SWP_NOZORDER | SWP_NOACTIVATE,
        ) != 0
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

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::System::StationsAndDesktops::{
        CloseDesktop, CreateDesktopW, GetThreadDesktop, HDESK, SetThreadDesktop,
    };
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetActiveWindow;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, GW_HWNDPREV, GetWindow, HWND_NOTOPMOST, SW_SHOWMINNOACTIVE,
        WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP,
    };

    struct Desktop {
        handle: HDESK,
        original: HDESK,
    }

    impl Drop for Desktop {
        fn drop(&mut self) {
            // SAFETY: 所有测试窗口先销毁；original 是系统借出的句柄，不关闭它。
            unsafe {
                SetThreadDesktop(self.original);
                CloseDesktop(self.handle);
            }
        }
    }

    struct TestWindow(HWND);

    impl Drop for TestWindow {
        fn drop(&mut self) {
            // SAFETY: 窗口由本线程创建，RAII 保证只销毁一次。
            unsafe {
                DestroyWindow(self.0);
            }
        }
    }

    fn above(first: HWND, second: HWND) -> bool {
        // SAFETY: 只遍历本测试桌面的窗口顺序，遇到空句柄终止。
        unsafe {
            let mut current = GetWindow(second, GW_HWNDPREV);
            while !current.is_null() {
                if current == first {
                    return true;
                }
                current = GetWindow(current, GW_HWNDPREV);
            }
            false
        }
    }

    #[test]
    fn native_recovery_preserves_focus_geometry_and_click_through() {
        // 新线程绑定到未切换显示的私有桌面，不在用户游戏画面上创建测试窗口。
        std::thread::spawn(|| {
            // SAFETY: 句柄都由本线程创建；桌面从不调用 SwitchDesktop，窗口由 RAII 先于桌面释放。
            unsafe {
                let name: Vec<u16> = format!(
                    "BAX-overlay-test-{}-{}\0",
                    std::process::id(),
                    GetCurrentThreadId()
                )
                .encode_utf16()
                .collect();
                let desktop = Desktop {
                    original: GetThreadDesktop(GetCurrentThreadId()),
                    handle: CreateDesktopW(
                        name.as_ptr(),
                        std::ptr::null(),
                        std::ptr::null(),
                        0,
                        0x01ff,
                        std::ptr::null(),
                    ),
                };
                assert!(
                    !desktop.handle.is_null(),
                    "{}",
                    std::io::Error::last_os_error()
                );
                assert_ne!(SetThreadDesktop(desktop.handle), 0);
                let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
                let create = || {
                    let hwnd = CreateWindowExW(
                        WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
                        class.as_ptr(),
                        class.as_ptr(),
                        WS_POPUP,
                        100,
                        100,
                        340,
                        120,
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                        std::ptr::null(),
                    );
                    assert!(!hwnd.is_null());
                    TestWindow(hwnd)
                };
                let overlay = create();
                let blocker = create();
                let foreground = GetForegroundWindow();
                let active = GetActiveWindow();
                let mut rect: RECT = std::mem::zeroed();
                assert_ne!(GetWindowRect(overlay.0, &mut rect), 0);

                assert!(!window_state(overlay.0).expect("window").shown());
                assert!(show_without_activating(overlay.0));
                assert!(show_without_activating(blocker.0));
                assert!(above(blocker.0, overlay.0));
                assert!(window_state(overlay.0).expect("window").topmost);
                assert!(show_without_activating(overlay.0));
                assert!(
                    above(overlay.0, blocker.0),
                    "置顶标志不变时也必须恢复 Z 顺序"
                );

                assert_ne!(
                    SetWindowPos(
                        overlay.0,
                        HWND_NOTOPMOST,
                        0,
                        0,
                        0,
                        0,
                        SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE
                    ),
                    0
                );
                assert!(!window_state(overlay.0).expect("window").topmost);
                assert!(show_without_activating(overlay.0));
                ShowWindow(overlay.0, SW_SHOWMINNOACTIVE);
                assert!(window_state(overlay.0).expect("window").minimized);
                assert!(show_without_activating(overlay.0));

                assert!(set_click_through(overlay.0, true));
                assert!(hide(overlay.0));
                assert!(!window_state(overlay.0).expect("window").shown());
                assert!(show_without_activating(overlay.0));
                let style = GetWindowLongW(overlay.0, GWL_EXSTYLE) as u32;
                assert_eq!(
                    style & (WS_EX_LAYERED | WS_EX_TRANSPARENT),
                    WS_EX_LAYERED | WS_EX_TRANSPARENT
                );
                assert_eq!(GetForegroundWindow(), foreground);
                assert_eq!(GetActiveWindow(), active);
                let mut after: RECT = std::mem::zeroed();
                assert_ne!(GetWindowRect(overlay.0, &mut after), 0);
                assert_eq!(
                    [after.left, after.top, after.right, after.bottom],
                    [rect.left, rect.top, rect.right, rect.bottom]
                );
                assert!(!show_without_activating(std::ptr::null_mut()));
                assert!(!hide(std::ptr::null_mut()));
            }
        })
        .join()
        .expect("私有桌面原生窗口回归");
    }
}
