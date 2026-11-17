# 开发指南

状态：生效 · 更新：2026-11-17

本文说明如何在本机构建、测试和运行 BAX。分支与提交约定见 [CONTRIBUTING.md](../../CONTRIBUTING.md)。

## 环境

| 工具 | 要求 | 说明 |
|---|---|---|
| Windows | 10 或 11 | 目标平台 |
| Rust | 1.88 或更新，`x86_64-pc-windows-msvc` | 工作区声明了 `rust-version = "1.88"` |
| MSVC 生成工具 | Visual Studio 2022 或 Build Tools，含“使用 C++ 的桌面开发” | 编译内置的 SQLite |
| rustfmt、clippy | `rustup component add rustfmt clippy` | 提交前检查 |

TLS 使用 Windows 自带的 SChannel，不需要安装 CMake、NASM 或 OpenSSL。0.2 起还需要 Node.js 和 WebView2 运行时，到时补充。

### 已知问题：全局的 CC 环境变量

如果系统里设置了 `CC`（例如直接指向 `cl.exe`），`cc` crate 会原样使用它，跳过 MSVC 环境探测，编译 SQLite 时报 `stdarg.h: no include path set`。仓库的 `.cargo/config.toml` 已经把 `CC` 置空，在仓库里构建不受影响；其他 Rust 项目遇到同样的错误时，删除这个全局变量即可。

### 依赖版本

`rusqlite` 固定在 0.40：0.41 起要求 Rust 1.89。升级工具链后可以放开。

## 常用命令

```powershell
cargo test --workspace                          # 全部测试
cargo clippy --workspace --all-targets          # 静态检查，提交前应当没有警告
cargo fmt --all                                 # 格式化
cargo build --release -p bax-cli                # 构建命令行工具 target\release\bax.exe
```

测试不访问网络，也不读本机的游戏日志。引擎的端到端测试在临时目录里写日志、用本地模拟服务器代替 BATrace。

## 命令行工具 bax

`bax` 直接使用各个库，用来在真实日志和真实接口上检查行为。它的输出里有其他玩家的名字和 ID，只在本机查看，不要贴到公开的地方。

| 命令 | 作用 |
|---|---|
| `bax logs find` | 显示自动找到的日志目录和全部候选位置 |
| `bax logs replay [文件] [--events]` | 回放一份日志（默认最新的一份），列出每场比赛的识别结果 |
| `bax logs replay --all [--tsv]` | 回放目录里全部日志，输出汇总；`--tsv` 每场比赛一行，用于和战报核对 |
| `bax logs watch` | 实时打印日志事件与对局状态变化 |
| `bax logs import [--recent N]` | 把历史日志里的比赛导入长期库，用于统计以前同场的次数 |
| `bax run [--json]` | 无界面运行引擎，打印每次状态变化（Ctrl+C 结束） |
| `bax api player <ID>` / `match <ID>` / `units` | 访问 BATrace，`--raw` 输出响应原文 |
| `bax api check-cache <原型缓存>` | 用原型 `elo_cache.sqlite` 里的真实响应检查数据模型，不联网 |

全局参数 `--data-dir` 指定数据目录；不指定时使用环境变量 `BAX_DATA_DIR`，再不指定则是 `%LOCALAPPDATA%\BAX`，与桌面应用相同。试验时建议指向一个临时目录，免得把试验数据混进日常使用的数据：

```powershell
.\target\release\bax.exe --data-dir .local\data logs import
.\target\release\bax.exe --data-dir .local\data run
```

`.local/` 不入库。库里的警告写到标准错误，用 `RUST_LOG` 调整详细程度，例如 `$env:RUST_LOG = "bax_engine=debug"`。

## 测试数据

- 测试和 `fixtures/` 只用虚构的玩家名、ID、卡组名和比赛编号。真实日志里抄出来的片段要先替换。
- 需要真实数据的检查（全量回放、原型缓存解析）通过命令行工具手动运行，结论写进 [validation/](../validation/README.md) 的记录，不进自动测试。
- `fixtures/` 下的文件按原始字节保存（见 `.gitattributes`），不要用会改写换行或编码的编辑器处理。
