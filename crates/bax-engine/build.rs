//! 把构建时的 git 提交写进 `BAX_GIT_COMMIT`，诊断日志的运行信息里会用到（ADR-0007）。

use std::path::Path;
use std::process::Command;

fn main() {
    let git_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.git");
    // 只在仓库里构建时才监视 .git：路径不存在时 Cargo 会认为每次都要重新运行
    for watched in ["HEAD", "refs/heads", "packed-refs"] {
        let path = git_dir.join(watched);
        if path.exists() {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
    let commit = Command::new("git")
        .args(["rev-parse", "--short=10", "HEAD"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .ok()
        .filter(|out| out.status.success())
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_owned());
    println!("cargo:rustc-env=BAX_GIT_COMMIT={commit}");
}
