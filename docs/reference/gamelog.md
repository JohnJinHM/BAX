# 游戏日志参考

状态：生效 · 更新：2026-11-17

本文记录《断箭》客户端日志里与对局有关的内容：文件位置、记录格式、BAX 识别的事件和各字段的取值。解析代码在 `crates/bax-gamelog/src/parse.rs`，事件的含义由 `crates/bax-core/src/lifecycle.rs` 解释。

统计数字来自 2026-11-17 对本机 913 份日志（2025-06-16 至 2026-11-16，共约 1.1 GB）的回放，游戏版本以 1.2.2 为主。示例中的玩家、ID 和比赛编号均为虚构。

## 文件

- 目录：Steam 库下的 `steamapps\common\broken_arrow\GameLogs`。BAX 从注册表（`HKCU\Software\Valve\Steam\SteamPath`、`HKLM\SOFTWARE\WOW6432Node\Valve\Steam\InstallPath` 等）找到 Steam，再读 `libraryfolders.vdf` 得到全部库。
- 文件名：`Gamelog__年_月_日__时_分.log`，按游戏启动时间命名，每次启动新建一份。按名字排序就是按时间排序。
- 编码：UTF-8，无 BOM，LF 换行。913 份里没有一份是 CRLF。
- 游戏运行期间文件一直被占用写入；以共享读方式打开不冲突。
- `GameLogs\NetworkMatches\` 下另有按比赛命名的详细日志（包含 `Spawn unit` 等事件），本机只有 2026-03 的一份遭遇战。生成条件不明，留作验证 4 的线索。

## 记录格式

```text
[2026-11-16 18:52:05.319]
Log: [NET_ROOM] WaitingRoom entered, fight id: 9000001

