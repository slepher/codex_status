# AP01 屏幕数据停更调查（2026-11-06，Asia/Shanghai）

## 范围与结果
用户确认是屏幕用量/进度停止更新，不是项目文档进度。仅读取现有日志、运行账本及 Bridge MCP 认证状态；未 claim、推送、重启、刷写或修改运行配置。

已定位到新 Data 发送之前：数据源正常且协调器有待发变化，设备仍在线运行/重绘，但自动占用/投递未推进。自动 claim 的具体失败阶段尚未证实，不将租约为空直接当作最终根因。

## 现场证据
- 登记 MAC 34F01557210B，现登记端点 192.168.31.207:18081；两次 MCP platform_status_refresh 均认证返回同 MAC。
- 最后 Data：15:42:35 seq238 发出，15:42:39 applied/pending；之后认证状态始终 applied_seq=238、data_crc=1679470992（641ab190）。没有下一笔 Data send。
- 数据源最新质量 good/error=null，周额度已用4%，RC2；协调器 push_dirty=true、full_sync_due=true、in_flight=null、next_seq=239、无发布或激活待办。确认指纹与当前指纹不同，排除“值未变化所以无需发送”。全量期限16:42:39已过。
- 两次设备采样 boot_id 均10898439199890799177；uptime_ms 8514396→8842682，frame_generation 156→162，frame_crc 2072620177→1211180388。应用仍能响应并产生新画面，不能据此证明物理LCD每帧呈现，但没有整机卡死证据。
- 两次 owner 均 id空/expires_in_s=0，未见其他Bridge占用；sync_enabled=true。设备回报 displayed 只对应旧数据238，不意味着当前数据源已同步。
- 主机Wi-Fi读数connected/en0；bridge_status显示172.19.0.1，但claim实际经wifi::bridge_address按en0选地址，不能仅凭该TUN地址断言根因。

## 调用链与诊断缺口
platform_sync_loop → platform::cycle → device_occupancy_gate → post_claim_for_mac → post_claim_once。claim依次经过主机地址选择、AP01认证身份预检、正式claim；失败时cycle直接返回。当前日志只在debug输出泛化的device claim unavailable；AP01分支缺少普通设备分支的claim阶段INFO记录。读状态持续成功、租约为空且无新Data，与此门禁阻断一致；仍需捕获实际ClaimError确认哪一步失败，不能宣布根因已结。

另一个独立诊断问题：refresh_http_status仍用公开GET /status.json；bridge_status实际报device GET HTTP rejected，日志却输出HTTP unreachable/ARP fallback。这与认证/api/status成功并存，是错误归类，不能作为AP01网络断开的证据。公共状态缓存与认证状态缓存不同，未证明该错误本身阻断Data。

被动回执捕获未执行成功：系统/dev/bpf0权限不足，sudo非交互需要密码。未修改系统权限、未保存原始报文，也未用重启掩盖现场。

## 接续
归A9/B6/C8协议整改：先给自动占用门禁补可见的结构化失败阶段/原因及AP01 claim回执诊断，复现后修真实原因；核验原context下下一Data applied并与最新源值一致，再由用户确认屏幕。不要重刷基座/救援槽，也不要以重新发布Bundle替代诊断。

## 证据位置
私有 artifacts/ap01-protocol-deploy-20261106/evidence/investigate-*.json：data_sources、coordinator、platform_status_refresh、status-second、device_view、connection及wifi_view。运行日志为macOS数据目录logs/bridge-app.log.2026-11-06（原日志时间UTC，本记录已换算UTC+8）。

## 修复计划（用户授权接续）
Plan：保留现场，获取明确claim失败阶段；修根因并加定向回归，构建部署Bridge，确认原AP01 context下新Data及续租。
Task：诊断/根因/回归/部署/运行验收。
Status：已修复并部署，17:16运行验收通过；未提交。
Review：根因定位后扩展为基座修复，先验证救援槽再更新应用槽；不改救援槽/App/Bundle，凭据不进日志；未知结果先观察原状态。

