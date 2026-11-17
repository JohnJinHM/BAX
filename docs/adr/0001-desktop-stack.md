# ADR-0001：桌面技术栈暂定 Tauri＋Rust

- 状态：已采纳（暂定，验证 1 和验证 5 通过后转为正式）
- 日期：2026-11-16
- 相关：[项目规划·技术方案](../planning/project-plan.md#技术方案)、[可行性验证](../validation/README.md)

## 背景

原型用 Python、Tkinter、SQLite、urllib 和 threading 实现，通过 ctypes 调用 Win32 完成托盘、快捷键和点击穿透，用 PyInstaller 发布。BAX 在原型之上增加可换肤的界面、图表、长期档案和战后分析。规划阶段暂定改用 Tauri＋Rust，界面由网页前端实现：统一的样式变量可以支撑换肤，图表库可以支撑趋势图和单位时间线。

## 决定

- 桌面外壳采用 Tauri（当前为 2.x），负责窗口、托盘、快捷键和应用生命周期。
- 业务逻辑用 Rust 实现：日志解析、接口查询、状态管理、计时和统计。
- 原型的 Python 源码只作为移植参考。只有确认某个 Python 模块复杂到值得保留时，才评估以 sidecar 方式运行它。
- 前端建议 Vue 3＋TypeScript＋Vite，图表建议 ECharts，存储建议 SQLite。这三项尚未定案。

## 后果

- 运行依赖 WebView2。资源评估要统计完整进程树，包含 WebView2 子进程。
- Overlay 的穿透、置顶和不抢焦点依赖 Tauri 的窗口能力，不足的部分用 Win32 补充。
- 这个选择是否成立取决于两项验证：Overlay 在目标显示模式下的实机表现（验证 1），以及游戏运行时的资源占用（验证 5）。任一项不通过，就重新评估外壳技术，并用新的 ADR 取代本条。
