# BAX

《Broken Arrow／断箭》的对局辅助工具。通过本机游戏日志识别对局，从 BATrace 获取玩家与比赛资料，并保存为本地长期记录。

**当前阶段：0.1 已完成。** 对局识别与玩家查询的后台部分已经可用，暂时通过命令行工具 `bax run` 查看；桌面窗口与对局悬浮窗从 0.2 开始。

## 功能

已经实现（0.1）：

- 自动找到游戏日志，实时识别比赛的阶段、双方玩家和本机玩家，处理断线与重连
- 每场比赛查询双方玩家的 ELO、等级、排名和排位战绩：先显示本地已有的资料，再刷新
- 本地保存玩家档案、ELO 变化和比赛名单，统计以前同场的次数；可以导入历史日志
- 推算正式开局时刻，与战报的误差在 2 秒以内

计划中：

- 对局悬浮窗：自动识别当前比赛和双方玩家，显示 ELO、等级、备注与简要画像
- 计时提醒：按正式开局后的时间点提醒
- 玩家档案：缓存资料，保存备注和历史相遇记录
- 风格统计：常用单位、兵种构成、出兵节奏、占点与补给倾向
- 战后分析：比赛结束后获取战报，展示出兵时间线和对比图
- 自定义皮肤：主题、颜色、透明度、字号与间距

首阶段不包含作弊、小号或异常行为的判定，见 [ADR-0002](docs/adr/0002-exclude-cheat-detection.md)。

## 技术栈

Rust 后台，SQLite 本地存储，目标平台为 Windows。桌面外壳暂定 Tauri，前端框架等选型在可行性验证后确定，见 [ADR-0001](docs/adr/0001-desktop-stack.md)。

## 试用

需要 Rust 1.88 以上和 MSVC 生成工具，详见[开发指南](docs/guides/development.md)。

```powershell
cargo build --release -p bax-cli
.\target\release\bax.exe logs import   # 可选：导入历史日志，用于统计以前同场的次数
.\target\release\bax.exe run           # 打开游戏后运行，Ctrl+C 结束
```

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
