//! 诊断日志：把各库的 `tracing` 记录写进本机文件（ADR-0007）。
//!
//! 各库只用 `tracing` 的宏埋点；这里负责装配订阅器、写文件、按大小换段和清理、
//! 记录 panic，以及导出脱敏后的诊断包。桌面应用和命令行在启动时调用 [`init`]。
//!
//! 埋点约定：玩家 ID 只放在 `player` 或 `self_player` 字段，比赛 ID 只放在 `match_id` 字段；
//! 玩家名称、卡组名、请求 URL 和游戏日志原文不写进诊断日志。导出时的脱敏依赖这条约定。

mod export;
mod retention;
mod sink;

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tracing::Subscriber;
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::time::FormatTime;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{EnvFilter, Layer, Registry, reload};

pub use export::{ExportOptions, Exported, export};
pub use retention::Limits;

use sink::{Sink, SinkWriter};

/// 数据目录下存放诊断日志的子目录。
pub const DIR_NAME: &str = "diag";
/// 覆盖级别的环境变量，语法同 `EnvFilter`，例如 `bax_engine=trace,warn`。
pub const ENV_VAR: &str = "BAX_LOG";
/// 界面设置里的诊断档位。键以 `ui.` 开头，由外壳读取（与 `ui.hotkeys` 相同）。
pub const SETTINGS_KEY: &str = "ui.diag";
/// 构建时的 git 提交，取不到时为 `unknown`。
pub const COMMIT: &str = env!("BAX_GIT_COMMIT");
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

const FLUSH_TIMEOUT: Duration = Duration::from_secs(2);

/// 数据目录下的诊断日志目录。
pub fn diag_dir(data_dir: &Path) -> PathBuf {
    data_dir.join(DIR_NAME)
}

/// 诊断档位。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DiagLevel {
    /// 生命周期与状态变化。
    #[default]
    Standard,
    /// 再加上每次请求、每个事件和每次决策的依据。
    Verbose,
}

impl DiagLevel {
    /// 对应的过滤规则。所有 BAX 的 target 都以 `bax` 开头，`EnvFilter` 按前缀匹配。
    pub fn directives(self) -> &'static str {
        match self {
            DiagLevel::Standard => "warn,bax=info,bax_ui=warn",
            DiagLevel::Verbose => "warn,bax=debug",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            DiagLevel::Standard => "standard",
            DiagLevel::Verbose => "verbose",
        }
    }
}

/// 界面设置 `ui.diag` 的内容。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DiagSettings {
    pub level: DiagLevel,
}

/// 标准错误的输出方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StderrOutput {
    Off,
    /// 与文件使用同一个档位，随档位切换。
    FollowLevel,
    /// 固定的过滤规则：依次看 `BAX_LOG`、`RUST_LOG`，都没有时用这里给的默认值。
    Env {
        default: &'static str,
    },
}

/// [`init`] 的参数。
#[derive(Debug, Clone)]
pub struct DiagConfig {
    /// 程序名，用作文件名前缀，例如 `bax-app`。
    pub program: String,
    /// 写文件的目录；`None` 时不写文件。
    pub file_dir: Option<PathBuf>,
    pub stderr: StderrOutput,
    /// 起始档位。
    pub level: DiagLevel,
    /// 为真时档位固定为 `level`，之后的设置变化不再生效（例如启动参数 `--verbose`）。
    pub level_forced: bool,
    /// 写在文件开头的附加运行信息。
    pub extra: Vec<(String, String)>,
    pub limits: Limits,
    /// 是否接管 panic 钩子，把崩溃写进诊断日志。
    pub panic_hook: bool,
}

impl DiagConfig {
    pub fn new(program: impl Into<String>) -> Self {
        DiagConfig {
            program: program.into(),
            file_dir: None,
            stderr: StderrOutput::Off,
            level: DiagLevel::Standard,
            level_forced: false,
            extra: Vec::new(),
            limits: Limits::default(),
            panic_hook: true,
        }
    }
}

type FilterHandle = reload::Handle<EnvFilter, Registry>;

