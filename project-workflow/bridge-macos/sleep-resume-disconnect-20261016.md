# 休眠后持续断连调查（B6/C8，2026-10-16）

## Plan / task

后续用户明确授权继续定位并实现自动恢复与错误反馈。实施计划：复现原生发现失败不完成connect的路径；补终态回执和阶段日志；隔离后复用supervisor受控退出/退避重建，持久化恢复状态供UI/MCP读取；回归覆盖进程退出顺序、重复失败退避、恢复状态不提前清除；执行设备页Node/WK与截图，构建备份部署并验自然会合。不得重发或清理未知业务事务。

保全日志和被动状态投影，核对系统休眠与 BLE 故障先后；用托管 Bridge 重启作单变量恢复验证，等待自然会合，不按 BOOT、不改固件或历史未知事务。

- [x] 核对交接、登记 MAC、运行实例、健康与最后联系。
- [x] 确认原生隔离路径和系统睡眠时间。
- [x] 保存现场后托管重启；健康204、扫描恢复。
- [x] 自然醒次认证与最终恢复核验。

## Status / evidence

时间均为中国标准时间。唯一登记 MAC `7C4FADB93408`，登记 IP `192.168.31.90` 仅作探测目标。原托管 supervisor37270 自昨天持续运行，health204。被动投影最后认证为10月15日18:01:48、固件缓存为0.18.104-note4；本轮开始无法取得设备当前认证状态。

10月15日18:02:59.450852收到匹配广播（scan2687），18:03:03.453182在4s连接超时后开始disconnect；清理返回ok/0ms，但连接超时留下原生完成未知，NativeLease仍进入隔离，18:03:03.453942 WARN `ble_native_quarantined_restart_required`。此后到本轮无BLE扫描日志，HTTP轮询/ARP持续失败。

系统日志首次合盖休眠为10月15日18:20:54，晚于BLE隔离约18分钟；10月16日09:49:10转FullWake。因此休眠不能解释本次首次隔离，唤醒后同进程保留隔离解释持续不恢复。具体4s连接超时原因未知，不归为昨天SPI显示自锁，也不把disconnect成功当作在途connect已结束证明。

## Hypotheses / review

1. 进程级隔离阻断恢复：重启后应恢复扫描（已证实）。
2. 设备广播但会合失败：扫描恢复后应见匹配广播，随后连接/协议失败。
3. 设备不广播或网络变化：扫描恢复后仍无匹配对象，或认证网络恢复失败。

生产 `open_link` 将 `Elapsed` 标记unknown，`lease.stopped = stopped && !unknown`；Drop设全局状态2，后续adapter提前拒绝。App后台只debug记录机会不可用；被动设备卡仍是“等待下次联系”、attention=[]。这是恢复与反馈缺口，不能靠直接清隔离值修复。

11:29:00保存故障日志/投影和业务作业哈希后，通过`tools/bridge-macos-service.py stop/start`重启默认实例，supervisor56032；首次即时health请求早于监听就绪失败，随后health204，11:29:05新scan1正常。运行数据保留。

11:29自然会合INFO确认generation3742714749/wake1849；wifi_match确认，network_start完整写104ms/ACK136ms，disconnect1ms成功。11:29:29认证HTTP proof确认Plan4048，11:29:42 sync-v1 complete。11:30独立认证刷新确认MAC、.104、bridge_reachable=true、RTC ready/valid，主卡recently_authenticated。jobs/asset_jobs/bundle_jobs/ota_jobs序列化哈希全部保持不变（不包含Wi-Fi作业哈希，未扩大结论）。恢复反馈环验证假设1；最初4s原生connect超时原因与永久恢复机制仍开放。

证据位于 `artifacts/bridge-macos/sleep-resume-20261016-*`；原始运行日志在用户私有data/logs。未构建、未刷机、未提交，既有工作区修改保留。永久修复应提供原生操作隔离后的可见故障和受控进程恢复；本次重启不等同修复。

## 再次复现与原生阶段（11:40）

用户在实现期间报告再次断连，现场仍为旧Bridge。11:39:32最后sync complete；11:40:40.338广播匹配，11:40:44.340连接超时，disconnect1ms完成后隔离，health204/supervisor56032未变，无主机休眠必要条件。

系统两次原文分别私有保存在`connect-timeout-system-20261015.log`、`connect-timeout-system-20261016.log`。昨晚18:02:59.643链路连接、.836 linkReady、.838 MTU ready、.845请求服务、.867系统从缓存返回primary services；后续没有特征发现请求，18:03:03.392才有App取消请求。锁定库的connect等待完整发现，不能把这一阶段称为未建立物理链路。

本次11:40:40.675 GATT connected，但11:40:44.380系统明确记录Failed to encrypt/STATUS719，.381断开且encrypted:0，.384 GATT disconnected；.413才收到App cancel。**两次共用超时→永久隔离放大链路，但首次失败阶段不同**。当前样本不能证明绑定丢失、密钥损坏或设备RF截止；未解除绑定。昨晚服务回调为何不完成仍未知，不能将源码中发现的错误吞没缺陷当该历史事件的直接实证。

## 实施与审阅

