# 执行任务与可执行验收断言

状态：2026-09-28设计定稿；下列测试尚未为sync-v1实现或运行。待办总入口docs/roadmap/backlog.md C8；本文件是该条目的执行分解，协议字段/参数以design.md为准。

## 模块实施顺序

- [x] P0 定稿Bridge→设备HTTP、端点与状态合同、Fake ROM职责。
- [ ] P1 同源diag v1/4096B环/CRC/gap/脱敏；LittleFS冻结文件与双元数据；接FFI和Fake ROM。
- [ ] P2 认证能力/sync_config；BLE轻量投影；完整状态来源和持久缓存。
- [ ] P3 begin/page/ack/complete；Bridge连续持久cursor与serial；故障切点、runner同步调度。
- [ ] P4 sync_open与15轮计数；light入/离场；异常/低电/退避及跨MAC。
- [ ] P5 OTA arm/ticket/运行分区hash；Bundle确认；三次有限确认重试。
- [ ] P6 先完成 Fake ROM 接入、兼容、S01–S14及可重放24h软件出口；再按 `device-tab.md` 完成设备 Tab M01–M17/U01–U20；相关总设计/PROGRESS更新。
- [ ] P7 仅后续授权后执行H01–H04所需最小硬件检查。

执行顺序：先完成生产 sync-v1，再做 Fake ROM S01–S14，最后按 `device-tab.md` 做设备 Tab U01–U20。P3 的共用协议入口和 Fake ROM LittleFS `dataDir` 显式绑定提前处理；不等到完整模拟器接入后再消除双写。

- [ ] P3 提前门槛：固件 HTTP handler 与宿主模拟适配调用同一 C++ 冻结格式及 begin/page/ack/complete 状态/错误判定；对同一请求和快照，输出批次原字节及协议结果一致。`sim_sync.cpp` 不保留第二套幂等、游标或完成规则。
- [ ] P3 提前门槛：Fake ROM 一进程一设备时，同步存储初始化显式绑定该进程 `dataDir`；两个设备进程隔离、重启后恢复同批次。进程全局 store 可保留，不为此新增多实例框架。
- [ ] 第二阶段集成门槛：`SimPower`/runner 只提供虚拟时钟、无线可达性及故障注入，不另算 sync 的 rounds/due/retry/complete；真实 Bridge 客户端经 Fake ROM HTTP/BLE 运行 S01–S14，核心单测不能代替。

新增纯协议逻辑集中src/v2_sync.*，沿现有render FFI接宿主。不要在device-sim Rust复制counter/ring/幂等决定；硬件读写适配的行为与异常单独标注。现有snapshot/owner/Plan/Bundle逻辑继续复用，不为本专项全面重构main。

## 测试公共设施

运行前隔离目录提权预建；固定虚构LAA MAC A=0200000000A1（154g）、B=0200000000B2（Note4）、独立端口/token/data。控制面凭据与业务token分开，日志不输出secret。隔离Bridge使用已有协作模式，真实目标时钟不被推进。新测试位置优先现有device-sim/tests/bootstrap.rs、core/app模块测试及tools/fake-rom-runner.mjs场景；慢集成案例可拆有名场景文件，不另引入框架。

所有场景记录seed、设备/Bridge单调和wall时钟、boot、diag_generation、wake/record/batch/client_serial、ACK offset、deadline、业务副作用数。模拟控制面可作断言oracle但Bridge禁止读取它。页面/文件断言基于真实字节和fsync后的文件，不只看内存JSON。

每个S类别至少一个正例和一个可区分错误实现的负例。不是对所有错误做笛卡尔积。需要故意等待/丢包的场景须声明expected failure；runner不许将其整体skip，须断言随后恢复状态。

## S01 逐轮计数和BLE开网

（S01–S14的运行接线、请求参数和具名测试入口见文末“Fake ROM操作规程”；其中标“待实现”的控制接口必须先完成，不能把当前不支持的调用当作测试通过。）

