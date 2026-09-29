# 设备状态、一次性 Wi-Fi 同步与统一诊断流

日期：2026-09-28。状态：实施合同定稿；下文所有 sync-v1 新字段、端点、状态机和诊断格式均待实现。本文不是部署记录。待办唯一入口为 docs/roadmap/backlog.md C8，执行顺序见 plan.md，断言见 task.md。

## 1. 阅读顺序、范围与依据

空白 context 先读 AGENTS.md、PROGRESS.md 最新节、本文、plan.md、task.md，再定位源码。保留当前工作区已有 RF1 和其它未提交工作，不凭历史文档判断运行版本。早期仅文档阶段的授权边界已由后续用户指令更新；构建、运行和实机访问以当前会话授权及 PROGRESS.md 现场为准，未经要求不提交。

用户确定的范围：

- 设备页移除 Codex 余量及屏幕内容、模板/Profile、字体编辑入口，保留只读设备实际显示和独立的数据投递许可；模板 Tab 本专项不增加对应编辑界面，也不设计迁移布局；数据页排版不改。
- 普通 deep BLE 会合精简；成功 Wi-Fi 同步后第 15 次 deep BLE 会合安排一次性 Wi-Fi，完整冻结批次经 Bridge 持久确认后回 deep，不建立正式 light。
- light 入口与退出各同步，任一次完整成功都清零计数。
- OTA 重启及 Bundle commit 后优先 Wi-Fi 确认，少量有界重试。
- log/history 合为单一跨 deep 诊断流；正常批次不设任意短总截止。
- Bridge 是 HTTP 客户端，设备是 HTTP 服务端。设备通过 BLE 的开网指令/正式计划获得 Wi-Fi 机会；本专项不实现 bridge_first 的设备→Bridge HTTP。
- 软件协议、时序、恢复大部分由 Fake ROM 验证；实机仅补硬件边界。

已核实基线：

| 事实 | 源位置（定稿时，行号可能随实现移动） |
|---|---|
| 当前 timer 只开 BLE，light Plan 后才走 Wi-Fi | src/main.cpp:4596、4600、5724 |
| 15 分钟是 BLE history 取数周期，不是第 15 轮 Wi-Fi | bridge/crates/app/src/wake_history.rs:15；platform.rs:1415、1648、1700 |
| RTC history 64×48 B；文字日志另有普通 RAM 4096 B环 | src/main.cpp:500、552、574；src/dev_log.cpp:3 |
| 命令检查 protocol/MAC/request_id/session_nonce；BLE 要绑定、加密及 endpoint token | src/v2_command_envelope.cpp；src/main.cpp:4091、4518 |
| light/owner、BOOT 和低电软件决定 | src/v2_state.h；src/owner_store.* |
| 已有持久认证快照和尝试记录；UI部分字段只用 public_online | core/src/platform/model.rs:704；app/ui/index.html:1073、1095 |
| OTA上传、版本和精确证明已有不同状态合同 | project-workflow/sleep-aware-bridge/design.md Q3、§5 |
| Fake ROM已有同源业务、真实客户端loopback、虚拟时间和D/E/F出口 | docs/fake-rom-simulator-design.md；docs/history/workflow/fake-rom-simulator-def/evidence.md、status.md；PROGRESS.md 的 Fake ROM节 |

路径中 app/core 默认前缀为 bridge/crates；文档中具体错误码和数值均为本次选定的工程规范，不代表已上线行为。

## 2. 不变量与执行单元

每 MAC 复用现有 coordinator，最多一个活动同步批次；不得另建通用任务框架。不同设备计数、日志、游标、owner、任务完全隔离。

Wi-Fi MAC 为身份；IP、显示名、广播仅是发现属性。保持 endpoint token（业务请求）与 device token（claim/OTA等操作）分域。新同步端点必须有有效匹配 owner；owner为空返回 claim_required，由现有显式 POST /claim 建立，不能靠 begin/data/读取隐式占用。BLE开网可沿用现有空闲owner bootstrap例外，但只给网络机会；有效他人owner拒绝。所有命令不隐式续 owner/light。

Profile仍为1–8有序项，全参与循环；保存不发布。同步不自动上传模板/OTA，也不自动把所有排队工作加入批次。Data仍按完整快照和原ACK推进指纹/full_sync_deadline；诊断完成不能替代Data ACK。

状态分三层：

1. 正式 PowerPlan / BOOT provisional：原有业务期限。
2. 执行阶段：DEEP、RENDEZVOUS、WIFI_CONNECT、WIFI_SYNC_ONCE、WIFI_LIGHT、SYNC_ABORT。
3. 同步义务：periodic、light_enter、light_exit、ota_confirm、bundle_confirm，可合并原因，但一个批次只完成冻结时已存在的义务。

