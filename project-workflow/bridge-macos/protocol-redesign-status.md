# 协议重设计执行状态

2026-10-12 22:44（Asia/Hong_Kong）；基线HEAD `49cec054`，进入时仅PROGRESS.md已有修改。用户持续目标：完成P0–P6，不以本里程碑结项；未提交/部署/访问设备。

## 当前增量

- P0基线与active_protected夹具见[基线](protocol-redesign-baseline.md)。有备用配置可删保存行、live active不变；唯一配置拒删。
- P1/T01：OperationReply区分原写、原结果查询和传输证据。查询失败（包括401/storage/rejected错误信封）保留原sent/Unknown/ID；只有成功读取的持久终态才能确认。成功/拒绝都先保存，再进入ACK退役；修正sync拒绝遗漏ACK。
- P1/T02设备侧：回执查找/存储唯一性/查询/ACK统一principal+namespace+request_id；跨namespace同号独立提交，内容冲突、重载、高水位与ACK隔离通过。Bridge冻结namespace/身份变更围栏尚待补齐。
- P1/T03：execution_intent独立于确认义务。取消/过期停止新写和新增RF需求；原事务继续借机会query-only，失败按原backoff，不换ID。新的冲突配置写返回wifi_confirmation_pending；终态保存后可安全退役。
- P1/T04：WFS5头和实际回执保存执行代际。新trial/activate先落pending，trial成功/失败落终态；activate仍成功后将active+回执一次原子保存。重启后新代际pending先持久interrupted，读回失败不得假终态；旧generation0 pending仍未知、不ACK。无控制事务的开机不额外写代际。
- HTTP实际发送进度区分NotSent/WriteUnknown/AckUnknown；业务session建立失败不计成业务写入，类型化session过期/限额错误不再靠字符串重试。已加loopback连接失败/session ACK丢失/完整业务帧ACK丢失验证；共享生产write循环的确定性故障注入覆盖逐字节短写后超时、零写、过期截止和完整写；部分写保留WriteUnknown且不从头重发。
- L06在已有设备锁入口：blocking worker共享OwnedMutexGuard，async waiter取消不释放仍能写的worker许可。配置/确认/诊断/Data/Bundle/Plan/OTA arm调用保持许可；显式sleep补同设备串行。真实loopback阻塞socket测试证明取消后本设备不可进入、另一设备可执行、底层截止后可再次进入。完整P3入口收敛/排队总截止仍待完成。

## 已验证与证据

- core：168 passed（lib146/client7/envelope7/template8），`artifacts/protocol-redesign-p1-core.log`。
- app：87 passed、3条件ignored；Fake bootstrap：64 passed。`artifacts/protocol-redesign-p1-integration.log`。HTTPworker取消回归包含其中。临时恢复“worker开始前释放许可”后同测试在live socket断言失败，finally恢复源码；red/green证据为`artifacts/protocol-redesign-http-worker-red.log`与`artifacts/protocol-redesign-http-worker-green.log`。
- C++管理全部9组通过：`pwsh -File artifacts/protocol-redesign-test-management.ps1`。存储/ASAN通过：`pwsh -File artifacts/protocol-redesign-test-storage-asan.ps1`，日志`artifacts/protocol-redesign-wfs5-storage.log`。两脚本按目录规则提权运行。
- MSVC14.44 + Note4 ArduinoJson7.4.3，直接包含生产wifi代码，HAL采用现行Windows render实现。存储故障注入覆盖截断/长度/count/CRC/分配失败/原子保存/读回失败/代际恢复；140次探测命中36个分配失败点。实际WFS4布局转换后generation0 pending不丢。
- 初始旧storage容量夹具按“每来源4项/最多3来源”断言导致空错误strcmp崩溃；ASAN定位后改为现行全局12配置限制，新增第13项拒绝。Fake旧management2夹具仍在，本轮仅按其声明核对超限NotSent/零事件；P3必须替换并删除旧正向路径。

## WFS5资源与候选

固定格式增量：头8 B，每条实际回执8 B；空/1/2/8回执分别8/16/24/72 B，不包括pending到interrupted错误字符串差。Windows64布局network104/receipt152/store232 B；不是ESP32动态RAM测量。

| 配置/回执 | 持久blob B | 宿主仓库堆 B | 克隆+保存峰值增量 B |
|---|---:|---:|---:|
| 0/0 | 41 | 0 | 314 |
| 1/1 | 251 | 425 | 1175 |
| 2/2 | 431 | 790 | 1900 |
| 12/8 | 5188 | 7066 | 17690 |
| 合并最大cursor/长字段/live active | 6382 | 8516 | 21536 |

执行代际随pending提交，不单独写Flash；trial终态增加一次提交；重启pending在一个批次中断提交。动态实机峰值、写延迟、Flash掉电仍未知。

默认Note4隔离构建成功（116.56s，不clean），候选 **0.18.86-note4**：

- ROM `artifacts/protocol-redesign-note4-0.18.86.bin`，1,840,800 B，SHA256 `b65223bf46d1f39584a2a21f3898be8f9e13c08fe5114815e274c396e166faae`。
- ELF同名前缀`.elf`，30,513,892 B，SHA256 `1f40009e0a700c23c79361edc353b15d4d3b8778a1ffb74abdf2ed6b878c7d2d`。
- map同名前缀`.map`，20,236,690 B，SHA256 `2c3e66a08c6c5935ee3ca0783bb63ec183ce7db902247d2a5e13411ca1dfef05`。
- marker `codex-status-ota-v1|zectrix-note4-400x300|0.18.86-note4`；RAM119044/327680、Flash1789071/6225920。build日志`artifacts/protocol-redesign-note4-build.log`。
- 匹配ELF的Wi-Fi栈64项通过（`artifacts/protocol-redesign-note4-stack.log`）；wifiSchemesBegin224、readWifiStore272、serviceV2Ble960、wifiSetupCommand672、wifiManagementWithContext720 B。局部链检查不包括全部平台/间接调用峰值。

**仅候选，不是可发布最终版**：兼容读取/旧网络入口仍存在。默认Bridge及设备均未改动，现场版本见PROGRESS。

## 22:58 P2扫描生命周期增量

生产find_device_matching的start/events/外设查询/properties/stop现在经过同一scan_with_cleanup。原观察窗口保留，start及查询消费该窗口，另外使用既有CLOSE_BUDGET（2s）清理；全部按同一绝对截止结算，不逐阶段重新起算。没有候选正常返回None；挂起的构造/查询明确work_timeout。

原生状态为停止/扫描中/未知三态。并发扫描和零工作预算在任何原生请求前拒绝；只有start已确定结束且stop成功才恢复停止。start超时、stop失败/超时、外层取消均隔离，后续adapter/scan明确要求重启，不继续创建扫描worker。未知start即使随后stop返回也不推断安全（迟到start可能再次启动watcher）。这里不声称重启之外的原生恢复已获证明。

`cargo test -p bridge-ble --lib --target-dir ../artifacts/cargo-target-convergence`：22 passed，日志`artifacts/protocol-redesign-p2-ble.log`。故障注入与生产共用完整扫描控制流，覆盖start/events/stop永久pending、取消、失败清理、零预算/并发零原生调用、正常无候选及已确认清理后的下一扫描。首次编译暴露async deadline借用，改move捕获后通过；未改业务电源期限或延长扫描观察窗口。

