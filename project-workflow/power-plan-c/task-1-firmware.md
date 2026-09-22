# Task 1 — 固件：会合窗口、渲染时机、校时、时钟修复

Status: pending（依赖：未提交的 v2 收敛工作树）

## 改动点

1. **修复 v2 会合唤醒时钟停更（正确性问题，优先）**
   - `deepNetDue()` 为真时跳过 `deepThinWake()`（`src/main.cpp:4512`），而无 light 计划的会合路径直接 `sleepToNextEvent()`（`src/main.cpp:4625-4628`），从不渲染时钟；
   - 每次会合后 `rtcNextNetAt = now+60` 且唤醒点对齐整分钟 → `deepThinWake()` 永不执行；
   - 修复：会合唤醒在"回复后或超时后"渲染一次时钟（复用 `clockTickWake` / `clkCaptureFromFramebuffer` 的时钟窗口逻辑）。

2. **窗口与提前关闭**
   - `v2Rendezvous()`（`src/main.cpp:3683-3699`）：deadline `15000` → `3000`；
   - plan ack 后 `500ms` → `150–200ms` 关闭；
   - 无连接/无回复：到 3s 关闭，不重试（下个周期再试）。

3. **渲染时机**
   - 渲染从"窗口前/窗口并行"移到"BLE 关闭后"（回复或超时后）；
   - 有 BLE 小数据更新时：数据 + 时钟同帧渲染一次；
   - 收到 light 计划：时钟并入 light 首帧，不额外画。

4. **校时**
   - 应用 bridge 回复中的 `server_time` + `tz_offset_min`（复用既有 time/tz 路径与 NVS `pm/tz`）；
   - 无回复：本地 RTC 外推；渲染必须使用校时后的值。

5. **广播参数显式化**
   - `src/ble_bridge.cpp:327`：`setMinInterval/setMaxInterval` = 30–60ms（0x30–0x60），不依赖 NimBLE 默认。

6. **DFS**
   - `src/main.cpp:363`：`min_freq_mhz` 40 → 80（BLE 场景；与 flash 40MHz 无关）。

7. **退出路径**
   - 保持现有：关广播 → `bleDeinit` → 释放 PM 锁 → 回 deep；不新增隐式续航。

## 验收

- 实机日志：窗口 ≤3s、连接后 ≤200ms 关闭、每周期连接 ≤1；
- `/status.json`：`awake_ms`/`ble_on_ms`/渲染次数/时间来源（见 task-3）；
- 无数据周期 `light` 次数 = 0；时钟每分钟更新，且 bridge 在线时来自校时；
- 停桥对照：超时用本地时间渲染，回 deep；
- `pio run`（两个 env）；回归 `node tools/test-quad-preview.mjs`、模板哈希不变。

## 风险

- 发现延迟无保证：若 3s 命中率仍不足，先查桥扫描（task-2），再考虑放宽窗口；
- 校时字段缺失时（旧桥）退化为本地时间，不得阻塞渲染；
- 时钟局刷耗时若 >0.8s，需回到时钟窗口优化（另议）。
