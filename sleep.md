# sleep.md — 电池续航与休眠执行方案（v4）

状态：**决策闭合，可执行**。按 M1→M2→M3 顺序实施，无待决项、无可选分支。
范围：固件 `src/`、桥 `bridge/`、模板资产 `tools/test-bridge/templates/`。

## 0. 最终决策

| # | 决策 |
|---|---|
| D1 | 双模式：DEEP（窗口、µA 级、IDLE 界面）/ LIVE（Wi-Fi 保活、0.5–2mA、数据界面） |
| D2 | 模式切换仅由可达性驱动；不做夜间/闲置降级；PC 常开过夜保持 LIVE |
| D3 | LIVE 仅随 M3（自编 core + PM）交付；不提供常亮过渡形态 |
| D4 | BLE 仅承担身份/信任/窗口通信；LIVE 中关闭 BLE，身份走 endpoint token 映射 |
| D5 | active：最后成功同步 + T=600s 超时抢占 + 显式覆盖最高优先 + 跨 BSSID 即时抢占；不做空闲闸门 |
| D6 | 默认 `idle_template = quad`（元素 `mode` 变体双用）；不提供第 4 个独立 idle 模板 |
| D7 | 协议新增：元素 `mode` 标记；idle bind `device.state`/`device.offline_mins`/`device.idle_reason`；envelope 增 `idle_template`、`active_hold_seconds` |
| D8 | 模板 `min_fw = 0.10`；三方（固件/Rust/Python 测试桥）与新 Node 预览生成器同步实现 |
| D9 | 桥：mDNS 解析设备地址、低占空扫描+退避、开机自启；M3 增 `POST /usage` 推送 |
| D10 | AP 配网仅在"无任何已保存 Wi-Fi 槽位"或"BOOT 长按 5s"时进入，5 分钟空闲超时后入睡 |

## 1. 目的与技术手段

| 目的 | 手段 |
|---|---|
| 显示"当前在用电脑"的余量 | active 判定（§2）；数据通道：Wi-Fi 拉取 / HTTP 推送（M3）/ BLE 窗口推送 |
| 换机免重配 | BLE 配对/绑定 + endpoint token；BLE 只用于配对、窗口通信、兜底 |
| 低功耗 | DEEP：deep sleep；LIVE：Wi-Fi modem-sleep + 自动 light sleep |
| 数据不过期 | LIVE 推送/轮询；DEEP 窗口取数（默认 5 分钟，退避 15 分钟） |

## 2. active 语义（定稿）

- 状态：`activeMac`（bridge MAC，BLE `peerAddress` 与 endpoint 记录同 key）+ `activeAt`；持久化在 `RTC_DATA_ATTR`。
- "连接" = 一次成功的数据同步（BLE usage 被接受 / Wi-Fi 拉取成功 / HTTP 推送被接受）。
- 判定规则：

| 来源 | 条件 | 行为 |
|---|---|---|
| active 机器 | 任意 | 接受、刷新 `activeAt`、按需渲染 |
| 其他机器 | `now - activeAt < 600s` | 忽略（可缓存，不切画面） |
| 其他机器 | `now - activeAt >= 600s` | 抢占为 active 并渲染 |
| 其他机器 | active 的 endpoint 不属于当前 BSSID | 视为不可达，立即抢占 |
| 显式动作 | 托盘"发送到设备"/MCP `profile_push`/BOOT | 直接置 active 并渲染，最高优先 |

- 显式覆盖的传输：M2 经 BLE usage 附 `"activate": true`；M3 同字段走 `POST /usage`。
- 限制（接受）：A 桥常开、人换到 B 时，A 持续同步导致 B 无法抢占；靠显式覆盖处理。

## 3. 根因（修改对象）

