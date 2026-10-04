# Fake ROM BLE 真实性补齐与 ROM / Bridge 共用合同

日期：2026-10-03。状态：**实施中（2026-10-03 用户授权开始实现）**。先交付 P0/P1 共享编码与末端保护，再接 P2 属性路径；软件阶段不部署、不操作设备。实施入口仍归 [backlog B6](../../docs/roadmap/backlog.md#b6-离线-ble-配-wi-fi-与-boot-蓝牙恢复入口2026-10-03-新开)，关联 B5 的 macOS 会合验收，不另建重复待办。Wi-Fi 产品语义以 [wifi-redesign.md](wifi-redesign.md) 为准，旧 [remaining-plan.md](remaining-plan.md) 中不一致的步骤不能覆盖最新要求。

## 1. 要解决的问题与范围

Fake ROM 目前能验证部分业务成功，却没有忠实模拟 BLE 回复落入 GATT 特征值、被读取和按请求关联的过程。设备真实出现过 635 字节写入超过 512 字节属性上限；同一时期 Bridge 可连接、可读身份，但取不到 status ACK。需要建立一条能在软件中暴露此类故障的链：**固件实际回复构建 → 最终编码字节 → 属性写入结果 → Bridge 实际客户端读取/等待 → 现有作业确认与恢复**。

交付范围是最小共享编码/容量边界、Fake ROM 属性行为、Bridge 等待与现有作业衔接、针对所有 BLE 出站类型的回归。不是只缩短一次 status，也不新建通用传输框架或异步任务平台。保留既有 Bundle 发布、OTA、数据序号/确认、owner、PowerPlan 与排队机制；不因本计划改动运行数据、旧 ROM、日志或其他未提交代码。

用户允许主要软件验收交给 Fake ROM。它应证明字节合同和业务恢复，不代替 RF、CoreBluetooth/TCC、真实绑定/NVS 断电、屏幕、电流或长时实机证明。

## 2. 已核对事实、证据与尚不能下的结论

| 证据层 | 已知事实 | 结论边界 |
|---|---|---|
| 当前 Bridge 源码 | `bridge/crates/ble/src/lib.rs::DeviceConnection::command` 的 fake 分支按 180 字节 POST 请求片段，总请求上限 8192；最后 HTTP JSON 直接作为 ACK。真实分支写 GATT 后读 Status 特征，最多等 5 秒匹配 `ack/request_id` | fake 绕过了实际属性容量、读取及等待路径；180/8192 是请求侧限制，不是回复容量 |
| 当前 Fake ROM 源码 | `device-sim/src/main.rs::sim_ble_command` 借用 HTTP status，删去 `data_contract` 后补 ACK/wake 字段直接返回；`sim_ble_fragment` 仅检查片段/重组/offset | 删除一个字段并不等于固件 BLE 投影；正常 HTTP 返回也不证明 GATT 能写入 |
| 当前固件源码 | `main.cpp::v2Response` 仅对含 `records` 的历史回复裁到 480；普通 status/其他 ACK 无最终总长度限制。Wi-Fi 回复有绕过 `v2Response` 的分支；`ble_bridge.cpp::refreshInfo` 单独写 INFO | 只改 status 或 `v2Response` 均会漏掉其他出站路径 |
| NimBLE 本地依赖 | `.pio/libdeps/zectrix-note4-b/NimBLE-Arduino/src/NimBLEAttValue.cpp::setValue` 先清空再 append；超 max=512 时 append 不写新值。`bleNotifyStatus{Quiet}` 未检查写入结果，随后可继续 notify | 该版本超限应建模为属性变空/没有新匹配 ACK，不能默认“保留上一条 ACK”；通知是否到达另行模拟 |
| 已有软件检查 | `tools/test-wifi-setup.cpp` 对 Wi-Fi 分页帧有 `<=512` 断言；`device-sim/tests/bootstrap.rs::bridge_device_connection_uses_fake_ble_without_os_radio` 验证正常业务 | 尚未覆盖普通状态、全部错误 ACK、属性失败、真实 ACK 等待，已有通过记录不能证明本缺口已闭合 |
| 实机日志 | `artifacts/bridge-macos/usb-contact-diagnosis-20261003-181134.log`：18:11:50.532（北京时间）记录 `val > max, len=635, max=512`；同次 Mac 日志 UTC 10:11:49.225115 写 status，10:11:54.226 超时 | 已证明 ROM 有超限写入；时序强烈指向 status，但超限日志无 op/request_id，不能最终认定 635 字节就是该 status 帧 |
| 现场历史 | 13:41:48 以后记录超过百次连接/INFO 身份核对成功，随后 status 5 秒超时；Wi-Fi 曾返回 Host is down。串口打开后记录 `USB_UART_CHIP_RESET`，复位后 Wi-Fi 恢复，18:13:06 已取得完整状态 | 串口改变了原现场；这次复位不能证明此前试连跳屏就是复位，也不能证明超限是全部断网/跳屏的根因 |

最后认证在机为 Note4 `7C4FADB93408` / `0.18.42-note4-b-wifi1`；`.43` 是未部署候选，当前工作树与在机代码不能混同。IP `192.168.10.101` 是历史登记/复位后观测，不是永久可达保证。本轮只读了已有证据，未重新探测设备；串口保持关闭。

## 3. 通道合同

### 三个不同的容量层

1. **MTU / ATT 片段**：决定一次底层传输能装多少数据，长读可能由多个 ATT 片段完成。不能据此把属性容量扩大。
2. **GATT 属性值**：当前 INFO、Status 的有效值最大 512 字节。长度是最终序列化后的 UTF-8 字节数，不是字符数，也不是未补 ACK 信封的 JSON 长度。
3. **应用分片/分页**：请求现用 180 字节片段、8192 字节普通总上限；Wi-Fi 当前业务另有 1024 字节入口限制，需保留并明确记录。回复只允许合同声明的分页；每页自身含完整关联信封仍必须 `<=512`。普通 ACK 不得偷偷截断 JSON 或拆成客户端不认识的多条通知。

512 是硬上限。可将 480 作为设计留量目标，但只能对最终帧测量，不能以“主体小于 480”代替最终检查。所有合法输入的正常回复必须有确定上界；超限保护是兜底，不应成为正常 status 的常规结果。

### 谁承载什么

| 通道/机会 | 可承载内容 | 超出范围时的处理 |
|---|---|---|
| 自然短 BLE 会合 | 精确身份、能力与会话、关键序号/修订、短 ACK、在本次预算内的小数据、申请现有有界 Wi-Fi 工作窗口 | 给短的能力/需要 Wi-Fi 结果，作业保留到合适会合；不读整套 Wi-Fi 方案、不拉详细诊断、不因暂时 HTTP 不通扩大 BLE 范围 |
| 正式 Wi-Fi 窗口 | 完整认证状态及 data_contract、全量 Wi-Fi 快照读取/比较所需内容、批量同步、详细诊断、Bundle、OTA | 沿现有认证、owner、窗口和队列；无法连接本次结束，任务继续等待 |
| 断网物理 BLE 维护窗口 | 已绑定加密链路上的必要离线配网、能力读取、恢复所需有界结果查询；必要内容显式分页 | 固定维护截止，不因读取/分页续时；全套日常方案比较仍等 Wi-Fi。无法在本窗口完成则保存可恢复意图并等待下一次授权机会 |

小数据不能仅凭“请求小于 8192”判断可在短会合完成：发送前估算片数与剩余窗口，预算不足则延后或转 Wi-Fi。连接、读取、ACK 等待、分页、重试均不改变 radio 截止或正式 light deadline。申请窗口只复用现有 PowerPlan/sync_open 的合法权限与条件；不能把人工 Wi-Fi 作业伪装成 periodic 同步来绕开条件。

Wi-Fi 管理继续采用 Bridge 本地目标、显式读取设备完整快照、同步基线三份记录；逐项/字段选中差异后显式双向同步。读取/本地保存/同步/试连/启用分开。BLE 缩短回复不得让缺字段被误认为空值、已删除、已同步或密码相同。

## 4. 所有出站路径的清单与容量责任

实施 P0 必须逐个调用点核对，下表是已确认入口，不是只挑正常成功帧的白名单。新增任何写 INFO/Status 的调用都必须进入同一容量检查。

| 类型 | 现有入口 | 新合同/检查重点 |
|---|---|---|
| INFO | `ble_bridge.cpp::refreshInfo`、`main.cpp` 提供的 infoExtra | 只放身份/固件与协议能力/会话所需状态；当前所有 extra 字段逐项审计，长诊断移出。未绑定信息不泄漏秘密；按最长合法版本/身份字段测量 |
| 普通 BLE status | `serviceV2Ble` → `v2Response` | 独立短摘要，不从 HTTP 状态删字段凑出；保留建立会话、选择数据/计划和识别上下文所必需的字段，整份字段合同走 HTTP |
| 普通成功 ACK | `v2Ack/v2BuildAck`，data/plan/sync_config/sync_open | 业务确认所必需的 result/seq/plan/context/retention 与关联信封；不携带全量诊断/长显示文本 |
| 拒绝与错误 ACK | 会话/认证、owner conflict、retired_protocol、未知 op、shape/range/storage 等 | 错误码有界，owner 只放必要有界身份；禁止回显任意长 op、输入对象或错误字符串。非法 request_id 不原样塞回复 |
| 历史回复 | history → `traceHistoryJson/v2Response` | 含最终信封的分页预算、游标/截断语义、空页/单条超预算/无进展保护；普通短会合不主动拉详细历史 |
| Wi-Fi 页与能力 | `wifiSetupReadPage`、`wifi_capabilities`、`wifi_scheme_get` | 页信封、hex chunk 膨胀、snapshot/offset/total/request_id 均计字节；固定快照，跨版本/过期重读，未完成不替换旧完整快照 |
| Wi-Fi 非分页回复 | `wifiSetupCommand`、wifi_owner、main 中 MAC/shape 拒绝 | setup/status/cancel/delete/select 等所有成功/拒绝/进度/结果都测最终长度；维护权限与 owner 分离仍保留 |
| AUTH / endpoint | auth handler、endpoint handler | 成功和失败均通过最终校验；token 不进日志或普通诊断；fake 不能再通过旁路无限大 JSON 绕过属性模型 |
| 其他兜底 | dispatchTemplateCtrl 的 not_ready、未就绪/解析失败以及今后新增回调 | 明确是否应有回复、能否关联；没有合法关联信息时只提供有界错误/记录原因，客户端仍遵循有界等待，不伪造成功 ACK |

P0 产物应包含每类“生产构建入口、允许字段/字节上限、最长帧、测试编号”表；未纳入的写入点视为验收缺口。命令不支持时应有明确短拒绝；不可把业务拒绝统统当作传输失败重试。

## 5. 最小共享实现边界（待实施）

### 固件负责定义真正的 wire bytes

复用已有 `src/v2_command_envelope.{h,cpp}`、`v2_status_snapshot.*`、`wifiSetupReadPage` 及 `bridge/crates/render` 的 C++ 宿主/FFI 路径。把 `main.cpp` 中纯 BLE 摘要构建与最终信封包装抽为小的无硬件函数；只有确实需要独立文件时才加 `ble_wire.{h,cpp}`，不复制第二套 Rust schema、不引入代码生成框架。

该边界接收类型明确的运行字段，完成：允许字段投影 → 关联信封 → 同一 ArduinoJson 配置编码 → 最终字节计数 → 可写/需要分页/合同错误结果。Fake ROM 将自己的运行状态作为输入，调用同一宿主构建函数，拿到**原始字节**后放入模拟属性。不得在 Rust 中重新 parse/serialize 后再当作原帧：转义、空值、省略规则和字段顺序可能改变长度。

INFO、AUTH/endpoint、Wi-Fi 外围直接构建也必须复用相应小构建器或至少同一最终校验，并由固件宿主用生产函数验证字节。Wi-Fi 业务状态机继续复用既有 HAL/宿主方法，避免为每种 ACK 手写模拟模板。共享的是编码/验证与必要纯逻辑，不要求把 `main.cpp`、NimBLE 或整个固件移植进模拟器。

### 先确定字段预算，再选择短摘要编码

P0 逐项核对现有合法长度和调用者依赖。已有普通 `request_id` 校验为最多 64 字节，但 status/早期错误不一定经过该校验；Wi-Fi identifier 约束也不同。统一在回显前处理，不只约束 Bridge 常见的短 ID。合法的 64 字节 ID、最长 context/template/job/owner 字段、32 位 wake 最大值、64 位 seq/plan 最大值、可选项全开应一起计算。对引号、反斜线和控制字符的编码膨胀逐个计量；UTF-8 多字节按字节上限。

若保留现字段名及所有合法长字段无法装下 512：短会合只保留决定后续动作的稳定 ID/修订，显示名、长字符串及详细对象移到 Wi-Fi；若必需字段仍超预算，增加一个明确协商的紧凑 BLE 摘要版本，Bridge 解码为现内部类型。不要静默缩短标识符、截断 request_id、模糊哈希身份或把新格式伪装成旧版；不恢复已废弃的 protocol/rv 通道。能力标记自身要有界，旧 ROM 能力缺失时走现有有界尝试/等待或提示升级，不能进入无限失败循环。

P0 必须用最坏输入的实际编码预算确定最小字段集合/版本后，P1 才实现；本设计不声称现有长字段组合已经能装下。成功/错误帧同等受约束，尤其不能在异常路径回显超长输入造成第二次超限。

### 属性写入兜底和可观测性

`ble_bridge.cpp` 在 INFO 与 Status 所有 `setValue` 前检查实际长度，并检查库提供的写入结果或写后长度；失败不继续伪装新帧通知。正常新合同应从上层保证可写，此处防漏网和内存分配等失败。

若能生成合法短错误：携带已验证的关联信息与 `reply_too_large` 等有界码，并区分“尚未执行”和“业务可能已执行，结果待查询”。**业务已提交后不能回报 rejected 来暗示未生效**。无法安全构建关联帧则不送新 ACK，客户端超时并进入待确认。错误降级不递归；非法关联字段不能靠截断后碰巧匹配另一个请求。

日志只记 op 枚举/脱敏关联标识、特征名、编码长度、上限、写入结果、wake、阶段与耗时；不记录原帧、密码、token。增加此元数据正是为了以后能区分“635 属于什么帧”，不根据缺日志猜定论。

## 6. Fake ROM 属性与故障仿真

现有 `/sim/ble/*` 继续作为本地测试载体，HTTP 只模拟操作，不等于 BLE 业务 ACK。最小改动如下：

1. 按连接 generation 保存 INFO/Status 原始字节、接收缓冲和有界事件；请求片段写入返回“传输已接收/片段继续”，最后一片只触发业务与属性更新，**不直接返回业务回复给 command**。
2. 另提供受现有模拟控制权限保护的属性读操作；Bridge fake transport 用它读原始 bytes。真实和 fake 共用 ACK 解析、关联、deadline、结果处理；只替换属性读写 I/O，不复制 command 的业务判断。
3. 默认容量固定 512；测试可以在专用控制入口注入字节/时序，不能扩大生产默认来让测试通过。INFO、AUTH/endpoint 回复也经过相同属性存储；不能只修 command 留旁路。
4. 明确保留两种测试对象：**现依赖行为模型**（setValue 超限先清空，后无匹配 ACK）与**新固件保护行为**（预检拒绝，不破坏旧属性，可能写短错误）。二者不得混成“超限总保留旧值”。旧 ACK/迟到 ACK 是独立故障注入，用于验证关联隔离。
5. 故障至少包括：属性超限、写入失败、写后断链、更新延迟、空属性、畸形字节、旧/错 request_id、错误 MAC/wake、通知丢失、响应被下一帧覆盖、读取失败、窗口中途关闭、分页快照失效、业务已持久化但 ACK 丢失。
6. 通知丢失且属性可读时，当前读式客户端应仍能成功；“丢通知”和“属性没有可读 ACK”分开。长读若被建模，应由多个片段重组出不超过 512 的同一属性，不能靠长读取出 635 字节。
7. 请求重组继续验证 180/8192 与 Wi-Fi 单独限制，并核对真实 `appendJson` 按 JSON 完整性派发、断链清缓冲行为。fake 的 offset/final 只是测试载体协议，不宣称实机线上存在这些字段；不完整、乱序、越限、断链重发要在客户端可见结果上对齐。

输出属性失败不能直接变成 `POST /sim/ble/fragment → HTTP 413 → command 立即失败`；这是请求上传失败模型，不是设备回复超限。可在控制诊断里看到属性写失败，但 Bridge 业务只能观察当时真实链路可见的内容。故障控制使用虚拟时间或可控时钟推进，避免测试靠真实睡眠数分钟；验证真实客户端 5 秒上限的代表案例可使用受控时钟，不为测试延长生产超时。

## 7. Bridge 有界尝试和异步待确认

复用 `DeviceConnection` 与既有设备作业/会合调度器，Wi-Fi 作业按 [wifi-redesign.md](wifi-redesign.md#异步会合作业读取与同步不以首次连接成败结案) 的状态语义扩展；不另起永久高频扫描/HTTP 轮询。

- 单条 BLE 命令沿现有最多 5 秒 ACK 等待，并受连接和本次 RF 窗口剩余时间的更早截止限制；重复读空值/旧 ACK 不能重置 deadline。窗口结束即结束本次尝试，不为排队任务无限续 RF。
- ACK 至少校验类别、合法 request_id、目标 MAC/已验证连接、会话与 wake 一致性。当前真实路径对 wake mismatch 仅警告；P0 应核对各类回复能力，在新合同中明确拒用过期会合 ACK，旧能力缺失不能被伪造成已确认 wake。fake 必须跑相同逻辑。
- 只读失败：保留读意图和旧完整快照，下一真实会合重读。同步写入在发送前已知失败可等待重试；写入完成或部分传输后结果未知时进入“设备已接收/待确认”或“提交结果待确认”，不能直接报失败/成功。
- 低层 `command` 目前每次生成新 request_id。对持久作业必须将冻结的请求 ID 传给命令或保留独立稳定事务 ID；重试不能生成新身份导致重复应用。只做这条最小接口调整，不改普通状态查询的现有便利入口。
- 待确认作业下一会合先查询原事务/修订与实际字段；已提交则确认，明确未提交且原条件仍成立才重试，同 ID 不同内容拒绝。若旧 ROM 无结果查询/幂等能力，用可验证 revision/seq 证明；仍无法判断则继续待确认/要求处理，禁止盲目重发有副作用操作。
- Wi-Fi 未就绪、一次连接超时或未发现设备是本次尝试结果，不是整个作业终态。退避 5/15/60 秒可沿设计使用，仅作为下一次机会的最早时间；下一次尝试由实际会合事件触发，不按退避间隔强制唤醒。
- 权限不足、缺凭据、修订冲突分别等待补充/处理；明确不支持/无效请求、用户取消、作业到期可结束。本地默认 24h 意图期限不延长设备窗口。取消已发送事务不假装回滚，仍标明已确认范围。
- 持久作业保存非秘密意图、原基线/批准范围、目标 MAC、本地修订、事务 ID 与阶段。Bridge 重启恢复等待/待确认；仅内存凭据丢失则等待重新输入，密码不进入普通队列 JSON。
- 同 MAC 沿已有传输互斥，保留 OTA/Bundle/数据优先与串行规则；同范围读复用在途任务。成功 ACK 才更新既有数据确认指纹/full_sync_deadline，Wi-Fi 同步要回读持久修订和所选字段才更新同步基线。

本计划不把整个 Wi-Fi 重设计提前包进 BLE 修复。只需接好最小读/同步作业的等待和待确认边界；完整差异 UI、原子变更集、独立试连/启用继续由 B6 Wi-Fi 方案阶段交付。缺该设备能力时，软件测试应报告不支持，而不是模拟通过便对当前 .42 宣称支持。

## 8. 必须能失败、也必须能通过的回归矩阵

| 场景 | 测试层 | 验收断言 |
|---|---|---|
| 最终合法帧 511 / 512 / 513 字节 | 共享编码边界、Fake 属性、真实 DeviceConnection 路径 | 511/512 可写可读；513 不成为成功 ACK，按对应旧/新行为进入有界错误/待确认 |
| 已观测 635 字节 | 故障注入 + 关联集成 | 构造明确标注“同长度合成帧”的 635 字节值复现无 ACK；日志有 op/字节数/结果。未抓到原帧前不能把夹具称为实机原帧 |
| 最长合法组合 | 固件宿主生产构建函数 | 64 字节 request_id；最长合法 context/template/job/owner/fw；wake 计数最大值、seq/plan 最大十进制位宽；全部可选字段同时存在，所有合法帧守上限 |
| 最坏编码 | 同上 | 引号、反斜线、控制字符、中文/多字节 UTF-8、边界空串；按编码 bytes 计量。非法 NUL/非法 UTF-8/超长字段按合同拒绝且错误帧也有界 |
| 各类成功与错误 | 固件宿主 + Fake 客户端 | 第 4 节每类至少正常和最坏拒绝帧，覆盖 INFO、auth/endpoint、普通/status/Wi-Fi/历史，不能仅测分页 chunk |
| 属性更新失败/空/旧/迟到/覆盖 | DeviceConnection fake transport | 无 HTTP 旁路成功；不匹配 ACK 不推进业务；deadline 不被重读重置；读失败/断链保留可恢复结果 |
| 通知丢失但值可读 | 同上 | 读式客户端正确完成，避免把 notify 丢失错误建模为必然 ACK 超时 |
| 请求分片边界 | 请求重组与客户端 | 179/180/181 的片大小、8191/8192/8193 总长度及 Wi-Fi 1024 边界；分片合法不意味着回复合法；断链不串接旧请求 |
| 分页一致性 | 生产 Wi-Fi/历史分页器 + Bridge 读取 | 每页含最长信封仍不超限；空/末页、offset/total、快照过期、单项装不下、重复页/无进展均有界；未完整不替换旧快照或推断删除 |
| Wi-Fi 暂不可用后恢复 | 现有作业 + Fake 虚拟会合 | 首次失败后仍等待；不永久轮询；下一真实会合取得完整快照或继续同步；不自动保存/启用/覆盖本地 |
| 已提交但 ACK 丢失 | 同上 | 同事务查询/回读后只确认一次；不重复提交、不扩大批准差异；重启恢复仍待确认 |
| 错 MAC/bond/owner/session/wake | 客户端 + 宿主业务 | 不接受他设备/旧会合结果，不自动 claim，不越 bond；拒绝与暂时传输失败分开 |
| 原机制兼容 | 适当既有回归 | 数据序号、PowerPlan 截止、sync 窗口、Bundle/OTA 排队与确认保持；保存目标不发布、不切网 |

大小测试不允许通过把所有字段都清空来过关。低层可测试任意字节长度；协议层必须使用可解码的真实生产帧验证，以区分“属性装得下”和“业务格式有效”。至少一个端到端测试从 `DeviceConnection::connect/command` 走全部路径，而非直接 POST 模拟端点断言 JSON 成功。

## 9. 分阶段实施、文件与退出条件

以下是分阶段要求；2026-10-03 用户已授权开始实现，当前交付与未完成边界见第11节。

| 阶段 | 工作与主要文件 | 阶段验收 / 依赖 |
|---|---|---|
| P0：锁定合同和版本边界 | 本文、`wifi-protocol.md`；只读梳理 `src/main.cpp`、`ble_bridge.cpp`、`v2_command_envelope.*`、`wifi_setup.cpp`、BLE 调用者 | 完成所有写入点清单、字段最大字节/错误回显规则、最终最坏预算、短摘要字段/能力协商决定；区分 .42、.43、工作树。不可拿未部署源码还原原 635 帧 |
| P1：共享生产编码与末端保护 | `src/v2_command_envelope.*`，必要时小 `ble_wire.*`；`main.cpp`、`ble_bridge.{h,cpp}`；`render/build.rs`、`render/src/ffi.cpp` 与必要 Rust 导出；`tools/test-wifi-setup.cpp` 和小宿主测试 | 固件和宿主使用同一构建/最终校验；全部合法最坏帧可写，越限和构建失败有明确结果，日志不泄密；512 边界覆盖。依赖 P0，不运行整板模拟 |
| P2：Fake 属性替代 HTTP ACK 捷径 | `device-sim/src/main.rs`、`ble/src/lib.rs`、`device-sim/tests/bootstrap.rs` | 写片段与读 Status 分离；共享 ACK 等待/关联；635/513 和属性失败在真实客户端路径复现；旧库与新保护模型明确。依赖 P1 的字节构建，可先用故障注入暴露旧缺口 |
| P3：连接现有作业恢复 | `bridge/crates/app/src/wifi.rs`、`platform.rs` 和既有调度/持久化代码，必要的 BLE 稳定事务 ID 入口；对应应用测试 | 首次不可达保留任务、实际会合再继续、丢 ACK 后查询/回读、重启恢复、冲突暂停、有限期限；无新永久服务。依赖 P2 与 Wi-Fi 设计所需能力，缺能力明确标注 |
| P4：软件验收与计划对账 | 上述宿主、BLE、Fake bootstrap、应用相关测试；B6 和 PROGRESS 证据 | 第 8 节逐行有结果/证据；既有发布/OTA/数据/电源相关回归无回退；把软件已验与未实现/未验硬件分栏记录。完成后仍不冒称已部署 |
| P5：另行安排最小硬件确认 | 仅 Note4 已确认目标与标准发布/回退流程；沿 B5 现场矩阵 | 核对真实特征值、自然短会合/断网维护/窗口截止、日志关联与已绑定重连；真实 RF/TCC/功耗、Windows/缺席硬件按实际验证。用户已追加 Note4 刷机授权，最小确认见第12节；完整矩阵仍未验 |

后续测试从既有 `tools/test-wifi-setup.cpp` 宿主入口、`bridge-ble`、`device-sim --test bootstrap` 及 app 相关用例选取，不例行全量重建/清缓存。`bridge-render` 已有 ArduinoJson/C++ 宿主依赖，优先复用；缺依赖时记录阻碍，不以当前任务为由运行 pio 下载或构建。所有生成物放既有隔离 artifacts/target，保留 AppSupport 数据和原 ROM。

推荐可独立审查的交付顺序是 P0+P1（合同与固件保护）→ P2（真实性回归）→ P3（恢复衔接）→ P4（软件证据）。不在同一改动中重写 Wi-Fi 全部 UI 或一般任务引擎。软件放行门槛是矩阵满足，不以一条 status 测试通过结项。

## 10. 未决项与未验限制

- 原 635 字节帧内容未知；有长度和时序证据，没有 op 级直接证明。修复范围覆盖全部出站类型，不把此不确定性掩盖成确定的 status 根因。
- P0 已实测并锁定 `ble_wire=1` 的紧凑摘要与能力边界，详见 wifi-protocol.md 新节；在机 .42 和旧 .43 候选仍没有这批改动。
- 本地 NimBLE 源码提供了清空/append 失败模型；当前在机具体依赖版本与镜像构建来源仍须发布记录关联。Fake 不能证明真实通知/读时序完全相同。
- 新 Wi-Fi 全量快照、原子变更、持久结果查询属于设计中的能力；当前 protocol=1 不等价。无能力场景要有真实不支持/待确认结果，不用模拟器补出的功能冒充 ROM 已有。
- 原试连跳屏/断网原因、真实自然会合成功率、TCC/主机换网、NVS 断电行为和耗电仍未验；串口复位后的恢复不是原故障消失的证明。
- 原稿是仅规划记录；用户授权后的软件实施见第11节。设备和运行 .app 未随源码变化，不依据软件测试宣称已部署。


## 11. 软件实施第一批（2026-10-03）

已交付 P0/P1 核心与 P2 属性/客户端链。P3只增加稳定请求ID入口，持久作业恢复尚未实现；P4全矩阵尚未放行，B6保持未结项。正式字节合同在 [wifi-protocol.md](wifi-protocol.md#ble-wire-v12026-10-03-软件候选未构建部署-rom)。

| 出站类 | 生产入口与最终门槛 | 字段预算 / 证据 |
|---|---|---|
| INFO | bleBuildInfo → writeAttribute | FW64 bytes base64；固定MAC/IPv4；完整能力/身份/wake；宿主样本415（布尔保守419） |
| status | bleBuildStatus → v2BleNotify/bleBuildReply → writeAttribute | request64/context32/template16/job16；nonce32hex；seq/plan64位；最终样本452（布尔保守458） |
| 普通成功/拒绝 | v2BuildAck → bleBuildReply；owner对象删除、未知op归command | 实际生产构建器各data/plan/sync_config/sync_open/command最大419；uint64高位data_seq完整保留 |
| history | traceHistoryJson → bleBuildReply → writeAttribute | 最终加入rid/wake后裁完整记录；宿主空/单条超预算/多条裁剪；Fake仍明确http_required，未实现独立history HAL |
| Wi-Fi页/进度/拒绝/owner | wifiSetupReadPage/wifiSetupCommand/main外围 → v2BleNotify → writeAttribute | 80 raw bytes页；生产状态机全部事务回归、最长request/query/network组合最大418；owner查询实际身份有界，完整对象走HTTP |
| AUTH / endpoint | bleBuildAuth/bleBuildEndpoint → writeAttribute | 同一生产构建器；AUTH真实32hex token正常/拒绝、endpoint正常/拒绝；Fake也存Status后读，不直接给token ACK |
| 未就绪/其他兜底 | dispatchTemplateCtrl → writeAttribute | 固定短错误，无合法关联时有界等待；全部INFO/Status setValue收敛到一个门槛 |

Fake现在保存原始C++编码字节；fragment/command只返回上传accepted，Status另读。按连接generation保存/隔离属性、清接收缓冲，重启不复用旧generation；有界脱敏属性事件可从控制权限 `/sim/ble/attributes` 查询。生产值不经Rust再parse/serialize；专用故障可构造同长度合成/错关联帧。

已软件覆盖：511/512/513、635合成超限（旧库清空、新门槛保留）、write_failed/empty/malformed/stale/覆盖/wrong request/MAC/wake/op、读失败、断链/窗口关闭、通知丢失但可读、虚拟时钟延迟、plan已提交但丢ACK及同ID确认、179/180/181片段、8191/8192/8193总长、JSON完整派发、断链不串接旧片段、1024 Wi-Fi客户端入口、既有数据/Plan/Bundle/OTA/sync回归。代表超限走真实5s上限；其他属性隔离案例采用外层200ms取消预算，没有改生产等待值。

仍未完成：

- P3：非秘密读/同步意图持久化、24h期限、机会触发的重试/退避、待确认先查询/回读、重启恢复和冲突暂停。现有Wi-Fi UI首次失败仍走旧结果流程；稳定ID入口不代表此阶段已实现。
- P2余项：Fake Wi-Fi事务/分页结果尚无对应HAL，INFO报告wifi_setup_protocol=0且命令明确unsupported；生产Wi-Fi状态机另由宿主验证。不能把Fake传输通过当作全量新Wi-Fi同步能力。history、跨bond/session/frozen-page故障还需更完整客户端矩阵；一般内存分配写失败目前是故障模型，不能证明真实库OOM路径。
- P4/P5：全部矩阵逐行最终放行、整板构建、实际NimBLE/CoreBluetooth/GATT值、自然会合RF/维护截止、Windows、功耗与原试连跳屏原因均未验；未打开串口、操作设备、重建/重启运行.app或部署ROM。

首批软件结果：BLE17、render45、Fake45、app49 passed/1 ignored、core119、MCP3，合计278 passed/1 ignored；Wi-Fi宿主HAL全部通过，最终关联帧最大418 B；Node检查 headless runner 语法通过，Windows全流程未执行。证据为既有 `artifacts/bridge-macos/ble-fidelity-verified.log`、`ble-fidelity-application.log` 与 `ble-fidelity-wifi.log`，构建仅复用隔离 `artifacts/bridge-macos/target`；运行数据/原ROM/既有未提交改动保留，没有提交。


## 12. 用户追加授权：Note4 刷机（2026-10-03，进行中）

用户追加“还有刷机”，范围加入当前 Note4 的标准构建、配套 Bridge 测试应用升级及 ROM 部署/身份与镜像确认。只构建 zectrix-note4-b；保留当前 .42 回退镜像、未部署 .43 候选与运行数据。候选版本 0.18.44-note4-b-ble1；P3/P2余项仍未因此实现。登记IP暂时 Host is down，先准备可核验产物，再沿认证OTA或核对USB身份的标准烧录流程部署。

刷机完成：唯一 Note4 构建132.29s；ROM `artifacts/bridge-macos/note4-0.18.44-ble1.bin`，1,697,632 B，SHA256 `56367D6BDBC056B597DE354B05B47320404A2712CBC4F631532A512D1A81D0C8`。USB核对7C4FADB93408后，原.42应用区与回退文件一致；只清otadata并写ota_0，整段读回一致，认证运行前缀哈希也一致。配套.app二进制73,593,616 B，SHA256 `51FBC70C2FDA7774E7DDF38FE4E6D5364800907C259DABB0452B31BAE567F9EA`，原按需托管实例已重启，数据和旧应用均备份。

本次P5最小确认：已绑定/加密后status、plan、sync_open新摘要ACK关联成功并记录会合完成；实机最终值279/233/164 B，INFO315–317 B、endpoint28 B。后续4s连接/3s INFO读取超时仍出现，原会合问题未结案，不能以一次USB供电成功替代自然timer/电池矩阵。认证状态/镜像、USB烧录/日志与Bridge脱敏关联证据见 `artifacts/bridge-macos/ble-fidelity-{running-status,running-image}.json`、`ble-fidelity-usb-{identity,flash,after}.log`、`ble-fidelity-real-ble-summary.log`。P3/P2余项及完整P4/P5未完成。


## 13. W0–W6补齐软件候选（2026-10-03，未部署）

首批之后已接入生产wifi_setup/wifi_management/单blob存储HAL，Fake报告wifi_setup_protocol=1及wifi_management=2，并经真实DeviceConnection属性等待与认证HTTP分页、跨bond/legacy、写后丢ACK/Bridge重启查询闭环验证。48B生产wake记录及compact history投影也已共用；history故障数据明确为合成fixture。P3持久读/同步、24h/机会退避、待确认查询/回读、字段冲突和新版UI已实施，不再将这些记为“尚无代码”。

五crate239 passed/1 ignored，Fake48 passed，总287 passed/1 ignored；最后调度app51/1 ignored及受最后变更影响的Fake Wi-Fi3项复测通过，host最终Wi-Fi418 B/授权196 B。完整命令/A01–A17具体证据范围在 [wifi-rendezvous-execution.md第11节](wifi-rendezvous-execution.md#11-w0w6-软件交付与验收对账2026-10-03未部署)，不把有限软件矩阵等同于全部时序压力或实体P4/P5放行。

W7本轮未执行，实际仍.44/原运行.app；没有整板编译/新包/部署或设备动作。账本8项保守永久保留且满时拒绝，是明确产品限制。真实NimBLE/RF、连续重连/INFO超时、自然会合/TCC/电流、试连实体屏幕跳变、NVS断电、Windows/其他架构/缺席1.54仍未验。一般诊断读任务扩展和完整app进程接Fake的抢锁/系统网络事件压力不在已通过声明内；B5/B6继续未结项。
