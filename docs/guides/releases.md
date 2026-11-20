# GitHub 发布与应用更新

状态：生效 · 更新：2026-11-19

## 使用者

启动 BAX 十秒后自动检查一次公开 Release，主窗口发现新版本时显示入口；设置页可手动重试、查看说明。Installer 版本点击“下载并安装”后下载并验证签名，随后退出 BAX、运行安装程序。等待室、加载、部署及对局期间暂缓安装；下载结束时再次检查比赛状态。Full 只更新 Full，Lite 只更新 Lite。

Dev / Portable 可检查，但通过“打开发布页”手动下载。便携更新保留旧 `data` 文件夹。安装更新继续使用原数据目录。没有公开 Release、网络失败、缺少更新清单或签名不符会显示状态，不会把它们当作“已经最新”。检查超时 20 秒，下载超时五分钟，失败可重试。应用不会自动下载或退出。尚不含 updater 的旧程序需先手动安装一次新版。

## 首次准备

当前尚无公开 Release。本次完成客户端、签名打包及发布工作流，保留 0.2.0 版本号；首次公开发布前完成[验证记录](../validation/player-specs-updater.md)中的实机项目。

公钥已写入 `src-tauri/tauri.conf.json`。对应私钥在维护者本机仓库外的 `$HOME/.tauri/bax-updater.key`，不要提交到 Git。备份该文件；后续发布必须使用同一私钥，否则已有安装无法验证更新。

在仓库 Settings → Secrets and variables → Actions 配置：

| Secret | 内容 |
|---|---|
| `TAURI_SIGNING_PRIVATE_KEY` | 私钥文件完整内容，不是本机路径 |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | 私钥密码；本次私钥无密码，可不设 |

当前连接的 GitHub 工具不支持设置 Actions Secrets；本次未写入远端 Secret、推送标签或创建 Release。

## 发布

1. 验收并合入 `main`。同步 Cargo workspace、Tauri 配置、package.json 和两份 lock 文件的版本号，整理 CHANGELOG。
2. 在该提交创建并推送附注标签 `vX.Y.Z`。工作流分别测试、构建 Full / Lite 签名安装包及便携 ZIP。
3. 工作流用客户端内置公钥验证安装包，再生成 `latest.json`、SHA256SUMS 和**草稿** Release。任何一版构建或验证失败，都不进入草稿阶段。
4. 从草稿下载验收两版，核对版本、OCR 资源和用户数据保留。公开 Release 并设为最新后，客户端才会从 `/releases/latest/download/latest.json` 发现更新。预发布不用于稳定更新通道。

工作流也可手动运行，但必须选择与配置版本匹配的 tag。草稿已存在时创建步骤会报错；重试前先在 GitHub 处理旧草稿。不要覆盖已公开资产，应发布新版本。

## 本机签名构建

```powershell
$env:TAURI_SIGNING_PRIVATE_KEY = Join-Path $HOME '.tauri/bax-updater.key'
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = ''
./scripts/build.ps1 -Mode Installer -Edition Full -SignUpdate -SkipOcrBuild -SkipNpmInstall
./scripts/build.ps1 -Mode Installer -Edition Lite -SignUpdate -SkipNpmInstall
```

普通构建不需要私钥。`-SignUpdate` 只接受 Installer，缺少密钥或 `.sig` 则失败。将两版安装包及同名 `.sig` 放入暂存目录后：

```powershell
cargo run -p bax-app --example verify-update -- <Full安装包> <Lite安装包>
node scripts/release-manifest.mjs <暂存目录> <X.Y.Z> [版本说明文本文件]
```

清单使用 `windows-x86_64-full` / `windows-x86_64-lite` 两个 target，内嵌签名内容，下载链接固定到对应 tag。依据 [Tauri updater](https://v2.tauri.app/plugin/updater/)，设计见 [ADR-0012](../adr/0012-signed-github-updates.md)。
