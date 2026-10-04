# Note4 数据延迟同步调查（2026-10-03）

## 范围与步骤

用户报告接 USB 时长期未同步、拔 USB 后较快同步，明确不预设 USB 为原因。随后补充 USB 已接回，并纠正现场为 **Bridge 92，设备 93**。

调查步骤：读取最新现场与运行日志 → 只读 HTTP 获取设备日志、状态、PM 与帧缓冲 → 对齐数据源/确认指纹/发送时间 → 使用已构建的生产 PlatformService 隔离回放与对照。未打开串口、改业务代码、重建/重启运行应用、刷机、写真实设备配置或主动推送。

## 结论：当前差异的直接根因已复现

Note4 运行 .44、已装模板 codex-status-a，本机没有该设备的本地 Profile；运行字段按设备报告合同识别。设备的 weekly.remaining 是可见用量，源端对应 weekly.usedPercent，应参与 push。

但 service.rs 的 triggers_for 在无 Profile binding 时，仅通过 source_has_field 找候选源；后者只检查 config.triggers 显式键。运行 Codex 源 config 只有 valid_for_s=3600，没有 triggers 覆盖，所以候选源未找到，weekly.remaining 没有生成合同内 trigger 映射。Coordinator::trigger_of 回退时直接分类原字段名；classify_trigger 只认 usedPercent 等 push 后缀，remaining 因而被错分为 pull。

结果：数据源已用 8%（剩余92），设备仍持有已确认的剩余93；Bridge 将用量变化判断为 pull-only，push_dirty=false、in_flight=null，不排新包，只等待默认3600秒 full sync。此路径违背可见用量变化主动发送完整快照的既有约定，不能用“正常每小时同步”结案。

代码位置（本次读取）：
- bridge/crates/core/src/platform/service.rs:2640 source_has_field；:2700 triggers_for。
- bridge/crates/core/src/coordinator.rs:310 trigger_of；:433 want_data 判据；:207 默认3600秒。
- bridge/crates/core/src/datasource.rs:196 classify_trigger；:363 remaining→usedPercent 解析已有，但分类回退未用它。

## 实机证据（北京时间）

- 19:55:39 seq10 首次 HTTP data 请求，约6秒后桥报超时；19:56:58 同 request/seq 重试 ACK applied/unchanged。该 ACK 表示幂等重试已应用，不足以单独判断首发的屏幕结果。
- 20:57:43 seq11 HTTP ACK applied/displayed，距前次 ACK 60分45秒；下一次 full_sync_deadline=1791035863，即21:57:43。
- 21:02:23 状态：MAC7C4FADB93408、FW0.18.44-note4-b-ble1、plugged=true、uptime4071s、wake_seq1、data_seq=applied_seq=11、displayed、Wi-Fi在线、owner56a6。不存在据此可证明的 USB 拔插重启或新自然 timer 会合。
- 21:04:53 同一数据源最新 weekly.usedPercent=8，source_quality=good；协调器 push_dirty=false、pull_only_change=true、in_flight=null、full_sync_due=false。
- 21:07:59 读取 /frame?which=last 与 /frame?which=frame，15011字节PBM完全相同，SHA256 4f4a448f16e5b85da325496a9934c53c58cfaa356f21685844ee66630ffecb51；两份软件缓冲均明确绘制93。实际屏幕93由用户反馈；软件缓冲不是面板物理读回。
- 现场确认 full_fp1472084593755028491 可由 usedPercent7/remaining93 的7个远程字段完整重算；当前 full_fp17468198322944852218 可由 usedPercent8/remaining92 同字段重算。
- 当前和确认 push_fp 都是5655168649363534298；仅将 account.plan / bridge.label / resetCredits.availableCount 纳入计算可逐位复现此数，说明可见配额字段没有参与 push 指纹。