范围限于扫描，Manager构造、connect/discover/INFO/配对、已建链取消/迟到回调、一次有界恢复、调度健康投影及全程截止仍未完成。现有connect失败测试仍仅证明旧2s清理返回，不证明新隔离/恢复要求。未运行原生无线或部署Bridge；固件未变，0.18.86哈希保持。

## 23:09 P2现代建链与控制写增量

DeviceConnection原生建链改为共享open_link控制流：确定connect错误最多两次，timeout不重试；discover/INFO/完整MAC/bond/rendezvous构造错误归同一清理出口。NativeLease从构造一直持有到连接确认关闭；取消建链、直接丢弃活连接、disconnect超时或未确认分片写保留隔离，不把适配器当空闲。Manager/adapters构造限5s，原生现代构造总上限25s，connect/setup段最多18s含原2s清理预算；尚不是业务execute全程预算。

自动会合首次未绑定明确要求显式配对；已绑定加密探测使用3s，用户显式request_device_token仍允许60s系统配对。endpoint/auth与CMD共享“写入可能仍在进行”保护：单独取消写future后保留原子标记，同连接下一写/命令NotSent，不因保留connection变量而漏掉隔离。

BLE26项通过（日志`artifacts/protocol-redesign-p2-ble.log`）；包含确定失败清理后下一链路、connect/setup/cleanup pending、构造取消/活连接丢弃、各setup拒绝统一清理、自动/显式配对及control write取消。现代建链变更集成app87/3忽略、Fake64通过（`artifacts/protocol-redesign-p2-integration.log`）；追加control write保护后的完整集成app87/3忽略、Fake64也通过，日志`artifacts/protocol-redesign-p2-control-integration.log`。测试不使用原生无线。

仍待完成：显式Pusher遗留调用未统一（有裸connect/discover，P3必须删除/收敛）；Fake早构造分支未同构，原生迟到回调/失效worker与一次有界恢复无实证，隔离只允许进程重启；共享调度健康投影/全部入口总截止仍待实现。不能将现代open_link局部完成当P2整体通过。未部署，固件未改。

## 23:19 P3旧Pusher后备入口删除

app自动循环仅从登记设备取得MAC候选，删除旧UDP/Pusher.cycle_once自动endpoint写/隐式adopt及退避驱动；删除无生产写入者的udp_ble标记与selected注册布尔旁路。UDP当前身份校验/endpoint发现提示保留，不授权未知设备写入。

BLE CLI强制mac/token/bridge-id，使用同一DeviceConnection、现行endpoint和status确认；删除BleConfig、Pusher构造状态、cycle_once/run、旁路push_endpoint和额外通知worker。原Pusher名称仅保留静态显式发现/配对助手，无旧后备循环。显式发现/取token通过on_link复用open_link/NativeLease；native connect仅剩这两个受控调用点，所有disconnect均有界。配对/取token不被误删，成功token读取与清理异常分别保留结果/隔离。

app87/3忽略、BLE26、Fake64通过（`artifacts/protocol-redesign-p3-entry-tests.log`）。删除无用字段后`cargo check -p bridge-app -p bridge-ble --tests`通过（`artifacts/protocol-redesign-p3-entry-check.log`）；CLI帮助另存`artifacts/protocol-redesign-p3-cli-help.log`，不接触无线。操作示例已更新docs/agent-operations.md。rg确认生产Rust不再包含BleConfig/cycle_once/Pusher::new旧驱动；历史文档保留历史事实。

本增量只完成P3删除表中的旧Pusher驱动一项。management2/permit/旧恢复路径、全部业务执行器、全层TransactionKey和安全格式转换仍开放；显式扫描/配对还需纳入业务层完整预算与健康投影，原生平台实证未执行。

## 23:33 P3 app Wi-Fi 单路径增量

删除app的management2/light/sync窗口推断、Permit缓存、wifi_authorize/wifi_migrate调用、旧trial/activate轮询及自定义恢复保存驱动。仅接受management3；旧设备返回update_required，在线迁移入口明确offline_migration_required。旧设备拒绝不清除已发送事务的确认义务。core/固件的permit字段和旧协议实现尚未删除，因此这只是app入口收敛，不是单栈发布完成。

显式trial/activate、自动激活、离线恢复保存统一进入wifi_attempt；HTTP只走现行wifi_command，查询仍可在写窗口关闭后执行。BLE回执查询不依赖owner写权限，owner检查和ACK遵守剩余业务预算；app显式Wi-Fi链路关闭前继续持有设备许可。主机网络变化仅阻止未发送且绑定host的任务，已发操作继续按原ID确认。

首次回归暴露了恢复路径提前取走只读任务的问题；修正为恢复仅选sync/recovery_save，RF已采样且会合允许的read显式调用同一持久执行器。现行Fake默认management3；旧RF未知的夹具仍走后续HTTP，带窗口的BLE读取保留Plan/SyncOpen延长与丢ACK证据。完整app87项通过/3条件忽略（artifacts/protocol-redesign-p3-wifi-tests.log），新增主机变化/原ID查询回归1项通过（artifacts/protocol-redesign-p3-host-guard.log），合计88个不同测试通过。git diff --check通过；没有改UI、固件或默认实例，未访问实机。

仍开放：core/device_client的permit参数、固件旧协议和存储转换、全层namespace冻结、全部入口单一执行器/绝对截止、原生恢复实证及P4–P6。Operation临时UI状态与非持久取消语义还需随全入口收敛清理，不能把本批删除当P3结项。

## 23:41 P3 core 写许可去 permit

wifi_attempt的许可参数从Option字符串改为write_allowed布尔值，全部生产/测试调用同步。业务apply JSON不再序列化permit；未获写许可仍不发新写，已发送事务的result查询先于写许可判断。追加现有冻结字段回归的请求形状断言，保证旧授权字段不会重新进入业务请求。

原management2的跨BLE/HTTP丢ACK恢复测试改为management3：USB连续窗口、显式HTTP claim、认证状态登记、真实BLE active ACK、绑定当前session的HTTP写；丢ACK后保持原ID，Bridge重启仅查询，设备进程重启获取新session后仍查询同一回执。转换夹具时发现wifi_owner不是Fake共享Wi-Fi处理器提供的命令，改用实际active读取验证BLE ACK；未提供session的写被拒且未提交，补齐当前认证session后通过，未放宽设备拒绝或降低断言。

联合日志artifacts/protocol-redesign-p3-permission-tests.log：app88通过/3条件忽略、core168通过，Fake63通过/该转换夹具1失败；夹具修正后该项单独通过artifacts/protocol-redesign-p3-current-receipt.log，最终覆盖64个不同Fake测试。补充请求形状断言通过artifacts/protocol-redesign-p3-no-permit.log。固件未修改，现有候选哈希不变；未启停默认Bridge、未部署或访问设备。

