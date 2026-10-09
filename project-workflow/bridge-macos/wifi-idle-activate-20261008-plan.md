# 周期 activate 调度修复

2026-10-09 14:10数据/Light追查：用户指出数值变化应按设置保持至少5分钟Light。实际能力max_light_s=600，coordinator MAX_LIGHT_S=600，普通数据工作分配10分钟；本段日志是14:05:14源usage changed rev2，14:06:05联合Data369+Plan2963 ACK applied/600 s。随后14:07:51 Plan2964及14:09:23 Plan2965 ACK120 s，14:10:58 Plan2966 ACK0；故原约14:16的期限被Bridge正常发出的新ID缩短，不能将Light已结束简单解释为正常10分钟到期。14:10新源变化未在该段日志确认，不否认用户屏幕观察时点。

根因代码：coordinator note_plan_ack仅pending_explicit_light命中时调用hold_light；普通数据Plan只记录last_accepted_at/remaining。Data note_ack成功清in_flight/push_dirty。service plan_for_rendezvous_network_work再据工作判Light：没有数据工作、没有hold时短Wi-Fi任务获120 s，任务结束直接Sleep。V2PlanState按新高ID接受并替换旧期限，不保护Bridge的业务驻留意图，因此属Bridge期限管理实现遗漏，设备执行新正式计划符合目前协议；不靠设备拒绝全部Sleep修补。后续同一A9计划需先复现自动Data→ACK→短读→闲置联系，保护已确认数据Light的绝对期限、避免重试续租、保留显式提前Sleep、恢复/重启与未确认结果。仅诊断/文档，未修代码、改网络或部署。

2026-10-09 14:06–14:12 追加图标诊断（只读现场/文档）：用户报告数据正常但Wi-Fi off及Bridge disconnected，提供14:11照片（weekly80/86%/OFF 0M，无Zzz）。照片只证明没有Zzz，不能推断light sleep。登记MAC7C4FADB93408/.177，当前无USB串口枚举；不主动复位/开网/修改配置。生产日志14:06:05联合Plan/Data seq369经BLE ACK applied；14:10:58 Sleep Plan2966、14:12:04 Sleep Plan2967经BLE ACK applied/Light剩余0，只证明计划接纳，不证明实际深睡。MCP只读缓存的旧radio/显示样本stale_boot，不作当前事实；48 s直连/status.json未成功，未把缺样本称成凭据错误或Wi-Fi硬件故障。

确定代码根因：radio.cpp的deviceStateText在wifiUp=false返回WIFI OFF；platform.cpp对BLE Data记录lastChannel=BLE、markSynced并安排会合结束绘图；display.cpp:875却仍直接将wifiUp=false映射offlineMins=0，而非依据最近Bridge联系。真实codex-status-a模板在device.offline_mins exists=true绘制Bridge断开位及OFF行；引擎offlineMins>=0就判字段存在。故新BLE数据正常与OFF 0M并不矛盾，是Wi-Fi关闭被错误映射成Bridge失联，.72网络重绘/Bridge应用模型修复没有覆盖这段固件逻辑。Wi-Fi off的显示条件独立，实际关网可能是电源计划行为，但照片时点的驱动事实未采到，不宣称已证明原因；EPD旧样本busy_fails28也不能证明当前图标是残影。

后续修正归backlog A9/B6：Bridge标记根据跨HTTP/BLE有效联系期限独立计算，保留Wi-Fi真实关闭、深睡/连接中语义，并测试最近BLE联系+Wi-Fi关闭时Wi-Fi off但Bridge不失联、联系过期及恢复重画，不为图标强制常驻联网。证据artifacts/note4-icons-diagnosis-20261009.json（模板条件及实际日志）、note4-icons-cache-20261009.json（观察时间/过期原因）与用户照片；本轮未修改实现、刷ROM、启停Bridge或提交，保留用户正在整理的交接/操作文档变更。

