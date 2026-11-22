# 应用图标

`source.svg` 为定稿源文件：X 使用精确直线路径，同向边缘平行；背景是嵌入的 PNG。`icon.png` 为对应的 1254×1254 RGBA 主图，切口垂直宽度约 36.23 像素。

安装 Pillow 后，在仓库根目录运行 `python scripts/generate-app-icons.py`，从主图生成各尺寸 PNG 与多尺寸 Windows ICO。修改 SVG 后，应先将其导出为同尺寸 `icon.png` 再运行脚本。

Tauri 的 `bundle.icon` 提供应用和窗口图标；`src-tauri/src/tray.rs` 通过 `default_window_icon()` 复用同一图标。NSIS 的 `installerIcon` 和 `uninstallerIcon` 显式引用 `icon.ico`。Full/Lite 共用此配置。

## 验证（2026-11-21）

- 所有 PNG 均为 RGBA；ICO 包含 16、20、24、32、40、48、64、128、256 像素九个尺寸，已查看小尺寸预览。
- `pwsh -NoProfile -File scripts/build.ps1 -Mode Installer -Edition Lite -SkipNpmInstall` 通过，包含前端类型检查、生产构建、Rust 编译和 NSIS 打包。
- 使用 Windows `ExtractAssociatedIcon` 从应用 EXE、安装包 EXE 提取图标，确认均为新版 X；生成的 NSIS 脚本中 `MUI_ICON` 与 `MUI_UNICON` 均引用新版 ICO。
- 已核对 Windows 默认窗口图标从 ICO 加载、托盘复用该图标的代码路径；未启动应用做托盘实测，未执行安装/卸载，也未重建 Full。
