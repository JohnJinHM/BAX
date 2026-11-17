# ADR-0006：前端采用 Vue 3＋TypeScript＋Vite，通信类型由 ts-rs 生成

- 状态：提议（验证 5 的资源数据出来后确认）
- 日期：2026-11-17
- 相关：[ADR-0001](0001-desktop-stack.md)、[ADR-0004](0004-state-ownership.md)、[总体架构·前后端通信](../architecture/overview.md#前后端通信)

## 背景

0.2 开始做悬浮窗和主窗口，需要定下前端框架。规划建议 Vue 3＋TypeScript＋Vite，图表建议 ECharts；ADR-0004 要求通信类型以 Rust 定义为准、TypeScript 类型由它生成，生成工具随前端框架一并选定。

两个窗口的需求差别很大：悬浮窗常驻、要小、每秒更新计时；主窗口按需打开，之后要承载档案、历史和图表。

## 决定

- 前端用 Vue 3（组合式 API、单文件组件）＋TypeScript＋Vite 7，两个窗口是 Vite 的两个入口页面（`index.html` 主窗口、`overlay.html` 悬浮窗），共用 `src/shared`。
- 不引入组件库和状态管理库。快照只有一份，用 `shallowRef` 保存即可；样式用 CSS 变量，之后的皮肤功能只换变量。
- 类型生成用 ts-rs：需要给界面用的 Rust 类型在 `ts` 特性下派生 `TS`，`npm run bindings` 把它们写到 `src/shared/bindings`。64 位整数按 `number` 生成（ID 与毫秒时间都在安全整数范围内）。生成的文件入库，不手改。
- 外壳自己的状态（悬浮窗是否显示、是否锁定、快捷键错误）字段很少，前端手写对应的类型。
- 本机工具链是 Rust 1.88，`tauri` 停在 2.11（2.12 起要求 Rust 1.90），前端的 `@tauri-apps/api` 与 `@tauri-apps/cli` 也停在 2.11，Tauri 要求两边的次版本一致。
- TypeScript 停在 5.9：7.0 换成了原生编译器，`vue-tsc` 对它的支持还没有经过验证。

## 后果

- 悬浮窗页面的脚本约 19 KB（gzip 后约 6 KB），主窗口约 14 KB，共用的 Vue 运行时约 75 KB（gzip 后约 30 KB）。
- Rust 一侧改了快照的类型，要重新运行 `npm run bindings`，`vue-tsc` 会指出前端没跟上的地方。
- 图表库（规划建议 ECharts）到 0.4 做战后图表时再引入，只在主窗口加载。
- 升级 Rust 工具链后可以一起升级 Tauri 与 rusqlite，届时修订本 ADR。

## 备选方案

- **React。** 生态更大，但这里没有用得上它的地方；运行时更大，悬浮窗希望尽量轻。
- **不用框架，直接写 TypeScript。** 悬浮窗足够简单，但主窗口之后的档案、历史与图表界面会变复杂，两个窗口用两套写法不划算。
- **tauri-specta 同时生成命令与类型。** 能省掉手写的命令包装，但它对 Tauri 2.11 的支持情况需要另外验证；命令只有十来个，手写包装的成本很低。