剩余旧栈定位：device_client::wifi_snapshot仍带permit参数；wifi_management.inc的WifiPermit/findWifiPermit及旧处理分支、transport.cpp的wifi_authorize/wifi_migrate、sim_wifi.cpp/旧全投影与render测试仍需与一次性格式转换一起删除。不能据本次服务接口清理宣布P3单栈完成。P4已确认现有/doUpdate空上传依赖Update.hasError会误报成功、错误target/写失败混为401，仍待实现和完整handler故障矩阵。

## 23:50 P4 独立OTA请求结果与清理

新增firmware/ota_http.inc供transport.cpp生产注册和tools/test-ota-http.cpp宿主直接编译。使用WebServer原生RequestHandler，在选定/doUpdate时初始化每请求状态；canUpload/handle不重新清空状态。审阅本地WebServer3.3.11解析和RequestHandler调用链，避免filter在每分片被重复调用导致状态重置。每请求（含无multipart文件）独立鉴权；只有本请求完整Update.end成功才能UPDATE OK。空文件/空请求/target不符400，token错误401，busy409，begin/write/end/取消失败500，停滞408；失败返回JSON error/stage。保留无target的token认证应急上传，不引入业务session/owner/Plan前置。

拒绝/busy请求不abort或释放其他原生/HTTP上传持有的Update和维护锁；HTTP自己的失败、取消、完成均释放。重启只在最终成功应答后安排；第二次上传在等待重启时busy。统一otaUploadTick修复millis为0时的watchdog空洞，并验证uint32绕回及恰好20000ms截止。未改正式PowerPlan、原生双槽或ArduinoOTA认证。宿主测试直接执行生产callbacks，但不声称验证了真实WebServer socket/parser、Flash写入和ArduinoOTA设备行为；这些仍需实机门禁。

故障矩阵通过artifacts/protocol-redesign-p4-ota-handler.log：空请求/每次鉴权、成功后空请求不复用成功、target正确/错误/省略、外部Update和旧HTTP busy拒绝/abort隔离、begin/write/end/空文件/取消/不完整/多文件、0起点与millis绕回20s截止、各失败后正常上传可进入。未启停Bridge、未访问设备或烧录。

候选仅Note4 zectrix-note4-b，版本0.18.87-note4，真实marker target=zectrix-note4-400x300（不混同env）。构建82.49s通过，日志artifacts/protocol-redesign-p4-note4-build.log；RAM119044 B，Flash1789123 B。候选文件：
- artifacts/protocol-redesign-note4-0.18.87.bin：1840944 B，SHA256 df0282d3d385f38b75eeb29d60e59435fd8885164116dcf1b05b20b7de9fbf10。
- 匹配.elf：30511172 B，SHA256 81a6175d923b80328ddfae901ffc06f3eb823224eec90fb930abbad7aba7c7c4。
- 匹配.map：20251179 B，SHA256 954db27aba49953022a54d342940695cd79d75fc75bc2e00c88caff48de7a5ad。

资源：不增加NVS记录，空/单次/典型/上限请求的新增持久字节均0；单请求固定结果状态MCU实测12 B，未按容量预分配仓库或保存凭据。新handler替换原有FunctionRequestHandler注册对象，不增加每请求动态缓冲；错误JSON为栈上128 B。实际ELF帧：upload144 B、handle208 B、canHandle48 B、cleanup/tick32 B；端到端Update/网络堆峰值与Flash行为仍需设备测量，不据单帧推断完整栈。Wi-Fi现有64项栈门禁通过（artifacts/protocol-redesign-p4-note4-stack.log），哈希/符号帧证据artifacts/protocol-redesign-p4-candidate.log。旧0.18.86保留为历史候选，当前源码只匹配0.18.87。

## 23:58 P3 快照客户端与授权残留清理

device_client::wifi_snapshot移除permit参数和线上JSON字段，app和所有测试调用同步；现行session_nonce及分页总截止保留。实际compact frame解码断言确认snapshot请求没有permit，而不是仅检查函数签名。删除无调用者的wifi_resume_authorized及app Fake的wifi_authorize分支；设备执行锁的permit是Rust锁许可，语义不同且保留。

将旧management2完整投影测试改为management3/session/owner：有效owner管理全部来源记录，原创建namespace作为来源保留；改变namespace仍可编辑旧配置并新增配置，同号事务在新namespace独立。保留UTF-8/转义文本、多页冻结、批量后半项拒绝导致整批不变、持久拒绝回执、配置变化后旧snapshot_expired。无session的读取不能把记录标为可写或返回密码。历史legacy投影记录仍在夹具内保留并验证，不把移除旧在线授权等同于完成离线转换。

联合core168/app88（3条件忽略）/Fake64通过artifacts/protocol-redesign-p3-snapshot-tests.log；现行完整投影先通过artifacts/protocol-redesign-p3-current-snapshot.log。新增帧级断言定向通过artifacts/protocol-redesign-p3-snapshot-frame.log；删除无用接口/夹具后cargo check app+device-sim --tests通过artifacts/protocol-redesign-p3-snapshot-check.log。git diff --check通过。固件未改，.87候选保持匹配；未提交、未启停默认Bridge、未部署。

剩余删除命中缩至固件wifi_management.inc/transport.cpp及render的模拟后端/旧格式测试；DeviceSim默认旧协议选择也待收敛。全层冻结TransactionKey、唯一业务执行器、一次性安全格式转换和原生实证仍未完成，P3不结项。

## 2026-10-13 00:04 P3 固件旧在线入口删除

serviceV2Ble删除wifi_authorize成功分支、wifi_migrate在线导入/删除旧NVS键的handler，以及未知Wi-Fi命令转入无context wifiSetupCommand的后备调用。未知/旧操作统一unsupported，现行控制仍经endpoint身份/principal/token及wifiControlContext，显式owner/capabilities只读入口保留。在线迁移分配/读旧凭据/退役旧键逻辑整段删除；没有擦除或迁移现场数据。库内部旧管理分支和测试仍待后续删除，不能据生产分派收敛宣称完整单栈。

生产C++管理测试新增旧命令/未知命令零NVS和WiFi.begin副作用断言，与原回执/恢复/空间矩阵共10组通过（artifacts/protocol-redesign-p3-retired-wifi.log）。Fake新增current_wifi_rejects_retired_commands_over_http_and_ble_without_mutation通过（artifacts/protocol-redesign-p3-retired-carriers.log），验证两载体返回unsupported、revision0/空配置/非busy。现有Fake64项上一批已通过，本次新增1项独立通过，不宣称实机BLE/Flash验证。

Note4候选0.18.88-note4，env zectrix-note4-b，marker zectrix-note4-400x300，37.79s构建通过（artifacts/protocol-redesign-p3-retired-note4-build.log）；RAM119044 B、Flash1781667 B，比.87减少7456 B。没有新增持久字段，空/单条/典型/上限新增NVS字节均0；删除在线迁移的临时WifiNetwork[MAX_SLOTS]堆分配及调用链，未测设备动态峰值。

候选仅保留未部署：
- artifacts/protocol-redesign-note4-0.18.88.bin，1833392 B，SHA256 c61840625b7ab424d18776164f1c24cbb1b1c6468151a7513728101ddd8bc7a0。
- 匹配.elf，30430064 B，SHA256 a74fe6cd6a56fb20f5d9aaa66a90399063ebb673c51e82d73db4c3d1fc5b817d。
- 匹配.map，20244179 B，SHA256 019508e68942528ef758bd6cbcd763187bff74fe72a81015d7a809a75aa70253。

