# Dev / Portable / Installer 构建验证

状态：通过（构建与打包检查） · 更新：2026-11-19

## 范围与方法

验证 `scripts/build.ps1` 的 Dev、Portable、Installer 与 Full、Lite 六种组合。构建只生成产物，不启动 BAX 或安装到现有系统；原生 OCR 单独重建并运行其单元测试与模型自检。

六种组合均实际构建通过，包含前端类型检查和 Vite 构建。测试复用本机 Node 依赖（`-SkipNpmInstall`），Full 应用构建复用刚生成的原生 OCR（`-SkipOcrBuild`）。首次 `npm ci` 因现有开发进程占用依赖而失败，依赖已恢复；新增复用选项供这种开发场景使用。

## 结果

- Dev：两版生成内置前端的 Debug EXE，Full 旁边有完整 `ocr-runtime`，Lite 无此目录。
- Portable：两版生成可读取、CRC 校验通过的 ZIP；包含启动器、使用说明、项目许可证。Full 包含 OCR 模型且 SHA-256 与清单一致，Lite 无 OCR。均未打入数据库、用户 data 目录或 PDB。
- Installer：两版生成带 Full/Lite 文件名的 NSIS 安装器。
- 启动器：从不同工作目录、中文与空格路径启动测试探针，正确设置同目录的绝对 data 路径，保留传入参数，不修改父进程环境。探针代替 BAX，避免读入本机游戏日志或改动用户数据库。
- 参数与恢复：Lite 搭配 SkipOcrBuild 会被拒绝；成功与失败后 CARGO_TARGET_DIR、工作目录恢复。便携打包暂存目录清理完成。
- PowerShell 解析与 `git diff --check` 通过。

本地日志：`.local/build-modes-*.log`；ZIP 清单检查：`.local/check-build-modes.py`；汇总：`.local/build-modes-results.json`。测试输出根目录通过 CARGO_TARGET_DIR 指向已有构建缓存，不改变默认输出约定。

## 未覆盖

本次未执行实际覆盖安装，也未重新进行游戏实机识别或在没有 WebView2 的电脑上测试。Portable 不自动安装 WebView2，数据目录启动器只管理 BAX 自己的数据。使用方式见[打包指南](../guides/packaging.md)。