“完整同步”限定设备→Bridge的冻结状态、诊断和确认观察；HTTP请求仍由Bridge发起。新增授权业务与后续日志不无限加入当前批次。

## 3. 轮数、开网授权与优先级

设备RTC保存 rounds（0–15饱和）、retry_skip、failure_count、due、wifi_retry_pending、未完成原因位、当前唤醒是否已计数；valid magic+CRC保护。定义：

- 仅 deep timer 路径实际进入BLE rendezvous窗口时加一。同一窗口连接/命令重试不加；无连接仍加。
- thin clock、light循环、按键/冷启动进入light、OTA重启不算deep BLE轮。
- 完整成功时 rounds=0、due=false、wifi_retry_pending=false、failure_count=retry_skip=0；仅清冻结时包含的原因位，后来新义务仍保留。下一deep BLE为1。
- 第15轮置due；Bridge在本轮认证状态见due后发sync_open。如本轮认证失败，绝不自行开Wi-Fi；due保持，下一个满足退避的认证会合开网。UI表述“第15轮已到期，等待认证开网”，不能承诺离线PC也能同步。
- light入场成功发生在第14轮时立即归零；其离场成功再次归零。重复light Plan不是再次入场。
- Bridge重启不改设备轮数。deep保留；RTC无效/冷断电置rounds=15、due=true、baseline=unknown，首次成功后恢复known。无逐轮NVS写入；软件重启若RTC校验失败同样处理，不声称准确延续原轮数。
- 有持久未完成批次时先续旧批次；成功后若有旧批次冻结之后新增的light_exit/OTA/Bundle义务，再冻结一次新批次，不能用旧状态证明新事件。
- 最高优先级：低电/用户物理关机、认证/owner边界；其次旧批次恢复；再OTA/Bundle确认、light_exit、light_enter、periodic。相同可达机会合并未冻结原因，避免重复建连。
- 若第15轮同窗收到有效light Plan，复用该Wi-Fi连接作light_enter+periodic，只冻结一次。
- 同步中收到新正式light Plan可正常接受；先完成旧批次，再执行新light入场义务。Light截止后新计划仍可改变正式计划，同步本身不改变它。

同步功能通过下面的BLE配置显式协商；已登记设备升级后由Bridge自动配置，是协议维护，不等于授权模板发布。原sync_enabled控制Data投递，不禁用本专项状态/诊断同步。当前设备业务协议的一次性统一见 `protocol-unification.md`，本节使用统一后的 wire 字面。

## 4. 认证、信封与能力

新增能力在认证GET /api/status及BLE status中报告：

sync_v1=1、diag_format=1、diag_capacity=4096、image_identity=["sha256-running-prefix-v1"]。

BLE status仅新增紧凑sync对象：v、enabled、rounds、due、retry_skip、pending（布尔）、completed_serial（十进制字符串）；普通BLE不包含诊断页、完整Wi-Fi详情或镜像摘要。升级窗口内的旧ROM缺此能力，生产切换后不作为可用业务目标。

HTTP命令共同信封：device_mac（规范化MAC）、bridge_id、session_nonce、request_id（1–64 ASCII字符）；同步端点另需sync_version=1作为批次格式。所有POST的Content-Type为application/json，body最大4096 B。Authorization: Bearer <endpoint token>；复用当前endpoint匹配方法，不接受token查询参数。BLE沿用绑定+加密peer与JSON token核验，并使用同一身份信封；`rv=2`/`protocol=2` 旧帧须在副作用前拒绝。重启后先status取新nonce；持久操作键不依赖nonce。

SHA256统一为64位小写hex；Base64使用RFC4648标准字母表和padding、无空白。device_mac除规范化入口外必须完全匹配，不按前后缀匹配。所有成功HTTP响应含op（sync_begin/page/ack/complete/arm/image）、result（applied或already_complete）、request_id、device_mac、session_nonce、sync_version=1及端点结果字段；page使用result=applied，ack另含batch_id/acked_offset，complete另含receipt（§5.5字段），arm另含ticket/job_id。未知JSON字段可忽略，但退役的平台 `protocol`/`rv` 字段必须拒绝且不能影响幂等键；缺必填/类型错/枚举错返回shape。request_id只关联一次请求，batch_id/client_serial才是持久幂等键。

新命令的检查顺序：token→JSON/尺寸→MAC及信封→owner→能力/参数→幂等/状态→副作用。旧端点的错误顺序不改。错误统一result=rejected、error=<code>、op、request_id；已通过身份检查的响应包含device_mac/session_nonce。HTTP码：

