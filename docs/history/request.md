# Codex Status 需求文档

- 版本：v1.0（需求闭合）
- 日期：2026-09-11
- 状态：需求已确认，待启动实现
- 相关文档：`docs/history/discussion-summary.md`（早期讨论归纳）

## 1. 产品目标

用一块便携的四色电子墨水屏，在任意办公地点的局域网内，显示**当前使用中电脑**上的 Codex 余量、重置时间与状态。

核心特征：

1. **便携**：设备在不同办公室、不同电脑之间移动使用；多台电脑与设备各自一次性 BLE 配对，换机免重配
2. **本地闭环**：数据在局域网（Wi-Fi 主通道）与 BLE 备选通道内传输，不出本机与设备之间。桥接不要求常开；不可达时设备保持上一版画面（墨水屏零功耗保图）
3. **模板化**：显示样式由模板定义；模板经 Wi-Fi（主）或 BLE（备）下载到设备本地渲染并热替换，换样式不刷固件
4. **配对即信任**：BLE 标准配对/绑定作为信任锚点；Wi-Fi 通道使用配对时下发的 token，不自建配对码/HMAC

## 2. 已定决策一览

| # | 决策 | 结论 |
|---|---|---|
| D1 | 桥接形态 | Rust 一体，运行在 Tauri v2 进程内（桥接 + 托盘同进程） |
| D2 | 数据源 | `codex app-server` JSON-RPC（stdio），只读方法；不直接处理 auth.json |
| D3 | 设备渲染 | 设备本地渲染；常态只收 usage JSON（约 1KB）；模板经主通道下载 |
| D4 | 主通道 | **Wi-Fi 局域网 HTTP**：设备主动拉取 `/usage`、`/template`（Bearer token） |
| D5 | 备选通道 | **BLE GATT**（NimBLE 外设 / btleplug central）：Wi-Fi 不可用时承载 usage + 模板，并刷新 Wi-Fi 地址 |
| D6 | 信任模型 | 首次 **BLE 标准配对/绑定**；配对时下发局域网 endpoint + token；零 mDNS、零广播、无配对码 |
| D7 | 电源 | 电池 + deep sleep：唤醒窗口优先试 Wi-Fi，全部失败再开 BLE 窗口；USB 在线时保持连接 |
| D8 | 固件技术栈 | PlatformIO + Arduino-ESP32（BLE 用 NimBLE-Arduino） |
| D9 | 热控定义 | 换模板能力（低频、版本化同步），不做实时帧推送 |
| D10 | 使用边界 | 不做公网通道；局域网或 BLE 近距离可达即可 |

## 3. 角色与场景

- 单用户多台电脑（办公室 A、办公室 B、笔记本），设备随身携带
- 设备与多台电脑各自一次性 BLE 配对；日常优先走所在局域网的 Wi-Fi HTTP
- Wi-Fi 不可用（不同网络/客户端隔离/地址失效）时自动切换到 BLE 备选通道
- 两者都不可达（电脑不在、桥接未运行、超出距离）时保持上一版画面（STALE/OFFLINE）

## 4. 硬件（按官方 wiki 与官方驱动核验，2026-09-11）

- 开发板：Waveshare ESP32-S3-ePaper-1.54G（SKU 34586，含电池版）；无电池版 1.54G-EN，共用同一套固件
- SoC：ESP32-S3-PICO-1-N8R8（双核 LX7 240MHz，512KB SRAM，8MB Flash + 8MB PSRAM，2.4GHz Wi-Fi + BLE5）
- 屏幕：1.54 英寸 200×200 四色（黑/白/红/黄），SPI，2bit/像素（帧 10,000 字节），全刷 20s / 快刷 15s，**无局刷接口**，反射式 >170°
- 板载：ES8311 音频编解码 + 麦克风 + 喇叭座、SHTC3 温湿度、PCF85063 RTC、TF 卡槽、ETA6098 充电管理、MP1605 3.3V DC-DC
- 按键：BOOT（GPIO0，下载模式）、PWR（GPIO18/17 电源控制，可编程）
- USB：Type-C 原生 USB（GPIO19/20），烧录与日志
- 供电：3.7V MX1.25 锂电池

### 4.1 引脚表（wiki 核验）

