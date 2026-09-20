# sleep-modes — v0.14 deep/light 双模式 + 时钟区域直写（实现计划）

依据：`docs/power-state.md` §13（权威设计，2026-09-20 定稿）。
证据：`artifacts/clkwin-ab-log.txt`（A/B 实测）、
`project-workflow/deep-pull-test/{plan,results}.md`（deep 反向拉取实测）。

## 目标

1. 固件支持 `deep` / `light` 双模式，默认 `deep`；桥在 pull 响应里决定模式。
2. deep 下每分钟只做**时钟区域直写**（不联网）；按 `next_contact_s` 定时
   反向拉取 `GET /usage`；离线按 1m×3→5m×3→15m 拉长。
3. light 下维持现行行为；无变化 X 分钟后设备自行回 deep 并 HTTP 通知桥。
4. 手动：deep 下 BOOT 单击 → light；再次单击 → BLE；不常驻。
5. 桥新增 `usage_rev`/`last_change`/`mode`/`next_contact_s`/`pending`；
   deep 期间推送失败不计告警；OTA/模板排队。

## 非目标

- 不改模板协议；不改 claim/lease 协议本身（只调整 deep 语义）。
- 不启用 PCF85063（列为后续项；先用 `esp_rtc_get_time_us()`）。

## 现状（2026-09-20 晚）

- 设备 `0.13.8-bw`（ota_1，next ota_0，light sleep 在线，电量 ~80%）；
  回滚 ROM `artifacts/codex-status-0.13.8-bw.bin`
  SHA256 `2E5CF9826C94E6551B6C5C284AE248480035C68EC3F664A0D7A4F5D091FF1C5A`。
- 桥 debug 父 PID 42344 + watchdog 33344（`tools/start-bridge.ps1`）。
- 已写未启用代码（不撤）：`bridge/crates/core/src/codex.rs`（rate-limit
  通知匹配 + `Notify`）、`runtime.rs`（通知即刷 + 3min 兜底）、
  `app/config.rs`/`core/main.rs`/`tools/test-bridge/bridge.py` 默认 180s。
- `src/main.cpp` 内测试代码（`#ifdef CODEX_DEEPPULL_TEST` /
  `CODEX_CLK_WINDOW_TEST`）保留，默认构建零影响；`EPD_SSD1681` 已有
  `WakePartialWindow`/`DisplayPartWindow`（窗口 60B 实测可用）。

## 任务

- **T1 固件·时钟直写与瘦唤醒**
  - 激活模板加载时计算并缓存时钟 rect（模板无 `device.now` → 不预留）；
  - 深睡 TIMER 唤醒走 `clockTickWake()`：恢复时间（`esp_rtc_get_time_us`
    差分）→ 60B 缓冲 → `WakePartialWindow`/`DisplayPartWindow` → 睡；
    不联网、不读模板、不动 BLE/HTTP；
  - 验收：与 A/B 同期数据（B≈796ms）同量级；连续 30min 无误刷/残影可控。
- **T2 固件·模式状态机**
  - `mode`（NVS+RTC）：deep/light；手动覆盖（BOOT 单击→light、再击 BLE）；
  - light 内"无变化 X 分钟"→ `POST /deep {next_contact_s}` → 深睡；通知失败
    照睡；插电/低电/BLE/OTA 优先级按 §13.1。
- **T3 固件·网络周期与离线重试**
  - 按 `next_contact_s` 醒来快连（缓存 BSSID/信道，不扫描）→ `GET /usage`
    （Bearer）→ 执行响应 `mode/next_contact_s/pending`；失败按
    1m×3→5m×3→15m；成功校时（epoch+`server_time`）。
- **T4 桥·决策与门控**
  - `usage_rev`（指纹变化 +1，剔除 `server_time`）、`last_change_at`、
    迟滞（升 light 最少驻留 5min，静默 10–15min 才降）；
  - pull 响应 `mode/next_contact_s/usage_rev/pending`；推送信封带 `mode`；
  - deep 预期离线：不计 `push_fail_streak`、不告警；announce/pull/claim 视为
    在线；lease 续约/放宽；OTA/模板排队并在 pull 窗口内推（现有
    `POST /doUpdate` 路径不变）。
- **T5 协议/三端**
  - 固件/Rust/Python 测试桥同步（envelope 新字段、默认值与缺省行为）；
  - `docs/power-state.md` §13 回归更新（实现后去掉"待实现"），AGENTS 视需要。
- **T6 验收**
  - 深睡≥2h 回归（时钟准、无残影失控、电量斜率）；升/降级迟滞；手动唤醒；
    BLE/OTA 打断；桥不可达；Wi-Fi 断；token 失效；低电；旧桥缺字段兼容。
- **T7 文档**：PROGRESS 最新节、plan/task 状态、测量数据归档 artifacts。

## 验收判据（摘要）

- deep 安静期平均电流目标 ≤2.5mA（时钟周期为主），网络周期按桥自适应；
- 升 light 延迟 ≤ 一个 `next_contact_s`（有网 ≤1min）；降 deep 按迟滞；
- A/B 视觉：时钟窗口更新仅动该区域；OTA 全流程在 light 或 pull 窗口可用；
- 三端哈希/协议测试全过，`git diff --check` 干净（不提交）。

## 构建 / OTA / 回滚

```powershell
# 测试 flag（环境变量，不改 platformio.ini）：
$env:PLATFORMIO_BUILD_FLAGS = "-DCODEX_DEEPPULL_TEST=1"   # 或 CLK_WINDOW_TEST
pio run
# OTA（桥 MCP）：
curl.exe -s -X POST http://127.0.0.1:8766/mcp -H "Content-Type: application/json" `
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"firmware_ota","arguments":{"rom":"<绝对路径>"}}}'
# 回滚 ROM：artifacts/codex-status-0.13.8-bw.bin
```

## 风险

1. 面板 mode1+reset 后 RAM 保持（单轮通过，长期观察）；
2. 窗口局刷残影累积 → 全刷策略；
3. RTC 8KB 容量与 RTC/NVS 写入磨损；
4. `CONFIG_RTC_CLK_SRC_INT_RC` 漂移（可选 EXT_CRYS/PCF85063）；
5. 同版本 OTA 校验超时（先 bump `FW_VERSION`）；
6. 板级深睡底流未测（功耗模型最大不确定度）。