| HTTP | error | 行为 |
|---|---|---|
| 400 | json、shape、session、mac、range | 本请求无副作用；session错误先取新status |
| 401 | unauthorized | 停止尝试，不能自动换token |
| 409 | occupied、claim_required、disabled、batch_conflict、stale_serial、digest_mismatch、owner_changed | 不改游标或计数；按具体原因处理 |
| 413 | body_limit | 拒绝超限体 |
| 422 | capacity、unsupported_identity | 不能生成合法批次或不支持证明 |
| 503 | storage、low_battery、temporarily_unavailable、batch_lost | 明确未完成；按异常合同 |
| 404 | unsupported | 新端点不存在；Bridge按旧能力处理 |

BLE无法表示HTTP状态时，在ACK增加http_status与同名error。新BLE响应仍受既有512 B限制；不得通过裁剪关键字段伪成功。

### 4.1 BLE配置与开网

BLE op=sync_config，参数 enabled:boolean；仅当前owner可设置，落NVS成功才applied。绑定当前bridge_id；新owner须重新配置。enabled=false不删除未确认批次，只停止自动新同步。Bridge UI首版不增加配置开关；该命令用于版本协商/回退。旧Bridge没有此命令，新ROM默认disabled，原行为继续。

BLE op=sync_open，参数 reason=periodic|retry，open_id（32hex）、wake_generation、wake_seq。periodic仅rounds=15且retry_skip=0接受；retry须有pending批次、wifi_retry_pending或未完成的light边界义务之一，且retry_skip=0。未到begin就失败也置wifi_retry_pending并保留对应reason位，不能等15轮才恢复。相同wake+open_id+reason重复返回原结果，不重启无线/任何期限；不同内容同ID返回batch_conflict。指令仅本醒次有效，旧wake/session拒绝，不持久成未来开网命令。边界义务位跨deep保留在RTC；冷断电未知边界由baseline=unknown和新代际gap表达，不冒充已完成。

同步enabled且目标匹配时，收到sync_open后先ACK，沿用200ms收尾关BLE，再开设备HTTP。有效正式light Plan仍可合并该机会。空owner只允许既有已绑定、有效device-token bootstrap路径取网络再claim；不能为了实现本命令放宽现有信任边界。若现有空owner bootstrap需要light Plan，沿用该路径，不把sync_open当claim。

light入/离场复用已授权Wi-Fi会话。OTA重启使用§9预先持久arm；Bundle在已打开HTTP连接报告。按需“无due也请求下一轮Wi-Fi”不实现：返回range，不顺带扩展用户授权。

### 4.2 HTTP端点

全部方向为Bridge→设备。GET /api/status保留旧形状并增同步摘要。完整详情只由以下begin冻结，避免普通BLE增肥。

| 端点 | 参数（除共同信封） | 结果 |
|---|---|---|
| POST /api/sync/begin | client_serial:string，reasons:[枚举] | 冻结/恢复批次manifest |
| POST /api/sync/page | batch_id、offset:uint32、limit:uint16 | 原始冻结文件字节页 |
| POST /api/sync/ack | batch_id、offset:uint32、prefix_sha256 | Bridge已持久连续前缀确认 |
| POST /api/sync/complete | batch_id、bytes:uint32、sha256 | Bridge整批持久确认，设备原子收据 |
| POST /api/sync/arm | job_id、kind=ota、image_bytes:uint32、file_sha256 | 持久化一次OTA后确认授权，返回ticket |
| POST /api/sync/image | image_bytes:uint32 | 当前运行分区前N字节SHA256，见§9 |

GET /api/status新增sync详情：enabled、phase、rounds、due、baseline、pending_batch（manifest或null）、last_completed（收据或null）、last_error、retry_skip、confirmation_pending。HTTP可完整；BLE只发前述轻量投影。

## 5. 冻结文件、序号、页面与持久ACK

### 5.1 设备存储

只保留一个逻辑诊断流：4096 B RTC字节环。为保证批次在中断、软件重启和冷断电后仍可续传，begin时把冻结集合复制为一份不可变LittleFS文件；这份副本是同一序列的传输快照，不是第二日志生产者或独立history。

设备预留40 KiB空间：active frozen文件最多16384 B、同大小临时文件、两份元数据各最多2048 B及余量。现有Bundle/字体空间检查必须扣除该保留额度；空间不足返回storage/capacity，不删活跃Bundle或旧批次。元数据A/B带单调revision+CRC；先写inactive、close并回读验证，再选择最高有效revision。blob先写临时、close/回读hash再写元数据引用；启动删除未被有效元数据引用的孤儿，不删除活跃文件。写入/断电测试必须使用同源存储路径，不能假设rename天然跨掉电原子。

这是本次定案的Flash使用：普通日志、轮数和阶段不写Flash；每次新批次写一次冻结文件，每次成功/arm更新小元数据。15分钟仅是示例节奏，不承诺Flash寿命；记录实际bytes/write次数，只有寿命实测需要时另优化，不能先改协议。

