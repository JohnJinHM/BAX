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

TLS 使用 Windows 自带的 SChannel，不需要安装 CMake、NASM 或 OpenSSL。

桌面应用另外需要：

| 工具 | 要求 | 说明 |
|---|---|---|
| Node.js | 22.12 或更新 | 前端构建（Vite 7 的要求） |
| npm | 随 Node.js 安装 | 依赖锁定在 `package-lock.json` |
| WebView2 运行时 | Windows 11 自带 | Windows 10 上安装包会自动下载 |

### 已知问题：全局的 CC 环境变量

如果系统里设置了 `CC`（例如直接指向 `cl.exe`），`cc` crate 会原样使用它，跳过 MSVC 环境探测，编译 SQLite 时报 `stdarg.h: no include path set`。仓库的 `.cargo/config.toml` 已经把 `CC` 置空，在仓库里构建不受影响；其他 Rust 项目遇到同样的错误时，删除这个全局变量即可。

### 依赖版本

本机工具链是 Rust 1.88，几个依赖因此停在旧一点的版本，升级工具链后可以一起放开（见 [ADR-0006](../adr/0006-frontend-stack.md)）：

- `rusqlite` 固定在 0.40：0.41 起要求 Rust 1.89。
- `tauri` 停在 2.11：2.12 起要求 Rust 1.90。前端的 `@tauri-apps/api` 与 `@tauri-apps/cli` 也要停在 2.11，Tauri 要求两边的次版本一致。
- TypeScript 停在 5.9，`vue-tsc` 对 7.0 的支持还没有验证。

## 常用命令

```powershell
cargo test --workspace                          # 全部测试
cargo clippy --workspace --all-targets          # 静态检查，提交前应当没有警告
cargo fmt --all                                 # 格式化
cargo build --release -p bax-cli                # 构建命令行工具 target\release\bax.exe
```

测试不访问网络，也不读本机的游戏日志。引擎的端到端测试在临时目录里写日志、用本地模拟服务器代替 BATrace。

## 桌面应用

```powershell
npm install                       # 第一次
npm run tauri dev                 # 开发：启动 Vite 并运行调试版，前端改动即时生效
npx tauri build --no-bundle       # 发布版 target\release\bax-app.exe，不打安装包
npx tauri build                   # 发布版并打 NSIS 安装包
npm run typecheck                 # 前端类型检查
npm run bindings                  # Rust 一侧改了快照类型之后，重新生成 src/shared/bindings
```

`bax-app.exe --hidden` 启动时不显示悬浮窗，只有托盘图标。数据目录与命令行工具相同，也可以用 `BAX_DATA_DIR` 指定。

### 诊断日志

桌面应用把运行过程写进 `%LOCALAPPDATA%\BAX\diag\`，每次启动一个文件，开发版同时输出到终端。格式、档位和隐私规则见 [ADR-0007](../adr/0007-diagnostic-logging.md)。

| 想要 | 做法 |
|---|---|
| 看请求、事件等细节 | 主窗口“设置·诊断”选“详细”，立即生效；或者用 `bax-app.exe --verbose` 启动 |
| 只看某个模块，或者开到 trace | `$env:BAX_LOG = "warn,bax_engine::session=trace"`，再启动；设了这个变量后界面上的档位不能改 |
| 跟踪正在写的文件 | `Get-Content -Wait -Encoding utf8 <文件>` |
| 只看一场比赛 | 在文件里搜索 `match_id=<比赛 ID>` |
| 发给别人 | 设置页“导出诊断包”，或 `bax diag export`；不要直接发送 `.log` 文件，里面有玩家 ID |

新增埋点时遵守这几条：玩家 ID 只放在 `player` 或 `self_player` 字段，比赛 ID 只放在 `match_id` 字段；不写玩家名称、卡组名、请求 URL 和游戏日志原文；字符串字段用 `%` 写出。引擎的 `diag_privacy` 测试会检查这些约定。

### 不开桌面应用预览界面

`dev/` 下的页面用 Tauri 官方的 `mockIPC` 冒充 Rust 一侧，喂给界面一份虚构的快照（`src/dev/fixture.ts`），用普通浏览器就能看：

```powershell
npx vite                          # 然后打开下面的地址
# http://localhost:1420/dev/overlay.html           悬浮窗（加 ?locked 看锁定状态）
# http://localhost:1420/dev/main.html?tab=reminders 主窗口，tab 可为 match、reminders、settings
```

这些页面不进发布包。改了快照结构时，`npm run typecheck` 会检查虚构快照是否还对得上。

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
| `bax diag path` | 显示诊断日志目录和其中的文件 |
| `bax diag export [--runs N] [--out 文件]` | 导出最近 N 次运行（默认 2）的诊断包，玩家 ID、比赛 ID 和用户名已替换 |

全局参数 `--data-dir` 指定数据目录；不指定时使用环境变量 `BAX_DATA_DIR`，再不指定则是 `%LOCALAPPDATA%\BAX`，与桌面应用相同。试验时建议指向一个临时目录，免得把试验数据混进日常使用的数据：

```powershell
.\target\release\bax.exe --data-dir .local\data logs import
.\target\release\bax.exe --data-dir .local\data run
```

`.local/` 不入库。库里的警告写到标准错误，用 `BAX_LOG` 或 `RUST_LOG` 调整详细程度，例如 `$env:BAX_LOG = "bax_engine=debug"`。全局参数 `--diag-file` 另外写一份与桌面应用相同格式的诊断日志，例如用 `bax run --diag-file` 复现问题时保留记录。

## 测试数据

- 测试和 `fixtures/` 只用虚构的玩家名、ID、卡组名和比赛编号。真实日志里抄出来的片段要先替换。
- 需要真实数据的检查（全量回放、原型缓存解析）通过命令行工具手动运行，结论写进 [validation/](../validation/README.md) 的记录，不进自动测试。
- `fixtures/` 下的文件按原始字节保存（见 `.gitattributes`），不要用会改写换行或编码的编辑器处理。