从一次真正complete后的rounds=0开始，逐事件执行14次deep timer会合；每次i断言rounds=i、周期Wi-Fi建连数=0、日志/image请求数=0。第15次认证后Bridge发sync_open一次；正式plan deadline不变，完整成功后rounds=0，实际HTTP服务关闭返回deep。

第15次无BLE认证：rounds=15/due=true、Wi-Fi数仍0；下次有效会合才开。thin唤醒与同窗重复连接不增加；重复open_id不会重复建连。以wall推进15分钟但零会合计数不动，反向证明不是15min定时器。

## S02 light入口、离场与触发合并

先完成14轮，发真实light Plan，入场批次reasons含light_enter；complete后0。期间重复Plan不产生新入场批次、不续deadline。到期离场产生light_exit，完成后deep且0；下一timer为1。

第15轮与light Plan同窗，仅一次Wi-Fi连接、一个含periodic+light_enter的批次。已有旧批次时先完成旧serial，再冻结离场新快照；不能用旧batch清掉后来light_exit义务。

## S03 有进展慢批次不设短总截止

冻结已知H，limit=64形成>16页；每10s完成一页durable ack，总时长>原light剩余和120s。每一步断言原正式deadline完全不变、phase=WIFI_SYNC_ONCE且effective_radio_reason=sync_drain时仍能页传输，完整前deep次数=0。冻结后生成20条日志，所有seq>H不出现在该文件，文件sha/bytes不变。complete后才退出；后续批次包含新增记录或显式gap。

重复同页/status90s而无acked_offset增加：必须进入sync_incomplete，不被假进展保活。这是异常测试，不把持续推进的正例套总时限。

## S04 ACK丢失、幂等与cursor

在part已fsync但ack未到设备、ack已到但响应丢失、complete收据落盘但最终ACK丢失三个切点注入。逐一恢复同batch；确认文件字节无重复、durable_offset不跳过未存页、最终同IDalready_complete。

完成后跑2轮，再重放旧complete，rounds仍2，防二次归零。同ID错误hash/bytes拒绝且收据不变；同record键不同字节报protocol_conflict。serial回退stale_serial，不能新建伪批次。

## S05 断链、失败退避和无进展

关联失败三次（15s/次、1s/3s间隔）后本醒次不再开网，pending和due保留。连续HTTP失败达到3次中止；成功请求但无持久页进展90s同样中止。

第1/2/3次失败分别跳过1/2/4个完整自然窗口；消耗skip至0的本窗口仍不重试，下窗才可开。成功后failure/skip归零。新按键绕退避只一次，仍遵守失败上限。断链恢复从durable_offset继续，不自动创建新batch或重刷业务。

## S06 Bridge存储和进程退出

注入part写失败、fsync失败、checkpoint原子替换失败、最终文件写成功但任务/完成意图未落盘。任一未满足完整持久条件不得发送complete；cursor不得超前。

至少一次在页持久后直接kill隔离Bridge（不执行正常清理），重启从同data恢复同serial和文件前缀；另一次在完整文件持久后、complete前退出，恢复仅补完成确认。已声明持久文件后来人为损坏必须archive_lost，不要求设备重造。

## S07 设备恢复域与真正硬退出

deep保留generation、rounds、ring；RTC校验无效建立新generation/unknown due，不能把随机内存当日志。冻结文件跨设备进程kill/restart仍可续原generation，新环新事件留下一批。

在blob临时写、blob校验后、元数据引用前、complete收据写前/后各选代表切点硬退出。只恢复最高有效元数据引用的完整blob；无完成收据不得假完成。收据已落盘时重复complete幂等，冷启动轮数unknown不能据旧收据假造精确计数。

## S08 容量、溢出、CRC与单一流

用相同4096B环写15轮最大正常事件夹具，字节<=3600且无gap。异常text超过96B仅一条truncated且UTF-8完整；持续异常覆盖时seq仍单调、gap可见。

begin后填满环，冻结文件bytes/hash不变，后续gap明确；已冻结副本回收不算丢失。CRC坏、header坏、>8 gap合并都给对应原因/代际。旧/log和/history均来自同流，不能出现未经过统一seq的文字。

