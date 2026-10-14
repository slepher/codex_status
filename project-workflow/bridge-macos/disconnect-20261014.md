# Note4 再次断连调查（2026-10-14）

## Plan / task

用户报告再次无法连接，要求调查。保留现场，核对部署/日志，再区分设备端 ACK 缺陷、Bridge 生命周期阻断与权限/主机状态；只调查，不启停/刷机/新建业务作业。关联 B6/C8。

- [x] 核对登记、部署哈希、HTTP 可达性与跨日 BLE 日志。
- [x] 对照生产 NativeLease/close_bounded 控制流，运行现有故障测试。
- [x] 记录确定结论、推断边界与恢复步骤；保留工作区其他 AP01 修改和暂存状态。

## Status / evidence

全部时间为中国标准时间。唯一登记 Note4 MAC `7C4FADB93408`，缓存地址 `192.168.31.90`；旧地址只作探测目标，未作为当前身份认证。最后一次认证 HTTP 成功是 **10月13日18:26:15**：`.99`、boot_id `784b648ef2f8350c110e458492036682`，Plan3889 remaining=0。之后无成功 HTTP 日志。10月14日本轮 GET `/status.json` 5秒超时，不能报告设备当前开机/电量/复位状态。

10月13日19:16:04及19:36:05 BLE INFO 验证同一 MAC / generation2443911192，seq120/140。19:16会合在5秒后失败，清理成功；**19:36:10.286开始disconnect，19:36:12.288报disconnect_timeout（2001ms）**，随后会合失败。此后到本轮检查无 `bridge_ble` 日志。次日仍有HTTP轮询与ARP回退。

Bridge health204；原托管 supervisor1941/child1943 仍运行，安装文件 SHA256 仍为 `df4d523d318446dfb78a7ad9a46aa8245ab35acb0efbbfde29e14782e210a7b2`。只读 `platform_wifi_view(refresh=false)` 返回 macOS Bluetooth/Location allowed，排除当前权限未授予。缓存 automatic_state=connected 不证明设备当前在线。无 USB 串口设备，仅蓝牙串口；本轮没有打开串口。

## Root cause / review

1. 昨天的固件稳定 MAC 修复仍未部署；.99仍具备 BLE-first endpoint ACK 全零 MAC 的缺陷。本轮19:16/19:36的INFO→5秒失败与其吻合，但未现场抓取ACK，不把吻合当作新抓包证明。
2. **长时间无法再尝试 BLE 的直接原因是 Bridge 永久隔离状态**：`close_bounded` 在断开超时后令 NativeLease.stopped=false；Drop 将全局 NATIVE_RADIO_STATE 设为2。随后 `Pusher::adapter` 的 acquire 在任何原生调用前拒绝 `ble_native_quarantined_restart_required`。代码不存在进程内清除此状态的路径，因此同一进程不会自行恢复。
3. App后台将该错误仅记 debug，并以通用分类隐藏细节；默认日志没有“需要重启”的持续可见提示。保护原生异步操作不重叠是合理的，但恢复路径和用户反馈未闭环，不能简单把状态重置为0或延长超时冒充修复。

现有测试二进制 `artifacts/bridge-macos/target/debug/deps/bridge_ble-ccb44d15dafb3455` 中 `failed_native_open_and_unknown_cleanup_quarantine_without_retrying_native_work` 通过（1项，2.32秒）：注入connect/setup/cleanup未知结束后隔离，并阻止后续原生调用。此为已有生产控制流故障回归，不是新ROM实机验收。

未直接读取进程内存中的状态值；根据已记录timeout、确定性的生产状态转移、同一运行实例和后续日志缺口判定。`device_discover(via=ble)`直接探针被自动审批拒绝，原因是可能建链并保存地址，未执行也未绕过；改用只读权限/日志和离线测试完成调查。

## Recovery / acceptance boundary

当前进程需要通过托管启动流程重启才能解除BLE隔离，但单独重启仍可能再次遭遇.99的端点回执问题。长期处理应配套刷入稳定MAC修复，并让Bridge明确暴露restart_required、通过确认旧原生操作已结束或受控进程重建恢复；不得不加验证地清隔离状态。

本轮未修改生产代码、未重启Bridge、未构建或刷机。下一步恢复/修复与自然timer醒次验收仍待执行；需要刷机时遵循用户约定，先通知按BOOT/light-sleep。安全摘要 `artifacts/bridge-macos/disconnect-20261014-summary.json`，原始日志在用户私有运行目录。
