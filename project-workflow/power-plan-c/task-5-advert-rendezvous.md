# Task 5 — 双策略会合与窗口对齐（候选，需 spike）

Status: 2026-09-27 修订候选：短广播回复含电量，OPEN_WIFI 后设备主动连接 Bridge HTTP 服务。
长期失配时自动保持 device_first 恢复，直到认证重校准并在约定未来窗口切回。
生产协议、反向 HTTP 传输与策略切换均未实现；空口编码/参数及适配器适用范围仍待冻结。
既有 spike 的结论不等于本次完整协议验收；待办状态以 `docs/roadmap/backlog.md` C4 为准。
task-2 的常驻 adapter/Peripheral 生命周期、discovery/INFO/ACK 提速仍优先；两策略须保留并能独立 A/B，
不通过反复撤销代码切换。task-3 分阶段数据决定预算，不能由旧采样推断 GATT 永远无法提速。

## 1. 需求、角色与共用业务

| 策略 | 会合发现与指令交换 | 后续交付 |
|---|---|---|
| `device_first`（默认/恢复） | 设备广播；PC 扫描并作为 central 连接，设备为 GATT server | 既有认证 status/data/plan；小快照 BLE，正式 light 后 HTTP |
| `bridge_first` | PC 在**每个**预定窗口重复广播认证指令，无工作也广播；设备扫描、验证并回复短 StatusBeacon | ACCEPT_SLEEP 后睡眠；OPEN_WIFI 后设备连入 LAN，主动请求 Bridge HTTP 服务，交换 v2 命令及业务 ACK |

倒转 BLE 广播/扫描方向及本策略的 HTTP 发起方向，不让设备发起既有 GATT 连接，不新增 PC GATT server。
每个窗口只生成**一条冻结的逻辑状态回复**，可在短预算内重复同字节广播；包括没收到有效指令的窗口。
回复包含本轮结果和电量，不携带完整状态或业务 ACK。设备承担有界发送尝试，不保证 Bridge 收到，
也不等待 Bridge 确认收到。漏收不能判离线，不能阻止设备按截止休眠；HTTP 认证联系可独立完成会合。
PC 市电承担等待/重试调度；设备仍支付有界扫描、回复、Wi-Fi 建连及等待能耗，成本绝非零。
两路径共用每设备 coordinator、MAC、owner、context、Data/Bundle/PowerPlan 和业务 ACK；
仅会合入口与 transport 分叉，不另建待办/指纹/发布模型。

## 2. 与现行 v2 的边界

总设计 §7/§8 定义双策略的业务与电源边界；`v2Rendezvous()` / `serviceV2Ble()`
仍仅实现 device_first。本次明确新增设备主动 HTTP 交换，须同步总设计中的旧“等待设备端口”描述。
现有 `v2_client.rs` 是 Bridge 主动访问设备的客户端，不能直接用于此方向；新增路由、固件 HTTP client、
反向传输适配与认证交换是**待实现能力**。启用须经过能力协商，不能把未认证 hint 接到开 Wi-Fi 分支。

候选边界：通过当前认证通道安装的会合配置预先限定短时无线引导预算。
认证 OPEN_WIFI 只触发本窗口一次有界 HTTP 会合机会；不创建正式 plan、不修改 accepted plan_id/light deadline，
不产生 BOOT provisional、不许可业务写入。独立 bootstrap 硬截止覆盖回复、连接及首次认证握手。
只有 Bridge 在 HTTP/GATT 中的正式 PowerPlan 可设 light deadline；接受正式计划后由其实际截止及设备安全上限
接管在线期限。重复广告、探测、读取、数据、claim/renew 均不延长任何截止。
ACCEPT_SLEEP 表示本次不打开 Wi-Fi、按既有到期路径收尾，不是新的 sleep PowerPlan；
不能撤销仍有效的正式 light/BOOT 窗口。短指令只用于 deep timer 会合入口。

控制密钥绑定已授权 Bridge/设备。有效他人 owner 拒绝指令；owner 过期也不能由广告重新创建。
空闲 owner 的开网机会只允许已有绑定且已验证设备操作 token 的 Bridge（总设计 §7 的有限 claim 机会）；
随后仍需设备操作 token 的显式 POST /claim。反向会话里的封装见 §3.2：它不是当前已有端点，
也不允许用 HTTP 建连或 `hello` 自动占用。HTTP `/v2/*` 保持 endpoint token + owner，
完整 MAC/session_nonce/context 重新握手；401/409 停写，不暗中抢占。既有 IP→UDP→ARP→BLE 发现链
保留用于 device_first/恢复；本策略的正常交付不需要 Bridge 发现设备 IP 或主动访问设备 HTTP 端口。

## 3. 协议时序与最小状态机

```text
device_first:
  timer → 设备广播 → PC central 连接 → 认证 status → 共用 coordinator
        → [小 Data + 业务 ACK] → 正式 PowerPlan + ACK → 关 BLE
        → sleep/渲染，或有界 Wi-Fi → 共用 HTTP 交付

bridge_first（已认证配置约定窗口、密钥、恢复周期）:
  PC: 窗口前 guard 请求启动 Publisher + Watcher；每窗口冻结一条 directive 并重复发送
      ├─ ACCEPT_SLEEP（无待交付工作）
      └─ OPEN_WIFI（已授权待办/正式同步到期/允许的 claim 恢复机会）
  设备: timer → SCAN（有界）→ 验证有效 directive → REPLY（重复 StatusBeacon）
      ├─ ACCEPT_SLEEP → 回复 ACCEPT_SLEEP → 关 BLE → 渲染 → deep
      ├─ OPEN_WIFI → 回复 WIFI_OPENING → 关 BLE → 连入 LAN → 主动 HTTP 引导
      ├─ 有效指令但被 owner/低电等拒绝 → 回复 REJECTED → 有界收尾
      └─ 截止无有效 directive → 回复 NO_DIRECTIVE（含原因）→ 关 BLE → 渲染 → deep
  PC: OPEN_WIFI 发出即并行等待 {机会性 StatusBeacon, 设备发来的 HTTP 请求}
  设备: POST Bridge /v2/rendezvous/hello → 双向认证挑战 → POST exchange
      → [显式 claim 封装 + 设备 claim 结果] → 正式 PowerPlan + 设备业务 ACK
      → Data / Bundle / Activate 的有界请求-响应交换 → 设备业务 ACK
  PC: 仅 BLE 回复到达仍有界等待；认证 HTTP 已通即可继续，不等广播补包
      → 无有效业务结果则待办保留；发送后未确认先按 Unknown 对账
  设备: 无正式 plan → bootstrap 硬截止关网；有正式 plan → 按实际计划截止执行
```

设备：`DEEP → DISCOVER(strategy) → REPLY(bridge_first) → DEEP | WIFI_BOOTSTRAP → WIFI_LIGHT → DEEP`。
device_first 的 DISCOVER 包含既有 GATT 交换。BOOT 始终允许 device_first，300s 从物理唤醒起算。
PC：`WAIT_WINDOW → EXCHANGE(strategy) → WAIT_HTTP | COMPLETE/RETRY_NEXT`；
WAIT_HTTP 内 BLE 监听与 Bridge HTTP 服务接收入站请求并行，通过认证状态握手才进入共用交付协调器。
同设备业务交付锁不得被整段无线等待占住；取消/截止后旧窗口异步结果不能触发新交付。

分别记录 directive、回复、电量观察、HTTP 认证联系和业务结果。WIFI_OPENING 不等于 Wi-Fi 已连接；
TCP 建连或普通 HTTP 请求不等于身份验证；StatusBeacon 摘要不是 Data/Bundle/PowerPlan ACK。
BLE 回复漏收但认证 HTTP 已通时直接继续，记 `status_beacon=missing,http=authenticated`，不等 BLE 补包。
可信 NO_DIRECTIVE 证明本窗口设备活跃；它报告设备没有接受有效指令，不能证明 Bridge 没有发出指令。
两路全无只记本窗口“未确认”，保留上次联系与电量及其时间，不改为离线或清零电量。
连续缺证据且恢复窗口失败可另报“恢复联系未确认”，仍不能把广告漏收当成设备离线事实。

### 3.1 短状态回复的决定与证据

| result | 接受条件与设备行为 | Bridge 可记录的证据 |
|---|---|---|
| ACCEPT_SLEEP | 接受本窗口有效 ACCEPT_SLEEP；本轮不开 Wi-Fi，按已有截止收尾 | 已接受本轮休眠指令、窗口活跃、电量；不是正式 sleep Plan ACK，也不是已进入 deep 的证明 |
| WIFI_OPENING | 接受 OPEN_WIFI 且本地策略允许；仅发起一次 bootstrap | 已接受开网指令、准备连接；不证明 DHCP、HTTP 或交付成功 |
| NO_DIRECTIVE | 扫描截止仍无可接受且通过认证/新鲜度校验的指令 | 本轮设备活跃，但没有有效指令；reason 可为无候选或仅无效候选 |
| REJECTED | 指令认证、新鲜度有效，但 owner 冲突、低电或配置安全边界不允许 | 活跃及有限拒绝原因；不执行请求动作 |

无效包不触发提前回复/开网，继续扫描到原截止。收到首个有效指令后冻结决定；本轮后续冲突包丢弃并记本地诊断，
不得把已冻结回复改成第二种状态。`NO_DIRECTIVE` 的“仅无效候选”受射频干扰影响，不归因到某个 Bridge。
状态冻结时采一次本轮已有的有效电量样本，0–100%；没有本轮样本则 255（未知），不为采样延长窗口。
重复包的电量保持相同。Bridge 去重键为 `(binding, epoch, window)`，保留首次收到时间；重复包不刷新“最新联系”时间。
较旧窗口的迟到包不能覆盖较新观察；同窗口不同有效状态包视为协议冲突，不混合字段。
HTTP 当前认证状态是更强、通常更晚的证据，同窗口广播不得覆盖其电量/完整状态。
单独保存 `last_beacon_observation` 与 `last_authenticated_status`；广播电量可显示来源与观察时间，但不合成为认证完整快照。
没有广播 ACK、补报队列或事后广播“已睡眠”；控制器发送失败也只记本地结果并按截止退出。

### 3.2 新增反向 HTTP 交换（候选，需单独实现与评审）

Bridge 的已配置 LAN HTTP 服务作为 server（默认现有 HTTP 端口，非 MCP 端口）；设备是 HTTP client。
通过已绑定认证配置预存该 Bridge 的地址、端口、bridge_id、endpoint token 和控制密钥关联。
地址不放进 24B 广告，也不信任任意广播/重定向地址；请求禁止跨源重定向。Bridge 换 IP 或配置失效时本轮限时失败，
下次 device_first 恢复通过认证通道更新地址，不新增 mDNS 或 Bridge 扫设备端口的隐藏兜底。

拟新增两个 Bridge 路由；名称和线格式仍须两端实现时冻结：

| 路由 | 设备请求 | Bridge 响应与副作用边界 |
|---|---|---|
| `POST /v2/rendezvous/hello` | protocol、完整 device_mac/bridge_id、epoch/window、设备 contact_nonce、当前 v2 session_nonce、能力及完整状态快照 | 返回随机 bridge_nonce、原 contact_nonce、交换限制；只创建有界临时握手记录，不 claim、不发业务命令 |
| `POST /v2/rendezvous/exchange` | 两方 nonce、exchange_seq、窗口/身份、上次响应的逻辑请求结果（若有）、必要的当前状态 | 返回一条有界逻辑 HTTP 请求或 WAIT/END；只有验证设备结果后才更新相应业务确认 |

