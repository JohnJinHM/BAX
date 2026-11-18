//! 导出诊断包：把最近几次运行的诊断日志合成一个文件，替换掉能认出人的内容。
//!
//! 诊断日志只在三个字段里写 ID：`player`、`self_player` 和 `match_id`（ADR-0007）。
//! 导出时把这些值换成本次导出内一致的代号，本机玩家统一记为 `SELF`；
//! 路径里 `Users` 下面那一级目录名换成 `%USERNAME%`。

use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

use super::retention;

/// 导出目录里最多保留的诊断包数量。
const KEEP_EXPORTS: usize = 10;
pub(crate) const EXPORT_DIR_NAME: &str = "export";

/// 导出选项。
#[derive(Debug, Clone)]
pub struct ExportOptions {
    /// 导出最近几次运行。
    pub runs: usize,
    /// 输出文件；`None` 时写到诊断目录下的 `export\`。
    pub out: Option<PathBuf>,
}

impl Default for ExportOptions {
    fn default() -> Self {
        ExportOptions { runs: 2, out: None }
    }
}

/// 导出的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exported {
    pub path: PathBuf,
    /// 包含的运行，从旧到新。
    pub runs: Vec<String>,
    /// 替换掉的玩家 ID 与比赛 ID 的个数。
    pub players: usize,
    pub matches: usize,
}

/// 导出诊断包。目录里没有诊断日志时返回 `NotFound`。
pub fn export(diag_dir: &Path, options: &ExportOptions) -> io::Result<Exported> {
    let files = retention::list(diag_dir)?;
    let mut runs = retention::group_runs(&files);
    runs.truncate(options.runs.max(1));
    if runs.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("{} 里没有诊断日志", diag_dir.display()),
        ));
    }
    runs.reverse();

    // 先找出本机玩家，正文里出现在 player 字段时也记为 SELF
    let mut self_ids = HashSet::new();
    for run in &runs {
        for path in &run.files {
            for line in BufReader::new(File::open(path)?).lines() {
                collect_values(&line?, "self_player", &mut self_ids);
            }
        }
    }

    let out = match &options.out {
        Some(path) => path.clone(),
        None => {
            let dir = diag_dir.join(EXPORT_DIR_NAME);
            fs::create_dir_all(&dir)?;
            let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
            dir.join(format!("bax-diag-{stamp}.txt"))
        }
    };
    let mut redactor = Redactor::new(self_ids);
    let mut body = Vec::new();
    for run in &runs {
        for path in &run.files {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("?");
            writeln!(body, "\n## ===== {name} =====")?;
            for line in BufReader::new(File::open(path)?).lines() {
                writeln!(body, "{}", redactor.line(&line?))?;
            }
        }
    }

    let names: Vec<String> = runs.iter().map(|r| r.name.clone()).collect();
    let mut file = BufWriter::new(File::create(&out)?);
    writeln!(file, "# BAX 诊断包")?;
    writeln!(
        file,
        "# 导出时间：{}",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S%:z")
    )?;
    writeln!(file, "# 包含的运行：{}", names.join("、"))?;
    writeln!(
        file,
        "# 已替换：玩家 ID {} 个、比赛 ID {} 个，本机玩家记为 SELF；路径里的 Windows 用户名记为 %USERNAME%",
        redactor.players.len(),
        redactor.matches.len()
    )?;
    file.write_all(&body)?;
    file.flush()?;
    drop(file);

    if options.out.is_none() {
        prune_exports(&diag_dir.join(EXPORT_DIR_NAME));
    }
    Ok(Exported {
        path: out,
        runs: names,
        players: redactor.players.len(),
        matches: redactor.matches.len(),
    })
}

fn prune_exports(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == "txt"))
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    files.sort_by(|a, b| b.cmp(a));
    for (_, path) in files.into_iter().skip(KEEP_EXPORTS) {
        let _ = fs::remove_file(path);
    }
}

/// 一行里所有 `key=数字` 的值。
fn collect_values(line: &str, key: &str, out: &mut HashSet<String>) {
    let mut rest = line;
    let mut offset = 0;
    while let Some((start, end)) = find_field(rest, key) {
        out.insert(line[offset + start..offset + end].to_owned());
        offset += end;
        rest = &line[offset..];
    }
}

/// 找到第一个 `key=数字`，返回数字部分的范围。`key` 前面必须不是标识符字符，
/// 这样 `self_player=` 不会被当成 `player=`。
fn find_field(text: &str, key: &str) -> Option<(usize, usize)> {
    let pattern = format!("{key}=");
    let mut from = 0;
    while let Some(found) = text[from..].find(&pattern) {
        let at = from + found;
        let boundary = text[..at]
            .chars()
            .next_back()
            .is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
        let start = at + pattern.len();
        let digits = text[start..]
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(text.len() - start);
        if boundary && digits > 0 {
            return Some((start, start + digits));
        }
        from = start;
    }
    None
}

/// 把 ID 换成代号。同一次导出里同一个 ID 总是得到同一个代号。
#[derive(Debug)]
struct Redactor {
    self_ids: HashSet<String>,
    players: HashMap<String, String>,
    matches: HashMap<String, String>,
}

impl Redactor {
    fn new(self_ids: HashSet<String>) -> Self {
        Redactor {
            self_ids,
            players: HashMap::new(),
            matches: HashMap::new(),
        }
    }

