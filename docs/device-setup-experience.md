# ESP32-S3-ePaper-1.54G 设备设置与调试经验

- 适用设备：Waveshare ESP32-S3-ePaper-1.54G（四色屏，SKU 34586 电池版；-EN 无电池版同固件）
- 记录时间：2026-09-11/12（固件 0.1.0 → 0.3.0 全过程）
- 相关文档：`PROGRESS.md`（当前进度）、`docs/history/request.md`（需求）
- 新设备（1.54 黑白版 SSD1681）差异见文末附录

---

## 1. 硬件速查

| 项 | 值 |
|---|---|
| SoC | ESP32-S3-PICO-1-N8R8（8MB Flash + 8MB PSRAM，Wi-Fi + BLE5） |
| 屏幕 | 1.54" 200×200 四色（黑/白/红/黄），2bpp，帧缓冲 10,000 字节 |
| 屏幕控制器 | JD79667 类（无局部刷新，全刷 15–20s，快刷 ~15s） |
| EPD 引脚 | PWR=GPIO6（低有效使能）, BUSY=8, RST=9, DC=10, CS=11, SCLK=12, SDI=13 |
| 电源锁存 | BAT_Control=GPIO17（=HIGH 保持开机），PWR 键=GPIO18，BAT_ADC=GPIO4（VBAT=VADC×2） |
| 音频电源 | GPIO42（实测 LOW 更稳，见显示部分） |
| USB | Type-C 原生 USB-Serial/JTAG（正常枚举为 COM3，无 CP210x） |
| 按键 | BOOT=GPIO0（低有效，内部上拉）；PWR=GPIO18 |

---

## 2. 开发环境

- PlatformIO Core 6.2.0 + `platform = espressif32@7.1.2`（Arduino core 2.0.17）
- 依赖库：`NimBLE-Arduino@^1.4.3`、`bblanchon/ArduinoJson@^7.0.4`
- `platformio.ini` 关键项：

```ini
[env:esp32-s3-epaper-154g]
platform = espressif32
board = esp32-s3-devkitc-1
framework = arduino
board_build.flash_size = 8MB
board_build.arduino.memory_type = qio_opi   ; 8MB OPI PSRAM，必须
board_build.partitions = partitions.csv     ; 自定义分区
board_build.filesystem = littlefs
build_flags =
  -DARDUINO_USB_MODE=1
  -DARDUINO_USB_CDC_ON_BOOT=1
```

- `board` 用通用的 `esp32-s3-devkitc-1` 即可，不要找冷门 board 定义；Flash/PSRAM 靠上面三行配置。

---

## 3. 首次接入与出厂备份（务必先做）

1. 插入 USB → 设备管理器应出现 USB-Serial/JTAG（COM3）；若进不去下载模式，按住 BOOT 再插拔。
2. **先读芯片信息**：`esptool.py -p COM3 flash_id` 确认 Flash 容量与型号。
3. **全片备份**（8MB 约 40 秒）：

```powershell
pio pkg exec -p tool-esptoolpy -- esptool.py -p COM3 -b 921600 read_flash 0x0 0x800000 factory-full-8mb.bin
Get-FileHash factory-full-8mb.bin -Algorithm SHA256   # 记录哈希
```

4. 建议同时单独读分区表：`read_flash 0x8000 0x1000 partitions-0x8000.bin`。
5. 出厂固件是 Waveshare 定制的 XiaoZhi（含时钟界面），**全片镜像恢复后即可完全还原**。

> 教训：项目一开始没做全片备份就去刷固件，后来靠“刷回出厂”交付才补做了备份。任何设备上手第一件事都是全片备份 + 哈希归档。

---

## 4. 分区表与 OTA 槽位

出厂布局**没有 OTA 双槽**（factory 4MB + assets）。自制固件用了双 OTA：

```csv
# Name,   Type, SubType, Offset,   Size,     Flags
nvs,      data, nvs,     0x9000,   0x4000,
otadata,  data, ota,     0xd000,   0x2000,
phy_init, data, phy,     0xf000,   0x1000,
ota_0,    app,  ota_0,   0x10000,  0x300000,
ota_1,    app,  ota_1,   0x310000, 0x300000,
coredump, data, coredump,0x610000, 0x10000,
storage,  data, spiffs,  0x620000, 0x1E0000,
```

经验点：

- **USB 烧录不会更新 otadata**。如果用网页/ArduinoOTA 刷过一次（写到了 ota_1），下次 `pio run -t upload`（写 ota_0）重启后仍会启动旧槽，表现为“刷了没生效”。解决：
  ```powershell
  pio pkg exec -p tool-esptoolpy -- esptool.py -p COM3 erase_region 0xD000 0x2000
  ```
- 加 `coredump` 分区可消除开机 `No core dump partition found!`，并便于崩溃分析。
- LittleFS 用 `storage` 标签时必须显式传分区名：
  ```cpp
  LittleFS.begin(true /*formatOnFail*/, "/littlefs", 10, "storage");
  ```