2026-10-09 修复部署完成：Note4 .72 ROM095BAD00…45865 / 1,901,760 B与认证运行镜像一致，默认Bridge B770BA43…CA4B0由计划任务运行；完整Status/整体请求期限、独立Wi-Fi与Bridge错误、无Data网络变化重绘及失败有界重试已实施。同源大Status端到端与超预算拒绝通过，Fake58/app75/core132/HTTP7及3项显式生产联测通过，匹配ELF栈门禁通过（Status976 B/分页helper128 B，未扩大限制）。原b0事务正常completed，正式Bridge刷新成功、运行镜像记录已按完整发布长度更新。data/exe已备份，书桌屏暂停与登记保留，无提交。设备驱动确认可信帧基线，仍有局刷回退；目视/长期RF功耗/动态峰值及macOS原生UI矩阵继续在backlog，不因本次上线全部结项。现场、资源和证据见PROGRESS顶节。

2026-10-09 用户授权修复并部署：完整Status使用单一认证快照的有界分页，保持4096B帧上限，不裁剪字段合同；同源Fake覆盖超帧、错读ID/过期/跨principal与恢复。Bridge请求期限包含锁/开会话/全部分页，错误载荷明确失败；Wi-Fi事实与Bridge验证独立，网络状态变化单独绘图且失败有界重试。先回归、匹配Note4 ELF栈/资源门禁，再备份当前data/exe/ROM并仅部署Note4与默认Bridge，查询既发原事务、不自动换ID重发；核对运行镜像与真实Status、独立Wi-Fi图标链路。书桌屏仍暂停，不提交。

2026-10-09 追加现场调查：用户报告断开图标。保存只读Bridge/事务现场，不把图标当认证连接失败证明；00:05 ready报告后HTTP200却identity_mismatch，原b0事务未确认；400ms HTTP探测实际4625ms，源码发现会话Mutex等待不计入请求期限。先补有界锁等待及失败Status识别回归，再按新协议既有授权更新Bridge、查询原事务并采集真实错误载荷；不重发未知activate、不换配置、不清NVS。00:33:57真实认证帧已证实reply_limit（HTTP200/信封result3）；00:35精简Status成功且HTTP ready。USB原生监听前后boot不变，当前IP/RSSI正常，同时存在EPD局刷失败/全刷回退。core131及HTTP6回归通过；Bridge锁/错误处理修正尚未部署，固件完整Status超帧尚待修正，结论及证据见PROGRESS最新节。

用户报告仍每约3分钟activate，随后明确要求先验证新协议，若仍触发就在新框架修正，并直接部署。运行Bridge38B7CE11/.69旧协议ROM；主工作树含c97fd87精简协议及固件模块化。旧协议隔离worktree仅留作对照，不部署。

步骤：新协议下验证无HTTP工作、BLE小数据、真实HTTP工作收尾及后续联系；闲置连接状态不能独立授予Light，显式请求/真实HTTP工作/既发事务保留网络权限。补齐同一Fake网络报告闭环：真实非loopback私有接口、生产Bridge接收器及认证回读，不放宽源IP校验；Fake使用同源C++ Wi-Fi/Rendezvous和共享信封，Rust网络适配不冒充Arduino HTTPClient/RF。通过回归与匹配ELF栈门禁后备份data/exe/ROM，协调更新默认Bridge和Note4新协议ROM，核对MAC/运行hash，并观察至少两个原3分钟周期。无提交、不直接改生产state。

状态：复现、修正、同源验证、协调部署与运行hash验收完成（2026-10-08 22:23）；用户随后授权提交本次部署源码及必要的模块拆分依赖。

新协议负对照仍申请Light，确认为陈旧连接状态的调度实现bug。正向四轮同源Fake闭环无HTTP探测/新activate/RF增加；独立report→生产接收器→认证回读通过。另修正owner_id事实读取、独立SyncOpen与Sleep确认及OTA image参数从URL移到业务载荷，未放宽权限或事务合同。

Note4 .71 ROM DE7897DD…E6D31/1,898,240 B与认证运行镜像完全一致，OTA e70d4281单次上传/succeeded/image_verified；默认Bridge最终62E6C492…59C76C4，PID41500，计划任务启动，data/exe/配置已私有备份保留。只更新Note4；书桌屏ROM不动，paused_device_macs排除其自动HTTP/BLE，登记/Profile/凭据保留，恢复时移除此MAC并重启Bridge。

