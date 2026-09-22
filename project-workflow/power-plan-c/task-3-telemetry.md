# Task 3 — 遥测与验收（无需电流仪）

Status: pending（依赖 task-1 的埋点）

## 埋点

`/status.json`（与 HTML 状态页）新增：

| 字段 | 含义 |
|---|---|
| `awake_ms` | 本次唤醒从 boot 到进入 deep 的非 deep 总时长 |
| `ble_on_ms` | 本次唤醒 BLE 射频在线时长 |
| `render_count` | 本次唤醒屏幕刷新次数 |
| `time_source` | `ble` / `rtc` / `none`（本次时钟来源） |

`/history`：每次 wake 增加时长与结果（thin/rendezvous/light），RTC 环形缓冲容量评估后再定。

## 验收流程

1. **无数据基线（桥在线）**：连续 30–60 分钟，逐周期记录 `awake_ms`/`ble_on_ms`/连接次数/`light` 次数；
   判据：每周期连接 ≤1、`light`=0、`ble_on_ms` ≤3s、awake ≤5%、时钟每分钟更新且 `time_source=ble`。
2. **停桥对照**：停桥 30 分钟；判据：每周期 3s 超时、`time_source=rtc`、awake ≈3.85s（6.4%，单列）、回 deep 不重试。
3. **小数据更新**：制造一次小字段变化；判据：`applied_seq` 前进、经 BLE 到达、不产生 light、数据与时钟同帧。
4. **大更新对照**：验证仍走 light（不计入本 KPI）。
5. **24h 汇总**：`Σawake_ms / 总时长 ≤5%`、估算 mAh/天；有仪器时用 PPK2/Joulescope 对拍 10–20 个周期。

## 工具

- 设备：`/status.json`、`/history`、`/log`、`/pmstats`（0.13.0+）；
- 桥：`power_view_v2` / coordinator summary（in_flight、plan_id、next_seq）；
- 调试：`device_contact_s`、`device_mode`、`device_sleep/wake` 加速循环；
- 注意：`pm_stats` 频率低，勿高频调用。

## 约束

- 结果记录到 `artifacts/`（不入库）；PROGRESS 只记结论与 SHA/ROM 现场。
