# BAX

《Broken Arrow／断箭》对局辅助工具。0.1（对局识别与玩家查询）已发布；0.2（Tauri 桌面应用、悬浮窗、计时提醒）开发完成，等待实机验收。

## 先读这些

- `docs/README.md`：文档地图与写作约定
- `docs/planning/project-plan.md`：范围、数据来源、推进顺序
- `docs/architecture/overview.md`：模块边界、数据流、仓库结构
- `docs/reference/`：游戏日志与 BATrace 接口的字段级说明
- `docs/guides/development.md`：构建、测试、命令行工具
- `CONTRIBUTING.md`：分支、提交信息、版本

## 常用命令

```powershell
cargo test --workspace
cargo clippy --workspace --all-targets   # 提交前应当没有警告
cargo fmt --all
cargo build --release -p bax-cli         # target\release\bax.exe
npm run typecheck                        # 前端类型检查
npm run bindings                         # 改了给界面用的 Rust 类型后重新生成 src/shared/bindings
npx tauri build --no-bundle              # target\release\bax-app.exe
```

界面可以不开桌面应用预览：`npx vite` 后打开 `/dev/overlay.html` 或 `/dev/main.html?tab=…`（虚构数据）。

## 必须遵守

- 不实现作弊、小号或异常行为的判定，也不移植原型 `elo_cheat_check.py` 的检测规则（ADR-0002）。
- 依赖方向：`bax-core` 不做 IO、不读时钟；`bax-gamelog`、`bax-batrace`、`bax-store` 只依赖 `bax-core`；`bax-engine` 依赖这四个库；`src-tauri` 和 `tools/` 依赖引擎。基础库不依赖 Tauri（ADR-0003）。
- 业务状态只在 Rust 一侧维护，前端不保存权威副本（ADR-0004）。`src/shared/bindings` 是生成的，不手改。
- `unsafe` 只允许出现在 `src-tauri/src/win32.rs`；其余成员继承 workspace 的 `unsafe_code = "forbid"`。
- 不要在用户玩游戏时启动带窗口的桌面应用做试验；需要时用 `--hidden` 或 `dev/` 预览页。
- 长期库没有自动过期或批量删除；清理只针对缓存库（ADR-0005）。
- 原始日志和接口响应含他人玩家 ID 与名称，只放 `.local/`。
- 诊断日志（`tracing` 埋点）里玩家 ID 只放 `player`/`self_player` 字段、比赛 ID 只放 `match_id` 字段，不写名称、卡组名、URL 和游戏日志原文（ADR-0007）。测试代码、`fixtures/` 和文档里只用虚构的名字、ID、卡组名和比赛编号。
- 自动测试不访问网络、不读本机游戏日志；用真实数据的检查走 `bax` 命令行，结论写进 `docs/validation/`。
- 文档正文用中文，文件名用小写英文加连字符。改变模块边界或技术选型时先写 ADR。
- 提交信息用 Conventional Commits 格式，摘要用中文。
- 移植原型代码时在文件头注明来源，并在 `NOTICE.md` 登记。
- 本机工具链是 Rust 1.88：`rusqlite` 固定在 0.40，`tauri` 与 `@tauri-apps/*` 停在 2.11；全局的 `CC` 环境变量由 `.cargo/config.toml` 置空。

原型源码在 `C:\Projects\ba-elo-overlay`，只作参考，不要修改。