静态断言RTC控制<=768B；blob<=16384；元数据各<=2048；40KiB预留的空间不足只能明确失败。双目标map门槛留授权构建检查，宿主malloc成功不算RTC通过。

## S09 OTA/Bundle正常确认及重启

真实Bridge先arm、带ticket上传有限目录镜像；pending_reboot落盘后丢UPDATE OK，设备重启开一次授权Wi-Fi，Bridge重新认证nonce/MAC，原job只上传一次。

确认endpoint超时首次+2次后保持pending，后续联系查询，绝不再上传。Bundle在commit后丢ACK、Bridge退出，恢复查询原job/commit_seq/context；只安装一次。display_state=failed时保留安装成功与显示失败，不能reported displayed。

## S10 精确镜像证据等级

目录含版本字符串相同但字节不同的X/Y。冻结X，运行Y，image返回当前Y前N字节hash，必须不能image_verified。运行X且MAC/target/N/hash全部匹配才image_verified。

请求错误N超分区拒绝；不同slot的暂存X不能代运行Y；缺能力维持version_seen_unproven/awaiting_confirmation。override到预期版本不给upload_ack；Bridge不读目录内部SHA。离线夹具证明算法覆盖N字节全文件，包括尾部。真实ESP分区读取和Flash加密语义只在H02证明。

## S11 双MAC与任务隔离

A有pending frozen+OTA，B联系100次并多次sync，切UI选择、复用旧IP候选；A的serial/轮数/游标/job/确认时间均不被B更新，A业务写数0直到A自身认证。返回错MAC页或image立即拒绝。

两MAC不同device/Bridge rate/offset，推进A不推进B；每MACdeadline、skip独立。Profile8项完整仍可循环，保存模板/Profile业务publish计数为0。

## S12 token、owner、session及低电

新端点无/错endpoint token为401；OTA/claim仍需device token。错MAC/session为400，owner空409 claim_required、他人409 occupied；全部业务副作用0。sync_open不创建owner，显式claim不延light。

传输期间lease过期先暂停/显式claim恢复；他人接管旧批次owner_changed，不向新owner交付旧blob。未插电battery=4触发原保护，pending不清、rounds不归零；battery=5不凭本专项新增门槛关机。模拟只证明软件判断。

## S13 唯一协议与一次性切换

新Bridge+旧ROM：认证能力不满足即拒绝业务写，UI明确显示待升级；不得回退旧history或旧路径。旧Bridge+新ROM：`/v2/*` 返回404，带 `rv=2`/`protocol=2` 的BLE指令在Plan/Data/sync配置等副作用前拒绝；新ROM保留token保护的OTA与只读身份/状态恢复入口。新+新：新 `/api/*`、无平台版本字段和 `ack="command"` 完整通过；config落盘ACK才启用同步。

两台已登记设备按 `protocol-unification.md` 由旧Bridge逐台OTA、核对各自MAC/target/版本后才切换Bridge；错版期间不报告业务成功。Bridge切换失败则保护原data并按顺序回退旧Bridge与两台旧ROM，不能只回退一侧留下永久混用。新owner必须重配；disabled不删pending。双MAC的能力和状态不得串用。旧Bridge行为若只用固定协议夹具验证，标明是模型，不能宣称旧EXE实跑。

## S14 状态、UI、时间与脱敏

先Wi-Fi采样heap=0、BLE=false、templates=[]，再只BLE联系；原值保持，Wi-Fi采样时间不变，最近BLE时间推进。失败HTTP/Bridge重启不清旧值；省略字段不清，null+read_error明确不可用。

radio/runtime/power>120s标stale；新boot令旧runtime/radio立即stale；firmware值仍保留原时间。wall倒退显示clock_anomaly不移动单调截止。public status错MAC不能覆盖。UI不因刷新页面产生sync_open/Plan。