两路都校验 `Authorization: Bearer <endpoint_token>`，且按 MAC/bridge_id 绑定正确 endpoint；
**Bearer 本身不是 server 身份证明，也不能可靠区分共用 endpoint token 的不同设备。** 新增每绑定 HTTP 消息认证：
从控制密钥按固定 HTTP 域标签派生专用 HMAC key（与广告域隔离），使用完整 32B HMAC-SHA-256。
签名输入用确定的长度前缀二进制编码，包含版本、方向、method/path、HTTP 状态码（响应）、完整身份、epoch/window、
两方 nonce（hello 首请求尚无 bridge_nonce）、exchange_seq、原始 body 的 SHA256；响应另绑定对应请求摘要。
请求/响应双方只先读有界认证头/路由字段，核对 token/身份/长度/签名后才处理业务内容；
未知字段/枚举按协商版本规则拒绝，不通过重编码 JSON 验签。
响应认证同时绑定 endpoint token 的摘要，使凭据轮换后的旧响应失效；endpoint token 校验仍是必需条件，HMAC 不替代它。

contact_nonce 与 bridge_nonce 各为新生成的 128-bit 随机挑战。Bridge 只有收到覆盖新 bridge_nonce 的首次有效 exchange
才记录 `http=authenticated`、接受 hello 状态并进入 coordinator；设备只有验证 hello 响应才处理后续命令。
hello 原包重试使用同 contact_nonce，Bridge 返回相同挑战；对重复 exchange 返回相同响应。握手缓存随固定截止失效，
旧挑战/旧窗口/另一个会话的响应不能复用。新 hello 只在配置允许的当前窗口及 bootstrap 硬截止内建立；
已认证并接受正式 light 的会话继续绑定其起始 epoch/window，不因进入下个分钟窗口自动失效，
也不能借该会话启动另一次 bootstrap；其生命期由实际正式期限及安全上限约束。
重连不生成新的 v2 session_nonce、不重置 plan_id、不增加截止；
设备冷启动仍按既有规则废止 v2 session_nonce 并进入 device_first 恢复。
完整 HMAC 保护 HTTP 消息身份、完整性和会话新鲜度，**不提供机密性**；普通 LAN HTTP 的 bearer/业务内容仍可能被旁听，
本候选不宣称解决现有 HTTP 的机密性限制。新控制密钥只通过绑定加密 GATT 配置；token/正文不得记入日志。

Bridge 在 exchange 响应里发**逻辑 HTTP 请求封装**：
`{method, path, authorization, request_id, content_type, body}`；二进制 body 用明确长度的编码，
单次 body 上限候选 8 KiB、Bundle chunk 4 KiB，完整 Bundle 上限沿用设备能力。只允许协议列明的路由，
不支持任意代理 URL、任意设备访问、任意方法或任意头转发。常规业务的 authorization 是 endpoint token；
设备在独立适配器中校验同一绑定，再把解析后的请求送入**同一份** v2 校验/提交逻辑，回 `{request_id,http_status,body}`。
现有依赖 `server` 对象的 handlers 不能直接“当作已复用”；须抽出共享校验/业务层及独立回包适配，保留现有入站端点行为。

**claim 的字面约束必须显式解决。** 空闲设备只接受独立逻辑请求 `method=POST,path=/claim`，
authorization 必须是现有**设备操作 token**，body 保留 bridge 身份及 lease；不得用 endpoint token、控制 HMAC、
hello、GET 状态或 Data 来替代 claim。设备执行与现有 `/claim` 相同的认证和 owner 判断并返回独立 claim 结果，
Bridge 确认 owner 为自身后才能下发常规业务；每次提交仍重验 owner，409 立即停止。续约也必须是独立显式 claim 请求，
没有隐式续约或自动 force 抢占。外层是设备向 Bridge 的 POST exchange，**不是当前直接打到设备的 HTTP POST /claim**。
因此这是新增的“显式 POST /claim 反向封装”能力，必须在实现前单独评审并同步 AGENTS/权威设计的不变量表述。
在该封装尚未通过评审/协商时，保守边界为：仅当前 owner 已有效的设备可用 bridge_first；
owner 空闲/到期时恢复 device_first 完成既有直接 POST /claim，绝不先自动 claim 再声称兼容。

每个 MAC 最多一个业务命令在途；设备只在下一次 exchange 主动上送上条逻辑请求结果，Bridge 收到并验证后才推进。
HTTP 200、返回命令、TCP 写完、hello、WAIT 均不是业务 ACK。PowerPlan 在设备接受时才改变实际截止；
Bridge 收到有效 Plan ACK 后记 accepted/实际剩余时长；ACK 的期限观察带设备单调采样点，
缓存的重复 ACK 不按新收到时间重新起算剩余时长。Data/Bundle/Activate 的 request_id、context、seq/job_id、
幂等、条件提交、A/B 原子落盘及显示结果完全沿用现行 v2；分片定位/传输回执不算 COMMIT 成功。
Bridge 先对账已有 Unknown 任务，再按现有授权调度；模板/Profile 保存、claim 成功、设备来请求都不产生发布授权。

响应丢失时设备重发**同序号同内容** exchange；Bridge 重发同条逻辑请求，不能每次生成新 request_id/plan_id。
设备执行过的请求返回原业务结果，不重复提交。收到设备结果后 Bridge 先可靠记录再发下一条；
同序号不同内容拒绝。Bridge 重启丢失临时缓存时会话失效，持久任务按现有 Sending→Unknown 流程等待认证对账，
不能用新的外层序号推断上条未执行。设备掉电/提交后 ACK 丢失也由下次完整状态对账，不凭广播完成任务。
适配器需为当前会话缓存最近 exchange 及业务请求结果；缓存丢失即终止此会话并重新对账，不能盲目重放非幂等操作。

无正式计划时 hello、claim 和请求重试全部耗用原 bootstrap 截止；先完成正式 light Plan + ACK 且预算足够再传大包。
正式 light 内设备可发有界 exchange 等待新工作；WAIT 只安排下一次请求，不能修改无线期限，不能让设备根据业务续租。
END 不代替正式 sleep Plan，只结束本轮交换；若已有有效 light/BOOT 窗口，继续遵守现行电源规则。
每次请求/收包/执行/回 ACK 都先检查剩余单调期限与安全收尾预算，不能在临界时发起无法完成的操作。
截止、低电、401/409、半包或网络失败结束当前传输；未确认命令留 Unknown/待下次对账。
即使最后 ACK 无法送达，也不得为送 ACK 或等 Bridge 确认延长截止。OTA 不由本候选自动获得反向传输支持，仍需其专用
上传/重启/镜像确认适配与能力；只有待 OTA 且无此能力时安排 device_first，不在此处偷偷回拨设备 HTTP。

## 4. 最小空口候选（legacy 31B）

单个 Manufacturer Specific Data AD；不依赖 scan response/extended advertising。
保守预留 Flags 3B + AD 长度/类型 2B + Company ID 2B，应用 payload **24B**。
Company ID 合法使用、Windows 自动附加 AD 与实际长度须 spike 抓包确认，不借用第三方 ID。
不同时塞名称/128-bit UUID；device_first 保持原发现格式，两格式分别识别。

| 应用字段 | 偏移 | 字节 | 含义 |
|---|---:|---:|---|
| protocol/type | 0 | 1 | 高 4 位版本、低 4 位 Directive/StatusBeacon；未知拒绝 |
| target_short_id | 1 | 3 | 绑定配置分配的路由短身份，不是 MAC 后缀凭证 |
| config_epoch | 4 | 4 | 密钥/策略配置代次，完整配置经认证通道协商 |
| window_seq | 8 | 4 | 本 epoch 会合窗口序号，一窗口一条冻结指令 |
| action/result | 12 | 1 | 下行 ACCEPT_SLEEP/OPEN_WIFI；上行 ACCEPT_SLEEP/WIFI_OPENING/NO_DIRECTIVE/REJECTED |
| reason | 13 | 1 | NONE、NO_CANDIDATE、INVALID_CANDIDATE、OWNER_CONFLICT、LOW_BATTERY、LOCAL_POLICY；按 result 约束组合 |
| battery_pct | 14 | 1 | 上行 0–100 或 255=未知；101–254 拒绝；下行固定 255 |
| reserved | 15 | 1 | 固定 0，非 0 拒绝；不是未协商功能开关 |
| auth_tag | 16 | 8 | 候选 HMAC-SHA-256 截断 64 bit，覆盖前 16B 与绑定上下文 |
| 合计 | | **24** | 16B 正文 + 8B tag；包含电量，不缩短身份、新鲜度或认证字段 |

多字节整数用网络字节序；长度必须恰好 24B，不接受截短、附尾或隐式版本猜测。
原包的 2B `schedule_hint` 删除，换成 1B 电量 + 1B 保留；窗口预测完全来自认证配置中的锚点/周期，
省去一条可能与正式排期矛盾的空口调度输入。无需把 tag 压到 32 bit，也不因电量增加第二个广播包。
reason 映射有限且互斥：接受结果必须 NONE；NO_DIRECTIVE 只允许 NO_CANDIDATE/INVALID_CANDIDATE；
REJECTED 只允许 OWNER_CONFLICT/LOW_BATTERY/LOCAL_POLICY。下行 reason 固定 NONE。

tag 输入含方向域分隔及完整 device_mac、bridge_id、配置摘要，防方向反射；
K 为每对 Bridge/设备专用控制密钥，不能直接广播或复用 bearer token。
新密钥材料只经已绑定加密 GATT 配置；普通 bearer HTTP 不作为密钥保密传输。
后续非密钥策略配置可走当前认证通道，设备安全存储、Bridge 凭据存储，不入仓库/日志。
64-bit tag 是有界窗口及限速下的预算建议；spike 须确定每绑定每窗口候选验签上限 N，
按全部窗口累计尝试 Q 评审约 Q/2^64 的随机伪造上界，未知短身份先过滤，超限只丢弃不延窗。
不足以满足目标时不得直接启用或继续缩 tag，重新评审 AD 载荷/身份预算；CRC/明文 hasWork 不构成认证。
NO_DIRECTIVE 用设备自己预期 epoch/window 签名，不把不可信来包序号回显为可信状态。

只接受当前配置及本地预期窗口；重复同字节返回同决定，不重开网/续截止，同序号不同内容拒绝。
Bridge 工作在窗口中变化，留到下窗口或认证通道处理，不能同序号先发 ACCEPT_SLEEP 后改 OPEN_WIFI。
过期/未来窗口、旧 epoch、未知目标拒绝。序号按认证锚点和单调经过时间推进，墙钟校时不能移动
已接受窗口；正常 deep 在 RTC 保留进度，不每分钟写 flash。冷启动/保留损坏/序号回卷前强制
device_first 重新认证配置新 epoch/密钥，禁止旧密钥下序号归零；epoch 不复用。
Bridge 重启缺配置同样先恢复，不能猜序号。短身份碰撞由完整绑定和不同密钥消歧，验证失败不算在线。
该最小包不携带墙钟校时：bridge_first 空闲周期用 RTC，认证 HTTP/GATT 或恢复窗口再校时；
广播不携带任何调度修正或 deadline 输入。各字段在本窗口内冻结，避免重复包内容变化。

## 5. 截止、Windows 收发、多设备

以下仅是 spike 起点，未实测定值，不声称 2s wake 已达标。

