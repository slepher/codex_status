# task-7 — M3 正身：自编 core + PM light sleep（T10 ≤2mA）· 新窗口执行准备

Status: prepared 2026-09-17 by the previous window; not started.
Scope: firmware only (new PlatformIO env + NimBLE 2.x 迁移 + PM/GPIO 策略), bridge 不动。
Goal: 让 LIVE 平均电流 ≤2mA（T10），并保持 T9/T11 与深睡/按键/OTA 全部回归通过。
Spec: `docs/history/sleep-plan-v4.md` §4.3/§6 M3；`architecture.md` §5 执行顺序；`task-6.md`（0.10.3 现状）。

## 0. 先读什么

1. `PROGRESS.md` 最新一节（2026-09-17 0.10.3）与 `project-workflow/sleep-battery/task-6.md`。
2. 本文档。设备/桥现场：设备 `192.168.1.50`、SSID `home-wifi`、BLE `CodexStatus-AABBCC`；
   桥用 `pwsh tools/start-bridge.ps1` 起（后台+tools 脚本，不会卡会话），PID 记 `artifacts/bridge-app-run.pid`。
3. OTA token 已持久化在设备 NVS：用 `python tools/device-auth/request_token.py`（需 BLE 窗口：BOOT 2s 或串口 CLI `pair`）取用；
   **不要把 token 写进任何文件/提交**。

## 1. 出发状态（0.10.3 + 实测结论）

- 固件 `0.10.3-bw`（ota_1）：ext1 BOOT/PWR 唤醒、timer 唤醒、PWR 长按软关机、token NVS、`/sleep`、`wake`/`pwr` 状态字段均已验证。
- LIVE 现形态 = 原厂 core，仅 `WiFi.setSleep(true)`（modem sleep）：实测约 **10%/h（数十 mA）**，T10 不达标（预期内）。
- T9 已验证：直推 `POST /usage` 0.95s 完成渲染；桥 3s 指纹检查（信封刷新→推送实测 2.0s）。
- 后台电池采样器在跑：`artifacts/battery-log.csv`（每 5min 一行）。
- 相关提交：`7dc0a2c`（0.10.3 加固）、`207a72b`（推送指纹）、`b786468`（启动免扫 BLE）、`b18679c`（start-bridge.ps1）。
- ROM：`artifacts/codex-status-0.10.3-bw.bin` SHA256 `C0AABDC117D88D8FB073306C27F0BAD5DB7967600A1ACFAF019B7EFE5780B61D`。

## 2. 构建 spike（第一件事，先证明能编译/能跑）

**原则：不动现有 env，新增一个并行 env；失败可一键回退。**

`platformio.ini` 追加（现有 `[env:esp32-s3-epaper-154g]` 保持不变）：

```ini
[env:esp32-s3-epaper-154g-pm]
platform = https://github.com/pioarduino/platform-espressif32/releases/download/stable/platform-espressif32.zip
board = esp32-s3-devkitc-1
framework = arduino
upload_speed = 921600
monitor_speed = 115200
board_build.flash_size = 8MB
board_build.arduino.memory_type = qio_opi
board_build.partitions = partitions.csv
board_build.filesystem = littlefs
lib_deps =
  h2zero/NimBLE-Arduino@^2.5.1
  bblanchon/ArduinoJson@^7.0.4
build_flags =
  -DARDUINO_USB_MODE=1
  -DARDUINO_USB_CDC_ON_BOOT=1
  -DCORE_DEBUG_LEVEL=1
  -DCONFIG_BT_NIMBLE_HOST_TASK_STACK_SIZE=8192
custom_sdkconfig =
  CONFIG_PM_ENABLE=y
  CONFIG_FREERTOS_USE_TICKLESS_IDLE=y
  CONFIG_FREERTOS_HZ=1000
  CONFIG_PM_DFS_INIT_AUTO=y
  CONFIG_PM_POWER_DOWN_CPU_IN_LIGHT_SLEEP=y
  CONFIG_PM_RESTORE_CACHE_TAGMEM_AFTER_LIGHT_SLEEP=y
  CONFIG_PM_SLP_IRAM_OPT=y
  CONFIG_PM_RTOS_IDLE_OPT=y
  CONFIG_PM_SLP_DISABLE_GPIO=y
  CONFIG_USJ_NO_AUTO_LS_ON_CONNECTION=y
  CONFIG_ESP_WIFI_SLP_IRAM_OPT=y
```

- `custom_sdkconfig` 会触发 framework 重装 + Arduino IDF 库重编（首次几十分钟），改任一选项都会重来；
  需要 Windows 已开启长路径支持（否则 pioarduino 会警告）。
