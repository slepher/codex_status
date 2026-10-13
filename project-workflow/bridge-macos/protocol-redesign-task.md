# 协议重设计执行任务

2026-10-12；承接 `49cec054` 的 [P0–P6计划](protocol-redesign-plan.md)，唯一待办仍为 B6/C8。

本轮里程碑：建立 P0 基线并实现 P1/T01 的查询失败保护。先复现旧 active_protected 断言及已发送控制事务查询失败；修正夹具与共享业务结果处理；运行定向及 core 回归。P1 其余事务键、取消/过期确认、HTTP发送证据及设备持久终态，以及 P2–P6，继续按原计划推进，不把局部修复记作整体完成。

- [x] P0：核对现行入口、操作索引、本机持久数据和旧路径删除边界；记录发布前迁移准备缺口。
- [x] P0：将 active_protected 分为唯一配置拒删、另有配置可删且 live active 不变。
- [x] P1/T01：先记录失败回归，再修查询错误不改变原写提交证据；重启后同 ID 只查询。
- [x] 验证、审查，更新 status/review、原计划与交接卡。

保留进入本轮前 PROGRESS.md 的本机部署记录。测试不启停默认 Bridge，不访问设备或改运行数据，不提交或部署。

## 继续推进至完整计划完成

用户已要求推进到完成，目标保持P0–P6全部要求，不以单项绿测试结项。当前顺序：P1/T02统一事务键并覆盖跨namespace同号、持久恢复及ACK围栏；P1/T03将执行意图与确认义务分开；随后继续P1持久终态/发送证据及P2–P6。构建、部署、实机证据遵循原计划门禁。

P1/T04候选WFS5：头部加8 B已使用执行代际，每条实际回执加8 B代际；空/单条/典型2回执/上限8回执持久增量8/16/24/72 B。RAM预算为每仓库8+8×实际回执数，另8 B当前代际；双仓库+双序列化缓冲的增量上界296 B（待宿主sizeof及最终ELF核实）。代际在本次开机第一次控制事务的pending原子提交中持久发布；没有控制事务的开机不单独写Flash，也没有对外可复用的执行代际。新trial成功/失败补一次终态提交，重启遇新格式pending合并一次interrupted提交；旧generation=0 pending继续未知。active仍与成功回执同次提交，pending恢复不能先改active。先读回校验再退役旧键；P3再执行一次性转换与删除兼容读取。


22:44增量验收：T01类型化查询/原写、T02设备namespace隔离、T03取消/过期确认、T04 WFS5持久终态、HTTP进度和L06已有设备锁入口worker持有许可均已实现。core168/app87+3忽略/Fake64、C++管理/ASAN存储、Note4候选及64栈项通过；资源实测及哈希见status。旧RAM增量296 B为设计预算，实际宿主表及嵌入式栈分别记录，动态实机峰值仍未知。

当前实施顺序不变，但P1全层namespace冻结需要与P3现行身份入口一起收敛，不能在旧management2旁再加一套长期兼容。HTTP部分写注入已通过，下一批推进P2完整原生BLE生命周期，再合并P3全层身份/统一执行器删除清单；独立P4可并行阶段实施（不表示启用子agent）。P5/P6仍须完成。

P2/L01扫描增量已验（BLE22项）：扫描start/events/lookup/properties/stop共用绝对截止并预留清理预算；取消/未知start或未确认stop隔离原生适配器，不再创建扫描worker。故障Adapter通过与生产相同的scan_with_cleanup路径，验证清理后下一扫描及隔离后零原生调用。连接/配对/INFO/链路代际仍继续同阶段，不因L01局部通过标记P2完成。

23:09 P2现代构造增量：Manager有界、open_link集中connect/setup/close与NativeLease持有；自动首次配对退出，显式配对保留；endpoint/auth取消同CMD保留未知写标记。BLE26通过；旧Pusher/Fake旁路、实际原生迟到/恢复及调度层仍开放，下一步按P2/P3依赖收敛入口，之后继续P4–P6。

P3旧Pusher后备删除：自动UDP分支不再对任意前缀设备写endpoint；仅已登记MAC进入现行会合。CLI改为强制mac/token并使用DeviceConnection，删除BleConfig/cycle_once/run/通知worker和selected注册布尔旁路。显式发现/配对/取token保留，全部复用open_link及NativeLease，app87/3忽略、BLE26、Fake64及最终tests编译通过；CLI帮助不触发无线，操作文档同步。