| 预算 | 候选/规则 |
|---|---|
| 会合周期 | 沿用正式配置 60s；先核对现有整分钟对齐偶见 2 分钟的问题 |
| guard | 候选 0.5–2s，加实测 RTC/PC 调度误差；接收时刻不等于精确发射时刻 |
| T_scan / T_reply | 候选 1–1.5s / 0.3–0.8s，有界重复收发 |
| T_bootstrap | 候选接受 OPEN_WIFI 起 15s，含回复、Wi-Fi、HTTP 认证和正式 plan；设备单调硬截止 |
| 设备 HTTP 单次 / 重试间隔 | bootstrap 候选 ≤1s / 0.2–0.5s，截断到剩余总预算；Bridge 有界响应，不长轮询跨截止 |
| PC 首次联系总等待 | 从窗口开启固定截止，覆盖 guard + T_scan + T_bootstrap + 小网络余量；重复回复不重置；正式 Plan 确认后按其实际截止管理会话 |
| 正式会话 | ACK 回实际剩余期限；大任务先确认正式 PowerPlan 及足够预算再开始 |
| 恢复 | 候选每 10 窗口做一次 device_first 校准检查；连续 2 次无有效指令或检查失败进入持续恢复，详见 §6；每次无线会合仍有界 |

Windows Publisher/Watcher 可以同时请求运行，但收发是 best-effort，不能依赖单包或假定所有适配器
可靠全双工/固定发射间隔。窗口内重复同一包，生命周期操作集中在窗口边界。
控制/回复广播均为 non-connectable；角色回调只入有界队列，由主任务校验/开网，不在回调阻塞。
并发不可靠时协商固定 TX→RX 两阶段：PC 先重复指令，再一次性停 Publisher 监听；设备在约定
RX 阶段重复回复。参数经认证配置、计入扫描/回复/bootstrap 预算，不在窗口内高频 Start/Stop。
设备开 Wi-Fi 前完成有界回复发送阶段，不等待接收确认；PC 从 OPEN_WIFI 窗口开始等待设备入站 HTTP，可与监听并行。
Publisher Aborted/能力缺失/系统休眠/Watcher 失败分别记原因，不无限补发；按恢复日程扫描
device_first。无法可靠广播的适配器保持默认策略，仅有扫描能力不够启用 bridge_first。

多设备按 MAC 独立窗口/epoch/待办，单适配器有界公平调度、错开预定窗口；容量不足拒绝新增日程或
认证重新排期，不以共享 hasWork 控制所有设备，不为拥堵延长设备截止。

## 6. 策略切换与恢复（旧候选，已被 §9 的策略内广播恢复取代）

本节保留早期 `DEVICE_FIRST_RECOVERY` / GATT 恢复设计供历史对照。用户已明确要求长期错窗时仍留在 `bridge_first` 协议内部；当前方案、场景与验收以 §9 为准。本节的恢复状态、恢复通道和定期 GATT 检查不得当作待实现要求。

默认 device_first。能力握手须同时确认新广播版本、电量字段、HTTP reverse-exchange 版本、HTTP 双向认证、
Bridge endpoint、逻辑请求白名单、预算及恢复；“支持 v2”或“能广播”不足以启用。反向 claim 独立能力未完成时只允许
owner 有效的会合；需要 claim 则使用 device_first。旧 `schedule_hint` 布局用不同空口版本，禁止混解。
经当前认证通道提交完整配置（strategy、epoch、安全密钥配置、Bridge endpoint、窗口锚点、预算、恢复日程、未来生效窗口），
设备验证并原子保存后回配置 ACK，当前会合结束后才在指定未来窗口生效；重复同配置幂等、冲突拒绝。
Bridge 收 ACK 前不显示“配置已接受”；即使已获配置 ACK，生效前也只显示“待生效”，不能显示已按新日程会合。
ACK 丢失和重校准收敛按 §6.3 处理。BOOT 总可 device_first，300s 不变；周期/持续恢复不开新配对。

### 6.1 持续恢复状态与触发条件

区分持久的用户意图 `configured_strategy=bridge_first` 与实际执行状态；临时恢复不替用户永久改策略。
实际状态为 `BRIDGE_FIRST → DEVICE_FIRST_RECOVERY → BRIDGE_FIRST_PENDING → BRIDGE_FIRST`。
持续恢复意味着**每个后续定时会合都采用 device_first**，中间照常 deep sleep；不代表持续开 BLE/Wi-Fi，
也不是恢复单窗后自动回原日程。没有认证重校准成功与明确未来生效窗口，就没有自动返回 bridge_first 的路径。

| 触发 | 候选规则 | 后续行为 |
|---|---|---|
| Bridge 长期未启动或双方错窗 | 连续 2 个本地预期窗口没有接受有效 Directive | 本窗 NO_DIRECTIVE 回复和收尾不延长；下一定时会合进入持续恢复 |
| 周期校准检查 | 正常模式每 10 窗口强制一次 device_first，独立于广告接收成功与否 | 本次须认证确认排期仍在误差预算内；没连上、认证失败或不能确认，均持续恢复，不单窗返回 |
| Wi-Fi 引导失配 | 连续 2 次接受 OPEN_WIFI，但截止前未完成 HTTP 双向认证 | 下一会合持续恢复，可更新 Bridge endpoint；广播到达不能掩盖长期 HTTP 不通 |
| 时间/配置不可信 | RTC 保留校验失败、时钟不连续、预测不确定度超 guard、未知 epoch、序号将回卷、冷启动 | 立即禁止按旧锚点接受指令，下一可用会合采用 device_first |
| 权限/能力不可用 | owner 空闲或到期而无反向 claim 能力、有效他人 owner、密钥撤销或配置不支持 | 停止当前业务，采用有界 device_first 恢复；仍须绑定认证与既有 claim，不能抢占或开放配对 |

连续未收到有效指令计数只由本地窗口推进；重复包不重复计数，无效/过期包不清零；
真正接受本窗口 Directive 才清零该计数。HTTP 引导失败计数仅由成功双向 HTTP 认证清零。
进入持续恢复后，上述计数清零、时间流逝、收到旧 epoch 的有效签名包，都不能撤销恢复状态。
低电保护优先于恢复，可能跳过无线并保护关机；恢复不授予额外电量预算。

周期检查若认证确认原 epoch/完整配置摘要一致、单调时间映射仍在 guard 内，可明确指定原日程的下个未来窗口返回；
这一条只适用于尚未发生失配的正常周期检查，不能用于退出已经锁存的持续恢复。
一旦需要修正 anchor/周期/相位，或已进入持续恢复，必须走 §6.3 的新 epoch 校准；不能只改墙钟或降低窗口校验标准。
恢复状态与失败计数保存在受校验的 RTC 保留区，普通 deep 不丢失、不每分钟写 flash；
冷启动无论持久策略为何都从恢复开始。已接受配置摘要/epoch 的安全检查点原子持久化，防止掉电后旧配置复活。

### 6.2 Bridge 重启后如何找到设备

Bridge 启动、系统唤醒或检测到本机单调时钟/原锚点不可延续时，把受影响设备的日程标为 `needs_recalibration`，
启动既有 device_first Watcher/central 恢复发现。**不能只按磁盘上的旧 bridge_first 窗口扫描**，也不能先等状态广告才扫描。
PC 在应用运行期间持续提供恢复扫描服务，按适配器约束公平分时；设备则在自己的有界周期主动广播既有 device_first 格式。
单次 PC 观察预算到期只更新“尚未重新校准”，后续继续扫描，不把设备删为离线、不停止恢复发现。
PC 如同时服务其他设备的 Publisher，必须预留实际可用的恢复 RX 时段；适配器无法满足时降低并发/暂停候选策略。
设备每轮 deep 的恢复周期候选仍为 60s，依据本地单调时间推进，不依赖已失效的 PC 墙钟对齐；
Bridge 无需预知其相位。未实测前不承诺固定几秒重捕获，但不能设计成双方永远只在旧错开窗口活动。

发现广播仍只是 hint；Bridge central 连接并核对绑定、完整 MAC、bridge/endpoint 凭据、owner 与设备当前会话状态，
才执行校准。它读取 `configured_strategy`、实际恢复状态/原因、当前及 pending 配置摘要/epoch、窗口序号、
设备单调时间样本、时钟误差界与恢复周期；这些是待新增的认证状态字段，不塞入短 StatusBeacon。
设备 IP/显示名不作为恢复身份，旧 BLE 地址缓存不能绕过 MAC 核对。
Bridge 必须从认证设备状态对账自身持久记录；磁盘里旧 anchor 不直接作为正确时间来源，
Bridge 不知道 epoch 分配历史或检测到记录回退时须经绑定加密 GATT 安装新控制密钥，不能猜测旧 key 下的安全 epoch。

### 6.3 认证校准、未来生效与丢 ACK 收敛

恢复会合使用已有绑定加密 GATT 认证入口；其配置写入须维持现行 owner/设备操作 token 权限边界。
有效他人 owner 不得被改排期；owner 空闲时先按现有有限电源机会完成直接 POST /claim，再继续授权配置操作。
校准失败只影响恢复状态，不创建 owner、不发布模板、不替换业务上下文，也不获得 BOOT provisional。

1. **测量并校验时间。** 在当前认证会话交换 Bridge/设备的单调收发时间戳，带本次随机会话挑战及配置摘要；
   用往返延迟为两时钟映射建立误差区间，拒绝过大的 RTT、时钟跳变和不确定度已超过 guard 的样本。
   设备墙钟可以在该认证通道另行校时，但会合 anchor 必须映射到设备本次单调时间，不能仅用一条 UTC 值减本地时钟。
   既有 PowerPlan、BOOT 和当前恢复无线截止保持原单调时间计算，校时不能移动它们。
2. **准备完整新排期。** Bridge 持久记录候选 `config_id/digest`、未用过的新 `config_epoch`、窗口序号起点、
   周期、双方单调 anchor 映射及误差界、guard/扫描/回复预算、恢复参数、endpoint 和明确的未来激活窗口。
   epoch 由每绑定的持久分配记录递增分配并核对设备高水位；耗尽/历史不可信时先换新密钥，禁止同 key/epoch 下重置 seq。
   激活时刻必须晚于当前有界会合结束，至少留下一个完整恢复周期作为准备余量；具体余量须经 spike 定值。
   选择任意远的未来时刻也不安全：预测到激活点的累计时钟误差必须仍小于 guard。
3. **设备安装并 ACK。** 设备校验完整配置、权限、候选摘要与未来期限，原子保存 pending 配置和 epoch 高水位后，
   返回 `config_id/digest/epoch/activation_window/result=accepted` 及实际激活单调时间/不确定度。
   此时状态为 BRIDGE_FIRST_PENDING，在生效前的定时会合继续使用 device_first；重复同一配置只回同一结果，
   不重新取“从现在起”的 anchor/生效时刻。相同 id 不同内容拒绝；尚未安装但激活点已过去的配置拒绝。
   未得到可接受误差界、安装失败或无法保存安全检查点，都保持 DEVICE_FIRST_RECOVERY。
4. **双方覆盖过渡期。** Bridge 必须先持久记录候选，再发送安装请求；发送后即使 ACK 丢失，也继续 device_first 扫描，
   并从该候选未来窗口按候选新 epoch 广播冻结指令。不得“没收到 ACK 就不广播”，否则设备可能已安装而被迫再次失配。
   Bridge 经认证读取确认设备 pending/active 摘要，或幂等重试相同配置来消除 unknown；不因 ACK 丢失生成另一个相对 anchor。
   Bridge 在此阶段重新退出/重启则失去原单调映射可信度，回到 §6.2 重新发现与校准；不得凭旧候选猜测发射窗口。
