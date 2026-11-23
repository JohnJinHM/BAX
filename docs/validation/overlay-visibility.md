# 悬浮窗切窗恢复检查与修复

状态：生效 · 更新：2026-11-22

## 结论

维护者反馈：悬浮窗偶尔消失或被遮挡，但设置仍显示为开启；通常与切回游戏、Alt+Tab 或切换全屏有关。

修复前确认两处实现缺口：显示状态只记录指令，没有核对操作结果或系统状态；置顶只在创建、主动显示时设置，没有在游戏重新进入前台或改变显示模式后检查、恢复窗口层级。随后维护者确认切换窗口会触发问题，授权修复。

现已增加前台切换后的两次延迟置顶，以及隐藏、最小化和置顶标志丢失时的恢复；显示意图与系统观测状态分开维护。原生窗口回归通过。维护者于 2026-11-22 试用修复版后确认验收通过，并要求发布 Release。

## 修复前的本机检查

- 检查日期为 2026-11-22，时间使用 America/Los_Angeles。
- 首次检查时正在运行的是 0.2.3，诊断日志的构建提交为 `d33b20cd1a`；当时工作区的 `overlay.rs`、`win32.rs` 与该提交一致。
- 环境为 Windows 11 25H2（26200.9974）、WebView2 157.0.4341.60。游戏与 BAX 均保持原进程运行。
- 只读检查窗口句柄、原生可见状态、最小化、DWM cloaking、扩展样式、矩形和 Z 顺序；没有操作游戏、切换焦点、修改显示模式或调用显示/隐藏 API。
- 16:20:16 起每约 250ms 读取一次，共运行 50 秒，仅保存状态变化的 12 份样本。采样捕获到前台从桌面程序切换至 `BrokenArrow.exe`。
- 所有保存样本中，悬浮窗均为可见、未最小化、未 cloaked，`WS_EX_TOPMOST` 保持开启，矩形位于游戏画面范围内；没有发现与悬浮窗矩形相交且排在其上方的可见、未最小化、未 cloaked 顶层窗口。游戏在这些样本中没有 `WS_EX_TOPMOST`。
- 本次运行日志记录启动显示，以及 16:15:34–35、16:20:00–01 两组由界面发起的隐藏/显示；检查时未发现 WARN、ERROR 或退出记录。现有日志不会记录外部层级变化，因此不能据此排除被遮挡。
- 用户配置为未锁定、开启贴边、不透明度 1、窗口缩放 0.8。采样中有内容引起的高度变化，没有出现零尺寸或隐藏状态。

原始窗口采样与只读脚本位于被 Git 忽略的 `.local/overlay-diagnosis/`。窗口采样不保存其他应用的标题或玩家信息。采样只能证明这些时刻的窗口管理状态，不能证明游戏画面的最终合成结果，也不能排除采样间隔内的瞬态变化。

## 修复前的代码依据

### 设置显示的是请求状态

[`overlay::set_visible`](../../src-tauri/src/overlay.rs) 调用原生显示/隐藏后，直接把传入的 `visible` 写入 `AppStatus.overlay_visible`。取 HWND 失败时也会继续更新状态；[`win32::show_without_activating`](../../src-tauri/src/win32.rs) 没有向调用者返回置顶操作是否成功。

[`SettingsPanel.vue`](../../src/main/SettingsPanel.vue) 完全依据这个布尔值决定按钮显示“隐藏悬浮窗”还是“显示悬浮窗”。窗口事件处理只保存移动位置，没有同步外部隐藏、最小化或销毁状态。即使窗口真的被系统隐藏，界面也可能继续认为它已显示。

此外，被其他窗口盖住与被隐藏不同：即使加入 `IsWindowVisible`，窗口完全被遮挡时它仍可能返回真，不能将该值直接视为“用户看得到”。参见 [Microsoft IsWindowVisible 文档](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-iswindowvisible)。

### 没有恢复窗口层级的路径

创建窗口时设置 `always_on_top(true)`；主动显示时调用 `ShowWindow(SW_SHOWNOACTIVATE)`，再调用 `SetWindowPos(HWND_TOPMOST, …, SWP_NOACTIVATE)`。除此之外没有前台切换监听或置顶校验。调整窗口大小和鼠标穿透使用 `SWP_NOZORDER`，会保留现有层级。

`HWND_TOPMOST` 保证窗口位于非置顶窗口之上，不保证永远位于其他置顶窗口之前。`SWP_NOZORDER` 则明确保留当前 Z 顺序。参见 [Microsoft SetWindowPos 文档](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwindowpos)。

提醒路径也只在 `overlay_visible == false` 时调用显示。如果应用状态仍为真，即使发生隐藏或层级异常，提醒也不会重新显示、提升悬浮窗。

### 原生操作与框架缓存存在分离

