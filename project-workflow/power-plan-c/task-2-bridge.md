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

4. **单次 wake 时长压缩（2026-09-22 实测后追加，优先）**
   - 实测 wake 6.2–8.6s，瓶颈在桥侧：扫描器冷启动、connect 首败重试、GATT discovery；
   - 常驻 adapter + 持续（或窗口前）扫描，避免每次 `Pusher::adapter()`/`start_scan` 冷启动；
   - 复用同一个 `Peripheral` 对象跨断连（btleplug #182），首次成功后尝试跳过重复
     `discover_services`（#453 说明服务变更才需重发现；失败要有回退重发现）；
   - 已知设备缓存 INFO（MAC/`rendezvous_v`），减少一次特征读取；
   - ACK 轮询 80ms → 20ms；connect 失败在同一窗口内重试（已做）；
   - 目标：无数据周期端到端 ≤3s；不达标则进入 task-5。

5. **窗口预测（候选，配合 task-5）**
   - 收到设备苏醒广播的 `next_wake_in_s`（或按 arrival 时间滑动平均）后，只在预测窗口前
     guard（覆盖 watcher 冷启动 0.5–2s）开启扫描/连接，替代 250ms 连续扫描；
   - 连续两次错过 → 退回短时连续扫描重捕获；
   - 需过滤设备 thin 时钟唤醒（无 `next_wake` 对齐时会白扫）。

## 验收

- 桥日志：每 60s ≤1 次 BLE 连接尝试/成功；
- 3s 窗口实测命中：连续 30 个周期连接成功率 ≥95%（实测记录）；
- 回复字段包含 `server_time`/`tz_offset_min`（单测或抓包，已含）；
- 单次 wake 分阶段计时（扫描/connect/discover/info/命令），无数据周期 ≤3s；
- `cargo test --workspace`、`cargo fmt`（新增/修改文件）、`git diff --check`。

## 风险（btleplug/Windows）

- #301：Win11 `connect()` 可能长时间不返回 → 必须有超时+重试，不能无限等待；
- #360：`discover_services()` 曾吞错导致重连死锁（0.11 已修）→ 跳过 discovery 的优化必须带失败回退；
- #155/#182：Windows 读值是缓存语义，Peripheral 对象跨断连保留 → 复用对象可行，但状态要显式管理；
- 服务/特征表变更后（GATT 缓存）必须允许一次完整重发现。
