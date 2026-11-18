//! 诊断日志的写线程。
//!
//! 记录先格式化成一行文本，经有界通道交给后台线程写盘，产生记录的线程不会因为写盘而阻塞。
//! 通道满时丢弃新记录并计数，恢复后补写一行说明丢了多少。

use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use tracing_subscriber::fmt::MakeWriter;

use super::retention::{self, Limits};

/// 通道容量，单位是条。
const CHANNEL_CAPACITY: usize = 4096;
/// 没有新记录时，多久把缓冲写到磁盘一次。
const IDLE_FLUSH: Duration = Duration::from_millis(250);
/// 写入失败后，隔多久再尝试重新打开文件。
const REOPEN_INTERVAL: Duration = Duration::from_secs(10);
const THREAD_NAME: &str = "bax-diag";

enum Msg {
    Line(Vec<u8>),
    Flush(mpsc::Sender<()>),
    Stop,
}

/// 写线程的句柄。
pub(crate) struct Sink {
    tx: SyncSender<Msg>,
    dropped: Arc<AtomicU64>,
    current: Arc<Mutex<PathBuf>>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl std::fmt::Debug for Sink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sink")
            .field("current", &self.current_file())
            .finish_non_exhaustive()
    }
}

impl Sink {
    /// 打开第一个文件，启动写线程。`header` 写在每个文件的开头，每项一行。
    pub(crate) fn start(
        dir: &Path,
        run: String,
        header: Vec<String>,
        limits: Limits,
    ) -> io::Result<Arc<Sink>> {
        fs::create_dir_all(dir)?;
        let mut writer = SegmentWriter {
            dir: dir.to_path_buf(),
            run,
            header,
            limits,
            segment: 0,
            file: None,
            written: 0,
            failed_at: None,
        };
        writer.open()?;
        retention::enforce(dir, Some(&writer.path()), limits);

        let current = Arc::new(Mutex::new(writer.path()));
        let dropped = Arc::new(AtomicU64::new(0));
        let (tx, rx) = mpsc::sync_channel(CHANNEL_CAPACITY);
        let thread = {
            let current = Arc::clone(&current);
            let dropped = Arc::clone(&dropped);
            thread::Builder::new()
                .name(THREAD_NAME.to_owned())
                .spawn(move || writer.run(&rx, &dropped, &current))?
        };
        Ok(Arc::new(Sink {
            tx,
            dropped,
            current,
            thread: Mutex::new(Some(thread)),
        }))
    }

    pub(crate) fn current_file(&self) -> PathBuf {
        self.current.lock().map(|p| p.clone()).unwrap_or_default()
    }

