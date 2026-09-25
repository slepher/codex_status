# task-3 — 0.13.8：1Hz 合并 tick + Wi-Fi 活跃窗口调优

## 背景

- 0.13.7 修复时钟 per-loop 回归后，设备 SLEEP ~87–91%、wake ~40/s。
- 0.13.7 后本机 A/B（token 门控 `POST /diag?loop_delay=N`，60s/档，25→500ms）：
  wake 39.5→8.3/s，但 SLEEP 恒 ~88–91%、每分钟清醒 ~5.1s —— loop 不再是瓶颈。
- pmstats 分解：wifi PM 锁 ~48.8ms/次 × ~1.15/s ≈ 3.4s/min（约 2/3 清醒预算），
  对应 IDF Kconfig `ESP_WIFI_SLP_DEFAULT_MIN_ACTIVE_TIME=50ms`（默认值）；
  其余 ~1.7s/min 为 lwIP/esp_timer 后台（对照 esp-idf #18029）。
- 代码审计残留同类 per-loop 调用：离线分钟块 `timeKnown()`/`time()`
  （`src/main.cpp:2354`，0.13.7 漏改）；`pollWifi()`/`serviceAnnounce()`
  每轮各一次 `WiFi.localIP()`。

## 改动

### F1（Kconfig，`platformio.ini` custom_sdkconfig）

- `CONFIG_ESP_WIFI_SLP_DEFAULT_MIN_ACTIVE_TIME=20`（50→20，range 8..60）
- `CONFIG_ESP_WIFI_SLP_DEFAULT_WAIT_BROADCAST_DATA_TIME=10`（15→10，range 10..30）
- `CONFIG_ESP_WIFI_SLP_DEFAULT_MAX_ACTIVE_TIME=60`（10→60，单位秒）

生效前提：kconfgen 对已有 sdkconfig 的值优先于 defaults，需删除生成的
`sdkconfig.esp32-s3-epaper-154g` 让其按 defaults 重生成（首次会重编 core）。

### F2（`src/main.cpp`）

- loop 尾部：1Hz 合并 tick，一次 `time()` 同时驱动 offlineMinute 与
  `device.now` 分钟重绘；移除两处独立 per-loop 检查。
- `serviceAnnounce()`：IP 变化检查 1Hz 门控；删除 `pollWifi()` 中重复的
  per-loop `WiFi.localIP()` 比较。
- `FW_VERSION` → `0.13.8-bw`。

## 预期与验证

- 预期：wifi 锁时间 3.4→~1.4s/min，SLEEP ~91%→~94–95%（loop 25ms）。
- 验证：`pio run`；MCP `firmware_ota`；OTA 后 60s `/pmstats` 与基线对比
  （25ms：SLEEP 91.0%、39.5 wakes/s、awake/wake 2.27ms）；桥 push 延迟/成功
  率回归；`/status.json`、EPD 重绘正常。
- 回退：`artifacts/codex-status-0.13.7-bw.bin`。

## 状态

- [x] 代码/配置（`platformio.ini` 三 Kconfig；`main.cpp` 1Hz tick + localIP 门控；删生成 sdkconfig 后构建）
- [x] 构建 + ROM 归档（`artifacts/codex-status-0.13.8-bw.bin`，SHA256 `2E5CF982…1C5A`；`pio run` SUCCESS）
- [x] OTA + A/B（MCP `firmware_ota` 0.13.7→0.13.8；稳态 SLEEP 92.3–93.3%、wifi 锁 24–32ms/次（原 48.8ms）、awake/wake 1.6–1.9ms（原 2.27ms）；桥 push 200 OK）
- [x] PROGRESS / docs 更新（PROGRESS 最新状态；`docs/power-state.md` §10）
