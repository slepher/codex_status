# 服务发现期间断开导致 Bridge 持续隔离

调查时间：2026-10-14 13:56（中国标准时间）。关联 B6/C8。

## 范围与状态

用户报告再次断连，已按 BOOT 开启 Wi-Fi。本轮核对运行实例、保全系统/Bridge/设备日志，并回放生产断开回调；未修改生产代码、重启 Bridge 或刷 ROM。原来的幂等 disconnect 修复仍在运行，新的遗漏位于连接请求完成路径。

- [x] 核对 Note4 MAC、认证 ROM 与运行 Bridge 哈希。
- [x] 对齐连接、服务发现、断开、超时和隔离时间。
- [x] 用生产 `confirm_disconnect` 方法及原 future 实现复现遗漏。
- [x] 连接生命周期修复、共享逻辑红→绿、Bridge部署与多轮实机会合通过；长期RF边界及完整B6保持开放，见联合实施。

## 现场与时间线

Bridge main PID 75559、supervisor 75555，均自 10:54:06 运行；二进制 SHA256 为 `f3576f025c422f1280c8795fc1fb47795fd407fd5f1abc95245b1f858d03da5a`。认证 `/api/status` 确认 Note4 `7C4FADB93408` 运行 `0.18.100-note4`，wake_generation=1895199226、wake_seq=131、reset_reason=8、wake_cause=ext1。用户 BOOT 后 HTTP 可达，当前设备不是崩溃或运行旧 ROM 的证据。

全部时间为中国标准时间：

| 时间 | 证据 |
| --- | --- |
| 12:18:05.107 | 上一轮 BLE 正常收到 status/claim/plan ACK，disconnect 成功，耗时 0ms。 |
| 12:20:04.518 | Bridge scan358 找到目标，进入连接。 |
| 12:20:04.568 | bluetoothd 收到该 Bridge 实例的连接请求。 |
| 12:20:04.825 | 系统向 Bridge 报告 App ready；随后查询服务、特征与描述符。 |
| 12:20:07.477–07.481 | 系统记录链路断开、`Connection timed out`，向 Bridge 派发断开状态；不是仍在等待系统断开。 |
| 12:20:08.520 | Bridge 的 4 秒连接等待结束，cleanup disconnect 成功，耗时 0ms。 |
| 12:20:08.521 | Bridge 仍判原生结束未知，进入 `ble_native_quarantined_restart_required`；此后没有 BLE 扫描/连接记录，HTTP 探测继续失败。 |
| 13:52:53 | BOOT 后 Wi-Fi 可达，Bridge 完成诊断批次645；随后显式认证状态刷新返回 ok。 |

约 92 分钟无法通过 BLE 恢复，与进程隔离持续存在吻合。HTTP 恢复不会清除原生 BLE 隔离。

## 已确认原因

btleplug 的 `connect()` 不在系统 Connected 回调时返回；它等服务/特征/描述符发现完成后，由 `check_discovered()` 完成 `connected_future_state`。

若期间收到断开，`on_peripheral_disconnect()` 调用 `confirm_disconnect()` 后删除外设。该方法完成断开和特征操作的等待，**遗漏 `connected_future_state`**。Bridge 的连接 future 因而永远 Pending，直到自身 4 秒超时。`open_link()` 对原生超时保留未知判断，即便后续 disconnect 已成功，也隔离整个原生 BLE 通道。

最小回放直接提取生产 `confirm_disconnect()`，导入生产 `future.rs`，只替换 CoreBluetooth 外设对象。构造 pending connect 和 pending close，再执行原生断开回调：close 为 Ready(Ok)，connect 仍 Pending；要求 connect 明确返回 Device disconnected 的断言失败。日志和源码见下方证据。它证明的是导致持续失联的完成逻辑遗漏，不是物理传输模拟。

上次修复覆盖“外设已移除后的重复 disconnect/state 请求”，因此本次 cleanup 的 0ms 成功与上次修复有效相符。现有 Fake 生命周期测试从连接已建立开始，未覆盖服务发现期间的 pending connect。

## 证据边界与修复要求

系统确认最初发生链路超时，但不能仅凭 reason 708 判定设备主动关闭、无线干扰或固件调度停顿。设备诊断批次645明确记录序号144–324被覆盖；当前 wake history 也只保留104以后，缺少当时醒次的设备细节。该初始断开原因仍未知，不能冒充已定位。当前 `/log` 只保留较晚会合 deadline 和 BOOT 联网记录。

修复应在依赖的原生断开回调中明确结束 pending connect；对已移除外设再次 connect 也应明确拒绝，避免有限重试进入另一条漏回复分支。真正没有结束回调的原生操作继续隔离，不通过盲清 NativeLease 或增大超时掩盖问题。补共享生产逻辑的最小回归：服务发现前断开、断开先于 cleanup、再次 connect 的明确失败、下一轮新连接可用，以及真正未知仍阻止重叠操作。

本次持久失联缺陷位于 Bridge 依赖层，修复无需新 ROM。现场当前 HTTP 可用，BLE 隔离仍需通过受控 Bridge 重启或修复部署恢复。

## 私有证据

均在已有 `artifacts/bridge-macos/`，原始内容不入库：

- `new-disconnect-20261014-system.raw.json`：系统 12:19:45–12:20:40 蓝牙日志，0600。
- `new-disconnect-20261014-bridge.log`：断连起点与 BOOT 恢复日志，0600。
- `new-disconnect-20261014-{status-json,log,pmstats}.raw`：设备网络现场，0600。
- `new-disconnect-20261014-auth-status.raw`：认证运行状态，0600。
- `new-disconnect-20261014-summary.json`：安全摘要。
- `new-disconnect-connect-replay.rs` / `.log`：生产断开回调失败回放。

运行数据中的 `platform/diagnostics/7C4FADB93408/645-0a6d7e0dee9b3bb250fbcfe15ae42ce4.json` 保存当前诊断批次及覆盖缺口。

## 14:28 再次 BOOT 的现场补证

用户报告再次断连并按 BOOT，立即保存 `/status.json`、`/history`、`/log`、`/pmstats` 与整份 Bridge 日志，随后认证读取状态。仍为 .100-note4、同一 wake_generation，醒次161为 button/ext1；此前155/157/159等 timer 醒次约5.3秒正常结束，结果均为 no_connection，夹杂约0.8秒的 thin 时钟醒次。设备日志反复 BLE session on → rendezvous deadline off，没有新 panic/WDT 证据。

Bridge supervisor75555/main75559始终自10:54运行，最后BLE事件仍是12:20:08的隔离；之后没有新的BLE扫描、连接或隔离起点。本次是同一隔离在前次Wi-Fi窗口结束后的延续，不能计为新修复部署后的复发。BOOT后认证network.bridge_reachable=true、Wi-Fi RSSI -33；HTTP Plan3921及Data600 applied/displayed通过。设备仍按原机制会合，是Bridge不再尝试连接。

证据 `artifacts/bridge-macos/disconnect-20261014-142831-{status-json,history,log,pmstats}.raw`、`-auth.raw.json`、`-bridge.raw.log`，原文0600；安全摘要 `disconnect-20261014-142831-summary.json`。连接修复与离线日志实施见[联合实施](offline-log-connect-fix-20261014.md)，本段采集时均未部署。
