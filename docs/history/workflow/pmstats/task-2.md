# task-2 — 微睡眠调研（light_sleep_counts ~125 次/s）

状态：根因已定位（2026-09-19 晚）；修复决策待定（见「下一步」）

## 现象

`/pmstats`（0.13.0-bw）：SLEEP ≈71–80%、`light_sleep_counts` 100–140 次/s、
平均每次 ~4.9ms；`light_sleep_reject_counts` 个位数。

## 根因（结论）

**不是异常唤醒源，是应用自己的轮询节奏**：`loop()` 尾部 `delay(5)`（Arduino
`delay` = `vTaskDelay(ms / portTICK_PERIOD_MS)`，HZ=1000 → 5 个 tick），每个
loop 迭代进一次自动 light sleep 并睡到下一个 RTOS tick，于是统计上呈现
“~5ms 一次微睡眠、100+ 次/s”。loopTask 自身 CPU 约 1–2%（每迭代 ~0.1ms），
代价主要是每次唤醒的固定开销（CPU_MAX 18–19%）。

### 证据

1. **定时器 dump**（0.13.1-bw，`CONFIG_ESP_TIMER_PROFILING` + `/pmstats?timers=1`）：
   没有 ~6ms 周期的 esp_timer；频率最高的 `mdns_timer` 100ms（10/s），
   ETSTimer 一次性重挂 2–3/s。`esp_timer` 不是微睡眠源。
2. **睡眠直方图 + 唤醒源**（0.13.2-bw，`CONFIG_PM_LIGHT_SLEEP_CALLBACKS` +
   `/pmstats?diag=1`）：实际睡眠（`slept_us>0` 且与 `light_sleep_counts` 相等）
   主导桶 **4–6ms**；wake_cause `timer` ≈ 全部，`wifi` ≈ 0.8–1.2/s
   （beacon，`listen_interval=10`），`gpio`=0。桶 `<1ms` 是**另一核的
   `vApplicationSleep` 未达阈值（3 tick）的尝试**，`slept_us=0`，未实际入睡。
3. **A/B（runtime 旋钮）**：0.13.2 新增 token 门控
   `POST /diag?loop_delay=N`（仅 RAM，重启回 5），60s 窗口实测：

   | loop_delay | 入睡次数 | SLEEP% | CPU_MAX% | `/status.json` 往返（PowerShell 5 次均值） |
   |---|---|---|---|---|
   | 5ms（默认） | 114–134/s | 71–80 | 18–19 | ~100ms（工具开销占大头） |
   | 10ms | 88/s | 86 | 12 | — |
   | 25ms | 45/s | 90 | — | — |
   | 50ms | 29.5/s | 90 | 7 | — |
   | 100ms | 22/s | 88 | 7 | ~460ms |
   | 200ms | 19/s | 91 | 6 | ~990ms |

   收益在 ~50ms 后基本饱和：100/200ms 只再省几 %，因为 `mdns_timer`
   （100ms，10/s）+ Wi-Fi beacon（~1/s）成了新的唤醒下限；而每个 HTTP 事务要
   多次 loop 才处理完（WebServer 在 `handleClient` 里逐段读），延迟成倍增长
   （请求/头/体各等一个 loop）。200ms 下 BOOT 快速单击还可能整个落在两次采样
   之间被漏掉，OTA 吞吐也会塌（MCP 180s 超时风险）。故默认不宜超过 ~25–50ms，
   若要兼顾 OTA/交互应做动态快路径（有 TCP 客户端/OTA 时回 5ms）。
4. **AP/DTIM 检查（原步骤②）**：wifi 唤醒只有 ~1/s，与
   `listen_interval=10` × beacon 100ms 吻合（MAX_MODEM 不看 DTIM）；不是微
   睡眠源，无需进路由器后台。
5. **`FREERTOS_HZ=100` 实验（原步骤③）已排除**：Arduino `delay()` 用
   `ms/portTICK_PERIOD_MS`，HZ=100 时 `delay(5)` → `vTaskDelay(0)`（仅
   yield）→ loop 忙轮询、idle 永不入睡，只会更糟。IDF 对 S3 也推荐 HZ=1000。
   `IDLE_TIME_BEFORE_SLEEP 3→5/10` 只影响进睡门槛，与已定位根因无关，暂缓。

## 已产出的固件

- **0.13.1-bw**：`GET /pmstats?timers=1`（`esp_timer_dump`，需
  `CONFIG_ESP_TIMER_PROFILING=y`）；CLI `timers`。
  ROM `artifacts/codex-status-0.13.1-bw.bin`
  SHA256 `81B39D9E8EE4F4AD5D50808D44E8D9214776A04343253B1FB41EFEDF7F13C485`。
- **0.13.2-bw**：睡眠诊断（`CONFIG_PM_LIGHT_SLEEP_CALLBACKS`：次数/均值/
  min/max/唤醒源/睡眠时长直方图）、`GET /pmstats?diag=1`（+FreeRTOS 任务与
  run-time dump）、CLI `diag`、token 门控 `POST /diag?loop_delay=N`；
  同时按用户要求：**BOOT 2s 只切模板，不再自动开 BLE**（`docs/power-state.md`
  §4 已更新）。
  ROM `artifacts/codex-status-0.13.2-bw.bin`
  SHA256 `D8D1CEF28FEC31A31CF4A4C8B27535B510BB69CC93052E09D8FFD741BAD15482`。
- **0.13.3-bw（当前运行）**：按用户决定把空闲 `loop_delay` 默认改为 **25ms**，
  `loopDelayForNow()` 在有 TCP 客户端（HTTP 请求/OTA 上传）期间自动回 5ms，
  保证吞吐与交互；`POST /diag?loop_delay=N` 仍可运行时覆盖。实测（OTA 后
  60s）：45.6 次/s、SLEEP 86%、CPU_MAX 9%（对照默认 5ms 的 134 次/s、
  80%、17%）。
  ROM `artifacts/codex-status-0.13.3-bw.bin`
  SHA256 `7BA1171DB0CB6171EF2F8719A3DBA1F92A2F677AD57735438CC2D481DDAA7731`。

## 下一步

1. ~~用户决策默认轮询节奏~~ → 已定：**默认 25ms + 传输时自动 5ms**
   （0.13.3 已 OTA）；后续观察 HTTP/OTA 回归（下次 OTA 全流程）。
2. T10 拔电电池斜率对照（5ms vs 25ms，`battery_mv` 斜率 + `/pmstats`）。
3. 可选：把 0.13.1/0.13.2 的诊断面（`?timers=1`/`?diag=1`/`POST /diag`）
   去留决定（诊断价值高、成本低，倾向保留）。