RTC总新增环4096 B，控制/当前醒次结构合计上限768 B（不含项目原有其它RTC对象）。编译static_assert两项；两目标最终map必须可链接并保留至少512 B RTC SLOW空余，否则容量设计未通过，禁止静默缩环。当前本地Note4 map显示rtc_slow长度0xF90、区域0x1E00，仅为旧产物线索；1.54当前map未找到，不能声称双目标已验证。

### 5.2 冻结内容

文件是UTF-8无BOM JSON，保存并传送原字节；hash为文件全部字节SHA256，不能由两端重新序列化后比较。最大16384 B，manifest值最大6144 B；诊断原二进制最多4096 B，Base64编码放records_b64。结构：

format="device-sync-1"；device_mac；bridge_id；batch_id；client_serial；reasons；snapshot；confirmation_observations；diag={generation,from_seq,through_seq,gaps,records_b64}。

batch_id为设备持久sync_serial十进制串加32hex随机后缀，设备每次新冻结前持久推进serial；不依赖wall时间唯一。所有u64序号在JSON中用十进制字符串，避免JS丢精度；时间毫秒受安全整数范围限制。

冻结时原子取得当前完整状态、确认观察、环高水位H及从已确认后至H的保留记录/丢失区间。若当前generation已变化，在gaps中记录previous_generation_lost及旧可知高水位；不得伪造未知数量。当前醒次尚未发生的sleep结果不在批次内；freeze后新增日志下一批。

完整状态包括target/fw/slot/reset、active/context/job/commit_seq/display_state、data/applied_seq、owner摘要、电源/连接、heap/EPD计数及各组采样信息。镜像证明仅有待OTA且请求成功时附上；不为每次周期批次计算镜像hash。

begin先对账pending OTA/Bundle并取所需image观察，再冻结。一次确认查询达到§8失败门槛仍可冻结confirmation_observations中的明确pending/error事实；这样的同步可以传输完成，但OTA任务本身仍待证明，不得把两种成功混合。

begin响应manifest：result=applied、batch_id、client_serial、bytes、sha256、diag_generation、from_seq、through_seq、acked_offset、reasons。再次begin遇已有同owner批次一律返回它，不另冻结。reasons只能声明实际存在义务；Bridge不能伪造light_exit来无条件开网。

### 5.3 持久幂等键

Bridge按MAC+bridge_id持久client_serial（从1开始，u64字符串），新逻辑批次前先落盘；同批恢复仍用原serial。设备存当前owner的最高client_serial及最新完成收据：

- 同serial且活动：返回原批次。
- 同serial且最后完成：返回result=already_complete和收据，不生成新批次、不再次清零。
- 较小serial：stale_serial；Bridge先status对账。
- 较大serial而存在活动批次：返回活动manifest，Bridge先续旧批次，尚未使用的新serial可留下一批。
- 无活动且serial较大：创建。幂等内容包括owner+serial+冻结前的义务集合；变更session_nonce/request_id不改变持久键。

owner转移时旧owner批次不能交给新owner：标记owner_changed、旧流未交付范围gap后释放旧传输快照；新的owner重新从可保留环记录开始。gap不是“旧批次传完”。返回写权限始终以当前owner为准，不用缓存收据恢复写权限。

### 5.4 分页与durable cursor

page的limit默认1024、合法1–1024；offset范围0..bytes。返回batch_id、offset、next_offset、data_b64、chunk_sha256、more。offset=bytes返回空页/more=false。每页读取不可变文件，不因读取推进任何cursor。

Bridge在实例data/platform/diagnostics/<MAC>/下保存<batch_id>.part、最终<batch_id>.json及checkpoint.json；流事件索引可从这些文件重建，不再额外存另一份持续追加wake-history。checkpoint记录MAC/bridge_id/batch/generation、durable_offset、prefix_sha256、状态。写顺序：

1. 验页身份、范围、长度、chunk hash；仅接受紧接durable_offset的页，重复已存页逐字节一致才忽略。
2. 写part并sync_all，再原子替换checkpoint并sync_all；成功后才能POST ack。
3. ack携带连续前缀长度与SHA256；设备流式验证该前缀。设备acked_offset只作RTC/当前进程优化，断电丢失可重发；绝不据page/ack释放源记录。
4. Bridge崩溃恢复以实际文件连续有效前缀为准，checkpoint不能超前；尾部不完整截到已验证边界并续传。从设备acked_offset也不能推断Bridge当前文件仍存在。
5. 整文件hash通过、JSON/记录序号/gaps校验通过后，原子转最终文件并持久化所有确认观察/状态缓存/完成意图，才发送complete。

Bridge数据库不引入；使用既有原子文件模式。事件唯一键MAC+diag_generation+seq；同键同字节去重，同键不同字节视protocol_conflict，禁止覆盖。游标只能跨过收到的连续记录或明确gap；gap单独可见。