23:33 P3 app旧Wi-Fi入口删除增量完成并验证：management2/Permit缓存/旧轮询/自定义恢复驱动移除，恢复保存与RF读取复用持久服务。app88个不同测试通过、3条件忽略；详见status。下一步继续core/device_client与固件permit/旧协议删除、TransactionKey与安全转换；P3未结项。

23:41 P3 core wifi_attempt去除字符串permit与wire字段，全部调用改显式write_allowed；当前协议跨载体丢ACK/双重启确认及无permit断言通过。继续快照参数/固件旧栈、安全转换与TransactionKey；独立P4真实handler问题已定位，未实现。

23:50 P4独立实现增量：OTA请求级结果、错误分类、busy所有权与有界清理完成；实际生产callbacks故障注入通过。Note4 .87候选与匹配ELF验证完成，未烧录。P4真实socket/Flash/ArduinoOTA设备回归与P6保持开放；继续原完整P0–P6范围。

23:58 P3快照客户端去permit、旧授权残留删除及现行完整投影测试完成；core168/app88/Fake64通过，帧级与最终编译补验通过。下一批优先固件/模拟后端删除清单、TransactionKey冻结及一次性格式转换，不将生产客户端清理缩为整体目标。

2026-10-13 00:04 P3固件旧在线入口删除完成并验证：C++管理10组、两载体新增拒绝测试、.88构建/marker/哈希、62栈项+1删除门禁通过；旧ELF验证器红绿对照通过。剩余内部旧栈、Fake默认模式、完整TransactionKey与安全转换继续，不改变P0–P6完整目标。

00:12 P3 库内 wifiSchemesMigrate 实现/声明及退役专用测试删除，保留当前错误帧与持久恢复检查；C++ 管理回归、.89 Note4 构建/marker/符号/63项栈检查通过。仅候选未部署，证据及资源表见 status；内部旧管理分支、模拟旧模式和安全转换仍未完成。

00:18 P3 device-sim 删除旧模式/HTTP-BLE后备与测试开关，全部65项回归在当前协议下通过。补齐旧测试的claim/session、持久trial回执和完整状态分页；未放宽生产合同。render造旧数据适配及固件库旧管理实现仍待安全转换后删除，详见status。

00:24 P3 render旧命令方法及FFI分派删除，重复ID持久恢复改固定265B合成夹具，render13项通过。存储故障测试改当前上下文并通过ASAN；app/device-sim tests编译通过。固件库旧分支及安全转换仍待完成，见status。

00:28 P3 正式窗口/围栏/sole-active测试改当前上下文，删除wifiSetupCommand内save/result临时permit后备。C++管理和Fake3项通过；.90候选、62项栈/删除门禁通过，.89新门禁负例及历史模式通过。旧管理源码/首批旧语义测试与安全转换仍开放，详见status。

00:35 P3 C++管理场景全部迁移当前上下文，permit仓库/授权/无context管理入口删除。管理回归、ASAN、render13/Fake65及.91候选62项栈门禁通过；旧setup双路径与安全转换仍开放，见status。

00:44 P3旧setup执行分支/替代存储/双生命周期删除，setup测试迁当前协议并扩至12条分页；P1补跨principal同namespace/ID重放及取消隔离。管理/setup/ASAN/render13/Fake65和.92候选62项栈门禁通过，未部署。安全转换与Bridge身份等原计划其余保持开放，见status。

00:48 P1 修复控制作业提前去重遗漏 principal，补重启后 trial/activate 同身份复用与跨身份拒绝，默认功能 core147项通过。下一步仍是 namespace 冻结及发送/查询/ACK 全链身份围栏；本增量不关闭 P1。

00:58 P1 作业namespace原子持久化、HTTP/BLE生产机会与三个ACK路径身份核对完成，app89/3忽略、core默认功能148及集成22通过。已发送旧作业缺身份保留未知且禁止猜测；生产BLE换namespace负例不发query/write/ACK。继续把身份约束收束进core强制接口、设备派发时围栏及安全转换；P1未完成。

01:06 P1 core发送/查询及ACK接口身份参数改为必填，回执确认/拒绝收私有，旧恢复直接确认旁路删除；直接core身份负例、重启原ID恢复与错误ACK保留通过。core149/app89/3忽略/集成22/Fake65通过。独立BLE ACK、设备派发围栏及旧数据转换仍开放。

