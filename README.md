# BAX

《Broken Arrow／断箭》的对局辅助工具。通过本机游戏日志识别对局，从 BATrace 获取玩家与比赛资料，并保存为本地长期记录。

**当前阶段：需求整理与可行性验证。** 仓库里目前只有文档，还没有可运行的程序。

## 计划中的功能

- 对局悬浮窗：自动识别当前比赛和双方玩家，显示 ELO、等级、备注与简要画像
- 计时提醒：按正式开局后的时间点提醒
- 玩家档案：缓存资料，保存备注和历史相遇记录
- 风格统计：常用单位、兵种构成、出兵节奏、占点与补给倾向
- 战后分析：比赛结束后获取战报，展示出兵时间线和对比图
- 自定义皮肤：主题、颜色、透明度、字号与间距

首阶段不包含作弊、小号或异常行为的判定，见 [ADR-0002](docs/adr/0002-exclude-cheat-detection.md)。

## 技术栈（暂定）

Tauri＋Rust 桌面应用，网页前端，SQLite 本地存储，目标平台为 Windows。前端框架等选型在可行性验证后确定，见 [ADR-0001](docs/adr/0001-desktop-stack.md)。

## 文档

| 文档 | 内容 |
|---|---|
| [项目规划](docs/planning/project-plan.md) | 范围、数据来源、技术方案与推进顺序 |
| [总体架构](docs/architecture/overview.md) | 模块、数据流、仓库结构 |
| [架构决策记录](docs/adr/README.md) | 关键决定及其理由 |
| [可行性验证](docs/validation/README.md) | 五项验证的进度与记录 |
| [文档索引与约定](docs/README.md) | 文档放在哪里、怎么写、何时更新 |
| [协作约定](CONTRIBUTING.md) | 分支、提交与版本 |

## 来源与许可

BAX 采用 MIT 许可，见 [LICENSE](LICENSE)。

BAX 以 BA-ELO-Overlay 原型为基础重新实现，已取得原作者同意。原型同样采用 MIT 许可，来源与许可声明见 [NOTICE.md](NOTICE.md)。
