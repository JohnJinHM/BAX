# Full / Lite 构建验证

状态：通过（构建与资源检查） · 更新：2026-11-19

## 范围

以原生 OCR 分支 ca087cb 为基础，整合 Full / Lite 构建选项。测试使用虚构数据；没有启动游戏或更改现有安装与用户数据库。

## 结果

- 默认特性的 workspace 测试：207 项通过（包括文档测试）；OCR 特性：215 项通过。
- 两种特性的 `cargo clippy --workspace --all-targets -- -D warnings` 均通过，`cargo fmt --all -- --check` 通过。
- `npm run build`（含类型检查）与 16 项 `npm run test:ui` 通过。
- 新增回归测试确认旧的启用 OCR 设置不能突破构建能力开关；禁用设置在两版都生效。
- 原生 OCR `--unit-test`、模型 `--self-test` 通过，两个模型 SHA-256 与仓库清单一致。Full 复用 ca087cb 对应的原生产物，本次没有改动 C++ 实现。
- 实际运行 `build-installer.ps1 -Edition Lite` 与 `-Edition Full -SkipOcrBuild`，均生成 NSIS 安装包。
- 检查生成的 NSIS 安装资源清单：Lite 不包含 OCR 工作进程、ONNX Runtime 和模型，release 目录也无 ocr-runtime；Full 包含工作进程、运行库与两个模型。

| 安装包 | 字节数 |
|---|---:|
| BAX-Lite_0.2.0_x64-setup.exe | 12021744 |
| BAX-Full_0.2.0_x64-setup.exe | 41336705 |

构建日志在本机 `.local/edition-*.log`，安装包与 SHA-256 在 `.local/releases/0.2.0/`，不入库。

## 未覆盖

本次未执行 Full/Lite 互相覆盖安装或重新进行游戏实机识别。两版保留相同产品名、应用标识和数据目录；旧安装遗留的 OCR 文件不会被 Lite 启动。识别的语言、布局与性能限制沿用[原生 OCR 验证](native-ocr.md)及[实机修复记录](native-ocr-live-fixes.md)。