/// 诊断日志的句柄：切换档位、刷盘、查看文件位置。丢弃时把缓冲写到磁盘。
#[derive(Debug)]
pub struct DiagHandle {
    filters: Vec<FilterHandle>,
    level: Mutex<DiagLevel>,
    /// 档位由 `BAX_LOG` 或启动参数决定，界面上的设置不生效。
    locked: bool,
    env_directives: Option<String>,
    sink: Option<Arc<Sink>>,
    dir: Option<PathBuf>,
    file_error: Option<String>,
}

impl DiagHandle {
    /// 切换档位。档位被锁定时什么也不做，返回 `false`。
    pub fn set_level(&self, level: DiagLevel) -> bool {
        if self.locked {
            return false;
        }
        let Ok(mut current) = self.level.lock() else {
            return false;
        };
        if *current == level {
            return true;
        }
        for handle in &self.filters {
            if let Err(err) = handle.reload(EnvFilter::new(level.directives())) {
                tracing::warn!(target: "bax_engine::diag", error = %err, "切换诊断档位失败");
                return false;
            }
        }
        *current = level;
        tracing::info!(target: "bax_engine::diag", level = level.as_str(), "诊断档位已切换");
        true
    }

    pub fn level(&self) -> DiagLevel {
        self.level.lock().map(|l| *l).unwrap_or_default()
    }

    /// 档位是否由环境变量或启动参数锁定。
    pub fn locked(&self) -> bool {
        self.locked
    }

    /// `BAX_LOG` 的内容，没有设置时为 `None`。
    pub fn env_directives(&self) -> Option<&str> {
        self.env_directives.as_deref()
    }

    /// 诊断日志目录；不写文件时为 `None`。
    pub fn dir(&self) -> Option<&Path> {
        self.dir.as_deref()
    }

    /// 正在写的文件。
    pub fn current_file(&self) -> Option<PathBuf> {
        self.sink.as_ref().map(|s| s.current_file())
    }

    /// 打不开诊断日志文件时的原因。
    pub fn file_error(&self) -> Option<&str> {
        self.file_error.as_deref()
    }

    /// 把已经产生的记录写到磁盘。导出之前和退出之前调用。
    pub fn flush(&self) {
        if let Some(sink) = &self.sink {
            sink.flush(FLUSH_TIMEOUT);
        }
    }
}

impl Drop for DiagHandle {
    fn drop(&mut self) {
        self.flush();
    }
}

/// 装配订阅器并设为全局默认，按需接管 panic 钩子。全局订阅器已经存在时（例如测试里）不覆盖它。
pub fn init(config: DiagConfig) -> DiagHandle {
    let panic_hook = config.panic_hook;
    let (subscriber, handle) = build(config);
    let installed = tracing::subscriber::set_global_default(subscriber).is_ok();
    if installed && panic_hook {
        install_panic_hook(handle.sink.clone());
    }
    log_startup(&handle);
    handle
}

/// 只装配订阅器，不安装。测试里配合 `tracing::subscriber::set_global_default` 或 `with_default` 使用。
pub fn build(config: DiagConfig) -> (impl Subscriber + Send + Sync + 'static, DiagHandle) {
    let env_directives = std::env::var(ENV_VAR).ok().filter(|v| !v.trim().is_empty());
    let env_filter = env_directives
        .as_deref()
        .and_then(|d| EnvFilter::try_new(d).ok());
    let locked = env_filter.is_some() || config.level_forced;
    let level_filter = |level: DiagLevel| match &env_directives {
        Some(d) if env_filter.is_some() => EnvFilter::new(d),
        _ => EnvFilter::new(level.directives()),
    };

    let mut layers: Vec<Box<dyn Layer<Registry> + Send + Sync>> = Vec::new();
    let mut filters = Vec::new();

    let mut sink = None;
    let mut file_error = None;
    if let Some(dir) = &config.file_dir {
        let run = run_name(&config.program);
        match Sink::start(
            dir,
            run,
            header(&config, env_directives.as_deref()),
            config.limits,
        ) {
            Ok(started) => {
                let (filter, handle) = reload::Layer::new(level_filter(config.level));
                filters.push(handle);
                layers.push(
                    tracing_subscriber::fmt::layer()
                        .with_ansi(false)
                        .with_timer(LocalTime)
                        .with_writer(SinkWriter(Arc::clone(&started)))
                        .with_filter(filter)
                        .boxed(),
                );
                sink = Some(started);
            }
            Err(err) => file_error = Some(format!("无法写入诊断日志目录 {}：{err}", dir.display())),
        }
    }

    match config.stderr {
        StderrOutput::Off => {}
        StderrOutput::FollowLevel => {
            let (filter, handle) = reload::Layer::new(level_filter(config.level));
            filters.push(handle);
            layers.push(
                tracing_subscriber::fmt::layer()
                    .with_timer(LocalTime)
                    .with_writer(std::io::stderr)
                    .with_filter(filter)
                    .boxed(),
            );
        }
        StderrOutput::Env { default } => {
            let filter =
                env_filter_from(&[ENV_VAR, "RUST_LOG"]).unwrap_or_else(|| EnvFilter::new(default));
            layers.push(
                tracing_subscriber::fmt::layer()
                    .with_timer(LocalTime)
                    .with_writer(std::io::stderr)
                    .with_filter(filter)
                    .boxed(),
            );
        }
    }

    let subscriber = Registry::default().with(layers);
    let handle = DiagHandle {
        filters,
        level: Mutex::new(config.level),
        locked,
        env_directives,
        sink,
        dir: config.file_dir,
        file_error,
    };
    (subscriber, handle)
}