check-wifi-stack.py新增显式--no-online-wifi-migration：要求最终ELF不含该handler，其他现行栈项照常必需；旧ELF默认模式继续保留原要求，不用删除检查掩盖历史大栈。新ELF62项现行帧/链及1项删除检查通过（artifacts/protocol-redesign-p3-retired-stack.log）；.87在新删除门禁下按预期失败，在默认历史模式通过（...retired-gate-red.log、...historical-stack.log）。候选哈希/符号另存...retired-candidate.log。serviceV2Ble帧944 B（门槛1024 B），仍需监控后续修改。未启停默认Bridge、未烧录。

下一批需删除wifi_setup.cpp/wifi_management.inc内的无context/permit实现与模拟器默认旧模式，同时完成安全一次性转换与TransactionKey冻结。旧数据只能在安全转换门禁完成后发布，当前候选不放行。

## 2026-10-13 00:12 — P3 删除库内旧在线迁移实现

删除已无生产调用者的 wifiSchemesMigrate 定义及公开声明；移除只针对该退役入口的测试，保留 BLE 512 B 错误信封边界（改用当前 wifi_save 操作）、v1 持久恢复/损坏保护及当前协议事务检查。源码 src/tools/bridge/crates 搜索无残余调用。没有删除持久格式兼容，也没有迁移真实数据；安全离线转换仍是发布前置条件。

生产 C++ 管理回归通过，日志 artifacts/protocol-redesign-p3-migration-library-test.log，覆盖原子提交/重放、当前协议、活动配置保护、跨 namespace 回执、WFS5 中断恢复和旧命令拒绝。删除旧测试不代表其历史功能继续受支持。未新增持久字段，空/单条/典型/上限新增持久字节均为 0；动态 RAM 峰值未测。

唯一目标 zectrix-note4-b 构建 51.52 s，通过 marker zectrix-note4-400x300 / 0.18.89-note4 核验；仅候选、未部署。RAM119036 B、Flash1781275 B（较 .88 分别减少 8 B / 392 B）。实际 ELF 栈门禁 63 项通过，符号表确认在线迁移 handler 及库函数均不存在。
- bin: artifacts/protocol-redesign-note4-0.18.89.bin，1833008 B，SHA256 3f2dfe316ed5b4b1d527cb67d3ad9f948b90b23c9f488c80a61ff1ab02f93295。
- elf: 同前缀 .elf，30403008 B，SHA256 817cbaf771fdca1b39c0d845af0076b91ceaeab8ef0c715908de0c2509d682d1。
- map: 同前缀 .map，20241282 B，SHA256 ed9469e4ac6dc09e1557913b03e8c7ec58be28ae802300da0f0e5346acae60d6。
- 构建/镜像/栈证据：artifacts/protocol-redesign-p3-migration-library-{build,candidate,stack}.log。

下一步继续旧 wifiSetupCommand/permit 与模拟后端的收敛；测试仍直接调用旧管理分支，不能将本增量算作 P3 完成。未启停默认 Bridge，未访问设备或烧录。

## 2026-10-13 00:18 — P3 Fake 只保留当前协议

删除 device-sim 的 unified 开关、CODEX_STATUS_SIM_WIFI_PROTOCOL 环境选择、management2 HTTP/BLE 后备、旧 1024 B 请求分支以及用 light 冒充 Wi-Fi 联网的显示分支。模拟器固定 management3，HTTP/BLE 均走生产 WifiControlContext。bootstrap 取消 --wifi-unified-test 私有标记，全部测试默认当前协议；app 的两个 Fake 启动夹具也移除已废弃变量。

首轮完整回归 61/65，4 项失败均定位并修复：BLE 有界读取/恢复测试补充真实 claim 和 status 会话；trial 使用持久回执，断言无保存配置而非没有 NVS 文件；两项同步重启测试在完整状态超过 4096 B 后实际取得分页信封，改用生产 device_client::status 完整读取，不扩大帧限制、不删除字段、不放宽断言。最终完整 65 passed / 0 ignored（53.56 s），包含断电批次恢复、跨载体原 ID 确认、旧操作拒绝和两种会合流程。

证据：artifacts/protocol-redesign-p3-single-sim.log（首轮），...single-sim-restart.log（分页根因），...single-sim-green.log（最终完整通过）。没有新增持久格式或 MCU 分配；此次仅宿主/Fake 改动，不重编 ROM，.89 候选及哈希继续有效。未部署或启停默认 Bridge。

补验 app 条件测试时发现旧满队列构造在第二项即被当前 wifi_confirmation_pending 阻止（...single-sim-app.log）。测试改为验证一个本地取消但尚未确认的原事务即阻止自动激活，连续尝试仍 attempts=0、无第二个设备回执且 RF begins=0；定向通过（...single-sim-app-green.log / ...single-sim-app-final.log）。core 的 wifi_retirement_preserves_unknown_submissions_and_pending_receipt_acks 仍覆盖32项容量保留/退役，未放宽保护。另一个依赖本地私有接口的条件测试本轮未运行。

边界：render 的 SimulatorWifi::command / C++ 旧适配仍被一项重复旧 ID 持久恢复测试用于造数据；固件库旧 permit/management2 实现也未删除。需要独立旧格式离线夹具/转换后继续移除，不能将 Fake 业务入口单栈等同于全仓单栈。

## 2026-10-13 00:24 — P3 删除 render 旧命令适配

移除 SimulatorWifi::command、codex_sim_wifi_command Rust 声明与 C++ 分派实现。唯一剩余调用原用于构造跨 namespace 重复 ID 数据；现改为固定 265 B 合成 WFS5 夹具（SHA256 3917660c159f834a9474d6939baf1ad4a6d328f147cf09f4814e03ea8733b2d3），来源是删除前旧 management2 writer 生成的 mac/windows 两个同名 open 配置及两份 save 回执，revision2。夹具嵌入测试，未使用现场数据或真实凭据。校验首次加载不改字节、两次重启后歧义删除明拒、歧义激活明拒、配置数和revision保留；无需恢复旧执行入口。

同时将 tools/test-wifi-storage.cpp 的分配故障与 commit-then-error 查询改走 wifiControlCommand + 显式合法上下文，删除 grant/permit 测试前置。140 次故障探针中36处实际触发分配失败，原子提交/未知结果确认、损坏/截断、容量与活动凭据保护均通过 ASAN。资源测量未变：空/单条/典型/满量 blob41/251/431/5188 B，组合上界6382 B；host clone-save峰值上界21536 B，不能代替实机动态峰值。

验证：render lib13项通过（artifacts/protocol-redesign-p3-render-no-old-adapter.log）；存储ASAN（...p3-current-storage-asan.log）；app/device-sim全部tests编译通过（...p3-render-adapter-check.log）。夹具生成原始合成记录见...p3-duplicate-fixture-capture.log。此前Fake65项保持上一批证据，本批没有重复宣称全套重新运行。

