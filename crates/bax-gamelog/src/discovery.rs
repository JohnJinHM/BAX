//! 找到游戏日志目录。
//!
//! 先从注册表取得 Steam 的安装位置，再读 `libraryfolders.vdf` 得到全部游戏库，
//! 在每个库里找 `steamapps/common/broken_arrow/GameLogs`。
//! 移植自 BA-ELO-Overlay 的 `_steam_library_dirs` 与 `find_game_logs`（`src/ba_elo_overlay.py`）。
//! 找不到时由用户手动指定路径，那一步不在这个库里。

use std::fs;
use std::path::{Path, PathBuf};

use crate::is_log_file_name;

const GAME_DIR: &str = "broken_arrow";
const LOG_DIR: &str = "GameLogs";

/// 没装在 Steam 默认库里时常见的位置，注册表读不到时用来兜底。
const FALLBACK_LIBRARIES: [&str; 3] = [
    r"C:\Program Files (x86)\Steam",
    r"C:\Program Files\Steam",
    r"D:\SteamLibrary",
];

/// 自动定位日志目录。优先返回里面确实有日志文件的目录，其次是存在但还没有日志的目录。
pub fn find_log_dir() -> Option<PathBuf> {
    pick_log_dir(&candidate_log_dirs())
}

/// 全部候选日志目录，按优先顺序排列，不保证存在。
pub fn candidate_log_dirs() -> Vec<PathBuf> {
    let mut libraries = library_dirs(&steam_roots());
    for fallback in FALLBACK_LIBRARIES {
        push_unique(&mut libraries, PathBuf::from(fallback));
    }
    libraries
        .iter()
        .map(|lib| log_dir_in_library(lib))
        .collect()
}

/// 某个 Steam 库下的日志目录。
pub fn log_dir_in_library(library: &Path) -> PathBuf {
    library
        .join("steamapps")
        .join("common")
        .join(GAME_DIR)
        .join(LOG_DIR)
}

fn pick_log_dir(candidates: &[PathBuf]) -> Option<PathBuf> {
    candidates
        .iter()
        .find(|dir| contains_logs(dir))
        .or_else(|| candidates.iter().find(|dir| dir.is_dir()))
        .cloned()
}

/// 目录里有没有游戏日志文件。
pub fn contains_logs(dir: &Path) -> bool {
    fs::read_dir(dir).is_ok_and(|entries| {
        entries
            .flatten()
            .any(|e| e.file_name().to_str().is_some_and(is_log_file_name))
    })
}

/// Steam 的全部游戏库：安装目录本身，加上 `libraryfolders.vdf` 里登记的其他库。
pub fn library_dirs(steam_roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut libraries = Vec::new();
    for root in steam_roots {
        push_unique(&mut libraries, root.clone());
    }
    for root in steam_roots {
        for vdf in [
            root.join("steamapps").join("libraryfolders.vdf"),
            root.join("config").join("libraryfolders.vdf"),
        ] {
            // 文件里可能有非 UTF-8 的路径，按有损方式读取，不因此放弃其余条目
            if let Ok(bytes) = fs::read(&vdf) {
                for path in parse_library_folders(&String::from_utf8_lossy(&bytes)) {
                    push_unique(&mut libraries, path);
                }
            }
        }
    }
    libraries
}

/// 从 `libraryfolders.vdf` 的文本里取出各个库的路径。
///
/// 只认 `"path"  "…"` 这一种键值，不解析整个 VDF 结构。
pub fn parse_library_folders(text: &str) -> Vec<PathBuf> {
    const KEY: &str = "\"path\"";
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find(KEY) {
        rest = &rest[at + KEY.len()..];
        let Some(body) = rest.trim_start().strip_prefix('"') else {
            continue;
        };
        let mut value = String::new();
        let mut chars = body.chars();
        let mut closed = false;
        while let Some(c) = chars.next() {
            match c {
                // VDF 用反斜杠转义，路径里的 `\\` 表示一个反斜杠
                '\\' => value.extend(chars.next()),
                '"' => {
                    closed = true;
                    break;
                }
                _ => value.push(c),
            }
        }
        if closed {
            rest = chars.as_str();
            let value = value.trim();
            if !value.is_empty() {
                push_unique(&mut out, PathBuf::from(value));
            }
        }
    }
    out
}

/// 路径的比较键：Windows 的路径不区分大小写，注册表里的路径还可能用正斜杠。
fn path_key(path: &Path) -> String {
    path.to_string_lossy()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase()
}

fn push_unique(list: &mut Vec<PathBuf>, path: PathBuf) {
    let key = path_key(&path);
    if !key.is_empty() && !list.iter().any(|p| path_key(p) == key) {
        list.push(path);
    }
}

