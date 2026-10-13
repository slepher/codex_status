# 协议重设计实现审查

2026-10-12 22:44；范围为P0/P1当前源码、宿主与候选证据。结论：增量通过当前回归，P1仍开放，P2–P6未完成，不能发布或标记目标完成。

- OperationReply查询错误只改本次联系结果，不改原sent/提交可能性；显式成功的终态才可确认。取消/TTL保留确认义务，错误后沿原ID/backoff；新冲突写明拒。
- 设备回执三元键匹配覆盖原apply重放与ACK高水位；Bridge目前仍仅冻结bridge_id/request_id，namespace来自认证连接，身份变化围栏需补齐后才能宣称全层一致。
- WFS5边界/校验后分配，分配失败明拒；interrupted先保存并读回才可查询。active与activate成功原子一致，新trial终态可重启查询；旧代际0未知不推断、不清空。WFS3/4/legacy兼容为候选过渡，必须按P3转换验收后退役。
- HTTP进度只在socket实际写入后升级，session-open失败不等于业务帧已发。anyhow context的类型检查使用downcast_ref（普通chain不能可靠取得上下文类型）。生产write循环的短写/超时/零写/过期截止注入已通过，真实loopback覆盖session与完整帧丢ACK。
- DeliveryLock保留真实OwnedMutexGuard的共享引用；只有已取得设备许可的调用路径可spawn worker，worker退出或panic后才释放其引用。loopback取消测试证明阻塞socket没有随async取消放行，第二设备独立；现有全局BLE锁/调度顺序/未收敛入口仍是P2/P3工作，未借本修复宣称完成。
- app87/3忽略、Fake64通过；core168、C++生产管理及ASAN存储故障注入通过。曾遇到旧storage容量断言与Fake旧限额/错误字符串断言失败，依据现行合同修fixture，保留原因于status。未延长生产超时以掩盖失败。
- 固件候选marker/哈希/匹配ELF64栈项通过；没有部署或实际Flash掉电/RF/功耗验收。宿主堆数据不是ESP32动态峰值。

具体候选/资源/日志见[状态](protocol-redesign-status.md)。继续原[P0–P6计划](protocol-redesign-plan.md)；不提交Git，不迁移真实数据，不以过渡兼容状态发布。


22:58 P2/L01审查：所有扫描出口统一stop且有剩余总预算；Drop把未完成生命周期标成未知，compare_exchange阻止并发扫描，只有已确认stop可恢复空闲，迟到start可能性保留隔离。生产与故障注入使用同一实现，22项BLE测试通过。原scan日志部分移动到统一失败出口，不把候选匹配当清理成功。Manager和连接生命周期尚未纳入，P2不能结项；恢复原生能力/HTTP服务独立性仍须完整调度集成及原生平台验收。


23:09 P2增量审查：确定connect错误才重试；Elapsed类型保留到open_link，超时即使disconnect成功仍保留隔离，避免迟到connect污染新链路。全部现代构造拒绝仅清理一次；NativeLease生命周期覆盖成功返回的连接。control write原子标记在完整写返回成功后清除，取消不会执行清除。BLE26通过；现代建链app87/3忽略与Fake64通过，control write最后完整集成另记status。显式Pusher/Fake捷径仍须P3收敛，不推断原生回调实机已验，不新增固件资源成本。


23:19 P3删除审查：旧Pusher循环无生产调用及定义，CLI改为明确MAC后使用当前链路；完整MAC校验先于写凭据，显式发现/配对保留。现代与bootstrap的native connect均由open_link持有原生许可，失败与成功收尾有界。成功只读结果不因close故障重做，NativeLease仍隔离未知清理。app87/3忽略、BLE26、Fake64和最终tests编译检查通过。仍不能据此宣称全协议单栈，management2/permit/旧恢复及转换未删除。

23:33 P3 app Wi-Fi审查：未发送旧能力明确拒绝，已发送确认保留；移除旧恢复驱动时补回host校验，新增回归证明host变化不阻挡原ID查询且不再发owner/save。恢复与RF只读分开选任务，共用wifi_attempt，不引入第二业务驱动。丢ACK仍按typed dispatch保留sent/unknown；超时测试由旧ble_window_budget改为真实result_unknown并保持sent/状态/可重新进入断言。app87+新增1通过/3忽略，RF延长/SyncOpen丢ACK及HTTP关闭顺序通过。core/固件permit、格式转换及UI临时Operation仍待后续收敛，未部署。

