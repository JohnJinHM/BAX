# 首个公开 Release：0.2.0

状态：生效 · 更新：2026-11-20

## 发布结果

维护者于 2026-11-19 确认已实机试用当前版本，要求标记验收并发布。代码快进合入 `main`，附注标签 `v0.2.0` 指向 `775314ae6fdc23970ef34bbdaf6945019c672d3e`；后续验收文档提交不改变发布二进制。

[BAX v0.2.0](https://github.com/JohnJinHM/BAX/releases/tag/v0.2.0) 于 2026-11-20 00:09（America/Los_Angeles）公开并设为 Latest，非预发布。[GitHub Actions](https://github.com/JohnJinHM/BAX/actions/runs/44698386896) 的 Full、Lite 和草稿任务全部成功。

## 验证

- 两个 Windows runner 各通过 228 项 Rust 测试（含文档测试）、26 项 UI / 发布清单测试，以及 rustfmt、Clippy 和前端生产构建。
- Full 与 Lite 均完成签名安装包、便携 ZIP 构建；工作流用客户端内置公钥验证实际安装包签名后才创建草稿。
- 从草稿下载全部八个资产，核对六个 BAX 文件的 SHA-256、两版更新 target、固定 tag 下载地址和清单中的签名。本机再次用内置公钥验证两份安装包，均通过。
- 解包四个程序包，PE 版本均为 0.2.0。Full 安装包与便携包中的两个模型哈希正确，原生 OCR 自检 `ready: true`、协议版本 1；Lite 的两种包均不含 OCR。
- 四个包均未携带用户数据库；两版便携启动器均使用旁边的 `data` 目录。本次未覆盖用户正在运行的安装，未将解包检查计为安装升级或数据库保留的端到端验收。
- 公开后匿名读取 `/releases/latest/download/latest.json`，内容与验收的草稿清单完全一致；Full / Lite 安装包的公开下载地址均返回 HTTP 200。GitHub latest API 指向 v0.2.0，`draft`、`prerelease` 均为 false。

| 资产 | 字节数 | SHA-256 |
|---|---:|---|
| Full 安装包 | 41,907,339 | `e6757f4a49a7689ed174488680b8f5cef76fe587210642e79b97e66f13f8dcc6` |
| Full 便携 ZIP | 46,327,971 | `cbaba24114205795c550f3e1cc1767f91d9e623b365910c6ee2fdf6ceb309c9a` |
| Lite 安装包 | 12,620,467 | `41fe1ec0b419f72cd64bec9df1db5ba153e961d62157a2d2a2c2cc990a837e4c` |
| Lite 便携 ZIP | 14,914,650 | `00502a47246424e440200459cbce404af52a7c6334b28839e22294c06f344859` |

本机下载和检查结果在 `.local/release-0.2.0-download/`，工作流日志在 `.local/release-tools/actions-44698386896.log`。签名私钥保持在仓库外，Actions Secret 已配置。

## 后续范围

跨公开版本的检查、下载、退出、覆盖安装、重启及用户数据保留仍需在后续版本做完整端到端验证。更多分辨率、显示缩放和游戏场景继续扩展；本次维护者实机验收不代表这些组合全部覆盖。上述限制已写入公开 Release 说明。