| 功能 | 引脚 |
|---|---|
| E-Paper | PWR=GPIO6（需使能）, BUSY=8, RST=9, DC=10, CS=11, SCLK=12, SDI=13 |
| I2C 共用总线 | SCL=48, SDA=47（RTC 0x51 / SHTC3 0x70 / ES8311 0x18） |
| RTC 中断 / 电池 ADC | RTC_INT=5 / BAT_ADC=4（VBAT = VADC × 2） |
| 电源控制 | BAT_KEY=18, BAT_Control=17 |
| I2S 音频 | MCLK=14, SCLK=15, ASDOUT=16, LRCK=38, DSDIN=45, PA_EN=42, PA_CTRL=46 |
| TF 卡 SDIO | CLK=39, MOSI=41, MISO=40 |
| UART0 / 扩展 | TX=43, RX=44 / 扩展 GPIO 1/2/3 |
| 不可用 | GPIO33–37（八线 PSRAM）, GPIO19/20（USB） |

### 4.2 烧录方式（如何写入 ROM）

- 出厂固件：官方仓库 `Firmware/01_factory.bin`，用乐鑫 Flash Download Tool 烧到地址 `0x00`
- 进入下载模式：按住 **BOOT** 再上电（或重新插拔 USB）
- 开发烧录：
  - Arduino IDE：arduino-esp32 **≥ 3.2.0**，示例在官方仓库 `Example/Arduino_3.2.0`
  - ESP-IDF **5.5.1**：官方仓库 `Example/ESP-IDF_5.5.1`（含 09_E_Paper_Test、08_BATT_PWR_Test 等）
  - PlatformIO：esptool 经 USB CDC/JTAG 上传；board 基线 `esp32-s3-devkitc-1` + 8MB flash + PSRAM；Arduino 3.2.0 的支持情况在构建时确认（否则用 pioarduino 平台分支或转 ESP-IDF）
- 擦除：Flash Download Tool 的 ERASE；失败则按住 BOOT 上电重试

### 4.3 屏幕驱动要点（官方 `EPD_1in54g` 驱动）

- 颜色 2bpp 编码：`BLACK=0, WHITE=1, YELLOW=2, RED=3`；每字节 4 像素，高位在前（pixel0 占 bit7–6，依次）
- 帧缓冲：200 ÷ 4 = 50 字节/行 × 200 行 = **10,000 字节**
- 关键命令：`0x10` 写显存 → `0x12` 触发刷新；快刷初始化附加 `0xE0/0xE6/0xA5`；睡眠 `0x02` → `0x07`=`0xA5`
- 面板电源 `EPD_PWR=GPIO6` 必须在初始化前使能

### 4.4 按键、电源与状态指示（官方 BSP 核验）

- 按键：BOOT=GPIO0、PWR=GPIO18（均低有效、内部上拉）；官方附 `multi_button` 库，支持单击/双击/长按/弹起事件
- 电源锁存：BAT_Control=GPIO17（1=保持开机，0=软关机）；PWR **长按 = 软关机**（官方 08_BATT_PWR_Test 已验证）
- 面板电源：GPIO6（低有效使能）；音频电源：GPIO42
- LED：官方 `led_bsp` 预留 GPIO3 为绿灯（低有效，红灯=-1 未接），但物料清单未列板载 LED，且 GPIO3 已引至扩展排针 → **实物确认**；可外接状态灯做闪码提示。出厂固件（Waveshare 自定义 `user_app_bsp`）实际会驱动绿灯 GPIO3 提示状态（`user_app.cpp` Green_led_user_Task：1–4 次闪/常亮/快闪）；XiaoZhi 核心框架的 `BUILTIN_LED_GPIO=NC` 不代表该自定义层
- 设备状态判断链：
  1. 屏幕（e-paper 断电保图）= 最后成功状态 + 时间戳，任何时刻可读
  2. BOOT 短按可作 deep sleep 唤醒源（GPIO0 为 RTC GPIO，ext0/ext1）→ 立即同步重绘
  3. 桥接侧用 last_seen 超时判离线（轮询间隔 × 2–3）
  4. 电池 ADC（GPIO4，VBAT=VADC×2）与 RSSI 随每次轮询上报