22:13确认收尾至22:23约596s，新增activate=0、Wi-Fi next_job_seq=168；书桌屏暂停后背景联系=0。实机有正常数据工作续发正式PowerPlan，因此不把该样本称为多轮纯空闲BLE测试。app75/core129/HTTP6/Fake57及3项显式同源联测、其他模块回归和匹配ELF栈门禁通过；实际RF/电池/掉电/动态RAM及macOS长期矩阵继续归backlog A9/B6。证据及完整ROM/ELF/Bridge SHA见PROGRESS最新节与artifacts/wifi-new-protocol-observation-20261008.json。


2026-10-09 用户补充状态合同：Wi-Fi连接状态与Bridge业务状态独立。Wi-Fi以设备驱动关联/IP事实及实际关网为准，Bridge认证失败、reply_limit、协议/回读失败、结果未知只影响Bridge层，不得写成Wi-Fi断开或据此自动换配置/重启关联。无网络时Bridge不可用，但仅Bridge失败不能反推Wi-Fi失败；电源窗口到期导致实际关网仍单独据驱动更新。固件Wi-Fi图标env.state经templateStateText/deviceStateText取wifiUp，pollWifi由WiFi.status更新；env.offlineMins属于Bridge失联标记，不能混称Wi-Fi图标。当前Bridge wifi.connection.state将bridge_unreachable塞入连接流程状态，是需要分层修正的模型问题；不因此断言固件已被HTTP错误直接断网。

2026-10-09 修复实施计划（用户授权推送，部署后两分钟未响应则继续调查）：
- task：自动计划不得提前覆盖已 ACK 的 Light 绝对期限，重放沿用原 ID；显式 Sleep 仍可提前结束。回归数据600秒→短读120秒→闲置→期限结束，并验证序列化恢复/重复 ACK 不续期。
- task：Bridge 图标按跨 BLE/HTTP 最近有效联系计算，Wi-Fi 事实独立；共享显示状态覆盖联系恢复时无业务变化的重绘。
- task：会合从 Light 进入 Sleep 走 enterDeep 的模式持久化及全屏睡眠图标路径，已有深睡薄唤醒保留时钟窗口。
- validation：共享 C++/Fake/Rust 回归、Note4 .73 构建/marker/哈希/匹配 ELF 栈门禁；备份运行数据和 exe 后部署默认 Bridge，仅 OTA Note4，认证运行镜像核验及两分钟观察。无响应继续日志/协议诊断，不换 ID 重发未知 OTA，不修改书桌屏。
- status/review：实施、Note4/默认Bridge部署、实机运行镜像与新增Fake场景验收完成（2026-10-09）；未提交，实体剩余边界见收尾记录。

