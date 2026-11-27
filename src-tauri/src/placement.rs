//! 窗口几何独立于引擎快照，拖动或缩放结束后才写入数据目录。
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewWindow, WindowEvent};

const DELAY: Duration = Duration::from_millis(450);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct Placement {
    monitor: Option<String>,
    // 相对显示器工作区的逻辑坐标，宽高是客户区尺寸。
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    maximized: bool,
}

impl Placement {
    fn normal(window: &WebviewWindow) -> Option<Self> {
        let monitor = window.current_monitor().ok()??;
        let origin = monitor.work_area().position;
        let position = window.outer_position().ok()?;
        let scale = monitor.scale_factor();
        let size = window.inner_size().ok()?.to_logical::<f64>(scale);
        (size.width > 0.0 && size.height > 0.0).then(|| Self {
            monitor: monitor.name().cloned(),
            x: (f64::from(position.x) - f64::from(origin.x)) / scale,
            y: (f64::from(position.y) - f64::from(origin.y)) / scale,
            width: size.width,
            height: size.height,
            maximized: false,
        })
    }

    fn restore(&self, window: &WebviewWindow) -> tauri::Result<bool> {
        if self.width <= 0.0 || self.height <= 0.0 {
            return Ok(false);
        }
        let monitor = window
            .available_monitors()?
            .into_iter()
            .find(|m| self.monitor.is_some() && m.name() == self.monitor.as_ref())
            .or(window.primary_monitor()?);
        let Some(monitor) = monitor else {
            return Ok(false);
        };
        let area = monitor.work_area();
        // 先移到目标屏，让系统按该屏 DPI 建立窗口边框。
        window.set_position(area.position)?;
        let inner = window.inner_size()?;
        let outer = window.outer_size()?;
        let scale = monitor.scale_factor();
        let (x, width) = axis(
            self.x,
            self.width,
            scale,
            area.size.width,
            outer.width.saturating_sub(inner.width),
        );
        let (y, height) = axis(
            self.y,
            self.height,
            scale,
            area.size.height,
            outer.height.saturating_sub(inner.height),
        );
        window.set_size(PhysicalSize::new(width, height))?;
        window.set_position(PhysicalPosition::new(
            area.position.x + x,
            area.position.y + y,
        ))?;
        Ok(true)
    }
}

fn axis(offset: f64, inner: f64, scale: f64, available: u32, frame: u32) -> (i32, u32) {
    let limit = available.saturating_sub(frame);
    let size = ((inner * scale).round() as u32).clamp(1, limit.max(1));
    let at = (offset * scale)
        .round()
        .clamp(0.0, f64::from(limit.saturating_sub(size)));
    (at as i32, size)
}

#[derive(Debug, Default)]
struct Cache {
    windows: BTreeMap<String, Placement>,
    dirty: bool,
    last_change: Option<Instant>,
}

#[derive(Debug)]
pub struct WindowMemory {
    path: PathBuf,
    cache: Mutex<Cache>,
}

impl WindowMemory {
    pub fn load(dir: &Path) -> Self {
        let path = dir.join("window-placement.json");
        let windows = match std::fs::File::open(&path) {
            Ok(file) => serde_json::from_reader(file).unwrap_or_else(|err| {
                tracing::warn!(error = %err, "无法读取窗口布局");
                BTreeMap::new()
            }),
            Err(_) => BTreeMap::new(),
        };
        Self {
            path,
            cache: Mutex::new(Cache {
                windows,
                ..Cache::default()
            }),
        }
    }

    fn flush(&self, cache: &mut Cache) {
        if !cache.dirty {
            return;
        }
        let write = || -> std::io::Result<()> {
            let mut file = tempfile::NamedTempFile::new_in(self.path.parent().expect("数据目录"))?;
            serde_json::to_writer(file.as_file_mut(), &cache.windows)?;
            file.persist(&self.path)?;
            Ok(())
        };
        match write() {
            Ok(()) => cache.dirty = false,
            Err(err) => tracing::warn!(error = %err, "保存窗口布局失败"),
        }
    }
}

