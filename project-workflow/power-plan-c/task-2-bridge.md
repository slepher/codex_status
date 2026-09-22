# Task 2 — 桥：时间下发、扫描覆盖率、窗口去重

Status: 时间下发、250ms 机会循环及 20ms ACK 已落地；常驻 adapter/缓存优化与独立 A/B 待验证。

## 改动点

1. **回复携带时间**
   - BLE 的 `status`/`plan`（以及 `data`）回复增加 `server_time`（epoch 秒）与 `tz_offset_min`；
   - 数据来源用桥本机 `now` 与本机 UTC 偏移（legacy 信封已有同名字段实现，复用语义）；
   - 字段缺失时设备侧退化为本地 RTC（见 task-1），桥不得阻塞。

2. **扫描覆盖率（Plan C 硬前提）**
   - 启动基线曾为扫 3s → 睡 5s；当前 main.rs 已为 250ms 机会循环，但 connect 内仍获取 adapter/扫描；
   - 改为：连续扫描（PC 市电）或扫描间隔 ≤0.75s；仅在设备会合窗口附近/有工作时连接；
   - 每窗口最多一个有界连接事务，事务内允许一次 connect 首败重试；不得以失败重置窗口总预算；
   - 无工作时也连一次发 sleep plan（让设备提前关窗）。

3. **传输语义**
   - HTTP 交付不得报成 BLE；BLE 交付记录 `transport="ble"`（现状已有，保持）；
   - `ble_cycle` 每窗口最多一个事务（含预算内首败重试）；当前成功后 55s 去重不等于严格窗口去重，需核对失败路径。

4. **单次 wake 时长压缩（2026-09-22 实测后追加，优先）**
   - 实测 wake 6.2–8.6s，瓶颈在桥侧：扫描器冷启动、connect 首败重试、GATT discovery；
   - 常驻 adapter + 持续（或窗口前）扫描，避免每次 `Pusher::adapter()`/`start_scan` 冷启动；
   - 优先验证同一 `Peripheral` 的生命周期复用，但不保持旧连接占用；当前 V2Connection::close
     注释记录 Windows disconnect 会清 GATT 缓存，跳过 disconnect 曾令隔次 connect 失败。
     不能直接跳过 discover_services；只有目标后端实测缓存有效时才启用，失效/服务变化完整重发现；
   - INFO 缓存只加速能力提示，完整 MAC、绑定/加密、认证 status/session_nonce 每会话仍核对；
     `rendezvous_v`/固件/配对变化或读写失败使缓存失效；
   - ACK 轮询 20ms 已在 command()；connect 首败重试已做，不重复计为新优化收益；
   - 目标：无数据周期端到端 ≤3s；不达标则进入 task-5。

5. **窗口预测（候选，配合 task-5）**
   - 收到设备苏醒广播的 `next_wake_in_s`（或按 arrival 时间滑动平均）后，只在预测窗口前
     guard（覆盖 watcher 冷启动 0.5–2s）开启扫描/连接，替代 250ms 连续扫描；
   - 连续两次错过 → 有界连续扫描重捕获（候选阈值）；不能无限延长设备窗口；
   - 需过滤设备 thin 时钟唤醒（无 `next_wake` 对齐时会白扫）。

6. **双策略共用入口（task-5 候选，先 spike）**
   - device_first 维持 PC central → 设备 GATT server；bridge_first 新增 PC 每窗口广播指令及
     StatusBeacon 监听，设备扫描/回复后按指令开 Wi-Fi，不新增 PC GATT server。
   - 无工作也发认证 ACCEPT_SLEEP；OPEN_WIFI 发出即并行等 StatusBeacon 与 HTTP，分别记录。
     回复 WIFI_OPENING 后继续有界重试 HTTP；回复漏收但认证 HTTP 通则正常交付。
   - Publisher/Watcher 同时请求运行是 best-effort；实测不可靠时一次 TX→RX 分时退化，
     重复广播且不高频 Start/Stop。目标硬件不支持时保持 device_first。
   - 会合结果进入现有 platform::cycle/应用服务/coordinator，复用 MAC、owner、状态/正式 plan、
     Data/Bundle 和业务 ACK；等待无线不长占业务锁。控制广播不续 light、不自动 claim/发布。
   - 默认、认证配置 ACK 切换、密钥/重放、deadline 与恢复规则以 task-5 为准；两个策略可独立 A/B，
     保留 task-2 本身的优化，不以新策略替代基线验证。

## 验收

- 桥日志：每会合窗口 ≤1 个 BLE 连接事务，首败重试单列且受同一截止约束；
- 3s 窗口实测命中：连续 30 个周期连接成功率 ≥95%（实测记录）；
- 回复字段包含 `server_time`/`tz_offset_min`（单测或抓包，已含）；
- 单次 wake 分阶段计时（扫描/connect/discover/info/命令），无数据周期 ≤3s；
- `cargo test --workspace`、`cargo fmt`（新增/修改文件）、`git diff --check`。

## 风险（btleplug/Windows）

- #301：Win11 `connect()` 可能长时间不返回 → 必须有超时+重试，不能无限等待；
- #360：`discover_services()` 曾吞错导致重连死锁（0.11 已修）→ 跳过 discovery 的优化必须带失败回退；
- #155/#182 是缓存/生命周期调研线索，不是当前后端跨断连免 discovery 的证据；以源码现场注释和复测为准；
- 服务/特征表变更后（GATT 缓存）必须允许一次完整重发现。
