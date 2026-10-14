# macOS BLE 断开竞态修复

## Plan / task

用户要求按系统/设备日志定位后修复。仅修复已确认的 CoreBluetooth 生命周期竞态，保留真正未知原生操作的隔离，使用 Ponytail 的最小根因修复原则。

- [x] 对齐系统、Bridge、设备三端记录，检查锁定依赖实现。
- [x] 在锁定 btleplug 0.11.8 的本地补丁中复现并修复已移除外设的断开/连接状态请求未完成。
- [x] 回归原生超时隔离、构建签名候选，按托管流程部署 Bridge。
- [x] 通过正式sleep计划触发定时唤醒，验证连续两次BLE会合、正常关闭及HTTP恢复；记录边界和交接。

## Evidence / diagnosis

10:38:58.488183 macOS bluetoothd 明确记录目标 UUID 2E8ACD9B-B277-26C1-2213-6338578A6FCC 已成功 remotely-initiated disconnect；58.489267向Bridge central67838送状态，58.490409 GATT disconnected。Bridge 58.529443才开始disconnect，10:39:00.531324超时。

设备CRC校验有效的诊断批次633：醒次2在7885ms接受network_start、8110ms主动BLE off(network accepted)、8142ms开始Wi-Fi、10297ms取得IP。不是设备卡死或系统没断开。

锁定btleplug0.11.8的on_peripheral_disconnect删除外设；之后disconnect_peripheral与is_connected仅在map命中时完成future，外设缺失时无回复。Bridge随后2秒timeout使NativeLease隔离，后台无法再次扫描。需要在库的原生消息线程确认外设已不在表后完成幂等关闭，不能用延长超时/盲清状态替代。

原系统日志私有保存artifacts/bridge-macos/ble-close-system-20261014.json（0600，不入库），设备安全摘要ble-close-device-20261014.json。历史ACK未知与本次原生断开缺陷分别记账。

## Review / acceptance

生产消息处理函数及原future实现的隔离回放：修复前2项Pending失败、2项正常路径通过；修复后4项全部通过。BLE26项（含真正未知原生操作继续隔离）、App94项通过/3条件忽略。`git diff --check`通过。vendor全部文件逐字节对照本机锁定缓存，除internal.rs两处之外原始文件和许可证一致；不修改全局Cargo缓存。NativeLease进入真正未知状态补WARN及restart_bridge动作，不静默清隔离。

Release构建2m56s、签名通过，10:54:06已按托管流程部署。新版20,743,856 B，SHA256 `f3576f025c422f1280c8795fc1fb47795fd407fd5f1abc95245b1f858d03da5a`，supervisor75555。私有备份`backups/ble-close-fix-20261014`保存旧App及data，141文件哈希对照通过；运行路径保持不变。证据`artifacts/bridge-macos/ble-close-deployed.json`，构建/回归日志同目录。

## 实机验证（11:10）

原计划3895在到期前被常规业务计划3896续期，因此没有把持续HTTP在线当BLE修复证据。11:06:17通过现有认证power_plan接口下发一次显式Sleep3897，设备返回applied/accepted_remaining_s=0；随后按原60s周期自行唤醒。没有改Wi-Fi配置或重新刷机，ROM保持.100。

- 11:08:03扫描13识别目标MAC，wake_generation1895199226/wake_seq6；Plan3899/Data590联合ACK通过，11:08:04.564关闭成功（0ms）。
- 11:09:02扫描14继续运行，11:09:10扫描17再次匹配目标，同醒次再次会合。network_start收到accepted，11:09:12.887关闭成功（1ms）；11:09:27会合完成，11:09:40 HTTP Plan ACK通过。
- 11:10:08认证HTTP状态确认.100、Data590、计划3899正常。测试未触发新的原生隔离。原Bridge进程保持运行，无中途重启。

现场已覆盖两次会合和BLE→Wi-Fi恢复；不是两个独立深睡醒次，也不声称长期RF稳定或此次实机精确重演远端先关闭的竞态。第二次系统日志明确locally-initiated，系统断开发生在Bridge最终cleanup前。原始竞态由实际handler/future的确定性红→绿回放锁定。

证据：artifacts/bridge-macos/ble-close-sleep-verification.json、ble-close-live-bridge.log、ble-close-live-status.json、ble-close-live-system-summary.json、ble-close-live-network-summary.json；系统原始日志0600私有保存。AP01修改保持。

## Fake ROM覆盖边界（后续已补齐）

最初的Fake实现通过HTTP走GATT/业务模型，缺少原生外设移除与NativeLease生命周期，因此当时的挂起close测试只能证明业务锁释放。用户指出应将BLE建模为独立设备并经ROM proxy驱动，这个逻辑竞态应在Fake复现；已按此实现。

现在CoreBluetooth与Fake peripheral proxy共享同一close/state/future逻辑，独立控制设备断开和回调丢失，复用生产隔离机制。临时恢复旧漏回复分支时，Fake ROM+假StaticJson数据源测试在第二次连接进入quarantine；保留修复时连续三次数据渲染/连接通过。详细结构、命令和回归边界见[独立BLE生命周期](fake-ble-lifecycle-20261014.md)。物理RF不在此软件测试声明内。