设备端 /log：
- 启动阶段 [ble-wire] chr=info bytes=0 max=512 result=precheck_failed。
- 四条 [epd] partial failed; escalating to full refresh。
- 当前 epd_busy_fails=4、epd_writes=14、baseline trusted、displayed；无 full refresh busy timeout 文本。本计数也包含局刷准备/分配失败，不等于四次已证明的硬件BUSY超时。
- /log 仅存 syncDiagnosticText 白名单文本（fail/error/sync等），不保留正常 data/USB/会合行，故缺日志不能推断没收到命令。最新现存归档99只含启动记录1–3；当前RTC diag_next_seq=14，尚不能从归档定位四次局刷失败的时间。

PM：USB NO_LIGHT_SLEEP 锁活动、累计约99%，仅2次light sleep；代码在USB连接时保持唤醒/BLE。说明USB会改变驻留行为，但隔离生产回放不含USB即可重现本次数据差异，故它不是该分类缺陷的必要条件。BLE connect/read/ACK 与HTTP间歇超时仍独立存在，未在本次确立底层原因。

首次自行查询 /api/status 使用设备操作token得到401，随后改为正确的Bridge业务端点token返回200。该401来自调查请求的token类型选择，不作为运行Bridge鉴权故障证据；无token值写入产物。

## 隔离回放与对照

脚本 artifacts/bridge-macos/usb-sync-repro.rs，日志 usb-sync-repro.log；链接已有 production bridge-core rlib，以新的 ignored 假设备目录运行，无真实网络、BLE、USB、设备操作。

1. 无本地Profile/default Codex源/设备合同 weekly.remaining：先确认93，再源更新92 → push_dirty=false / pull_only_change=true / next=idle；越过full_sync_deadline → light_data且v=92。**复现成功**。
2. 同一条件，仅源增加 usedPercent=push 显式覆盖 → push_dirty=true / light_data。**对照通过**。
3. 同一条件，仅设备合同直接绑定 canonical weekly.usedPercent → push_dirty=true / light_data。**对照通过**。

初次rustc选择了不同编译配置的serde_json产物而出现类型不兼容；改用bridge-core匹配的现有rlib后编译/运行成功，未安装依赖或修改Cargo配置。当前候选源码也含该缺陷；本回放不是部署验证。

## 修复范围（未实施）

修复共享合同trigger解析/默认分类回退，使remaining按已存在的canonical usedPercent分类，同时保留显式来源覆盖和pull-only元数据语义。加入真实PlatformService的“无本地Profile、设备报告合同、初次ACK后93→92自动排队”回归，覆盖重启与HTTP/BLE入口。无需更改USB策略或先改固件。运行Bridge是否升级及实机92收敛仍待后续修复任务，唯一待办见backlog A8。

## 证据文件

- artifacts/bridge-macos/usb-sync-20261003-210223-log.txt / -status.json.txt
- artifacts/bridge-macos/usb-sync-20261003-210453--api-status.txt / --pmstats-diag=1.txt
- artifacts/bridge-macos/usb-sync-20261003-210453-platform_power_view.json / -platform_data_sources.json
- artifacts/bridge-macos/usb-sync-20261003-210759-last.png / -frame.png（及同名PBM）
- artifacts/bridge-macos/usb-sync-repro.rs / usb-sync-repro.log
- AppSupport运行bridge-app.log.2026-10-03 与platform/diagnostics/7C4FADB93408/99-d4d6b62789fa21a262828532e65331d9.json（原件未改）

## 追加调查：USB/电池通道与Fake ROM边界（2026-10-03，仍只调查）

用户询问是否“Bridge没有模板”导致、为何电池正常，以及另一进行中任务存在时只调查。本节未修改生产源码/运行Bridge/设备，不交办其他任务。

### 本地模板与Profile的区别

设备已有codex-status-a，设备合同有效；Bridge无需保存模板副本即可供数。问题前提是缺少本地Profile的显式source binding，默认源无triggers覆盖，触发canonical分类回退缺陷。缺少模板副本本身不是禁用同步条件，不能以复制模板/重建Profile作为产品上正确结论。

### 两通道生产协调器回放

