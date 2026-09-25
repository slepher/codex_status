# deep-pull-test — 简易测试方案（deep 唤醒 → 快连 → 反向拉取 → 浅睡）

状态：2026-09-20 编写并执行（简易一次性测量，不做产品化）。

## 目标

量测「深睡定时唤醒 → 快连 Wi-Fi → 反向拉取 bridge `GET /usage` → 进入
light sleep」整个流程：

- 各阶段墙钟耗时：`boot_us` / `wifi_ms` / `http_ms` / `parse_ms` / `total_ms`；
- 该流程消耗的 CPU 时间：进浅睡前采样 `/pmstats` 的 Mode stats
  （`CPU_MAX` = 240MHz 忙、`APB_MAX`/`APB_MIN` = 降频运行、`SLEEP` = 等待/睡眠）。

## 方法

测试固件（构建期注入 `-DCODEX_DEEPPULL_TEST=1`，`FW_VERSION=0.13.9-dptest`）：

1. OTA 后首次 boot：正常 `startNormalMode()` 连接一次，把 AP `channel`/`BSSID`/
   slot 写入 RTC（供后续快连）；随后 `deepSleepFor(30)` 启动测试循环。
2. 之后 5 个周期（`DP_TEST_CYCLES`）：RTC timer 唤醒 → **不扫描**、用缓存
   BSSID/信道 `WiFi.begin(ssid, pass, ch, bssid)` → `GET http://<bridge>:8765/usage`
   （`Bearer` 取设备端 `EndpointRec.token`）→ ArduinoJson 解析 → 记录该周期
   耗时与 `pmStatsText()` 到 RTC → 继续深睡 30s。
3. 第 5 周期结束：打印全部周期汇总（RTC 累积，深睡不丢）后返回正常流程，
   设备进入 light sleep 在线态，等待回滚 OTA。
4. 全程只读拉取，不推送、不改模板、不 claim。

## 执行步骤

```powershell
$env:PLATFORMIO_BUILD_FLAGS = "-DCODEX_DEEPPULL_TEST=1"
pio run                                     # 测试 ROM
pwsh tools/start-bridge.ps1                 # bridge 必须在跑（设备要拉它）
# MCP firmware_ota -> 测试 ROM；等 ~3.5 min（5x30s + 初始连接）
curl.exe -s http://192.168.3.163/log        # 读 [dp] 行汇总
# MCP firmware_ota -> artifacts/codex-status-0.13.8-bw.bin（回滚）
curl.exe -s http://192.168.3.163/status.json
```

## 判据

- 5/5 周期 `fast=1` 且 `code=200`（bridge 在线时）；
- 得到 `wifi_ms` / `http_ms` / `parse_ms` / `total_ms` 分布；
- `[dp] pm modes @end` 给出该周期的 CPU 模式时间（CPU_MAX 即 240MHz 忙时）。

## 回滚

- 原 ROM：`artifacts/codex-status-0.13.8-bw.bin`
  SHA256 `2E5CF9826C94E6551B6C5C284AE248480035C68EC3F664A0D7A4F5D091FF1C5A`；
- 回滚走 MCP `firmware_ota`（绝对路径），完成后 `/status.json.fw` 应为 `0.13.8-bw`；
- 测试代码全部在 `#ifdef CODEX_DEEPPULL_TEST` 下，默认构建零影响。

## 风险

- 测试期间 bridge 必须在线；离线时 pull 失败（记录 code），仍能量连接耗时；
- 深睡/浅睡期间 GPIO17/6/42 保留由 `epdBegin()` 里的
  `retainSleepCriticalGpio()` 保证（测试路径会先走 `epdBegin`）；
- 异常兜底：设备最终会停在 light sleep 在线态，可直接 OTA；极端情况用 USB 重刷。