## 最终实机验收（17:16，Asia/Shanghai）

槽1运行 `0.1.1-cuktech`，build `99ad180acb5c37c4f3cc96db912131e9`；原App `.109-ap01`、集合3和包摘要不变。新boot `7692962541050194540`，health healthy/fault0。救援槽0先实际启动验证维护可用，更新槽1后头/manifest/尾抽样仍相同；不是整槽读回。Wi-Fi完整配置与操作前一致，未切网或清配置。

17:14:33自动claim成功；可写恢复轮换context，旧context请求明确409拒绝后，经设备状态对账，17:14:50新context的Data242 applied；17:15:43自动续租200。17:16认证owner=ef61/剩余269s、Data242/displayed、CRC250b3db0，协调器push_dirty=false/full_sync_due=false/in_flight=null、源good。用户确认“数据已更新，操作正常”。此次未发布Bundle或更新业务App。

延迟边界：现有启动健康批准需约30秒，首次投递遇context切换后对账约13秒，成功Data传输3.8秒。用户询问的长耗时主要是诊断、回归、构建、同机救援验证和6.9MB基座上传。上述启动/context延迟未在本次扩展优化；正常运行UI故障恢复不再永久锁住已验收集合。恢复策略的故障注入由宿主生产组合证明，未在实机刻意卡住UI，也不宣称长期运行无故障。

私有最终证据：`claim-repair-acceptance.json`、`claim-repair-final-base.json`、`claim-repair-final-app.json`、`claim-repair-race-red-green.json`、`claim-repair-upload.jsonl`；均在本专项既有 evidence 目录。两个仓库改动均未提交。

## 根因与修复接续

诊断版 Bridge 取得 HTTP 503 确定拒绝：`app_trial_readonly/lifecycle/read_device_status`。旧 AP01 分支丢弃底层错误并统一报结果未知。基座独立维护返回同 MAC/boot、runtime fault4=UiStale，但当前 App health=0；用户确认旋钮/UI 正常。基座 healthSelected 撤销集合写门，App 遵从只读策略拒绝 claim，导致 owner 到期与 Data 停在238。

生产监督器 seqlock 读取最多尝试三次，竞争失败与真实过期共用立即永久 fault 路径。持续发布同一新鲜时间戳的生产代码并发测试，原代码首轮错误锁存 fault4，修复代码50轮通过。现场故障瞬间未记录，不能证明当时一定由竞态触发；此缺陷可独立产生同样症状。

用户明确选择同时简化运行期策略：已持久批准的集合允许 UI 过期后经新鲜一致的全部 App 健康样本恢复；试运行未批准、AppFailed/Worker/维护失败继续锁存。transaction hold 不能被恢复覆盖，未知持久结果不重试。逻辑归兄弟 firmware 仓库，未复制到本仓。监督器 RV32 sizeof 从28到36B，增加8B常驻 RAM，无新增持久字段、堆分配或大栈对象。

验证：监督器红绿反例；宿主 ASan/UBSan 集合写门/真实文件/生命周期集成通过，含批准后 UI 故障恢复、hold 阻断和 worker 故障仍关闭；RV32 编译/资源审计及最终 ROM 校验通过。独立维护/OTA/无稳定记录切槽回归通过。Bridge claim12项通过，首次两个 Fake BLE 用例被系统代理503干扰，测试进程 NO_PROXY 后通过。跨仓边界守卫通过。

Bridge 已部署81809632B，SHA256 `cb2c81fc6bfbab503fb3785dd2a7c3278f38d4e0f9257e301a8db70f74cd0770`；完整 data 和旧 App 备份 `artifacts/ap01-protocol-deploy-20261106/claim-repair-before.tar.gz`。结构化诊断区分失败阶段、确定拒绝与结果未知，不记录原始错误或凭据。

