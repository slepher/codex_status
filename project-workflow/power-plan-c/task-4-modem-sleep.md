# Task 4 — 第二阶段：BT modem sleep + DFS 实测定值

Status: pending（第一/二阶段可独立验收；本任务决定 3s 窗口的能量收益）

## 背景

- 现状 sdkconfig：`# CONFIG_BT_CTRL_MODEM_SLEEP is not set`、`RTC_CLK_SRC_INT_RC`；
- 无省电时窗口均值 30–80mA；开 modem sleep+DFS（无 light sleep）后广播 800ms ≈16.4mA（esp-idf #947 Espressif 实测）；
- light sleep 档（≈4.2mA 广播 / 1.8mA 连接）需要准确的 32k 时钟；本板晶振挂在 PCF85063 上、未证实接到 ESP32 XTAL_32K_P/N，暂按不可用。

## 改动

1. `platformio.ini` custom_sdkconfig 增加：
   - `CONFIG_BT_CTRL_MODEM_SLEEP=y`（名称以当前 IDF 版本 Kconfig 为准，构建后核对生成的 sdkconfig）；
   - 保持 `CONFIG_PM_ENABLE/DFS_INIT_AUTO`；
2. `src/main.cpp:363` min_freq 80MHz（与 task-1 同）；
3. 若固件在 BLE 会话期间持有 PM 锁导致降频不生效，按 IDF 文档确认锁行为，不绕过安全语义。

## 验收与回归

- 实机实测：窗口期平均电流（PPK2/Joulescope 或按 pmstats 口径估算）与广播/连接稳定性；
- 回归：BLE 连接成功率、`reserved` 计划处理、OTA、Wi-Fi light 会话不受影响；
- 若出现连接不稳（社区已知 40MHz min 会掉线），回退 min=80 并记录；
- 结果决定：3s 窗口保留或回调至 1.5s（能量差 ~10mAh/天）。

## 风险

- BT modem sleep 与 Wi-Fi/BLE 共存、与 DFS 组合存在历史 bug（esp-idf #947/#15891），需整机回归；
- 无 32k 晶振时勿尝试 light sleep + BLE。