```

每条记录以时间戳行开始（本机本地时间，不带时区，毫秒精度），第二行是级别和消息，消息可以带后续行，最后是空行。级别有 `Log`、`Warning`、`Error`，与对局有关的都是 `Log`。部分消息的后续行长得像别的事件（例如多行警告里出现 `Scenario controller loaded`），所以只解析每条记录的第一行消息，名单与启动信息除外。

## 事件

| 日志原文（开头） | 事件 | 用途 |
|---|---|---|
| `BrokenArrow startup info:` 后续行 `Version: 1.2.2.1` | 游戏版本 | 记入比赛 |
| `GetPersonaName   名字` | 本机 Steam 昵称 | 识别本机玩家 |
| `Deck set to: 卡组名` | 选定卡组；`null` 表示清空 | 记入比赛，卡组名可自定义 |
| `Start reconnect to battle` | 开始重连 | 旧版日志判断重连的唯一依据 |
| `[NET_ROOM] Network lifecycle \| Stage=Game \| Action=Entered \| … \| RoomType=… \| FightId=… \| Status=…` | 进入房间，带状态 | 发现比赛；判断是否重连 |
| `[NET_ROOM] Network lifecycle \| Stage=Game \| Action=Exited \| Cause=… \| … \| Status=…` | 离开房间，带原因和状态 | 断线、结束 |
| `[NET_ROOM] WaitingRoom entered, fight id: N`、`GameRoom entered, fight id: N` | 进入房间，不带状态 | 与上一行重复；旧版日志只有这一种 |
| `[NET_ROOM] WaitingRoom exited`、`GameRoom exited` | 离开房间，不带原因 | 同上 |
| `Room: (WaitingRoom\|房间号), Client: (名字\|ID)` | 等待室里出现一名玩家 | 名单之前提前查询 |
| `Start loading battle... map: 地图, scenario: - 场景` | 开始加载战场 | 地图与场景 |
| `FID:比赛ID 版本 - 名字 (UTC 时间)` | 本机在这场比赛里的名字 | 识别本机玩家（最可靠） |
| `Player list:` 后续行 `ID: N, Name: 名字, Team: 队伍` | 完整名单 | 队伍、名字 |
| `Scenario controller loaded` | 场景加载完成 | 正常开局时的计时依据 |
| `[NET_ROOM] Network traffic history \| Cause=MatchEnded \| FightId=N \| …` | 比赛结束 | 结束时刻 |

以下相似的行不是事件：`Scenario controller start post load`、`end post load`、`wait for all clients connected`（同一阶段的其他步骤），`Request reconnect data`，`Client name - 名字`（进入对局房间后陆续出现，与名单重复），`SID:…`（会话编号，不是比赛编号），`[LOBBY]` 和 `[AUTH]` 的 `Network lifecycle`（大厅与登录）。

### 房间状态（Status）

| 取值 | 进入时的含义 | 样本中进入次数 |
|---|---|---|
| `WaitClients` | 正常开局 | 等待室 509，对局房间 505 |
| `DeploymentPhase` | 重连时正处于准备阶段 | 1 |
| `InProcess` | 重连时比赛正在进行 | 等待室 36，对局房间 36 |
| `End` | 重连时比赛已经结束 | 3 |

### 离开原因（Cause）

| 取值 | 场景 | 样本 |
|---|---|---|
| `ServerExitInstance` | 从等待室转入对局房间（状态仍为 `WaitClients`）；比赛结束后被请出对局房间（状态 `End`） | 等待室 544，对局房间 75 |
| `MatchEnded` | 比赛正常结束，状态 `End` | 413 |
| `ConnectionLost` | 连接超时，比赛照常进行 | 15 |
| `ServiceDisposed` | 主动退出或关闭游戏 | 14 |

比赛结束有两个信号：`Network traffic history | Cause=MatchEnded`（488 次），以及离开对局房间时状态为 `End`。两者一般相隔几十秒，先到的那个为准。

## 正常开局与重连的事件顺序

正常开局：

1. 进入等待室（`Status=WaitClients`），陆续出现 `Room: … Client: …`
2. 开始加载战场 → `FID:` 行
3. 离开等待室（`ServerExitInstance`）→ 进入对局房间（`WaitClients`）→ 名单
4. 场景加载完成：准备阶段开始
5. 比赛结束 → 离开对局房间

中途重连（游戏重启后）：

1. `Start reconnect to battle`
2. 进入等待室（`Status=InProcess`）→ 开始加载战场 → `FID:` 行
3. **场景加载完成**：此时比赛早已开始，这一行不能当作计时起点
4. 离开等待室 → 进入对局房间（`InProcess`）→ 名单 → `Request reconnect data`

旧版日志没有 `Network lifecycle` 行，只能靠第 1 步的 `Start reconnect to battle` 区分重连。

## 名单

- 格式：`ID: 1, Name: 名字, Team: Alpha`。旧版写作 `ID: 1 Name: 名字Team: Alpha`，名字与 `Team:` 之间没有分隔（样本中 1247 行）。
- 队伍：`Alpha`、`Bravo`、`Spectators`（观战，样本中 413 人次）。
- 名字可以为空（样本中 16 人次），可以包含逗号、空格和 emoji。名字以 ID 为准，空名字用接口资料补全。
- AI 玩家的 ID 是负数，例如 `ID: -12, Name: Bot 4, Team: Alpha`（样本中 43 份日志、230 行），出现在遭遇战和合作模式里，与真人混排。AI 的 ID 每场重新编号，BAX 不把它们放进名单。
- 名单以空行结束。实时读取时如果一轮读取没有新内容，不再等结尾的空行。

## 本机玩家

日志没有直接写出本机玩家的 ID。BAX 依次使用：记住的 ID → `FID:` 行里的名字 → `GetPersonaName` 的昵称，名字必须在名单里恰好匹配一人。回放中有名单的 555 场比赛全部识别成功。以前没记过 ID 时，长期库里出现场次超过八成的玩家也可以作为线索（样本中为 517/527 场）。

## 回放统计

2026-11-17 回放 913 份日志：557 场比赛（同一场跨两份日志时计两次，去重后 529 场），555 场有名单，其中 5v5 共 471 场（含 40 场有观战），重连进入 41 场，断线 29 次，被新比赛取代 18 次。新开且已过加载的比赛全部得到计时基准。
