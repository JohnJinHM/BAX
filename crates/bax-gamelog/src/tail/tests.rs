use std::fs::OpenOptions;
use std::io::Write;

use super::*;

/// 记下读取器交出的全部内容，方便断言。
#[derive(Debug, PartialEq, Eq, Clone)]
enum Got {
    Opened(String),
    Truncated,
    Line(String),
}

fn poll(tailer: &mut LogTailer) -> Vec<Got> {
    let mut got = Vec::new();
    tailer
        .poll(&mut |item| {
            got.push(match item {
                TailItem::FileOpened { path } => Got::Opened(
                    path.file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or_default()
                        .to_owned(),
                ),
                TailItem::Truncated { .. } => Got::Truncated,
                TailItem::Line(line) => Got::Line(line.to_owned()),
            })
        })
        .expect("读取应当成功");
    got
}

fn lines(got: &[Got]) -> Vec<&str> {
    got.iter()
        .filter_map(|g| match g {
            Got::Line(l) => Some(l.as_str()),
            _ => None,
        })
        .collect()
}

fn append(path: &Path, bytes: &[u8]) {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .expect("打开文件");
    file.write_all(bytes).expect("写入");
}

fn tailer(dir: &Path) -> LogTailer {
    // 测试里每次都重新列目录
    LogTailer::new(dir).with_rescan_interval(Duration::ZERO)
}

const FIRST: &str = "Gamelog__2026_11_16__18_31.log";
const SECOND: &str = "Gamelog__2026_11_16__18_51.log";

#[test]
fn reads_existing_content_then_only_what_is_appended() {
    let dir = tempfile::tempdir().expect("临时目录");
    let path = dir.path().join(FIRST);
    append(&path, b"one\ntwo\n");

    let mut t = tailer(dir.path());
    let got = poll(&mut t);
    assert_eq!(
        got,
        vec![
            Got::Opened(FIRST.into()),
            Got::Line("one".into()),
            Got::Line("two".into())
        ]
    );
    assert_eq!(poll(&mut t), vec![], "没有新内容时什么都不交出");

    append(&path, b"three\n");
    assert_eq!(lines(&poll(&mut t)), ["three"]);
    assert_eq!(t.current_file(), Some(path.as_path()));
}

#[test]
fn a_partial_line_waits_for_its_newline() {
    let dir = tempfile::tempdir().expect("临时目录");
    let path = dir.path().join(FIRST);
    append(&path, b"complete\nhal");

    let mut t = tailer(dir.path());
    assert_eq!(lines(&poll(&mut t)), ["complete"]);

    append(&path, b"f of a li");
    assert_eq!(lines(&poll(&mut t)), Vec::<&str>::new());

    append(&path, b"ne\nnext\n");
    assert_eq!(lines(&poll(&mut t)), ["half of a line", "next"]);
}

#[test]
fn multibyte_characters_split_across_reads_stay_intact() {
    let dir = tempfile::tempdir().expect("临时目录");
    let path = dir.path().join(FIRST);
    let text = "ID: 1, Name: 芳乃 さくら🍊, Team: Alpha\n".as_bytes();

    let mut t = tailer(dir.path());
    let mut seen = Vec::new();
    // 每次只多写一个字节：任何多字节字符都会被拆开
    for byte in text {
        append(&path, &[*byte]);
        seen.extend(poll(&mut t));
    }
    assert_eq!(lines(&seen), ["ID: 1, Name: 芳乃 さくら🍊, Team: Alpha"]);
}

#[test]
fn invalid_utf8_is_replaced_instead_of_failing() {
    let dir = tempfile::tempdir().expect("临时目录");
    let path = dir.path().join(FIRST);
    append(&path, b"bad \xff byte\nfine\n");

    let mut t = tailer(dir.path());
    assert_eq!(lines(&poll(&mut t)), ["bad \u{fffd} byte", "fine"]);
}

#[test]
fn truncation_restarts_from_the_beginning() {
    let dir = tempfile::tempdir().expect("临时目录");
    let path = dir.path().join(FIRST);
    append(&path, b"old one\nold two\npartial");

    let mut t = tailer(dir.path());
    assert_eq!(lines(&poll(&mut t)), ["old one", "old two"]);

    std::fs::write(&path, b"new\n").expect("重写文件");
    let got = poll(&mut t);
    assert_eq!(got, vec![Got::Truncated, Got::Line("new".into())]);
    assert_eq!(poll(&mut t), vec![]);
}