- CoreBluetooth三种发现失败回调原来静默丢弃NSError，等待中的connect只能超时。新增DiscoveryFailed消息，经实际连接future完成器返回明确stage；WARN记录数值错误码。补空服务发现终态及迟到重复发现不panic；保留4s期限与真实未知隔离，不靠延时或清状态。
- 原生open日志明确connect_and_discovery期限，补link_connected/services/characteristics/descriptors/discovery_complete阶段，均不记录凭据或属性值。
- 隔离由后台循环检测（含暂停/权限不可用时），落盘结构化error/stage/action/attempt/retry_at；5/30/120/300s退避，旧child退出75、supervisor wait确认后重建，恢复退出不占普通崩溃熔断次数。Windows沿既有watchdog同退出码处理，未作Windows实机验收。macOS直接启动App同样进入supervisor。
- 恢复状态跨进程保留，必须验证BLE联系且原生载体不隔离才清恢复状态；普通启动、扫描或HTTP成功不清。业务结果独立，不把无线恢复伪写为配置作业成功。持续健康300s才重置退避级别。
- UI/MCP共用投影明确蓝牙异常/自动恢复，提示无需BOOT及恢复次数；列表attention同步，不再仅显示等待。原生操作未知时保持隔离直到进程退出。

## 回归 / 部署进度

App107通过/3既有条件忽略、BLE28通过；新测试覆盖跨进程退避、连续4次恢复不触发崩溃熔断，以及真正子进程退出后才能启动替代进程。原生回调源码回放覆盖服务/特征/描述符错误→生产消息路由→原connect future；旧版缺失错误分支失败，新版3项通过。Node旧HTML失败/新HTML通过；WK68场景通过，560/1000自动恢复截图已审阅。合成IPC和源码回放均不声称证明物理RF、加密或真实Rust/设备通信。

11:56停止已核对旧Bridge，App/data私有备份`backups/ble-recovery-20261016-115638`，154文件逐份哈希一致，旧exe仍为a99e759c…。11:58构建、签名及部署完成：79,575,872 B，SHA256 `466bc079ea937598528e42447f91f4f2f016f870d5207be30a000e1810e07e79`，supervisor66915/health204。启动前154文件哈希仍一致。最终App107通过/3忽略，新增NSError数值日志另经实际native编译/源码回放通过。未刷固件、未提交。

11:59只读platform_wifi_view确认新版macOS Bluetooth/Location均not_determined、host permission_required，因签名变更需要系统授权，尚无BLE扫描。本轮已请用户通过Bridge权限入口允许两项；此阶段不宣称设备已恢复或实机自动重建通过。证据`ble-recovery-{backup,deployed,build,final-app}-20261016.*`及`native-discovery-{red,green}-20261016.log`（artifacts/bridge-macos）。

自动恢复反向回归补齐：临时恢复旧supervisor策略，同一真实子进程测试在第三次退出停止（期待5次，实际3次），按预期失败；finally还原修复后两项测试通过。日志`ble-recovery-supervisor-{red,green}-20261016.log`。此过程只重编测试可执行文件，已部署exe独立SHA256复核不变。启动后jobs/asset_jobs/bundle_jobs/ota_jobs及wifi.jobs内容与备份全部相同（`ble-recovery-post-start-20261016.json`）。

## 授权后实机验证（14:02–14:06）

用户确认已允许，独立MCP读取Bluetooth/Location均allowed、host connected。首轮wake2102已完成原生发现/身份核验，但sync_config后到wifi_match前约8.8s消耗RF，最终明确NotSent/window_budget_exhausted，cleanup0ms，未隔离。未把该轮当网络恢复。

下一自然timer wake2104在14:03完成Plan4050/Data645；SyncOpen与network_start回执未知如实保留，14:03:45认证HTTP proof、14:04:00 sync-v1 complete。14:03:59独立完整认证确认MAC/.104、timer/reset8、Bridge reachable=true、manual/provisional均0、Plan4050 light。此次没有重启Bridge或按BOOT。

待同步完成后，用正式power_plan sleep下发4051，设备返回applied/accepted_remaining_s0。14:05:54自然wake2106，link_connected→services1→characteristics7→descriptors→discovery_complete约409ms，status/claim/Plan均ACK，14:05:55会合完成。supervisor66915保持，授权后无隔离记录，恢复状态文件未创建；因此**正常重连已验，但真实隔离后自动重建还未现场触发**，不得将源码/子进程回归当现场证明，也不人为破坏绑定制造故障。

补正Wi-Fi验收取数：实际持久路径是`wifi[MAC].jobs`，先前顶层`wifi.jobs`比较无效，已在post-start证据注明并移除该项。重新与部署前私有备份逐份比较，9条作业完全相同，SHA256均`a76897d13c574abc7ae290f6aef210a795fc320c8ae464cc73cf50a7cc7aa8df`，独立证据`ble-recovery-wifi-jobs-20261016.json`。实机完整状态、被动投影及日志分别为`ble-recovery-live-status-20261016.json`、`ble-recovery-live-view-20261016.json`、`ble-recovery-natural-live-20261016.log`；正式sleep回执`ble-recovery-sleep-20261016.json`。权限等待解除；最初发现/加密失败的深层原因和长期实机隔离恢复仍开放。

14:07:01自然wake2108再次认证同MAC，14:07:03会合complete。同supervisor、无隔离、无手动重启，覆盖正式Sleep后两个不同自然醒次；没有现场自动child重建证据，不扩大结论。最终日志已补保存，`git diff --check`通过。