设备页无Codex配额；屏幕内容/模板/字体仅归属声明、数据页布局未改。哨兵Wi-Fi/AP密码、endpoint/device token注入全部日志生产者，RTC、串口镜像、HTTP兼容视图、batch文件、Bridge归档均搜索不到secret。

## 软件最终出口与硬件边界

- [ ] S01–S14代表case均有执行命令、确定输入和结果；无“整体skip后通过”。
- [ ] 双target Fake ROM 24h协作运行，实际经过第15轮/入口/离场和恢复；未消费I/O时不跳时。
- [ ] 同快照/seed至少1h双回放归一化trace、批次内容hash、帧CRC一致（随机ID映射见F3，每次自身原文件hash独立验证）。
- [ ] Bridge与设备各至少一次进程kill/restart；确认生产Bridge/实机未触及。
- [ ] 相关测试与git diff --check通过；新能力未获实现证据前仍标待实现。
- [ ] H01 RF/GATT物理切换、H02 RTC/Flash/分区/bootloader、H03按键/面板按授权最小烟测；H04仅电池端测量可得功耗结论。
- [ ] 最终交接分别列设计、源码/软件验收、构建、实机部署及证据，不混用完成等级。

## Fake ROM操作规程（具体接点与运行命令）

本节只补模拟验收职责，不改design.md生产协议。2026-09-28只读复核未运行任何进程。现有接口与待实现接口严格分开；实现后可按此节独立启动隔离夹具、执行S01–S14并保存证据。

### F0 已有接口清单

| 所在进程/接口 | 已有请求或入口 | 确切边界 |
|---|---|---|
| device-sim启动 | --listen 127.0.0.1:0 --mac 02:00:00:00:00:A1 --target codex-status-154g --data-dir <绝对目录> --seed 1 --epoch-ms 1790553600000 --wake-cause cold | 三个必填环境变量CODEX_STATUS_SIM_ENDPOINT_TOKEN、CODEX_STATUS_SIM_DEVICE_TOKEN、CODEX_STATUS_SIM_CONTROL_TOKEN在单设备内互异；stdout首行ready JSON给实际http地址。Note4 target=zectrix-note4-400x300。 |
| 设备控制面 | GET /sim/state、GET /sim/frame、GET/POST /sim/time | Bearer control token；不同于/sim/ble的endpoint token。 |
| 设备时间 | POST /sim/time：{"op":"rate","rate_ppm":0}、{"op":"step","delta_ms":60000}、{"op":"wall","offset_ms":-3600000} | 先暂停再step；wall是偏移，不是绝对wall_ms。正常timer靠step触发，不用/sim/wake伪造。 |
| 设备唤醒/输入 | POST /sim/wake {"cause":"button"}（或cold/soft）；POST /sim/power {"plugged":false,"battery_pct":4}；POST /sim/display {"fail_next":true}；POST /sim/button {"hold_ms":2000} | button wake要求当前asleep；button hold要求awake，hold_ms范围[2000,15000)。低电是软件输入，不是ADC测量。 |
| 设备现有ACK故障 | POST /sim/fault {"stall_ack_after":"bundle_commit","duration_ms":10000} | 支持none/bundle_commit/data/activate/ota_upload；仅已applied后一次消费，duration_ms为1000–120000真实毫秒。当前不支持sync_*，不能据此声称已测同步丢ACK。 |
| 设备现有存储故障 | POST /sim/storage {"write_budget":0}，null恢复；{"crash_after_sync":"slot","count":1}或kind=metadata | 目前接SimBundleDevice，不覆盖新的sync文件；同步日志须新增接点。已有bootstrap硬退出case可复用操作方式。 |
| 设备OTA目录 | GET /sim/versions；POST /sim/versions {"id":"v2","reboot":true}；启动--catalog <绝对JSON> | 目录目前2–3条id/fw/target/size/sha256，不保存完整运行字节；override有独立来源，不等于上传。 |
| BLE客户端 | bridge_ble::DeviceConnection::connect_any(targets, endpoint_token, bridge_id)，随后command("status", {})/command("plan", body) | CODEX_STATUS_SIM_BLE_ENDPOINTS为{"0200000000A1":"http://127.0.0.1:<port>"}；显式注册后走loopback，不碰OS radio。fragment为180 B；当前sim_ble_command只分派status/plan/data。 |
| Bridge控制面 | MCP端口的POST /sim/clock、POST /sim/run | 需CODEX_STATUS_BRIDGE_SIM_CONTROL_TOKEN及CODEX_STATUS_SIM_COOPERATIVE=1；Bearer为Bridge control token。不是业务HTTP端口。 |
| Bridge时间 | {"mac":"0200000000A1","op":"set","monotonic_ms":0,"wall_ms":1790553600000,"rate_ppm":0}；随后rate/step/get/wall | set建立视图；get也POST。Bridge wall用绝对wall_ms；未建立视图时runner的rate会失败。真实MAC拒绝实验时钟。 |
| Bridge单轮 | POST /sim/run {"mac":"0200000000A1","kind":"ble"}或kind=http | 当前同步等待整轮完成，clock遇sim_control锁返回409 in_flight；不能直接推进等待中的异步虚拟事件。HTTP outcome.cycle=complete只表示函数返回，不证明业务成功，须看持久文件/设备状态。 |
| 设备注册 | MCP tools/call name=platform_device_register_v2，arguments={"mac":"0200000000A1","endpoint":"127.0.0.1:<port>","name":"Fake A"} | endpoint无http://；注册核验公开+认证状态，不claim、不publish。 |

