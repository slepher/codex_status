# 超时预算与取消收口（B6/C8，2026-10-15）

## Plan

前置已部署成果提交21a98a5。延续超时审查，只改相关传输/业务调用，不改固件、鉴权、历史回执或未知写隔离。候选按下面顺序实施，完成回归后备份部署Bridge。

## Task / status

- [x] BLE：自动恢复、分页、首次token、旧history预算下沉carrier，取消/错误准确透传；生产Fake慢写/ACK丢失/预算耗尽/后续会合回归。
- [x] HTTP：就绪用认证compact，核对所需字段，不把单页200当成功；claim的身份预检/回退/worker共用原绝对期限，不用外层短取消。
- [x] core：RPC锁/写/应答共用期限、pending取消清理、半帧退役；恢复/功耗GET总deadline与响应上限。
- [x] 相关全套回归、diff审查、备份部署、实机认证只读验收。

## Review / 诊断依据

沿用timeout-audit-20261015.md已有四条慢写复现，新增正式回归先红。按证据依序验证：外层取消绕过carrier结果分类；完整分页成本与短探测预算不符；spawn_blocking/管道写阶段独立预算造成迟到工作。真正partial/native write超时仍应隔离，测试不得通过清标志伪造恢复；完整写后的ACK未知不应伪造partial-write隔离。

## 实施

BLE新增command_budget及wifi_read_budget/wifi_snapshot_budget，逐命令取phase剩余、5s命令上限和认证RF截止最小值；分页不重新开始总预算。自动恢复仅主机查询可被外层取消，GATT链不再包外层timeout；match/owner/恢复作业原错误向上传递，session建立失败也不继续发命令。note_link、事务前wifi_status与回执读取共用显式budget。首次token移入10s会合phase，保留5s carrier上限，失败立即结束本轮，不再继续endpoint/Plan；NotSent/WriteUnknown/AckUnknown结构化返回。真正部分写/原生写失败仍保留write_uncertain及NativeLease隔离，不清旗。

旧history去掉500ms外层和2s内层两套计时，改为单一2s诊断Budget再与原shared_tail/RF取最小值；逐页使用剩余期限，错误透传后不继续该轮可选GATT工作。已确认Plan/Data不重发。可选回执/恢复尾部仍可有自己的短预算，但预算只能在carrier内生效。

HTTP快速就绪探测/控制预检改认证compact，身份/nonce等校验不放宽。最终HTTP proof继续取完整状态（compact会删除sync/data_contract，不能提供完整窗口事实），以原shared_tail总deadline读完分页，移除每次3s重启全读；worker开始时取剩余期限。claim预检、认证回退及socket worker传入同一个Budget；默认一次claim9s，子阶段2/2/5s仅上限，worker排队不刷新期限，取消等待不延长socket写许可。原MAC/operation token/401/409规则不变。

Codex RPC锁等待/写入/flush/应答统一20s截止；同步小pending表配合Drop清理，取消/异常/应答迟到均不遗留条目。写中取消或失败会退役stdin，下一请求拒绝复用；正常完整写后应答超时不会冒充半帧。反向RPC答复也沿用同一有界写 helper。运行循环原有失败重建子进程策略不变。

恢复/status.json与/pmstats GET采用一个绝对截止，connect、每段write/read按剩余期限；响应最多64KiB，UTF-8/HTTP状态及分隔符明拒。未添加依赖、固件变更或对真实Codex子进程进行故障注入。

## 回归与审查

正式先红：timeout-budget-red-20261015.log同时证明慢token被取消且阻断下一命令、自动恢复丢失结果分类、分页将WriteUnknown变成window_budget_exhausted。修复后同用例通过。GET两项先红timeout-get-red-20261015.log证明慢流突破总期限、响应无上限。

新增生产调用测试包括：慢token1400ms及history700ms成功后继续status；预算300ms写中超时保留result_unknown及真正未知写保护；已完成写但ACK丢失（match/owner/history/snapshot/status）可继续独立status；真实ble_cycle首次token失败不再进入endpoint/Plan；完整HTTP分页超过3s但仍在原6s期限内成功、200ms预算不能续期；claim慢预检耗尽预算后无迟到POST；RPC锁/应答/背压超时及外层取消清理pending并保护半帧；GET慢流150ms总截止及70KB响应拒绝。

