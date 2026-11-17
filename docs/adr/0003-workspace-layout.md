# ADR-0003：用 Cargo workspace 分离核心库与 Tauri 外壳

- 状态：提议
- 日期：2026-11-16
- 相关：[总体架构·分层与模块](../architecture/overview.md#分层与模块)

## 背景

原型的主程序集中在 `src/ba_elo_overlay.py` 一个文件里（约 15 万字节），`Overlay` 类同时调度界面、查询和检测。规划要求把窗口展示与业务状态分开，并用真实日志回放来测试重连、空名字、文件切换等情况。可行性验证阶段还需要一些独立的小程序，例如日志回放和接口探测，它们不应该依赖窗口。

## 决定

仓库根目录是一个 Cargo workspace，包含四个库和一个应用：

| 成员 | 内容 | 允许的 workspace 内依赖 |
|---|---|---|
| `crates/bax-core` | 领域模型、对局生命周期、计时、统计 | 无 |
| `crates/bax-gamelog` | 日志发现、增量读取、解析 | `bax-core` |
| `crates/bax-batrace` | BATrace 客户端 | `bax-core` |
| `crates/bax-store` | SQLite 存储 | `bax-core` |
| `src-tauri`（`bax-app`） | Tauri 外壳与后台任务编排 | 全部 |

`bax-core` 不做 IO，也不读系统时钟。四个库都不依赖 Tauri。`tools/` 下的验证程序可以作为 workspace 成员，依赖需要的库。

## 后果

- 依赖方向由编译器保证。解析和统计代码写不出对窗口或网络的调用。
- `bax-core` 的测试不需要编译 HTTP、SQLite 和 Tauri，回放测试运行得快。
- 验证阶段写的解析和请求代码直接进入库，正式开发时不用重写。
- 跨库移动类型比在单个库里麻烦。边界划错时，合并两个库的代价不高，可以接受。

## 备选方案

- **全部写在 `src-tauri` 里，用模块划分。** 起步最省事，但模块之间的依赖方向只能靠自觉，测试和验证工具都要带上 Tauri 一起编译。
- **一个 `bax-core` 库加 `src-tauri`。** 能脱离 Tauri 测试，但纯逻辑和网络、数据库仍在同一个编译单元里，纯逻辑部分的隔离得不到保证。库的数量增长到难以管理时，可以退回这个方案。