源位置：device-sim/src/main.rs的parse_options、sim_time_post、sim_wake/power/fault/storage/versions、app；tests/bootstrap.rs的Simulator及bridge_device_connection_uses_fake_ble_without_os_radio；ble/src/lib.rs的DeviceConnection；app/src/main.rs的sim_clock_handler/sim_run_handler；app/src/platform.rs的register_device。接口清单须随新增实现更新，不把旧D/E/F证据套在新sync路径。

### F1 固定启动、准备与进程恢复

1. 复用bootstrap.rs的Simulator::start_with_dir、ready行解析、request、stop_preserving_data。新sync_v1测试始终串行：当前BLE注册表用进程环境变量，平行测试会串注册表。每case创建自己的隔离目录；测试命令因动态建目录应依AGENTS提权执行。清理只作用于该case记录的Child/PID与经绝对路径核验的目录，不结束生产Bridge/watchdog。
2. 新增bootstrap内的最小BridgeHarness（待实现），从CODEX_STATUS_SYNC_TEST_BRIDGE_EXE读取绝对EXE；变量缺失或EXE不在本次隔离target目录时明确失败，不skip。通过Command的子进程env设置CODEX_STATUS_INSTANCE=sync-v1-<case>、CODEX_STATUS_PORT/MCP_PORT（各自空闲且不同）、CODEX_STATUS_TOKEN、CODEX_STATUS_SIM_BLE_ENDPOINTS、CODEX_STATUS_SIM_COOPERATIVE=1和CODEX_STATUS_BRIDGE_SIM_CONTROL_TOKEN。使用隐藏窗口、重定向并消费stdout/stderr；启动有wall watchdog，测试finally按句柄停止本实例watchdog再停止主进程。
3. Instance::from_env会把CODEX_STATUS_DATA改成<exe>/instances/<name>/data，因此不能只设另一个data路径就宣称隔离。预建/记录该实际路径及运行PID；从同路径重启，恢复期间不得覆盖state.json。宿主测试常量都是假凭据，不复用生产密钥。
4. 同一Bridge目前使用ctx.config.token认证所有目标。双Fake设备的endpoint token须与该测试Bridge的CODEX_STATUS_TOKEN一致；各设备device/control token仍分离。跨MAC/错token测试显式换输入；不要为追求“每设备不同endpoint token”让当前生产Bridge无法认证。
5. 启动两fake后先暂停各设备时钟，再用Bridge /sim/clock set初始化对应MAC（值取设备sim/state的当前monotonic/wall，不能运行一段后硬设回零）。以真实MCP注册endpoint，按现有真实device_client编译/安装quad或Note4模板夹具，并显式claim到该隔离Bridge的真实bridge_id。bootstrap的bridge-test只用于不带真实Bridge的局部case，不能拿它占住全链测试设备。
6. 取得device token走现有DeviceConnection::request_device_token并由真实Bridge路径持久保存。sync_config/sync_open必须经DeviceConnection.command；状态/日志/image走生产HTTP客户端。先做一次完整同步建立rounds=0，再下正式sleep Plan开始S01，不能直接改RTC轮数。
7. 进程恢复沿用同data-dir和catalog；设备启动--wake-cause明确cold/soft/deep，使用同源保留域恢复。Bridge恢复后重新取nonce、对账原batch/job；不要重新注册清空状态或复制“期望结果”进去。

