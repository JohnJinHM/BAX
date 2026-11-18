//! BAX 的开发与验证命令行。
//!
//! 它直接使用各个库，不经过界面，用来在真实日志和真实接口上检查行为。
//! 输出里会出现玩家名字和 ID，只应在本机查看，不要贴到公开的地方。

mod api;
mod diag;
mod logs;
mod pregame;
mod run;

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "bax", version, about = "BAX 开发与验证工具")]
struct Cli {
    /// 数据目录；省略时使用环境变量 BAX_DATA_DIR，再省略则是 %LOCALAPPDATA%\BAX
    #[arg(long, global = true)]
    data_dir: Option<PathBuf>,
    /// 同时把诊断日志写进数据目录下的 diag\（与桌面应用相同的格式）
    #[arg(long, global = true)]
    diag_file: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// 游戏日志：定位、回放、监视、导入
    Logs {
        #[command(subcommand)]
        command: LogsCommand,
    },
    /// 无界面运行引擎，打印当前比赛与玩家资料的每次变化（Ctrl+C 结束）
    Run {
        /// 日志目录；省略时用记住的目录，再省略则自动定位
        #[arg(long)]
        dir: Option<PathBuf>,
        /// 每份快照输出一行 JSON
        #[arg(long)]
        json: bool,
    },
    /// 访问 BATrace 接口，或者检查数据模型
    Api {
        #[command(subcommand)]
        command: ApiCommand,
    },
    /// 诊断日志：查看位置，导出脱敏后的诊断包
    Diag {
        #[command(subcommand)]
        command: DiagCommand,
    },
}

#[derive(Debug, Subcommand)]
enum DiagCommand {
    /// 显示诊断日志目录和其中的文件
    Path,
    /// 导出最近几次运行的诊断包，玩家 ID、比赛 ID 和 Windows 用户名已替换
    Export {
        /// 导出最近几次运行
        #[arg(long, default_value_t = 2)]
        runs: usize,
        /// 输出文件；省略时写到诊断目录下的 export\
        #[arg(long)]
        out: Option<PathBuf>,
    },
}

#[derive(Debug, Subcommand)]
enum LogsCommand {
    /// 显示自动找到的日志目录和全部候选位置
    Find,
    /// 核查等待室线索与完整名单的覆盖率，不访问接口
    Pregame {
        /// 要核查的日志；省略时使用最新日志
        file: Option<PathBuf>,
        #[arg(long)]
        dir: Option<PathBuf>,
        /// 核查目录里的全部日志
        #[arg(long, conflicts_with = "file")]
        all: bool,
    },
    /// 回放日志，检查解析与对局识别的结果
    Replay {
        /// 要回放的日志文件；省略时回放目录里最新的一份
        file: Option<PathBuf>,
        /// 日志目录；省略时自动定位
        #[arg(long)]
        dir: Option<PathBuf>,
        /// 回放目录里的全部日志，只输出汇总
        #[arg(long, conflicts_with = "file")]
        all: bool,
        /// 逐条打印解析出的事件
        #[arg(long)]
        events: bool,
        /// 配合 --all：每场比赛输出一行制表符分隔的数据，用于和战报核对
        #[arg(long, requires = "all")]
        tsv: bool,
    },
    /// 监视日志目录，实时打印事件与对局状态变化
    Watch {
        /// 日志目录；省略时自动定位
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// 把历史日志里的比赛导入长期库，用于统计以前同场的次数
    Import {
        /// 日志目录；省略时自动定位
        #[arg(long)]
        dir: Option<PathBuf>,
        /// 只导入最近的若干份日志
        #[arg(long)]
        recent: Option<usize>,
    },
}

#[derive(Debug, Subcommand)]
enum ApiCommand {
    /// 查询一名玩家的资料
    Player {
        id: u64,
        /// 输出响应原文
        #[arg(long)]
        raw: bool,
    },
    /// 查询一场比赛的战报
    Match {
        id: u64,
        /// 输出响应原文
        #[arg(long)]
        raw: bool,
    },
    /// 查询单位目录
    Units {
        /// 输出响应原文
        #[arg(long)]
        raw: bool,
    },
    /// 用 BA-ELO-Overlay 原型缓存里的真实响应检查数据模型（只读，不联网）
    CheckCache {
        /// 原型的 elo_cache.sqlite
        path: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let data_dir = || -> Result<PathBuf> {
        cli.data_dir
            .clone()
            .or_else(bax_engine::default_data_dir)
            .context("无法确定数据目录，请用 --data-dir 指定")
    };
    // 库里的警告写到标准错误，BAX_LOG 或 RUST_LOG 可以调整，例如 bax_engine=debug。
    // 加 --diag-file 时另外写一份诊断日志文件（ADR-0007）
    let mut config = bax_engine::diag::DiagConfig::new("bax");
    config.stderr = bax_engine::diag::StderrOutput::Env { default: "warn" };
    config.panic_hook = cli.diag_file;
    if cli.diag_file {
        config.file_dir = Some(bax_engine::diag::diag_dir(&data_dir()?));
    }
    let _diag = bax_engine::diag::init(config);

    match cli.command {
        Command::Logs { command } => match command {
            LogsCommand::Find => logs::find(),
            LogsCommand::Pregame { file, dir, all } => pregame::run(file, dir, all),
            LogsCommand::Replay {
                file,
                dir,
                all,
                events,
                tsv,
            } => logs::replay(file, dir, all, events, tsv),
            LogsCommand::Watch { dir } => logs::watch(dir),
            LogsCommand::Import { dir, recent } => logs::import(&data_dir()?, dir, recent),
        },
        Command::Run { dir, json } => run::run(data_dir()?, dir, json),
        Command::Api { command } => api::api(command),
        Command::Diag { command } => match command {
            DiagCommand::Path => diag::path(&data_dir()?),
            DiagCommand::Export { runs, out } => diag::export(&data_dir()?, runs, out),
        },
    }
}
