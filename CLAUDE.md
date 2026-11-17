# BAX

《Broken Arrow／断箭》对局辅助工具，Tauri＋Rust 桌面应用。当前处于可行性验证阶段，仓库里只有文档。

## 先读这些

- `docs/README.md`：文档地图与写作约定
- `docs/planning/project-plan.md`：范围、数据来源、推进顺序
- `docs/architecture/overview.md`：模块边界、数据流、仓库结构
- `CONTRIBUTING.md`：分支、提交信息、版本

## 必须遵守

- 不实现作弊、小号或异常行为的判定，也不移植原型 `elo_cheat_check.py` 的检测规则（ADR-0002）。
- 依赖方向：`bax-core` 不做 IO、不读时钟；`bax-gamelog`、`bax-batrace`、`bax-store` 只依赖 `bax-core`；只有 `src-tauri` 依赖全部。
- 业务状态只在 Rust 一侧维护，前端不保存权威副本。
- 长期库没有自动过期或批量删除；清理只针对缓存库。
- 原始日志和接口响应含他人玩家 ID 与名称，只放 `.local/`；入库的样本先脱敏，放 `fixtures/`。
- 文档正文用中文，文件名用小写英文加连字符。改变模块边界或技术选型时先写 ADR。
- 提交信息用 Conventional Commits 格式，摘要用中文。
- 移植原型代码时在文件头注明来源，并在 `NOTICE.md` 登记。

原型源码在 `C:\Projects\ba-elo-overlay`，只作参考，不要修改。
