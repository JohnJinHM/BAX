# 数据源与 BATrace 兼容接口参考

状态：生效 · 更新：2026-11-21

本文记录 BAX 的主源与补充源、兼容响应字段的含义和已知的特殊取值。客户端代码在 `crates/bax-batrace`，响应模型在 `model.rs`，主源适配在 `client/upstream.rs`。

字段说明来自两处：原型 BA-ELO-Overlay 本机缓存里的真实响应（玩家资料 196 份、战报 307 份、比赛列表 44 份、单位目录 1 份，2026-11-17 用 `bax api check-cache` 全部解析成功），以及 2026-11-17 对单位目录的一次实际请求。示例值均为虚构。

## 主源与补充源

BAX 现在优先查询 `https://api.brokenarrowgame.tech/user/{id}` 获取基础资料，使用同站 `/user/{id}/last_fights` 与 `/statistic/personal/{marketId}` 获取近期比赛及部分累计统计；战报优先从 `https://s3.brokenarrowgame.tech/fights/fight_{id}.json` 读取。路径和字段实测见[兼容性检查](../validation/upstream-api-compatibility.md)。主源的正式配额与服务承诺尚未确认。

- 基础资料转换：`lvl/rt/rk/rtgms` → `level/rating/rank/ratingGames`；BATrace 补充其他字段，主源已取得的字段优先。
- 累计统计只映射实际返回的计数。空模式对象不转换为零；不将主源 `updateDate` 当作已验证的数据更新时间。主源没有累计价值等字段时，保留 BATrace 或本地已有值。
- 主源失败或数据不合格时尝试 BATrace；两个来源均失败时返回错误，由引擎保留已有档案和已归档战报。昵称搜索、分页比赛列表、单位/专精目录仍使用 BATrace。
- 战报包装为现有 `matchInfo` 归档格式，保留未知字段；只对单位 `TotalSupplyPointsConsumed` 的整值浮点数做有范围校验的整数转换，真实小数不会被截断。
- 主源玩家资料、近期列表和昵称搜索内存缓存 60 秒；累计统计与 BATrace 玩家补充缓存 15 分钟；战报和目录缓存 24 小时。同对象并发请求合并，缓存至多保留约 128 项、单项 512 KiB，大对象仍可获取并交由长期库归档。空/无效主源战报不作长期缓存。
- API 主源、战报对象源和 BATrace 分别暂停请求：429 遵守 Retry-After 且至少 30 秒；连接、超时、拒绝和站点错误暂停 60 秒。某一源被限流不阻止其他源回退。BATrace 非 JSON 的路由级 404 视为服务错误，而非玩家/战报缺失。
- 设置 → 数据源 → 查询数据源状态：分别请求公开样本，显示结果、延迟和时间；手动检测复用 30 秒内结果，不走业务成功缓存，也不清除业务限流。战报样本缺失只表示该样本未验证。CLI 同功能：`bax api sources`。

下列接口说明保留 BATrace 原有路径和字段，供补充源与兼容归档使用。

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

原型发现不带清晰标识的请求会被 403 拒绝。请求频率和站点允许的接入方式尚未与站点确认，客户端采用清晰标识、缓存及退避；公开可读不等于稳定性承诺。

## 接口

| 路径 | 内容 | 空结果的含义 |
|---|---|---|
| `/api/players/search?q=昵称&limit=50` | 昵称候选，含 `players[].id/name/level` 与 `count`；可能是子串匹配，结果不保证身份唯一 | 没有候选；保留画面名字 |
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
| 单位目录 `category_type` | 0～6，含义见下文“单位分类”。2026-11-17 的目录共 462 个单位 |
| 单位实例的 `SpawnTime`、`DeathTime` | **Unix 秒**，不是相对开局的秒数。相对开局的时间要减去 `StartTime` |

## 玩家资料

`info`：`id`、`name`、`steamId`、`level`、`rating`（ELO）、`rank`、`ratingGames`，以及头像、徽章等外观字段。

`statInfo`：累计统计，后缀 `Rt` 为排位、`Ct` 为自定义、`Sk` 为遭遇战，包括场次、胜负、离场、击毁与损失（数量和价值）、对局时长、补给、占点、回收、自毁、友伤，以及按地图的场次、专精等级与经验、专精组合经验与等级、`updatedAt`。样本中 196 份资料有 145 份没有 `statInfo`，界面要按字段完整程度展示。

`last_fights_data`：近期比赛 ID 的字符串数组。

`specExp` / `combinedSpecsExp` 存在经验封顶，不能据此排序使用频率。专精改用[已有战报的出兵约束](player-specializations.md)识别；资料刷新按字段合并，`statInfo: null` 保留本机已有统计。

## 战报

`matchInfo` 包含地图、开始与结束时间、时长、胜方、`TeamMatchPoints`（双方得分），以及 `Data`：玩家 ID → 该玩家的战报。

玩家战报包括赛前赛后 ELO、出兵与回收价值、击毁与损失、伤害、占点、补给、勋章、经验，`UnitData`（实例 ID → 单位实例）和 `TransferredUnits`（转交的单位，只有类型、选项和出生时间）。样本中 307 份战报有 125 份出现转交单位，1 份没有任何单位数据。