2026-10-09 .73 实施/部署/复核收尾：
- plan/tasks 已完成：自动 rendezvous/boot 不以短读或闲置 Sleep 替换尚有效的已确认 Light，复用原 ID/内容以保留截止时刻；已有显式 hold 只使用剩余时间，显式 Sleep 可提前结束。没有给读、续约或 ACK 增加新的设备 light deadline。普通窗口仍可因真实新工作延长，周期/纯读取不会凭空重新给600秒。
- 固件 Bridge freshness 使用 rtcLastSyncEpoch 的 HTTP/BLE 有效联系，wifiUp 只决定 Wi-Fi事实；NetworkDisplayState 同时观察 Bridge 标记变化，有界重画。未知时间沿用已留存标记，不根据 Wi-Fi 关闭猜失联。会合从Light进入深睡调用已有enterDeep，先持久化MODE_DEEP并绘制睡眠全帧；已经DEEP的薄会合仍走时钟窗口，PC USB与物理恢复窗口不受绕过。
- Note4 已部署0.18.73-note4/ota_1。ROM artifacts/note4-0.18.73-sleep-state.bin，1,901,840 B，SHA256 127345E2980912906F083D7EADF25FB20AC2619DA1D7B1542A8B9A1770DA9450；匹配ELF F815B66B948A5FDE4F4D66FC21AAA2083CEEC77BEAFB6389A3032D0B9031E525。默认Bridge B745E69EF9F4AF79876CA5C4D47AD40A6501773F413D8554A7764227C78A272D，计划任务PID37732/watchdog69768，真实data/exe私有备份在artifacts/bridge-before-fix73-20261009.*。书桌屏暂停/登记保留，无提交。
- OTA原事务1a4be8d0在14:31:52排队，14:32:32上传ACK，14:34:14自动succeeded/image_verified，attempt_count=1。排队后两分钟尚未确认时继续只读调查，未换ID/重传；14:34:34直接设备认证Status返回.73，14:34:50完整运行镜像哈希与上述ROM一致，wifi_connected=true/http.ready=true/EPD busy fails=0。上传ACK后约102秒确认，当前未复现回报丢失或无限待确认；未因此擅改确认协议。后续真实认证观察又见.73及新的600秒计划2982剩余512秒，旧缓存不当现场。
- validation：app75/core133/HTTP7通过；Fake新增3场景，最终61/61及共享render通过。覆盖Note4真实Data应用seq、Light600秒→120秒短读→idle复用→600秒到期Deep，并对实际Fake帧的Zzz/Wi-Fi/Bridge区域逐像素对拍；Wi-Fi关着时BLE联系正常、7分钟联系过期、同内容新Data恢复；OTA延迟120秒+Bridge重启后仍查询原job，仅版本不确认镜像，最终hash证明且只有一次上传。Fake联系环境接共享C++规则/模板引擎；仍不执行ESP-IDF自动light sleep、Arduino无线栈或实际墨水屏。
- Fake附带修正：失败业务无JSON时也生成明确拒绝及阶段；缺request_id的拒绝仍用固定帧请求号绑定回执，编码失败不沿用旧成功。重放围栏保存原始请求帧，避免变量遮蔽误保存业务响应；联系恢复测试同时检查真实applied_seq，不能以缓存成功ACK冒充Data已执行。此适配仅宿主Fake程序，不需再刷Note4。
- 资源review：静态RAM117,948 B（较.72 +8 B，NetworkDisplayState增加一个int并产生对齐成本），Flash1,850,655 B（+52 B）；无新NVS格式/空仓库/大栈对象/堆预留，原子持久化不改。匹配原ELF Wi-Fi栈门禁通过，Status976 B/dispatcher960 B，现有最重本地链预算4096 B未扩大。未测实体动态堆峰值、电池电流和掉电矩阵，屏幕物理目视也未代验。
- review：改动限定计划保护、联系语义及既有深睡入口；明确保留Wi-Fi实际关闭。BLE-only Light时Wi-Fi off仍是有效RF事实，Zzz只表示Deep而非所有Light状态。三项生产Bridge/Fake显式联测记录artifacts/note4-fix73-fake-network-final-tests.log，构建/完整回归/运行证明/两分钟观察分别在note4-fix73-*证据；A9/B6剩余实体矩阵继续开放。

