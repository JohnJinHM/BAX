# ADR-0012：GitHub Release 签名更新

状态：已采纳 · 日期：2026-11-19

桌面外壳采用 Tauri updater，GitHub Release 保存安装包、签名和 `latest.json`。Rust 持有检查结果、下载进度和待安装对象，前端只订阅状态及发送命令。签名公钥随应用发布，私钥保存在仓库外和发布环境的 Secret 中。

Full / Lite 使用独立 target，避免自动切换版本。只有 Installer 构建启用应用内安装；Dev / Portable 可检查版本，在发布页手动取对应产物。启动自动检查一次，设置页可重试；下载及退出安装由用户点击触发，准备阶段和对局中拒绝安装，下载结束再检查一次。

发布工作流先运行测试，再构建两个版本，上传到草稿 Release；维护者验收后公开。没有可用发布源时显示未就绪，不报告“已经最新”。普通构建不要求签名；`-SignUpdate` 的安装包构建必须成功生成签名才可发布。

依据：[Tauri updater 官方文档](https://v2.tauri.app/plugin/updater/)。为兼容当前 Rust 1.88，使用 2.9.0 插件及系统 TLS。
