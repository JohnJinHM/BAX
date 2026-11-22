# 0.2.3 发布验收

状态：生效 · 更新：2026-11-22

## 发布结果

维护者要求把外观个人调整合入 `main` 并发布新版，并确认取色器等外观调整在桌面应用中可用。代码快进合入 `main`，附注标签 `v0.2.3` 指向 `d33b20cd1aad5d6facfbe5c3c74d5a2960d978f3`。

[BAX v0.2.3](https://github.com/JohnJinHM/BAX/releases/tag/v0.2.3) 于 2026-11-22 03:53（America/Los_Angeles）公开并设为 Latest，非预发布。[GitHub Actions](https://github.com/JohnJinHM/BAX/actions/runs/45016288573) 的 Full、Lite 和草稿任务全部成功。发布后的文档提交不改变标签与分发文件。

本次包含外观设置里的个人颜色、主窗口字号与间距，比赛记录条目的单行等高修复，以及 README 致谢与来源引用的整理。继续分发 Full／Lite 签名安装包与便携 ZIP。

## 验证

- 外观个人调整的预览页截图与单元测试见[皮肤验证·个人调整](skins.md#个人调整)；维护者确认取色器等功能在桌面应用中可用。
- 本机与两个 Windows runner 均通过 248 项 Rust 测试、41 项前端测试、rustfmt、Clippy、Vue 类型检查和前端生产构建；Rust 检查启用 `ocr,installed-updater`。Cargo、Tauri、npm 及两份 lock 文件版本一致。准备期间游戏和日常使用的 BAX 都在运行，本机检查以低于正常的优先级运行。
- 下载全部八个草稿资产，核对六个 BAX 文件的 SHA-256、两版更新 target、固定标签下载地址及清单签名。使用客户端内置公钥在本机再次验证 Full／Lite 安装包签名，通过。
- 解包 Full 安装包和便携包：主程序版本均为 0.2.3，两个 OCR 模型哈希正确，原生自检返回 `ready: true`、协议版本 1；包内没有用户数据库，便携启动器使用旁边的 `data`。Lite 的构建和签名验证由 CI 完成，本机核对分发文件完整性与更新清单。
- 日常使用的 BAX 正在运行，桌面应用是单实例，再启动会把它的主窗口带到游戏前面，所以本轮没有在本机启动下载的 Release 程序。
- 公开后匿名查询 GitHub Latest 得到 v0.2.3，`draft` 和 `prerelease` 均为 false；更新入口的 `latest.json` 与已校验的草稿清单哈希一致；Full／Lite 安装包公开下载地址均返回 HTTP 200。

| 资产 | 字节数 | SHA-256 |
|---|---:|---|
| Full 安装包 | 42,580,197 | `26715a10ba800816ae9a603348d7fe9cda042737c882afeb2d98040778e216d0` |
| Full 便携 ZIP | 46,880,273 | `841030e5bc2c27c08bc369df8edf0812dec0f56e0c58d8dba892f76d7c6fed13` |
| Lite 安装包 | 13,147,974 | `e06a57c1dd6366cff0bbfd074ce057597c4f0652b0c10b37bb745130039b49a7` |
| Lite 便携 ZIP | 15,443,266 | `9a26050142b4e86d3eb1f9be5f8992c58f502f8d0996f02f9efad4093f3b3857` |

本机过程日志、下载资产和校验结果保存在 `.local/release-0.2.3/`。

## 未覆盖的范围

本轮没有在本机启动下载的 Release 程序，也没有重做覆盖安装升级、重启及用户数据保留流程；PvE 战报的实时可用性、更多游戏布局及长时间接口稳定性仍未确认。公开 Release 说明已列明这些范围。