- 改动分区表后首次启动会重新格式化文件系统，存储内容会丢，测试前心里有数。

---

## 5. 显示调试（本项目最大的坑）

### 症状
固件完全正常运行（串口日志、HTTP、BLE 全通过），但墨水屏始终停在出厂女孩图；后来变成全白。

### 排查过程与根因
1. 官方 Arduino 位操作驱动（bit-bang）→ 无效
2. 硬件 SPI 20MHz mode0 + 复位低电平 20ms → 仍无效
3. 在初始化前做**面板电源循环**（EPD_PWR=GPIO6：HIGH 500ms → LOW 200ms）→ 能刷出四色条（`tools/epd-test` v3）
4. 主固件照搬后仍白屏 → 对比发现测试工程还做了 **GPIO17 HIGH（VBAT 电源锁存）** 和 **GPIO42 LOW**，补上后才正常显示

**最终可用初始化序列**：

```cpp
pinMode(EPD_PWR_PIN, OUTPUT);        // GPIO6 低有效
digitalWrite(EPD_PWR_PIN, HIGH);     // 先断电
delay(500);
digitalWrite(EPD_PWR_PIN, LOW);      // 再上电
delay(200);
pinMode(42, OUTPUT); digitalWrite(42, LOW);    // 音频电源关闭（经验值，缺了会白屏）
pinMode(17, OUTPUT); digitalWrite(17, HIGH);   // VBAT 锁存，必须
delay(20);
DEV_Module_Init();                   // 硬件 SPI 20MHz, MSBFIRST, SPI_MODE0
EPD_1IN54G_Init();                   // 复位时序：1→200ms→0→20ms→1→200ms
EPD_1IN54G_Clear(EPD_1IN54G_WHITE);
```

- 结论：面板一旦处于掉电/卡死状态，**必须重新断电上电**才能被初始化；且 VBAT 锁存不拉高会白屏。
- 硬件 SPI：`SPI.begin(12, -1, 13, -1); SPI.beginTransaction(SPISettings(20000000, MSBFIRST, SPI_MODE0));`

### Waveshare GUI_Paint 的颜色参数陷阱
`Paint_DrawString_EN(x, y, s, font, a, b)` 内部把参数当 `(background, foreground)` 用（与其文档相反）：

```cpp
// 想要黑字白底：传 (WHITE, color)
Paint_DrawString_EN(8, y, text, &Font16, EPD_1IN54G_WHITE, EPD_1IN54G_BLACK);
```

### 字体只支持 ASCII
`Paint_DrawChar` 按字节查表，遇到 UTF-8/中文会读到表外内存（乱码甚至崩溃）。所有绘制文本先过滤：

```cpp
c >= 32 && c <= 126 ? c : '?';   // 固件与 PC 桥两端都做了净化
```

### 刷新频率与频闪
- 四色屏**不支持局部刷新**，每次更新全屏闪烁 15–20 秒，这是硬件特性。
- 缓解：用“显示字段签名”去重（plan/used/windowMins/resetsAt 不变就不重绘）。坑：最早比较整个 JSON，`server_time` 每秒都在变 → 每 30 秒闪一次；改成只比较显示字段。
- 首屏验证要等：启动到出图可能 40–60 秒。

### 调试用测试工程
`tools/epd-test/`（独立小工程，只做初始化+四色条），迭代比主固件快得多，是定位显示问题的关键工具。

---

## 6. 烧录、OTA 与恢复出厂

```powershell
# 常规烧录（USB）
cd "D:\codex_status"
pio run -t upload
pio pkg exec -p tool-esptoolpy -- esptool.py -p COM3 erase_region 0xD000 0x2000   # 关键补刀

# 恢复出厂全片（先校验哈希）
Get-FileHash "...\factory-backup\factory-full-8mb.bin" -Algorithm SHA256
pio pkg exec -p tool-esptoolpy -- esptool.py -p COM3 -b 921600 write_flash 0x0 "...\factory-full-8mb.bin"
```

- 固件内 OTA 两套：网页 `http://<IP>/update`、ArduinoOTA（hostname `codex-status-xxxx`，密码在源码 `OTA_PASSWORD`）。
- OTA 后如果又用 USB 刷写，记得清 otadata（见第 4 节）。
- USB 刷写速度 921600 稳定；USB-Serial/JTAG 无需驱动。

---

## 7. Wi-Fi 配网

- 首次启动（或读不到 NVS）自动进 AP 配网：热点 `CodexStatus-XXXX`（密码 `codex1234`），浏览器开 `http://192.168.4.1`，可选扫描到的 SSID 或手动输入，保存后自动重启。
- 凭据存在 NVS 命名空间 `wifi`：`s0/p0`、`s1/p1`、`s2/p2`（最多 3 个），启动按槽位顺序尝试，每个超时 15s。
- 掉线保护：Wi-Fi 断连超过 2 分钟自动重启。
- 实测环境：设备 `192.168.1.51`，PC `192.168.1.100`（动态）——脚本里别写死 PC IP。

