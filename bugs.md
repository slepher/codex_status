# bugs.md — 现场缺陷记录

> 2026-09-22 实机发现（设备 `书桌屏` 70041DD7A340 / 192.168.3.163，固件 0.16.7-bw；
> 桥为 `bridge/target/debug` 运行实例）。两缺陷同源：v2 数据通道的 ACK 确认与设备唤醒基线。
> 均未修复，本轮未改任何代码。

## BUG-1：v2 数据推送永不确认，`data_seq` 不推进（设备不刷新）

**现象**
- 设备在线、上下文一致、`display_state=displayed`，但设备长时间停在 `applied_seq=9`；
  桥 `push_dirty=true`、`full_sync_due=true`，`in_flight` 一直不消失。
- 面板停在旧快照（15:01/15:09 实测 WEEK 56 / 5H 36 / BATT 47%），桥实时为
  weekly used 50%（剩 50）、5h used 100%（剩 0）。
- 设备持续收到 `[v2] req data`（桥每 60s 重发同一内容），回 `unchanged`、不重绘。

**证据**
- `power_view_v2`：`in_flight={kind:ble_data, seq:9, crc:010277d2, attempts:1}` 长期不变；
  context `next_seq=9`、`acked_push/full=false`、`last_applied_seq=0`；`data/platform/state.json` 同。
- 桥日志无 deliver 结果（自动下发不落日志），但 usage 变化多次（14:35–14:44、15:11:47）
  从未推送给设备。

**根因**
- `bridge/crates/core/src/coordinator.rs:420`：
  `content_crc = crc32(canonical_bytes(DataSnapshot))`。
- `bridge/crates/core/src/coordinator.rs:761`：发给设备的 `crc` =
  `data_fields_crc(fields)`（与固件逐字节一致）。
- `bridge/crates/app/src/platform.rs:376`：`deliver()` 把**消息体 crc** 传给 `note_ack`；
  `coordinator.rs:449` 拿它与 `flight.content_crc` 比较 → 永不相等 → `AckOutcome::Stale`
  → `in_flight` 不清、`next_seq` 不前进、新快照（seq≥10）永远不发。

**修复方向（未实施）**
- `platform.rs` 改用 `decision["content_crc"]`（`platform/service.rs:963` 已返回）或两端统一为
  `data_fields_crc`；补单测覆盖「ACK 走平台 deliver 路径」。
- 运行中的 `bridge-app.exe` 构建于 14:03:50，早于 `coordinator.rs`（14:15:30）等改动；
  验证前需重建（避免锁 `target/debug`，可用隔离 `CARGO_TARGET_DIR`）。

## BUG-2：唤醒后重放冻结旧快照，pull 的新数据被还原

**现象**
- 每次 timer 唤醒（deep sleep 唤醒 = 重启）后先显示 cache/pull 数据，随后（≤60s，桥下一次
  auto-deliver）被桥重发的 `seq 9` 旧快照覆盖 → 面板还原为旧版数据；下一轮唤醒重复。

**证据**
- 设备 `/history`：13:42–15:03 共 47 条，每 10–12 分钟
  `ENTER_DEEP(ev2) → BOOT(ev1, aux=4 timer) → NET_OK(ev4) → TO_LIGHT(ev6)`；15:03:02 NET_OK。
- 设备 `/log`：本次 15:03 唤醒中 `[v2] req data` 紧跟 `[tpl] rendered quad (PULL)`（应用后重绘）。
- 面板 `GET /frame?which=last` = 旧（WEEK 56 / 5H 36，来自 ≤14:44 的 envelope），
  而 15:03 的 envelope 已是 50/0；`?which=saved`（14:18 睡前抓拍）= WEEK 75 / 5H 35。
- 桥 usage 变化时间点：14:44:36 之后直到 15:11:47 无变化 → 冻结内容属于 14:35–14:44 窗口。

**根因**
- `src/main.cpp:4505`（HEAD 为 `src/main.cpp:4504`）：每次启动执行
  `v2DataSeq.beginContext(v2NowMs(), 1)` → `appliedSeq=0`、`appliedCrc=0`、`haveApplied=false`。
- `src/main.cpp:3363-3370`：`handleV2Data` 把 `dseq`/`dfields` 写入 NVS，但全工程无读回路径
  （无 `getULong64`/`getString`）；`src/v2_runtime.cpp:174` 的 `v2UsageFromFields` 定义未用。
- `src/v2_state.h` `observe()`：无已应用基线时任意 seq 均为 `V2_DATA_APPLIED`
  → 桥重发的冻结 `seq 9` 被当新包应用，`renderActiveUsage()`（`src/main.cpp:3376`）重绘覆盖。
- 与 BUG-1 构成循环：桥永不推进 seq → 设备每次唤醒重放同一旧包。

**修复方向（未实施）**
- 固件：跨唤醒保留/恢复 applied 基线（接通 `dseq`/`dfields` 回读，或按已应用 seq 拒绝旧包）。
  注意：未提交的 0.16.8 只删了 `noteApplied(seq,0)`，**启动清零仍在**，单刷 0.16.8 不消除还原。
- 桥：先修 BUG-1，让 seq 前进、旧包被新快照替换。

## 验证建议

- 桥修复后重建：确认 `in_flight` 清空、`next_seq` 推进、设备 `applied_seq` 递增、
  `full_sync_deadline` 被 ACK 重置。
- 固件修复后 OTA：确认唤醒后 `applied_seq` 不为 0，且 pull 后不再出现数据回退。