- 注意：官方 `button_bsp.c` 中 BOOT 的长按/弹起回调误挂到了 PWR 按键上（复制粘贴 bug），实现时应修正

### 4.5 出厂固件与分区（2026-09-11 实机读取）

- 芯片实测：ESP32-S3-PICO-1 (LGA56) rev v0.2，8MB Flash（GD）+ 8MB PSRAM（AP_3v3），MAC `70:04:1D:AA:BB:01`，USB-Serial/JTAG（COM3）
- 固件实测：project `xiaozhi`，**version 2.0.1**，构建于 2026-05-18，ESP-IDF v5.5.1（与仓库源码/出厂 bin 版本号一致，仅构建时间不同）
- 分区表实测（等于 `partitions/v2/8m.csv`）：nvs 0x9000/16K、otadata 0xD000/8K、phy_init 0xF000/4K、**factory 0x10000/4MB**、assets 0x410000/约4MB
- 结论：出厂布局**没有 ota_0/ota_1 槽位**，当前固件不支持双分区 Wi-Fi OTA（`assets` 是网络加载内容的独立通道，不用于固件）；自制固件可自行选择带 OTA 的分区表
- 2026-09-11 实机状态：已备份出厂全片（`D:\codex_status_backup\factory-backup\factory-full-8mb.bin`，SHA256 `7E9CF08B...C29F25`），并已刷入自制精简固件 **0.1.0-pre**（双 OTA 布局：ota_0/ota_1 各 3MB + storage 1.94MB；功能：多 SSID 配网门户 + 网页/ArduinoOTA 双 OTA；固件 777KB，占 OTA 槽 25%）；当晚网页 OTA 验证通过（0.1.1-ota，双分区切换正常）；下一步：Wi-Fi 主通道 + BLE 备选固件（见 §7）

## 5. 系统架构

```
codex-cli app-server（本机，stdio JSON-RPC）
    │ account/rateLimits/read（快照）
    │ account/rateLimits/updated（稀疏增量推送）
    ▼
Rust 桥接（Tauri v2 进程内）
    ├─ 上游共享缓存（一次取数）
    ├─ Wi-Fi 主通道 HTTP 服务（/usage、/template，Bearer token）
    ├─ BLE central（btleplug）：备选通道 + 下发/刷新 endpoint+token
    ├─ 模板库 + 设备记录
    └─ 托盘 UI（配置 / 模板管理 / 配对 / 预览）
    │
    ├── 主：局域网 HTTP（设备拉取）──────────────┐
    └── 备：BLE GATT（绑定链路即信任）─────────┐  │
                                              ▼  ▼
ESP32-S3 墨水屏设备（Wi-Fi STA + BLE peripheral）
    ├─ 唤醒：按 MRU 试各 bond 的 endpoint → GET /usage（Bearer）
    ├─ 全部失败：开 BLE 窗口 → 已绑定主机推送 usage/模板并刷新 endpoint
    ├─ 模板（两通道皆可传）→ A/B 槽原子写入 + 内置默认模板兜底
    ├─ 电池：deep sleep + 定时唤醒；USB 在线：保持连接
    └─ 按键（切模板 / 强制 BLE 窗口 / 恢复出厂）
```

## 6. 数据源（已实测，2026-09-11）

### 6.1 二进制定位

- 真 CLI：`codex-cli 0.153.4`，位于 MSIX / Codex Minibar 安装目录：
  `%LOCALAPPDATA%\Codex Minibar\desktop-cli\OpenAI.Codex_<版本>_x64__...\codex.exe`
  以及 `%LOCALAPPDATA%\Packages\OpenAI.Codex_2p2nqsd0c76g0\LocalCache\Local\OpenAI\Codex\bin\codex.exe`
- **坑**：npm 全局存在同名包 `codex@0.2.3`（静态站点生成器），会抢占 PATH。桥接定位二进制必须校验 `--version` 输出包含 `codex-cli`；候选顺序：用户配置路径 → 已知安装路径（取最新版本）→ PATH 兜底校验

### 6.2 协议

- 传输：stdio JSON-RPC（换行分隔）；`--listen` 亦支持 `unix://` 与 `ws://`，daemon 模式可用（实现时评估 spawn 自管 vs 连接 daemon）
- 握手：`initialize {clientInfo}` → 通知 `initialized {}` → 业务方法
- 使用的方法（只读）：
  - `account/rateLimits/read` → 快照
  - `account/rateLimits/updated` → 服务端推送的稀疏增量，客户端合并到最近快照或重取
  - `account/usage/read`（token 用量，可选展示）