2026-10-09 16:30 手动激活/Sleep 显示重新调查（未修复、未重新部署）：
- 现场：c9 于16:05接收后 report_received、认证回读成功，最后 confirmed_operation/operation_result；ca 于16:17接收 activate/network_start 后最终 query_result/durable_rejection/window_closed，选择配置ID均为 home。不能把 acknowledged 当作最终成功。16:18:40认证缓存为 .73、boot_id 81d36f8e98e2806568585bc8f5642680、uptime635601ms、reset_reason8、Light2991/600s、wifi_connected=true/RSSI-66、HTTP.ready=false/IP缺失、EPD busy fails21/writes4。最新BLE会合仍能确认身份和计划，HTTP回读超时；不因此推断密码错误或panic。
- 直接诊断：16:19:43及16:24:30向登记IP读取/status.json、/log、/pmstats、认证/api/status均超时。Get-CimInstance Win32_SerialPort无COM；Win32_PnPEntity无VID_303A，因此本轮没有打开串口、没有主动复位，未获取真实设备DevLog。缓存时间明确保留，不能称为当前实时Wi-Fi状态。
- 确定代码遗漏1：display::v2RendezvousRender(false)在无v2WakeRenderPending时只调用v2RendezvousClockRender；已有clock baseline成功时只改时钟区域，不比较/刷新Bridge freshness。radio::markSynced更新联系时间并不意味着旧Bridge失联像素清除；normal-loop serviceNetworkGlyph因会合后立即深睡不执行。Fake目前按display_state变化直接绘制全帧，不能覆盖这个实际固件调度遗漏。
- 确定代码遗漏2：power::enterDeep在renderCurrent后无条件置rtcDeepGlyph bit4，renderActiveUsage在epdFlush前已经更新bit8；这些RTC位不能证明物理屏已画成功。现场21次busy失败使旧像素保留成为实际风险，但没有DevLog不能确定失败的GPIO/供电/控制器原因。
- 激活代码路径：application主循环在v2InRendezvous && bleOn && v2RvNetworkExpected时跳过整个wifiSetupPoll；radio会合只在非manual BLE hold时关闭BLE，rendezvousWifiTick又在wifiSetupBusy时提前返回。因而物理恢复hold期间，即使WiFi驱动已关联/IP已有，事务也可能无法执行wifiFinishActivation而一直running。wifiControlUntil取Light presence与会合deadline最小值；wifiSetupConstrain在该期限耗尽时先wifiSetupAbort(window_closed)，主动WiFi.disconnect(false)。这是可解释ca回执的实现路径；现场manual hold是否有效尚未取得设备事实，已询问操作入口，不将路径分析冒充完整实机复现。
- 下一步任务：核对物理恢复入口/hold状态；补实际会合调度的Fake回归（已关联+BLE hold仍在+activate持久完成、Sleep无Data联系恢复的图标刷新、EPD失败不能冒充显示成功），再按现有授权修复/构建/部署及认证运行验收。不得靠延长Light、禁用BLE保护或放宽鉴权掩盖问题。

2026-10-09 16:39 USB实机及Bridge恢复路径补证：
- 用户接入USB后枚举COM5，VID303A/PID1001；Composite serial为7C:4F:AD:B9:34:08，匹配登记。使用CreateFileW/ReadFile/WriteFile，仅SetCommTimeouts，无SetCommState/控制线调用，无清缓冲；18秒status/log/status返回2507字节，私有原文artifacts/note4-manual-20261009-usb-163544.private.log。前后认证HTTP均超时、原文未见ROM reset文本，故仅记录未主动设置控制线，不宣称boot_id未变/已排除所有复位。
- 串口运行fw=0.18.73-note4/ota_1，state=WIFI OFF/ip=-/rssi0/heap109828，epd_writes15/busy_fails23/trusted1/frame_black5643。可证采样时确实无Wi-Fi，非仅图标；但busy_fails为RTC累计值，不能当成本boot的23次失败，trusted1和15写入证明部分全帧成功，不能把局刷失败回退直接称为整屏故障。log含多条partial failed; escalating to full refresh及会合network0/light1/elapsed8211ms、awaiting Bridge-selected network over BLE。随后18秒diag/panelpower只读和55秒自然周期监听均0字节；USB再次枚举仍存在，无烧录/下载模式/重置。
- 新定位Bridge实现缺陷：bridge/crates/app/src/wifi.rs::note_match仅接受固定错误名单，不包含设备合法window_closed/storage_unknown等错误；stage=failed/error=window_closed直接return None。automatic_on_link因note_match None返回Ok而不激活；prepare_connection也不更新状态。16:39真实连接缓存仍connecting、observed_at1791533831（16:17:11），而原事务ca已failed/query_result/durable_rejection/window_closed@16:18:40，network_id=home。16:20–16:34多轮wifi_match/network_start/HTTP超时，无新wifi_activate，与此路径一致。证据artifacts/note4-manual-usb-bridge-summary-20261009.json。这是确定的失败恢复实现缺口，不需要靠物理hold假设解释后续不恢复。
- 原始ca窗口为何耗尽仍待设备历史日志/覆盖实际调度的复现；物理hold阻塞路径是代码发现，不能据这次短日志宣称现场hold已确认。睡眠freshness不刷新仍为独立确定遗漏。后续任务追加：合法结构化设备错误必须投影到Connection，按失败类别决定有界恢复；未知错误不得默默丢弃保留connecting。不允许无界换ID重试、放宽token/owner或把Bridge失败当密码错误。

