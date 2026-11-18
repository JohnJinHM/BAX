//! 诊断日志文件的命名、分组与清理。
//!
//! 每次启动一个文件：`<程序>-<启动时间>-<进程号>.log`，写满后接着写 `….1.log`、`….2.log`。
//! 第一个点之前的部分是这次运行的名字，同一次运行的各段文件靠它归到一起。

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// 文件大小与数量的上限。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// 单个文件写到这么大之后换下一段。
    pub segment_bytes: u64,
    /// 整个目录的总大小上限。
    pub total_bytes: u64,
    /// 整个目录的文件数上限。
    pub max_files: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            segment_bytes: 8 * 1024 * 1024,
            total_bytes: 64 * 1024 * 1024,
            max_files: 30,
        }
    }
}

/// 目录里的一个诊断日志文件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LogFile {
    pub path: PathBuf,
    pub len: u64,
    pub modified: SystemTime,
}

/// 一次运行写出的全部文件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Run {
    pub name: String,
    /// 按段号排列。
    pub files: Vec<PathBuf>,
    /// 各段里最晚的修改时间。
    pub modified: SystemTime,
}

/// 第 `segment` 段的文件名。第 0 段没有段号。
pub(crate) fn segment_file_name(run: &str, segment: u32) -> String {
    if segment == 0 {
        format!("{run}.log")
    } else {
        format!("{run}.{segment}.log")
    }
}

/// 从文件名拆出运行名和段号；不是诊断日志的文件返回 `None`。
pub(crate) fn parse_file_name(name: &str) -> Option<(&str, u32)> {
    let stem = name.strip_suffix(".log")?;
    match stem.split_once('.') {
        None if !stem.is_empty() => Some((stem, 0)),
        Some((run, segment)) if !run.is_empty() => Some((run, segment.parse().ok()?)),
        _ => None,
    }
}

/// 目录第一层的诊断日志文件。子目录（例如导出目录）不算。
pub(crate) fn list(dir: &Path) -> io::Result<Vec<LogFile>> {
    let mut files = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        let name = entry.file_name();
        if !meta.is_file() || name.to_str().and_then(parse_file_name).is_none() {
            continue;
        }
        files.push(LogFile {
            path: entry.path(),
            len: meta.len(),
            modified: meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
        });
    }
    Ok(files)
}

/// 超出上限时要删除的文件：从新到旧累计，放不下的都删掉。`keep` 是正在写的文件，永远保留。
pub(crate) fn pick_removals(
    files: &[LogFile],
    keep: Option<&Path>,
    limits: Limits,
) -> Vec<PathBuf> {
    let mut sorted: Vec<&LogFile> = files.iter().collect();
    sorted.sort_by(|a, b| b.modified.cmp(&a.modified).then(b.path.cmp(&a.path)));
    let mut count = 0usize;
    let mut total = 0u64;
    if let Some(current) = keep.and_then(|k| files.iter().find(|f| f.path == k)) {
        count = 1;
        total = current.len;
    }
    let mut removals = Vec::new();
    for file in sorted {
        if Some(file.path.as_path()) == keep {
            continue;
        }
        if count < limits.max_files && total.saturating_add(file.len) <= limits.total_bytes {
            count += 1;
            total += file.len;
        } else {
            removals.push(file.path.clone());
        }
    }
    removals
}

/// 按上限清理目录，返回删掉的文件数。删不掉的文件（例如另一个进程正在写）跳过。
pub(crate) fn enforce(dir: &Path, keep: Option<&Path>, limits: Limits) -> usize {
    let Ok(files) = list(dir) else {
        return 0;
    };
    pick_removals(&files, keep, limits)
        .iter()
        .filter(|path| fs::remove_file(path).is_ok())
        .count()
}