| # | 问题 | 证据 |
|---|---|---|
| 1 | 休眠是死代码：`cfg/batt` 只读不写 | `src/main.cpp:972-978` |
| 2 | 仅在 Wi-Fi 拉取成功后才睡 | `src/main.cpp:1124-1129` |
| 3 | 不可达时进 AP 配网模式常亮 | `src/main.cpp:676-695`、`965-968` |
| 4 | 面板从不进 sleep | `EPD_SSD1681_Sleep()` 未被调用，`EPD_SSD1681.cpp:245` |
| 5 | BLE 广播 24/7、30s 拉取、10s ADC、分钟级重绘 | `ble_bridge.cpp:250`、`main.cpp:1107-1129` |
| 6 | Wi-Fi 槽位盲试 3×15s、无 scan/last-used；MRU 全局；endpoint 无 BSSID | `main.cpp:526-631`、`bridge_store.h:4-10` |
| 7 | 桥在设备不可见时 30s 扫描失败→15s 重试，24/7 空扫 | `bridge/crates/app/src/main.rs` BLE 循环 |
mDNS 不是缺陷：`ArduinoOTA.begin()` 默认 `_mdnsEnabled=true`，内部已启动 mDNS，`codex-status-XXXX.local` 自 0.4.0 起可解析（`MDNS.addService(http)` 亦已存在）；固件无需新增 `MDNS.begin()`（重复调用只会误报失败）。M3 只需桥侧解析主机名。

## 4. 设计

### 4.1 模式与状态机

| 状态 | 进入条件 | 功耗 | 显示 |
|---|---|---|---|
| DEEP | 启动、窗口取数失败、LIVE 退出 | µA 级 | IDLE 模板（连续 2 窗失败或从未同步） |
| LIVE | Wi-Fi 关联且到桥取数成功（仅 BLE 可达的网络保持 DEEP） | 0.5–2mA | 数据模板 |

- 转换：DEEP 窗口快扫 Wi-Fi → 连接 → 同 BSSID 端点取数（BLE 并行）；成功且 Wi-Fi 路径可达桥 → M3 进 LIVE，M2 渲染数据模板后入睡；失败按 §4.2 处理。
- LIVE 退出：Wi-Fi 断 >3min，或连续 10min 无成功同步，或显式要求。
- 退出动作：渲染 IDLE（指纹去重）→ `WiFi.disconnect(true)` → BLE deinit → `esp_deep_sleep_start()`。
- 防抖：LIVE 至少驻留 3min；回 DEEP 后至少 5min 再尝试 LIVE。
- M2 显示规则：窗口成功 → 数据模板；连续 2 窗失败或启动从未同步 → IDLE 模板。

### 4.2 DEEP 行为

1. 唤醒源：定时器（启动/换网后前 3 窗 60s，之后 300s；退避 900s）或 BOOT（ext1，`main.cpp:986`）。
2. 唤醒后进入 15s 取数窗口：
   - Wi-Fi：scan 可见 AP 匹配已保存槽位，last-used 优先，连接超时 8–10s；
   - BLE：开广播等待已绑定桥连接（endpoint/usage/显式覆盖/模板），窗口结束停播；
   - 端点：active 新鲜时只试 active；否则同 BSSID MRU 优先，单端点超时 2s。
3. 成功：更新缓存/指纹/active，按 §4.1 渲染；失败计数按 §4.9。
4. 收尾：`EPD_SSD1681_Sleep()` → `WiFi.disconnect(true)` → `esp_deep_sleep_start()`。
5. 无新数据不重绘；DEEP 下不同步分钟。

### 4.3 LIVE 行为（M3）

- 形态：`WIFI_PS_MAX_MODEM` + PM 自动 light sleep；BLE deinit；DTIM/listen interval 调参。
- 接收：`POST /usage`，`Authorization: Bearer <endpoint token>`；token 映射 `activeMac`；按 §2 处理，返回 `{"accepted":bool}`。
- 推送：桥在 usage 指纹变化时推送 + 5 分钟心跳；设备端不再主动轮询，保留 15 分钟兜底轮询。
- 可用：模板 Wi-Fi 拉取、OTA、`/status.json`。
- 桥短暂重启：10min GRACE 内不退出；超时回 DEEP。
- 配对/BLE-only 操作：临时退出 LIVE（BOOT 2s 配对窗口）。

