# Note4 连接现场调查（2026-10-30）

时间：21:20–21:26，Asia/Hong_Kong。关联 B6/A9；仅只读调查，无重启、刷机、配网或强制接管。

- 用户报告设备未移动，Windows Bridge 刚启动；第二次 Wi-Fi 正常但 Bridge 未连接。后补充 Mac 从办公室带回且一直休眠，可能间歇唤醒联系设备；此为可能原因，未读取 Mac 日志验证。
- Windows Bridge 21:20:53 启动，principal 8c94；PID3308、watchdog28940，8765/8766 监听正常。
- 同 MAC 7C4FADB93408，认证固件 0.18.107-note4/ota_1。设备日志先报 plan_network_target_mismatch（关联开始前校验失败），随后按键 BOOT 窗口中成功关联并取得 192.168.3.177，继而报 bridge_http_unreachable。
- 认证状态确认 wifi_connected=true、bridge_reachable=false、owner_id=56a6，Windows principal 的 wifi_control.reason=occupied。恢复视图 owner 为 Mac Bridge 192.168.3.31:8765；Windows claim 多次409，Mac health请求3秒超时。租约从剩余91秒递减到34秒，last_seen_s保持5，观察期间无续约证据。
- 21:25:28 Windows 自动 claim 200；21:25:31 Data1321 applied/displayed；21:25:35 Plan8913 applied。owner 已变为8c94/192.168.3.111。用户随后确认正常连接。
- 结论：本次后半段 Windows 接手受 Mac 旧租约阻塞，到期自动恢复。Mac 间歇唤醒是用户提供的待证假设；首次 target mismatch 的具体不匹配字段及来源尚未知，不宣称固件问题已修复，不将本轮归因于设备移动或信号不足。

私有证据：artifacts/wifi-families/note4-20261030-network.log、note4-20261030-auth-status.json；原始 Bridge 日志 bridge/target/debug/data/logs/bridge-app.log.2026-10-30。

## Mac 对应逻辑核查（2026-10-30）

只读源码审查，未执行 macOS 原生测试、未修改/部署程序。

- host_wifi.m:125 每次调用 CoreWLAN 读取 interface.ssid；wifi.rs:398–436 在生成计划时重新读取宿主网络。未发现应用层永久沿用启动 SSID 的缓存。读不到当前网络时旧版不应直接选上次办公室网络。
- 最新记录的 Mac 部署是 default-data-sync/status.md 所记 2026-10-23，hash d0f0948c…d35b9；对应变更提交9240ad9。10月27日网络族及wc=2修复只记录Windows部署。Mac当前运行版本仍须实机核对，不能据旧记录断言未更新。
- git show 9240ad9:bridge/crates/app/src/wifi.rs 确认：prepare_connection遇wc!=1直接返回；automatic_on_link遇wc!=1写firmware_upgrade_required后返回；target_query仍将host_ssid/ssid/label纳入摘要且无match_v=2。新Note4为wc=2，故旧Mac可能跳过核对/修复而继续发旧格式Plan；新Windows变更设备网络ID/元数据后，即使密码或SSID未变，也可能产生plan_network_target_mismatch。尚无当时Mac目标快照，不能确定具体字段。
- 当前rendezvous.rs:175先claim，245才准备网络、267生成目标；网络检查失败不会撤销已经建立的300秒租约。因此BLE接触后网络不通/随后休眠可留下占用，符合本次Windows等待旧租约过期的现场。它解释占用窗口，不证明Mac如何从睡眠中运行。
- 最新源码wifi.rs:420/507/591已支持wc=1|2，v2摘要不含显示名。后续优先核对Mac运行hash、当时目标ID/SSID和非敏感错误日志；确认旧版后再安排Mac配套更新，不通过修改家中密码或强制覆盖设备配置来试错。

## 23:46唤醒与Windows自动重启调查（2026-10-30 23:55）

本轮仅记录两个未修问题，唯一待办入口分别为C8、A9。Mac原生构建/验收仍留到Mac；不将本轮归因于Mac。

### C8：Windows原生BLE建链超时后无法确认关闭，触发进程恢复重启

- 23:45:39 scan887发现Note4并开始connect_and_discovery；23:45:43.120耗时4006ms报ble_connect_timeout。
- 随后disconnect清理日志明确为result=ok/elapsed_ms=0，但立即报告ble_native_quarantined_restart_required。两层结果必须区分，不能将此简单归为disconnect调用失败；原生关闭为何仍无法确认尚未定位。
- 23:45:43.533 watchdog记录Bridge service failed/recovery scheduled；23:45:48.602启动新Bridge。当天22:40:16、22:56:17也出现同类隔离/恢复日志，具备重复现场；不推断所有事件根因相同。
- 影响：中断本轮通信并重建Bridge运行态，与下述A9恢复缺口叠加后可造成额外600秒联网。后续需定位原生连接/发现/清理与迟到完成的实际状态；验收覆盖超时后原生关闭确认、锁/资源释放及后续会合，保留必要的未知关闭隔离保护，不能仅隐藏错误或禁用恢复。

### A9：Bridge重启恢复将已确认的同内容数据重新判为push，授予600秒Light

- 23:45:48 Bridge自动重启后，23:46:49准备Data1336；23:46:50经BLE联合Plan/Data ACK，Plan8986 accepted_remaining_s=600。
- Data1336 content_crc=88b1e361，与22:57已ACK的Data1334相同。该CRC只说明已记录的完整内容一致，不构成所有质量/触发指纹逐字段相同的独立证明。
- 源码依据：service.rs恢复Coordinator时还原context/plan，但未将context已确认指纹还原到运行时DataState；refresh_all_contracts→refresh_contract→Coordinator::set_contract把push_dirty设为sync_enabled。plan_for_rendezvous将push_dirty/未确认push视为other_work_want_light，从MAX_LIGHT_S取得600秒。因此启动恢复存在把同值重新当push的路径；尚未用专门红绿回归隔离所有条件，不宣称已修。
- 验收应覆盖同context、合同/值/缺失/质量均未变化的进程重开，不产生虚假push或600秒计划；真实变化、设备丢失Data基线、合同/context变化和未知在途快照仍按原规则恢复。另验证纯周期全量/诊断不会续期。

设备独立认证：MAC7C4FADB93408、wake_seq7260、wake_cause=timer、reset_reason=8；owner8c94、Plan8986。设备先定时唤醒，再接受Windows业务Light，未见Mac发放该计划的证据。

私有证据：artifacts/wifi-families/note4-wake-20261030-{status.json,recovery.json,device.log}；原Bridge日志bridge/target/debug/data/logs/bridge-app.log.2026-10-30（UTC15:45:39–15:47:01，对应本地23:45:39–23:47:01）。相关摘录保存在artifacts/wifi-families/note4-wake-20261030-bridge.log。

## 2026-10-31 修复跟进

以上“未修”为调查时点。两项源码修复已完成，Core/BLE/App 347通过、3条件忽略；尚未部署或完成Windows原生多周期验收。C8现在保留超时原操作并有界收尾，未知关闭保护保留；A9恢复ACK与原在途快照，同值重开不再形成虚假push。见[状态/验收边界](connection-fix-status.md)、[根因与兼容审阅](connection-fix-review.md)。
