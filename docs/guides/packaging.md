# Windows 安装包构建

状态：生效 · 更新：2026-11-19

## 构建环境

Windows x64；Python 3.13 x64、Node.js 22、Rust 1.88、Visual Studio C++ 构建工具和 Windows SDK。首次构建需要联网下载 npm/Python/Rust 依赖、模型和 NSIS 工具。用户安装后不需要 Python。

在仓库根目录运行：

```powershell
pwsh -File scripts/build-installer.ps1
```

脚本创建 `.local/ocr-build-env`，安装 `ocr/requirements-build.txt` 中的依赖，校验 `ocr/models.json` 的模型哈希，生成并离线自检 OCR 运行时，随后执行 npm ci、前端生产构建、Rust release 构建和 NSIS 打包。

只修改 Rust/前端、OCR 代码和依赖未变时，可复用已生成的运行时：

```powershell
pwsh -File scripts/build-installer.ps1 -SkipOcrBuild
```

输出：

- `.local/ocr-dist/bax-ocr/`：OCR EXE、Python 与原生库、三个模型、licenses 和 manifest.json。
- `target/release/bundle/nsis/BAX_0.2.0_x64-setup.exe`：一键安装程序。设置 `CARGO_TARGET_DIR` 后，`target` 替换为指定目录。
- `target/release/bax-app.exe` 与旁边的 `ocr-runtime/`：应用与配套运行时；不能只复制主 EXE。

安装包使用 `src-tauri/tauri.ocr.conf.json` 资源配置。单独运行普通 `cargo build` 或不带资源配置的 Tauri build 不会生成完整的一键安装包。标准入口应使用上面的 PowerShell 脚本。

## 安装与数据

安装包把 OCR 和模型放在应用安装目录，工作数据继续使用 `%LOCALAPPDATA%\BAX`（支持 `BAX_DATA_DIR`）。更新应用不覆盖玩家数据库。首次安装若系统缺少 WebView2，安装器会联网下载它；OCR 本身完全离线，BATrace 资料查询仍需联网。

支持 v5 数据库。v4 会先备份为 `bax.db.v4.bak` 再新增 v5 视图；已有 v5 不重建、不降级。更高版本仍明确拒绝，避免错误打开未知结构。

## 验证

```powershell
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
& .local/ocr-dist/bax-ocr/bax-ocr.exe .local/ocr-dist/bax-ocr/models --self-test
```

自检输出 `{"ready": true}`，并在进程内部禁止网络连接。可用 `--image 图片路径` 对离线截图验证完整模型推理；输出含玩家昵称，真实样本仅保存在 `.local/`。
