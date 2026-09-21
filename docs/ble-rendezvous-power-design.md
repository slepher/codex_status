# 每分钟 BLE 会合与大黑块刷新：目标设计

状态：2026-09-21，设计稿，**尚未实现、尚未测量目标方案功耗**。本次只新增设计文档，不改变现场固件、桥、GATT 表或既有协议。实现计划见 [plan](../project-workflow/ble-rendezvous-power/plan.md) 与 [task-1](../project-workflow/ble-rendezvous-power/task-1.md)。

## 1. 目标、范围与非目标

电池 deep 模式仍每分钟由 RTC 唤醒，先更新分钟时钟，再开启短 BLE 会合。桥选择无操作、直接 BLE 小数据更新、或显式租用 Wi-Fi light 会话。默认周期不启动 Wi-Fi。BOOT 唤醒默认给予 300 秒 Wi-Fi light 时间，其后仅显式续租或提前结束控制寿命。

大黑底用量象限和大数字必须按语义区域、黑白转换方向、面板历史选择刷新方式；无线省电不能以明显残影和发灰为代价。

不在本里程碑实现代码、修改面板 LUT、改换控制器、部署固件或改变模板保存/推送授权。不是实时远程唤醒：deep 内无线关闭，桥只能等待下一次会合；一次机会的等待最长约 60 秒，加上启动、发现、传输时间。丢失机会可令实际延迟超过一个周期，不承诺 Windows 上 100% 发现。

## 2. 已实现事实与设计假设

