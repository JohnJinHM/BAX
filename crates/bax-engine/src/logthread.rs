//! 日志线程：定位日志目录，按间隔读取新内容，把结果交给对局会话。
//!
//! 读文件是阻塞操作，放在独立的系统线程里，不占用异步运行时。

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use bax_gamelog::{LogWatcher, WatchItem, discovery};
use tokio::sync::mpsc;

/// 日志线程发给会话的消息。
#[derive(Debug)]
pub(crate) enum LogMsg {
    /// 首次自动查找未命中，之后仍定期重试。
    NotFound,
    /// 开始读这个目录。
    Watching {
        dir: PathBuf,
    },
    Item(WatchItem),
    /// 读取失败，之后会继续重试。同样的错误只报告一次。
    Failed {
        message: String,
    },
    /// 读取恢复正常。
    Recovered,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct LogTiming {
    pub poll: Duration,
    pub discovery: Duration,
}

/// 日志线程的句柄。丢弃句柄时线程会在下一次检查时退出。
#[derive(Debug)]
pub(crate) struct LogThread {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl LogThread {
    pub(crate) fn spawn(
        dir: Option<PathBuf>,
        timing: LogTiming,
        tx: mpsc::Sender<LogMsg>,
    ) -> std::io::Result<LogThread> {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let handle = thread::Builder::new()
            .name("bax-log".to_owned())
            .spawn(move || run(dir, timing, &tx, &flag))?;
        Ok(LogThread {
            stop,
            handle: Some(handle),
        })
    }

    /// 通知线程退出，交回线程句柄，调用方可以等它结束。
    pub(crate) fn stop(&mut self) -> Option<JoinHandle<()>> {
        self.stop.store(true, Ordering::SeqCst);
        self.handle.take()
    }
}

impl Drop for LogThread {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

fn run(dir: Option<PathBuf>, timing: LogTiming, tx: &mpsc::Sender<LogMsg>, stop: &AtomicBool) {
    let dir = match dir {
        Some(dir) => {
            tracing::info!(dir = %dir.display(), "使用指定的游戏日志目录");
            dir
        }
        None => {
            tracing::info!("自动定位游戏日志目录");
            let mut attempts = 0u32;
            loop {
                attempts += 1;
                if let Some(found) = discovery::find_log_dir() {
                    tracing::info!(dir = %found.display(), attempts, "找到游戏日志目录");
                    break found;
                }
                if attempts == 1 {
                    if tx.blocking_send(LogMsg::NotFound).is_err() {
                        return;
                    }
                    tracing::info!(
                        candidates = discovery::candidate_log_dirs().len(),
                        "没有找到游戏日志目录，稍后重试"
                    );
                }
                if !sleep_unless_stopped(timing.discovery, stop) {
                    return;
                }
            }
        }
    };
    if tx
        .blocking_send(LogMsg::Watching { dir: dir.clone() })
        .is_err()
    {
        return;
    }

    let mut watcher = LogWatcher::new(&dir);
    let mut last_error: Option<String> = None;
    let mut items = Vec::new();
    while !stop.load(Ordering::SeqCst) {
        let result = watcher.poll(&mut |item| items.push(item));
        // 恢复的消息要先于这一轮读到的内容：界面不应该在“已就绪”之后还显示旧的错误
        if result.is_ok() && last_error.take().is_some() {
            tracing::info!(dir = %dir.display(), "游戏日志读取恢复正常");
            if tx.blocking_send(LogMsg::Recovered).is_err() {
                return;
            }
        }
        for item in items.drain(..) {
            if tx.blocking_send(LogMsg::Item(item)).is_err() {
                return;
            }
        }
        if let Err(err) = result {
            let message = err.to_string();
            if last_error.as_deref() != Some(message.as_str()) {
                tracing::warn!(dir = %dir.display(), error = %message, "读取日志失败");
                last_error = Some(message.clone());
                if tx.blocking_send(LogMsg::Failed { message }).is_err() {
                    return;
                }
            }
        }
        if !sleep_unless_stopped(timing.poll, stop) {
            return;
        }
    }
}

/// 睡一段时间，期间每隔一小会儿看一眼是否该退出。返回 `false` 表示该退出了。
fn sleep_unless_stopped(duration: Duration, stop: &AtomicBool) -> bool {
    let step = Duration::from_millis(50);
    let deadline = Instant::now() + duration;
    loop {
        if stop.load(Ordering::SeqCst) {
            return false;
        }
        let now = Instant::now();
        if now >= deadline {
            return true;
        }
        thread::sleep(step.min(deadline - now));
    }
}