一个旧组合用例在新实现中3s慢响应被完整读回，继而发现缺失session。原断言预期人为3s超时已不适用，改为要求真实http_session_mismatch，仍核对作业保留、无claim、Plan仅发一次和有界结束；实际预算耗尽由独立200ms/慢分页用例验证。WK/UI本轮未改变，已部署逐秒倒计时保持。

最终完整回归：App104、BLE27、core157及集成7+7+8，共310通过/3既有条件忽略；`artifacts/bridge-macos/timeout-budget-regression-20261015.log`。RPC首次测试因debug大JSON序列化已消耗短期限、尚未写入而失败；改为较小且仍能产生真实管道背压的载荷，并等待stdin写锁进入后取消，不放宽生产20s期限。三项RPC专项与最终全套均通过，`timeout-rpc-green-20261015.log`。最终代码审查及git diff --check通过；没有清除未知写/owner/原作业保护。

## 部署 / 当前实机边界

12:14托管部署完成，exe79,481,840 B、SHA256 `a99e759c2f8158be635e69c81cc76189f0179290e0f8fd4f689e43475f521995`；supervisor37270，health204。备份 `~/Library/Application Support/dev.codexstatus.bridge/backups/timeout-budget-20261015-121322`，153文件哈希一致。9条原作业哈希不变，未改配置/ROM/BOOT/USB。

部署后完整认证HTTP在45s有界复验内均连接超时，不能记设备已恢复。只读platform_wifi_view确认bluetooth/location均not_determined、host permission_required、automatic permission_required；自动扫描按权限门禁停止。已异步请用户从Bridge设备页完成系统授权，设备保持电池供电、不按BOOT。两包ad-hoc designated requirement分别绑定不同cdhash（旧bdf904e853a361b86f1e3c2d3b4ed21f28276bc0，新acfb62c4c0db808182059352d4fab15cd1871ccf），与新版需重新授权的现场一致；没有修改TCC数据库/签名政策或放宽权限门禁。下一步等授权后认证HTTP及自然会合验收。

证据 `artifacts/bridge-macos/timeout-budget-deployed-20261015.json`、`timeout-budget-build-20261015.log`、`timeout-budget-permissions-20261015.json`。未产出成功live验收文件，不将此前旧版11:38成功借作新版本证明。

## 系统授权后最终实机验收（14:00）

用户确认完成授权；只读查询bluetooth/location均allowed，host connected。用户确认设备仍在Mac约1m内、电池供电、屏幕时间正常更新，持续不按BOOT。初次45s读状态仍离线，继续保持自动扫描；14:00自然会合成功，wake_generation951072247/wake_seq1186。

同一会合中wifi_match已确认、claim已确认、Plan4011 applied；SyncOpen在5s carrier截止为AckUnknown，network_start完整写后ACK read_failed同样AckUnknown；没有把两者写成已确认成功，也没有partial-write隔离。BLE正常收尾后14:00:19.854338认证HTTP proof成功，14:00:31 sync-v1 transfer complete/整个rendezvous complete。

14:00:55独立完整认证状态：MAC7C4FADB93408、0.18.102-note4，wake_cause=timer/reset_reason8，HTTP ready=true、bridge_reachable=true/error=null，正式Plan4011剩554s，manual_remaining_s=0/provisional=false/剩0。9条历史作业哈希全部不变，无新增配置作业。部署exe哈希仍a99e759c…521995，health204。此前“待系统授权/完整认证未通过”是12:17过程状态，现已解除；新版本正常链路恢复验收通过，不把单轮成功扩大为全部RF/原生连接/长期矩阵通过。

证据 `artifacts/bridge-macos/timeout-budget-live-20261015.json`、`timeout-budget-natural-recovery-20261015.log`、`timeout-budget-permissions-authorized-20261015.json`；未作实机故障注入，慢写/丢ACK/取消等矩阵由前述宿主测试证明。本轮后续源代码未提交，用户要求先提交的此前成果为21a98a5。
