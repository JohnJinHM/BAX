//! 增量读取不断增长的日志文件。
//!
//! [`LogTailer`] 盯住目录里最新的一份日志，每次 [`LogTailer::poll`] 把新写入的内容切成完整的行交出去。
//! 它处理这些情况：
//!
//! - **半行。** 游戏可能只写了一行的前半截。没有换行符的残余留到下一次，与后续内容拼起来。
//!   按字节切行，所以多字节字符被拆在两次读取之间也不会出错。
//! - **截断。** 文件比上次读到的位置短，说明被清空重写，从头再读。
//! - **换文件。** 游戏每次启动新建一份日志。先把旧文件读到末尾，再从头读新文件。
//! - **文件消失。** 当前文件被删掉时重新选最新的一份。

use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::is_log_file_name;

/// 单行的长度上限。超过它的内容按一行交出，防止损坏的文件把内存撑满。
const MAX_LINE_BYTES: usize = 1 << 20;
const READ_BUFFER_BYTES: usize = 64 * 1024;

/// [`LogTailer::poll`] 交出的内容。
#[derive(Debug, PartialEq, Eq)]
pub enum TailItem<'a> {
    /// 开始读一份文件，随后的行都来自它。
    FileOpened { path: &'a Path },
    /// 当前文件被截断，随后的行从文件开头重新读出。
    Truncated { path: &'a Path },
    /// 一行完整的文本，不含换行符。
    Line(&'a str),
}

#[derive(Debug)]
struct Current {
    path: PathBuf,
    offset: u64,
}

/// 日志目录的增量读取器。
#[derive(Debug)]
pub struct LogTailer {
    dir: PathBuf,
    current: Option<Current>,
    carry: Vec<u8>,
    buffer: Vec<u8>,
    last_scan: Option<Instant>,
    rescan_every: Duration,
}

impl LogTailer {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        LogTailer {
            dir: dir.into(),
            current: None,
            carry: Vec::new(),
            buffer: vec![0; READ_BUFFER_BYTES],
            last_scan: None,
            rescan_every: Duration::from_secs(2),
        }
    }

    /// 多久重新列一次目录来发现新文件。读取当前文件不受这个间隔限制。
    pub fn with_rescan_interval(mut self, every: Duration) -> Self {
        self.rescan_every = every;
        self
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// 正在读的文件。
    pub fn current_file(&self) -> Option<&Path> {
        self.current.as_ref().map(|c| c.path.as_path())
    }

    /// 读出自上次以来新增的内容，返回读到的字节数。
    ///
    /// 目录不存在或不可读时返回错误，状态保持不变，之后可以再试。
    pub fn poll(&mut self, sink: &mut dyn FnMut(TailItem<'_>)) -> io::Result<u64> {
        let mut newer = None;
        let due = self
            .last_scan
            .is_none_or(|at| at.elapsed() >= self.rescan_every);
        if self.current.is_none() || due {
            let latest = latest_log_file(&self.dir)?;
            self.last_scan = Some(Instant::now());
            match (&self.current, latest) {
                (None, Some(path)) => self.open(path, sink),
                (Some(current), Some(path)) if path.file_name() > current.path.file_name() => {
                    newer = Some(path)
                }
                _ => {}
            }
        }

        let mut total = self.drain(sink)?;
        if let Some(path) = newer {
            // 旧文件不会再增长：没有换行符的残余就是它的最后一行
            self.finish_line(sink);
            self.open(path, sink);
            total += self.drain(sink)?;
        }
        Ok(total)
    }

    fn open(&mut self, path: PathBuf, sink: &mut dyn FnMut(TailItem<'_>)) {
        self.carry.clear();
        sink(TailItem::FileOpened { path: &path });
        self.current = Some(Current { path, offset: 0 });
    }

    fn finish_line(&mut self, sink: &mut dyn FnMut(TailItem<'_>)) {
        if !self.carry.is_empty() {
            sink(TailItem::Line(&String::from_utf8_lossy(&self.carry)));
            self.carry.clear();
        }
    }

    fn drain(&mut self, sink: &mut dyn FnMut(TailItem<'_>)) -> io::Result<u64> {
        let Some(current) = self.current.as_mut() else {
            return Ok(0);
        };
        let len = match fs::metadata(&current.path) {
            Ok(meta) => meta.len(),
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                // 文件被删：下一次重新选最新的一份
                self.current = None;
                self.carry.clear();
                self.last_scan = None;
                return Ok(0);
            }
            Err(err) => return Err(err),
        };
        if len < current.offset {
            current.offset = 0;
            self.carry.clear();
            sink(TailItem::Truncated {
                path: &current.path,
            });
        }
        if len == current.offset {
            return Ok(0);
        }

        // 游戏一直占着这个文件写入；标准库在 Windows 上默认以共享读写方式打开，不会与它冲突
        let mut file = File::open(&current.path)?;
        file.seek(SeekFrom::Start(current.offset))?;
        let mut total = 0u64;
        loop {
            let n = file.read(&mut self.buffer)?;
            if n == 0 {
                break;
            }
            current.offset += n as u64;
            total += n as u64;
            split_lines(&mut self.carry, &self.buffer[..n], sink);
        }
        Ok(total)
    }
}

/// 把一块字节切成行。末尾没有换行符的部分留在 `carry` 里。
fn split_lines(carry: &mut Vec<u8>, chunk: &[u8], sink: &mut dyn FnMut(TailItem<'_>)) {
    let mut rest = chunk;
    while let Some(pos) = rest.iter().position(|&b| b == b'\n') {
        let (line, tail) = rest.split_at(pos);
        if carry.is_empty() {
            sink(TailItem::Line(&String::from_utf8_lossy(line)));
        } else {
            carry.extend_from_slice(line);
            sink(TailItem::Line(&String::from_utf8_lossy(carry)));
            carry.clear();
        }
        rest = &tail[1..];
    }
    carry.extend_from_slice(rest);
    if carry.len() > MAX_LINE_BYTES {
        sink(TailItem::Line(&String::from_utf8_lossy(carry)));
        carry.clear();
    }
}

/// 目录里最新的一份游戏日志。文件名里带时间，按名字排序就是按时间排序。
pub fn latest_log_file(dir: &Path) -> io::Result<Option<PathBuf>> {
    let mut latest: Option<PathBuf> = None;
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name_str) = name.to_str() else {
            continue;
        };
        if !is_log_file_name(name_str) {
            continue;
        }
        if latest
            .as_ref()
            .is_none_or(|best| Some(name.as_os_str()) > best.file_name())
        {
            latest = Some(entry.path());
        }
    }
    Ok(latest)
}

/// 目录里全部游戏日志，从旧到新。
pub fn list_log_files(dir: &Path) -> io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if entry.file_name().to_str().is_some_and(is_log_file_name) {
            files.push(entry.path());
        }
    }
    files.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
    Ok(files)
}

#[cfg(test)]
mod tests;