新隔离证据 artifacts/bridge-macos/usb-sync-transports-repro.rs / .log：
- HTTP与BLE各自的无Profile/default remaining场景，初始93 ACK后源变92，均push_dirty=false/pull_only_change=true/idle，3600s到期才发送。
- HTTP与BLE各自的显式usedPercent=push覆盖、canonical usedPercent两组对照，均push_dirty=true且分别light_data/ble_data。
- 因此该Bridge缺陷与传输入口无关，不能单凭它解释“电池正常/USB异常”的差别。上一结论只确立当前Bridge92/设备93的原因，未证明用户最初USB事件全部由此造成。

### 实固件通道与驻留差别（代码事实，原事件未证实）

USB不是业务数据线；同步仍走Wi-Fi HTTP/BLE，串口只提供CLI。USB plugged基于usb_serial_jtag_is_connected（USB主机SOF，不仅是供电）；拔线需连续10s确认，pollPlug随后renderCurrent，重画lastUsage缓存。USB禁止正常进入deep、保持BLE；电池在正式Plan/其他条件允许时deep，再通过timer wake进入有界BLE会合，正式light窗口才提供Wi-Fi。电池也可以在light期间走HTTP，USB也可以走BLE，不能画成严格一对一协议切换。

额外发现固件绘制调度边界：
- applyV2Data HTTP：立即renderActiveUsage并返回displayed/failed/unchanged。
- applyV2Data BLE：不论是否正处于v2InRendezvous，都置v2WakeRenderPending=true / v2DisplayState=pending，不立即画。
- v2WakeRenderPending唯一消费点为v2RendezvousRender，而该函数仅由deep timer的v2Rendezvous退出时调用。普通USB常驻loop没有对应pending drain。
- USB常驻BLE若收到新data，可能出现“已接收但未立即画”；其他renderCurrent触发可以顺带更新，拔线pollPlug触发renderCurrent尤其符合“拔线后很快更新”预测。此为代码可见的调度缺口，尚未按该用户事件复现。
- 日志中13:26与13:30曾有BLE applied/pending，但wake_seq53/57属于较早timer会合，不是刚才拔线现场；19:54之后唯一两次数据确认seq10/11均HTTP，20:57:43那次是HTTP displayed。没有把这些无关ACK拼接成USB因果证据。
- 因而当前92/93误分pull已确立；USB特定BLE绘制、持续BLE/Wi-Fi共存与重连超时的作用仍待事件关联，不能认定因果。

### Fake ROM覆盖情况（当前工作树，不代表候选已部署）

已体现：
- SimPower含plugged/deep_on_usb；reconcile_power经bridge-render调用共享v2PowerSleepDecision。
- bootstrap::configured_power_sleeps_and_timer_and_button_wakes_have_distinct_windows，在bootstrap.rs:1466起明确测试USB=true时sleep Plan仍light、USB=false后deep、HTTP关闭、timer与button不同窗口。
- 有HTTP关闭/短BLE窗口/Plan打开HTTP、原始BLE属性/ACK与同源v2DecideData等能力。

缺口：
- /sim/power直接赋plugged布尔并立即reconcile，不执行USB SOF检测、10s拔线去抖、pollPlug的renderCurrent；不模拟USB主机驱动与真实PM lock/RF。
- Fake BLE data在device-sim/main.rs的sim_ble_command分支直接调用device_command_impl(data,false)，最终bundle.data进入render/ffi.cpp:codex_sim_data，firstApplied即simRender。没有实固件的BLE pending→radio关闭→render的生命周期；绘制环境仍固定PULL/light/WIFI ON。
- Fake能够表现电源/可达性差别，却会在接收BLE后立即给出画面结果，从而漏过上述USB常驻延迟绘制缺口。
- run-sync-headless只提供periodic/light/restart/device-restart四种scenario；runner动作仅light，未设计USB常驻→拔插→变化的端到端比较。既有无Profile字段合同单测只测account.plan初始投递，没有remaining首次ACK后的变化分类。
- 本轮为调查增加的生产协调器回放证明分类问题，并非Fake ROM已补齐USB矩阵；未改这些业务测试或Fake实现。

