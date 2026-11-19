# ADR-0011：同一主线构建 Full 与 Lite

状态：已采纳 · 日期：2026-11-19

## 背景

两个版本共享日志、玩家档案、战报和界面，只在赛前 OCR 能力上不同。长期维护 OCR 分支会重复同步通用修复。

## 决策

`main` 是唯一长期分支，同一提交通过 `scripts/build-installer.ps1 -Edition Full|Lite` 构建两个版本。取代 ADR-0010 的独立分支约定，保留原生工作进程与离线模型方案。

Cargo 默认不启用 OCR；桌面 `ocr` feature 显式传递给引擎，只有 Full 构建启用。Lite 不编译工作进程调度、不启动 OCR、不打包运行库和模型。快照协议保持一致，前端通过后端 AppStatus.ocrSupported 隐藏 OCR 设置。

两版沿用 BAX 的应用标识、产品名和数据目录，以覆盖安装切换；不作为可并行运行的两个产品。安装包文件名和构建输出目录区分 Full/Lite。Lite 即使读到原有启用 OCR 的设置也不能启动识别。旧安装遗留的 OCR 文件不代表 Lite 支持 OCR。

## 验证

分别执行默认特性与 OCR 特性的 Rust 测试、Clippy；检查前端构建；生成两版安装包，检查 Lite 的资源列表没有 OCR。保留原生 OCR 自检与现有游戏实测限制。
