# BAX

《Broken Arrow／断箭》的对局辅助工具。通过本机游戏日志识别对局，从 BATrace 获取玩家与比赛资料，并保存为本地长期记录。

**当前版本：0.2.0。** 首个公开版本包含对局悬浮窗、主窗口、托盘、计时提醒、玩家档案和战后分析。从 [GitHub Releases](https://github.com/JohnJinHM/BAX/releases/latest) 下载 Windows x64 的 Full 或 Lite 安装包、便携版；验收范围与限制见发布说明。

## 功能

已经实现：

- 自动找到游戏日志，实时识别比赛的阶段、双方玩家和本机玩家，处理断线与重连
- 每场比赛查询双方玩家的 ELO、等级、排名和排位战绩：先显示本地已有的资料，再刷新
- 本地保存玩家档案、ELO 变化和比赛名单，统计以前同场的次数；可以导入历史日志
- 推算正式开局时刻，与战报的误差在 2 秒以内；可以对照游戏画面手动校准
- 对局悬浮窗：置顶、半透明，显示计时和双方玩家；锁定时鼠标穿透到游戏，不抢游戏焦点
- 计时提醒：按正式开局后的时间点响铃并闪烁，每场每条只提醒一次
- 玩家档案：按名字或 ID 查找，查看 ELO 变化与同场比赛，写备注；备注也显示在对局和悬浮窗里
- 专精：选图／选卡时在悬浮窗显示上场组合，加载后收起，赛后切换为本局组合；当前对局详情也有专精列。后台最多补查 5 份敌方最近战报；个人页展示已有战报场次，见[统计口径](docs/reference/player-specializations.md)
- 自定义与 PvE：按实际人数显示双方与旁观者，识别 PvE 后暂停本局 OCR，见[模式识别](docs/reference/match-modes.md)
- GitHub 更新：启动自动检查，安装版点击下载并安装；Full / Lite 分开更新，见[发布指南](docs/guides/releases.md)
- 战后分析：比赛结束后归档战报，展示双方对比、净出兵价值曲线、出兵时间线和含回收的兵种构成；玩家与兵种支持多选，分析顶栏随滚动置顶
- 主窗口：完整的玩家资料表、提醒与校准、设置；托盘菜单与全局快捷键（默认 Ctrl+Alt+B 显示或隐藏、Ctrl+Alt+L 锁定或解锁）

计划中：

- 风格统计：常用单位、兵种构成、出兵节奏、占点与补给倾向
- 自定义皮肤：主题、颜色、透明度、字号与间距

首阶段不包含作弊、小号或异常行为的判定，见 [ADR-0002](docs/adr/0002-exclude-cheat-detection.md)。

## 技术栈

Rust 后台，SQLite 本地存储，Tauri 桌面外壳，Vue 3＋TypeScript 前端，目标平台为 Windows。选型见 [ADR-0001](docs/adr/0001-desktop-stack.md) 与 [ADR-0006](docs/adr/0006-frontend-stack.md)，两者都等资源占用的验证结果确认。

## 试用

需要 Rust 1.88 以上、MSVC 生成工具和 Node.js 22，详见[开发指南](docs/guides/development.md)。

```powershell
pwsh -File scripts/build.ps1 -Mode Dev -Edition Lite
cargo build --release -p bax-cli
.\target\release\bax.exe logs import   # 可选：导入历史日志，用于统计以前同场的次数
.\target\editions\lite\debug\bax-app.exe # 启动桌面应用，常驻托盘
```

不需要窗口时，`.\target\release\bax.exe run` 在命令行里显示同样的信息。

数据保存在 `%LOCALAPPDATA%\BAX`。

## 文档

| 文档 | 内容 |
|---|---|
| [项目规划](docs/planning/project-plan.md) | 范围、数据来源、技术方案与推进顺序 |
| [总体架构](docs/architecture/overview.md) | 模块、数据流、仓库结构 |
| [架构决策记录](docs/adr/README.md) | 关键决定及其理由 |
| [可行性验证](docs/validation/README.md) | 五项验证的进度与记录 |
| [开发指南](docs/guides/development.md) | 环境、构建、测试与命令行工具 |
| [文档索引与约定](docs/README.md) | 文档放在哪里、怎么写、何时更新 |
| [协作约定](CONTRIBUTING.md) | 分支、提交与版本 |

## 来源与许可

BAX 采用 MIT 许可，见 [LICENSE](LICENSE)。

BAX 以 BA-ELO-Overlay 原型为基础重新实现，已取得原作者同意。原型同样采用 MIT 许可，来源与许可声明见 [NOTICE.md](NOTICE.md)。

## Full 与 Lite

Full 的“赛前玩家识别”默认启用；Lite 不提供 OCR，继续从日志获取正式名单。先按画面昵称与等级匹配本地账号，必要时查询在线候选；正式日志 ID 到达后更新。候选不会写成比赛参与记录。首版支持双侧卡片布局，只采集前台游戏客户区，不保存或上传截图。

两个版本在 `main` 共用一套代码，通过构建选项区分，共享数据目录并支持覆盖安装切换。

Full 安装包包含 C++/ONNX Runtime 工作进程和两个文字识别模型，不需要 Python 或 OpenCV，首次识别不下载模型。按双侧五行卡片的固定区域识别昵称与等级；当前验证覆盖约 16:9 的加载/部署截图，其他布局等待日志。支持现有 v5 数据库，v4 会在备份后自动升级。

统一构建，以下命令中的 Edition 都可换成 Lite：

```powershell
pwsh -File scripts/build.ps1 -Mode Dev -Edition Full       # 独立 Debug EXE
pwsh -File scripts/build.ps1 -Mode Portable -Edition Full  # 便携 ZIP
pwsh -File scripts/build.ps1 -Mode Installer -Edition Full # 安装包
```

构建依赖、产物位置和增量构建方法见 [打包指南](docs/guides/packaging.md)。CLI 不自动采集画面；游戏内验收范围见 [赛前验证](docs/validation/pregame-roster.md)。
