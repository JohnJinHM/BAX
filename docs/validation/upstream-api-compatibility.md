# 上游 API 兼容性检查

状态：生效 · 更新：2026-11-21（America/Los_Angeles）。前半部分记录只读兼容性检查，后半部分记录接入实现与验证。具体样本标识仅保存在本机原始记录中。

## 来源与方法

- 候选 API：`https://api.brokenarrowgame.tech`。
- 战报：`https://s3.brokenarrowgame.tech/fights/fight_{id}.json`。
- 路径依据：[BA Hub 配置](https://github.com/impossiblyprofane/ba-hub-unified/blob/master/backend/.env.example)及[请求实现](https://github.com/impossiblyprofane/ba-hub-unified/blob/master/backend/src/services/statsClient.ts)。这不是官方接口契约；本次没有确认正式开放政策和配额。
- 从本机 BAX SQLite 库只读选样，与远端 JSON 递归比较。原始样本及临时验证 SQLite 保存在 `%TEMP%/bax-source-audit-20261121/`，不提交玩家资料。
- 使用当前源码构建 `cargo run -p bax-cli --offline -- api check-cache <临时数据库>`，实际调用 BAX 的 serde 模型和玩家摘要生成逻辑。没有以“能解析”为证据推断所有缺省字段齐全。

## 战报

| 样本 | 结果 |
|---|---|
| 排位：4 场样本 | HTTP 200；与已归档 BATrace 战报字段及数值一致 |
| 自定义：4 场对照样本 | HTTP 200；与已归档 BATrace 战报字段及数值一致；包含 1v1/5v5 与观战者 |
| 另一场自定义样本 | HTTP 200；含 10 位参战者及 3 位观战者，无 BATrace 本地对照 |
| 本地日志识别的遭遇战样本（Narva_Skirmish） | HTTP 403，无法验证战报内容；不能据此推断所有 PvE 均不支持 |
| 公开页面中带 AI 的旧战报样本 | HTTP 404；可能涉及历史保留、对象不存在等，原因未确认 |

9 份成功样本共有 4,288 个单位实例、45 条转交记录。保留 Type、TeamId、WinnerTeam 的缺省方式、OptionIds、Unix 时间、双方得分、ELO、伤害与回收等现有结构。

**存在数字类型差异。** 首轮通用 JSON 比较把整数和同值浮点数视为相等，因此只能称“字段及数值一致”，不是 JSON 类型完全一致。上游 `TotalSupplyPointsConsumed` 在样本中有 990 个浮点数，均无小数部分；BAX `UnitInstance.total_supply_points_consumed` 为 `Option<i64>`，直接包装为 `matchInfo` 后 9/9 解析失败。仅将数值为整数且在 i64 范围内的浮点数转换为整数、不四舍五入其他数值后，9/9 解析成功；3 份被解析器识别为含转交单位。正式适配应针对整数目标字段实施精确转换，遇到真实小数时明确报错或调整类型，不能一律截断。

## 玩家资料

2 名玩家的基本资料、累计统计、近期比赛分别返回 HTTP 200，未携带令牌。

| 上游字段/路径 | BAX 对应项 |
|---|---|
| `/user/{id}` 的 id/name/steamId | info 同名字段 |
| lvl / rt / rk / rtgms | level / rating / rank / ratingGames |
| `/user/{id}/last_fights` | last_fights_data；样本各返回 100 个 ID，不能证明无限历史或分页支持 |
| `/statistic/personal/{marketId}` 的 statisticByLobbyType.Rating | 转换成 statInfo 中 Rt 后缀字段 |
| fightsCount / winsCount / losesCount / leavesCount | fightsCountRt / winCountRt / lossCountRt / leaveCountRt |
| killsCount / deathsCount / totalMatchTimeSec | unitsKilledCountRt / unitsLostCountRt / matchDurationTimeSecRt |
| updateDate | 可映射 updatedAt，但其时间语义尚未确认是否与 BATrace 一致 |

转换并组合后，2/2 玩家资料通过 BAX 模型与摘要逻辑。排位场数、胜负、离场、击毁损失数量、平均时长等可计算。基本资料 rtgms 与累计统计 fightsCount 不一致的情况已观察到，不能混作一个字段。

已发现的缺口：

- personal 响应未提供排位累计击毁/损失价值，BAX 的累计价值交换比无法保持完整。
- 两个样本的 Custom、Scenario、Skirmish 均为 `{}`；至少一名玩家的历史 BATrace 档案含非零自定义和遭遇战统计。不能把空对象转换为全零。
- 未取得 BATrace 原有的完整专精等级/经验、国家经验、组合经验、回收/自毁计数、友军补给等字段。当前专精偏好主要由已归档战报推导，因此不等于这些界面功能全失效。
- 地图统计按名称返回，BATrace 按地图 ID 返回，需要版本对应表。
- 未找到且未验证官方候选源的昵称搜索接口；OCR 识别陌生玩家仍存在依赖缺口。
- 未验证等价的分页比赛摘要和 totalCount；不能以 100 个 ID 替代任意历史查询。
- 单位目录没有取得上游等价接口。BAX 当前仍请求 BATrace `/api/units`；独立运行需要提供完整本地目录。已有改装精简目录并不自动替代其全部用途。

## 请求与错误行为

- `/user?ids={id1},{id2}` 返回两名对应玩家；同场 10 人批量请求返回 403。不能据此确定固定批量上限，也不能照搬参考项目的 20 人分块注释。
- `/user/2147483647` 返回 HTTP 200 + `{}`；适配器须识别为空资料，不能合成成功档案。
- 超出常见 32 位范围的玩家 ID 请求返回 400；不存在的测试战报对象返回 HTML 404。
- 参考项目将 `/statistic/individualPlayerStats/{参数}` 作为玩家统计备选，但传入 Steam ID 返回参数 `count` 校验错误；不能直接照抄该回退路径。
- 未做压力测试、长期稳定性验证或维护恢复测试；未确认新战报发布延迟、历史保留期限及 partnerToken 的作用。

## 结论

排位与自定义战报是最成熟的迁移部分，需响应包装与数字类型适配。已知 ID 的玩家基础资料和部分累计统计可迁移，但无法认定无损、完整替代 BATrace。PvE、陌生昵称搜索、完整累计统计、目录与批量稳定性仍须补证。据此采用独立适配器处理字段、缓存和回退，继续保留 BATrace 补充源；实现见下节。


## 接入实现与验证（同日）

已接入主源优先、BATrace 补充/回退，保持现有归档格式与数据库结构。设置新增按接口分项的状态查询，CLI 增加 `bax api sources`。

验证：

- 本地模拟服务器覆盖数字精确转换/拒绝小数、AI 和观战者结构、并发去重、字段优先级、缺失补充不影响基础资料、独立 429 暂停、双源失败分类、空对象、空战报后续收录、缓存到期和主源恢复、状态检查结果与按钮节流。
- 全 workspace（含 OCR、installed-updater）248 项测试通过；Clippy 全 workspace/all-targets 同功能配置零警告通过；前端 34 项测试、Vue 类型检查和生产构建通过，桌面 Full 编译检查通过。
- 新客户端真实只读查询：玩家基础资料、累计统计及 100 个近期比赛成功；排位战报（541 个单位实例）、自定义战报（900 个单位实例）成功。状态查询中两个主源样本成功，BATrace 的玩家补充、搜索、目录为 HTTP 404。
- 设置页使用明确标注的预览数据检查按钮禁用/加载、分项成功与失败，1280×720 和 1000×800 布局可读。浏览器预览不等于桌面 IPC 实机验收。
- 没有运行真实游戏验收，PvE 的实时上游可用性仍未确认；未发布或覆盖安装现有应用。

## 独立 Debug 构建修正（同日）

首次提供的 Debug EXE 误用 `cargo build -p bax-app --features ocr` 生成，依赖 `devUrl` 的 `localhost:1420`，不能在关闭 Vite 后直接运行。改用统一入口重新构建：

```powershell
pwsh -NoProfile -File scripts/build.ps1 -Mode Dev -Edition Full -SkipOcrBuild -SkipNpmInstall
```

- Tauri 执行前端类型检查、生产资源打包及 Debug 编译；输出目录的原生 OCR 资源逐文件校验和协议/模型自检通过。
- 确认 1420 端口没有监听服务，使用临时 `BAX_DATA_DIR` 启动 `target/debug/bax-app.exe`。桌面悬浮窗与主窗口正常渲染；可访问性树中的页面地址分别是内置协议 `http://tauri.localhost/overlay.html` 与 `http://tauri.localhost/`，不是开发服务器。
- 设置页的数据源区域正常显示。自动补读本机最新游戏日志后，一场排位战报成功归档，桌面战后分析可打开；BATrace 单位目录仍返回 HTTP 404。
- 本次未修改原有用户数据库，也未覆盖安装版；开发与打包指南已明确直接 Cargo 编译和独立 Debug 交付的区别。

维护者已确认修正后的独立 Debug 版测试没有问题，并授权发布。本轮没有完成覆盖安装升级或新增 PvE 样本验证。
