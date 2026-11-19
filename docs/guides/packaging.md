# Windows OCR 版本编译与打包

状态：生效 · 更新：2026-11-19

## 分支与同步

`main` 是标准版；`codex/ocr-native` 是独立 OCR 版本。后者已合入 `origin/main` 的 `33fc093`。后续同步在 OCR 分支工作区执行：

```powershell
git fetch origin
git merge origin/main
```

解决冲突后运行本页检查再提交。此约定不创建后台同步任务，也不自动推送远端。不要将整个 OCR 分支合回标准版；通用修复优先在 main 完成再合入。

## 一键构建

需要 Windows x64、Node.js 22、Rust 1.88、Visual Studio 2022 C++ 构建工具与 Windows SDK、CMake 3.20+、PowerShell 7。构建和用户运行均不需要 Python；只有可选图片回归脚本需要 Python/Pillow。首次构建需联网获取依赖和模型。

```powershell
pwsh -File scripts/build-installer.ps1
```

脚本下载并校验 Microsoft.ML.OnnxRuntime 1.22.0 NuGet SDK 与两个 ONNX 识别模型，编译 C++ 工作进程，执行原生单元测试与模型自检，打包应用本地 MSVC DLL，然后构建前端、Rust release 和 NSIS 安装器。无文字检测模型、Python、OpenCV、NumPy。

仅构建原生 OCR：

```powershell
pwsh -File scripts/build-ocr-native.ps1
# 已有缓存，禁止脚本下载：
pwsh -File scripts/build-ocr-native.ps1 -SkipDownload
```

应用代码修改后复用已有原生 OCR 产物：

```powershell
pwsh -File scripts/build-installer.ps1 -SkipOcrBuild
```

产物：

- `.local/ocr-dist/bax-ocr/`：原生 EXE、ONNX Runtime、MSVC DLL、两个模型、许可证及 manifest。
- `target/release/bundle/nsis/BAX OCR_0.2.0_x64-setup.exe`：安装器。`CARGO_TARGET_DIR` 可修改 target 位置。
- `target/release/bax-app.exe` 与旁边 `ocr-runtime/`：配套使用，不能单独复制主 EXE。

安装资源配置为 `src-tauri/tauri.ocr.conf.json`。普通 Tauri build 不带此配置时不打包 OCR。开发模式优先读取构建产物 `.local/ocr-dist/bax-ocr`，发布模式读取安装目录资源。

## 数据与支持范围

用户数据继续使用 `%LOCALAPPDATA%\BAX`，也支持 `BAX_DATA_DIR`。已有 v5 直接打开；v4 自动备份并升级。标准版与 OCR 版不应同时运行。

OCR 仅在前台为 Broken Arrow、日志显示赛前连接正常且正式名单未知时采集。布局是约 16:9 的双侧五行卡片；不覆盖其他等候大厅布局或超宽屏。最低接受 1280×720，低分辨率可能只识别部分卡片；不确定的名字或等级会被拒绝，日志 ID 到达后替换候选。字体中的缺字方框、同形异码字符无法仅凭截图确定账号。

OCR 无网络请求和模型下载路径。BATrace 查询仍需联网。安装器若发现 WebView2 缺失，会联网补装。

## 验证

```powershell
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
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