- **不使用**：`account/rateLimits/consumeCredit`（会消耗重置次数）及一切写操作

### 6.3 响应结构（实测）

```json
{
  "rateLimits": { "limitId": "codex", "primary": { "usedPercent": 91, "windowDurationMins": 10080, "resetsAt": 1789628788 }, "secondary": null, "planType": "prolite", "credits": { "balance": "0", "hasCredits": false, "unlimited": false } },
  "rateLimitsByLimitId": {
    "codex": { "...": "同上" },
    "codex_bengalfox": { "limitName": "GPT-5.3-Codex-Spark",
      "primary":   { "usedPercent": 0, "windowDurationMins": 300,   "resetsAt": 1789150425 },
      "secondary": { "usedPercent": 0, "windowDurationMins": 10080, "resetsAt": 1789737225 } }
  },
  "rateLimitResetCredits": { "availableCount": 1, "credits": [{ "status": "available", "title": "Full reset", "expiresAt": 1791174137 }] },
  "accountId": "<opaque>"
}
```

**三条强制规则**：

1. 窗口类型**按 `windowDurationMins` 分类**（300=5 小时，10080=周），绝不按 primary/secondary 位置
2. 额度是**多桶模型**（`rateLimitsByLimitId`，如 `codex`、`codex_bengalfox`），每个桶有各自的窗口与套餐
3. `planType` 是约 17 值枚举（free/go/plus/pro/prolite/team/enterprise/...），展示需完整映射，不能只认 PLUS/PRO