- 生成物 `sdkconfig.defaults` / `sdkconfig.esp32-s3-epaper-154g-pm` 会出现在仓库根，**加入 .gitignore**，不要提交。
- 构建：`pio run -e esp32-s3-epaper-154g-pm`。
- 备选路线（本 spike 失败才走）：Arduino-as-IDF-component 或 esp-nimble-cpp；不要同时开 `framework=arduino,espidf`（会关掉 hybrid compile）。

## 3. 固件代码工作清单

1. **PM 配置**（LIVE 进入时）：`esp_pm_configure({max_freq_mhz=240, min_freq_mhz=40, light_sleep_enable=true})`，
   记录返回值（`ESP_ERR_NOT_SUPPORTED` = tickless 没开成）。退出 LIVE/深睡路径不需要停 PM。
2. **Wi-Fi PS**：先保持 `WiFi.setSleep(true)`（MIN_MODEM）测；若 AP DTIM=1 导致 ~2.45mA，改
   `esp_wifi_set_ps(WIFI_PS_MAX_MODEM)` + `listen_interval=3`（预期 ~1.33mA；入站推送延迟 ~300ms，T9 仍够）。
3. **GPIO 睡眠策略**（`PM_SLP_DISABLE_GPIO=y` 下）：
   - 必须保留：GPIO17（锁存，HIGH）、GPIO6（面板使能，HIGH=断电）、GPIO42（音频电源，LOW）→
     `gpio_sleep_sel_dis()`；若开 CPU 掉电仍丢电平，加 `gpio_hold_en()`（唤醒后 `gpio_hold_dis()` 再改电平）。
   - BOOT/PWR（0/18）：若希望 LIVE 光睡中按键立即响应，`gpio_sleep_set_pull_mode(UP)` + `gpio_wakeup_enable(LOW_LEVEL)`
     + `esp_sleep_enable_gpio_wakeup()`；不做也不阻塞验收。
   - 其余（EPD SPI 8–13、ADC4）可放开。验证：`gpio_dump_io_configuration()` 看 `SleepSelEn`。
4. **BLE 2.x 迁移**（`src/ble_bridge.cpp`，1.4.3 → 2.5.x）：
   - `onRead(c)` / `onRead(c, desc)` → `onRead(NimBLECharacteristic*, NimBLEConnInfo&)`（合并实现）。
   - `onConnect(server, desc)` → `onConnect(NimBLEServer*, NimBLEConnInfo&)`；`desc->conn_handle`→`connInfo.getConnHandle()`；
     bonded/encrypted 用 `connInfo.isBonded()/isEncrypted()`；地址 `connInfo.getAddress()`。
   - `onDisconnect(server, desc)` → `onDisconnect(NimBLEServer*, NimBLEConnInfo&, int reason)`。
   - `onAuthenticationComplete(desc)` → `onAuthenticationComplete(NimBLEConnInfo&)`。
   - `onWrite(c, desc)`（5 个回调）→ `onWrite(NimBLECharacteristic*, NimBLEConnInfo&)`；`writeAllowed()` 改查 `connInfo`。
   - 广播：`setScanResponse(true)` → `enableScanResponse(true)`；**必须 `adv->setName(deviceName)`**（2.x 默认不播名字，
     否则桥/Python 按 `CodexStatus-` 前缀扫不到）。`NimBLEDevice::startAdvertising()/stopAdvertising()` 保留。
   - LIVE 仍要 deinit BLE（现状即可）；DEEP 窗口逻辑不变。
5. **深睡路径保持**：`armWakeSources()`/ext1/定时唤醒/`deepSleepFor()` 不动；确认 PM 不会干扰 deep sleep。
6. **看门狗回归**：LIVE 下 Wi-Fi 连接、`POST /usage`、模板拉取、OTA、`/status.json` 在光睡下都要复测。

## 4. 已知坑清单（研究结论，按优先级）

1. `custom_sdkconfig` 改一次=重装框架+重编库；先小步改、集中验证。
2. 只开 `CONFIG_PM_ENABLE` 不够，必须 `esp_pm_configure(light_sleep_enable=true)`；tickless 未开时返回不支持。
3. 官方 S3 电流：auto light sleep DTIM1 2.45mA / DTIM3 1.33 / DTIM10 0.93；modem sleep ~19–40mA。DTIM 由 AP 决定。
4. `PM_SLP_DISABLE_GPIO=y` 把全部 GPIO 在睡眠时切高阻 → GPIO17 锁存可能释放（掉电）、GPIO6 可能误开面板；按 §3.3 豁免。
5. `PM_POWER_DOWN_PERIPHERAL_IN_LIGHT_SLEEP` 实验性：UART FIFO flush 可能阻塞入睡→tick 崩溃，先别开。
6. PSRAM（qio_opi）与 `CONFIG_ESP_SLEEP_POWER_DOWN_FLASH` 共享电源，别开 flash 掉电。
7. 光睡会 gate USB-Serial/JTAG：主机可能报掉线且不自动重枚举；`USJ_NO_AUTO_LS_ON_CONNECTION=y` 让插 USB 时不自动光睡（开发期）。
8. 自动光睡内部占用 timer 唤醒源：不要在 PM 活性期间手动配 timer/手动 `esp_light_sleep_start`。
9. PM 会降低 tick/中断精度（Kconfig 明示），计时类逻辑留余量。
10. `FREERTOS_SMP` 默认 n，保持默认（PM_ENABLE 依赖 `!FREERTOS_SMP`）。
11. BLE 睡眠时钟精度要求 ≤500ppm；主 XTAL 常开费电、外置 32k 才省——LIVE 里 BLE deinit，可规避。
12. ArduinoOTA 在 core 3.x 改 SHA256+PBKDF2 挑战应答，旧 espota 可能不兼容；我们主用网页 `/doUpdate`，不受影响。

