# Windows Full / Lite 编译与打包

状态：生效 · 更新：2026-11-19

## 版本与主线

`main` 是唯一长期分支，同一提交构建 Full（完整版，含 OCR）与 Lite（无 OCR），见 [ADR-0011](../adr/0011-full-lite-builds.md)。两版共用日志解析、战报、档案和数据结构。

## 统一构建入口

需要 Windows x64、PowerShell 7、Node.js 22、Rust 1.88、Visual Studio 2022 C++ 构建工具与 Windows SDK。Full 另外需要 CMake 3.20+；构建与运行均不需要 Python。首次构建需联网获取依赖，Full 还会下载并校验 ONNX Runtime SDK 与两个识别模型。

```powershell
pwsh -File scripts/build.ps1 -Mode Dev -Edition Lite
pwsh -File scripts/build.ps1 -Mode Dev -Edition Full
pwsh -File scripts/build.ps1 -Mode Portable -Edition Lite
pwsh -File scripts/build.ps1 -Mode Portable -Edition Full
pwsh -File scripts/build.ps1 -Mode Installer -Edition Lite
pwsh -File scripts/build.ps1 -Mode Installer -Edition Full
```

不传参数默认 `Dev / Full`。脚本只构建，不自动启动或安装应用。版本号读取 Tauri 配置，产物路径在成功后打印。

| 模式 | 用途 | 产物（以 Lite 为例） |
|---|---|---|
| Dev | 内置前端的 Debug 版，不依赖 Vite，便于本机调试 | `target/editions/lite/debug/bax-app.exe` |
| Portable | 优化的 Release 便携 ZIP | `target/editions/lite/release/bundle/portable/BAX-Lite_0.2.0_x64-portable.zip` |
| Installer | 优化的 Release NSIS 安装器 | `target/editions/lite/release/bundle/nsis/BAX-Lite_0.2.0_x64-setup.exe` |

Full 对应 `editions/full` 与 `BAX-Full`。Dev 与 Release 分开，Full 与 Lite 分开；Portable 和 Installer 共用各自版本的 Release 编译缓存。`CARGO_TARGET_DIR` 可指定输出根目录，脚本在其下追加 `editions/full` 或 `editions/lite`，结束或失败时恢复环境变量及工作目录。

Full 的 `bax-app.exe` 必须与旁边的 `ocr-runtime/` 一起复制，包含原生工作进程、运行库、两个模型和许可证。Lite 不读取或构建 OCR 产物。

Full 在所有模式下显式替换输出目录的 `ocr-runtime/`，逐文件校验 SHA-256，并从输出目录运行模型及协议自检。不能依赖 Tauri 的 `--no-bundle` 更新已有资源：旧目录可能残留 Python 工作进程。引擎要求原生工作进程协议版本 1，不兼容时会提示重新构建或安装 Full。

## 增量构建与旧入口

默认执行 `npm ci`，Full 默认重建原生 OCR。已有依赖、只改 Rust/Vue 时可以复用：

```powershell
pwsh -File scripts/build.ps1 -Mode Dev -Edition Full -SkipOcrBuild -SkipNpmInstall
pwsh -File scripts/build.ps1 -Mode Dev -Edition Lite -SkipNpmInstall
```

`-SkipOcrBuild` 仅适用于 Full，仍检查原生 manifest、必需运行库和许可证、模型 SHA-256，不允许复用旧 Python 产物。`-SkipNpmInstall` 适用于全部模式；调用方负责确保依赖与锁文件一致，也可避免开发服务器占用文件时 `npm ci` 无法清理依赖目录。

旧命令 `scripts/build-installer.ps1 -Edition Full|Lite` 保留，转交统一入口的 Installer 模式。两个 Skip 参数均支持。

只构建原生 OCR：

```powershell
pwsh -File scripts/build-ocr-native.ps1
pwsh -File scripts/build-ocr-native.ps1 -SkipDownload # 已有缓存，禁止下载
```

## Portable 使用方法

ZIP 内有一层版本目录，包含 `bax-app.exe`、`start.cmd`、使用说明和许可证；Full 另外含 `ocr-runtime/`。解压到可写目录后，通过 **start.cmd** 启动，BAX 的数据库、设置和诊断日志写入启动器旁边的 `data/`。支持从其他工作目录启动，也支持包含空格、中文的解压路径。

直接双击 EXE 仍使用 `BAX_DATA_DIR` 或默认 `%LOCALAPPDATA%\BAX`。ZIP 不包含已有用户数据；更新时退出 BAX，保留旧 `data/` 并复制到新版本目录。构建仅打包程序与资源，不扫描或覆盖用户解压后的目录。

目标电脑需要已安装 WebView2 Runtime，便携包不会自动补装；WebView2 自身缓存不保证随 `data/` 搬迁。两版、便携版与安装版共用应用身份，切换前退出已运行的 BAX。

## 热更新开发

Dev 构建是独立 Debug EXE；需要前端热更新时仍使用 Tauri 开发服务器：

```powershell
$env:CARGO_TARGET_DIR = "$PWD/target/editions/lite"
npx tauri dev

# Full 先准备原生 OCR，再启动：
$env:CARGO_TARGET_DIR = "$PWD/target/editions/full"
npx tauri dev --features ocr --config src-tauri/tauri.ocr.conf.json
```

普通 Tauri build 默认是 Lite。手动构建 Full 必须同时传 `--features ocr --config src-tauri/tauri.ocr.conf.json`。开发模式优先读取输出目录的资源，缺少时回退到源码目录 `.local/ocr-dist/bax-ocr`；Release 只读取 EXE 旁边的安装资源。

## 数据与支持范围

用户数据继续使用 `%LOCALAPPDATA%\BAX`，也支持 `BAX_DATA_DIR`。已有 v5 直接打开；v4 自动备份并升级。两版保持相同的产品名 BAX 和应用标识，覆盖安装切换，共用原有记录和设置，不支持并行运行。旧安装目录可能保留 OCR 文件，但 Lite 没有 OCR 工作进程代码，旧开关不能启用它；两版安装包本身的资源分别核查。

OCR 仅在前台为 Broken Arrow、日志显示赛前连接正常且正式名单未知时采集。布局是约 16:9 的双侧五行卡片；不覆盖其他等候大厅布局或超宽屏。最低接受 1280×720，低分辨率可能只识别部分卡片；不确定的名字或等级会被拒绝，日志 ID 到达后替换候选。字体中的缺字方框、同形异码字符无法仅凭截图确定账号。

OCR 无网络请求和模型下载路径。BATrace 查询仍需联网。安装器若发现 WebView2 缺失，会联网补装。

## 验证

```powershell
cargo test --workspace
cargo test --workspace --features bax-app/ocr
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --all-targets --features bax-app/ocr -- -D warnings
npm run test:ui
& .local/ocr-dist/bax-ocr/bax-ocr.exe --unit-test
& .local/ocr-dist/bax-ocr/bax-ocr.exe .local/ocr-dist/bax-ocr/models --self-test
```

图片回归是开发验证，不属于安装依赖。需要 Python、Pillow，以及 Windows 微软雅黑和 Arial 字体：

```powershell
python scripts/test-ocr-native.py
python scripts/test-ocr-native.py --screenshot 'C:/path/to/screenshot.png'
```

虚构四语卡片、空白、单侧、缺少黄色等级标签、不支持比例、模型缺失进行断言；可选真实截图测量四档分辨率，每档预热后 10 次采样。真实昵称和截图仅保存在 `.local/native-validation`。这不替代真实俄日文截图、多布局和游戏帧时间验收。