- 完整协议 schema：`%TEMP%\opencode\codex-app-server-schema\`（用 `codex app-server generate-json-schema --experimental --out <dir>` 重新生成）；桥接按 CLI 版本锁定并随升级更新

## 7. 数据通道与设备 ↔ 桥接协议

### 7.1 通道模型与信任

- **主通道：Wi-Fi 局域网 HTTP**——设备主动拉取（`/usage`、`/template`），省电、快，适合模板等较大对象
- **备选通道：BLE GATT**——Wi-Fi 不可用（不同网络、客户端隔离、endpoint 失效）时，设备打开广播窗口，已绑定主机连接后推送数据；能力与主通道等价（usage + 模板）
- **信任锚点：BLE 标准配对/绑定**（LE Secure Connections，一次性）；局域网侧不自建配对码/HMAC
- **寻址：零 mDNS、零广播**——桥接通过 BLE 向设备下发自己的局域网 endpoint 与 token；地址变化时下次连接自动刷新

### 7.2 首次配对（BLE）

1. 设备首次开机（或长按 BOOT）打开配对窗口（如 120 秒），广播自定义 128-bit 服务 UUID
2. 桥接（central）扫描并连接，发起标准 BLE 配对/绑定
3. 绑定成功后，桥接通过 GATT 写入：
   - `wifi_endpoint = { host, port }`（本机当前局域网地址）
   - `token`（随机 32 字节，主通道 Bearer 凭据）
4. 设备按 bond 持久化（主机 + endpoint + token）；后续重连免配对
5. 多主机：设备保存多个 bond（上限可配，建议 8）；配对窗口外拒绝未绑定主机

### 7.3 Wi-Fi 主通道（HTTP，Bearer token）

- `GET /usage` → usage 信封（7.5）；设备唤醒后按 MRU 顺序尝试各 bond 的 endpoint，每个超时 2–3s
- `GET /template?id=<id>&hash=<hash>` → 模板 JSON；hash 一致返回 304
- 请求带 `Authorization: Bearer <token>`；桥接 HTTP 仅监听局域网
- 连续 N 次失败 → 设备进入 BLE 窗口等待刷新 endpoint

### 7.4 BLE 备选通道（GATT）

| 特征 | 方向 | 内容 |
|---|---|---|
| `info` | read | 设备型号、固件版本、协议版本、bond 数 |
| `wifi_endpoint` | central → device（write） | 更新 host/port/token（每次连接必写） |
| `usage` | central → device（write） | 精简 usage JSON（7.5），按 MTU 分片 |
| `status` | device → central（notify） | 电量、渲染结果、last_sync、ack/错误码 |
| `template_ctrl` | central → device（write + notify 流控） | 模板协商：id/hash/version/分片序号 |
| `template_data` | central → device（write） | 模板 JSON 分片；hash 校验后 A/B 槽写入 |

- MTU 协商（建议 ≥247）；写入一律 write-with-response；分片带序号 + 总长 + CRC

### 7.5 usage 信封（两通道共用）

```json
{
  "schema": 1,
  "server_time": 1789150000,
  "next_sync_seconds": 300,
  "bridge": { "label": "OFFICE-PC", "hostId": "ab12" },
  "account": { "plan": "prolite" },
  "buckets": [
    { "id": "codex", "name": null,
      "windows": [
        { "kind": "weekly", "usedPercent": 91, "resetsAt": 1789628788, "windowMins": 10080 }
      ],
      "credits": { "balance": "0", "hasCredits": false, "unlimited": false } }
  ],
  "resetCredits": { "availableCount": 1, "nextExpiresAt": 1791174137 },
  "templates": { "full": { "version": 7, "hash": "ab12cd" } }
}
```

- 连接/请求建立后给出最新快照；上游 `account/rateLimits/updated` 到达时刷新
- `next_sync_seconds`：电池模式唤醒窗口建议（默认 300–900）；USB 在线时忽略
- 设备忽略未知字段（宽容解析）；信封带 `schema` 版本号

### 7.6 模板下发规则

- 对比 hash，不一致才传输；模板含 `min_fw`，不兼容则拒绝下发
- 设备遇到不认识的原语时**拒绝整份模板**，继续用旧模板，绝不半渲染

### 7.7 管理面（托盘 ↔ 桥接，进程内）

- 模板库 CRUD、预览渲染、已绑定主机列表、激活模板、解除绑定（token 同步失效）
- 托盘通过进程内调用/本地 IPC 访问桥接，不开放网络端口

## 8. 模板系统

### 8.1 模板格式（声明式 JSON）

```json
{
  "schema": 1, "id": "full", "version": 7, "hash": "ab12cd", "min_fw": "1.0",
  "canvas": { "w": 200, "h": 200 },
  "fonts": { "d16": { "w": 8, "h": 16, "atlas": "<base64 位图图集>" } },
  "elements": [
    { "type": "text", "bind": "account.plan",  "x": 5,  "y": 68, "font": "d16" },
    { "type": "bar",  "bind": "buckets[codex].weekly.remaining", "rect": [54, 77, 102, 9] }
  ]
}
```

- 原语集合（v1）：`text`、`bar`、`rect`、`line`、`icon`（位图引用）
- 数据绑定路径是设备与模板之间的稳定契约，需版本化；模板只决定"怎么画"，数据只提供值
- 字体：固件内置若干基础字体 + 模板可携带只含所需字符的位图图集
- 数据绑定字段最小集：账户套餐、各桶各窗口的 usedPercent/resetsAt、重置次数与到期、桥接 label、数据时间戳（用于 STALE 表达）

### 8.2 存储与生命周期（设备端）

- LittleFS：**全局模板库**（模板与主机无关，谁连接就推谁的模板集）
- 最多保留 K=4 个模板，LRU 淘汰；当前激活模板永不淘汰
- A/B 双槽原子写入；内置出厂默认模板兜底（flash 空 / 模板不兼容 / 桥接不在）

### 8.3 模板切换

1. 托盘手动：通过 BLE 推送 template_ctrl 指定激活模板
2. 设备按键：短按循环本地槽位，通过 status 特征上报
3. 规则自动（延后）：临近重置切倒计时模板、夜间切极简

优先级规则：桥接指定为权威；设备按键是本地覆盖并上报；桥接再次明确指定时覆盖设备。

### 8.4 预览一致性

- 渲染器唯一实现放桥接（Rust），托盘预览直接复用
- 设备端 C++ 引擎与桥接渲染器通过 golden 测试对齐：固定模板 + 固定数据 → 帧缓冲逐字节比对

## 9. 固件需求

### 9.1 同步循环

```
唤醒（USB 在线常驻；电池为定时唤醒）
  → 按 MRU 顺序尝试各 bond 的 Wi-Fi endpoint：GET /usage（Bearer，单个超时 2–3s）
      → 成功：hash 变化则 GET /template → 本地渲染 → 刷屏 → 睡眠(next_sync_seconds)
      → 全部失败：打开 BLE 广播窗口（约 10–30s）
            → 已绑定主机连接（免配对）→ 推送 usage/模板并刷新 endpoint
            → 渲染刷屏 → 睡眠
  → 记录 last_sync；无论成败，屏幕保持可读的上一版画面