## 5. 验收与测量

- **T10**：串在电池回路的电流表（JST 1.25）平均电流 ≤2mA；没有表则用 `artifacts/battery-log.csv` 的
  `battery_mv` 斜率（≥2h，对比 0.10.3 基线 ~10%/h）做上界判断，并在结论里标注"估算"。
- **T9**：直推 `POST /usage`（Bearer endpoint token）→ `epd_writes` 增长；桥日志 3s 内推送。
- **T11 回归**：无 token `/doUpdate` 401；bond 保留；`/status.json`；OTA（网页）。
- **深睡回归**：`/sleep` 定时唤醒、BOOT/PWR ext1 唤醒、PWR 长按软关机。
- **按键**：LIVE 光睡下 BOOT 短按换模板、BOOT 2s 配对窗口、PWR 长按关机仍工作。

## 6. 回退与提交

- 新 env 失败不影响原 env：`pio run -e esp32-s3-epaper-154g` 仍可构建旧固件；ROM 0.10.3 已归档，可随时 OTA 回退。
- 提交信息英文祈使句、沿用现有风格；**未经用户同意不要提交**；每个里程碑后更新 `PROGRESS.md` 与
  `project-workflow/sleep-battery/status.md`，ROM 归档 `artifacts/codex-status-<ver>-bw.bin` + SHA256。
- 后台进程用 `pwsh tools/start-bridge.ps1`，停止用 PID 文件；不要擅自停桥/设备服务。
- 不提交任何密钥（Wi-Fi 密码、BLE/OTA token）；不把 token 写进文档或日志。

## 7. 执行记录（2026-09-17 晚，task-7 进行中）

### 7.1 已落地（未提交）

- `platformio.ini`：新增 `[env:esp32-s3-epaper-154g-pm]`，与本文件 §2 基本一致，差异：
  - build_flags 增 `-DCODEX_PM=1`（同时把 `FW_VERSION` 切到 `0.11.0-bw`）与
    `-DCODEX_NIMBLE_V2=1`（双 env 共用源码的编译开关）；
  - `CONFIG_BT_NIMBLE_HOST_TASK_STACK_SIZE=8192` 从 build_flags 挪进 custom_sdkconfig：
    重编 core 时 sdkconfig.h 会定义 5120，命令行 -D 会被重定义（且不生效），放进 sdkconfig 才真正生效。
- `src/ble_bridge.cpp`：1.x/2.x 双版本（`PeerRef` + `PEER_ARG` 宏），2.x 广播必须
  `adv->setName()` + `enableScanResponse(true)`；2.x 回调签名只保留带 `NimBLEConnInfo&` 的版本。
- `src/main.cpp`：`enterLive()` 调 `esp_pm_configure({240, 40, light_sleep=true})` 并记日志；
  `retainSleepCriticalGpio()` 在 `epdBegin()` 里对 GPIO17/6/42 `gpio_sleep_sel_dis()`；
  `/status.json` 增 `pm_light_sleep`；stock env 保持 0.10.3 行为（版本号不变、PM 不编译）。
- `.gitignore`：sdkconfig*、`CMakeLists.txt`、`.dummy/`、`dependencies.lock`、`managed_components/`。
- 回归：`pio run -e esp32-s3-epaper-154g` SUCCESS（337 s），原 env 未被破坏。

### 7.2 构建环境的坑（新增，必须遵守）

1. **项目路径带空格被 IDF 拒绝**：`espidf.py:2629` 显式检查 `FRAMEWORK_DIR/BUILD_DIR`；
   `...\codex status` 直接 `Error: Detected a whitespace character in project paths.`。
   绕法：junction `C:\Users\user\AppData\Local\Temp\opencode\codex-status` → 仓库根，
   并让 pio 进程的**真实 cwd** 落在 junction 上（`System.Diagnostics.ProcessStartInfo` 分离启动）。
   Shell 的 `workdir`/`Set-Location` 会被解析回真实路径 → 仍报错。