窗口以 `visible(false)` 创建，之后由 BAX 直接调用 Win32 显示，tao 的内部可见标志没有随之变为真。此问题已在 `win32.rs` 模块注释中记录，当前鼠标穿透实现使用原生 API 来规避它。

本机依赖 tao 0.35.3 的 `WindowFlags::apply_diff` 在样式实际变化且内部可见标志为假时会调用 `SW_HIDE`；相同标志则直接返回。因此，不能把反复调用 Tauri 的 `set_always_on_top(true)` 当成可靠的恢复手段，也不应通过切换其布尔值来强制更新。没有发现这条路径在本次运行中被触发，不把它列为已证实的消失原因。

## 尚待区分的触发原因

1. 切回游戏或切换显示模式后，其他置顶窗口排到悬浮窗上方，或悬浮窗的置顶状态被外部改变。旧版本缺少检测和恢复，本次采样没有捕获到该情形；已在私有桌面的原生回归中构造并验证恢复。
2. 游戏进入独占全屏或切换显示合成路径，普通桌面窗口的置顶状态不能代表最终画面是否包含悬浮窗。没有验证故障时的实际呈现模式。Microsoft 说明独占全屏与全屏优化下的叠加机制不同，参见 [Demystifying Fullscreen Optimizations](https://devblogs.microsoft.com/directx/demystifying-full-screen-optimizations/)。

## 已实现的恢复

- Windows 每约 250ms 读取前台句柄、前台矩形和悬浮窗状态。前台句柄或矩形改变后，在检测后的约 250ms、1s 各重新置顶一次；同一窗口的尺寸变化也会重新安排恢复。稳定状态下不会持续写 Z 顺序。
- 重新置顶直接使用原生 `SetWindowPos(HWND_TOPMOST, …, SWP_NOACTIVATE | SWP_NOOWNERZORDER)`，保留位置大小，避开 tao 的可见状态缓存。已经显示时不再重复调用 `ShowWindow`。
- 如果系统隐藏、最小化窗口或清除置顶标志，按用户显示意图恢复；持续失败时每两秒最多重试一次并记录告警。恢复成功记录 info，正常切窗置顶记录 debug。
- 用户主动隐藏或以 `--hidden` 启动时停止自动恢复。排队的显示操作执行前核对最新意图；定时检查也在执行时读取意图，不携带过期的“显示”值。
- 原生操作统一调度至窗口所属主线程；定时任务等上一轮完成后才排下一轮，避免界面忙时堆积任务。
- `overlay_visible` 改为系统的“可见且未最小化”状态。置顶操作返回结果并核验实际标志，失败记录日志；这个布尔值仍不代表未遮挡或实际全屏合成结果。
- 提醒开启“显示悬浮窗”时会重新显示并置顶，不再因旧布尔值为真而跳过。

## 验证

- `cargo test -p bax-app`：15 项通过，包括 6 项恢复时序测试与 1 项原生窗口回归。
- `cargo test -p bax-app --features ocr`：同样 15 项通过，覆盖 Full 特性构建。
- 恢复时序覆盖：置顶标志未丢失时的两次重排、正常后停止写入、主动隐藏取消恢复、快速切换替换旧计时、失败节流、前台短暂为空、同一句柄的全屏尺寸变化。
- 原生回归在新线程绑定的私有 Windows 桌面创建窗口，从不调用 `SwitchDesktop`，不会出现在用户的游戏画面。覆盖两个置顶窗口互相遮挡、置顶标志被清除、隐藏、最小化、空句柄；验证恢复后前台/活动窗口、位置大小和锁定穿透标志保持不变。
- `cargo clippy --workspace --all-targets --features bax-app/ocr -- -D warnings`：通过。
- `pwsh -NoProfile -File scripts/build.ps1 -Mode Portable -Edition Full -SkipOcrBuild -SkipNpmInstall`：通过，包含前端类型检查和生产构建、原生 OCR 资源校验及自检。修复版程序为 `target/release/bax-app.exe`，便携包为 `target/release/bundle/portable/BAX-Full_0.2.3_x64-portable.zip`；本地候选版本号仍为 0.2.3，没有发布。
- ZIP 内程序与输出程序的 SHA-256 一致（`d5c94df1195ad16e60f8fb2d4a13bcb3ea03bb03eccc6701a17658eb61b31125`），包含 OCR 资源，不含用户数据。

维护者已确认本次切换窗口后消失的问题修复。验收反馈没有逐项注明窗口化、无边框全屏和实际独占全屏的覆盖情况，因此不据此认定全部显示模式兼容性通过。独占全屏的最终合成不由桌面置顶标志保证，Overlay 实机兼容的整体验收仍保留原状态。

本次修复已随 [v0.2.4](release-0.2.4.md) 发布，双版本分发包及公开更新入口均通过校验。