MCP请求是现有JSON-RPC：POST /mcp，body含jsonrpc:"2.0"、id、method:"tools/call"、params:{name,arguments}，主机为127.0.0.1。测试使用已有生产MCP处理器，不能通过手写state.json冒充注册/冻结/确认。初始化夹具可复用现有编译/发布助手；全部准备动作计入trace，并在基线同步之后才清“场景内副作用”计数。

### F2 待实现的测试接点与输入

这些是隔离control API，不是新的生产协议。均要求显式实验模式、loopback、control token；Bridge端另要求LAA MAC。复用现有/sim路由，拒绝未知point/错误参数，不允许任意文件路径或代码执行。

| 控制输入/接点（待实现） | 实现要求及用途 |
|---|---|
| GET /sim/state增sync/diag/test_events | 显示生产sync phase、rounds/due/skip、当前batch/client_serial、acked_offset、generation、earliest/latest、used_bytes/gaps及next_event_ms。test_events含计数wifi_open、sync_begin/page/ack/complete、ota_upload、bundle_commit、fault_consumed；必须在实际接点记数，不能在测试脚本猜。 |
| POST /sim/diagnostics {"op":"append_text","text":"…"} | 经同一脱敏/有界生产入口分配seq；允许长输入以验证96B截断。另支持{"op":"corrupt_record","seq":"<已存在序号>"}、{"op":"corrupt_header"}，仅改指定CRC/字节后调用真实恢复；不提供set_rounds/set_cursor。 |
| POST /sim/observations {"heap_free":0,"ble_connected":false} | 只改硬件采样适配输入；null及字段能力另由响应夹具覆盖，用于S14，不直接改Bridge缓存。模板为空由合法未配置设备场景提供，不把有Bundle状态硬改为[]。 |
| POST /sim/features {"sync_v1":false,"image_identity":false} | 仅测试协商视图；sync_v1=false同时缺能力且新端点返回unsupported，不能只隐藏广告仍暗中执行新协议。旧Bridge行为通过固定旧命令场景或真实旧EXE验证，并标清证据级别。 |
| POST /sim/fault {"sync_point":"complete_receipt_committed","effect":"drop_reply","nth":1} | 扩展现有严格schema，保留原stall_ack_after兼容。sync schema的point见下表；effect仅fail/drop_reply/exit/delay，delay另须delay_ms=0..120000（虚拟ms）。fault一次消费，重启默认不再次注入。 |
| Bridge POST /sim/fault {"mac":"0200000000A1","sync_point":"part_fsync","effect":"fail","nth":1} | 仅隔离Bridge控制面新增，触发真实归档器错误返回；不能拿设备Bundle的write_budget冒充Bridge磁盘故障。exit必须进程立即退出而不运行正常清理。 |
| 同源Wi-Fi事件适配 | 设备fault point=wifi_associate、effect=fail、nth=1；一次使关联请求失败。连续三次用三次明确注入/有限fault队列，仍执行生产15s/退避决定，不直接把结果设incomplete。 |
| OTA字节存储适配 | catalog增加受限fixture字节路径（由测试生成并核对原size/hash）或保留实际上传字节；active槽选择与目录版本绑定。sync/image从active字节读取N，S10再放一个不同字节的inactive槽。旧目录无真实字节时image能力必须关闭。 |