2026-10-09 .74 修复部署任务（用户“继续”，保留此前修复/部署授权）：
- plan/task：将禁止RF开始与事务完成轮询分开，物理BLE hold期间也能确认已连接事务；同配置已健康连接的显式activate不先断开重连。保持原25s事务预算/正式PowerPlan期限，未知结果查询原事务。
- task：Bridge投影所有符合有界错误码格式的失败，不丢弃window_closed；可恢复网络错误有界退避/最多3次，存储结果未知、未知错误及认证失败阻止自动新激活。
- task：Sleep无Data会合按共享判定重画已失效的Bridge标记；只在确认帧成功后更新RTC失联/已画睡眠位，失败留给后续会合重试。
- validation：共享生产Wi-Fi Fake覆盖BLE hold完成/普通BLE延后RF/健康链不重连；Fake Sleep无Data恢复逐像素对照；Bridge失败投影/不滑动退避回归，完整app/render/device-sim测试、Note4匹配ELF栈检查与hash，备份data后重建默认Bridge，Note4 OTA并认证运行验证。书桌屏保持暂停，不提交。
- status/review：实施中；Fake仅共享生产算法/渲染/持久事务，不证明真实RF/EPD，最终部署证据待补。

2026-10-09 17:08 .74 实施/部署收尾：
- 上述任务已实施。wifiSetupPoll新增startAllowed，仅延后deferredSelection开始RF，不阻止已开始事务的期限/成功检查；同SSID已有驱动连接和IP的显式激活不先disconnect/begin。应用不再因物理BLE hold跳过整个poll。既有owner/token/session、25秒预算、正式Light期限和普通BLE释放保护保留。
- Bridge note_match通过有界ASCII错误码验证投影失败，window_closed可在固定退避后最多3次恢复；未知/存储结果未知等错误不自动新激活，不保留假connecting。附带非敏感错误诊断日志。不更改Wi-Fi凭据，也不将Bridge错误当密码错误。
- Sleep无Data会合使用共享sleepFrameNeedsRefresh判断最近确认的标记；联系恢复/过期或未确认睡眠帧会全帧重画。renderActiveUsage仅确认全帧后更新失联位，enterDeep不再无条件宣称已绘制。Fake重启曾丢失联系事实的缺口已补：deep/button保留RTC联系事实；模拟面板失败保留上次确认标记，下次显示重试。
- validation：完整app76通过/3忽略、shared render全部通过，最终Wi-Fi6及完整shared render再通过，完整Fake62/62。新增共享生产Wi-Fi测试覆盖物理BLE hold完成/普通BLE延后RF/同配置健康链不重连；Sleep无Data通过自然深睡重启、新session认证BLE Plan与像素对照，并故障注入验证失败不能提前更新确认标记。Fake不执行Arduino调度/实际RF/物理EPD，实机原ca的物理hold触发条件仍没有完整历史日志证明。
- ROM artifacts/note4-0.18.74-activation-recovery.bin，1,902,416 B，SHA256 C83E75DBCAB4F758CA259CD08C06EAC6553683795B86C65266B49F8ACBA93813；匹配ELF B6C585A4A10C135B2A42D7777FA053054AB53892FD623782C0C536FFF7EA20F6。静态RAM117,948 B（与.73相同），Flash1,851,231 B（+.73 576 B），无新NVS格式或持久空仓库；匹配ELF栈门禁通过，包括wifiSetupPoll144 B/Status976 B/dispatcher960 B。动态峰值实测未新增。
- 默认Bridge最终SHA256 420F336CB235EFC0071195B8190A8A298015FE064ED0579705EC61FB73E61A05，PID45088/watchdog55692，由既有CodexStatusBridge任务启动；旧exe/data私有备份artifacts/bridge-before-fix74-20261009.*。书桌屏暂停/登记保留，未提交。
- OTA原job1172f92d/request note4-activation-recovery-74-20261009在17:01:29排队；两分钟未上传时继续诊断，没有换ID。17:04:24实机原home配置新激活cb，17:04:31认证回读成功，cb最终completed/confirmed_operation；17:04:55 OTA upload_ack，17:06:17自动succeeded/image_verified，attempt_count=1。17:06:52直接认证运行.74/ota_0/reset_reason3（软件重启）/boot7657744f84beed7afba885f1d2f05d32，完整运行镜像hash一致。17:07:31 Wi-Fi connected/HTTP ready、Light2997剩519秒、last_push正常、失联位8清除、epd_writes4/累计busy_fails1；软件显示事实不代替Sleep画面目视、多周期电池/RF/EPD验收。
- USB旁支：用户报告16:52–54无线接收器所在右侧扩展坞断连。Kernel-PnP/Device Management事件1010明确VID1A86/PID80A0在16:52:22、16:53:02、16:53:09、16:53:13意外移除，最多27子设备。拓扑为XHC0/USB(2)，Note4在XHC3/USB(1)，不同root hub。原生COM5只读监听16:38结束，期间仅status/log/diag/panelpower查询、SetCommTimeouts，无DCB/DTR/RTS/下载/烧录/控制器复位；16:52–54仅软件构建/Fake测试。后续.74部署仅网络OTA。证据artifacts/usb-dock-drop-20261009.json，暂不能判定扩展坞供电/线缆/驱动原因，未擅自操作主机USB设备/驱动。