### 4.4 IDLE 模板（默认 quad 变体）

- 角色：桥侧 `idle_template`（默认 `quad`）指定；可切换到 full/mini；idle 模板 pin 住不被 LRU 淘汰。
- 元素 `mode`：`idle` / `live` / `any`（默认 any）；LIVE 渲染跳过 `mode:idle`，IDLE 渲染跳过 `mode:live`。
- quad 布局（`tools/test-bridge/templates/quad.json`）：

| y | LIVE | IDLE |
|---|---|---|
| 144 | — | `5H HH:MM`（`buckets[codex].5h.resetsAt`，`when` 存在才显示） |
| 158 | `5H HH:MM` | `BATT n%` |
| 172 | `BATT n%` | `IDLE`（`device.state`） |
| 186 | `SYNC HH:MM`（`mode:live`） | `OFF 12M`（`device.offline_mins`，prefix `OFF `，suffix `M`） |

- 5H/BATT 各复制 live/idle 两个 `mode` 变体（模板坐标为静态）；余下元素不变。
- IDLE 渲染用缓存的最后一份 usage（含 `bridge.label`、`account.plan`、`buckets[...]`、`resetCredits`），不发起取数。
- idle bind 仅在 IDLE 渲染解析：`device.state`（`LIVE`/`IDLE`）、`device.offline_mins`、`device.idle_reason`（`boot`/`wifi_lost`/`bridge_lost`/`env_switch`）；`when` 缺失自动隐藏。
- 无缓存（首次启动/恢复出厂/缓存丢失）：**不渲染任何模板**，改用内置默认界面（沿用 `screenIdle()` 内置屏），内容：`CODEX STATUS` / `IDLE · NO LINK` / `BATT n%` / `IP <addr>` / `SYNC --:--` / `FW <ver>`；拿到缓存后按模式切回模板渲染。

### 4.5 固件改动

- 模式：CLI `mode auto|deep|live`（默认 auto）；`live` 在 M3 前等价 deep。
- 重写睡眠条件（`main.cpp:1124-1129`）：窗口结束必睡，与拉取成败解耦。
- `RTC_DATA_ATTR`：activeMac/activeAt/BSSID/渲染指纹/失败计数/idle_reason。
- Wi-Fi 选网：scan + last-used 槽位持久化 + 8–10s 超时；endpoint 记录增 BSSID；同 BSSID 优先。
- usage 缓存：最后一份 usage JSON 持久化（NVS blob，仅指纹变化时写）；DEEP 渲染 IDLE 用它。
- 模板：`TplMeta`/`tplStore` 支持 idle id 与 pin；渲染跳过 `mode` 不匹配元素；缓存为空时不走模板渲染、回内置默认界面；校验器接受 `mode` 白名单与 3 个新 bind。
- 唤醒渲染：跳过 `EPD_SSD1681_Clear()`（`main.cpp:196-226`）；每次绘制后 `EPD_SSD1681_Sleep()`；绘制前重新 init（验证 SSD1681 mode 1 + 局刷）。
- BLE info 附当前 IP/端口，并在网络重连/IP 变化时刷新；mDNS 无固件改动（由 ArduinoOTA 提供）。
- AP 配网按 D10；取消无条件兜底。
- M3：`POST /usage` 路由（token + active 规则）、PM 配置、BLE deinit、LIVE 看门狗。

### 4.6 桥改动