23:41 core写许可改为bool，事务JSON移除permit；未授权新写和已发确认分别保持原行为。冻结字段测试明确断言无permit。旧丢ACK测试切management3，并保留真实BLE ACK、提交revision、Bridge/设备重启原ID查询；初次缺session被设备拒绝，修正夹具后通过。app88/core168/Fake64个不同测试通过，详细分批证据见status；快照客户端和固件旧路径仍未删。

23:50 P4审查：最终响应依据本请求完成状态，不使用Update.hasError历史值；原生RequestHandler只在解析前清空结果，不在分片重置。未持有Update的请求不清理他人的维护锁；错误均有明确HTTP状态和固定error/stage，无凭据日志。直接生产callbacks故障矩阵通过，Note4 .87构建/marker/匹配ELF栈通过。宿主未包含真实WebServer socket/parser、Flash/ArduinoOTA和断电实机，P4尚不能整体结项。资源及候选哈希见status，未部署。

23:58 P3快照审查：生产客户端不再接受或发permit；帧级断言、现行owner跨namespace编辑/保留来源、无session隐藏密码、UTF-8多页冻结及原子拒绝回归通过。无调用者resume_authorized和旧授权夹具已删；锁permit不属于旧协议。core168/app88+3忽略/Fake64及最终tests编译通过。固件、模拟后端旧栈与安全转换仍开放。

2026-10-13 00:04 P3分派审查：旧authorize/migrate及无context后备调用从firmware transport删除，未知统一拒绝；当前离线控制/owner/capabilities保留。C++零副作用与HTTP/BLE负向回归通过，.88真实ELF证明迁移handler不存在；新门禁能拒绝.87且默认历史模式继续通过，不降低其他栈要求。库内部旧分支/模拟默认旧模式及安全转换尚未完成，候选不得发布，未访问设备。

00:12 P3 库内 wifiSchemesMigrate 实现/声明及退役专用测试删除，保留当前错误帧与持久恢复检查；C++ 管理回归、.89 Note4 构建/marker/符号/63项栈检查通过。仅候选未部署，证据及资源表见 status；内部旧管理分支、模拟旧模式和安全转换仍未完成。

00:18 P3 device-sim 删除旧模式/HTTP-BLE后备与测试开关，全部65项回归在当前协议下通过。补齐旧测试的claim/session、持久trial回执和完整状态分页；未放宽生产合同。render造旧数据适配及固件库旧管理实现仍待安全转换后删除，详见status。

00:24 P3 render旧命令方法及FFI分派删除，重复ID持久恢复改固定265B合成夹具，render13项通过。存储故障测试改当前上下文并通过ASAN；app/device-sim tests编译通过。固件库旧分支及安全转换仍待完成，见status。

00:28 P3 正式窗口/围栏/sole-active测试改当前上下文，删除wifiSetupCommand内save/result临时permit后备。C++管理和Fake3项通过；.90候选、62项栈/删除门禁通过，.89新门禁负例及历史模式通过。旧管理源码/首批旧语义测试与安全转换仍开放，详见status。

00:35 P3 C++管理场景全部迁移当前上下文，permit仓库/授权/无context管理入口删除。管理回归、ASAN、render13/Fake65及.91候选62项栈门禁通过；旧setup双路径与安全转换仍开放，见status。

00:44 P3旧setup执行分支/替代存储/双生命周期删除，setup测试迁当前协议并扩至12条分页；P1补跨principal同namespace/ID重放及取消隔离。管理/setup/ASAN/render13/Fake65和.92候选62项栈门禁通过，未部署。安全转换与Bridge身份等原计划其余保持开放，见status。

00:58 P1 namespace增量：冻结字段复用既有原子状态存储，失败不写入半成品；旧已发送缺身份不自动绑定。生产HTTP/BLE和ACK已核对当前认证身份，BLE负例证明换namespace/缺失身份时无query/write/ACK。app89/core148及集成22通过；core低层接口仍可被调用者绕过，设备提交时身份变化及独立BLE ACK仍待补，不能以本轮入口回归宣称全层TransactionKey完成。

01:06 上条core低层发送/查询/ACK缺少必填身份的缺口已修复，直接core负例覆盖不调用传输与错误ACK不退役，旧恢复确认旁路已删除。联合core149/app89/集成22/Fake65通过；设备提交时变化、独立BLE ACK和转换仍不视为完成，未部署。