fn env_filter_from(vars: &[&str]) -> Option<EnvFilter> {
    vars.iter().find_map(|name| {
        let value = std::env::var(name).ok().filter(|v| !v.trim().is_empty())?;
        EnvFilter::try_new(value).ok()
    })
}

fn log_startup(handle: &DiagHandle) {
    if let Some(err) = handle.file_error() {
        tracing::error!(target: "bax_engine::diag", "{err}");
    }
    if let Some(d) = handle.env_directives()
        && EnvFilter::try_new(d).is_err()
    {
        tracing::warn!(target: "bax_engine::diag", "环境变量 {ENV_VAR} 的内容无法识别，改用设置里的档位");
    }
}

/// 这次运行的名字：`<程序>-<启动时间>-<进程号>`。
fn run_name(program: &str) -> String {
    let program: String = program
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    format!(
        "{program}-{}-{}",
        chrono::Local::now().format("%Y%m%d-%H%M%S"),
        std::process::id()
    )
}

fn header(config: &DiagConfig, env: Option<&str>) -> Vec<String> {
    let build = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    let level = match env {
        Some(d) => format!("{ENV_VAR}={d}"),
        None if config.level_forced => format!("{}（启动参数指定）", config.level.as_str()),
        None => config.level.as_str().to_owned(),
    };
    let mut lines = vec![
        "BAX 诊断日志。本文件含玩家 ID，分享时请用“导出诊断包”，不要直接发送本文件。".to_owned(),
        format!(
            "程序：{} {VERSION}（{build}，提交 {COMMIT}）",
            config.program
        ),
        format!("系统：{}", os_version()),
        format!("进程号：{}", std::process::id()),
        format!("启动时间：{}", local_now()),
        format!("诊断档位：{level}"),
    ];
    for (key, value) in &config.extra {
        let mut line = String::new();
        let _ = write!(line, "{key}：{value}");
        lines.push(line);
    }
    lines
}

/// 本地时间，带时区偏移，精确到毫秒。游戏日志也用本地时间，两边可以直接对照。
pub(crate) fn local_now() -> String {
    chrono::Local::now()
        .format("%Y-%m-%dT%H:%M:%S%.3f%:z")
        .to_string()
}

#[derive(Debug, Clone, Copy)]
struct LocalTime;

impl FormatTime for LocalTime {
    fn format_time(&self, w: &mut Writer<'_>) -> std::fmt::Result {
        w.write_str(&local_now())
    }
}