2026-10-09 17:25 数据全链路跟踪任务：
- plan/task：检查源快照→设备认证合同→调度→事务ACK→运行序号/显示。已确认Wi-Fi/HTTP在线而OTA后data_seq0；真实Bridge手动375号完整快照ACK applied/displayed。自动targets把30秒旧状态排除，阻断自身refresh。修复刷新准入与投递新鲜度分离，过期在线设备先认证刷新，离线仍等待真实发现。
- validation/review：添加过期状态与Fake真实HTTP投递回归；重建部署Bridge并跨多个自动周期核对源值/合同/设备序号与确认指纹，禁止靠手动推送冒充自动恢复。ROM不需仅为宿主调度重编，书桌屏仍暂停。

2026-10-09 17:33 自动数据投递修复部署：
- status：旧device_cycle_targets要求online且30秒新鲜，慢工作/OTA/状态刷新耗时使下一周期排除设备，从而永远不能刷新。不是Wi-Fi凭据或activate取消。17:20认证Wi-Fi/HTTP在线但seq0；17:21手动375号ACK显示77，仅为故障定位，不能算自动恢复。
- implementation：targets过期online仍准入，刷新先于claim/data；每6个10秒周期探测登记IP，覆盖桥重启无缓存及短时HTTP失败。探测仍身份/认证有界失败，暂停书桌屏不准入；没有放宽MAC/token/owner或改变Light续期规则。
- validation：完整app76/3条件忽略通过；生产Bridge报告→共享ROM Fake回归扩展真实Bundle、认证token/续约、过期缓存调度、used30→35连续两次自动应用，设备序号递增、in_flight清空、确认指纹匹配latest。第一次Fake夹具缺少合法token/诊断路径断言已修正，最终显式回归通过。git diff --check通过。
- deployment：17:31:23默认任务重启PID54288/watchdog2428，exe76337653390D4EC6F9E162E14BD02B2BF58FD9AF47852A5746F76C0B4DB1432C；备份exe/data artifacts/bridge-before-data-chain-20261009.*。ROM不重刷仍0.18.74-note4。17:32:32自动发送seq377，17:32:34ACK applied/displayed，source used24→余76、acked_push_fp=push_fp676763099578049970；直接认证同boot7657744f84beed7afba885f1d2f05d32/data_seq377/applied_seq377/displayed。只读观察持续，不手动刷新/推送制造恢复。
- review：确认数据源→合同→自动HTTP→ACK→实际运行序号，显示软件回执不代替目视/物理EPD矩阵。运行数据未删、书桌屏仍暂停、USB不操作、未提交。

17:34:51收尾观察：17:34:27新source轮询成功，source_quality=good/source_observed_at1791538473；随后自动认证last_status_at1791538483，acked_push_fp与push_fp均676763099578049970、push_dirty=false、in_flight=null。直接认证seq377/applied377/displayed，同boot未重启，plan2999 remaining417秒（继续递减），无相同快照重复data send。证据artifacts/note4-data-chain-final-power-20261009.private.json与note4-data-chain-runtime-20261009.json。