设备point：begin_blob_write、begin_blob_verified、begin_metadata_committed、page_before_reply、ack_before_apply、ack_after_apply、complete_before_receipt、complete_receipt_committed、wifi_associate。Bridge point：part_write、part_fsync、checkpoint_replace、final_archive_committed、completion_intent_committed、complete_before_send。fail仅在能够真实返回错误的位置；drop_reply仅回复边界；exit仅持久切点；不合法组合返回400。延迟使用对应实验单调时间并注册未来完成事件，不能调用真实tokio sleep后声称虚拟时间已覆盖。

S04绑定ack_after_apply/drop_reply、complete_receipt_committed/drop_reply；S06绑定Bridge的part_fsync/fail及completion_intent_committed/exit；S07绑定begin_blob_verified/exit与complete_before_receipt/exit；S09复用既有bundle_commit、ota_upload丢ACK，再叠加新pending持久切点。每case首先断言fault_consumed增加1，否则测试失败，避免“不曾命中但结果看似正确”。

### F3 协作runner必须补的具体行为

1. 保留现有同步POST /sim/run兼容。新增可选action=start：{"mac":"…","kind":"http","action":"start"}立即返回{run_id,state:"running"}；action=poll带同MAC/run_id返回running或done及原outcome。一次每MAC至多一个run，重复start返回原run；poll不再次调用production cycle。BLE同样可用；省略action仍是旧同步行为。
2. 当前sim_control锁期间/sim/clock step一律409，不能用“启动异步run”就绕过屏障。新增已登记virtual_wait状态（run_id、next_event_ms、时域、响应尚未消费）；只有参与者都idle或明确virtual_wait时允许推进到最近事件。任意未登记真实socket/文件I/O在途仍409。delay故障到点先完成响应并让业务消费，再继续跨后续截止；控制面poll不计同步进展。
3. 先增加一个定向测试：start发送被登记延迟10s的page；未知在途step得到409；等待状态登记后允许共同推进10s，poll得到完成，durable_offset推进一次。若仍需实际等10s、直接跳过响应或死锁，则F3未通过。
4. runner读取next_event_ms（有sync能力时）决定下个事件，不再仅从power.mode=light/deep推断；WIFI_SYNC_ONCE与离场drain也调HTTP。sync run未结束时轮询控制面、处理未来事件，不能在20s call watchdog到时取消整个合法批次。短控制请求仍保留20s wall watchdog，真正未知I/O卡死报告harness_timeout，与设备no_progress分开。
5. 正常长跑保持每次BLE成功断言；故障场景由具名Rust测试显式预期contact=false/401/409等并验证恢复，不能让runner忽略全部BLE错误。max_events达到但until_ms未到仍失败。普通页/status重复不算progress。
6. trace增加phase、rounds/due/skip、batch/serial、generation、durable_offset、formal_deadline、radio_reason、fault_consumed。随机batch后缀/nonce、宿主PID和wall耗时按首次出现映射后比较；保留映射原件，不盲删业务serial/seq/hash。由于冻结JSON含随机batch_id，跨独立回放比较“归一化后内容hash”，同时每次自身原字节hash必须验证；不能要求不同随机ID的原文件SHA256相等。

### F4 具名测试、命令与最小场景