- BLE 循环：每 20s 一轮（扫 5s）；连续 12 轮未发现 → 每 60s 一轮；连接成功推送后暂停扫描，直到 usage 指纹变化或 5 分钟心跳到期。
- usage 指纹不变不写 CHR_USAGE；显式推送带 `"activate": true`。
- M3 推送：解析设备地址（mDNS `codex-status-XXXX.local`，失败回退 BLE 缓存的 IP）→ `POST /usage`（token + 指纹变化 + 5min 心跳）。
- 模板：`idle_template` 单选（默认 quad）；envelope 下发 idle id；推送 profile 时附带 idle 模板；面板加"待机模板（IDLE）"选择；MCP `profiles_list/save` 同步该字段。
- 开机自启：tray 注册 Windows 自启（菜单项"开机自启"从预留改为可用）。
- app-server 轮询 60s 不变。

### 4.7 寻址与 IP 漂移

- 设备→桥：BLE `push_endpoint`（host=`lan_ip()`）按 MAC 刷新；窗口内自动跟随新网段；`lan_ip()` 多网卡选路列入 M2 验证。
- 桥→设备：`codex-status-XXXX.local`（MAC 后缀稳定）+ BLE info 缓存 IP 兜底。
- 信任：mDNS 仅寻址，接入以 bond/token 为准；`request.md` D6"零 mDNS"由本方案修订为"允许 mDNS 寻址"。

### 4.8 场景：回到已保存 Wi-Fi 的环境

- 桥已开机：IP 未变 → Wi-Fi 拉取成功；IP 变 → BLE 写新 endpoint + 推 usage，随后 Wi-Fi 可达；跨网段/隔离 → 走 BLE，保持 DEEP。
- 桥短暂重启：LIVE 驻留不动（GRACE 10min）；超时回 DEEP 窗口；桥启动后立即扫描。
- 桥未开机：保持 IDLE、正常入睡、不误入 AP；空窗退避；BOOT 短按强制开窗。
- 换网/回访：scan + last-used 选网；BSSID 变更判定；旧 active 立即抢占。

### 4.9 参数（定值）

| 参数 | 值 | 说明 |
|---|---|---|
| DEEP 窗口间隔 | 60s（前 3 窗）→ 300s；退避 900s | 连续 3 次失败进入退避，成功复位 |
| DEEP 窗口 | 15s | Wi-Fi 连接 + BLE 广播 |
| Wi-Fi 连接超时 | 8–10s | 单槽位 |
| 单端点超时 | 2s | 取数 |
| active T | 600s | envelope `active_hold_seconds` |
| IDLE 切换阈值 | 连续 2 窗失败 | 或从未同步 |
| LIVE 退出 | Wi-Fi 断 3min / 无同步 10min | 最小驻留 3min |
| DEEP 最短保持 | 5min | 防抖 |
| 桥扫描 | 5s/20s；12 轮未发现 → 5s/60s | 成功推送后暂停至指纹变化/5min |
| 推送心跳 | 5min | M3 |
| AP 空闲超时 | 5min | 之后入睡 |

## 5. 明确不做（非目标）

- BLE 连接穿过 light sleep（IDF 限制，已论证）。
- 无 PM 的常亮 LIVE 过渡形态。
- 桥侧键鼠空闲闸门。
- 唤醒计划公布（`next_wake_epoch`）；以扫描+退避替代。
- USB/电源自适应切换。
- 面板模式开关（仅 CLI 调试）。
- 第 4 个独立 idle 模板。
- 自动 AP 兜底（改为 D10）。
- 既有非目标：公网通道、BLE OTA、实时帧推送。

## 6. 实施计划

### M1 — 低风险修复与基础设施（固件 0.9.0）

1. 每次刷新后 `EPD_SSD1681_Sleep()`，绘制前重新 init，验证局刷不受影响。
2. BLE 仅窗口内广播，入睡前停播；`mode auto|deep|live` CLI。
3. 渲染去抖：数据指纹不变不刷；窗口/DEEP 下不同步分钟。
4. BLE info 附 IP/端口，并在 IP 变化时刷新（mDNS 已由 ArduinoOTA 提供，无固件改动）。
5. 桥：tray 开机自启落地；首次启动立即扫描。