仍开放：固件库 wifiManagementAuthorize/wifiManagementCommand/无context wifiSetupCommand，以及 C++ 管理测试旧语义；持久格式安全转换、P1/P2其余与P4–P6仍按原计划推进。此次只改宿主适配及测试，.89候选ROM不变；未部署、未启停默认Bridge。

## 2026-10-13 00:28 — P3 删除 save/result 临时 permit 后备

C++ 正式窗口批量保存/结果查询、永久围栏及sole-active删除场景改为 wifiControlCommand + 显式上下文。跨principal测试区分伪造body的principal_mismatch与合法不同principal查询unknown；namespace隔离、提交后错误重启确认、旧任意ID回执保留和窗口边界断言保留。

据调用清单删除 wifiSetupCommand 中 wifi_save/wifi_result 临时签发permit、转无context管理的旧适配。生产当前save/result继续在wifiControlCommand直接分派。库内旧管理函数和旧setup分支仍被首批C++测试调用，源码尚未完全退役；本次链接器已移除wifiManagementAuthorize/Command入口，不等于整库单栈。

C++管理回归通过（artifacts/protocol-redesign-p3-save-adapter-test.log）；Fake当前合同/旧命令拒绝/完整投影3项通过（...save-adapter-fake.log）。Note4 .90唯一env zectrix-note4-b构建64.56s，marker zectrix-note4-400x300/0.18.90-note4核验通过（...save-adapter-build.log、...save-adapter-candidate.log）。
- bin artifacts/protocol-redesign-note4-0.18.90.bin，1830160 B，SHA256 98fe73b2af5f3d94dcf26b525bf800b0840cf479339321b164ecf97098fb7b94。
- 匹配elf 30357052 B，SHA256 ee407566f4fa70095e551358bf23d05907bd9e47723bb75d0c9a29b7b9500a64。
- 匹配map 20227649 B，SHA256 9d0e0a1068ebdff427337bb1f8c1f2d3a5236d4e2b6a0fbf87068355cc75f54d。
- RAM119036 B、Flash1778511 B，较.89 Flash减少2764 B；无新增持久字段，空/单/典型/上限新增NVS字节均0，动态资源峰值未测。

栈工具新增--no-legacy-management-entry，明确要求两个旧入口缺席；当前帧/链60项、在线迁移删除和旧管理入口删除2项，共62 PASS（...save-adapter-stack.log）。.89新门禁按预期FAIL旧管理入口仍链接，原模式通过（...save-adapter-gate-red.log、...save-adapter-historical.log），没有降低现行帧1024 B或局部链4096 B门槛。仅候选未部署，未启停默认Bridge。后续继续迁移首批C++旧语义检查并删除permit/旧setup实现，以及安全数据转换和原P0–P6其余项。

## 2026-10-13 00:35 — P3 删除 permit 管理实现

全部 tools/test-wifi-management.cpp 场景已改走 wifiControlCommand：设备级owner读取/管理全部来源，namespace仅作来源/事务隔离；伪造bridge_id明拒，合法其他principal查询unknown；trial持久回执但不保存候选配置；session reset不撤销已持久成功回执；期限耗尽不启动回退RF且保留旧活动凭据。独立RF夹具避免与容量测试共享已满回执。保留分页过期、坏批次原子拒绝、储存未知、回执重放/围栏、激活/DHCP/认证失败/期限、活动删除、WFS5重启保护；新增伪造permit/namespace不能绕过claim断言。

删除 WifiPermit/wifiPermits/findWifiPermit、wifiManagementAuthorize/Command及头文件声明。wifiManagementWithContext改为必需引用，移除null-context及按permit过滤/命名空间查找分支，可信上下文直接决定principal/namespace/window/owner。源码检查无旧管理符号调用；持久格式未改，旧数据加载/回执保护未删除。wifiSetupCommand中的旧setup/select/delete及transactionUnified双路径仍待处理，不能宣称P3结束。

验证：生产C++管理完整回归（artifacts/protocol-redesign-p3-no-permit-test.log）、存储ASAN（...no-permit-storage.log）、render13项（...no-permit-render.log）、Fake完整65项/0忽略（...no-permit-fake.log，62.07s）均通过。存储仍36个实际分配失败点/140探针，blob空/单/典型/满量41/251/431/5188 B，组合上界6382 B、host clone-save峰值21536 B；本增量空/单/典型/上限新增持久字节均0。实机动态峰值未测。

Note4唯一env zectrix-note4-b候选0.18.91-note4，56.54s构建，marker zectrix-note4-400x300核验、permit仓库/入口符号缺席及62项栈/删除门禁通过（...no-permit-build.log、...no-permit-candidate.log、...no-permit-stack.log）。RAM118844 B、Flash1776859 B，较.90分别减少192 B/1652 B。
- artifacts/protocol-redesign-note4-0.18.91.bin，1828416 B，SHA256 d15a5ec43a7f0aeaca0f25d67680a80d8661bb85072f7f70d359383e5938fecb。
- 匹配elf 30331272 B，SHA256 da693d26c31b50fc19bc861db70f01f1e09368e6b45b189e0cb8298f4846fb7b。
- 匹配map 20222959 B，SHA256 39661d894e4c0592e501a225c7500f0b8020f6628c553d6cb5406def72b5dc3f。

仅候选未部署，未启停默认Bridge、未访问真实设备。下一步旧setup源码/旧test-wifi-setup.cpp仍需迁移删除；安全转换、事务身份及原P0–P6其余范围保持不变。

## 2026-10-13 00:44 — P3 删除旧setup执行栈；补P1 principal隔离

tools/test-wifi-setup.cpp改为当前trial/save/activate分离合同：trial先存回执、不保存候选；完整DHCP才终态；BLE连接变化不取消业务；owner管理全部来源；期限不延长；旧命令拒绝；12条配置的最大转义分页。删除原wifi_setup自动保存/激活、wifi_select和wifi_scheme_delete成功断言，相关保存/活动删除仍由管理测试覆盖。最大最终BLE回复420 B，保持512 B界限。

固件删除旧setup/select/delete分派、save(WifiSchemes&)替代存储路径、unified选择参数、transactionUnified/transactionGeneration/replaceNetwork状态、断链取消与旧完成分支。wifiSetupPoll只接收now和RF startAllowed，application及Fake适配同步；删除wifiSetupRadioClosed空行为及radio调用。当前trial/activate统一使用持久pending/终态和原期限。

清理发现旧内存fingerprint重放只核对namespace/requestId，跨principal同号可能命中上一事务；删除该捷径，全部重放由上层持久三元身份回执处理。setup_status/cancel补selectingBridge核对。新增生产测试：同namespace/ID、不同principal分别保留两份trial回执；原principal查询自己的完成结果、另一principal查询其accepted结果；原principal取消不能停止另一principal事务。此补丁覆盖设备端缺口，不代表Bridge全层TransactionKey已完成。

验证：C++管理及新增身份检查（artifacts/protocol-redesign-p3-no-setup-management.log）、setup帧（...no-setup-wire.log）、存储ASAN（...no-setup-storage.log）、render13项（...no-setup-render.log）、Fake完整65项/0忽略（...no-setup-fake.log，58.52s）均通过。存储仍36处实际分配故障/140探针；持久格式和空/单/典型/上限字节未变，新增NVS字节均0；host组合峰值21536 B，实机动态峰值未知。