// 在主线程读取原生状态。最小化时 Windows 可能给出 (-32000, -32000)。
fn capture(window: &WebviewWindow) {
    #[cfg(windows)]
    let (minimized, maximized) = match window.hwnd() {
        Ok(hwnd) => crate::win32::layout_flags(hwnd.0),
        Err(_) => return,
    };
    #[cfg(not(windows))]
    let (minimized, maximized) = match (window.is_minimized(), window.is_maximized()) {
        (Ok(minimized), Ok(maximized)) => (minimized, maximized),
        _ => return,
    };
    if minimized {
        return;
    }
    let normal = if maximized {
        None
    } else {
        Placement::normal(window)
    };
    let memory = window.state::<WindowMemory>();
    let mut cache = memory.cache.lock().expect("窗口布局");
    let old = cache.windows.get(window.label());
    let next = if maximized {
        old.cloned().map(|p| Placement {
            maximized: true,
            ..p
        })
    } else {
        normal
    };
    let Some(next) = next else { return };
    if old == Some(&next) {
        return;
    }
    cache.windows.insert(window.label().to_owned(), next);
    cache.dirty = true;
    if cache.last_change.replace(Instant::now()).is_some() {
        return;
    }

    let app = window.app_handle().clone();
    tauri::async_runtime::spawn(async move {
        let mut delay = DELAY;
        loop {
            tokio::time::sleep(delay).await;
            let memory = app.state::<WindowMemory>();
            let mut cache = memory.cache.lock().expect("窗口布局");
            delay = cache
                .last_change
                .map_or(Duration::ZERO, |at| DELAY.saturating_sub(at.elapsed()));
            if delay.is_zero() {
                memory.flush(&mut cache);
                cache.last_change = None;
                return;
            }
        }
    });
}

/// 窗口仍隐藏时恢复，然后才监听用户操作。
pub fn attach(window: &WebviewWindow, fallback: impl FnOnce() -> tauri::Result<()>) {
    let memory = window.state::<WindowMemory>();
    let saved = memory
        .cache
        .lock()
        .expect("窗口布局")
        .windows
        .get(window.label())
        .cloned();
    let restored = saved.as_ref().is_some_and(|p| match p.restore(window) {
        Ok(restored) => restored,
        Err(err) => {
            tracing::warn!(error = %err, label = window.label(), "无法恢复窗口布局");
            false
        }
    });
    if !restored && let Err(err) = fallback() {
        tracing::warn!(error = %err, label = window.label(), "无法设置默认窗口位置");
    }
    capture(window);
    if restored && saved.is_some_and(|p| p.maximized) {
        if let Err(err) = window.maximize() {
            tracing::warn!(error = %err, "无法恢复最大化");
        } else {
            capture(window);
        }
    }
    let app = window.app_handle().clone();
    let label = window.label().to_owned();
    window.on_window_event(move |event| {
        if matches!(
            event,
            WindowEvent::Moved(_)
                | WindowEvent::Resized(_)
                | WindowEvent::ScaleFactorChanged { .. }
                | WindowEvent::CloseRequested { .. }
        ) && let Some(window) = app.get_webview_window(&label)
        {
            capture(&window);
            if matches!(event, WindowEvent::CloseRequested { .. }) {
                flush(&app);
            }
        }
    });
}

pub fn flush(app: &AppHandle) {
    let memory = app.state::<WindowMemory>();
    let mut cache = memory.cache.lock().expect("窗口布局");
    memory.flush(&mut cache);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scales_monitor_relative_bounds_and_keeps_the_frame_visible() {
        assert_eq!(axis(80., 600., 1.5, 1920, 24), (120, 900));
        assert_eq!(axis(900., 600., 1.5, 1024, 24), (100, 900));
        assert_eq!(axis(-30., 1200., 1., 1024, 16), (0, 1008));
    }
}