DoD：`pio run` 通过；`epd_writes` 在无变化窗口不增长；`codex-status-XXXX.local` 可解析（回归验证）；`/status.json` 正常。

### M2 — 双模式 DEEP + IDLE 模板（固件 0.10.0 + 桥 + 模板）

1. 协议：元素 `mode`；idle bind 3 个；`idle_template`/`active_hold_seconds` 入 envelope；`min_fw 0.10`；四处同步（固件、`bridge/crates/core/template.rs`、Python 测试桥、`tools/generate-quad-preview.mjs`）。
2. quad idle 变体（§4.4 布局）+ `idle.json` 不新增。
3. 固件：睡眠解耦、RTC 状态、选网/BSSID、usage 缓存、IDLE 渲染与切换、AP 策略、失败/退避/加速窗口。
4. 桥：扫描节奏与退避、idle_template 配置与 pin、面板"待机模板"、MCP 字段、profile 附带下发。
5. 场景行为（§4.8）全部落地。

DoD：`pio run`、`cargo test --workspace`（隔离 target dir）、`node tools/generate-quad-preview.mjs` + `node tools/test-quad-preview.mjs`、`git diff --check` 全过；验收矩阵 T1–T8 通过；ROM 归档 + `PROGRESS.md` 记录。

### M3 — LIVE 轻睡（固件 0.11.0 + 桥）

1. 自编 core（Arduino as IDF component 或 lib-builder）开 `CONFIG_PM_ENABLE` + tickless，关 BLE 控制器。
2. 固件：LIVE 状态机、`POST /usage`（token + active 规则）、看门狗、DTIM 调参、兜底轮询。
3. 桥：mDNS 寻址 + `POST /usage`（指纹 + 心跳）。
4. 实测电流与延迟；不达标（>2mA 平均）则本里程碑不通过。

DoD：T9–T11 通过；LIVE 电流 ≤2mA；LIVE→DEEP 切换用例通过；ROM 归档 + `PROGRESS.md`。

## 7. 验收矩阵

| # | 场景 | 期望 |
|---|---|---|
| T1 | PC 关机过夜（DEEP） | `battery_mv` 降幅 ≤5%/夜 |
| T2 | 桥运行，窗口取数 | 窗口内 ≤15s 完成；成功即数据界面 |
| T3 | 数据无变化连续窗口 | `epd_writes` 不增长（无重绘） |
| T4 | 回访旧环境（桥 IP 变、换 BSSID） | 首/次窗同步；旧 active 立即抢占；不误入 AP |
| T5 | active 规则 | 超时抢占、显式覆盖、双机同/异账号、三机切换 |
| T6 | 桥重启 | LIVE GRACE 内不退出；DEEP 下桥启动后下一窗恢复 |
| T7 | IDLE 模板 | SYNC 不出现；左下 5H/BATT/STATE/OFF；切换只刷一次；缓存 usage 可见；无缓存时不渲染模板，显示内置默认界面 |
| T8 | 模板一致性 | quad 新 hash 三方一致；旧固件 `min_fw` 拒绝；预览逐像素 |
| T9 | LIVE 推送 | 变化后 ≤5s 显示；Wi-Fi 断 3min 回 DEEP |
| T10 | LIVE 电流 | 平均 ≤2mA |
| T11 | 回归 | token 401、bond 保持、`/status.json`、OTA |

## 8. 已知限制（接受，不再变更）

- active 盲区（A 桥常开、人换 B）→ 显式覆盖兜底。
- 元素坐标为静态 → 同一模板的多模式元素用 `mode` 变体复制。
- DEEP 唤醒为整机重启 → RAM 状态依赖 `RTC_DATA_ATTR`/NVS。
