# Task 2 — 桥：时间下发、扫描覆盖率、窗口去重

Status: pending

## 改动点

1. **回复携带时间**
   - BLE 的 `status`/`plan`（以及 `data`）回复增加 `server_time`（epoch 秒）与 `tz_offset_min`；
   - 数据来源用桥本机 `now` 与本机 UTC 偏移（legacy 信封已有同名字段实现，复用语义）；
   - 字段缺失时设备侧退化为本地 RTC（见 task-1），桥不得阻塞。

2. **扫描覆盖率（Plan C 硬前提）**
   - 现在：扫 3s → 睡 5s（`app/src/main.rs:2328-2344`），对 3s 窗口命中率不足；
   - 改为：连续扫描（PC 市电）或扫描间隔 ≤0.75s；仅在设备会合窗口附近/有工作时连接；
   - 同一会合窗口只连一次（记录窗口标识/`last_connect_at` 去重）；
   - 无工作时也连一次发 sleep plan（让设备提前关窗）。

3. **传输语义**
   - HTTP 交付不得报成 BLE；BLE 交付记录 `transport="ble"`（现状已有，保持）；
   - `ble_cycle` 的超时/重试不得多于每窗口一次。

## 验收

- 桥日志：每 60s ≤1 次 BLE 连接尝试/成功；
- 3s 窗口实测命中：连续 30 个周期连接成功率 ≥95%（实测记录）；
- 回复字段包含 `server_time`/`tz_offset_min`（单测或抓包）；
- `cargo test --workspace`、`cargo fmt`（新增/修改文件）、`git diff --check`。