因根因属于基座，修复范围由最初 App/Bridge 调查扩展到基座。候选 `0.1.1-cuktech` 支持 App，6935928B，SHA256 `07e2af965ab8d433928efb2fd53309873ed08e6e3803df7edeec1c3d60e2b560`，兄弟仓 `build/claim-ui-stale-20261106/ap01-0.1.1-cuktech-app-final.bin`。已先认证切入原0.1.0无App救援槽0，独立维护可用，头/manifest/尾抽样不变；只向非活动槽1上传，最终运行验收待补。不创建稳定记录，不写救援槽或更换 App/Bundle。

## 启动延迟优化（用户授权接续）
Plan：稳定集合首次完整健康样本即批准，不重复30秒试运行；claim认证预检同时更新协调器context/合同，首次Data不使用旧context。
Task：修改共享选择器与AP01 claim预检；覆盖稳定/试运行/回退/故障与context轮换回归；构建、保留救援槽部署，认证核验启动至首次Data时序。
Status：17:32 已部署并认证验收；用户确认ota_1目前不卡顿。未提交。
Review：不新增持久字段/状态机，不跳过文件完整性、实时健康、transaction hold或未知提交保护；新集合与回退继续30秒观察。claim不发布Bundle、不延长PowerPlan。


### 启动优化最终验收（17:32）
- 基座槽1 `0.1.2-cuktech`，build `1e311c12e2881d7ea0195ac00743233d`；ROM `../cuktech-ap01-firmware/build/claim-ui-stale-20261106/ap01-0.1.2-cuktech-app.bin`，6935928 B，SHA256 `0c840a788b73d72e2e0e2a8a69c0b1d4c19a6fd6907232420e5fa508903aeacb`。原App `.109-ap01`/集合3/manifest及package摘要不变。
- Bridge复用AP01 claim认证预检的完整状态，通过已有revalidate同时对账context/合同；不增加HTTP往返。81809808 B，SHA256 `922f03681d994d04877e2b3087484ef64c7bd439cb73c22a919b905175724406`。完整应用/data/plist备份 `artifacts/ap01-protocol-deploy-20261106/startup-before.tar.gz`。
- 17:31:18.660原OTA事务返回重启请求；首次健康采样设备启动时间10.343秒，17:31:31.520 claim成功，31.552直接发送新context Data245，35.431 applied。重启请求至应用约16.77秒（包含启动/联网/调度及3.85秒传输）；claim至首次Data约31毫秒，无旧context Data、409或下一轮对账等待。没有测得精确首次健康时刻，不把采样上界当作启动耗时。
- 17:32认证boot `5131925667226155066`，context `4738433448b9783a0000000000000001`，Data245/displayed，owner ef61；coordinator push_dirty/full_sync_due=false，in_flight=null，源good。17:32:42自动续租200。
- 救援槽0实际启动验证后仅上传非活动槽1；Wi-Fi完整配置、救援槽头/manifest/尾抽样保持一致（不是整槽读回）。用户确认ota_1目前不卡顿。
- 验证：Bridge claim13项通过（含旧缓存到新context的首笔Data且仅一次状态预检）；真实启动组合ASan/UBSan证明稳定集合首次健康开放、Trial仍30秒、hold/fatal继续阻断；AUT3 103189检查覆盖稳定启动不重新持久验收及Trial/Rollback/存储故障；最终ROM校验及独立OTA/无稳定记录切槽/维护回归通过。无新增RAM对象、持久字段、堆/栈预算。两仓diff检查和跨仓边界守卫通过。
- 用户报告ota_0上传时旋钮卡顿，不能确认卡顿起点；当时6553600/6935928字节receiving，health72ms/OTA状态586ms，上传持续推进。暂疑写闪存期间临时阻塞，未取得同槽空闲对照，不能断言仅由上传或排除救援基座UI问题；未为此再次切槽。此现象不等同此前UiStale永久写禁令。
- 证据在既有evidence目录：`startup-acceptance.json`、`startup-observe.jsonl`、`startup-first-data.log`、`startup-final-base.json`、`startup-final-app.json`、`startup-rescue-lag.json`、`startup-claims.log`；兄弟build目录 `startup-checks.log`、`startup-journal.log`、`startup-maintenance.log`、`startup-verify.log`。长期运行与ota_0空闲旋钮对照未验。