/// 从注册表读出 Steam 的安装目录。
#[cfg(windows)]
pub fn steam_roots() -> Vec<PathBuf> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

    let sources = [
        (HKEY_CURRENT_USER, r"Software\Valve\Steam", "SteamPath"),
        (
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\WOW6432Node\Valve\Steam",
            "InstallPath",
        ),
        (HKEY_LOCAL_MACHINE, r"SOFTWARE\Valve\Steam", "InstallPath"),
    ];
    let mut roots = Vec::new();
    for (hive, key, value) in sources {
        let found: Option<String> = RegKey::predef(hive)
            .open_subkey(key)
            .and_then(|k| k.get_value(value))
            .ok();
        if let Some(path) = found {
            push_unique(&mut roots, PathBuf::from(path.trim()));
        }
    }
    roots
}

/// 非 Windows 平台没有注册表，只能依靠手动指定的路径。
#[cfg(not(windows))]
pub fn steam_roots() -> Vec<PathBuf> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    const VDF: &str = r#""libraryfolders"
{
	"0"
	{
		"path"		"C:\\Program Files (x86)\\Steam"
		"label"		""
		"contentid"		"123"
		"apps"
		{
			"1604270"		"52428800000"
		}
	}
	"1"
	{
		"path"		"D:\\SteamLibrary"
		"apps"
		{
		}
	}
	"2"
	{
		"path"		"E:\\Games\\名前 with space"
	}
}
"#;

    #[test]
    fn reads_library_paths_from_vdf() {
        assert_eq!(
            parse_library_folders(VDF),
            vec![
                PathBuf::from(r"C:\Program Files (x86)\Steam"),
                PathBuf::from(r"D:\SteamLibrary"),
                PathBuf::from(r"E:\Games\名前 with space"),
            ]
        );
    }

    #[test]
    fn vdf_oddities_do_not_break_parsing() {
        // 重复、大小写不同、空值、没有闭合的引号、键后面不是字符串
        let text = r#"
            "path" "D:\\Lib"
            "path" "d:\\lib\\"
            "path" ""
            "path" { }
            "PATH" "X:\\ignored-key-is-case-sensitive"
            "path" "E:\\Unclosed
        "#;
        assert_eq!(parse_library_folders(text), vec![PathBuf::from(r"D:\Lib")]);
        assert_eq!(parse_library_folders(""), Vec::<PathBuf>::new());
    }

    #[test]
    fn libraries_include_roots_and_vdf_entries_without_duplicates() {
        let steam = tempfile::tempdir().expect("临时目录");
        let other = tempfile::tempdir().expect("临时目录");
        fs::create_dir_all(steam.path().join("steamapps")).expect("建目录");
        let vdf = format!(
            "\"libraryfolders\"\n{{\n\t\"0\"\n\t{{\n\t\t\"path\"\t\t\"{}\"\n\t}}\n\t\"1\"\n\t{{\n\t\t\"path\"\t\t\"{}\"\n\t}}\n}}\n",
            steam.path().display().to_string().replace('\\', "\\\\"),
            other.path().display().to_string().replace('\\', "\\\\"),
        );
        fs::write(
            steam.path().join("steamapps").join("libraryfolders.vdf"),
            vdf,
        )
        .expect("写入 vdf");

        let libs = library_dirs(&[steam.path().to_path_buf()]);
        assert_eq!(
            libs,
            vec![steam.path().to_path_buf(), other.path().to_path_buf()]
        );
    }

    #[test]
    fn prefers_a_directory_that_already_has_logs() {
        let empty = tempfile::tempdir().expect("临时目录");
        let with_logs = tempfile::tempdir().expect("临时目录");
        let missing = empty.path().join("nope");
        fs::write(
            with_logs.path().join("Gamelog__2026_11_16__18_51.log"),
            b"x",
        )
        .expect("写入日志");

        let candidates = vec![
            missing.clone(),
            empty.path().to_path_buf(),
            with_logs.path().to_path_buf(),
        ];
        assert_eq!(
            pick_log_dir(&candidates),
            Some(with_logs.path().to_path_buf())
        );
        // 都没有日志时退而求其次，取第一个存在的目录
        assert_eq!(
            pick_log_dir(&[missing.clone(), empty.path().to_path_buf()]),
            Some(empty.path().to_path_buf())
        );
        assert_eq!(pick_log_dir(&[missing]), None);
    }

    #[test]
    fn log_dir_path_inside_a_library() {
        assert_eq!(
            log_dir_in_library(Path::new(r"D:\SteamLibrary")),
            Path::new(r"D:\SteamLibrary")
                .join("steamapps")
                .join("common")
                .join("broken_arrow")
                .join("GameLogs")
        );
    }
}
