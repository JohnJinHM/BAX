# 来源与许可声明

## BA-ELO-Overlay

BAX 以 BA-ELO-Overlay 原型为基础重新实现，已取得原作者同意。原型采用 MIT 许可，版权声明为 `Copyright (c) 2026 BA-ELO-Overlay contributors`。

BAX 自身采用 MIT 许可（见 [LICENSE](LICENSE)）。BAX 中移植自原型或对其进行实质性改写的代码，同时受下面的许可条款约束，分发时须保留这份版权声明和许可声明。

### 移植登记

移植或实质性改写原型代码时，在这里登记。

| BAX 文件 | 原型来源（`src/ba_elo_overlay.py`） | 说明 |
|---|---|---|
| `crates/bax-gamelog/src/discovery.rs` | `_steam_library_dirs`、`find_game_logs` | 注册表与 `libraryfolders.vdf` 定位 Steam 库，候选目录与兜底路径 |
| `crates/bax-gamelog/src/parse.rs` | `LogWatcher` 及其正则 `RE_FIGHT`、`RE_CLIENT`、`RE_PLAYER`、`RE_PERSONA` | 改为按记录解析并带时间；保留空名字；增加生命周期、加载、场景就绪、结束与重连 |
| `crates/bax-gamelog/src/tail.rs` | `LogWatcher.run`、`_latest` | 按偏移量增量读取最新日志；改为按字节切行并处理截断与换文件 |
| `crates/bax-batrace/src/client.rs` | `fetch_json` | 接口路径、请求头、默认不走代理、检查返回的是 JSON |
| `crates/bax-engine/src/session.rs`、`config.rs` | `EloService`、`_warm`、`_fill_levels`，常量 `WORKERS`、`INFO_TIMEOUT` | 5 路并发、单人请求 8 秒超时、旧资料兜底、换局后丢弃旧结果、12 秒与 24 秒后重试 |

## ba-dragon

战后归档与战报统计参考了 [ba-dragon](https://github.com/AlexanderHYu/ba-dragon)（MIT 许可，`Copyright (c) 2026 AlexanderHYu`）的做法，没有复制代码。参考的内容：

| BAX 文件 | 参考的做法 |
|---|---|
| `crates/bax-core/src/report.rs` | 单位成本按目录基础价格加总后，等比例缩放到玩家官方的“出兵价值 − 回收价值”；`WasRefunded` 是回收（全额退款），每条记录算一次出兵 |
| `crates/bax-engine/src/reports.rs`、`config.rs` | 按比赛 ID 取 `/api/match`、比赛结束后按一串时间点重试；请求串行并留间隔 |

ba-dragon 的玩家水平评分、称号与挂机判断属于对玩家的判定，不在 BAX 的范围内（ADR-0002），没有参考。

### 许可全文

```text
MIT License

Copyright (c) 2026 BA-ELO-Overlay contributors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```