#[test]
fn a_newer_file_takes_over_after_the_old_one_is_drained() {
    let dir = tempfile::tempdir().expect("临时目录");
    let first = dir.path().join(FIRST);
    append(&first, b"a\n");

    let mut t = tailer(dir.path());
    assert_eq!(lines(&poll(&mut t)), ["a"]);

    // 游戏退出前写下最后几行（最后一行没有换行符），随后重启并新建日志
    append(&first, b"b\nlast words");
    let second = dir.path().join(SECOND);
    append(&second, b"fresh\n");

    let got = poll(&mut t);
    assert_eq!(
        got,
        vec![
            Got::Line("b".into()),
            Got::Line("last words".into()),
            Got::Opened(SECOND.into()),
            Got::Line("fresh".into()),
        ]
    );
    assert_eq!(t.current_file(), Some(second.as_path()));

    // 旧文件之后再有写入也不会回头去读
    append(&first, b"ignored\n");
    assert_eq!(poll(&mut t), vec![]);
}

#[test]
fn picks_the_latest_file_and_ignores_other_files() {
    let dir = tempfile::tempdir().expect("临时目录");
    append(&dir.path().join(FIRST), b"old\n");
    append(&dir.path().join(SECOND), b"new\n");
    append(
        &dir.path().join("Gamelog__2026_11_17__09_00.txt"),
        b"not a log\n",
    );
    append(&dir.path().join("zzz.log"), b"not a game log\n");
    append(&dir.path().join("Gamelog__.log"), b"no timestamp\n");

    let mut t = tailer(dir.path());
    assert_eq!(
        poll(&mut t),
        vec![Got::Opened(SECOND.into()), Got::Line("new".into())]
    );

    let all = list_log_files(dir.path()).expect("列目录");
    let names: Vec<_> = all
        .iter()
        .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
        .collect();
    assert_eq!(names, [FIRST, SECOND]);
}

#[test]
fn an_empty_directory_yields_nothing_until_a_log_appears() {
    let dir = tempfile::tempdir().expect("临时目录");
    let mut t = tailer(dir.path());
    assert_eq!(poll(&mut t), vec![]);
    assert_eq!(t.current_file(), None);

    append(&dir.path().join(FIRST), b"hello\n");
    assert_eq!(
        poll(&mut t),
        vec![Got::Opened(FIRST.into()), Got::Line("hello".into())]
    );
}

#[test]
fn a_missing_directory_is_an_error_that_can_be_retried() {
    let dir = tempfile::tempdir().expect("临时目录");
    let missing = dir.path().join("GameLogs");
    let mut t = tailer(&missing);
    let err = t.poll(&mut |_| {}).expect_err("目录不存在应当报错");
    assert_eq!(err.kind(), io::ErrorKind::NotFound);

    std::fs::create_dir(&missing).expect("建目录");
    append(&missing.join(FIRST), b"now it works\n");
    assert_eq!(lines(&poll(&mut t)), ["now it works"]);
}

#[test]
fn a_deleted_current_file_falls_back_to_the_latest_remaining() {
    let dir = tempfile::tempdir().expect("临时目录");
    let first = dir.path().join(FIRST);
    let second = dir.path().join(SECOND);
    append(&first, b"older\n");
    append(&second, b"newer\n");

    let mut t = tailer(dir.path());
    assert_eq!(lines(&poll(&mut t)), ["newer"]);

    std::fs::remove_file(&second).expect("删除文件");
    assert_eq!(poll(&mut t), vec![], "发现文件消失的这一次不交出内容");
    assert_eq!(
        poll(&mut t),
        vec![Got::Opened(FIRST.into()), Got::Line("older".into())]
    );
}

#[test]
fn large_content_spanning_many_buffers_is_read_completely() {
    let dir = tempfile::tempdir().expect("临时目录");
    let path = dir.path().join(FIRST);
    let mut expected = Vec::new();
    let mut content = Vec::new();
    for i in 0..20_000 {
        let line = format!("line number {i} with some padding to cross buffer boundaries");
        content.extend_from_slice(line.as_bytes());
        content.push(b'\n');
        expected.push(line);
    }
    append(&path, &content);

    let mut t = tailer(dir.path());
    let got = poll(&mut t);
    assert_eq!(lines(&got), expected);
}

#[test]
fn an_overlong_line_is_handed_over_in_pieces() {
    let dir = tempfile::tempdir().expect("临时目录");
    let path = dir.path().join(FIRST);
    append(&path, &vec![b'x'; MAX_LINE_BYTES + READ_BUFFER_BYTES]);
    append(&path, b"\nafter\n");

    let mut t = tailer(dir.path());
    let got = poll(&mut t);
    let got = lines(&got);
    assert!(got.len() >= 3, "超长行被切成多段，共 {} 段", got.len());
    assert_eq!(got.last().copied(), Some("after"));
    let total: usize = got[..got.len() - 1].iter().map(|l| l.len()).sum();
    assert_eq!(total, MAX_LINE_BYTES + READ_BUFFER_BYTES);
}
