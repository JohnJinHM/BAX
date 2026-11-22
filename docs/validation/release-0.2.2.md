# 0.2.2 发布验收

状态：生效 · 更新：2026-11-21

## 发布结果

维护者确认独立 Full Debug 版测试没有问题，并授权补充旁观标签、整理数据源文档和发布新版。代码快进合入 `main`，附注标签 `v0.2.2` 指向 `40236b0b22e819ad955eaca89fba67dd4768951d`。

[BAX v0.2.2](https://github.com/JohnJinHM/BAX/releases/tag/v0.2.2) 于 2026-11-21 22:44（America/Los_Angeles）公开并设为 Latest，非预发布。[GitHub Actions](https://github.com/JohnJinHM/BAX/actions/runs/44993700498) 的 Full、Lite 和草稿任务全部成功。发布后的文档提交不改变标签与分发文件。

本次包含游戏数据源优先、BATrace 补充／回退与状态查询、旁观记录标签，以及主线已有的皮肤、边缘吸附和图标更新。继续分发 Full／Lite 签名安装包与便携 ZIP。

## 验证

- 灰色“旁”标签按本机 Spectators 队伍显示；默认和断箭皮肤的浏览器预览通过，保留胜、负、弃的区分，无控制台错误。
- 主源字段转换、缓存、回退及独立 Debug 启动结果见[数据源兼容性记录](upstream-api-compatibility.md)。维护者确认 Debug 版测试没有问题。
- 本机与两个 Windows runner 均通过 248 项 Rust 测试、34 项前端测试、rustfmt、Clippy、Vue 类型检查和前端生产构建；Rust 检查启用 `ocr,installed-updater`。Cargo、Tauri、npm 及两份 lock 文件版本一致。
- 下载全部八个草稿资产，核对六个 BAX 文件的 SHA-256、两版更新 target、固定标签下载地址及清单签名。使用客户端内置公钥在本机再次验证 Full／Lite 安装包签名，通过。
- 解包 Full 安装包和便携包：主程序版本均为 0.2.2，两个 OCR 模型哈希正确，原生自检返回 `ready: true`、协议版本 1；包内没有用户数据库，便携启动器使用旁边的 `data`。Lite 的构建和签名验证由 CI 完成，本机核对分发文件完整性与更新清单。
- 下载的 Full 便携 Release 程序使用独立 v5 测试库、空日志目录和 `--hidden` 启动，进程运行正常且诊断日志无错误；测试进程已退出，未修改原有用户数据库。
- 公开后匿名查询 GitHub Latest 得到 v0.2.2，`draft` 和 `prerelease` 均为 false；更新入口的 `latest.json` 与已校验的草稿清单哈希一致；Full／Lite 安装包公开下载地址均返回 HTTP 200。
- 本次修改的 Markdown 相对链接检查通过。

| 资产 | 字节数 | SHA-256 |
|---|---:|---|
| Full 安装包 | 42,588,334 | `284fee203caa9e8c92304b07f969295a866e3d27f0f4a034ac067fc364422c09` |
| Full 便携 ZIP | 46,870,204 | `b46eb9e5b8e2788272bf8d4d5a49b2f86d72f6c62f089b085afb2133c95a0500` |
| Lite 安装包 | 13,143,729 | `505e434ba5d75b6db1f499567da0a9439f649d69b7ea2f1547b16bc0e3c22b17` |
| Lite 便携 ZIP | 15,446,763 | `9d62e968478f6bcac8edf700fd373aa781e1336c8a371915c05ecfaf2af7be5b` |

本机过程日志、下载资产和校验结果保存在 `.local/release-0.2.2/`。

## 未覆盖的范围

本轮没有完整重做覆盖安装升级、重启及用户数据保留流程；PvE 战报的实时可用性、更多游戏布局及长时间接口稳定性仍未确认。BATrace 故障期间昵称搜索、完整统计与单位目录可能缺失，已有本地记录可继续查看。公开 Release 说明已列明这些范围。