---

## 8. BLE（测试与联调）

- 服务/特征（NimBLE 外设）：

| 特征 | UUID 后缀 | 方向 |
|---|---|---|
| service | `e7f1a000-...-3c0d5e9a0000` | — |
| info | 001 | 读（型号/固件/模板列表） |
| endpoint | 002 | 写（host/port/token JSON） |
| usage | 003 | 写（usage JSON） |
| status | 004 | 读+notify（ack、错误码） |
| template_ctrl | 005 | 写（begin/end/activate/list/delete） |
| template_data | 006 | 写（2B 小端 offset + 分片） |

- 配对：NimBLE `setSecurityAuth(true,false,true)` + NoInputNoOutput（Just Works + SC + bonding），PC 端 bleak / btleplug 直接连，Windows 不需要手动配对。
- 测试桥（Python bleak）：
  - 正常模式：扫描 `CodexStatus-*` → 写 endpoint → 设备走 Wi-Fi 拉取 → BLE status 回执。
  - **备选通道验证小技巧**：写一个死端口 endpoint（如 `127.0.0.1:9999`）让 Wi-Fi 必失败，再通过 BLE 推 usage → 屏幕上应出现 `SRC xxx BLE`。
- BLE 写入一律 `write-with-response`（流控即协议）；分片大小 180B 稳。
- 设备地址显示为 MAC+1（`...A9:C5`），正常现象。

---

## 9. 串口日志读取（别触发复位）

直接 `SerialPort.Open()` 会拉 DTR/RTS 把板子复位，导致看不到启动日志。必须：

```powershell
$p = New-Object System.IO.Ports.SerialPort COM3,115200,None,8,one
$p.DtrEnable = $false; $p.RtsEnable = $false   # 必须
$p.Open()
```

现成脚本：`C:\Users\user\AppData\Local\Temp\opencode\read_serial.ps1 -Seconds 75`。

## 10. 摄像头读屏（无示波器也能验证）

- `python capture_cam.py out.png`（1280×720）拍桌面上的设备；屏幕区域小，需要裁剪 + 放大。
- 设备斜放时用透视校正（`cv2.getPerspectiveTransform`）把屏幕拉正再读字。
- 光照/反光会骗人：全白也可能只是反光，先等 20s 越过刷新窗口，多拍几张对比。
- 读不出小字时：临时改用大号字体（Font24）或对比明显的模板做验证。

## 11. 电源与低功耗

- 电池版：GPIO17 是电池供电锁存，**开机后必须尽快拉高**，否则松手即断电（USB 供电时不明显）。
- 建议：电池模式 deep sleep（定时唤醒 + BOOT 唤醒），USB 在线常驻。固件里已留框架（NVS `cfg/batt` 开关，默认关）。
- PWR 长按软关机 = GPIO17 拉低（未实现，列为待办）。
- 面板本身掉电保图，是“保持上一版画面”的天然能力。

## 12. 常见坑清单

- 中文显示乱码：控制台 `chcp 65001`；Python 重定向日志必须 `-u`，否则看不到输出。
- 后台脚本（桥接/服务）必须 `Start-Process -WindowStyle Hidden` 启动 + 日志重定向，并记 PID；不要在前台跑常驻脚本。
- 长轮询/等待不要内联在单条命令里，容易误判为卡死。
- `nvs_open failed: NOT_FOUND` 噪声：首次启动前先用 `Preferences.begin("brg", false)` 预创建命名空间。
- 墨水屏刷新慢：任何“改了但没效果”的判断都要等 20 秒以上再下结论。
- GPIO17/42 不要挪作他用（显示与供电相关）；GPIO33–37 被 PSRAM 占用，GPIO19/20 是 USB。
- 官方 BSP 的按键回调有复制粘贴 bug（BOOT 长按挂到了 PWR），实现按键时别照抄。

---

## 附录：新设备（ESP32-S3-ePaper-1.54 黑白版）移植差异

- 屏幕：SSD1681，200×200 黑白，**支持局部刷新（~300ms，无闪烁）**，全刷 ~1.5s。
- 引脚与四色版完全一致（含 GPIO6/17/42 的供电与锁存），Wi-Fi/BLE/配网/OTA/按键逻辑可直接复用。
- 需要改的只有显示层：换 SSD1681 驱动；帧缓冲 1bpp（5000 字节）；`Paint_SetScale(2)`；UI 去掉红/黄；可启用局部刷新做分钟级更新。
- 参考实现：`clawdmeter-epaper` 的 `waveshare_epaper_154` board（含 partial refresh 初始化序列与 LUT）。
- 初始化顺序同样先做电源循环 + GPIO17/42，再初始化面板。
