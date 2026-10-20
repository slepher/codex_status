# Wi-Fi 激活期间余量同步延迟调查（2026-10-20）

## 范围与反馈环

用户要求调查原因；本轮只读运行数据/MCP、检查生产调度、回放时序和运行已有测试，未修改生产代码、配置、运行实例或固件。保留接手时工作区全部未提交改动。登记设备为 Note4 `7C4FADB93408`，IP `192.168.31.90`，认证缓存版本 `.105-note4`。无串口设备在场，未进行串口操作。

反馈环采用当天日志对齐（源刷新→HTTP Data 发送→ACK），再用生产 `tick % 6 == 0` 条件回放延迟。它能稳定复现调度延迟，但不等于端到端重放真实余量变化；日志没有历史余量具体数值，`usage changed` 也可能包含其他字段，不能据此声称每个变化都是屏幕余量。

## 假设与核验

1. remaining 错分 pull：源码与已有回归不支持。`datasource.rs::classify_trigger` 先将 remaining 解析为 usedPercent，默认 push；运行源配置没有 trigger override。
2. Wi-Fi Data 被调度节流：已确认。`app/src/main.rs:3460–3510` 为防安装任务高频重试，将所有自动 delivery 共用 `auto_deliver && tick % 6 == 0`。`platform.rs::cycle` 最后只有 deliver_now 才执行 deliver，前面即使认证 HTTP 成功、PowerPlan ACK 成功，也不会发送 Data。
3. 源采集或网络失败叠加：两者都有证据。源 fallback 默认180秒；本时段未收到唤醒 poller 的 rate-limit notification。早期 Data 存在发送/前置读取超时；不能把所有延迟归为调度。

## 已确认的延迟链

- run_poller 写共享 envelope 后，没有事件直接唤醒平台投递；平台循环固定 sleep(10秒)，随后串行等待每台设备的网络工作，下一轮才消费 envelope。
- 平台循环只在每6轮放行自动 Data，正常空载也有额外最多约60秒等待；它不是按真实时间运行的60秒定时器。
- cycle 在 Data 前串行执行 refresh_status、占用/claim、sync_http（总预算90秒）、正式Plan及可能再次sync_http。`deliver_now=false` 的轮次同样做这些工作。因此慢请求不仅延后本轮，还拖长后续所有轮次与源快照进入 coordinator 的时间。多设备也共享这一循环。
- 10:35–11:04日志中Plan发送间隔中位约38.45秒（包含不同路径/长间隔，不能当每轮精确耗时）。纯条件回放：变化在一次投递之后、下一轮开始时到达，空载等50秒；每轮工作28.45秒时等220.7秒。此回放解释可能接近4分钟的等待，不是实测某次余量端到端时延。
- BLE会合在生产rendezvous路径可同轮联合Plan/Data，不受上面tick%6门限约束；设备普通会合周期为60秒，故Wi-Fi已在线但Data排队时，BLE下一次会合可能更快。HTTP健康时会抑制普通BLE，这时缺乏另一通道补救。
- 源侧实际10:37:02、10:40:04、10:43:05等约181秒刷新；本时段 rate-limit update notification 为0。不能假定独立spawn的 app-server 会为其他 Codex 会话实时广播变化；具体跨进程通知机制本轮未单独复现。源采集等待与投递等待可叠加。

## 日志证据（北京时间）

原文位置：`~/Library/Application Support/dev.codexstatus.bridge/data/logs/bridge-app.log.2026-10-20`，不复制原始私有数据入库。

| 时间 | 观察 |
|---|---|
| 10:34:01.218 | usage changed/refreshed |
| 10:35:16.906 | HTTP Data867开始发送，距源刷新75.688秒 |
| 10:35:18.838 → 10:35:20.056 | 实际HTTP Data请求/200，仅1.218秒 |
| 10:35:20.124 | Data867 applied/displayed ACK，距源刷新78.906秒 |
| 10:37:02 / 10:40:04 | 源usage changed；后续大量认证status和Plan成功，未出现HTTP Data；缺历史值，不能断言这些变化均需触发余量push |
| 10:54:41.897 | BLE联合Plan/Data869 applied；不是与上述HTTP同一次变化的对照实验 |
| 10:11:45 → 10:11:51 | Data864实际发送后6秒超时；10:20:57同ID重试成功，证明另有历史网络失败 |

11:06左右只读 MCP data_probe：weekly usedPercent40（remaining60）、质量good、resetCredits2；该读数只证明当时源缓存，不证明屏幕值。后续设备可能已离开light窗口，未为调查延长窗口或主动push_now。

## 验证与结论边界

### 用户补充后的历史核验

用户记得最初源实时刷新、Wi-Fi激活时实时投递。Git支持原始意图，不能把180秒说成原本要求的正常实时行为：

- `e22d952`（2026-09-21）在run_poller明确加入 `rate_limit.notified()`，注释规定通知为主、180秒只作fallback。当前runtime仍保留同一机制，文件此后未有修改提交；没有证据说该实时机制被删。当前通知未到达的原因仍未查明，可能涉及app-server行为/跨会话范围，尚不能给出确定上游回归版本。
- `d162945`父版本的Wi-Fi usage push每3秒检查指纹，变化就发送；代码注释明确T9 ≤5秒目标。
- `d162945`（2026-09-22）引入v2平台时，新增10秒循环、每6轮才deliver的门限，同时让v2设备跳过原快速usage push。该提交是**可定位的投递路径行为回退点**：安装重试节流连带限制了Data。
- `fc4f23d`（2026-09-25）按MAC重构继续沿用coordinator路径，未恢复快路径；当前未提交main.rs改动也没有引入这处tick门限。因此不能归咎于今天的HTTP健康/BLE抑制修复；后者可能使长期缺口更明显。
- 已留存的10月4–20日运行日志定向搜索该notification消息无匹配。这仅证明这些保留日志无记录，不证明历史所有环境从未实时刷新。

- 已有 `bridge_core-f41a94d56798c0d3 remaining --nocapture` 四项通过，覆盖字段转换、无本地Profile/重启后的余量push、override和context更换。该二进制时间为10月19日14:24；仅作既有回归证据，不代表当前工作区重新构建验证。
- 时序回放稳定得到50秒/220.7秒的门限等待。没有构造新目录或新增生产instrumentation。
- 已定位与权威设计§9.1“Wi-Fi light可达期间后续push立即排完整投递”不符的确定调度缺口；不能声称已修复，也未完成真实新余量变化的端到端复验。
- 修复方向：已知健康HTTP的dirty Data独立触发/排队，模板/安装保留自己的退避；源快照消费与慢网络循环解耦，诊断同步不能占住实时Data调度。采集侧的180秒fallback与跨会话通知另行确定可接受时延。保持原事务未知结果查询、成功ACK推进指纹、Data不续Light等不变量。
- 待办归 A9，关联B6/C8；本轮不实施、不部署、不提交。