唯一Note4 env zectrix-note4-b候选0.18.92-note4，56.68s构建；marker zectrix-note4-400x300、旧生命周期符号缺席及62项栈/删除门禁通过（...no-setup-build.log、...no-setup-candidate.log、...no-setup-stack.log）。RAM118788 B、Flash1774743 B，较.91减少56 B/2116 B。wifiSetupPoll帧96 B、局部链400 B；wifiSetupCommand帧576 B、局部链1312 B，仍不涵盖间接平台调用。
- artifacts/protocol-redesign-note4-0.18.92.bin，1826240 B，SHA256 011211093b0323057a6bcbebfeda095be203628401d89fa03c66a0eae1e3287d。
- 匹配elf 30306192 B，SHA256 83df98933f7ea5292d1845ef926aa0f246e6f53bb539491bc817f5ce397ba5e6。
- 匹配map 20214883 B，SHA256 16f867a377d303233421a8273ba64ce1e186fa5f14bdbd01ffa8f19acc1aab0a。

仅候选未部署。安全离线格式转换/真实数据保护、Bridge完整事务身份、统一有界执行器、原生BLE恢复及P4–P6验收仍开放；不把旧在线栈删除等同于整个计划完成。

## 下一步与未验边界

### 2026-10-13 01:39 P0 Bridge未知持久字段拒绝与只读清单

根PersistedState、WifiState和Job原先按serde默认忽略未知字段，未知事务约束可能被丢弃。新增测试先复现根对象的unknown字段被接受，再为三层启用deny_unknown_fields。新程序遇到未知字段会拒绝加载并保留原文件；app初始化直接传播加载失败，没有空平台替代路径。已知旧作业字段缺namespace仍允许读入，但确认义务与新业务写阻断保留，不能猜测身份。原文件字节不变与根/Wi-Fi/job三层拒绝均回归通过。

core默认功能150、app90/3条件忽略、Fake65通过；日志 `artifacts/protocol-redesign-p0-state-format-red.log`、`...-core.log`、`...-consumers.log`，diff检查通过。该行为有意拒绝未来或不认识的字段，不能把拒绝升级变成清空文件重启；当前已知格式兼容回归通过不等同于所有现场文件均已验证。

按运行数据规则首次只读统计本机配置会话数据根/默认exe数据根的state.json：227255 B，SHA256 `1f9742fdee96bd47e4a87c5a67f4abac81d994d65c7ce826ad2985f26aa18443`；登记2设备、Wi-Fi仓库1份、作业15个，confirmation_pending=0、receipt_ack_pending=0，三层未知字段计数均0。只保存不含凭据/记录原文的统计 `artifacts/protocol-redesign-p0-local-state-inventory.json`。没有冻结运行写入，未独立验证进程数据根覆盖设置，统计是该文件的时间点快照；不推广到macOS、其他实例或设备NVS，不等同于完整类型/语义验证。

未改写真实数据、未访问设备、未转换/部署。BRS离线迁移、Bridge/设备联合身份清单及安全升级仍开放；P0–P6未完成。

### 2026-10-13 01:32 P0/P3 Note4旧ABI离线转换

从Note4 .70与.93匹配ELF的DWARF分别提取旧类型布局，四种类型的大小和全部字段偏移一致。证据 `artifacts/protocol-redesign-p0-legacy-layout-0.18.70.json`（ELF SHA256 `70670516dba389c87dd14116d81f8e63931e8b5515e746870198b3a6caccd333`）和 `...-legacy-layout.json`（.93 SHA见候选记录）。

| 类型 | 字节数 | 字段偏移（按声明顺序） |
|---|---:|---|
| WifiLegacyNetwork | 186 | bond0、id22、host55、ssid88、password121 |
| WifiLegacySchemes | 2296 | version0、networks4、selectedBond2236、selectedId2258、crc2292 |
| WifiLegacyReceipt | 232 | bridge0、request65、bond130、hash152、error184、revision224 |
| WifiLegacyStoreV2 | 5248 | schema0、schemes4、active2300、revision2488、activeVersion2496、versions2504、labels2600、receipts3384、crc5240 |

新增仅宿主包含的 `tools/wifi-legacy-convert.inc`，按这些明确小端偏移解码，不使用宿主struct布局。v1核验完整长度/版本/CRC，v2核验内外CRC与字段终止符；WFZ2解压严格限5248 B并继续同样校验。通过当前WifiStore形状检查后统一写WFS5；原事务三元身份、指纹和pending保留，旧格式没有的代际/高水位不补造，sequenced回执缺cursor明确拒绝。

合成v1/v2/WFZ2、配置/凭据/active和未知回执保留、全部截断/CRC覆盖字节翻转、压缩截断/超限、缺cursor拒绝测试通过。工具共3项、render完整13项及最终CLI帮助通过；日志 `artifacts/protocol-redesign-p0-legacy-convert.log`、`...-render.log`、`...-help.log`。diff检查通过；无固件变更，.93候选不变，未访问设备或转换真实数据。

下一步仍是BRS旧键、Bridge作业与设备回执联合清单/身份恢复、不能恢复时可执行写围栏以及最终升级验收。旧在线格式读取不能仅因该工具可运行就删除；P0/P3不结项。

### 2026-10-13 01:25 P0/P3 显式格式离线转换工具

新增独立CLI `wifi-store-convert`（`tools/wifi-store-convert.rs`，由bridge-render构建），宿主FFI直接复用固件WFS3/4/5解码校验与WFS5编码。转换不调用 `wifiSchemesBegin`，不打开模拟NVS、不执行启动中断恢复或修改全局事务状态；旧pending、指纹、principal/namespace/request_id、cursor高水位、配置/密码/active及已有执行代际原样保留。显式格式之外拒绝，不按当前绑定补猜旧字段。

文件入口有8192 B上限；仅新输出，排他暂存、同步/读回、同目录硬链接无覆盖发布，保留原输入。已有输出/暂存（含输入别名）拒绝；正常失败清除本次暂存，不创建目录、不写真实设备。CLI只输出长度。Cargo保持bridge-render原默认运行目标，原预览命令不受新增binary影响。

合成WFS3/4/5转换、逐字节配置/凭据/active及回执字段、高水位、幂等、全部截断/位损坏、未知版本、超限、无效cursor、WFS5执行代际不被恢复逻辑改写、输入/输出不覆盖共2个测试通过；共享render库完整13项通过，CLI帮助实际运行通过。日志 `artifacts/protocol-redesign-p0-offline-convert.log`、`...-render.log`、`...-help.log`；diff检查通过。操作说明已加入agent-operations。

未转换真实数据/未部署；.93固件候选不变。此工具只完成显式设备blob转换：v1/raw-v2/WFZ2仍需匹配布局证据，BRS旧键、Bridge未知作业身份与设备回执联合核对、可执行写围栏及真实升级流程尚未完成，不能因此删除全部兼容读取或关闭P0/P3。

### 2026-10-13 01:18 P0/P1 绑定库损坏不得重建身份