5. **到点切换并观察。** 设备仅在已接受的未来激活点、RTC 连续且误差仍在 guard 内时应用该新配置，退役旧 epoch，
   清零相应失败计数后开始 bridge_first；无有效 pending 配置不退出恢复。首窗若漏收指令，仍按连续 2 次缺失规则
   重新进入持续恢复，不回滚到旧 epoch。Bridge 区分“配置已接受”和“新日程已有证据”；只有新 epoch 的有效回复或
   认证状态/HTTP 联系才能报告按新日程会合，配置 ACK 本身不证明射频命中。

不尝试用有限次 ACK 保证双方同时知道切换成功。收敛依靠“设备持久 pending/epoch 高水位 + Bridge 持久候选 +
过渡期新日程广播和恢复扫描 + 失配后持续 device_first”。即使配置 ACK、状态回复连续丢失，也始终留有再次认证校准的路径。
Bridge 退出任意久后重启都从发现真实设备状态开始；如果它一直未运行，设备可以无限多个周期保持恢复，
每周期仍只支付有界会合成本。不能用恢复轮数或超时自动恢复旧日程。

### 6.4 漂移、掉电与安全/功耗边界

- RTC 误差界按上次认证校准误差加目标实测漂移上界随经过时间增长；在覆盖 guard 前主动进入持续恢复。
  尚无可信漂移上界时保持保守的周期校准，不承诺长期空闲仍无需重校；不能靠无限加宽扫描窗口掩盖漂移。
- 设备冷启动、RTC 校验失败或 pending 激活 anchor 所属单调时基丢失，均废止该激活安排并保留 epoch 高水位，
  进入 device_first；下一次用新 epoch/必要时新密钥，不能把 pending 的相对延迟重新从启动时刻计时。
- 旧 epoch 指令、旧校准会话、旧配置安装消息、旧 ACK 不得清除恢复标志；配置 ACK 只确认匹配当前候选的摘要。
  正常墙钟校正不调整已接受 window_seq，更不能把序号倒退到可接受历史广播的位置。
- 单个恢复会合的广播、连接、认证、校准、claim 和必要回包共用其原总预算。到点未完成就保留恢复状态、下次再试，
  不持续扫描、不自动开 Wi-Fi、不续 light/owner、不获得 BOOT 300s。需要正式 light 的部分仍先接受正式 PowerPlan。
- 只有恢复状态变化/接受新配置才写必要的安全检查点；连续失败次数保留在 RTC。评估长期恢复功耗与配置写入寿命，
  不能把“Bridge 用市电”视为设备恢复无成本；任何未来退避策略须有明确最大恢复间隔，不能导致无法重新发现。

## 7. spike 与验收

1. 先测 task-2 后的 device_first，再同负载/PC 网络形态独立 A/B bridge_first，各 ≥30 周期；
   分别记录 RX/TX/connect/discovery/INFO/HTTP/render 时长、全部失败分母及重捕获次数。
2. Windows 并发和一次 TX→RX 退化、实际 31B AD、蓝牙/系统重启、当前适配器成功率；
   无数据 ≤2s、命中率 ≥95% 仅候选目标，非兼容适配器不能默认启用。
3. 注入下行全丢、上行全丢但 HTTP 成功、只有回复/HTTP 超时、两路全丢、重复/冲突/重放/
   错目标/错密钥、owner 冲突/到期、PC 退出、设备冷启动、切换 ACK 丢失、RTC 漂移、多设备排队。
4. 每窗口设备有回复发送尝试，PC 分别记录 beacon/HTTP/业务 ACK；仅成功业务 ACK 更新
   指纹/full_sync_deadline/job/plan。NO_DIRECTIVE ≠ ACCEPT_SLEEP，WIFI_OPENING ≠ 交付完成。
5. PC 崩溃、连续重复包、认证失败、半包、无正式 plan 均不导致无限开网；原正式计划幂等、BOOT、
   pull-only/claim 和显式发布规则不回归。完整回归在后续实现阶段运行。
6. 能耗计入扫描、回复、Wi-Fi 建连/等待、渲染、恢复窗口；无电流仪仅报估算区间，
   不以“回复成本≈0”或跨项目标称续航代替本板证据。

本次补充的可执行验收判据（实现后才运行；本次只评审文档）：

| 故障/场景 | 必须观察到的结果 |
|---|---|
| 无工作、有 ACCEPT_SLEEP | 一条冻结状态重复发送，含电量；不开 Wi-Fi、不申请广播 ACK；到期睡眠 |
| PC 完全漏收状态广播 | 设备退出时刻不晚于相同配置预算；PC 保留旧状态/时间并记本窗未确认，不判离线 |
| 下行全丢或只有错签名/过期包 | 本地窗口签名 NO_DIRECTIVE；不开 Wi-Fi、按恢复日程执行；伪包不能延窗 |
| 0%、100%、未知电量与重复/迟到包 | 边界编码正确；无样本为 255；同包不反复刷新联系时间；旧包不覆盖新 HTTP 状态 |
| 回复全丢、OPEN_WIFI 成功 | 抓包首个 TCP SYN 是设备→Bridge；仅允许该方向也能完成认证/正式 Plan/Data/Bundle；Bridge 不访问设备端口 |
| WIFI_OPENING 后 DHCP/Bridge 地址/HTTP 失败 | 不把广播当 HTTP ready；全部重试共用固定 bootstrap 截止；任务不虚报成功 |
| 仿冒 server、错误 endpoint token、跨 MAC 共用 token、旧 HTTP 响应 | 双方 nonce/HMAC/绑定校验拒绝；无 claim、无开网续期、无业务写入 |
| hello/exchange 重试、claim/Plan/COMMIT 响应丢失 | 固定交换序号与业务 ID；无重复占用副作用/Plan 续期/二次提交；丢结果先 Unknown 对账 |
| owner 空闲/他人占用/中途到期 | 未协商反向 claim 时回 device_first；已协商时仅显式 POST /claim 封装可改 owner；错操作 token 为 401，冲突 409 |
| Bundle 暂存中切模板、截止或进程重启 | context 条件失败或丢弃暂存，保留原 A/B 完整包；不能凭 HTTP 200 更新成功指纹/job |
| 最后业务 ACK 丢失或来不及送出 | 设备不延截止；Bridge 下次认证状态对账；传输 ACK 与业务 ACK 分开统计 |
| 不支持新广告/反向 HTTP、OTA 无适配 | 维持/恢复 device_first，不尝试旧布局、自动回拨或偷偷降级认证 |
| Bridge 停止 24h/更久并注入设备 RTC 相位漂移 | 连续 2 窗失配后每个后续会合都是 device_first；超过 10/100 个恢复周期仍不自行切回旧日程，单窗无线不超预算 |
| Bridge 重启且持久记录中的窗口已错开 | 不依赖旧 anchor 的 Watcher 能发现设备；通过完整 MAC/绑定认证重建状态，完成新 epoch 校准后才在约定未来窗口切回 |
| 第 10 窗周期检查失败，之后重放有效旧指令 | 失败即锁存持续恢复；旧包不能清除状态；正常检查仅在认证确认原排期误差后才能指定未来返回窗口 |
| OPEN_WIFI 指令一直可收、HTTP 连续失败 | 达阈值仍进入持续恢复并认证修复 endpoint，不因广告成功永久卡在失败 bootstrap |
| 配置安装 ACK 丢失，设备已保存 pending | Bridge 仍按候选日程广播并继续恢复扫描；重试相同配置不移动激活点；认证读摘要可收敛 |
| 配置请求丢失、Bridge 已准备新日程 | 设备保持 device_first，Bridge 仍可发现；不会凭不存在的 pending 配置切回，过期安装请求被拒绝 |
| 保存 pending 后设备掉电、Bridge 退出或任一单调时基重置 | 废止不可映射的激活点，保留安全 epoch 高水位；重新认证配置，不按旧相对延迟/旧 key 序号归零启动 |
| 校准 RTT 超界、墙钟前后跳、漂移误差超过 guard | 保持持续恢复或主动进入恢复；PowerPlan/BOOT/当前会合截止不移动，不能通过加宽无线窗口掩盖问题 |
| 新 epoch 首窗/首两窗全部漏收 | 一窗未确认、两窗后重新持续恢复；不回滚旧 epoch；配置 ACK 不能被统计为新日程命中 |
| 重放旧配置/ACK、owner 冲突、低电 | 旧会话/摘要/高水位检查拒绝；无抢占、无重新配对、无无限在线；保护关机可优先于恢复 |

每条注入同时断言 owner、plan_id/实际截止、active_context、data_seq/job 与确认指纹是否变化；
成功率分别统计广播发送尝试、收到有效回复、完成 HTTP 认证、完成业务 ACK 的分母，不能把其中一项代替另一项。

实现门槛：spike、预算/密钥生命周期/合法 AD 标识定值、HTTP 双向认证与反向 claim 封装评审，
同步权威设计/不变量并确认能力协商后再安排源码任务；不代表候选参数已冻结或现有协议已经实现。
本轮只改文档。task-1 §9 的明文 hint 仅适用于旧发现提示，不适用于本控制面；
task-3 的小数据 BLE 验收归 device_first，bridge_first 验收 HTTP 快照与相同业务 ACK，分记 transport。

## 8. 已核对复用点（2026-09-22）

- `bridge/crates/ble/src/lib.rs`：V2Connection::connect/command/close，PC central、MAC 核对、20ms ACK；
  当前注释明确 Windows disconnect 清 GATT 缓存，不能假定可跳过 discovery。