新增测试入口统一命名sync_v1_s01到sync_v1_s14（可附下划线描述），放现有device-sim/tests/bootstrap.rs，复用其helpers；涉及真实Bridge由BridgeHarness调用，涉及core/显示的快速单测仍在对应crate保留。每个S类别完整断言见正文。新增环境变量CODEX_STATUS_SYNC_TEST_BRIDGE_EXE仅供测试harness定位已构建EXE；缺变量明确失败。以下是**实现后执行命令，本次未运行**；目录/编译/测试自动创建目录须以AGENTS要求的提权命令运行，cwd为仓库根：

    cargo build --manifest-path bridge/Cargo.toml --target-dir bridge/target-sync-v1 -p device-sim -p bridge-app
    $env:CODEX_STATUS_SYNC_TEST_BRIDGE_EXE = (Resolve-Path bridge/target-sync-v1/debug/bridge-app.exe).Path
    cargo test --manifest-path bridge/Cargo.toml --target-dir bridge/target-sync-v1 -p device-sim --test bootstrap sync_v1_ -- --list
    cargo test --manifest-path bridge/Cargo.toml --target-dir bridge/target-sync-v1 -p device-sim --test bootstrap sync_v1_ -- --test-threads=1

--list输出必须覆盖s01…s14所有前缀；零项/缺类别直接失败，不能把Cargo“0 passed”写通过。定位单类用sync_v1_s04替代sync_v1_；改core/app持久接点后运行其具名sync_v1_单测。已有真实BLE链回归命令：

    cargo test --manifest-path bridge/Cargo.toml --target-dir bridge/target-sync-v1 -p device-sim --test bootstrap bridge_device_connection_uses_fake_ble_without_os_radio -- --exact --test-threads=1

长跑沿现有命令，不虚构当前不存在的--scenario：

    node tools/fake-rom-runner.mjs artifacts/sync-v1/runner.json

runner.json由harness在完成设备注册、claim、模板安装、基线同步及两侧clock set之后生成（文件含假control token，保持ignored、不上传日志）：

    {
      "until_ms":86400000,
      "max_events":20000,
      "bridge":{"url":"http://127.0.0.1:<MCP端口>","token":"<Bridge控制凭据>"},
      "devices":[
        {"mac":"0200000000A1","url":"http://127.0.0.1:<A端口>","token":"<A控制凭据>","scale_ppm":1000000,"bridge_scale_ppm":1000000},
        {"mac":"0200000000B2","url":"http://127.0.0.1:<B端口>","token":"<B控制凭据>","scale_ppm":1000000,"bridge_scale_ppm":1000000}
      ],
      "trace":"artifacts/sync-v1/day.ndjson"
    }

端口使用ready/实际Bridge监听值替换，不能照抄占位字符串执行。当前runner默认max_events=2000不足以覆盖历史2861事件的24h，故这里显式20000；若新增分页超过上限应记录实际事件数后合理增大，不静默丢事件。1h重放使用until_ms=3600000，从同一份已停止写入的磁盘快照分别复制到两个隔离实验目录；不在仍写入时复制。停止/恢复均由harness负责，脚本运行完不遗留隔离watchdog。

S01最小完整操作顺序：pause双方→真实注册/claim/安装→经DeviceConnection sync_config→真实Wi-Fi complete→正式sleep→逐次设备step到next_event、Bridge step同delta→/sim/run ble→第15轮观察sync_open→生产HTTP sync run→核对最终文件/收据/rounds/deep。S03使用单测中生产sync客户端的合法page_limit=64（只在test helper参数中传递，不新增用户配置），每页注入10s虚拟delay；不得换成手工脚本下载来替代Bridge落盘流程。

S06/S07由harness实际Child.kill/wait或命名持久切点exit终止进程；重新启动后读原文件/nonce/serial，不调用正常reset代替。S13旧端能力可用/sim/features模型，旧Bridge路径用固定旧命令集合或明确版本EXE；报告区分“兼容模型”与“真实旧EXE”。S14 UI可用离线service返回夹具驱动设备页渲染断言，不能因模拟器无桌面UI就删掉来源/年龄测试。

### F5 验收证据与硬件未覆盖项

每个case产出{case_id,target,seed,inputs,expected,actual,assertions,fault_consumed,process_restarts}及脱敏trace、关键批次/收据hash、原始时域。新接点实现后重新执行对应case；既有D/E/F结果只证明基线。RF/GATT物理窗口、ESP32网络栈、实际RTC/Flash掉电、加密分区读取、bootloader、ADC/USB/面板和电池端功耗仍只由授权硬件烟测证明。