检查设备提交边界时发现：`load(Endpoints&)` 将 `records2` 长度零当作旧格式入口。当前格式键存在但损坏为空时，物理窗口下 `storeUpsert` 可把它当空库并创建新namespace。新增生产绑定库测试先复现 `truncated BRS2 accepted at 0 bytes`，再修复为检测键存在并返回storage；所有长度0至完整长度前一字节的截断均拒绝认证和物理注册且保留原字节。测试HAL的长度读取改为无插入副作用，补isKey以匹配Preferences语义。

既有同principal换peer必须物理授权、授权后namespace保持、同token不能换principal、损坏/截断、原子迁移与容量不驱逐测试全部通过。日志 `artifacts/protocol-redesign-p1-binding-red.log`、`artifacts/protocol-redesign-p1-binding-green.log`。源码仅新增存在性检查，不改持久编码/仓库数量；实测空库无blob0 B、单条108 B、典型两条187 B、八条697 B、最大字段八条2856 B。无新增应用堆分配；实际动态NVS内部成本仍归实机资源验收。

默认 `zectrix-note4-b` **0.18.93-note4** 候选构建32.41s成功，marker `codex-status-ota-v1|zectrix-note4-400x300|0.18.93-note4` 核对；RAM118788 B不变，Flash1774755 B（+12 B）。匹配ELF的62项栈/删除门禁通过；storeUpsert帧128 B/局部链560 B、storeAuthenticate帧64 B/局部链496 B。证据 `artifacts/protocol-redesign-p1-binding-build.log`、`...-candidate.log`、`...-stack.log`。

- ROM `artifacts/protocol-redesign-note4-0.18.93.bin`，1826256 B，SHA256 `10a99b7975ec7ff18ead83b848be77aa978f62c6b1be65ccd53479ed292b0005`。
- ELF同名前缀，30306264 B，SHA256 `9f17e8de6b1e30c8dfdc8c9a0a220b75a395011cc2ff6971ff93d2fa11df4e34`。
- map同名前缀，20214984 B，SHA256 `f4d44ddc53b6f7ec547e17a90c5c61b5c037433023d805466e565a71cc83c485`。

未部署/访问设备。正常重绑定namespace不变、唯一storeClear调用位于factoryReset并清auth后重启是代码证据；Flash故障、复位时并发与清除后旧会话拒绝尚未完整实证，因此不据此关闭全部设备派发身份边界。安全离线转换、删除剩余旧格式及P0–P6其余继续。

### 2026-10-13 01:11 P1 无活跃作业的 BLE 回执退役

原BLE退役循环只在 `unified_on_link` 执行业务后运行，已完成且仅剩待ACK的作业不能独立退役。现提取共享退役循环，并在成功会合结束、关闭BLE前独立处理待ACK；只在确有待ACK时读取本轮认证status，复核原principal/namespace，使用现有shared_tail剩余预算并最多占500ms，不建立新业务作业、不重发原写、不授予PowerPlan。原业务完成后仍复用已读status，避免重复读取。

生产 `ble_cycle` 新测试覆盖Bridge重启后无runnable作业、首次ACK丢失后下一会合使用原query_id、status读取超时后下一会合恢复、namespace不符保留待ACK；断言没有save/trial/activate/result/plan/network_start。app全量90通过/3条件忽略（58.12s），随后仅扩展status超时测试，定向最终1.21s通过。日志 `artifacts/protocol-redesign-p1-ble-ack-app.log`、`artifacts/protocol-redesign-p1-ble-ack.log`；diff检查通过。未部署或访问设备；真实原生BLE恢复、设备派发时身份变化及旧格式安全转换仍未完成。

### 2026-10-13 01:06 P1 core 强制身份接口

`wifi_attempt` 必填 principal/namespace，在任何写入/原ID查询的传输回调前通过共享持久绑定检查；不匹配时保存明确错误，保留 sent/未知确认义务。`wifi_receipt_acked` 也要求原身份，错误身份不能清除待ACK。内部回执确认/确定拒绝方法收为私有；删除无生产调用者的 `wifi_confirm_recovery` 旁路，其凭据保护测试改走现行原ID回执查询。app及Fake全部调用者已更新，Fake身份来自认证status。

新增直接调用core而不经过app守卫的回归：首次发送前/已发送两态，principal变化/namespace变化/namespace空值，均不调用传输；重启后原身份可继续原ID，错误ACK保留待退役；旧已发送缺身份不可猜测。core默认功能149、app89/3条件忽略、core集成22、Fake65全部通过；联合日志 `artifacts/protocol-redesign-p1-required-identity.log`，core单独日志 `artifacts/protocol-redesign-p1-core-identity.log`，diff检查通过。未提交/部署/访问真实设备。

后续仍需独立BLE待ACK调度及设备实际提交时身份变化验收。代码已确认 `storeUpsert` 对既有principal保留namespace，但清除绑定再建立等场景仍需完整证据，不能据此关闭派发围栏。旧持久数据安全转换和P0–P6其余范围继续开放。

### 2026-10-13 00:58 P1 Bridge namespace 持久化与生产入口围栏

作业新增可缺省 `transaction_namespace`，通过既有原子状态存储在首次派发前冻结；后续不能换 principal/namespace。已发送、可能提交或待ACK的旧作业不允许按新连接补猜身份。HTTP机会、BLE写/确认机会共用绑定服务；三个现有ACK路径均检查认证状态与作业保存身份。纯读不建立写事务身份，不增加身份读取预算。BLE先保留host变更检查，再用本轮有界认证status取得控制身份。

新增持久恢复、原ID保持、跨principal/namespace拒绝、旧已发送缺身份保留未知、ACK字段缺失/不符回归。生产BLE恢复测试增加换namespace和缺失身份两例，断言不发save/result/result_ack并保留确认义务。旧测试后端补齐wifi_status分页与真实身份字段，已发送夹具显式保存原namespace。

`cargo test -p bridge-core -p bridge-app --target-dir ../artifacts/cargo-target-convergence` 通过：app89/3条件忽略，core默认功能148、集成7+7+8通过。日志 `artifacts/protocol-redesign-p1-namespace.log`。仅宿主变更，未部署/未触碰真实数据，.92固件候选不变。

仍未结P1：core `wifi_attempt`/回执接口本身尚未强制传入身份，需将入口围栏收束成不可绕过的共享接口；还需验证认证读取后、实际提交前身份变化的设备端围栏，以及无活跃作业的BLE待ACK处理。旧持久未知作业身份恢复必须由有证据的离线转换处理，不从当前namespace推定。P0–P6其余范围不变。

### 2026-10-13 00:48 P1 控制作业去重身份

`wifi_queue_control` 的提前去重原来只比较 kind/body，可向另一 principal 返回原 trial/activate 作业，并绕过底层 bridge_id 长度校验。现先校验 principal，并仅向同 principal 返回同内容作业；其他身份明确返回 wifi_control_busy，不修改原状态。新增回归覆盖两类命令、重启后原 ID 重用、外来/空/超长身份拒绝及再次落盘恢复一致。定向测试通过，默认功能 core 库 147/147 通过；日志 `artifacts/protocol-redesign-p1-control-principal.log`、`artifacts/protocol-redesign-p1-control-principal-core.log`。此计数不替代此前其他功能配置的测试。未部署；完整 namespace 冻结、发送/查询/ACK 身份围栏尚未完成。