事实以 [PROGRESS 最新节](../PROGRESS.md)、[power-state §13](power-state.md#13-v014-方案deeplight-双模式--时钟区域直写0140-起已实现) 及代码为依据；power-state 前半部保留了旧版行为，不能把其“无周期扫描”“只在断网 deep”当成目标限制。

| 项目 | 当前已实现/记录 | 本文目标 |
|---|---|---|
| 现场版本 | PROGRESS 为 0.15.0-bw；AGENTS 概览版本较旧 | 后续版本能力协商启用 |
| deep 接触 | 桥可达时每 60 秒 Wi-Fi GET usage；失败才 1m×3/5m×3/15m 退避 | 每 60 秒 BLE，会合无响应照常下一分钟 |
| light 控制 | idle 默认 600s；桥 activity 最短驻留 300s、安静 600s；pending 窗口 180s | 独立且显式的 light lease |
| BLE | `ble_bridge.*` 已有 info/endpoint/usage/status/template-control/template-data/auth；写入要求 encrypted + bonded | 复用现有 UUID/属性，扩展版本化消息 |
| PM | 当前 BLE 控制器初始化期间持有 NO_LIGHT_SLEEP；`bleDeinit()` 调用 `NimBLEDevice::deinit(true)` | 每次结束必须 deinit，进入 Wi-Fi light 前也关闭 BLE |
| 屏幕 | `epdFlush` 相同帧跳过；变化 >5000/40000 像素即全刷；最多连续 30 次局刷后全刷；时钟阈值 90 | 语义区域独立预算，保留全帧安全阈值 |
| 旧图 | RAM `lastDisplayedFrame`；RTC 时钟窗口最多 64B；0x26 为 previous plane | 所有可局刷区域必须具备可信旧像素 |
| 恢复经验 | 0.14.13 进 deep 前强制全刷修复 Zzz/BT 基线漂移 | 未验证前保留保护；不能直接删掉 |

记录中的时钟窗口 A/B 为 795.6ms（793.9–797.0），不是注释“~300ms”；其中波形约 580ms、面板唤醒/reset 约 215ms，缩小矩形不会等比例缩短这部分。旧网络窗口约 0.03–0.04mAh/次、每分钟屏幕唤醒约 0.012–0.013mAh/次属于现有模型的粗估，不能冒充电流仪新实测。ESP32-S3 本板 BLE 初始化/广播/连接/加密/收尾电量均待测。

用户照片显示大黑块残影/发灰，这是质量需求输入；本文未对照片做定量取样，不宣称已定位唯一物理原因。工作假设是局刷波形、电荷历史及旧图基线共同影响；控制器型号 SSD1681 或 SSD2683 本身都不能保证消除残影。

## 3. 状态机、优先级与有界时序

电池路径优先级：低电保护/物理断电 > 不可延长的事务硬截止 > 已开始事务的有界安全收尾 > BOOT 新 light 会话 > 已授权电源命令 > 周期会合。普通读写、连接存在、claim 心跳都不增加电源时间。BOOT 不打开周期会合的配对入口。

```text
DEEP --RTC--> CLOCK --> BLE_ADVERTISE
                       ├─ 无指令/NOOP/超时 --> RADIO_OFF --> DEEP
                       ├─ UPDATE --> VERIFY --> APPLY/REFRESH --> ACK --> RADIO_OFF --> DEEP
                       └─ WAKE_LIGHT --> ACK --> BLE_OFF --> WIFI_CONNECT --> LIGHT
DEEP --BOOT（先清 Zzz，无 Connecting 页）--> WIFI_CONNECT（静默）--> LIGHT(300s，连上后显 Wi-Fi 图标)
LIGHT --显式 RENEW_LIGHT--> LIGHT(新截止)
LIGHT --到期/SLEEP_AFTER_ACK--> 有界收尾 --> RADIO_OFF --> DEEP
任何状态 --低电保护--> 关闭无线/安全停止写入 --> POWER_OFF
```

先执行时钟窗口再广播，避免窗口被屏幕 BUSY 吞掉。无时钟元素时略过屏幕，但仍有分钟会合。时间未知时只走相对 60 秒调度，不显示伪造时间。下一 RTC deadline 以周期锚点计算，错过的 tick 不补跑；长事务结束安排下一个未来分钟，不立刻反复唤醒。

从 deep 唤醒（BOOT/PWR 按键，或失败退避期间的定时重试唤醒）不得显示全屏 `Connecting:` 页：唤醒后立即以正常模板替换睡眠帧（Zzz 先消失），此时 Wi-Fi 尚未连上，模板按 `device.state` 条件隐藏 Wi-Fi 图标；连接成功后重渲一帧显示 Wi-Fi 图标（相对唤醒全刷基线的局刷）。进入 deep 时重绘睡眠帧：Zzz 出现、Wi-Fi 图标消失。唤醒首帧必须是全刷基线（唤醒后局刷基线不可信）；连接失败或超时先恢复睡眠帧，再按有界期限回 deep。冷启动与显式配网/诊断路径仍可显示 `Connecting:`。

下表是**初始建议默认/配置范围**，不是测量结论。硬上限不可被分片、读状态、重新连接或重复命令刷新；设备拥有最终裁决。

| 阶段 | 默认；允许配置 | 超时动作 |
|---|---|---|
| 时钟/单次显示事务 | 总 10s；5–15s，BUSY 单等待≤5s | 标记 display invalid，关屏电源，继续会合或睡眠；不循环重刷 |
| BLE 初始化 | 2s；1–3s | 关闭控制器，记失败，下分钟重试 |
| 广播可见窗口 | 1.5s；1–2s，故障/保守测试最高5s | 没连接即关闭；5s不是必需默认 |
| 连接后恢复加密/首条命令 | 3s；2–5s | 断开，不接受未绑定连接拖延 |
| BLE 单事务 | 从接受 BEGIN 起8s；3–10s；无进展2s | NACK/丢暂存/断开 |
| BLE 总 radio-on | 从初始化前起15s；10–20s | 即使仍连接也结束；不允许新事务 |
| ACK 排空 | 500ms；200–1000ms | 未确认也关闭，下次重复 revision 取 ACK |
| Wi-Fi 建连+取得IP | 15s；5–30s，计入 light lease | 失败即回 deep；不在窗口里无限重连；唤醒路径不画 `Connecting:` 页，Wi-Fi 图标仅连上后显示 |
| light 单次 lease | 300s；30–600s | 到期停止接受新工作并收尾 |
| light 模板/数据事务 | 总30s、停滞5s；总10–60s | 丢暂存；过租期最多额外10s收尾 |
| OTA | 总180s、停滞20s；总60–300s | abort/释放锁/恢复旧槽；过租期最多额外30s |
| radio 关闭/清理 | 2s；1–3s | watchdog 复位，RTC 标记下次直接 deep，避免复位循环常开 |

显示事务不能突破 radio 总截止：完成接收和原子数据提交后可先关 BLE，再完成本地屏幕操作；结果以 `display_pending` ACK 表达。若要在本轮返回已显示 ACK，应在剩余 radio 预算足够时才启动波形。显示总时限独立生效。

PC USB 插电继续采用显式 plugged 策略，不要求电池 deep；USB 自身 PM 锁仍可能阻止 light sleep。目标电池路径中，插拔线不重置已有截止；拔线若无有效 lease 则立即进入收尾。充电头不能被当作 PC USB。AP 配网/恢复出厂维持原长按语义，电池 AP 5min 期限保留。

## 4. BLE 广告与 Windows 发现

桥启动一次常驻事件订阅/被动扫描优先，不每分钟重建适配器和等待长同步扫描。若 btleplug/Windows 后端不能选择真正 passive 模式，记录实际 API/扫描模式并测量，不能声称已经被动扫描。匹配目标后立即停止本次扫描或发起连接；完成后恢复等待其他会合。多设备由单适配器调度，错过窗口按下分钟重试，不要求设备无限等待。

推荐 legacy advertising 间隔初值 100ms（可测 50–250ms）。广播只放协议版本、设备身份短索引、revision 摘要、状态位（deep会合/已有owner/能力位）；不放用量、用户名、token、Wi-Fi 凭据或更新内容。完整设备 Wi-Fi MAC 和能力在 info 校验，BLE 地址或名称不是主键，摘要不是认证依据。

当前 31B 广告装不下完整名字加128-bit UUID。推荐使用现有服务 UUID 的 Service Data：flags 3B + Service Data 的长度/类型2B、UUID16B、版本1B、设备索引2B、revision摘要4B、flags1B，共29B；完整名字可放 scan response。被动路径不得依赖 scan response。设备索引/摘要碰撞时读取 info 核实 MAC 和完整 revision，不能跳过校验。实际 AD 编码与 Windows 事件是否携带 Service Data 必须抓包验证；若后端不支持此筛选，使用既有名称主动扫描兼容模式并测功耗。

连接后广播截止不再适用，改用独立事务期限，但总 radio-on 不变。先读取 capability 并订阅 status，再发命令。无待办且 revision 摘要相同可完全不连接；有时间同步待办可用 NOOP 携带时间，不能虚增 usage revision。

会合自适应在设备上有界：默认1.5s；连续10次无已认证联系可试2s，每10个周期最多一个5s恢复窗口；重新联系成功恢复默认。此规则也会在桥关闭时浪费电，必须测量后开关控制，初版默认关闭。失败不自动开启 Wi-Fi。桥重试有抖动，同周期最多一次连接，防止竞争和耗电攻击放大。

## 5. GATT 事务与线协议 v2

### 5.1 复用与能力协商

保留全部 UUID 和属性：`...a005` template-control 承载带 `rv:2` 的控制 JSON；`...a006` template-data 承载事务分片；`...a004` status 承载 ACK/NACK；`...a001` info 增加 `rendezvous_v:2`、最大长度、类型、radio剩余时间。无 `rv` 的旧模板命令继续旧分支，不得误判新包；未协商v2时不发任何新控制。usage旧特征 `...a003` 保留兼容，不在会合中走未提供原子性的旧流式解析路径。

现有 status 是 READ|NOTIFY，属性本身未强制加密。复用方案必须在服务端对读取/通知分派检查 bonded+encrypted，未授权只返回公开摘要，不泄露 ACK 内容或密钥；必要时在连接状态重建公开值。不能把全局 characteristic value 留成敏感值供下一客户端读取。初版 ACK 不含用量/token。

推荐不增加特征。若复用无法隔离旧数据解析，可评估新 UUID/版本服务，但必须维护上述复用兼容方案，并测试 Service Changed 与 Windows 缓存；可能需要用户解除配对再配对。不能静默更换属性表后把连接失败归为射频问题。

### 5.2 消息格式

控制为 UTF-8 JSON，每个对象≤512B（设计新增上限，按MTU分片拼接；禁止多个未结束对象）。公共字段 `rv:2, op, request_id, bridge_id, session_nonce`；连接建立后设备给随机 session_nonce，命令必须匹配。`request_id` 在会话内唯一。电源命令另携设备 token，在加密链路校验且绝不日志打印。INFO 对外仅返回能力，nonce经授权status握手返回。

revision 使用 `(bridge_id, epoch, counter)`：epoch 为桥持久化的随机版本域，counter 为域内递增 uint64，JSON 用十进制字符串防止精度损失。桥重启恢复域和计数；丢失持久数据后生成新域，通过授权同步才切换。不同域不能简单按数字大小比较。广告仅放摘要；完整 tuple 和 payload CRC 通过加密 status 核对。

| 指令 | 必需负载/返回 | 语义 |
|---|---|---|
| `NOOP` | 可带发送时 `server_time,tz_offset_min`；`noop_ack` | 联系/校时，不刷usage、不续light |
| `UPDATE_BEGIN` | `revision,length,crc32,type`；`begin_ack(next_offset)` | 检查身份、owner、类型、尺寸、预算，分配唯一暂存 |
| `UPDATE_CHUNK` | 二进制头+offset+data；按需 `chunk_ack(next_offset)` | 只写暂存，不改屏幕和活动数据 |
| `UPDATE_COMMIT` | `request_id`；`update_ack/nack` | 长度/CRC/schema/业务全校验后提交 |
| `WAKE_LIGHT` | `lease_s`；`wake_ack(granted_s,lease_id)` | 仅会合状态；ACK后关BLE、开Wi-Fi |
| `RENEW_LIGHT` | `lease_id,lease_s`；`renew_ack` | 仅有效light会话，经Wi-Fi认证控制端点 |
| `SLEEP_AFTER_ACK` | `lease_id`或当前BLE事务id；`sleep_ack` | 完成本次已开始事务并排空ACK后关闭；无新工作 |

CHUNK 线格式为 `magic:u8=0xB2, request_id:u32LE, offset:u32LE, data:bytes`，单ATT写长度≤`MTU-3`，data上限为`MTU-12`；MTU=23时只有11B数据。控制JSON的 request_id 使用同一u32数值。每次只允许一个活动事务；旧模板二进制解码只在旧会话启用，不能用第一个字节猜测分流。优先 Write With Response，响应只代表GATT收包，不代表应用提交。

CRC32为IEEE反射算法（poly 0xEDB88320，init/xorout 0xFFFFFFFF），覆盖完整原始payload字节；十六进制8字符表示。严格连续offset；完全相同的重复已收分片返回原next_offset，重叠但内容不同、跳洞或越界均NACK并取消。ACK由status分片发送：头为`magic:u8=0xB3, message_id:u16LE, index:u8, count:u8`，index从0开始、count为1–255；每片≤MTU-3，余下为UTF-8 JSON字节，连接级重组上限512B、超时1s，缺片即丢弃重读，不拼接不同message_id。也可在授权status读回完整结果；桥必须解析应用层结果。

`type=usage` 为现有usage信封的完整快照，保留缺桶/RC/label显示规则，不能只发百分比而抹掉reset时间等语义。`type=time` 为有界时间/时区小数据，独立版本，不改usage revision。模板JSON、模板激活、profile、固件块均不属于usage；初版会合拒绝 `type=template|firmware`，回 `needs_wifi`，桥必须另发 WAKE_LIGHT。usage 初始上限2048B，可配置512–4096B，实际选值由MTU、分片吞吐和8s事务预算验证；不是“2KB一定能在1.5s完成”。超限或剩余预算不足在BEGIN即拒绝，不收一半才升Wi-Fi。

### 5.3 原子性、幂等与 ACK

COMMIT先验证所有字节、JSON schema、身份、owner、revision、字段范围及模板渲染可行性，构造候选usage和候选帧，再交换活动状态。失败清暂存，旧revision/屏幕不变。半包、CRC错误、断连一律不能触发局刷。一个有效revision最多一次数据提交；重复tuple必须匹配已存长度/CRC，否则 `revision_conflict`；完全重复即幂等ACK，不重刷、不续租。同域较旧counter返回`stale_revision`及当前版本；同域更新counter可跨号，允许桥合并中间更新。新epoch必须在已有授权HTTP会话内显式登记（例如`POST /power`的`SYNC_REVISION_EPOCH`控制，携token和owner校验），不能由普通BEGIN或未认证广播切换版本域。counter将溢出时也走新epoch，不回绕比较。

分别保存 `applied_revision` 和 `displayed_revision`。ACK包括 `request_id,revision,result,display_state`，状态为 `unchanged|displayed|display_pending|display_failed`。数据提交成功但面板BUSY失败时不能谎称显示成功，也不能把数据回退后让桥反复提交同一数据；记录显示修复待办，下个周期按全刷恢复。重复revision只ACK当前状态，修复由设备本地任务处理。ACK丢失时桥下一会合查询或重发；不等待无限确认。模板变化造成相同usage但不同帧，由独立template revision触发显示，不伪造usage变化。

接收、验证、提交和屏幕驱动放主任务串行执行；BLE回调只入有界队列。内存不足即NACK。禁止回调内开始Wi-Fi或阻塞面板波形。

## 6. Light lease 规则与桥决策

light lease是电源许可；owner lease是占用许可，两者字段/时钟/续租完全分离。`deadline = monotonic_now + granted_s`，不依赖校时后可跳变的wall clock。BOOT从按键唤醒开始300s，Wi-Fi连接时间计入。WAKE_LIGHT从命令接受时起计，返回grant；RENEW从接受时起重设deadline，不累计旧剩余时间。每次续租30–600s，重复request_id返回原deadline，不能重复延长；设备重启不恢复未过期light承诺，回安全启动策略。

状态读取、普通探测、连接、claim/续owner、普通数据、相同revision、template保存和activity.note_contact都不能续light。持续交互由桥明确发RENEW（例如剩余60s且确有进行中工作），完成即SLEEP_AFTER_ACK。桥可多次显式续租支持持续交互，但每次都有独立有限截止；无有效指令必回deep，禁止后台heartbeat无条件续租。

light中BLE已关闭，RENEW/SLEEP走新增认证HTTP `POST /power`（目标接口）：与BLE控制字段同义、Bearer设备token、hostId/owner校验、lease_id匹配。BOOT会话lease_id从status读取但读取不延长。HTTP ACK发送后只等待有界排空；桥通过UDP/IP更新确认Wi-Fi已上线，wake_ack仅说明接受请求，不证明已连上AP。POST /power是新增设计，不是现有工具已支持的接口。

到期时不接受新BEGIN/OTA，不允许临近到期的BEGIN“抢占”无限宽限。已接收事务使用 `min(自身总截止, lease_deadline+宽限)`；OTA停滞20s和总180s都适用。宽限内拒绝RENEW以外的新业务；达到硬截止abort、释放PM锁并睡眠。低电优先中止OTA，使用旧有效槽，不能为等待ACK继续耗尽电池。

桥选择顺序：他人owner→不写；同revision且无需校时→无连接；小usage→BLE原子更新；模板/OTA/超限usage/连续交互→WAKE_LIGHT；进入light后验证身份、必要时显式claim，再执行已授权队列。usage变化本身不再自动触发300s Wi-Fi。保留现有“保存不推送、profile显式推送且≤3启用”。队列usage合并最新revision；模板/OTA保留用户意图、错误和取消状态，不能把保存当推送。

## 7. 安全、占用与持久状态

周期会合只接受已有bond的加密连接，不开放新配对窗口；新绑定需要用户物理操作。当前无输入输出配对不能被描述为具备MITM认证；session_nonce防误重放/跨连接混用，不能替代链路安全。限制每周期一个客户端和一个事务，未认证连接也消耗总窗口，超时即关机，以免连接占坑常开。

保持显式 `POST /claim` 是唯一owner创建/转移路径，不能新增隐式“BLE claim”。已有匹配owner可BLE直传，更新仅按既有规则刷新owner last_seen，不延长light。有效owner不匹配返回 `owner_conflict`（HTTP409同义）；无owner时遵守旧安全规则。为保持桥“空闲先claim”策略，新绑定/owner过期的桥先经授权WAKE_LIGHT取得HTTP机会，再POST /claim，之后BLE会合才能享受免Wi-Fi更新。该偶发成本应计入功耗，不能暗中放宽claim规则。广播owner位只作提示，不作为授权。

全部敏感操作保留token门控；/update、/doUpdate、ArduinoOTA、POST /claim不绕过。BLE直传校验bridge_id与已绑定会话/现有owner；匿名广播不能唤醒Wi-Fi。token轮换只经已有安全交接，日志不输出token、凭据或完整usage。

| 存储 | 目标内容 | 丢失后的行为 |
|---|---|---|
| 普通RAM | BLE暂存、候选帧5000B、活动解码对象、当前lease | 丢暂存；不推断已显示 |
| RTC | 版本magic/CRC、调度、applied/displayed revision、可信旧帧或区域像素、ghost计数、历史 | CRC/版本/复位原因不符即失效 |
| NVS/现有持久存储 | 配置、bond/token、owner、模板、最后完整usage快照及revision检查点 | 冷启动恢复可渲染内容；显示必须全刷 |
| 桥data目录 | revision域/计数、待办、能力、BLE绑定映射 | 不以名称替代MAC；重新协商 |

RTC完整旧帧需5000B，加现有1440B历史和状态后必须先做链接器容量审计，不能假设一定放得下。推荐优先保留完整5000B旧帧，必要时减小历史环；若容量不足则只保留时钟及已选区域，未保留区域的usage更新强制全刷。区域摘要/CRC能判断相同，不能重建0x26像素。不能用有损摘要冒充旧图。

持久usage快照采用现有存储设施上的双记录/提交标记，绑定revision、CRC和模板hash；按真实变化检查点，分钟时钟不写flash。RTC承载正常deep间最新状态；冷启动恢复旧检查点后向桥报告实际applied revision以便重传，不把检查点以后未持久数据报告为已持久。若实现要提供跨断电“durable ACK”，必须在ACK前完成持久提交并测flash磨损；初版ACK仅承诺当前会话/有效RTC连续性，能力字段明确 `durability:rtc`。

GPIO17锁存无毛刺恢复与SPI初始化幂等修复必须保留。deep前写RTC有效标记须在屏幕完成、无线关闭之后；上次显示中断/异常reset可由dirty标记识别。掉电、软复位、watchdog、模板hash变化、帧CRC不符都撤销显示可信标记，第一次必要显示走全刷。

## 8. 大黑块与分钟时钟刷新策略

### 8.1 语义区域与变化检测

从模板渲染布局建立 `clock/text/usage_digit/solid_tile/inverted_text` 区域，usage四象限按实际rect或元素包围盒定位，不硬编码quad坐标。第一阶段由固件根据rect填充与usage绑定推导，避免立即改模板协议；推导不明确时使用保守整帧策略。若后续新增refresh_hint，必须同时修改固件、Rust canonical JSON、Python及三端hash测试。hint只能更保守，不能让模板关闭安全全刷。

每区域记录旧像素、frame CRC、模板hash、旧/新黑像素数B、面积A、白→黑W2B、黑→白B2W、changed、背景极性、上次全刷、累积ghost预算。黑色位为0。计算 `d=changed/A`、`b_old/new=B/A`、`s=(W2B+B2W)/A`，以及黑→白绝对数。区域相交或按字节扩张后相交必须合并，使用最保守类别；重叠波形不得重复计数遗漏。判定顺序先看基线是否可信，再看极性/黑量变化，最后看面积和预算。

这样即使全屏只变化1%，也能识别一个小象限内部的大规模擦黑。89↔90、99↔00属于数字笔画的双向转换；黑底反白字变化也有黑→白擦除，不能因为背景“没变”就永远局刷。背景黑↔白、区域反相、大黑面积减少优先清影。

### 8.2 初始分层决策（保守阈值，必须实测校准）

按最严重区域决定本次操作；同时保留全帧changed >12.5%的全刷兜底。下列比率均按**整个语义区域面积**计算，不能把dirty bounding box当分母。

| 条件（按顺序） | 默认动作 |
|---|---|
| 帧/plane不可信、异常复位、模板布局/极性更换、上次BUSY失败 | 全屏标准全刷 |
| 所有区域像素相同 | 不刷新；仅ACK/更新业务元数据 |
| 背景反相，或任一区域 `abs(b_new-b_old)≥0.15`，或高墨量区 `B2W/A≥0.10`，或 `d≥0.25` | 全屏标准全刷 |
| 高墨量区（`max(b_old,b_new)≥0.35`或solid/inverted类别）发生变化，预算达到4次或累积`s≥0.50` | 全屏标准全刷；计数按即将执行的操作判断 |
| 高墨量区 `d≤0.05`且`B2W/A≤0.02`、预算未到限，且局刷基线已验证 | 允许一次窗口局刷；黑底反白字按同样规则 |
| 高墨量区介于上述阈值之间 | 默认全刷；仅实验模式可用已验证的窗口清白再写 |
| 普通小文本/数字低墨量区 | 窗口局刷；最多10次或累积`s≥1.0`即全刷 |
| 独立分钟clock区 | 保留90次阈值；到限安排本地全刷，不能为清影强制Wi-Fi |

高墨量预算4次是起始上限，范围1–8次；clock为60–90次起步，不与usage计数互相顶替。每次成功局刷相关区域计数+1并累加s，任一全屏全刷成功才清零全部预算。窗口清白再写不是全屏清影，不能清零其他区域预算；同区域按两次波形记账，初版也不重置其ghost预算。不变数据不增加预算。多区域分别局刷波形数过多时直接全刷（初值≥3个窗口），实测电量与闪烁再调。

若照片显示一次局刷已经发灰，相关黑底类别的预算下调到1，即每次数据变化全刷；不能为了追求局刷率接受质量失败。分钟clock只刷新自己的窗口，不因此反复翻转大黑块。睡眠图标应固定占用小区域，避免每次短会合闪BT图标造成额外刷屏。

### 8.3 双相/清白再写的边界

当前驱动只有 vendored WF_FULL/WF_PARTIAL，没有经过验证的区域去残影波形。默认选全刷，不把两次局刷称为等价全刷。实验功能必须先在对应面板/温度验证：以可信旧图写0x26，将区域变白并执行已允许的波形，确认BUSY完成后将白图作为新previous plane，再写目标区域并执行第二相。需要补齐对0x24/0x26、边框及窗口外区域不受扰动的验证；任一步失败即dirty，下一次全刷。不得随意拼改电压/LUT或在未知温度下启用。

这会有白闪、双倍局刷固定成本，也可能使黑色更灰；仅当照片指标合格且积分电量低于全刷才值得保留。SSD1681残影主要应从波形/电荷历史和基线一致性处理，缩小更新矩形单独不能消除。换成SSD2683也需面板组合、温度与LUT实测，不能靠型号承诺。

### 8.4 对齐、旧图与 deep 恢复

X起点向下对齐8像素，终点向上扩至字节末并夹到199；Y按`199-screen_y`映射。扩出的边缘像素必须从真实旧帧/候选帧填充，不能用白色补边覆盖邻近图标。0x26回填使用上次**成功完成显示**的像素；0x24写候选图。只有BUSY成功后才能更新软件旧帧/RTC和displayed_revision。

deep醒来会重启MCU；屏幕可视图像保留不等于控制器RAM可靠。每次窗口刷新前按当前驱动思路显式恢复窗口previous plane，不依赖mode1+reset恰好保留。RTC无完整旧帧时，时钟仍可用独立旧窗口，但usage只能全刷。保存新帧时必须纳入本周期先执行的clock结果，防止usage刷新把分钟时间写回旧值。

驱动目前`readBusy()`超时仅日志并返回void，目标实现必须传播成功/失败；否则无法建立可信previous plane或可靠ACK。全刷需能绕过“memcmp相同直接返回”：clean_requested即使像素相同也执行清影。clock到90次时用本地已缓存usage/模板重建全帧，不需要联网；无可重建内容则停止进一步局刷并标记维护待办，不能用未初始化图像全刷。首次进入目标模式前必须建立本地可重建快照，此为启用门槛。

## 9. 功耗模型与测量

按电池端积分电量而不是芯片规格估算：

`Q_day = I_deep × T_deep + N_clock×q_clock_increment + N_adv×q_adv_increment + N_update×q_ble_update_extra + N_full×q_full_extra + N_wifi×q_wifi_connect + I_light×T_light + Q_recovery`

时间用小时、电流mA，事件量mAh。每项increment均扣除同时间段deep底流；q_clock包含本次boot时不要再在BLE项重复计boot。全刷extra应相对被替代的局刷，或直接按互斥事件类别统计，不能两边都算完整刷新。

60秒一天1440次；旧网络0.03–0.04mAh/次对应43.2–57.6mAh/天；clock粗估0.012–0.013对应17.28–18.72mAh/天。旧网络数字是否已含clock需要重新分段积分，不能直接相加宣布总量。BLE新方案节约额为被移除网络开销减去BLE全生命周期及增加的清影/偶发Wi-Fi开销。没有实测q_adv、底流和电池有效容量前不承诺续航天数。

测量事件至少覆盖：纯deep底流、仅clock、clock+无人广播1/1.5/2/5s、加密重连、相同revision ACK、512/2048B更新、失败超时、WAKE_LIGHT关联、BOOT300s、黑块局刷/全刷/实验双相。断开USB以免USB锁/供电路径污染；统一电池电压、温度、RSSI、AP条件，每类≥30次记录平均/P95/最大电量和耗时。借助电流仪/分流采样积分；battery_mv斜率仅作长测交叉证据。

## 10. 异常、恢复与兼容

| 场景 | 确定行为 |
|---|---|
| PC关闭/桥崩溃/BLE关闭/一次漏扫 | 时钟照走，广播到期deep，下分钟重试；不自动起Wi-Fi |
| 连接不加密/恶意保持连接 | 首命令与radio硬截止；清暂存并deinit |
| 半包/CRC/schema失败 | NACK，无revision/屏幕变更 |
| 提交后ACK丢失 | 下次同revision幂等ACK；不重刷 |
| OTA/模板超时、低电 | abort清理锁，旧有效槽/旧模板保留，deep或断电 |
| WAKE_LIGHT后AP不可达 | 最多连接期限，记失败回deep；桥保留待办 |
| bridge_id/owner/MAC不符 | 拒绝，不强制接管、不延lease |
| 冷启动/RTC损坏/显示中断 | 显示基线失效，全刷恢复；无新数据时用持久快照 |
| 连续清理失败 | RTC故障计数，直接radio-off/deep；用户BOOT进入诊断，不自动常开 |

双端按能力显式选择 `legacy_wifi_pull` 或 `ble_rendezvous_v2`，模式写配置并可回滚；旧桥不支持v2时继续现有60s Wi-Fi路径，不能在未协商时切BLE后永久失联。新桥配旧固件也只走旧行为。已启用v2后桥暂时消失不能当作“不支持”而每分钟自动Wi-Fi；恢复由BOOT或先前授权的配置切换。旧activity迟滞代码保留给legacy分支，v2不能同时接受旧mode字段偷偷延长lease。

时间同步用发送时server_time和tz_offset_min，BLE NOOP可校时；稳定时期建议每15分钟联系校时（5–60min可配置，待测RTC漂移），这不表示usage变化。跨时区/夏令时改变需立即排NOOP；未联系则按旧时区走并显示同步年龄，不能每分钟向flash写相同TZ。

## 11. 遥测与可观测性

扩展现有RTC历史环，保持有界容量，不以每包flash日志取证。记录wake原因、clock完成、adv开始/结束、加密耗时、BEGIN/COMMIT结果、ACK排空、BLE deinit、Wi-Fi开始/失败、light grant/renew/expire、abort、deep；记录单调时间差与序号，wall clock仅辅助。

status/history目标字段：`power_protocol, next_rendezvous_in_s, adv_ms, radio_on_ms, rendezvous_count/miss_count, auth_fail, mtu, rx_bytes, tx_result, applied_revision, displayed_revision, display_pending, lease_id, light_remaining_s, lease_reason, owner_remaining_s, refresh_kind/reason, dirty_pixels, region_id, W2B/B2W, ghost_budget, previous_valid, busy_timeout, pm_cleanup_fail`。电量由测量仪关联序号；设备不得报告未测mAh。桥记录发现延迟、机会序号、ACK延迟、重试/队列年龄；“预期deep”“未见会合”“确认故障”分开，不把deep HTTP失败当push告警。

## 12. 分阶段实现顺序与准入

1. 基线取证：保存当前ROM/配置/照片，测现有分钟周期和黑块对照；审计RTC/堆容量、驱动BUSY返回值、GATT兼容分流；建立可回滚开关。
2. 先完成显示安全层：区域统计、ghost预算、错误传播、旧帧可信标记、相同帧清影和本地全刷；保守全刷路径通过照片验收后再开放黑块局刷。
3. 实现v2事务：现有特征复用、能力协商、分片/CRC/幂等、ACK、授权、超时；此时仍可用显式测试会话，不改变默认无线模式。
4. 实现固件会合与lease：clock优先、radio硬截止、BOOT300s、HTTP电源命令、deinit检查、OTA收尾；验证每条返回deep路径。
5. 实现桥常驻事件监听、持久revision、usage合并、WAKE/RENEW/SLEEP调度和owner衔接；用户显式模板/OTA队列做端到端。
6. 小规模启用v2，执行功耗/Windows多轮/长测；仅在可达性与画质达标后默认启用。双相实验最后独立开启，不阻塞保守全刷方案发布。

各阶段均先计划、记录测试和PROGRESS；本文件不授权提交或部署。修改模板协议时仍需三端哈希与共用渲染像素一致性检查。现场开启前保证BOOT可恢复legacy模式。

## 13. 测试与验收矩阵

阈值是首轮工程验收建议，不能报告为已通过。照片固定支架、曝光、照明、距离、白平衡，配同面板标准全刷图；对齐像素区域后比较，不以自动曝光照片推算电量。

| 测试 | 序列/条件 | 必须观察/通过条件 |
|---|---|---|
| 数字残影 | 89↔90、99↔00各100次，白底黑字与黑底反白 | 每次记录路径/预算；旧笔画无持续可辨残留；黑区不能随次数单调发灰 |
| 黑块极性 | 黑底↔白底、区域反相、全黑↔空白各30轮 | 触发全刷；边缘/邻区不被白补边污染 |
| 预算边界 | 1/4/5/10/30/90/120次；多个区域交错 | 正确升级，clock与usage独立，无计数丢失 |
| 不变数据 | 同revision重发100次，丢ACK后重发 | usage刷屏数0，lease截止不变，ACK幂等 |
| deep跨周期 | 每分钟clock、每5分钟数字变化，≥2h后24h | 0x26基线一致，时间不回滚，累计预算持续 |
| 温度 | 室温及面板规格允许的低温点（记录实际°C） | 波形质量/BUSY时长不过限；未验证低温禁黑块局刷 |
| 电源故障 | 接收/提交/波形/ACK各阶段断电与软复位 | 不展示半包；下次基线失效全刷；revision可重同步 |
| 事务错误 | MTU23/协商MTU、乱序/重复/越界/CRC错、内存不足 | 有界NACK、旧状态保留、radio按时关 |
| 会合 | 1/1.5/2/5s，桥重启/PC睡醒/弱RSSI/多设备 | 统计首轮命中、两轮命中、P95延迟；不承诺100% |
| lease | BOOT300s；普通读取/claim/数据轰炸；显式renew/重放 | 无显式renew时300s到期；重复request不延长；Wi-Fi失败及时deep |
| 静默唤醒 | deep 下 BOOT/PWR，及失败退避的定时重试；连接成功与超时各≥10次 | 全程无 `Connecting:` 页；唤醒首帧（全刷）即清 Zzz，关联中隐藏 Wi-Fi 图标，连上后局刷显示；进 deep 恢复 Zzz 且隐藏图标；失败恢复睡眠帧并有界回 deep；冷启动仍显示连接页 |
| OTA/安全 | 过期中OTA、停滞、token错、他人owner、无bond | 有界abort/旧槽可启动；鉴权不变，无隐式claim |
| PM | 每条退出路径、deinit失败注入 | BLE锁释放；无无限radio；总窗口在配置硬限内 |

照片指标初值：以同次全刷黑白动态范围D归一化，旧笔画区域与邻近同色区域的平均亮度残差≤0.05D，黑块平均亮度相对全刷变浅≤0.05D；测量误差/照明漂移另报告。达不到则该区域改为每次全刷，双相关闭。像素验收要求软件候选图与共用引擎参考逐像素一致、窗口外软件像素零差异；软件像素通过不能替代照片验收。

无线实验准入建议：稳定桌面条件至少1000个机会，首机会成功≥95%、两机会累计≥99%，报告置信范围与环境；不达标优先调扫描/1–2s窗口，5s只作对照，仍不达标保留legacy默认。所有无指令/无效流量测试必须100%在硬截止内关闭无线（允许记录时钟误差容限100ms），任何无限保持即阻止发布。

功耗准入以同场景整机24h电量为准：v2空闲必须低于legacy，报告误差区间、全刷次数与失联比例；改善落入测量误差则不宣称省电成功。回滚条件包括持续发灰、基线错位、事务非原子、owner/token回归、漏会合达不到目标或PM锁泄漏；按模块关闭黑块局刷/双相，或回legacy协议，保留诊断证据。

## 14. 开放问题（不阻塞保守方案）

- 本板RTC可用空间能否同时容纳5000B旧帧、历史和usage摘要？不足时使用有限区域+全刷回退，不能默认依赖PSRAM跨deep保存。
- Windows适配器是否及时报告Service Data、重连加密P95多长？这决定广告窗口和小包上限，需实机而非规格值。
- 现有面板在低温/高黑量下可接受的局刷预算与两相收益是什么？未测前使用全刷兜底；面板温度未可靠读取时不启用温度自适应。
- q_adv、deep底流、全刷额外电量与真实电池容量尚未知；不作天数承诺。
- owner租期与15分钟校时频率如何减少偶发Wi-Fi claim成本？初版保持现有owner语义，优化需独立协议决策。
- RTC-only ACK的跨断电恢复体验是否足够？如果必须持久ACK，再单独评估flash写入寿命和事务存储方案。

## 15. 外部依据与适用边界

- [Espressif BLE 低功耗参数说明](https://docs.espressif.com/projects/esp-techpedia/en/latest/esp-friends/advanced-development/performance/lowpower/lowpower-demo.html)：扫描窗口小于扫描间隔可降低平均功耗，连续扫描功耗最高；被动扫描更省电，发现目标后应停止扫描。本文据此把持续监听的成本放在有电源的 bridge 端，并要求目标出现后停止本次扫描/转连接。
- [ESP-IDF NimBLE power-save 示例](https://github.com/espressif/esp-idf/tree/master/examples/bluetooth/nimble/power_save)：说明 ESP32-S3 的 BLE modem/light-sleep 能力和典型芯片级数值。该数值不等于本设备整机电流，本文只把它当机制依据，仍要求板端积分测量。
- [Microsoft BLE advertisement watcher](https://learn.microsoft.com/en-us/windows/uwp/devices-sensors/ble-beacon)：Windows 以事件流接收广播并支持筛选；active scanning 更耗电。官方没有给出所有适配器都满足的发现延迟保证，因此本文不把1秒、1.5秒或5秒写成普适可靠窗口。
- [Microsoft BluetoothLEDevice 连接说明](https://learn.microsoft.com/en-us/uwp/api/windows.devices.bluetooth.bluetoothledevice.frombluetoothaddressasync)：创建对象不一定已经连接，GATT 操作的连接等待可能显著变化。本文因此把“广播被发现”和“加密连接+事务完成”拆成独立期限，并保留下分钟重试。
- [Ruuvi sensor protocols](https://github.com/ruuvi/ruuvi-sensor-protocols) 与 [BTHome](https://bthome.io/) 展示了把小型状态/序号放在广播、容忍漏包并重复发送的实际模式。本文只借鉴“摘要广播+可重试”思想；业务 usage 仍走已绑定加密连接，不把敏感数据放入广播。

这些资料都不证明本项目在特定 Windows 驱动、适配器、RSSI 和一分钟节奏下的成功率，也不替代 SSD1681 面板的照片与功耗实测。§13 的准入矩阵是最终定值依据。