01:11 独立BLE ACK缺口已修复：不依赖runnable作业，身份不符/状态读取失败/ACK未知均保留待退役，下一会合只重试原query_id；成功会合共享预算内最多500ms且不授PowerPlan。生产路径四类场景与app90项通过；此宿主回归不替代真实BLE/设备掉电及派发身份变化验收。

01:18 BRS2损坏边界审查发现零长度当前键可误入旧格式路径，已由生产库红例证实并修复为storage且保留数据；全长度截断、物理注册不覆盖、既有namespace保持回归通过。.93候选RAM不变/Flash+12 B及62项栈门禁通过。factoryReset后旧会话/并发和实机Flash仍未验，不把本轮局部修复视为完整P1闭环。

01:25 显式格式离线工具复用生产编解码，避开启动代际推进/回执终态变更；合成字段保留、完整损坏/截断与无覆盖文件测试通过，render13项通过。这里只证明WFS显式blob转换，不证明raw ABI布局或Bridge/设备联合未知事务安全切换；在线旧格式暂未删除，真实数据未触碰。

01:32 旧ABI布局已通过.70/.93 ELF DWARF全字段对照；离线解析使用固定小端偏移，不将宿主sizeof当设备布局。v1/v2/WFZ2校验与保留回归通过，未知回执无高水位时不猜测。仍不能由单blob转换证明Bridge/设备联合迁移安全，BRS和可执行阻断尚待实现/验收。

01:39 修复serde静默丢弃未知事务字段的加载边界，三层拒绝/不改原文件/旧未知作业阻断测试通过；core150/app90/Fake65通过。本机一次只读清单待确认/ACK为0，仅证明该文件快照，不代表设备NVS、其他实例或整体迁移已安全。未知字段使启动失败是保留数据的明确拒绝，不能降级空库。

01:51 P3 BLE同步/异步channel中转已删除，共用异步业务核心；同步HTTP仅Ready适配，无第二份状态机。取消审查发现直接drop会遗留执行中状态，已补持久取消守卫并覆盖同进程重调度/重启原ID确认/未发读取恢复。core151、定向取消及app90/3忽略通过；原生取消隔离与全业务统一执行器仍需后续验收。

01:58 P0/P3 Wi-Fi旧持久格式在线读取/自动迁移删除，旧格式和已有空键明拒且保留，离线转换独立。WFS5合成转换恢复与回执/cursor保留、管理/ASAN/render/Fake通过；.94候选旧reader符号删除及62项栈门禁通过。不是BRS联合转换或真实发布完成证明；当前不自动删除旧键。

收尾审查：BRS旧token可认证且补猜namespace已由红例证实并删除；完整BRS2身份和拒绝保留回归通过。另发现endpoint注册只发bool且Bridge未等持久ACK，用户要求快速收尾后未继续改造，必须在恢复任务后处理。阶段提交不等于计划完成。

## 2026-10-13 endpoint 登记增量审查

- GATT写完只释放原生在途标记，业务成功必须由完整关联且stage=verified/persisted的ACK证明。查询与写共享截止、有不同ID；缺旧固件能力在任何endpoint发送前明拒。
- storage/401/读失败不会走registration_mismatch分支，不能把“查询失败”解释为“未登记”。原生未知写与业务ACK未知分开，避免Fake恢复成功而原生适配器却永久隔离的差异。
- BRS核对只读，身份与凭据同时匹配；不改变namespace、不猜迁移身份、不清旧键。生产存储成本和已有原子写/读回保持。完整设备handler未做嵌入式构建/栈实测，源码候选与.95 ROM明确分开。
- Fake共享wire但使用合成登记文件，证明Bridge及协议错误链/重启核对，不证明真实NVS掉电或原生回调。UI的64个WK场景为合成IPC；真实通信证据来自Rust/Fake测试，不能相互替代。
- 本轮不结P1/P2/P3整体；排队预算、统一全业务执行器与联合离线迁移仍按原计划推进。

## 2026-10-13 配置/事务独立存储审查

WFS4已混存active选择、回执和高水位，不可笼统称纯配置；用户选择物理分开。实现保留WFS4配置编码、独立WTX1事务与WCM1根，读旧记录不改字节，首次提交前先建立旧/空根；提交根缺失且存在准备块时拒绝，防止回退旧回执。验证事务单变不写配置、配置字节保持、未知不猜、完整配对原子恢复与容量失败保留。App93/3忽略、render68/Fake66、生产C++/ASAN和匹配`.98` ELF62项通过。16KiB真实NVS的最大组合/GC、实体掉电/堆与BRS仍待验，不依据Fake宣称可以发布；详见拆分专项。
