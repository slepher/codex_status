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

6. **分阶段计时与通道对照（2026-09-22 追加）**
   - 单次 wake 分段：boot/面板、扫描发现、connect、GATT discovery、INFO、每条命令、渲染、关断；
     目标：无数据 ≤3s / 超时 ≤4s；超限项直接对应 task-2 §4 或 task-5；
   - BLE 会合 vs Wi-Fi 快连各 ≥30 周期对照（awake_ms、估算 mAh/周期），回答“哪条通道更省”；
   - 窗口命中率与预测误差：开启 task-5 前为连续扫描命中率；开启后测“预测窗口内心跳命中率、
     重捕获次数/24h”；
   - 会合周期分布：统计相邻 rendezvous 间隔（当前偶见 ~2 分钟，确认是分钟对齐所致）。
   - 工具：桥日志 `v2 rendezvous complete/failed` + `HIST_WAKE.dur_ms` + `/status.json` 分阶段字段
     （必要时给固件 DevLog 加段计时）。

7. **功耗预估（采用“CPU/射频时长 × 手册电流”，无需电流仪，2026-09-22 定）**
   - 埋点（0.17.2+）：`/status.json`
     - `render_ms`：本次唤醒所有波形耗时累计（`wakeRenderMs`，含时钟窗）；
     - `light_sleep_ms` / `light_sleep_count`：本次 boot 的 PM light-sleep 累计（来自 sleepDiag）；
     - `deep.acc_cycles/acc_awake_ms/acc_ble_ms/acc_render_ms`：**仅 deep 周期**的 RTC 累计
       （light 会话不计入），用于取平均；
   - 电流表（ESP32-S3 DS v2.2；`plan.md` 依据同源）：
     - deep 7–8µA；light sleep 240µA；
     - CPU active（DFS 80/240MHz 混合）20–40mA（base 30）；
     - BLE RX 93mA / TX 0dBm 176mA；广播占空 2–5% → 窗口平均取 120mA（base），上下界 93–176；
     - 渲染按 CPU active 计（面板/SPI 占比小）；
   - 公式：`E_cycle ≈ ble_s×I_ble + (awake_s−ble_s)×I_cpu`；
     `mAh/day ≈ cycles/day×E_cycle + I_deep×(86400−cycles×awake_s)/3600`；
   - 工具：`node tools/estimate-power.mjs [ip] [--period 60]`（读 acc 累计，打印每周期
     分段/awake 占空/每日 mAh 的 low-base-high 区间与电池天数）；
   - 局限：是**估算**不是实测；CPU 电流区间、BLE TX/RX 混合比例、WiFi light 会话未入
     acc（单列）；结论前用 USB 功率计长时间均值或 PPK2 对拍 ≥10 个周期；
   - 验收口径：task-4 A/B 与 24h 汇总均以该脚本输出 + 桥日志为准。

## 工具

- 设备：`/status.json`、`/history`、`/log`、`/pmstats`（0.13.0+）；
- 桥：`power_view_v2` / coordinator summary（in_flight、plan_id、next_seq）；
- 调试：`device_contact_s`、`device_mode`、`device_sleep/wake` 加速循环；
- 注意：`pm_stats` 频率低，勿高频调用。

## 约束

- 结果记录到 `artifacts/`（不入库）；PROGRESS 只记结论与 SHA/ROM 现场。