### 5.5 完成点与断电恢复

complete必须bytes/hash匹配活动文件；设备先持久写完成收据（batch_id、owner、client_serial、bytes/hash、generation、through_seq），再在RTC仅推进匹配generation的acked_seq、归零rounds，最后返回result=applied。同一已完成ID重试返回already_complete，不能再次归零后来已增长的轮数。设备采用RTC last_reset_serial保证一次性清零；软件/冷重启只恢复完成收据用于对账，不据历史收据伪造当前计数。

设备收到complete表示Bridge声明“已持久保存”，不必等待ACK的ACK。回复提交本地网络栈后可正常退出；若回复丢失，Bridge保留awaiting_device_ack，下次status见对应收据即完成。已落盘收据允许清理冻结文件；清理失败只记错误，不撤销完成。blob缺失/损坏且没有完成收据则batch_lost：明确缺口、保持due，不伪完成。

Bridge丢了已声明持久的本地文件是Bridge数据损坏：不得要求设备一定仍保留；报告archive_lost。数据保留上限每MAC最近128批或32MiB，先达到者触发仅清理已终态最旧文件；永久保留一个有界retention_floor摘要和最近收据，不回退cursor请求全部历史。

## 6. 统一诊断记录格式

stream generation为随机128bit、32hex，RTC校验失败创建新值，deep保留。seq为u64，从1递增且给被丢弃事件也分配序号。记录Little Endian：

len:u16（包括头与payload及CRC），kind:u8，flags:u8，seq:u64，wake_seq:u32，uptime_ms:u32，payload:0..96B，crc32:u32。固定头20 B，尾4 B，总长24..120 B；CRC32/IEEE覆盖CRC前所有字节。generation在环header和批次中，不重复写每记录。环操作使用短临界区，不在锁内分配String或写Flash。

kind固定：1 wake_summary、2 event、3 text、4 sync_result。flags bit0=text_truncated，bit1=time_unknown，其余必须零；未知kind可存档但不解释，未知format拒绝。text payload为UTF-8最多96 B，不切断码点，串口亦输出脱敏后的同一内容；一条过长文本生成一条truncated记录，不无限拆片。禁止将token/密码/认证头/原始账户载荷写入；src/main.cpp现有AP密码日志点必须删敏感字段。

event payload：event_code:u16、argc:u8（0..4）、reserved:u8=0、args:argc×i32；code在同源头文件集中定义，不在Rust另抄。固定code/参数如下，参数数目不符视格式错误：1 radio(state:0off/1BLE/2WiFi)；2 wifi(stage:0attempt/1associated/2IP/3lost,error)；3 auth(http_status)；4 owner(result:0claimed/1renewed/2expired/3conflict)；5 data(result:0applied/1unchanged/2rejected,seq_low32)；6 plan(mode:0sleep/1light,granted_s)；7 bundle(result:0committed/1rejected,commit_seq_low32)；8 ota(stage:0armed/1accepted/2reboot/3confirmed/4failed,error)；9 display(result:0displayed/1pending/2failed,duration_ms)；10 sync(result:0begin/1complete/2incomplete/3blocked,error)。seq_low32仅辅助检索，完整身份来自业务快照/Bridge，不能据低32位确认任务。

wake_summary为旧WakeTraceRec的48 B语义载荷（保留阶段/耗时/CRC字段兼容解释，内含的32位wake_generation仍为旧唤醒关联字段，不能替代外层128位diag generation）；整体仍走本格式，当前wake只在工作结构累计，收尾追加不可变summary。sync_result payload固定result:u8（0complete/1incomplete/2blocked）、reason_bits:u8（periodic/light_enter/light_exit/ota_confirm/bundle_confirm依次bit0..4）、error:u16、batch_serial:u64，共12 B。error数字统一：0none、1association、2http_timeout、3no_progress、4unauthorized、5occupied、6storage、7low_battery、8corrupt、9capacity、10owner_changed、11unsupported；文字错误补短text。event中的error复用此表。

正常deep轮默认记录一条wake_summary；其重复说明文字改由摘要解码生成，避免每分钟重复printf挤占环。正常必要增量最多3条无长文本event；15轮正常夹具上限3600 B。异常文字和长light可溢出，不能声称无限全量无损。只去除重复表达，错误及关键阶段不得为凑容量静默关闭。

RTC环header维护generation、next_seq、acked_seq、earliest_seq、累计lost及最多8个gap区间；gap字段为from_seq、through_seq、reason（overflow|corrupt|reset|migration|owner_changed）、coalesced布尔，端点均包含。超过8段时合并为保守的大区间并标coalesced=true：合并范围内幸存记录也从本次传输集合移除，明确算丢失，使记录与gap互不重叠，不把其中记录算已收到。腾空间优先回收已确认记录，其次可回收已经在持久冻结文件中的副本；仍不足才覆盖最旧未持久记录并记gap。begin后冻结副本不可改，新日志只写环；冻结副本保护本批完整性，环自身溢出属于下一批gap。空增量用from_seq=acked_seq+1、through_seq=acked_seq、records为空且gaps为空，客户端接受该唯一反向空区间。

