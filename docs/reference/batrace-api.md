# BATrace 接口参考

状态：生效 · 更新：2026-11-17

本文记录 BAX 使用的 BATrace 接口、响应字段的含义和已知的特殊取值。客户端代码在 `crates/bax-batrace`，响应模型在 `model.rs`。

字段说明来自两处：原型 BA-ELO-Overlay 本机缓存里的真实响应（玩家资料 196 份、战报 307 份、比赛列表 44 份、单位目录 1 份，2026-11-17 用 `bax api check-cache` 全部解析成功），以及 2026-11-17 对单位目录的一次实际请求。示例值均为虚构。

## 接入方式

| 项目 | 取值 |
|---|---|
| 站点 | `https://bot.batrace.top` |
| 方法 | 只用 GET |
| 请求头 | `Accept: application/json`、`X-Client: BAX`、`User-Agent: BAX/版本 (+local tool)` |
| 代理 | 默认不走系统代理（原型的经验：经代理访问容易变慢或被拒绝） |
| TLS | Windows 系统自带（SChannel） |
| 超时 | 玩家资料 8 秒，其余 15 秒 |
| 并发 | 同时最多 5 个玩家资料请求 |

原型发现不带清晰标识的请求会被 403 拒绝。请求频率和站点允许的接入方式尚未与站点确认，正式发布前需要确认。

## 接口

| 路径 | 内容 | 空结果的含义 |
|---|---|---|
| `/api/players/info?stbid=玩家ID` | 玩家资料、累计统计、近期比赛 ID | `info` 为空：站点没有这名玩家 |
| `/api/players/matches?stbid=玩家ID&offset=N&limit=N` | 比赛摘要，新的在前，分页 | — |
| `/api/match?matchid=比赛ID` | 完整战报，含单位实例 | `matchInfo` 为空或 `Data` 为空：尚未收录 |
| `/api/units` | 单位目录 | — |
| `/api/dek/specializations` | 专精目录 | 结构尚未确认 |

## 错误分类

| 情况 | BAX 的分类 | 是否自动重试 |
|---|---|---|
| 超时 | 超时 | 是 |
| 连接失败、读到一半断开 | 网络 | 是 |
| HTTP 429 | 限流，读取 `Retry-After` 的秒数 | 是，并暂停全部请求 |
| HTTP 5xx | 站点出错 | 是 |
| HTTP 404，或空结果 | 没有收录 | 否 |
| HTTP 401、403 等其他 4xx | 被拒绝 | 否 |
| 状态 200 但不是 JSON | 被拒绝（多半是人机验证页） | 否 |
| JSON 结构不符 | 解析失败 | 否 |

## 省略默认值

接口不输出取默认值的字段，键根本不出现。BAX 的模型里这些字段都可缺省，原样表示“有没有这个键”：

| 字段 | 缺省时的含义 |
|---|---|
| 战报与比赛列表里玩家的 `TeamId` | 0，即 Alpha |
| `WinnerTeam` | 0，即 Alpha 胜 |
| 单位实例的 `DeathTime` | 存活到比赛结束 |
| 单位实例的 `KilledCount`、伤害、补给等计数 | 0 |

样本中字段的出现比例：玩家的 `TeamId` 50%，`WinnerTeam` 57%，单位实例的 `DeathTime` 58%、`KilledCount` 31%、`TotalDamageDealt` 61%。

## 编号与特殊取值

| 字段 | 取值 |
|---|---|
| `TeamId` | 0 Alpha，1 Bravo，100 观战 |
| `WinnerTeam` | 0 或 1；样本中见过 101 和 -1，含义不明，BAX 视为胜方未知 |
| 玩家 `Id` | 真人为正数；遭遇战与合作模式的 AI 为负数（样本中 -2 至 -15），每场重新编号，BAX 不当作玩家 |
| `DLRatio`、单位的 `TotalDamageReceived`、`TotalSelfDamageDealt` | 出现过 -1，应视为无效值，不能当作数值参与统计 |
| `StartTime`、`EndTime` | Unix 秒。`EndTime − StartTime` 与 `TotalPlayTimeInSec` 相差 0～2 秒 |
| `VictoryLevel` | 样本中有 40、50、60、70 |
| `EndMatchReason` | 样本中有 1、2、3 |
| 单位目录 `category_type` | 0～6。原型的用法：1 步兵，2 坦克，3 火炮与防空，5 直升机，6 固定翼；0 与 4 的含义待验证 4 确认。2026-11-17 的目录共 462 个单位 |

## 玩家资料

`info`：`id`、`name`、`steamId`、`level`、`rating`（ELO）、`rank`、`ratingGames`，以及头像、徽章等外观字段。

`statInfo`：累计统计，后缀 `Rt` 为排位、`Ct` 为自定义、`Sk` 为遭遇战，包括场次、胜负、离场、击毁与损失（数量和价值）、对局时长、补给、占点、回收、自毁、友伤，以及按地图的场次、专精等级与经验、专精组合经验与等级、`updatedAt`。样本中 196 份资料有 145 份没有 `statInfo`，界面要按字段完整程度展示。

`last_fights_data`：近期比赛 ID 的字符串数组。

## 战报

`matchInfo` 包含地图、开始与结束时间、时长、胜方、`TeamMatchPoints`（双方得分），以及 `Data`：玩家 ID → 该玩家的战报。

玩家战报包括赛前赛后 ELO、出兵与回收价值、击毁与损失、伤害、占点、补给、勋章、经验，`UnitData`（实例 ID → 单位实例）和 `TransferredUnits`（转交的单位，只有类型、选项和出生时间）。样本中 307 份战报有 125 份出现转交单位，1 份没有任何单位数据。

单位实例：`Id`（单位类型，对应单位目录）、`OptionIds`（改装选项）、`SpawnTime`、`DeathTime`（秒，相对开局）、`KilledCount`、`BuildingDestroyedCount`、各类伤害、`TotalSupplyPointsConsumed`、`WasRefunded`、`WasAirdropped`、`SelfDestruction`。转交与接收的归属口径由验证 4 确定。