    fn send(&self, line: Vec<u8>) {
        match self.tx.try_send(Msg::Line(line)) {
            Ok(()) | Err(TrySendError::Disconnected(_)) => {}
            Err(TrySendError::Full(_)) => {
                self.dropped.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    /// 等写线程把已经收到的记录写到磁盘，最多等 `timeout`。在写线程自己里调用时直接返回。
    pub(crate) fn flush(&self, timeout: Duration) {
        if thread::current().name() == Some(THREAD_NAME) {
            return;
        }
        let (ack, done) = mpsc::channel();
        if self.tx.send(Msg::Flush(ack)).is_ok() {
            let _ = done.recv_timeout(timeout);
        }
    }
}

impl Drop for Sink {
    fn drop(&mut self) {
        let _ = self.tx.send(Msg::Stop);
        if let Some(thread) = self.thread.lock().ok().and_then(|mut t| t.take()) {
            let _ = thread.join();
        }
    }
}

/// 交给 `tracing_subscriber::fmt` 的写入端：每条记录取一个 [`EventWriter`]，写完时整行发出。
#[derive(Debug, Clone)]
pub(crate) struct SinkWriter(pub Arc<Sink>);

impl<'a> MakeWriter<'a> for SinkWriter {
    type Writer = EventWriter<'a>;

    fn make_writer(&'a self) -> Self::Writer {
        EventWriter {
            sink: &self.0,
            buf: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub(crate) struct EventWriter<'a> {
    sink: &'a Sink,
    buf: Vec<u8>,
}

impl Write for EventWriter<'_> {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.buf.extend_from_slice(data);
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for EventWriter<'_> {
    fn drop(&mut self) {
        if !self.buf.is_empty() {
            self.sink.send(std::mem::take(&mut self.buf));
        }
    }
}

/// 写线程里的状态：当前段的文件和已写的字节数。
struct SegmentWriter {
    dir: PathBuf,
    run: String,
    header: Vec<String>,
    limits: Limits,
    segment: u32,
    file: Option<BufWriter<File>>,
    written: u64,
    failed_at: Option<Instant>,
}

impl SegmentWriter {
    fn path(&self) -> PathBuf {
        self.dir
            .join(retention::segment_file_name(&self.run, self.segment))
    }

    fn open(&mut self) -> io::Result<()> {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.path())?;
        let mut file = BufWriter::new(file);
        let mut header = String::new();
        if self.segment > 0 {
            header += &format!(
                "# 接续 {}，第 {} 段\n",
                retention::segment_file_name(&self.run, 0),
                self.segment + 1
            );
        }
        for line in &self.header {
            header += "# ";
            header += line;
            header += "\n";
        }
        file.write_all(header.as_bytes())?;
        self.written = header.len() as u64;
        self.file = Some(file);
        Ok(())
    }

    fn run(mut self, rx: &mpsc::Receiver<Msg>, dropped: &AtomicU64, current: &Mutex<PathBuf>) {
        loop {
            match rx.recv_timeout(IDLE_FLUSH) {
                Ok(Msg::Line(line)) => {
                    let lost = dropped.swap(0, Ordering::Relaxed);
                    if lost > 0 {
                        let notice = format!(
                            "{}  WARN bax_engine::diag: 记录产生得太快，丢弃了 {lost} 条\n",
                            super::local_now()
                        );
                        self.write(notice.as_bytes(), current);
                    }
                    self.write(&line, current);
                }
                Ok(Msg::Flush(ack)) => {
                    self.flush();
                    let _ = ack.send(());
                }
                Ok(Msg::Stop) | Err(RecvTimeoutError::Disconnected) => {
                    self.flush();
                    return;
                }
                Err(RecvTimeoutError::Timeout) => self.flush(),
            }
        }
    }

    fn flush(&mut self) {
        if let Some(file) = self.file.as_mut()
            && file.flush().is_err()
        {
            self.file = None;
            self.failed_at = Some(Instant::now());
        }
    }

    fn write(&mut self, data: &[u8], current: &Mutex<PathBuf>) {
        if self.file.is_some()
            && self.written > 0
            && self.written + data.len() as u64 > self.limits.segment_bytes
        {
            self.flush();
            self.segment += 1;
            self.file = None;
            self.failed_at = None;
        }
        if self.file.is_none() {
            if self
                .failed_at
                .is_some_and(|at| at.elapsed() < REOPEN_INTERVAL)
            {
                return;
            }
            if self.open().is_err() {
                self.failed_at = Some(Instant::now());
                return;
            }
            self.failed_at = None;
            if let Ok(mut path) = current.lock() {
                *path = self.path();
            }
            retention::enforce(&self.dir, Some(&self.path()), self.limits);
        }
        let Some(file) = self.file.as_mut() else {
            return;
        };
        if file.write_all(data).is_ok() {
            self.written += data.len() as u64;
        } else {
            self.file = None;
            self.failed_at = Some(Instant::now());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(sink: &SinkWriter, text: &str) {
        let mut writer = sink.make_writer();
        writer.write_all(text.as_bytes()).expect("写入缓冲");
    }

    #[test]
    fn lines_reach_the_file_with_the_header() {
        let dir = tempfile::tempdir().expect("临时目录");
        let sink = Sink::start(
            dir.path(),
            "test-run".to_owned(),
            vec!["程序 test".to_owned()],
            Limits::default(),
        )
        .expect("启动");
        let writer = SinkWriter(Arc::clone(&sink));
        line(&writer, "第一行\n");
        line(&writer, "第二行\n");
        sink.flush(Duration::from_secs(5));
        let text = fs::read_to_string(sink.current_file()).expect("读文件");
        assert_eq!(text, "# 程序 test\n第一行\n第二行\n");
    }

    #[test]
    fn a_full_segment_continues_in_the_next_file() {
        let dir = tempfile::tempdir().expect("临时目录");
        let limits = Limits {
            segment_bytes: 40,
            total_bytes: 1_000,
            max_files: 10,
        };
        let sink =
            Sink::start(dir.path(), "run".to_owned(), vec!["头".to_owned()], limits).expect("启动");
        let writer = SinkWriter(Arc::clone(&sink));
        for i in 0..6 {
            line(&writer, &format!("line {i} 0123456789\n"));
        }
        sink.flush(Duration::from_secs(5));
        let first = fs::read_to_string(dir.path().join("run.log")).expect("第一段");
        let second = fs::read_to_string(dir.path().join("run.1.log")).expect("第二段");
        assert!(first.starts_with("# 头\nline 0"));
        assert!(second.starts_with("# 接续 run.log，第 2 段\n# 头\n"));
        let last = sink.current_file();
        assert_ne!(last, dir.path().join("run.log"));
        assert!(
            fs::read_to_string(last)
                .expect("最后一段")
                .contains("line 5")
        );
    }

    #[test]
    fn old_files_are_removed_when_segments_pile_up() {
        let dir = tempfile::tempdir().expect("临时目录");
        let limits = Limits {
            segment_bytes: 20,
            total_bytes: 1_000,
            max_files: 3,
        };
        let sink = Sink::start(dir.path(), "run".to_owned(), Vec::new(), limits).expect("启动");
        let writer = SinkWriter(Arc::clone(&sink));
        for i in 0..10 {
            line(&writer, &format!("line {i} 0123456789\n"));
        }
        sink.flush(Duration::from_secs(5));
        let count = retention::list(dir.path()).expect("列目录").len();
        assert!(count <= 3, "文件数 {count} 超过上限");
        assert!(sink.current_file().exists());
    }
}
