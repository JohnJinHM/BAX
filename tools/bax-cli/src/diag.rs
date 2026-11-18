//! `bax diag`：查看诊断日志的位置，导出脱敏后的诊断包（ADR-0007）。

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use bax_engine::diag::{self, ExportOptions};

pub fn path(data_dir: &Path) -> Result<()> {
    let dir = diag::diag_dir(data_dir);
    println!("{}", dir.display());
    let Ok(entries) = std::fs::read_dir(&dir) else {
        println!("（目录还不存在：桌面应用运行过、或命令行加了 --diag-file 之后才会创建）");
        return Ok(());
    };
    let mut files: Vec<(String, u64)> = entries
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == "log"))
        .filter_map(|e| {
            let len = e.metadata().ok()?.len();
            Some((e.file_name().to_string_lossy().into_owned(), len))
        })
        .collect();
    files.sort();
    for (name, len) in &files {
        println!("  {name}  {:.1} KB", *len as f64 / 1024.0);
    }
    println!(
        "{} 个文件，共 {:.1} MB",
        files.len(),
        files.iter().map(|(_, len)| *len).sum::<u64>() as f64 / 1024.0 / 1024.0
    );
    Ok(())
}

pub fn export(data_dir: &Path, runs: usize, out: Option<PathBuf>) -> Result<()> {
    let dir = diag::diag_dir(data_dir);
    let exported = diag::export(&dir, &ExportOptions { runs, out })
        .with_context(|| format!("导出 {} 里的诊断日志失败", dir.display()))?;
    println!("{}", exported.path.display());
    println!(
        "包含 {} 次运行：{}；替换了玩家 ID {} 个、比赛 ID {} 个",
        exported.runs.len(),
        exported.runs.join("、"),
        exported.players,
        exported.matches
    );
    Ok(())
}