CRC坏记录记corrupt gap；RTC header失效是新代际，不从随机内存恢复。冷断电丢失未冻结RTC数据属于明确限制；冻结文件仍续原generation，新环新generation排在下批。/log和/history只作同流派生兼容视图，没有独立生产者/游标。公开接口不得暴露新增加的敏感详情；正式归档只走认证sync协议。

## 7. light期限与完整批次

正式light deadline只有正式PowerPlan修改。读取、传输、claim/renew、重试不续它。timer sync不获得BOOT provisional。

入口同步使用当前连接；离场时停止接纳新Data/Activate/Bundle/OTA写事务，冻结light_exit义务。已正式提交的操作先对账，未提交原事务依自身规则终止。sync端点、认证status、必要显式claim/renew和新正式Plan仍可处理。

若light/BOOT截止到达时同步进行中，phase切为WIFI_SYNC_ONCE，power.plan.mode/remaining仍报告真实原计划，effective_radio_reason=sync_drain。正常有进展的冻结批次直到complete才退出，不能为了旧light截止截断或伪续light。此规则明确替代总设计§7对同步批次“到点中止并立即关网”的旧描述；其它业务事务仍有原截止。

处理请求、重复相同页/ack或status不能无限保活：仅首次有效begin、连续新页被Bridge确认持久（acked_offset增加）、以及complete算同步进展。首个begin等待及异常无进展采用§8。准备镜像hash/文件持久化的本地循环必须喂看门狗、记录阶段，但本地重复循环不能当无限网络进展。

同步未开始时light到期仍允许一次离场同步机会；无Bridge请求即按无进展异常退出。用户物理关机/低电优先，无须等批次。明确UI/MCP“立即sleep”仍走离场同步，显示正在收尾；硬中止只由用户物理关机或安全异常触发，避免隐含新软件强制取消语义。

## 8. 有界异常，不截断正常批次

定案初值：Wi-Fi关联单次15s、最多3次，间隔1s/3s；HTTP单请求Bridge端15s；连续3次请求失败进入本轮异常。请求有响应但cursor不推进，设备与Bridge均以90s单调“无进展”判失败；正常每90s内至少有一个新持久页的批次无总时长上限。单请求超时是可重试异常，不是整个批次成功截止。Fake ROM的外部进程wall watchdog单独分类，不能混入上述设备逻辑。

不在同一醒次无限重开：本轮最多上述3次建连/请求尝试。失败保留blob/序号/due，记sync_incomplete及reason。后续自然BLE窗口退避：第1、2、≥3次失败分别跳过1、2、4个完整窗口；设备每个窗口开始先消耗skip，本窗口消耗到0仍不重试，下一窗口才可sync_open。成功清零；新的用户物理唤醒、OTA/Bundle确认义务可绕过退避一次，其失败仍受同一门槛。401/409则blocked，不自动反复写；仅owner/token实际恢复或显式操作后恢复。

电池未插电且battery<5%沿用现有v2BatteryPowerOff，立即保护关机；Wi-Fi尝试前和每页/长计算分块检查。battery不可读时报告unknown并沿用硬件现有安全策略，不伪造百分比。低电中断不清零、不删除pending。不能拿低电模拟输入证明实际ADC和供电正确。

owner过期时暂停业务，允许token保护显式claim；90s无进展仍生效。不同owner接管按§5.3报告owner_changed，不向新owner泄露旧冻结状态。Bridge磁盘失败不ack；blob生成失败不begin成功；设备storage失败不complete成功。每种异常须在状态及下一可交付诊断中可见。

## 9. OTA/Bundle确认与真实镜像证明

> 2026-09-29 OTA 修订：下述 `sync/arm`、ticket 与持久 `confirmation_pending` 是已部署旧 ROM 的过渡合同，不再适用于 `ota_auth=token` 新 ROM。新 ROM 只要求 BLE 签发的设备操作 token，直接调用 `/doUpdate`；`/api/ota/image` 仅凭该 token 返回运行分区前缀 SHA256，不依赖 sync owner/诊断批次。Bridge 对旧 ROM 保留 arm 路径直到升级完成，对新 ROM 直接上传并独立验证运行镜像。设备端不要求项目 marker；正常 Bridge 的 MAC/目标/marker/SHA256 预检继续保留。完整变更与部署证据见 `ota-recovery.md`、`PROGRESS.md` 最新节。