待办继续以backlog为唯一来源；本报告只记录缺口与证据，不操作另一进行中任务。

## A8 修复执行（2026-10-03，用户已授权）

1. 在生产 PlatformService 增加无模板/Profile、首次 ACK 后 remaining 93→92 的失败回归，覆盖 HTTP/BLE、重启、显式 push/pull 覆盖及 pull-only 不推送/不改 Plan。
2. 在共享 classify_trigger 入口复用 resolve_field 归一化，避免所有默认分类调用者遗漏 remaining；保留服务层来源覆盖与设备合同。
3. 运行 focused 红/绿、core 与 Bridge 相关回归，保留其他任务工作树修改。
4. 用户已选择一并打包部署当前 Bridge（含另一任务 Wi-Fi 候选），保留 .44 设备 ROM。备份 AppSupport 数据和当前 .app，核对路径/服务后替换；不改真实网络、Profile/模板或固件。
5. 检查新版应用/健康端点、源值/确认指纹、数据 ACK 与只读帧缓冲；记录真实验证边界、产物 SHA256，更新 PROGRESS/backlog。

用户进一步授权本轮 Bridge 和 Note4 ROM 一起更新；W7 从本轮按授权接续，候选版本 0.18.45-note4-b-wifi3，保留 .44 回退和全部运行数据。

## A8 修复与 W7 实际交付（2026-10-03）

用户已授权推进，并选择Bridge+Note4 ROM一起更新。最终生产修复为共享classify_trigger复用resolve_field，以及设备context reconcile失效旧data确认指纹、重新排完整快照。保留显式push/pull、设备报告需求、许可、序号递增和旧ACK隔离；未创建本地Profile。

新增service14组合覆盖ACK后93→92、HTTP/BLE、Bridge重启、显式覆盖、pull-only不推/不改Plan、新context与旧ACK。两种缺陷均先失败后通过。五crate唯一242 passed/1 ignored，Fake48各有通过结果，合计290/1 ignored；Fake首轮BLE用例126s后窗口结束失败，NO_PROXY/no_proxy=loopback复测18.39s通过；最后service变更相关Fake Wi-Fi/丢ACK/重启复测0.64s通过。UI/最终diff通过。

Note4 .45标准OTA job63be0d96=succeeded/image_verified；ota_1前1,720,784 B SHA256=B4635DC064378427FACBB62C6A7780DE9649E819CD0DB834B0063B90F149468C，与冻结ROM一致。wifi_management=2。最终Mac Bridge75,188,560 B / SHA256=FA15F8B52D383BEFB039B800900D3D91342409AE5A064B0944F9082C92627817，经原LaunchAgent运行。旧应用/.44 ROM/ota_0和AppSupport私有data备份保留；安装身份/endpoint配置哈希未改。

22:36:11自动HTTP seq16 ACK applied/displayed；源usedPercent11=remaining89，认证data_seq=applied_seq16，push_fp=acked_push_fp18396089969837334073，最后写屏缓冲89，无in_flight。没有push_now、源伪造、模板发布、实际Wi-Fi写入、串口或提交。旧92/93差异在部署前小时同步已更新90，本轮采用最新90→89；首次重发不等于持续同context每次变化实机延迟测试。

最后公共状态plugged=false，reset_reason8经实际IDF头确认是deep-sleep wake；不能将本次HTTP成功当USB常驻BLE pending消费缺口已修。之后HTTP Host is down只记不可达。USB/Fake绘制差异、重连/局刷及真实Wi-Fi/RF/TCC/功耗仍归backlog B5/B6/A3开放项；A8闭环仅指已复现的Bridge判定缺陷。

最终证据为artifacts/bridge-macos/a8-{rom-metadata,app-metadata,deployment,running-image,firmware_ota_status}.json、a8-verified-{status.json,power,sources}.json、a8-verified-last.pbm/png及a8-*.log。自行镜像采样的token类型401已纠正；运行OTA使用设备操作token，普通业务使用endpoint token，证据不含凭据。