- `bridge/crates/app/src/main.rs`：250ms v2 机会循环/55s 去重，尚非常驻 Publisher/Watcher 调度。
- `bridge/crates/app/src/platform.rs`：ble_cycle/cycle、occupancy_gate、plan_for_rendezvous/note_ack，共用业务入口。
- `src/ble_bridge.cpp`：NimBLE server、绑定/加密、原 GATT/广播；扫描/StatusBeacon 尚需实现。
- `src/main.cpp`：serviceV2Ble/v2Rendezvous/applyV2Plan 与 /v2/*；timer 目前开网依赖正式 plan。
- `docs/generic-display-platform-design.md` §2、§6–8、§10：身份/owner、正式 PowerPlan、ACK、多设备边界。

2026-09-27 本次只读补核：`bridge/crates/core/src/v2_client.rs` 当前使用 TcpStream 主动连设备，
`src/main.cpp` 的 `handleV2Status`、`v2Command`、`handleClaim` 当前绑定入站 server/已有 BLE 回包路径。
上述代码只能证明可抽取的共用业务边界，不能证明反向 HTTP/claim 已存在。未构建、未部署、未操作设备。

## 9. Note4 纯广播重新对齐实验方案（2026-09-27，Astra 设计，待实现/实测）

本附节是供原型实施与 Luna 实机执行的实验合同；**不把 §6 的 device_first/GATT 恢复作为实验中的自动兜底**。
§1–8 和总设计仍记录上一版候选，不能据此认为用户已选择了 GATT 恢复。此轮要比较 bridge_first 内部的纯广播恢复，
实验结果出来后再决定正式恢复合同。本文所有时长、成功率和成本排序都是待验证假设。
分工：Astra 写方案；主执行者实现原型和审核；Luna 测 Note4；最终汇总由主执行者完成。

### 9.1 本轮只读证据与能力边界

- 仓库现状：本文件、PROGRESS、backlog、总设计均已有未提交文档改动；本附节只追加，不覆盖已有内容。
- `src/main.cpp:4576` 的 `v2Rendezvous()` 仍为设备开 BLE、等待 central、处理 `serviceV2Ble()`、等正式 Plan ACK、关 BLE；
  未发现生产 bridge_first、恢复广播、广播校准或反向 HTTP 状态机。当前源码 Note4 版本为 `0.18.25-note4-b`。
- 本轮读磁盘 `bridge/target/debug/data/platform/state.json`：Note4 MAC `7C4FADB93408`、IP `192.168.3.177`、
  sync_enabled=true；`last_authenticated.observed_at=1790511922`，http_v2、fw=`0.18.25-note4-b`、battery=72、
  context=`be2d8738639543f5`、active=`codex-status-a`、data/applied_seq=136、commit_seq=272。
  这是已有认证快照，不是本轮直接联系证据；其 provisional=274s 也不能当作当前仍有 274s。
- `src/ble_bridge.cpp:560` 左右的 `bleScanJson()` 与 token 鉴权的 `POST /diag?blescan=N&company=65535` 已有。
  它会停设备广播、active scan、临时设置 Wi-Fi PS_NONE，并同步阻塞 HTTP；它**不是只读状态接口**，
  不是实际 deep timer 入口，也不是恢复/认证/RTC 对齐实现。本轮没有调用该入口或打开串口。
- `bridge/crates/ble/examples/adv-spike.rs` 是已有独立 Windows Publisher + btleplug watcher 工具，
  只发固定测试字节，没有 challenge、认证、窗口调度或设备回复；默认 `bridge/target/debug/examples/adv-spike.exe` 不存在，
  不代表隔离 target 下也不存在。可以复用 Publisher/Watcher 实现，不能把旧输出当新方案验收。
- `task-6-bridge-first-impl.md` §1.1–1.2 的旧证据来自 2026-09-23 Realtek 适配器和 **1.54 / 0.17.9-bw**：
  24B payload、non-connectable、设备收 14 包/12s、首次271ms；PC Started P50 17.5ms、P95 22.3ms（n=3）。
  它证明旧现场单向可行，不证明 Note4 的双向广播、短窗命中率或错窗恢复。
- 受限会话的 `Get-PnpDevice` / `Get-ScheduledTask` 均拒绝访问，未因此改设置/启动服务。
  阶段2需在非受限只读检查中重新确认当前适配器、驱动与任务/PID，不能把旧 Realtek 现场当成实时枚举。

Windows Publisher 是 best-effort；Started 仅是 API 状态，不能作为精确空口 TX 时间。
同适配器通常不能收到自己的广播，必须以 Note4 的独立 RX 时间戳为命中证据。
来源：[Microsoft Publisher 文档](https://learn.microsoft.com/en-us/uwp/api/windows.devices.bluetooth.advertisement.bluetoothleadvertisementpublisher?view=winrt-26100)、
[Microsoft BLE Advertisements](https://learn.microsoft.com/en-us/windows/uwp/devices-sensors/ble-beacon)。

### 9.2 三个待比较候选

基线前提：双方已持有可信绑定配置和控制密钥，设备 RTC 连续；Bridge 进程离线导致旧相位不可用。
新增的Bridge重启、设备冷启动及移出覆盖场景见 §9.7；RTC不连续但持久安全记录完整时可重新挑战校准，不能沿用旧anchor。
恢复时不改变业务 PowerPlan、不打开 Wi-Fi、不 claim、不接 GATT、不传业务数据；只找回双方的窗口映射。
“设备先广播”并不等于现有 device_first：下表 B 的所有收发仍是 non-connectable 广告，不建立连接。

| 候选 | 重新相遇方式 | 起始参数（待测） | 预计优点 | 预计代价/失败模式 |
|---|---|---|---|---|
| A：PC 长恢复广播，设备短扫描 | PC 重启即 Publisher+Watcher，向该绑定持续重复 RECOVER_AVAILABLE；设备每个本地恢复周期扫，命中后进入共用广播挑战 | 恢复周期60s、设备扫描1.5s、PC一个观察批次130s，应用运行期间继续后续批次 | PC 不用知道旧相位，设备离线时只付固定短扫成本；逻辑最直接，优先做基线 | PC长期占广告资源；Windows并发收发能力须实测；首次命中只证明发现，仍需双向校准 |
| B：设备短恢复广播，PC监听后回指令 | 设备每轮直接发 RECOVER_CHALLENGE；PC常驻Watcher，命中才发OFFER，设备转短扫 | 设备TX 0.8s、RX 2.5s，单轮总上限6s；60s周期 | Bridge没有长期Publisher，离线时设备TX可能比RX便宜；无旧相位依赖 | Windows从收到事件到更新Publisher的延迟可能超过RX窗；设备通常要付TX+RX，未测不能说更省电 |
| C：PC周期恢复广播，设备轮转短扫 | PC按新单调周期发恢复包；设备连续失败后将本地扫描相位按固定步长轮转，命中后走共用挑战 | PC每10s发2s；设备每轮扫描1.5s，轮间隔依次为60/62/64/66/68s后重复（相位步进表须实际核验） | 减少PC长期广播占用，设备单次RX不膨胀；保留纯广播 | 多轮才命中，周期混叠/Windows延迟可破坏覆盖；不存在无条件“5轮必达”保证；设备额外定时唤醒和RTC误差增加 |

C 的首个最小实现建议使用更易审计的恒定62s恢复周期，相对PC 10s周期每轮前移2s、5轮走一圈；
上表变周期表作为可选后续试验，避免一开始同时改变太多变量。这个覆盖推导只在理想时序且TX可用宽度足够时成立。
PC每10s“请求”发2s并不等于空口真的有2s；记录 Started/Aborted/实际接收后再判断是否满足覆盖条件。
正常无失配时仍使用同一组60s bridge_first 日程；C只改变恢复周期，不能改正式 PowerPlan、显示墙钟或业务deadline。

第四个可选比较项是渐进扫描预算（0.5/1/2/4s封顶），但不作为首批第四套实现：它与A叠加，用于找短扫成功率/成本曲线。
禁止扫描预算无上限增长；最大4s仍失败就下一轮重试，应用退出多久都不改变这个上限。

### 9.3 共用广播校准：从“找到包”到“排期确实对齐”

**最小可证目标分两层。** RF 原型可以先只交换实验run_id/nonce并测时间，明确标记 `auth=none, production_eligible=false`，
不执行 OPEN_WIFI/claim/Plan，不修改生产会合配置，也不能得出“认证恢复已通过”。
正式候选再在相同收发骨架上加入每绑定HMAC、新鲜随机挑战、epoch持久检查点与未来激活。
这种拆分允许先测当前适配器是否能完成短窗双向广播，避免为了比较RF时序先实现整个反向HTTP协议。

认证原型的24B帧布局如下，使用新实验协议版本（例如高4位0xE），绝不复用 §4 的线上解释：

| 字段 | 字节/偏移 | 规则 |
|---|---|---|
| version/type | 1 / 0 | AVAILABLE=0、CHALLENGE=1、OFFER=2、ACK=3、NORMAL=4；其余拒绝 |
| binding_short | 3 / 1 | 只路由，完整MAC/bridge_id/原配置摘要进入HMAC上下文 |
| epoch | 4 / 4 | AVAILABLE/CHALLENGE为当前epoch；OFFER/ACK/NORMAL为候选新epoch |
| body | 8 / 8 | 逐类型见下文 |
| tag | 8 / 16 | HMAC-SHA256截断64位，方向、type、完整绑定、原配置摘要与事务上下文域分隔 |

- AVAILABLE body：PC本次恢复随机标记64位；只触发一次有界挑战，**不能单独校时、激活或清除失败状态**。
  其认证上下文使用不可变的预安装绑定/周期白名单摘要，epoch只作PC已知值提示，不作为发现阶段相等过滤条件，
  否则PC丢过ACK后只发旧epoch会与设备再次永久错开；新排期必须随后按CHALLENGE/OFFER高水位规则检查。
  它允许在失配状态识别已有绑定；旧AVAILABLE重放最多浪费一次受总预算限制的尝试。
- CHALLENGE body：设备新生成64位随机 `recovery_nonce`。本次唤醒仅一个nonce，在开始请求发送前取设备单调点 `d0`。
  nonce须来自已初始化的硬件随机源，不能用millis/MAC；定期更换、累计碰撞预算和64bit tag验签预算在正式冻结前评审。
  重复广播保持相同nonce和d0；PC收到第一份时记录自己的单调 `b1`，重复包不替换这个测量点。
  CHALLENGE也用上述不可变绑定摘要认证，epoch报设备当前安全高水位；PC才能在不相信旧窗口映射的情况下识别当前事务。
  不可变摘要覆盖密钥代次、完整双方身份和可选周期/预算集合；恢复广告不能修改它。
- OFFER body：`first_window_seq:u32=0, activate_after_10ms:u16, period_code:u8, flags:u8=0`；
  初值 `activate_after=6500` 即相对**设备原d0**的65s，period_code从预安装白名单选择60s（后续可测120s），
  不允许广告携带任意长周期。OFFER的tag额外覆盖recovery_nonce和原epoch；回应只对本轮nonce有效。
  新epoch必须严格高于双方持久高水位；初版只接受原epoch+1，耗尽/历史回退/不一致拒绝并标记需另行恢复。
  Bridge先持久记录候选和b1，再发送同一OFFER；不能重复包改相对延迟。
- ACK body：`first_window_seq:u32, round_trip_10ms:u16, battery_pct:u8, result:u8`。
  设备收到OFFER时取d2，测R=d2-d0，向上取整到10ms；tag覆盖本次nonce和完整OFFER摘要。
  仅 `result=accepted` 且R不超预算才安装未来排期；电量字段沿用0–100/255，重复ACK冻结。
  原epoch、nonce、d0和完整pending事务保留到激活/到期；不得因epoch预分配就丢掉ACK重试上下文。
- NORMAL body沿用 §4 的window_seq/action/reason/battery/reserved；原型仅ACCEPT_SLEEP。
  tag使用新配置摘要和新epoch；设备从新seq=0开始只因新epoch已持久保留，绝不同epoch倒退序号。

所有帧长度恰24B、大端、未知枚举/保留值拒绝；测试Company ID沿用现有spike隔离约定，生产仍须确定合法标识。
HMAC key只在实验准备阶段经已有绑定加密GATT的新增诊断命令安装（复用特征表）或经物理受控预置；
**不能通过明文 /diag HTTP 发送新密钥**。准备使用GATT不算恢复使用GATT，统计起点为准备完成并关掉该连接之后。
若首批只实现RF原型，明确保留上述认证测试为未执行，不用公开常量密钥假扮认证。

**时间误差界，无需相信Windows发射时间：**

1. 因果关系为设备d0请求发CHALLENGE → PC首收b1 → PC发OFFER → 设备接收d2；整段包括队列、重复广告、OS调度。
2. 设测得R=d2-d0，则设备d0在PC时间上的映射落在 `[b1-R, b1]`（加量化与时钟速率误差）；
   因此设备未来anchor `d0+D` 对应 `[b1-R+D, b1+D]`，PC可取中点作预测，半宽R/2为保守误差。
3. 初始候选 `Rmax=2.5s, guard=2s`；接受前还要留出10ms量化、65s漂移和实测调度余量。
   若余量不足就拒绝本轮、保留原恢复状态，不能为了“通过”把截止无限延长。
4. ACK漏收时PC没有R，先按预配置Rmax覆盖整个anchor区间并继续监听，不能“不见ACK就不发新epoch”。
   设备到未来anchor激活后按新epoch扫，收到NORMAL才回新epoch状态；Bridge据此记 `aligned_observed`。
   只有ACK时标为 `schedule_accepted`，只有AVAILABLE/CHALLENGE时标为 `discovered`，统计分开。
5. 新排期首两窗均无有效NORMAL，则设备继续纯广播恢复；不回滚退役epoch、不悄悄转GATT。
   新一轮nonce/epoch重试必须遵守持久高水位和写入速率上限；实验先限制每小时最多6次成功配置安装，
   射频测时可反复运行但不每次写Flash。掉电/RTC失效废止旧单调anchor/pending激活点，不能重新从开机计算旧D；
   持久绑定、权限与epoch高水位完整时可重新进行本协议挑战，安全记录不完整才需要另行配置恢复，详见 §9.7。

这校准的是会合单调时间相位和白名单周期，**不是墙钟日期/时区/长期振荡器频偏估计**。
观察多轮误差才能估计速率；一次RTT只给当次offset区间。Bridge长期离线期间的累计偏移不影响发现，
但若设备/Bridge丢失密钥、配置摘要/epoch安全记录，或需换endpoint/claim/Bundle/正式PowerPlan，
24B协议不足以重建那些事实，须另行设计更丰富的认证通道。应报 `needs_provisioning/config_repair`，不可冒称本方案已修复。
owner有效且为其他Bridge时拒绝安装；owner空闲/过期的最保守原型仅测RF、不改持久排期、不claim，
将“已有绑定是否可仅校准无线时序”单列后续授权合同评审。这样不借广播绕过当前owner规则。

### 9.4 共用状态机与最小原型任务

```text
设备 NORMAL_SCAN --连续2窗无有效指令--> RECOVERY
RECOVERY A/C: bounded scan AVAILABLE → challenge TX → offer RX
RECOVERY B: challenge TX → offer RX
offer缺失/无效/超RTT/owner冲突 → 记录原因 → radio off → deep → 下轮同候选
offer有效 → 原子pending+ACK TX → radio off → deep → 未来anchor
未来anchor → NORMAL_SCAN(new epoch) → NORMAL reply → 后续同周期
两窗失败 → RECOVERY(仍是A/B/C，保留安全高水位)

PC A: long AVAILABLE TX + Watcher → challenge → persist candidate → OFFER → ACK/未来NORMAL
PC B: Watcher → challenge → persist candidate → OFFER → ACK/未来NORMAL
PC C: periodic AVAILABLE TX + Watcher → challenge → 同上
ACK超时 → keep unknown candidate / cover candidate future window + recovery Watcher
PC进程再退出 → 重启走不依赖旧anchor的A/B/C发现，旧candidate只作对账记录
```

最小原型P0（测RF/时间，不作认证完成结论）：

1. 独立Rust example `advert-realign`，复用adv-spike API，参数 `--strategy a|b|c --run-id --period-s --scan-ms --rounds`；
   只发测试帧、接收Note4测试帧、JSONL记录。不得加载生产state、device-token或coordinator，也不监听8765/8766/8767。
2. Note4增加token鉴权实验入口及有界run/stop/status；只在显式start后进入实验。
   第一批可RAM循环模拟扫描相位，用于排除无线API问题；第二批必须走真实timer/deep并RTC保留run_id/cycle/phase/trace，
   正常固件默认路径不变。实验状态自动在限定轮数/到期后退出，物理BOOT可中止；不要增加GATT特征表。
3. 复用NimBLE的scan API，但实验scan用passive模式、Wi-Fi关闭，并补non-connectable advertiser/收发切换与有界回调队列。
   扫描/广播/回调全部受同一轮硬截止；结束恢复原生命周期，不能留下stack或Wi-Fi电源锁。
4. trace用RTC/固定RAM环形缓冲，结束时一次性取出；不要每分钟写Flash，也不要测试中为取日志打开Wi-Fi或串口。
5. 常驻Publisher+Watcher是首选测法；若适配器并发不可靠，另开明确命名的半双工试验并加长设备有限预算，
   不能直接沿用并发成功率。先用2.5s重复challenge跨PC RX时段、再有界offer RX，测总时长后定值，不能声称旧6s必够。

P1（认证排期原型）：在P0上加本节帧HMAC、准备期安全配置命令、epoch/pending原子存储、RTT过滤、未来激活和丢ACK对账。
两个版本的报告分开，P0不能替代P1的安全/掉电验收。反向HTTP、数据发布、业务ACK不属于本轮最小实现。

### 9.5 Luna 实机矩阵、指标与失败判据

| 测试 | 注入/样本 | 必须看到的证据 | 失败或结论限制 |
|---|---|---|---|
| R0 无改ROM接收检查 | 现有adv-spike + Note4现有token `/diag?blescan=12&company=65535`；3轮 | Note4 raw帧、长度、non-connectable、RSSI、首包时间；PC状态变化 | 只算Note4单向RF能力；Wi-Fi开着/active scan/无deep，不能算任何候选恢复 |
| R1 收发骨架 | P0 A/B/C各30轮，统一距离、供电、周期、窗口预算；交错顺序ABC/BCA/CAB | 每轮run_id、真实Note4 TX/RX、PC RX、总radio_on、失败分母 | 最后一轮也须结束并有设备trace；PC自收/Started不算设备收到 |
| R2 人工错相位 | 对0/1/5/15/30/59s相位，各候选每档5次；调整实验anchor，不改系统/显示墙钟 | discovered/offer/ACK/首NORMAL分段时间；恢复轮数 | 一次命中不等于稳定；各档样本小，报告原始比例，不能宣称通用95% |
| R3 Bridge离线恢复 | harness离线2轮、10轮、30min；重启随机相位；每候选≥3次 | 离线期间设备每轮无线预算不增长；重启后恢复且后续10窗命中 | 暂停harness ≠ 停生产Bridge；24h实际离线必须另测，不能用注入代替 |
| R4 漂移 | 只在实验RTC映射注入±1000ppm、±5000ppm及±30s offset；恢复后30窗 | 每窗预测区间/实际RX残差、失配再恢复、deadline不变 | 注入证明算法容差；不能当Note4物理RTC漂移测量 |
| R5 丢包/进程退出 | 分别丢AVAILABLE、CHALLENGE、OFFER、ACK、首NORMAL/前两NORMAL；PC在OFFER后退出 | 不续窗口；ACK丢时PC覆盖候选未来区间；两NORMAL丢重进纯广播恢复 | 只记录discovered不能报aligned；旧epoch不得复活 |
| R6 认证/隔离 | P1错MAC/short碰撞/错key/旧nonce/旧epoch/改period/反射/重放/超Rmax/owner冲突 | 均拒绝改变排期；坏包不延长radio deadline；无业务状态改变 | P0不执行这一行，不得用run_id过滤冒充认证 |
| R7 实际deep与掉电 | P1每方案≥30个真实deep周期，另断电/冷启动至少1次 | RTC trace跨deep、pending/epoch检查点、冷启动拒绝旧anchor | 打开串口造成复位必须单列，不能混入漂移/丢包样本 |

R3/R5/R7的新增具体场景展开为 §9.7 的S1–S11；这些用例共用同一批证据，避免把同一次成功重复计入样本量。

记录字段：run/variant/cycle、fw/ROM SHA、MAC、adapter/driver、供电/电量、distance/RSSI、PC monotonic各事件、
设备单调wake/TX/RX/off、epoch/nonce摘要/seq、R/误差区间、失败原因、累计RX/TX/awake/render时长。
密钥、token、完整敏感正文不入trace；从两端原始时间计算跨时钟差必须使用上述映射，不能直接相减UTC日志。

预注册验收门槛：安全/截止不变量零违反；A/B在PC可用后候选180s内、C候选360s内看到新日程并连续10窗稳定；
各候选首批≥30次完整恢复试验报告n/N、P50/P95/最大值和超时样本，不能删除失败后只报成功分位数。
95%仅产品候选目标；30次全成功仍不足以证明普遍成功率≥95%，不以小样本过度外推。
方案比较同时给每离线小时的设备累计radio_on、每次成功恢复的RX/TX时间、PC广播占用；没有电流仪只报告这些代理指标，
不能把电量百分比/USB电流当电池续航结论。更换扫描时长后应重新对照同预算基线。

### 9.6 生产隔离、回滚与执行入口

阶段1只读与本文追加已完成；本轮无源码改动、无构建/刷写、无设备请求、无Bridge重启、无新目录。
后续执行顺序如下，涉及新目录的创建和首次生成仍按AGENTS提权：

1. 非受限只读核对 `Get-PnpDevice -Class Bluetooth -PresentOnly`、驱动版本、`Get-ScheduledTask -TaskName CodexStatusBridge`、
   Bridge exe/主进程/watchdog/8765/8766；重新从state读取Note4身份和当前快照。只看文件名/所需字段，不输出凭据。
2. 先存当前Note4 ROM副本、大小、SHA256及marker，确认回滚ROM为Note4；保存生产data全量私有备份与Note4实验前状态摘要。
   不删除或复制覆盖正在写的生产state；要一致备份，按现有流程短停Bridge再备份，并记录停启对其他设备的影响。
3. 独立harness使用专有run_id和测试帧类型，不连接任一GATT、不claim、不调用生产API，不读生产运行数据。
   设备实验必须显式仅选择MAC `7C4FADB93408`，试验BLE时不发生产service广播，防后台Bridge自动连接干扰。
   若实际radio API仍与生产Bridge冲突，再按授权流程停止**该路径**默认实例；不能未经测量默认停全部蓝牙/其他Bridge。
4. 原型仅构建Note4：`pwsh -File tools/pio-target.ps1 -Target note4`；逐次记录新版本/marker/大小/SHA至PROGRESS。
   OTA安装由主执行者按当前按MAC队列流程处理，确认同MAC认证fw后才让Luna开始；保存/发布模板不属于本试验。
5. 现成PC射频工具命令（会广播，并非本阶段只读命令）：在bridge目录，
   `cargo run --target-dir ../artifacts/cargo-target-powerc -p bridge-ble --example adv-spike -- --seconds 20 --rounds 3`。
   目标目录不存在时先提权预建或提权运行构建，保留已有缓存；设备R0需合法操作token，调用诊断前确认足够的授权在线期限。
6. 待实现的新入口约定（以下**目前不可执行**，实现者若调整必须更新实验清单）：
   `advert-realign --strategy a --run-id <id> --period-s 60 --scan-ms 1500 --rounds 30`；b/c同理。
   Note4 `POST /diag?advert_test=start|stop` 和 `GET /advert_test/status` 为建议接口，start配置应在有界body中传，
   按现有token权限保护；不得把现有 `blescan` 当作该接口已经存在。P0数据读取标清只读，start/stop明确有副作用。
7. 结束时先stop harness、等待其Publisher/Watcher真正停止；退出设备实验、收trace、核对同MAC fw/context/Profile/data/owner/PowerPlan。
   保留的实验固件必须默认关闭试验；需要回滚时只刷已备份的Note4 ROM，不擦NVS/LittleFS、不解除配对。
   若曾停Bridge，按watchdog→任务流程核查后用 `pwsh -File tools/start-bridge.ps1` 恢复；验证任务/PID/8765/8766与两设备记录。

风险主要是临时失联、无线开机增加、实验状态跨deep错误、RTC/Flash检查点错误和Windows收发竞争。
用运行轮数/总时限上限、物理BOOT中止、已核对的Note4回滚ROM、固定缓冲和禁止业务副作用控制。
在缺少可用OTA窗口或USB回滚路径时不开始无法自行退出的试验；不要为了取日志复位正在测的相位现场。

### 9.7 三类长期中断、覆盖迁移与新增实机矩阵（用户补充）

本节细化“长期不在线”的原因。长时间没有包不能区分关机、距离、射频干扰、PC暂停与日程错位；
Bridge保留上次成功观察及其时间，分别报告 `contact_unconfirmed`、`radio_unavailable`、`realigning`、
`aligned_observed` 或有明确证据的 `config_repair_required`。未收到包不推导永久离线、不删除绑定、不清空电量或待办。
设备按本地窗口计数触发恢复，不能从某个超时推导Bridge永久消失。电量保护仍优先，因保护跳过无线单独计数。

#### 9.7.1 通用恢复分流与统一预算

| 可验证的状态 | 分流 | 限制 |
|---|---|---|
| 双方时基连续、配置/epoch/序号可信且原guard仍覆盖误差，设备尚未锁存恢复 | 正常窗口直接接受新鲜NORMAL，保持原epoch/相位 | 第一份有效NORMAL即可恢复联系观察；仍需后续10窗观察稳定性，不为单次漏包分配新epoch |
| 任一旧anchor不可延续、连续2窗无有效指令、已锁存恢复或误差超guard；绑定/安全记录/所需权限完整 | 按所选A/B/C进行纯广播挑战和未来排期 | 即使旧NORMAL后来可听见，也不能跳过已锁存恢复的nonce/新epoch握手；不自动GATT/Wi-Fi |
| 仅业务endpoint不可达、AP变更或owner需处理 | 无线相位恢复与业务阻塞分别记录 | BLE能对齐不等于LAN/业务已恢复；广播不能更新凭据、endpoint、owner或正式Plan |
| 密钥/绑定摘要/安全高水位缺失或矛盾，或有效他人owner | 停止排期安装，给出明确原因 | 仅绑定短ID、MAC后缀或“听起来像旧设备”的包不可信；不降级明文配置，不自动配对/claim |

全部候选使用同一设备单轮**6s总硬截止**（从本轮首次启用BLE的请求时刻算，包含初始化、scan、TX、offer、ACK和关无线）。
关无线预算预留250ms作为试验起点，距截止不足以完成下个阶段就跳过该阶段；实际关栈超界本身计预算失败，不能隐藏。
A/C发现scan=1.5s，B的challenge TX=0.8s和offer RX=2.5s都只是总预算中的子上限；共同RTT过滤仍为2.5s。
迟到/重复/坏包、适配器恢复、业务待办都不重新开始6s计时。连续无Bridge时A/B仍60s、C仍62s恢复一轮，
不因离线小时数增加扫描时长，也不因刚回到覆盖就连续唤醒加速。若另测渐进或半双工，三候选统一新的总预算并另列数据集。

PC按130s观察批次记结果，批次结束可继续下一批；不由该超时关闭永久恢复入口。
Watcher/Publisher失败时取消本代所有异步任务、停止旧对象，再按1/2/4/8/30s（封顶）重建；没有无线能力时不反复开忙循环。
同一绑定最多一条校准事务/一条冻结OFFER，队列有界；超时不累积无限任务。PC重新可用后的恢复计时起点是
Watcher和所需Publisher已可用，另报告从用户恢复PC到无线可用的系统延迟；不能把OS恢复时间从总体验数据中消失。
PC旧回调必须携带本地radio/session generation，适配器重置或系统恢复后丢弃旧代回调，避免它们假装收到新回复。

#### 9.7.2 Bridge长期不在线后回来

- **仅短暂进程暂停，映射可证明连续：** 可保留原日程，但同时提供所选恢复发现，避免设备已锁存恢复而PC只发NORMAL。
  保存过单调anchor不等于证明跨进程连续；最小原型对真正进程重启一律标 `needs_recalibration`。
- **进程退出后重启，磁盘绑定/密钥/epoch记录完整：** 不依赖旧anchor，直接启用A长广播、B监听或C周期广播。
  设备可能已离线多轮，也可能还在正常窗口；PC同时公平安排原已知NORMAL过渡窗与恢复入口，不能使恢复RX被Publisher完全饿死。
  启动恢复不等于授权新发布，原待办仍按原业务确认规则保留。
- **OS睡眠/唤醒、适配器重置：** 无论进程PID是否相同，废止旧时钟映射的可信标志和无线对象代次。
  磁盘候选保留作对账，旧OFFER不能在新会话按“现在+65s”重发；重新产生事务并走A/B/C。
- **仅易失anchor/测量缓存丢失：** 可以纯广播重校；**密钥、绑定白名单摘要或epoch安全账本丢失/回退：**
  本最小协议不能凭一份可能重放的CHALLENGE重建安全历史。只记录配置缺失，不创建新key、不重置epoch为0。
  密钥仍在但高水位丢失也属于 `config_repair_required`，不能归类为“只丢相位”。恢复备份或更丰富认证修复是独立步骤。
- **owner因长期离线到期：** 按 §9.3 的保守权限门槛可继续P0/RF发现，但P1不能安装排期，报 `owner_repair_required`。
  这不是A/B/C射频失败，也不证明广播能恢复生产业务；若要允许绑定Bridge在owner空闲时仅改会合日程，必须先单独评审合同。

#### 9.7.3 设备长期不在线后回来

- **深睡且RTC连续：** 区分“按原日程睡眠”与“跳过多轮无线后醒来”。RTC校验、epoch及误差预算仍满足且未锁存恢复时，
  可以直接尝试正常窗；跳过轮数不能把旧window_seq归零，需按原anchor计算当前序号。误差已超guard时直接进入所选纯广播恢复。
  测试休眠期间保存的相位，不改墙钟或既有PowerPlan；实际经过时间照样消耗原期限。
- **断电/冷启动/RTC失效，但持久记录完整：** 废止旧anchor、旧nonce及pending的单调激活点，保留已预留epoch高水位，
  先用新硬件随机nonce和当前单调d0走A/B/C，分配高于已预留值的新epoch。
  不需要因为“冷启动”就必然接GATT；但必须先证明持久key/绑定/权限记录足够。若冷启动导致owner有效性无法证明，
  按权限阻塞处理，不利用BOOT provisional或广告创造owner。普通物理BOOT的既有300s规则不被实验另起一次或延长。
- **PC此时在线：** 由该候选的持续发现入口接回，测冷启动到BLE可用、BLE可用到对齐两个阶段；
  **PC此时离线：** 设备每轮仍按6s/60或62s预算，保持显示和凭据，PC之后回来从 §9.7.2 收敛。
  不因为设备已经冷启动一次，就无限保持扫描或Wi-Fi等待PC。
- **设备持久配置损坏/工厂清除：** 拒绝旧控制广告；广播校准不承担新设备配对、密钥/owner/网络配置恢复。
  真正冷启动试验不得擦生产NVS/LittleFS来模拟这个分支，只在实验状态副本或明确的诊断注入中置“配置不可用”。

当前P0若只把试验状态放RAM/RTC，断电后自动退出是正确的退出行为，**不是冷启动恢复通过**。
要测真实冷启动P1，必须新增显式预先授权的、仅限测试run_id和有限重启次数的持久试验配置，
启动即消费一次次数并保持硬预算；试验结束清除该配置。不得为了让试验跨重启就默认永久开启诊断模式。

#### 9.7.4 移出无线覆盖、边缘信号和新位置/AP

- 离开期间只记未确认；设备在连续两窗缺失后锁存所选恢复，正常深睡。回来时若仅漏一窗且原误差/epoch有效，
  正常NORMAL即可接回；已锁存恢复则即使原epoch和window本来仍可计算，也必须完成纯广播重校，不在两种状态间抖动。
- 同一LAN并不保证BLE可达，BLE可达也不保证LAN相同。设备移到另一个AP而已保存的Wi-Fi配置仍有效、Bridge endpoint仍可达时，
  无线相位可以按A/B/C恢复；HTTP交付要在其独立、有界且正式授权的阶段验证，不计入本轮纯广播成功分母。
- 新AP/新网段需要新凭据、隔离客户端或旧Bridge地址不可达时，保留 `aligned_observed, business=network_unverified/blocked`；
  24B广播不传Wi-Fi密码/IP/endpoint/token，不信任广告推荐的新URL。网络修复须另行认证配置，不能自动mDNS/GATT兜底。
- 用户澄清：移出覆盖再返回是必须考虑的协议场景，本轮通过可控故障注入测试，不要求用户搬动设备。
  注入只证明逻辑收敛和预算；不证明真实距离、人体遮挡、多径、天线方向或边缘位置的射频性能。
- RSSI只作为观测，不能作为身份凭证或“已回来”的单一判据；没有收到包时RSSI是缺测，不是负无穷或旧RSSI仍有效。
  边缘位置即使偶尔收到AVAILABLE也可能丢OFFER/ACK，仍按同一6s截止收尾，下轮继续；记录分阶段成功率，
  不因RSSI下降自动增大发射功率/扫描时长而破坏A/B/C公平条件。故障注入解除后独立测恢复延迟。

#### 9.7.5 Luna新增注入清单与可证明范围

以下操作必须在已部署且验证自动退出的原型上执行；harness进程控制和覆盖丢包模型可自动化，不需要用户配合移动设备。
真正断电/系统睡眠测试若没有操作条件则标“未执行”，可以先做对应逻辑注入，但不能将其写成真实断电/系统恢复结论。
生产Bridge/data、Windows适配器和Wi-Fi配置不作破坏性注入；涉及整机睡眠/适配器重置应由主执行者协调其他正在运行的任务。

| ID / 场景 | 可执行注入与步骤 | 范围与判据 |
|---|---|---|
| S1 Bridge退出/保留配置 | 每候选建立10窗基线；只终止独立harness；分别离线2轮、10轮、30min；从同一实验配置重启，使用预选相位0/15/30/59s | P0测真实收发/时序，P1测安全epoch和实际排期；设备trace证明离线每轮≤6s。重启须不依赖旧anchor；恢复后10窗稳定。24h仅单独长测，30min不冒充长期漂移结论 |
| S2 Bridge系统睡眠 | harness运行中记录最后单调/状态；现场使PC睡眠，至少跨2个设备恢复周期后唤醒；另做保持PID的radio对象stop/recreate | 真实OS睡眠和软件对象重建分别标记；P0可测API重建与无线重新发现，P1检查旧回调/旧候选不激活。若只做stop/recreate，不声称适配器硬件重置或OS恢复通过 |
| S3 适配器真实重置 | 先核对适配器实例；现场关闭/开启蓝牙或重插该测试适配器，记录驱动/错误；设备不断电 | P0/P1均可测收发恢复；只测本机适配器，不外推其他硬件。旧radio generation结果必须丢弃；重建退避有界。共享适配器影响其他设备需记录，不静默批量禁用 |
| S4 Bridge配置缺失 | 在隔离实验配置副本分别注入：仅anchor缓存空、缺key、缺绑定摘要、缺epoch账本、旧账本；不删除生产文件 | P0仅能验证错误分类/停止行为；P1验证后四项拒绝安装/拒绝旧包，第一项可广播对齐。P0的run_id不是key，不能算凭据缺失安全测试 |
| S5 设备RTC连续长睡 | 试验start时预设在指定cycle跳过无线2/10轮及30min（实际deep，保留RTC），到点恢复；PC分别保持在线和离线后再上线 | 要求有真实deep能力；RAM循环仅测状态机。记录RTC连续、wake reason、跳过窗口序号不倒退、旧期限自然减少。guard内未锁存可直接恢复，超guard走A/B/C |
| S6 设备真正冷启动 | P1预先装有限次数的跨冷启动试验配置；现场完全切断设备供电后恢复；PC在线/离线各一组 | 拔USB但电池仍供电不算断电；以reset reason和RTC失效证据核实。普通软件reset另列。旧anchor/pending不得重新起算，epoch高水位保留；权限不满足应阻塞而非强行对齐 |
| S7 两端交错回归 | 两端均不在线；先启动设备、至少等2轮后启动harness；再反向先harness后设备；分别用RTC连续和真实冷启动 | P0只能做RF/模拟状态组合；真实deep/P1才证明持久状态交错。设备未等到PC时不能连续扫描；PC不能等设备先发生产GATT广播；每候选各顺序≥3次 |
| S8 覆盖中断后返回（注入） | PC/设备都固定不动；预置共同故障时段：PC暂停该run全部下行广播并丢弃上行，在恰1正常窗与≥3窗两档后解除；设备仍正常按预算运行；另测仅上行丢失、仅下行丢失 | P0可测实际Note4收发骨架对广播中断的恢复，P1+真实deep验证短缺失可直接恢复/持续缺失锁存恢复；两端trace确认受影响窗数，解除注入即起恢复计时。不是实际移出覆盖或距离测量 |
| S9 边缘丢包模型（注入） | 固定设备，用同一预生成序列对上下行分别注入50%丢包、连续3窗全丢后1窗放行、持续10轮后全恢复；可另给测试事件附带合成弱RSSI，真实RSSI保留单独字段 | P0测分阶段丢包/重试/成本；P1检查nonce/epoch及预算；合成RSSI不改变真实射频，不能算边缘接收能力。包级丢弃发生在标记的测试接收边界，区分控制器收到后被注入丢弃与真实未收包 |
| S10 同LAN/新AP状态（注入） | 在实验网络状态副本分别置“旧endpoint可用”“已入新AP但旧endpoint不可达”“无可用Wi-Fi凭据”；叠加S8广播中断/恢复，不修改生产Wi-Fi配置或实际AP | P0/P1可验证广播恢复不以网络可用为前提、网络问题分类独立；当前纯广播原型不执行反向HTTP，业务结果标network_unverified。新AP实际关联、DHCP、路由/HTTP验证需后续实现和独立网络测试，不算本轮通过项 |
| S11 owner/安全记录长期失效 | 仅实验状态副本注入owner过期、他人owner、设备配置不可用；与S1/S5/S6组合，不force claim不清生产NVS | P0只测分类，P1必须拒绝排期安装且保留显示/业务状态；报权限/配置阻塞，不把射频命中记为完整恢复成功 |

公平执行：同一Note4、ROM、测试key代次、固定摆放、供电、无线总预算、相位表和中断持续时间；候选顺序ABC/BCA/CAB轮换。
覆盖注入使用相同时间表/随机种子与明确的上下行丢弃位置；合成RSSI、注入丢包和实际射频观测分别记录，避免混为物理测量。
S1/S5/S7/S8每个选定组合各候选至少3次先做筛查，后续只对仍有不确定度的项增加样本；小样本如实报告。
初版C固定62s，A/B固定60s，周期差本身是策略成本，报告实际恢复轮数与墙钟延迟，不能只比较“第几轮”。
安全写入每小时6次限制照常执行，P1样本分时安排；因限速待下一窗口单列 `install_rate_limited`，不为了赶测试关闭保护。
测试run的总时限必须预先覆盖离线段+恢复观察段，例如30min离线的用例可设60min硬上限；新包不能重置这个总时限。

各用例最终结果分为：`direct_resume`、`broadcast_realigned`、`discovered_only`、`permission/config_blocked`、
`timeout`、`not_executed`；前两项再分别给后续10窗稳定数。P0一律带 `auth=none`，不进入P1成功率分母。
从现场PC/设备/覆盖真正恢复的时刻计算总延迟；另列radio初始化、发现、挑战、未来激活等待、稳定观察耗时。
A/B 180s、C 360s是候选的“首次新日程证据”目标，后续10窗另计；有权限/配置阻塞时不适用这个无线目标。
以上是新增设计与待执行矩阵，未产生任何新的实测结果。

### 9.8 Note4 + Windows RF P0 实机筛查（2026-09-27）

本节是 §9.7 的**有限子集**实测，修正上一句的时间范围：该句描述撰写 §9.7 时的状态。设备为登记 MAC `7C4FADB93408`，实验固件 `0.18.26-note4-b-rf0/rf1`；PC 独立 `bridge/crates/ble/examples/advert-realign.rs`，每次 `--seconds 10 --period-ms 10000 --on-ms 2000`，设备经 token 保护的 `/diag?realign=a|b|c&run_id=N` 显式运行。A持续发 AVAILABLE，B只监听设备 CHALLENGE，C每10s发2s AVAILABLE。所有测试包使用隔离Company ID `0xFFFF`，按run_id匹配，无密钥或业务副作用。Note4诊断时经HTTP触发、Wi-Fi仍在线，**不是实际deep/自动定时唤醒**；PC Started只是API状态，成功必须同时有设备OFFER命中与PC challenge记录。

| ROM / 方案 | 双端完成OFFER | Note4无线阶段与观察 | 结论 |
|---|---:|---|---|
| rf0 A | 1/3 | 成功轮约4.84s；其余两轮1.5s内未听到AVAILABLE | 可工作但当前短扫描命中不足；PC连续Publisher占用最长 |
| rf0 B | 3/3 | 成功轮约3.33s，OFFER首包39/477/1127ms | 本批最稳定；设备每轮先发0.8s再扫，PC需持续Watcher |
| rf0 C | 0/3 | 两轮没听到AVAILABLE；一轮发现后PC未见CHALLENGE | 周期TX与双向接收竞争，短窗脆弱 |
| rf1 A（命中即停扫） | 1/3 | 成功轮约1.15s；两轮仍未听到AVAILABLE | 优化缩短成功轮无线时间，没有改善本批发现率 |
| rf1 B（命中即停扫） | 3/3 | 约1.29/1.88/1.45s，OFFER首包464/1051/625ms | 作为下一阶段认证原型首选 |
| rf1 C（命中即停扫） | 0/3 | 一轮107ms听到AVAILABLE，却仍未收到OFFER；PC未见CHALLENGE | 早停不足以解决；需重新设计同适配器半双工时序 |

两版共18轮A/B/C基线全部在设备6s硬截止内结束。C另以1s、5s相位延迟各试一轮，均未听到AVAILABLE；这只是两档短样本，不能据此计算覆盖概率。B再做独立harness停发一轮，设备OFFER缺失且约3.33s收尾；下一轮恢复harness后设备收到OFFER（首包1560ms，约2.38s收尾）。这模拟Bridge广播/扫描暂时不可用后重新发现，**没有实际停止生产Bridge、移动设备或验证真实长期离线/RTC漂移**。原始双端记录在 ignored `artifacts/rf0-*`、`artifacts/rf1-*`，失败轮保留在分母。

推荐先以B进入P1认证/epoch/未来窗口排期原型；A保留为PC可可靠长广播且设备扫描预算可增加时的备选，C在建立明确TX/RX交替与相位覆盖证据前不进入生产。3/3只是可行性，不能宣称成功率、功耗优劣或对其他适配器的可移植性。尤其B的常驻Windows Watcher与生产Bridge的BLE会合并发仍需观察；设备TX/RX耗时只是无线开启时间代理，没有电流仪测量。真正重对齐需实施 §9.3 的认证nonce、误差界、持久epoch/pending、ACK丢失收敛、未来anchor与后续正常窗口；冷启动、真实deep、owner/key缺失、新AP和覆盖移动按 §9.7 保留为待测，不把P0的OFFER当 `broadcast_realigned`。

### 9.9 正常bridge_first时间窗口的现有模型审计与几何测试

`tools/fake-rom-runner.mjs` 的24h回放与 `device-sim` 支持Bridge/设备独立时钟速率及timer wake，但每次窗口调用的是 `bridgeRun(..., 'ble')`，即现有device_first GATT会合；它没有Bridge Publisher、设备扫描起止、空口帧到达或两个窗口重叠判定。§9.8 的RF P0则由HTTP触发醒着的Note4，PC连续/周期广播，没有生产的60s锚点、RTC deep唤醒误差。因此两者都不能给出bridge_first预定窗口的实机交会精度。

为把可计算的边界与无线成功分开，`tools/test-bridge-first-window.py` 固定示例参数：周期60s、设备从预测点起扫描1.5s、PC从预测点前1s到后1.5s广播、把至少0.5s重叠作为**敏感度测试阈值**，并扫相位误差及单调钟速率。0.5s不是实测“足够收到一包”的门槛，PC真实广播间隔与OS延迟未进入模型。

| 设备相对Bridge开始误差 | 几何重叠 |
|---:|---:|
| -2.5s / +1.5s | 0 |
| -2s / +1s | 0.5s |
| -1s 至 0 | 1.5s |
| +0.5s | 1s |

在初始相位正确、持续不重校的示例下，正向+20/+100/+1000ppm分别约13.9h/2.78h/17min后首次小于0.5s重叠；每10个60s窗口认证校准一次时，+100/+1000ppm累计误差约60/600ms，分别仍有1.44/0.9s几何重叠。此为所选窗口和理想线性时钟的计算，不是Note4实测漂移；当前Note4修复后的24h物理RTC误差还没有新测量。Bridge重启丢失anchor、设备冷启动或搬出覆盖导致相位可偏移几十秒时，正常短窗几乎没有保证，须进入§9.7的独立恢复入口。

即使几何完全重叠，也不能推出收包：§9.8 的A在PC连续Publisher、设备1.5s扫描的两版实机各仅1/3收到完整双向广播，说明当前无线/OS条件下该短预算还没有稳定证据。正式定值须实现正常bridge_first预定窗口，并同时记录Bridge实际Publisher Started/Stopped、设备真实timer wake/扫描起止、首个认证Directive的设备单调时间、RTC残差和后续回复；至少跨多次deep与数小时、覆盖正负相位、Bridge/设备重启及失败轮，才能报告实机命中率和P95交会误差。

补充当前`0.18.25-note4-b`诊断测量：Note4核对MAC后接受正式light Plan 633，Windows独立测试Publisher连续运行90s且报告Started；设备经已授权HTTP依次执行12次1s、12次2s `/diag?blescan`，Company ID `0xFFFF`。1s命中8/12，2s命中9/12；命中轮首次匹配分别115–735ms及24–1915ms。原始逐轮记录 `artifacts/window-normal-note4.jsonl`，PC状态 `artifacts/window-normal-publisher.jsonl`（均ignored）；测试后正式sleep Plan 634 ACK，剩余0。此诊断使用**active scan、Wi-Fi light、HTTP触发**，并且PC连续广播；未使用bridge_first的预定PC短窗口或设备deep timer。因此它只测“已重叠时短扫描能否收到测试包”，不能报告正常bridge_first的窗口时间误差或命中率；1s/2s各12轮也不足以精确估计可靠性。
