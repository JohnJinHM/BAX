# BAX

《Broken Arrow／断箭》的对局辅助工具。通过本机游戏日志识别对局，从 BATrace 获取玩家与比赛资料，并保存为本地长期记录。

**当前阶段：0.2 开发完成，等待实机验收；0.3（玩家档案与备注）与 0.4（战后分析）已开发完成。** 桌面应用已经有对局悬浮窗、主窗口、托盘、计时提醒、玩家档案和战后分析；悬浮窗叠在游戏上的表现需要按[实机验收清单](docs/guides/acceptance-0.2.md)确认后再发布 0.2.0。

## 功能

已经实现：

- 自动找到游戏日志，实时识别比赛的阶段、双方玩家和本机玩家，处理断线与重连
- 每场比赛查询双方玩家的 ELO、等级、排名和排位战绩：先显示本地已有的资料，再刷新
- 本地保存玩家档案、ELO 变化和比赛名单，统计以前同场的次数；可以导入历史日志
- 推算正式开局时刻，与战报的误差在 2 秒以内；可以对照游戏画面手动校准
- 对局悬浮窗：置顶、半透明，显示计时和双方玩家；锁定时鼠标穿透到游戏，不抢游戏焦点
- 计时提醒：按正式开局后的时间点响铃并闪烁，每场每条只提醒一次
- 玩家档案：按名字或 ID 查找，查看 ELO 变化与同场比赛，写备注；备注也显示在对局和悬浮窗里
- 战后分析：比赛结束后自动获取并保存战报，展示双方对比、出兵价值曲线、每次出兵的时间线和兵种构成
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
npm install
npx tauri build --no-bundle            # 生成 target\release\bax-app.exe
cargo build --release -p bax-cli
.\target\release\bax.exe logs import   # 可选：导入历史日志，用于统计以前同场的次数
.\target\release\bax-app.exe           # 启动桌面应用，常驻托盘
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