2. **盘符根/subst 不行**：`os.path.basename("P:\\")` 为空，pioarduino 生成的
   `project(<basename>)`（`espidf.py` `create_default_project_files`）变成 `project()` → CMake 报
   “Macro invoked with incorrect arguments”。必须用有名字的 junction 目录。
3. **不要用 `-v`**：GBK 控制台下 PlatformIO 输出线程会 `UnicodeEncodeError` 后挂住构建
   （日志停在 traceback、无编译器进程），非 verbose 输出正常。
4. SCons 是按需装到 `~/.platformio/packages/tool-scons/scons-local-4.11.1`；安装窗口内可能报
   `No module named 'SCons.Tool.FortranCommon'`，重跑即可。
5. `custom_sdkconfig` 首次构建会在 core 重编（几十分钟），本窗口尚未跑完；日志
   `artifacts/pm-build.log`。

### 7.3 现场与待办

- 设备仍是 **0.10.3-bw（ota_1）LIVE**；桥 `bridge-app` PID 43768 正常（勿停）。
- 电池 ~30% / 3.59 V 且 LIVE 掉电明显，PM 落地越早越好；回退 ROM
  `artifacts/codex-status-0.10.3-bw.bin` 未动。
- 下一步：非 verbose 重跑 pm 构建 → USB/OTA 烧 `0.11.0-bw` → 冒烟（`/status.json` 看
  `pm_light_sleep`、BLE 重配/被扫、HTTP push）→ T10 电池斜率（标注估算）、T9/T11、深睡/按键/OTA 回归。

### 7.4 执行记录（2026-09-18 凌晨，0.11.1 → 0.11.9 收尾）

- **构建环境定案**：仓库改名去掉空格后直接在仓库目录构建；`platform_packages` 钉
  `tool-scons@4.11.1`；把 pioarduino 平台 `platform.json` 里 tool-scons 的 `package-version`
  改为 `4.41101.0`（否则平台版本检查删包，SCons 报 `No module named 'SCons.Tool.FortranCommon'`；
  备份 `artifacts/platform-espressif32-platform.json.bak`）。`custom_sdkconfig` 改动或中断后
  需删 `sdkconfig.defaults` 以强制整套 libs 重编。
- **双 env 撤销**：`platformio.ini` 只保留 `esp32-s3-epaper-154g`（pioarduino/PM/NimBLE 2.x），
  stock env 退役；`-DCODEX_PM=1 -DCODEX_NIMBLE_V2=1` 常开。
- **闪存 40MHz（本板必须）**：GD25Q64 在 80MHz 下 ID 读错（`0B20E4`）、页编程超时，
  导致 NVS/LittleFS 写入失败（`restore cache fail`、`nvs_open NOT_FOUND/INVALID_STATE`）。
  设 `board_build.f_flash = 40000000L`，并新增 `tools/bootloader_40m_fix.py`（pre 脚本，
  pioarduino 只有 80m/120m 预编译 bootloader，缺 40m ELF 时用 80m ELF 补位；频率由
  elf2image 按配置写入镜像头）。修复后 flash ID `00c84017`、直写/回读校验通过。
- **固件修正**：0.11.3 `macSuffix()` 改 `esp_read_mac()`（新 core 下 Wi-Fi 未初始化时
  `WiFi.macAddress()` 返回全 0）；0.11.6 `enterLive()` 改 `WIFI_PS_MAX_MODEM` +
  `listen_interval=10`（提前 re-associate 使关联请求携带）；0.11.8 广播去掉 128 位服务 UUID
  （NimBLE 2.x 31 字节放不下，`Data length exceeded`）；0.11.9 `handleDisconnect()` 重开广播；
  诊断字段 `wifi_slots`/`ap_reason`；CLI `stay`（USB 调试不睡）。
- **验收**：0.11.9-bw LIVE，`/status.json` `pm_light_sleep=true`、`live=true`、`channel=PUSH`、
  `endpoints=1`、quad active，桥 HTTP push 200（PM 生效后）。BLE 重新配对 + endpoint/模板回推成功。
  ROM `artifacts/codex-status-0.11.9-bw.bin` SHA256
  `DC5CCCAA388D112BDC9051FCEA1B4995C71C95CED3735DDFAFC09D70540BCEBF`。
- **遗留**：T10 电池斜率（需退出 `stay` 并拔 USB，正常 DEEP/LIVE 循环 ≥2h）；T9/T11 正式回归
  （无 token 401、OTA）；深睡/ext1/按键回归；`bootArmed` 使"按住 BOOT 从深睡唤醒"不再直接开
  配对窗口（需先唤醒再按 2s）。