### OTA授权及回报

Bridge在已获BLE light计划的Wi-Fi会话、上传前调用sync/arm，保存job_id、image_bytes、file_sha256及当前owner；设备持久返回ticket（32hex）。新协议OTA上传附X-Codex-Sync-Ticket，仍需原device token、target及owner门控；无/错token保持401。ticket只能绑定本次owner/job/长度/hash，不是写权限凭证。

上传完整接受并校验成功后，先把arm变为pending_reboot并持久化，再返回UPDATE OK并沿用原延迟重启；失败上传取消arm。仅armed但未pending的普通复位清除arm；pending在重启后消费为一次Wi-Fi确认机会，属于先前BLE/HTTP明确授权的后续步骤，不是设备主动HTTP。Bridge发现同MAC后重新认证；新nonce必需。第一次后续开网失败保留confirmation_pending但不在本启动无限重开，下一BLE按retry/已授权light恢复。

少量立即重试采用§8；job查询、image请求最多首次+两次，不重复上传。若这些查询失败但包含pending/error观察的整批已传输成功，保留OTA confirmation_pending，等下一次正常periodic/light Wi-Fi机会查询，不能仅凭confirmation_pending每轮重新开网；如果整轮网络同步未完成，才置wifi_retry_pending并按退避续传。任务仍保持upload_ack / version_observed / image_verified等级；同版本或旧版本未知不作版本变化证明。真实状态不支持身份算法仍待确认，不能凭sync_complete判OTA成功。已有sleep-aware Q3及允许后续独立业务解除阻塞规则不改。

### 精确身份算法

选用sha256-running-prefix-v1：请求image_bytes=N，设备从ESP当前运行的OTA分区offset=0读取恰好N字节，分块4096 B计算SHA256；N必须>0且<=实际运行分区size，超限range。响应algorithm、image_bytes、sha256、running_slot、fw_target、fw、session_nonce。同boot+N可RAM缓存；跨重启失效，只在待OTA确认时查询。

Bridge比较冻结上传文件的全部N字节SHA256、长度、target与同MAC新认证session。算法明确证明“当前运行分区前N字节等于上传文件”，不声称测量芯片执行流或安全启动远程证明。N由冻结文件而非设备自报截短；Bridge仍先验证ROM格式/marker/target。设备不能回显上传时保存的file_sha256，必须读取当前运行分区；不读暂存/非运行槽。此方案避开ESP image内置digest与整文件覆盖范围不同的问题，不把两个不同算法字段直接比较。

Fake ROM用已激活目录字节模拟读取，Bridge仅看设备面响应；真实bootloader/分区选择必须由硬件烟测证明。实现期核对当前ESP-IDF API和Flash加密读取语义；若无法取得与上传文件同一字节域，不能启用该能力，保留unsupported_identity与待确认，而不是换算法仍沿用名字。

### Bundle

commit后沿现有Wi-Fi读取认证committed_job_id、commit_seq、active_context、template_ids和display_state，优先冻结bundle_confirm批次；已有活动旧批次先完成再冻结本次结果。commit applied不等于displayed，failed/pending保留原显示结果。ACK丢失先查原job，不发新job。不存在新的自动publish；恢复仍遵循每MAC任务顺序与1–8完整Profile。

## 10. 设备状态与UI合同

沿用DeviceRecord.last_authenticated扩展有界完整Wi-Fi快照和组级metadata，不另建全局设备缓存。组为identity、firmware、radio、runtime、display、power、jobs；每组包含received_at、sampled_boot_id、sampled_uptime_ms、sampled_wall（null可）、transport、quality。

字段：

- identity：MAC、target、显示名来源（Bridge配置）。
- firmware：fw、slot、reset_reason、ABI与支持能力；镜像证明独立任务字段。
- radio：Wi-Fi关联/RSSI/SSID（默认不持久SSID原文，显示已连接网络；显式本地诊断可查看）、BLE连接观察。
- runtime：heap_free、heap_min、uptime。
- display：EPD写入/失败计数、active/context、display_state。
- power：battery、formal plan ID/mode/remaining、phase、radio_reason、rounds/due。
- jobs：OTA/Bundle观察与原job ID。

BLE只更新实际包含的轻量组字段；不刷新整个Wi-Fi组年龄。字段缺省=本响应未采样，显式null+reason=本次不可用；reason为not_sampled|unsupported|not_applicable|read_error。无历史值显示对应原因；有历史值保留原时间。0/false/空数组有效。quality=observed|stale|unavailable；没有统一TTL把所有值清空：radio/runtime/power年龄>120s标stale，firmware/identity不自动过期但始终显示采样时间；已知新boot使旧runtime/radio立即stale。