继续P1全层TransactionKey/身份变化；P2完整BLE代际/清理隔离；P3所有入口唯一执行器、排队绝对截止与安全一次性数据转换后删除旧栈；P4独立OTA实际handler失败矩阵；P5统一状态投影及本地两小时证据；最后P6候选与实机矩阵。macOS原生回调、ESP32Flash/RF/功耗不可由Fake代验。

## 2026-10-13 01:51 P3 共用异步业务核心，删除BLE线程中转

- `wifi_attempt_async` 是唯一业务状态转移实现，载体只实现 `Transport::exchange`。同步HTTP与现有Fake调用经Ready适配单次poll，未创建运行时/线程，仍由原HTTP worker持有设备许可；BLE直接await载体，无双向mpsc、阻塞recv或专用业务worker。未新增依赖。
- BLE沿用原预算、命令ID、认证namespace及设备许可；读取/snapshot与命令映射集中在BleTransport，结果查询仍保留原query_id，ACK仍先持久确认后退休。
- 异步取消守卫将未发送作业收回waiting_rendezvous，将已发送作业收回awaiting_confirmation。取消不清sent/未知/原事务身份；持久收尾失败记录明确错误，不冒充成功。底层控制写的poison与链路收尾机制继续生效。
- 新增取消回归覆盖永久pending写入、同进程原ID查询、重启后原ID查询及禁止新控制写；未发送读取取消后可直接重新begin，不要求重启。core151及最终定向1项通过，app90通过/3条件忽略（59.20s）；Fake65通过；最终结果见同名前缀日志。
- 证据：`artifacts/protocol-redesign-p3-async-{core,app,fake,cancel,diffcheck}.log`。本轮无固件改动、无部署、无真实设备/数据操作。此处只收敛Wi-Fi业务驱动，不代表Data/Plan/Bundle及全入口执行器、P2原生生命周期、P5或P6完成。

## 2026-10-13 01:58 P0/P3 Wi-Fi运行时仅接受WFS5

- 删除固件WFS3/4、v1/raw-v2/WFZ2读取、自动转换及启动/保存时旧scheme键删除。当前reader只接受WFS5，离线工具保留旧显式/ABI解码，WFS5转换复用当前reader。无新协议降级或在线转换入口。
- `Preferences::isKey`区分不存在与现有零长度/错误类型键。当前键或旧scheme存在但不能读取时storeReady=false，保留全部原字节、拒绝业务写入；诊断格式为unreadable而非empty。当前WFS5旁的旧键只报告存在，不擅自退役。Fake文件模型同样拒绝已有空文件，不再按旧magic搬到另一个键读取。
- 生产管理回归通过；ASAN存储覆盖旧格式拒绝/保留、已有空键、当前格式损坏与截断、36个分配失败点/140探针、提交后错误与重启确认。离线转换4项含运行时拒绝原文件、转换后恢复并保留回执/cursor；render13及其他宿主集成通过，Fake65通过（25.31s）；app90通过/3条件忽略（58.78s）。
- WFS5格式/实际持久字节未变化：空41 B、单网络单回执251 B、典型2+2为431 B、12+8为5188 B、组合上限6382 B；宿主clone/save峰值增量314/1175/1900/17690 B、组合21536 B。只是宿主请求字节，非ESP32实测动态峰值。删除旧格式路径也删除原生5248 B兼容副本和Wi-Fi专用inflate工作区。
- 仅候选 **0.18.94-note4**，env zectrix-note4-b；marker正确，构建41.92s，RAM118788 B不变，Flash1772647 B（较.93减少2108 B）。ROM `artifacts/protocol-redesign-note4-0.18.94.bin` 1824144 B，SHA256 `20826f653f6842231aa7d3eaf2ac7e4e7e54a574ec526a1d61d983f11ab8444f`；ELF30277040 B SHA `850a113cf1cb8a691dbf4caae56d9a0c7e47f57d03d63125f9d9935dfb0e365e`；map20205939 B SHA `7a43b8a1a3e9d76852b4291be06c84acbbaea29a7d3b4f61c94235c681da8282`，同名前缀保存。
- 匹配ELF旧wifiConvertLegacy/wifiDecode3/wifiInflate/WifiLegacy符号均不存在；62项栈/旧入口门禁通过。readWifiStore帧80 B/本地链320 B，启动帧224 B/链544 B。保留通用压缩代码不代表保留Wi-Fi旧持久读取。
- 证据统一前缀 `artifacts/protocol-redesign-p3-storage-current-`：management/asan/render/fake/app/build/candidate/stack/diffcheck.log。未访问设备、未改真实数据、未部署；BRS旧记录绑定转换、Bridge联合围栏、P1/P2/P3全业务执行器/P4实机/P5/P6仍开放，不能发布。

## 2026-10-13 收尾提交（用户要求停止推进并保留进度）

- 用户要求快速结束当前任务、保留进度并提交；这是阶段性快照，不是P0–P6结项，不部署。后续从本节及PROGRESS继续。
- BRS旧逐键读取、从当前peer补猜namespace、自动删除旧键已删除。BRS2必须包含principal/identity/namespace；孤立旧键、空键或缺身份记录拒绝认证和物理/非物理注册，保存原字节；完整当前BRS2仍可正常更新，旁边旧备份键保留。
- 生产BRS红绿：旧版在旧token认证断言失败，修复后旧整库/孤立键/空键/三类缺身份/逐字节损坏与截断/持久写失败/已提交但返回失败/当前换peer保留namespace/容量不驱逐全部通过。持久成本仍为单条108 B、典型187 B、8条697 B、满字段8条2856 B，不新增格式字段。
- **继续前优先补验**：BLE endpoint注册入口丢弃storeUpsert具体错误（`handleBleEndpoint`只返回bool）；Rust `write_endpoint`只等待GATT写入，不等待持久ACK。需要关联本次请求的结构化回执及Bridge/UI完整错误链，防止BRS迁移阻断被误报成功。本次只记录发现，未展开回执协议改造。设备首次启动会由application.cpp创建brg命名空间，未把Preferences只读open失败猜成空库。
- BRS旧库离线转换尚未实现；缺身份时当前运行时阻断写入，不补猜身份。实际联合备份/转换/部署和P1/P2/P3全业务/P4实机/P5/P6仍开放。
- 证据：`artifacts/protocol-redesign-p3-binding-current-{red,green,build,candidate,stack}.log`；候选最终身份见PROGRESS。

最终候选0.18.95-note4构建28.42s，RAM118788 B/Flash1771331 B，62项栈门禁通过；ROM1822768 B SHA256 `844d02066561f9d874b17e23683771a627478e8f87ee82dfaf7e660056f550a1`，ELF30265616 B SHA256 `7c48c64482cee05d18d6d7109d25eecfcfe8a194ba29aafe482d9543510a0c5a`，map20204959 B SHA256 `a893ffc06b3c3e530d168d16eedfefb1471a60da4c6c16dc76d4a1683b5ede97`。均为artifacts/protocol-redesign-note4-0.18.95同名前缀，未部署。