/// panic 时写一条 error，等写线程刷盘，再交给原来的钩子。
fn install_panic_hook(sink: Option<Arc<Sink>>) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        let thread = thread.name().unwrap_or("<未命名>");
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_default();
        let payload = info.payload();
        let message = payload
            .downcast_ref::<&str>()
            .map(|s| (*s).to_owned())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_default();
        // 发布版去掉了符号，回溯只有地址，只在开发版里记录
        if cfg!(debug_assertions) {
            let backtrace = std::backtrace::Backtrace::force_capture();
            tracing::error!(target: "bax_engine::diag", thread = %thread, location = %location, %backtrace, "程序崩溃：{message}");
        } else {
            tracing::error!(target: "bax_engine::diag", thread = %thread, location = %location, "程序崩溃：{message}");
        }
        if let Some(sink) = &sink {
            sink.flush(FLUSH_TIMEOUT);
        }
        previous(info);
    }));
}

/// Windows 的版本，例如 `Windows 11 25H2（内部版本 26200.7171）`。
#[cfg(windows)]
pub fn os_version() -> String {
    use winreg::RegKey;
    use winreg::enums::HKEY_LOCAL_MACHINE;

    let Ok(key) = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion")
    else {
        return "Windows（版本未知）".to_owned();
    };
    let build: Option<String> = key.get_value("CurrentBuild").ok();
    let ubr: Option<u32> = key.get_value("UBR").ok();
    let display: Option<String> = key.get_value("DisplayVersion").ok();
    // ProductName 在 Windows 11 上仍然写着 Windows 10，按内部版本号区分
    let major = match build.as_deref().and_then(|b| b.parse::<u32>().ok()) {
        Some(b) if b >= 22_000 => "Windows 11",
        Some(_) => "Windows 10",
        None => "Windows",
    };
    let mut text = major.to_owned();
    if let Some(display) = display {
        text += " ";
        text += &display;
    }
    if let Some(build) = build {
        match ubr {
            Some(ubr) => text += &format!("（内部版本 {build}.{ubr}）"),
            None => text += &format!("（内部版本 {build}）"),
        }
    }
    text
}

#[cfg(not(windows))]
pub fn os_version() -> String {
    std::env::consts::OS.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_hides_debug_and_verbose_shows_it() {
        let dir = tempfile::tempdir().expect("临时目录");
        let mut config = DiagConfig::new("unit");
        config.file_dir = Some(dir.path().to_path_buf());
        config.panic_hook = false;
        let (subscriber, handle) = build(config);
        tracing::subscriber::with_default(subscriber, || {
            tracing::debug!(target: "bax_engine::session", "看不到的细节");
            tracing::info!(target: "bax_engine::session", player = 101, "看得到");
            tracing::info!(target: "hyper", "别人的 info 看不到");
            assert!(handle.set_level(DiagLevel::Verbose));
            tracing::debug!(target: "bax_engine::session", "切换后看得到的细节");
        });
        handle.flush();
        let text = std::fs::read_to_string(handle.current_file().expect("有文件")).expect("读文件");
        assert!(text.starts_with("# BAX 诊断日志。"));
        assert!(!text.contains("看不到的细节"));
        assert!(text.contains("看得到 player=101"));
        assert!(!text.contains("别人的 info"));
        assert!(text.contains("切换后看得到的细节"));
        let line = text
            .lines()
            .find(|l| l.contains("看得到 player=101"))
            .expect("有这一行");
        // 本地时间带偏移、级别、target 都在一行里
        assert!(line.contains(" INFO bax_engine::session: "), "{line}");
        assert!(line.as_bytes()[4] == b'-' && line.contains('T'), "{line}");
    }

    #[test]
    fn a_forced_level_ignores_later_changes() {
        let mut config = DiagConfig::new("unit");
        config.level = DiagLevel::Verbose;
        config.level_forced = true;
        config.panic_hook = false;
        let (_subscriber, handle) = build(config);
        assert!(handle.locked());
        assert!(!handle.set_level(DiagLevel::Standard));
        assert_eq!(handle.level(), DiagLevel::Verbose);
    }

    #[test]
    fn settings_round_trip_with_defaults() {
        let parsed: DiagSettings =
            serde_json::from_value(serde_json::json!({ "level": "verbose" })).expect("解析");
        assert_eq!(parsed.level, DiagLevel::Verbose);
        let empty: DiagSettings = serde_json::from_value(serde_json::json!({})).expect("解析");
        assert_eq!(empty.level, DiagLevel::Standard);
    }
}