失败更新last_attempt，不改last_success/原时间。wall倒退标clock_anomaly，超时用单调时间；无可信wall只显示收到于/本次启动采样。公开status/ARP只作候选，不覆盖认证缓存。界面读本地快照，不为绘制页面隐式开Wi-Fi或续计划。设备页删Codex余量及屏幕内容、模板/Profile、字体编辑入口，保留实际显示只读信息与设备级数据投递许可；模板 Tab 暂不增加对应界面，数据页布局不动。

## 11. 一次性切换、恢复与能力

新ROM默认sync disabled；旧Bridge与新ROM错版只允许发生在两台设备逐台OTA的受控窗口。旧HTTP业务路径不存在，旧BLE的 `rv`/`protocol` 帧被拒绝；旧Bridge可继续给尚未升级的另一台设备服务，不把新设备的暂时不可管理状态当同步成功。两台新ROM均经身份/版本观察后才切换生产Bridge。新Bridge只使用当前 `/api/*`，认证能力不满足即拒绝管理，不恢复旧BLE history业务。切换与回退次序见 `protocol-unification.md`。

已启用设备遇同owner旧Bridge时，90s无进展保证异常退出，pending不丢；Bridge回退需逐台回退旧ROM后才恢复旧业务。新owner必须重新配置。配置disabled保留已冻结数据供未来恢复，不借兼容路径泄露给旧Bridge。

RTC使用独立diag magic/schema，不修改其它RTC保留域的magic。旧history整体结束为旧代际；无法证明恢复的记录报告migration_gap，不解释成新格式。旧JSONL留为legacy只读归档；新流不再写它。新增持久字段带默认值，不删除设备/Profile/contexts/plans/keys；不得删target/debug/data。

不改GATT特征表，复用现有命令特征；若实现确需改表，视为偏离本合同，先更新配对缓存迁移计划。Bridge按data根隔离命名实例，所有新文件在实例data中。

## 12. Fake ROM主验收与明确限制

已有真实同源C++设备业务、DeviceConnection loopback、分片和独立时钟；D/E/F历史出口不是新功能证据。device-sim目前power.light才开放HTTP、timer窗口模型固定5s，runner只对light做HTTP且BLE失败直接终止，必须补接sync phase、预期故障、未来I/O事件和跨deep诊断。

新增决定由固件/宿主共享小切片，不在Rust复制状态机；读取Flash/RTC/无线的宿主适配可模拟。控制面只注入环境和校验真相，Bridge只走设备面。所有u64串、文件字节、CRC/容量、序号及错误顺序同源对拍。

**生产与 Fake ROM 的边界（主代理 2026-09-28 核对）：** `v2_sync` 的诊断环、轮数及完成状态和 `v2_sync_store` 的冻结存储已适合两端共用；冻结文件格式以及 begin/page/ack/complete 的状态、幂等和错误判定也必须经同一个生产 C++ 协议入口。固件 HTTP handler 留下 WebServer、token/owner/session、真实快照、随机源与无线操作；Fake ROM 留下 HTTP/BLE loopback、虚拟时钟和物理可达性、按目录存储与故障注入，不另写协议结果。当前生产 `src/main.cpp` 与 `bridge/crates/render/src/sim_sync.cpp` 对这些协议决定各有一份实现，是实施中需要收敛的现场，不是已验收能力。每个 Fake ROM 进程只模拟一台设备时，现有进程全局 `v2_sync_store` 可保留，但必须在同步存储初始化前显式绑定该设备 `dataDir` 并证明多设备进程互不串目录；无需为此先引入多实例存储框架。`SimPower` 与 runner 的环境调度可以保留，不得另算 sync 的 rounds/due/retry/complete。核心单测不能替代经真实 Bridge 客户端与 Fake ROM HTTP/BLE 路径的集成证据。

主要出口是task.md S01–S14及双target可重放虚拟24h场景；至少一次Bridge和设备真实进程kill/restart。实机只验RF/GATT、真实RTC/Flash/分区/bootloader、物理按键/面板和必要电池端积分；软件通过不冒充硬件。

## 13. 决策状态

本文已给实现必需的端点、认证、序号、空间上限、持久顺序、计数、异常阈值、能力回退和断言；没有待用户选择的产品问题（Clarification required: none）。

以下是实现门槛而非可偷偷默认的成功：双目标RTC map、40KiB文件预留、当前IDF运行分区字节读取语义、Flash写边界故障恢复。任一不通过必须明确报告对应门槛并修订实现/合同，不能缩容量、删确认或宣称硬件通过。

工程取舍：4096B RTC统一流+仅冻结时写Flash，换取跨重启批次恢复；每页1024B避免1.54大JSON内存压力；90s无进展/三次尝试阻止失败无限清醒，同时不限制持续推进批次总长；运行分区前N字节hash定义明确的整文件对应关系。按需下一轮开网仍不实现，未来候选不混入首版。