```

### 9.2 按键 UX

- 短按：循环切换本地模板
- 长按：强制 BLE 窗口（立即通过 BLE 同步并刷新 Wi-Fi endpoint；同时允许新主机配对）
- 超长按 10s：恢复出厂（清全部 bond + 设置）

### 9.3 失败策略

- 取数/取模板失败：**不重绘**，屏幕保持上一版（墨水屏保图特性）
- 画面上的时间戳元素表达数据新旧（STALE 语义）
- 找不到任何已配对桥接：保持画面，不连接陌生桥接

### 9.4 配网与固件更新

- Wi-Fi 凭据：保留 AP 配网门户（多 SSID；现有 dev 固件已验证）
- 固件更新：USB 首选（BOOT 下载模式）；Wi-Fi 网页/ArduinoOTA 次选（dev 固件已验证双分区）；BLE OTA 未决（见 O10）

### 9.5 功耗

- USB 在线：保持 Wi-Fi/HTTP 可用（实时同步，不睡眠）
- 电池：deep sleep + 定时唤醒；优先 Wi-Fi（快、省电），失败才开 BLE 窗口；`next_sync_seconds` 由桥接下发（默认 300–900s）
- 4 色屏以全刷为主；唤醒期间尽量缩短射频驻留

## 10. 桥接需求（Rust / Tauri v2）

1. app-server 生命周期管理：定位真 CLI、spawn/daemon、重启、版本与 schema 兼容检查
2. 共享上游缓存：一次取数；`account/rateLimits/updated` 到达时刷新并推送给已连接设备
3. **Wi-Fi 主通道 HTTP 服务**：`/usage`、`/template`（Bearer token）；仅监听局域网；支持模板 hash/304
4. **BLE 备选与地址同步（btleplug）**：维护已绑定设备；连接后推送 usage/模板并写入最新 endpoint+token；断线重连
5. 配对管理：配对窗口内完成首次配对；绑定列表、解除绑定（token 失效）、系统蓝牙设置引导
6. 模板库与设备记录（全局模板库；设备记录只存绑定关系、连接状态与已同步 hash）
7. 日志与诊断：上游错误分类（timeout/offline/auth_required）、设备连接状态、last_sync、模板同步状态
8. 平台：Windows（主）+ macOS；托盘常驻（无线通道需要桥接进程在运行）

## 11. 托盘 app 需求

- 设备卡片：连接状态（Wi-Fi/BLE）、电量、固件版本、当前激活模板、last_sync / STALE 提示
- 模板库：列表、导入导出、版本/hash、**预览渲染**、复制/删除
- 分配：激活模板、槽位顺序
- 配置：设备名、同步间隔、app-server 路径、Wi-Fi 监听端口、开机自启
- 配对：打开设备配对窗口、已绑定主机列表、解除绑定（token 同步失效）、系统蓝牙设置引导
- 托盘快捷菜单：快速切模板、立即同步、暂停同步
- UI 复杂度原则：单设备时隐藏设备管理页；多设备才展开

## 12. 安全模型

- 信任锚点 = BLE 配对/绑定：只有已绑定主机可写入 endpoint/token 或连接备选通道
- Wi-Fi 主通道使用配对时下发的 Bearer token（每 bond 一份）；设备只存本机 bond 对应的 token
- endpoint/token 只经 BLE 加密链路下发，不在局域网明文广播；局域网 HTTP 的 TLS 加固列为后续项（见 O12）
- 设备仅在配对窗口期接受新配对（首次开机 / 长按打开），窗口外拒绝未绑定主机
- 解除绑定：桥接删除 token/记录；设备端恢复出厂清 bond；主机端需同步在系统蓝牙中"移除设备"，否则重连失败
- 数据脱敏：usage 信封不含邮箱、accountId 等身份字段

## 13. 规模与上限

| 项 | 上限 | 说明 |
|---|---|---|
| 设备绑定主机数 | 8（可配） | NimBLE bond 存储；每 bond 记录 endpoint + token |
| Wi-Fi 主通道 | 设备主动拉取 | 唤醒按 MRU 尝试各 endpoint，单个超时 2–3s |
| BLE 同时连接数 | 1 | 先到先得 |
| 设备端模板数 | 4 | 全局模板库，LRU 淘汰 |
| 上游取数 | 1 份共享缓存 | 主/备通道共用同一份快照 |

明确不做：跨主机数据聚合、统一账号体系、设备舰队批量管理、多设备同时连接。

## 14. 非目标（明确不做）

- mDNS / 服务发现 / 广播寻址 / 自建配对码与 HMAC（已由 BLE 绑定 + endpoint 下发替代）
- 公网数据通道（Gist / Cloudflare Worker / 手机热点方案）
- 多设备同时连接、跨主机聚合、统一账号体系、设备舰队批量管理
- MQTT / 长连接实时推送、实时帧流
- 多 Provider（Claude/Gemini 等，后续另立需求）
- 直接解析/刷新 auth.json（凭据生命周期完全交给官方 CLI/app-server）

## 15. 待验证 / 未决

| # | 事项 | 阻塞什么 |
|---|---|---|
| O1 | 实物核对板卡丝印与排针（wiki 已给出全部引脚；1.54G/-EN 同固件） | 低 |
| O2 | 4 色屏长期残影与全刷频率（官方驱动无局刷，全刷 20s / 快刷 15s） | 低 |
| O3 | 板载按键可用性（官方 BSP 已确认可用；实现时修正其回调 bug） | 按键 UX |
| O4 | app-server 生命周期：spawn 自管 vs daemon 连接（`--listen ws://`） | 桥接实现 |
| O5 | 模板 schema 定稿（原语细节、字体图集格式、bind 路径终稿） | ~~模板系统~~ v1 已落地：text/bar/rect/line/icon、内置字体 f8–f24、bind 路径见 PROGRESS §3.2；字体图集暂缓 |
| O6 | BLE 配对安全模式（Just Works vs Passkey）与配对窗口交互 | 固件/桥接配对 |
| O7 | STALE 阈值、`next_sync_seconds` 默认值 | 固件显示 |
| O8 | macOS 上 codex CLI 安装路径与版本探测 | 跨平台 |
| O9 | BLE 分片传输参数（MTU、分片大小、流控、CRC） | ~~设备与桥接联调~~ v1 已定：2B offset 顺序分片 + write-with-response 流控 + CRC32；待上板联调 |
| O10 | 电池模式 BLE 窗口时长/间隔与桥接重连策略；BLE OTA 是否做兜底 | 固件功耗与更新 |
| O11 | Windows/macOS 的 btleplug 与系统蓝牙配对交互差异 | 桥接跨平台 |
| O12 | Wi-Fi HTTP 是否加 TLS（token 目前明文走局域网） | 安全加固 |

## 16. 关键参考

- 数据获取实现（早期本地参考仓库）：`token-monitor`（RPC 实现）、`usage-display`（窗口分类与回退）、`Codex-Usage`（wham 端点全解析）
- 桥接/设备形态参考：`iauso`（RPi 桥接 + API 鉴权）、`clawdometer-eink`（同款屏幕的 BLE 方案）
- 视觉基线：`artifacts/codex-quota-preview.png` 与 `tools/generate-preview.mjs`（第一版模板的种子）
- 设备官方资料：Waveshare `ESP32-S3-ePaper-1.54G` 仓库（Arduino/ESP-IDF 示例、EPD 驱动、原理图、出厂固件）；wiki `docs.waveshare.com/ESP32-S3-ePaper-1.54G`；出厂 XiaoZhi 固件版本 **2.0.1**（`Example/XiaoZhi/01_xiaozhi-esp32/CMakeLists.txt` 的 `PROJECT_VER`，与 `Firmware/01_factory.bin` 内字符串一致；仓库提交 2026-07-04）
- 协议 schema：`%TEMP%\opencode\codex-app-server-schema\`（codex-cli 0.153.4）
