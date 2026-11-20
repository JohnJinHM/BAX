# 0.2.1 发布验收

状态：生效 · 更新：2026-11-20

## 发布结果

维护者同意发布，并确认正式发布保留 Full／Lite 两版、本机验收只做 Full。代码快进合入 `main`，附注标签 `v0.2.1` 指向 `243d91361c8801ca92b2573d085322a40152a23c`。

[BAX v0.2.1](https://github.com/JohnJinHM/BAX/releases/tag/v0.2.1) 于 2026-11-20 12:04（America/Los_Angeles）公开并设为 Latest，非预发布。[GitHub Actions](https://github.com/JohnJinHM/BAX/actions/runs/44805441547) 的 Full、Lite 和草稿任务全部成功。发布后的文档提交不改变该标签与二进制。

## 验证

- 本机 Full 通过 232 项 Rust 测试（含文档测试）、30 项前端测试、rustfmt、Clippy 和前端生产构建；Rust 检查启用 `ocr,installed-updater`。
- 最终界面中，兵种构成的统计选择恢复到标题、全选／清空所在行。浏览器无控制台错误，其余交互与原始战报重算结果见[本轮功能验证](match-analysis-polish.md)。
- Full 便携构建输出在默认 `target/release/`；PE 版本为 0.2.1，ZIP 内主程序与输出目录主程序哈希一致。使用独立数据库副本、空日志目录完成隐藏启动，诊断日志无错误。
- 两个 Windows runner 各通过 232 项 Rust 测试、30 项前端测试、rustfmt、Clippy 和前端生产构建；均完成签名安装包与便携包，用客户端内置公钥验证签名后创建草稿。
- 下载草稿的全部八个资产，核对六个 BAX 文件的 SHA-256，以及两版更新 target、固定标签下载地址和清单中的签名。本机再次验证 Full 安装包签名，通过。
- 解包 Full 安装包与便携包，PE 版本均为 0.2.1，两个 OCR 模型哈希正确，原生自检返回 `ready: true`、协议版本 1；均未携带用户数据库，便携启动器使用旁边的 `data` 目录。Lite 的构建与签名验证由 CI 完成，本机只核对分发文件完整性与更新清单。
- 公开后匿名读取 `/releases/latest/download/latest.json`，与验收的草稿清单哈希一致。Full／Lite 安装包公开下载地址均返回 HTTP 200；GitHub latest API 指向 v0.2.1，`draft`、`prerelease` 均为 false。

| 资产 | 字节数 | SHA-256 |
|---|---:|---|
| Full 安装包 | 41,972,441 | `2eadedc02fb6c91f2aca1013f0748f9e60549187ffb18769d1fc1bc2267703dd` |
| Full 便携 ZIP | 46,337,210 | `dccdfa963328671768edf87c0a3969647571bb652c6c8f759b81227053e70b0e` |
| Lite 安装包 | 12,623,116 | `a81d779b29523ac764a1fba9e1add34ee7fe2bd08cf84a09da01a865588543de` |
| Lite 便携 ZIP | 14,927,796 | `1ff4d291e367c6043a1d9d1acb8b641f477108eff1896d53aeadf4eefbe21a00` |

本机日志、界面截图、下载资产与校验结果在 `.local/release-0.2.1/`。

## 后续范围

跨版本检查、下载、退出、覆盖安装、重启与用户数据保留的完整流程，以及不同自定义／旁观者布局和 PvE 切换的实机场景，继续补充验证。公开 Release 说明已列明此范围。
