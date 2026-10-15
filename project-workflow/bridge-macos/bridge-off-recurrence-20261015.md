# Bridge OFF再次失联调查（2026-10-15 11:20，B6/C8）

## 现场与结论

用户报告Bridge OFF，页面最近认证46分钟前。11:20只读核对托管supervisor99172/进程99195、MCP8766仍监听；未重启/按BOOT/刷ROM/改配置。部署仍10:21预算修复4329f4be…bad64。

最后完整认证HTTP为10:33:40（1792031620），最后BLE认证联系11:09:03（1792033743）。页面46分钟指旧HTTP快照，并不等同于Bridge进程关闭，也未反映期间BLE联系。

## 两阶段阻断

1. 10:35起BLE仍能status/owner/claim/wifi_match/Plan；但自动配网路径未完成开网。11:07仍Plan4004 applied，剩324秒，不是完全没有收到Plan。Bridge本地Wi-Fi connection保存waiting_for_window / wifi_confirmation_pending / attempts0；本地未确认事务围栏阻止新activate，automatic_on_link返回此错误，rendezvous的wifi_repair_error又跳过network_start。每约92秒重试，错误持续。10:36:49起明确日志出现wifi_confirmation_pending（10:35首轮尚无此文本，只有同型失败，不能把首次日志时间提前）。
2. 11:09:04.746331 wifi_match开始写，预算4427ms；11:09:06.055794在写阶段失败，耗1309ms，仍剩3118ms/RF预算。outcome=WriteUnknown/reason=write_failed，不是700ms外层取消，也不是预算耗尽。断开确认0ms，但未知写旗使NativeLease进入ble_native_quarantined_restart_required，之后不再扫描；HTTP继续失败。底层失败是否由RF/设备断开/CoreBluetooth引发，当前日志未记录具体native错误，仍未知。

## 未确认历史作业

按生产Job::confirmation_pending谓词，只读现场计算得到3条：

- 3c4519756ecc9178：sync、cancelled、sent/may_have_committed=true、无confirmation。
- a7efde70624b7097：sync、cancelled、sent/may_have_committed=true、无confirmation。
- 1ef9c9c688225ab2：activate、expired、原request w3_0000000000000038、transaction_identity_unknown、无confirmation。

core/platform/wifi.rs:294明确取消/过期不会证明原提交失败，queue约462行据此拒绝新sync/recovery_save/trial/activate。该保护本身符合未知结果不能冒充未提交的合同，不能删记录或改sent绕开。wifi.rs:488前后的自动恢复需要新activate，拒绝后rendezvous.rs:397的wifi_repair_error阻挡network_start；旧Bridge错误随后被note_match继承并用“device Wi-Fi failure observed”日志输出，文案会误导成设备报告。此处wifi_confirmation_pending的真正来源是Bridge本地围栏。

## 验收边界与下一步

早先10:23自然恢复是真实的一轮成功，不等于后续重连和未知提交矩阵已通过。本次10:35之后的断网恢复缺口与11:09原生写失败/隔离叠加；上一轮修复仍有效阻止700ms提前取消，但不能覆盖这些新原因。

需要核对/合法结清原事务身份与结果；为不修改持久配置的存量网络恢复明确准入边界，不能让历史未知提交自动授权新写。原生写失败要保留具体native终态，并在有证据证明原生队列已终止后安全恢复；不能单凭disconnect成功就清未知旗。重启Bridge可解除当前隔离，但不会消除持久的3条未知事务围栏，本轮没有把重启当修复。

证据：artifacts/bridge-macos/bridge-off-20261015-1120-safe.json，仅白名单字段/事件；不含密码或token。本轮未修改生产代码或部署，B6/C8继续开放。