/// 把文件按运行归组，最新的运行在前。
pub(crate) fn group_runs(files: &[LogFile]) -> Vec<Run> {
    let mut runs: Vec<(Run, Vec<(u32, PathBuf)>)> = Vec::new();
    for file in files {
        let Some((name, segment)) = file
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(parse_file_name)
        else {
            continue;
        };
        match runs.iter_mut().find(|(run, _)| run.name == name) {
            Some((run, segments)) => {
                run.modified = run.modified.max(file.modified);
                segments.push((segment, file.path.clone()));
            }
            None => runs.push((
                Run {
                    name: name.to_owned(),
                    files: Vec::new(),
                    modified: file.modified,
                },
                vec![(segment, file.path.clone())],
            )),
        }
    }
    let mut runs: Vec<Run> = runs
        .into_iter()
        .map(|(mut run, mut segments)| {
            segments.sort();
            run.files = segments.into_iter().map(|(_, path)| path).collect();
            run
        })
        .collect();
    runs.sort_by(|a, b| b.modified.cmp(&a.modified).then(b.name.cmp(&a.name)));
    runs
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn file(name: &str, len: u64, age_secs: u64) -> LogFile {
        LogFile {
            path: PathBuf::from(name),
            len,
            modified: SystemTime::UNIX_EPOCH + Duration::from_secs(10_000 - age_secs),
        }
    }

    #[test]
    fn file_names_round_trip() {
        let run = "bax-app-20261117-203015-4120";
        assert_eq!(parse_file_name(&segment_file_name(run, 0)), Some((run, 0)));
        assert_eq!(parse_file_name(&segment_file_name(run, 3)), Some((run, 3)));
        assert_eq!(parse_file_name("notes.txt"), None);
        assert_eq!(parse_file_name("run.x.log"), None);
        assert_eq!(parse_file_name(".log"), None);
    }

    #[test]
    fn removals_start_from_the_oldest() {
        let limits = Limits {
            segment_bytes: 10,
            total_bytes: 25,
            max_files: 10,
        };
        let files = [
            file("a.log", 10, 300),
            file("b.log", 10, 200),
            file("c.log", 10, 100),
        ];
        assert_eq!(
            pick_removals(&files, None, limits),
            vec![PathBuf::from("a.log")]
        );
    }

    #[test]
    fn the_file_being_written_is_kept_even_when_it_is_the_oldest() {
        let limits = Limits {
            segment_bytes: 10,
            total_bytes: 100,
            max_files: 2,
        };
        let files = [
            file("current.log", 5, 500),
            file("b.log", 5, 200),
            file("c.log", 5, 100),
        ];
        assert_eq!(
            pick_removals(&files, Some(Path::new("current.log")), limits),
            vec![PathBuf::from("b.log")]
        );
    }

    #[test]
    fn segments_are_grouped_by_run_newest_run_first() {
        let files = [
            file("old.log", 1, 900),
            file("new.1.log", 1, 10),
            file("new.log", 1, 50),
            file("new.2.log", 1, 5),
        ];
        let runs = group_runs(&files);
        let names: Vec<&str> = runs.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["new", "old"]);
        assert_eq!(
            runs[0].files,
            [
                PathBuf::from("new.log"),
                PathBuf::from("new.1.log"),
                PathBuf::from("new.2.log")
            ]
        );
    }

    #[test]
    fn enforce_deletes_files_on_disk() {
        let dir = tempfile::tempdir().expect("临时目录");
        for name in ["a.log", "b.log", "c.log", "keep.txt"] {
            fs::write(dir.path().join(name), "0123456789").expect("写文件");
        }
        let limits = Limits {
            segment_bytes: 10,
            total_bytes: 1_000,
            max_files: 1,
        };
        let keep = dir.path().join("b.log");
        assert_eq!(enforce(dir.path(), Some(&keep), limits), 2);
        assert!(keep.exists());
        assert!(
            dir.path().join("keep.txt").exists(),
            "不是诊断日志的文件不动"
        );
    }
}