    fn line(&mut self, line: &str) -> String {
        let mut text = redact_user_dirs(line);
        for key in ["self_player", "player", "match_id"] {
            text = self.replace_field(&text, key);
        }
        text
    }

    fn replace_field(&mut self, text: &str, key: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        while let Some((start, end)) = find_field(rest, key) {
            out.push_str(&rest[..start]);
            out.push_str(&self.alias(key, &rest[start..end]));
            rest = &rest[end..];
        }
        out.push_str(rest);
        out
    }

    fn alias(&mut self, key: &str, id: &str) -> String {
        if key != "match_id" && self.self_ids.contains(id) {
            return "SELF".to_owned();
        }
        let (map, prefix) = if key == "match_id" {
            (&mut self.matches, 'M')
        } else {
            (&mut self.players, 'P')
        };
        let next = map.len() + 1;
        map.entry(id.to_owned())
            .or_insert_with(|| format!("{prefix}{next:02}"))
            .clone()
    }
}

/// 把 `\Users\<名字>\` 或 `/Users/<名字>/` 里的名字换成 `%USERNAME%`，不分大小写。
fn redact_user_dirs(line: &str) -> String {
    // 只把 ASCII 字母转小写，字节位置与原文一致
    let lower = line.to_ascii_lowercase();
    let mut out = String::with_capacity(line.len());
    let mut copied = 0;
    let mut search = 0;
    while let Some(found) = ["\\users\\", "/users/"]
        .iter()
        .filter_map(|p| lower[search..].find(p).map(|i| (search + i, p.len())))
        .min()
    {
        let (at, len) = found;
        let name_start = at + len;
        let name_end = line[name_start..]
            .find(['\\', '/', '"', '\'', ' ', ')', ',', ';'])
            .map_or(line.len(), |i| name_start + i);
        let name = &line[name_start..name_end];
        if !name.is_empty() && !name.eq_ignore_ascii_case("public") && name != "%USERNAME%" {
            out.push_str(&line[copied..name_start]);
            out.push_str("%USERNAME%");
            copied = name_end;
        }
        search = name_end.max(name_start);
    }
    out.push_str(&line[copied..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_replaced_consistently_and_self_is_marked() {
        let mut redactor = Redactor::new(HashSet::from(["500".to_owned()]));
        let lines = [
            "INFO match{match_id=9000001}: 本机玩家 self_player=500",
            "DEBUG 请求玩家资料 player=101 elapsed_ms=120",
            "DEBUG 请求玩家资料 player=500",
            "DEBUG 请求玩家资料 player=101 match_id=9000002",
        ];
        let out: Vec<String> = lines.iter().map(|l| redactor.line(l)).collect();
        assert_eq!(
            out,
            [
                "INFO match{match_id=M01}: 本机玩家 self_player=SELF",
                "DEBUG 请求玩家资料 player=P01 elapsed_ms=120",
                "DEBUG 请求玩家资料 player=SELF",
                "DEBUG 请求玩家资料 player=P01 match_id=M02",
            ]
        );
    }

    #[test]
    fn similar_keys_and_non_numbers_are_left_alone() {
        let mut redactor = Redactor::new(HashSet::new());
        let line = "players=4 my_player=7 player=abc 规则 rule_id=3";
        assert_eq!(redactor.line(line), line);
    }

    #[test]
    fn windows_user_names_in_paths_are_hidden() {
        assert_eq!(
            redact_user_dirs(r"dir=C:\Users\Someone\AppData\Local\BAX file=c:\users\SOMEONE"),
            r"dir=C:\Users\%USERNAME%\AppData\Local\BAX file=c:\users\%USERNAME%"
        );
        assert_eq!(
            redact_user_dirs("path=/Users/示例/x"),
            "path=/Users/%USERNAME%/x"
        );
        assert_eq!(redact_user_dirs(r"C:\Users\Public\x"), r"C:\Users\Public\x");
    }

    #[test]
    fn export_merges_recent_runs_oldest_first() {
        let dir = tempfile::tempdir().expect("临时目录");
        let write = |name: &str, text: &str| {
            fs::write(dir.path().join(name), text).expect("写文件");
            // 保证修改时间有先后
            std::thread::sleep(std::time::Duration::from_millis(20));
        };
        write("app-1.log", "# 头\nINFO player=111\n");
        write("app-2.log", "# 头\nINFO self_player=222 match_id=9\n");
        write("app-2.1.log", "INFO player=222 player=333\n");

        let out = dir.path().join("out.txt");
        let exported = export(
            dir.path(),
            &ExportOptions {
                runs: 5,
                out: Some(out.clone()),
            },
        )
        .expect("导出");
        assert_eq!(exported.runs, ["app-1", "app-2"]);
        assert_eq!((exported.players, exported.matches), (2, 1));
        let text = fs::read_to_string(out).expect("读导出");
        for id in ["111", "222", "333"] {
            assert!(!text.contains(id), "导出里还有 {id}");
        }
        let first = text.find("app-1.log").expect("有第一次运行");
        let second = text.find("app-2.1.log").expect("有第二段");
        assert!(first < second);
        assert!(text.contains("player=SELF player=P02"));
    }

    #[test]
    fn exporting_an_empty_directory_is_an_error() {
        let dir = tempfile::tempdir().expect("临时目录");
        let err = export(dir.path(), &ExportOptions::default()).expect_err("没有可导出的");
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
    }
}
