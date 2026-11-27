# BAX

《Broken Arrow／断箭》的对局辅助工具。通过本机游戏日志识别对局，优先从游戏数据源获取玩家基础资料与战报，由 BATrace 补充缺失功能，并保存为本地长期记录。

**当前版本：0.3.0。** 包含对局悬浮窗、主窗口、托盘、计时提醒、玩家档案和战后分析。从 [GitHub Releases](https://github.com/JohnJinHM/BAX/releases/latest) 下载 Windows x64 的 Full 或 Lite 安装包、便携版；验收范围见发布说明。

## 功能

已经实现：

- 自动找到游戏日志，实时识别比赛的阶段、双方玩家和本机玩家，处理断线与重连
- 每场比赛查询双方玩家的 ELO、等级、排名和排位战绩：先显示本地已有的资料，再刷新
- 本地保存玩家档案、ELO 变化和比赛名单，统计以前同场的次数；可以导入历史日志
- 推算正式开局时刻，与战报的误差在 2 秒以内；可以对照游戏画面手动校准
- 对局悬浮窗：置顶、半透明，显示计时和双方玩家；锁定时鼠标穿透到游戏，不抢游戏焦点
- 计时提醒：按正式开局后的时间点响铃并闪烁，每场每条只提醒一次
- 玩家档案支持搜索、收藏、备注与 ELO 历史。刷新资料时默认同步最近 20 场战报，展示常用型号、开局偏好、作战效能与单位偏好。雷达图基准可在设置中调整，详见[统计口径](docs/reference/battle-analytics.md)。
- 专精：选图／选卡时在悬浮窗显示上场组合，加载后收起，赛后切换为本局组合；当前对局详情也有专精列。后台最多补查 5 份敌方最近战报；个人页展示已有战报场次，见[统计口径](docs/reference/player-specializations.md)
- 自定义与 PvE：按实际人数显示双方与旁观者，识别 PvE 后暂停本局 OCR，见[模式识别](docs/reference/match-modes.md)
- GitHub 更新：启动自动检查，安装版点击下载并安装；Full / Lite 分开更新，见[发布指南](docs/guides/releases.md)
- 战后分析提供双方对比、累计净出兵与损失曲线、存续时间和出兵批次。出兵分布与补给消耗树图可下钻至单位型号，补给往来图可对照队友间使用和提供的补给量。
- 主窗口：完整的玩家资料表、提醒与校准、设置；托盘菜单与全局快捷键（默认 Ctrl+Alt+B 显示或隐藏、Ctrl+Alt+L 锁定或解锁）
- 外观默认使用「断箭」，也可切换「经典」，调整颜色、字号与间距。悬浮窗可调整宽度、拖动吸附屏幕边缘，主窗口记住位置与大小。
- 数据源状态：设置中手动查询各接口的可用性、响应时间和检查时间

玩家单字标签、判定规则与 STB 历史战报见 [ADR-0016](docs/adr/0016-player-labels-and-stb-history.md)。

## 数据来源

| 来源 | 用途 |
|---|---|
| 本机 GameLogs | 识别对局、阶段、正式名单、本机队伍与比赛结束 |
| `api.brokenarrowgame.tech` | 优先获取已知玩家 ID 的基础资料、近期比赛 ID 和部分累计统计 |
| `s3.brokenarrowgame.tech` | 优先读取完整战报，包括比赛结果、玩家统计和单位实例 |
| [BATrace](https://bot.batrace.top/) | 提供社区标记、昵称搜索和单位／专精目录 |

BAX 在本机转换字段、合并缺失资料，复用缓存并合并重复请求；各来源分别处理限流和故障暂停。网络失败时保留已有玩家资料和已归档战报。Full 的赛前 OCR 在本地处理截图，昵称匹配仍可能使用 BATrace 搜索。

目前已验证玩家基础资料及排位、自定义战报。累计价值等统计字段仍需要补充源，PvE 战报覆盖情况尚未确认；接口的正式配额与服务承诺也未确认。可以在**设置 → 数据源 → 查询数据源状态**查看当前检查结果。字段映射与缓存规则见[数据源参考](docs/reference/batrace-api.md)，样本和限制见[兼容性验证](docs/validation/upstream-api-compatibility.md)。

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

安装版数据保存在 `%LOCALAPPDATA%\BAX`，便携版使用启动脚本旁的 `data` 目录。首次打开已有长期库时会在数据目录生成对应旧版本的备份（如 `bax.db.v7.bak`），随后升级至结构 v12。回到旧版时需使用对应版本的备份。

## Full 与 Lite

Full 的“赛前玩家识别”默认启用；Lite 不提供 OCR，继续从日志获取正式名单。先按画面昵称与等级匹配本地账号，必要时查询在线候选；正式日志 ID 到达后更新。候选不会写成比赛参与记录。首版支持双侧卡片布局，只采集游戏客户区，切到后台后仍可识别，最小化时暂停采集，不保存或上传截图。

两个版本在 `main` 共用一套代码，通过构建选项区分，共享数据目录并支持覆盖安装切换。

Full 安装包包含 C++/ONNX Runtime 工作进程和两个文字识别模型，不需要 Python 或 OpenCV，首次识别不下载模型。按双侧五行卡片的固定区域识别昵称与等级；当前验证覆盖约 16:9 的加载/部署截图，其他布局等待日志。

统一构建，以下命令中的 Edition 都可换成 Lite：

```powershell
pwsh -File scripts/build.ps1 -Mode Dev -Edition Full       # 独立 Debug EXE
pwsh -File scripts/build.ps1 -Mode Portable -Edition Full  # 便携 ZIP
pwsh -File scripts/build.ps1 -Mode Installer -Edition Full # 安装包
```

构建依赖、产物位置和增量构建方法见 [打包指南](docs/guides/packaging.md)。CLI 不自动采集画面；游戏内验收范围见 [赛前验证](docs/validation/pregame-roster.md)。

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

## 致谢

- BA-ELO-Overlay 作者 [零壹ZERONE](https://space.bilibili.com/501523106)（哔哩哔哩）：BAX 以 BA-ELO-Overlay 为基础重新实现，日志识别、玩家查询和悬浮窗都从它起步。
- [BATrace](https://bot.batrace.top/) 数据站：提供玩家资料、战报和单位目录，BAX 的早期验证和补充数据都依靠它。
- [VRCX](https://github.com/vrcx-team/VRCX)：灵感来源，玩家档案、备注与历史记录的组织方式受它启发。

## 来源与许可

BAX 采用 MIT 许可，见 [LICENSE](LICENSE)。

BAX 以 BA-ELO-Overlay 原型为基础重新实现，已取得原作者同意。原型同样采用 MIT 许可。原型、游戏素材、字体与 OCR 运行时的来源与许可声明见 [NOTICE.md](NOTICE.md)。