单位实例：`Id`（单位类型，对应单位目录）、`OptionIds`（改装选项）、`SpawnTime` 与 `DeathTime`（Unix 秒）、`KilledCount`、`BuildingDestroyedCount`、各类伤害、`TotalSupplyPointsConsumed`、`WasRefunded`、`WasAirdropped`、`SelfDestruction`。

以下规律来自原型缓存里的 307 份战报、183,898 个单位实例（2026-11-17 分析，见[验证 4](../validation/04-units-and-specs.md)）：

- **时间。** 准备阶段部署的单位在 `StartTime` 后 2～12 秒出现（中位数 3 秒），所有实例的时间都落在 `StartTime` 与 `EndTime` 之间。42.6% 的实例没有 `DeathTime`，即存活到比赛结束。
- **实例键。** `UnitData` 的键在一场比赛内唯一，没有出现在两名玩家名下的情况。
- **转交。** 玩家 A 把单位交给 B 之后，实例在 B 的 `UnitData` 里，A 的 `TransferredUnits` 里留有同一个键，单位类型与出生时间都相同；A 自己的 `UnitData` 里不再有它。样本中 125 份战报共 2,474 个转交实例，全部符合。所以 `UnitData` 是比赛结束时各自名下的单位，原本由谁出兵要把转交还原回去。
- **价值。** 按目录的基础价格累加实例，只有战报 `TotalSpawnedUnitScore` 的 61%～88%（中位数 76%），差额来自改装选项等。单个单位的实际成本不能用基础价格代替。
- **专精。** 当前战报样本没有直接提供专精或卡组字段；结合游戏单位可用性表，可以对出兵约束得到唯一组合的战报识别逐场专精。多解或冲突时不计数。

## 收录时间与列表接口

- `/api/match` 不是比赛一结束就有：还没收录时返回 404 或空壳。ba-dragon 的经验是一两分钟，偶尔要好几分钟；本机观察疑似约 10 分钟刷新一次。测量见[验证 3](../validation/03-data-delay.md)。
- ba-dragon 记录过 `/api/players/matches` 列表停止更新（2026-09 实测最新一条停在 9 月中旬），按比赛 ID 取 `/api/match` 更可靠。
- `WasRefunded`：单位被回收（飞机返航、卡车开回、开局卖掉），官方全额退款。它照样上过场，所以算一次出兵，只是不计入花费。

## 单位分类

目录的 `category_type` 与 `role` 组合起来才能区分兵种。下表的名称是按目录里的单位名推断的，尚未在游戏里逐一核对。

| category | 推断的含义 | role 与示例 |
|---|---|---|
| 0 | 侦察 | 10 侦察步战车（M3A3 CFV）；11 侦察坦克；12 轻型侦察车（LAV-25）；13 侦察与炮兵观察车（Stryker RV、M7 BFIST）；32 侦察步兵；33 狙击手；36 特种侦察；100 无人机（MQ-9 Reaper） |
| 1 | 步兵 | 30 普通步兵；31 突击与精锐步兵；33 狙击手；34 便携防空；35 反坦克导弹；36 特种部队 |
| 2 | 车辆 | 10 步战车（BMP-3、Bradley）；11 坦克（T-72B、M1A1）；12 运输车（BTR-82、Stryker ICV）；13 反坦克车辆（Humvee TOW-2）；133 扫雷车 |
| 3 | 支援 | 14 补给车（LMTV、KamAZ）；15 远程防空（Patriot、S-300）；16 近程防空（Avenger、Tor）；130 火箭炮（HIMARS、Grad）；131 迫击炮；132 弹道导弹（Iskander）；133 自行火炮（M109A7、2S19） |
| 4 | 非作战 | 机组与飞行员、训练靶、舰艇、NPC 等，样本里只有 26 个实例 |
| 5 | 直升机 | 70 侦察与轻型攻击；71 通用运输；72 重型运输；73 武装直升机 |
| 6 | 固定翼 | 160 对地攻击（A-10C、Su-25）；161 打击（Su-24M2、F-117）；162 轰炸机；163 运输与空投；164 战斗机与多用途 |

规划要求“火炮和防空、坦克和运输车分别处理”，就要按 role 拆开 category 2 与 3。原型的检测模块把整个 category 3 当作“火炮/防空”，其中混进了补给车。

## BAX 的单位改型显示

战报的 `UnitData.Id` 是基础单位，`OptionIds` 是该实例选择的改装选项。BAX 使用随应用打包的 BA-Units 1.2.2.1 精简目录，按 Modifications.Order 顺序处理替换单位、替换名称、拼接后缀与图标覆盖；不为战报缺失的选项补选默认配置。未知选项不参与名称解析，未知单位沿用 BATrace 目录名称。

基础单位 ID 保持原值。统计与批次按基础 ID 加排序去重后的完整选项集合分组，转交实例保留改型。名称与图标在读取缓存时计算，所以旧战报无需重新获取。此次修复不改变单个单位成本的估算口径。

更新游戏数据时，在仓库根运行 `node scripts/import-unit-variants.mjs [BA-Units/output] [缩略图目录]`，提交精简 JSON 及新增的改型图标。当前目录版本为 1.2.2.1；更早版本曾经变化过的选项名称仍按该目录解释。