01:11 P1 独立BLE待ACK会合步骤完成，复用共享剩余预算、上限500ms。生产ble_cycle无活跃作业/Bridge重启/丢ACK/status超时后重试/namespace不符通过；app90/3忽略及最终定向通过。继续设备提交时身份变化与安全转换，不关闭P1/P0–P6。

01:18 P0/P1 修复BRS2零长度当前键误入旧格式、物理注册可重建namespace的问题；生产库零长度红绿对照、全截断/身份/持久回归通过。.93 Note4候选及62项栈门禁通过，未部署。剩余设备派发边界、安全转换与P0–P6继续。

01:25 P0/P3 新增独立WFS3/4/5→WFS5离线工具，复用固件编解码且不跑启动恢复；保留unknown/指纹/三元身份/高水位/代际/配置，拒绝覆盖。工具2项、render13项、CLI帮助通过。raw ABI旧格式、BRS/Bridge联合清单与最终可执行围栏仍开放，未转换真实数据。

01:32 P0/P3 从.70/.93 Note4 ELF核对全部旧ABI字段偏移后，离线工具支持v1/raw-v2/WFZ2，按字节偏移解析而非宿主struct；内外CRC、解压上限、字段/未知保留及缺cursor拒绝通过。工具3项/render13项通过；BRS/Bridge联合迁移与最终围栏仍开放，未转换真实数据。

01:39 P0 Bridge根/Wi-Fi/Job持久对象拒绝未知字段，原文件不改写；旧缺namespace未知作业仍保留并阻止新写。core150/app90/3忽略/Fake65通过。本机默认文件只读统计15作业、待确认/待ACK均0，证据带文件hash与数据根未独立核实边界；未访问设备或改写真实数据。继续BRS与跨实例/设备联合迁移。

01:50 P3 将Wi-Fi业务状态机改为可等待载体，HTTP同步适配复用同一实现，BLE直接await并删除双向channel/阻塞worker。保留每设备许可与原预算，取消时持久收尾，已发仅查原ID、未发读取可重调度。core151及取消定向通过；app90/3条件忽略、Fake65最终回归通过，统一全部业务操作与P0–P6其余仍开放。

01:55 P0/P3 当前增量：移除固件在线WFS3/4/v1/v2/WFZ2读取与自动迁移，仅保留WFS5；离线转换独立保留旧格式解析。先验旧格式/零长度键保留且阻断写入，再跑存储/管理/转换/Fake与候选栈门禁。BRS联合迁移和真实切换未完成，不发布。

01:58 上述Wi-Fi当前格式收束完成，转换4项/render13及集成/管理/ASAN/Fake65/app90（3条件忽略）通过；.94候选与62项栈门禁通过，未部署。BRS与真实联合迁移、全业务执行器及原计划其余仍开放。

02:02 P0/P3 当前增量：BRS运行时只读完整BRS2身份；旧逐键仓库或缺principal/identity/namespace记录拒绝且阻断物理/非物理注册，不从当前peer补猜namespace，不自动删除旧键。验证完整旧库/孤立键/空键/缺身份/当前绑定续用与持久失败，构建Note4候选；不替代离线联合迁移。

收尾：用户要求结束当前任务并提交阶段进度。BRS当前完整身份限制及旧库阻断红绿通过；BLE endpoint持久ACK/结构化错误链发现未修，作为继续入口。P0–P6不结项，不部署。

## 2026-10-13 恢复推进（Bridge / Fake ROM）

设备不在现场，用户限定本轮聚焦Bridge与Fake ROM。首先完成endpoint注册纵向路径：请求关联、读持久ACK、拒绝与未知分离、app错误传播、Fake故障矩阵。复用共享C++ wire编码并同步设备入口合同，不构建/烧录ROM、不启动生产Bridge、不接触真实数据。随后按原计划继续可在宿主验证的边界；实机项目保持未验。

- [x] endpoint请求/ACK关联及错误语义，更新共享wire与当前入口。
- [x] Bridge有界确认、未知/取消保护及app错误传播。
- [x] Fake ROM和app生产会合回归；记录证据和剩余边界。

2026-10-13用户确认配置/事务物理拆分：WFS4配置编码保留，WTX1独立回执/执行代际/高水位，WCM1根原子选择；旧WFS4直接读取并保留原事务，取消WFS5配置升级安排。生产存储/配网/管理、ASAN、App93/3忽略、render68、Fake66和`.98` ELF62项通过；资源/故障证据见wifi-config-state-separation.md。真实NVS容量/掉电及BRS身份仍未放行，无提交/部署。
