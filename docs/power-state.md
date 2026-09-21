# power-state.md — v0.12 运行/电源状态机（设计已定稿，待实现）

状态：2026-09-18 需求讨论定稿，尚未动代码。实现计划随后写在
`project-workflow/`（v0.12 专项）。本文件是运行/电源行为的权威描述，
与 `AGENTS.md`、`PROGRESS.md` 冲突时以本文件为准（实现完成后回归更新）。
v0.12 已于 0.12.x 实现（§1–§12 描述现行固件）；v0.14 deep/light 双模式方案见
§13（2026-09-20 设计定稿，待实现；实现后 §3/§6 相应回归更新）。

## 1. 背景

- 旧架构 = DEEP/LIVE 双模式 + 窗口/退避 + 待机模板，状态多、难验证，且 0.11.x
  的 light sleep 实际从未生效。
- 根因（代码级）：IDF 中 S3 复用 C3 BLE 控制器，`CONFIG_BT_CTRL_MODEM_SLEEP=n`
  时控制器在 `esp_bt_controller_enable` 期间一直持有 `ESP_PM_NO_LIGHT_SLEEP`
  锁（`bt.c`：`no_light_sleep=1` → 创建并于 enable 时 acquire；日志原话
  “light sleep mode will not be able to apply when bluetooth is enabled”）。
  只要 NimBLE 初始化着，系统就进不了 light sleep。
- 2026-09-17 夜掉电 = 桥崩溃（Codex 自动升级 → app-server 退出 → tao panic）
  + `stay` 阻止睡眠 + 上述锁 → 设备整夜空转约 25–40mA。
- 新目标：**常开单模式**；空闲自动 light sleep（BLE 必须先 deinit）；不主动
  deep sleep（仅三条例外，见 §3）；BLE 只在会话期存在；T10 平均电流 ≤2mA。

## 2. 硬件事实（ESP32-S3-ePaper-1.54 V2，已对照官方原理图）

- 双色 LED 模块 L2：**绿 LED = LED_G = GP3**（3V3 → R3 24K → 绿 LED → GP3，
  **低电平点亮**）；**红 LED = ETA6098 STAT**（充电中点亮，固件不可控、未接
  任何 GPIO）。GP3 不能用于检测插电；GPIO3 是 S3 的 JTAG strapping 脚，
  仅上电采样，运行期当输出无副作用。
- `GP4 = BAT_ADC`（VBAT = VADC×2；百分比线性映射 3300mV=0% / 4200mV=100%）。
- `GPIO17 = BAT_Control` 锁存（HIGH 保持开机）；`GPIO18 = PWR` 按键；
  `GPIO0 = BOOT`；`GPIO6 = EPD 电源`；`GPIO42 = PA_EN`。
- **插电判定 = PC USB 主机存在（USB-Serial-JTAG SOF 检测）**；充电头不算插电。
  可靠性待实测（拔插；核心 SOF 看门狗有 3ms 容差）。
- `CONFIG_USJ_NO_AUTO_LS_ON_CONNECTION=y`（保持现状）：PC USB 连着时 USB 自己
  持有 `NO_LIGHT_SLEEP` 锁 → 芯片自动保持醒着，串口始终可用；不需要再改此开关。
- 板上无 VBUS/CHG 输入引脚（已在原理图确认）。

## 3. 状态机

| 状态 | 条件 | 无线 | 睡眠 | 屏幕状态字 |
|---|---|---|---|---|
| AP | 无凭据；或 BOOT 15s | Wi-Fi AP | 插电：不睡（USB 自锁）；电池：5min 空闲 → 深睡（无定时唤醒，按键才醒） | `AP` |
| BLE ON | Wi-Fi 已连 ∧（单击 或 插电+>20% 常开） | Wi-Fi STA + BLE | 不睡 | `BLE ON` |
| IDLE | Wi-Fi 已连、无会话 | Wi-Fi STA（MAX_MODEM） | light sleep（BLE 先 deinit） | `BLE OFF` |
| WIFI OFF·插电 | PC USB ∧ 失联 | Wi-Fi STA 每 **60s** 重试 | **不睡**（串口可用） | `WIFI OFF` |
| WIFI OFF·电池 | 未插电 ∧ 失联 | 全关 | deep sleep：1min×3 → 5min×3 → 15min 永久；按键唤醒清零重来 | `WIFI OFF` |
| 低电 | 未插电 ∧ <5% | 全关 | 断电（GPIO17 拉低） | — |

- 深睡醒来 = 整机复位：Wi-Fi 与 BLE 一同重新初始化（不存在"只开一个"）。
- **唯一主动 deep sleep 路径**：WIFI OFF·电池、AP 无凭据超时（无定时）、
  低电保护（实为断电）。其余时刻永不 deep sleep。
- WIFI OFF 时不建立 BLE 会话（Wi-Fi 不通时单独开 BLE 无意义，插电时同样）。

## 4. 按键与 LED

- **BOOT 单击**（<2s 释放；1s 节流，屏幕始终反映最终状态）：
  IDLE → BLE ON；BLE ON → BLE OFF（→ light sleep）；WIFI OFF（插电清醒态）→
  立即重试 Wi-Fi；AP 态无操作。
- **BOOT 2s**：切下一个本地模板（0.13.2 起不再顺带开 BLE 会话；需要 BLE
  用单击）。
- **BOOT 15s**：进入 AP 配网模式。
- **BOOT 30s**：恢复出厂（清 NVS/绑定/模板/凭据后重启进 AP）。
- **PWR 3s**：断电（现有逻辑，拉低 GPIO17；USB 供电时退化为重启）。
- **GP3 绿灯**：BLE ON 常亮；每次状态切换闪一下；WIFI OFF 重试时可选闪一下。

## 5. BLE 会话规则

- BLE ON 进入：初始化 NimBLE + 广播（名字 `CodexStatus-XXXXXX`）；状态字
  `BLE ON`、绿灯亮。
- 保活：**PC USB 插着 ∧ 电量 >20% → 无限常开**；充电回到 20% 自动恢复；
  掉到 20% 或拔线 → 保留 **120s**（任何 BLE 连接/传输期间不切断，断开后重新
  计时）→ 关 BLE；单击亦可立即关闭。
- 关闭 = 停广播 + `NimBLEDevice::deinit(true)`（释放控制器 PM 锁，light sleep
  的前提）；状态字 `BLE OFF`、绿灯灭。
- BLE 用途（一次性/按需，仅身份类）：配对与绑定、endpoint/token 下发、
  OTA token 取用。模板推送与日常数据一律 HTTP（`POST /template`，见 §9）；
  BLE 不再传输模板。
- 触发 bridge 握手：设备 UDP 广播带 `ble=1` 标志，bridge 收到后做一次定向
  BLE 连接完成 endpoint/token 下发即断开；不周期扫描。

## 6. Wi-Fi 策略

- 开机有凭据：尝试连接最多 30s；成功 = 拿到 IP（随后由 bridge 的 push 论证）。
- 失联判定：连续 **30s** 未连接（断线或连不上）。
- 插电失联：保持醒着，每 60s 重试；单击立即重试。
- 未插电失联：按 1min×3 → 5min×3 → 15min 永久 的定时唤醒重试；BOOT/PWR
  （ext1）按键唤醒同样重试并清零计数。
- 无凭据：AP 模式；电池下 5min 空闲后深睡（无定时；按键唤醒回 AP）。
- Wi-Fi 恢复后：插电且 >20% → 回 BLE ON 常开；否则回 IDLE。

## 7. 屏幕状态字与模板协议

- `device.state` 绑定值 = `BLE ON` / `BLE OFF` / `WIFI OFF` / `AP`
  （WIFI OFF 优先于 BLE ON；正常 IDLE 显示 `BLE OFF`）。
- **`when` 条件（0.13.5+）**：`{"bind":…,"exists":bool}`（值是否存在）或
  `{"bind":…,"equals":…}`（与绑定的渲染文本比较，支持字符串与整数，如
  `device.state` equals `"WIFI OFF"`）；两种键互斥且只允许 `bind` + 其一，
  未知键/类型整份拒绝。`equals` 模板需 `min_fw=0.13.5`。
- **`device.now`（0.13.6+）**：设备本地时钟 `HH:MM`（由 `server_time` 同步、
  RTC 走时；时间未知时不存在）。模板引用该绑定时设备每分钟局部重绘一次
  （与桥 5min 推送解耦）；不引用则维持"仅事件重绘"。**0.13.7 修复实现**：
  「是否引用」在模板加载时缓存（`activeTplHasNow`，勿每轮 `indexOf` 扫
  ~8KB 模板——会冲掉唤醒路径缓存、light sleep 碎片化）、分钟检查用
  `millis()` 门控 1Hz（勿每轮 `timeKnown()/time()`）；改回原写法实测
  SLEEP 26%/CPU_MAX 62%（正常 ~92%/7%）。
- **默认 quad（v9）状态图标**：右上第一行 `[BT][WiFi][Bridge][时间]`——
  时间为 `device.now`（每分钟走）；BT 仅 `device.state=BLE ON` 显示；WiFi 在
  `WIFI OFF` 显示断弧+`!`；Bridge 在 `device.offline_mins` 存在或 `WIFI OFF`
  时显示斜杠；原 `device.state` 文本行已移除，状态由图标表达。
- **移除待机模板功能**：删除 `mode: idle/live` 元素语义（模板引擎/Rust
  canonical/Python 测试桥三端同步），模板改为单布局；`device.idle_reason`
  绑定删除；`device.offline_mins` = 联系不上 bridge 的分钟数：桥有 5min
  心跳，设备在距上次成功同步 >6min（`BRIDGE_LOST_MIN`）后才提供该绑定
  （值为距上次同步的分钟数，含阈值前的时间），期间每分钟重绘；桥正常
  心跳时整行隐藏（刚同步的 0 也不会显示）。
- 模板 `full/mini/quad` 改完重推（哈希全变），三端哈希一致的不变量不变。
- 深睡期间 e-paper 保留 WIFI OFF 画面。

## 8. 低电保护

- 未插电且 `battery < 5%`：拉低 GPIO17 断电（保护电芯）。
- 电量检查周期：IDLE/BLE ON 每 5min 一次；WIFI OFF 深睡期间不做。
- 无充电状态引脚，充回 20% 的判定 = 插电中出现 `battery > 20%`。

## 9. Bridge 端改造

- **删除待机模板**：`core`（envelope/runtime/tests）、`app`（config/main/UI）、
  `mcp` 的 `idle_template` 字段与相关命令/下拉全部移除。
- **缓存设备最后状态**（fw、模板清单、endpoint、在线状态），面板显示用缓存。
- **不做周期 BLE 扫描**：BLE 仅按需一次性连接（UDP announce `ble=1` 或用户
  显式动作）；日常 `POST /usage` 走 HTTP；设备 `GET /usage` 也走 HTTP。
- **模板只经显式推送（HTTP）**：设备不在 boot、BLE 交接或在线稳态自动拉取
  模板；只有用户（面板）或 agent（MCP `profile_push`）显式推送时才传输并
  激活。传输走设备 `POST /template`（endpoint token 门控，body 为原始模板
  JSON，query 带 `id/version/hash/activate`；固件做 CRC/min_fw/dry-run 校验
  后落盘并重绘），推送前先 GET `/status.json` 比对 hash 跳过未变化模板。
  桥的 `GET /template` 仅供工具/调试；信封里的 `templates` 哈希表仅作参考。
- **UDP 通告**：设备拿到 IP 后及每 ~5min 向 `255.255.255.255:8767` 广播
  `{magic, mac, ip, port, proto, ble, fw}`；bridge 监听并更新对应 MAC 的
  endpoint；envelope 里带上 bridge 的 host/port，设备侧 endpoint 自愈。
- **双击 exe 即用**：不依赖脚本/计划任务；进程内拉起隐藏 watchdog 子进程
  （同 exe + `--watchdog <pid>`）：父进程 exit code 0 → 一同退出；非 0（tao
  panic 等）→ 重新拉起；5min 内连续 3 次异常退出则停止并写日志。
- 根因修复：app-server 断开不再走退出路径；spawn `codex.exe` 失败时重新发现
  新版路径（Codex 自动升级会换目录）。

### 9.1 设备身份与发现（device-discovery，0.13.4 起）

- **唯一键 = 设备 Wi-Fi MAC**（`/status.json.mac`、UDP 通告 `mac`、BLE info
  `mac` 同源），学习后持久化 `<exe>/data/bridge-app.json`；**显示名
  `device_name`** 仅桥本地、可重名、可改名（默认 `CodexStatus-<MAC 后缀>`）；
  IP 是可变属性。旧配置（只有 `device_ip`）照常工作，首次学到 MAC 时生成默认
  名写回；此后 MAC 不符的通告一律拒绝。
- **发现链**：属性 IP（HTTP）→ UDP 通告（`255.255.255.255:8767`，主路径）→
  ARP 按 MAC 扫本机 /24（`GetIpNetTable` 邻居表 + UDP poke + `SendARP`，
  `cfg(windows)`；HTTP 连续失败 ~20s 自动触发）→ BLE 读 info
  `{mac,ip,http_port}`（手动兜底：单击 BOOT 开会话；`ble=1` 通告触发的
  cycle 顺带采纳）。显式入口：面板「重新发现」/ MCP `device_discover
  {via: auto|arp|ble}`。发现即写回 `bridge-app.json` 并触发一次推送。
- **固件 0.13.4 关闭 mDNS**（`ArduinoOTA.setMdnsEnabled(false)`，无
  `MDNS.addService`）：失去 `.local` 与 `_http._tcp`（DHCP hostname/DHCP
  option 12 保留）；bridge/panel/MCP/device-auth 均不依赖 mDNS。

### 9.2 设备占用 claim/lease（可选的独占层，0.13.4 起）

- 设备侧 `owner = {id,name,host,port,since_s,last_seen_s,lease_s}` 存 NVS，
  **只由显式 `POST /claim`（token 门控）写入/清空**；`usage`/`template` 永不
  创建/转移 owner（仅刷新匹配 id 的 `last_seen`）。lease 到期只清空为空闲。
  写接口 owner 校验：无 owner → 完全按旧规则（含 activate/BSSID/timeout）；
  owner 有效且 `bridge.hostId`（template 用 `bridge_id`/`X-Bridge-Id`）不符 →
  HTTP 409 + owner，无例外。`/status.json.owner` 暴露占用者（空闲 `null`，
  含 `expires_in_s`）。
- **桥自动决策**：空闲/过期 → 主动 claim → 推送；己方 → 推送 + 每 60s 幂等
  续约（与推送解耦）；他人 → 不 claim、不推送，面板/MCP 显示占用者与
  "强制接管"；收到 409 只显示不静默重试。claim token 用缓存
  `<exe>/data/device-token.json`（设备 token），401/缺失提示单击 BOOT 走 BLE
  交接；显式动作用户可触发一次 BLE 取新 token。
- **用户动作**：强制接管、释放（release + 本地让步，不再自动 claim，直到
  "占用/恢复"）、重新发现；MCP `device_owner`（只读）/`device_claim{force?}`/
  `device_release`。旧固件无 `/claim`（404）时自动回退旧推送行为。

## 10. 固件删除清单

`windowMode`/`liveMode`/退避与窗口同步、idle/live 双渲染、`device.state=IDLE`
原因文本、`stay` 与 `/sleep`、CLI `sleep`/`mode`、`/status.json` 的 `mode`/
`idle_reason` 等窗口字段（改为 `ble_on`/`plugged`/`wifi_state`/`retry_stage`/
`last_push`）、待机模板存储槽。**保留**：`armWakeSources`（仅 WIFI OFF 电池态
与 AP 无凭据深睡使用）、`powerOff`、OTA（上传期间持 `NO_LIGHT_SLEEP` 锁）。
新增调试 CLI `pmstats`（`esp_pm_impl_dump_stats` + `esp_pm_dump_locks` 输出到
DevLog/`/log`），用于证明 light sleep 生效。0.13.0 起同一文本也可经只读
`GET /pmstats` 免 token 读取（与 `/log` 同级；实现见
`project-workflow/pmstats/`），bridge 面板「功耗」tab 与 MCP `pm_stats` 用它。
0.13.3 起 `loop()` 空闲轮询为 25ms（有 TCP 客户端/OTA 时自动回 5ms，见
`loopDelayForNow()`），把 light sleep 碎片从 ~134 次/s 降到 ~45 次/s；诊断
参数见 `project-workflow/pmstats/task-2.md`。0.13.8 起 Wi-Fi 自动 light sleep
的 modem 活跃窗口按 Kconfig 调优（实测 wifi PM 锁 48.8→24–32ms/次、无推送
窗口 SLEEP ~92–93%）：`CONFIG_ESP_WIFI_SLP_DEFAULT_MIN_ACTIVE_TIME=20`
（默认 50，ms）、`..._WAIT_BROADCAST_DATA_TIME=10`（默认 15，ms）、
`..._MAX_ACTIVE_TIME=60`（默认 10，秒，null-data keep-alive）。改这三项后必须
删除生成的 `sdkconfig.esp32-s3-epaper-154g` 再构建（kconfgen 对已有 sdkconfig
的值优先，不删不生效；会触发全量 core 重编，~16.5 min）。同版起 loop 尾部为
1Hz 合并心跳：离线分钟与 `device.now` 共用一次 `time()`，`serviceAnnounce`
的 IP 变化检查同样 1Hz 门控；详见 `project-workflow/pmstats/task-3.md`。

## 11. 验收

- **T10**：拔电 IDLE 态平均电流 ≤2mA（电池 `battery_mv` 斜率 + `pmstats`
  双证；对照 0.10.3 ~10%/h）。
- **T9**：light sleep 下 bridge `POST /usage` 成功且延迟可接受（唤醒延迟）。
- **状态机回归**：单击 BLE ON/OFF 屏幕可见、2s 切模板（不自动开 BLE）、15s AP、30s
  恢复出厂、GP3 指示；插电常开/拔电 120s；20% 宽限与自动恢复。
- **WIFI OFF**：拔 AP 后 30s 判定 → 深睡；1m×3/5m×3/15m 定时重试节奏正确；
  按键唤醒清零；插电态不睡、60s 重试、串口可用；屏幕 `WIFI OFF`。
- **模板**：三端哈希一致；去 `mode` 后 `device.state` 显示正确。
- **Bridge**：无周期扫描；UDP 更新 endpoint 生效（换 IP 场景）；watchdog 对
  tao panic/强杀能自愈；Codex 升级后自动重新发现 exe。
- **OTA** 全流程回归（light sleep 中可上传、token 401 行为不变）。

## 12. 风险/待验证

- PC USB SOF 拔插检测可靠性（实现后实测多轮；拔线 3ms 内失效）。
- UDP 广播的 Windows 防火墙放行（首次可能弹窗）与 LAN 内无认证风险（仅更
  新已存在 MAC 的 IP，可接受则不做签名）。
- light sleep 下 WebServer/OTA 的稳定性与延迟（HTTP 已实测：空闲 25ms 轮询下
  往返 ~0.3–0.5s；传输时回 5ms；OTA 全流程待下个版本回归）。
- USB 供电时 USB 锁使 light sleep 不可用属预期（插电本就不睡）。
- 实现顺序建议：固件状态机 → 模板协议去 `mode` → bridge 去待机/缓存/UDP →
  watchdog → 三端测试与整机验收。

## 13. v0.14 方案：deep/light 双模式 + 时钟区域直写（0.14.0 起已实现）

状态：2026-09-20 设计定稿并实现（固件 `0.14.0-bw`、桥 `activity` 模块）。
本节取代 §3 的"常开单模式"，§6 的 Wi-Fi 策略按本节修订。实现映射见 §13.8。

### 13.1 状态与优先级

| 模式 | 行为 | 功耗（估） |
|---|---|---|
| `deep`·时钟周期 | RTC 定时（60s）唤醒 → **只做时钟区域直写**（不联网）→ 深睡 | 地板 ~0.7–0.8mA |
| `deep`·网络周期 | 按 `next_contact_s` 唤醒 → 快连 Wi-Fi → `GET /usage` 反向拉取 → 按响应执行转场 | 每次 ~0.03–0.04mAh |
| `light` | 现行 light sleep 常连 + 桥推送（分钟时钟走 `device.now` 渲染） | 10–15mA（实测） |

优先级：低电断电 > BLE 会话 > OTA 窗口 > 手动唤醒 > 桥指令 > 本地兜底。
插电不参与 deep（保持 light + BLE 自动规则）；无 Wi-Fi 时时钟周期照走、
网络唤醒按 1m×3 → 5m×3 → 15m 拉长（§6 现有节奏），时间只在网络成功时同步。

### 13.2 时钟区域直写（A/B 已验证）

- **预留区域从激活模板计算**：扫描 `elements[]` 中 `bind=="device.now"` 的
  text 元素，取 `font`/`scale`/`x`/`y`（有 `rect`+`align` 按引擎规则换算）；
  最大串固定 `HH:MM`（5 字符）。**模板无该元素 → 不预留**（该情况下 deep
  分钟唤醒可直接省掉）。
- 窗口字节对齐（SSD1681 `setWindow` 用 `x>>3`）。当前 quad v9 实测：
  `x=161..195 y=9..20`（`f12` 7×12 → 35×12px，窗口 5×12B = **60B**）。
- **RTC 状态**（`RTC_DATA_ATTR`）：`valid`、`fontId`、`scale`、`xOff`、
  窗口 rect、窗口旧像素（60B）、`partialCount`。深睡不丢。
- **驱动新增**（`EPD_SSD1681`）：`WakePartialWindow` / `DisplayPartWindow`；
  Y 映射与整屏一致（`RAM y = HEIGHT-1 - screen y`），0x26 只用 RTC 旧像素
  回填窗口（不整幅回填）。
- **A/B 实测**（2026-09-20，`artifacts/clkwin-ab-log.txt`）：
  - A（现行：模板渲染 + 整帧局刷）**864.4ms**（862.2–866.3）；
  - B（窗口直写）**795.6ms**（793.9–797.0）= build 0.45ms + 面板唤醒/reset
    215ms + 局部波形 580ms + 睡 0.03ms；
  - 差异仅 ~69ms（渲染/diff 部分）；**波形与面板唤醒是固定成本，与窗口大小
    无关**。B 的价值是深睡可用（只需 60B RTC 状态，不需要 template/usage/
    帧缓存），不是提速。
- **残影**：deep 期间局刷计数存 RTC；到阈值（建议 60–120 次或每日一次）在
  网络窗口用缓存 usage 重渲 + 全刷清影（无网络时接受残影或强制一次网络窗口）。

### 13.3 拉取、转场与通知（桥决定，设备执行）

- **pull 请求**：`GET /usage`（Bearer endpoint token）带 `next_contact_s`
  （设备计划）。**pull 响应**：`mode=deep|light` + `next_contact_s`（桥覆盖）
  + `pending{ota,templates}` + `usage_rev`。
- **桥的 activity 判定**：用量指纹**实际变化**时间（不是轮询时间）+ 手动/模板/
  OTA 活动；有活动 → `light`，静默 → `deep`。带迟滞：升 light 后最少驻留
  5min，连续 10–15min 无变化才降。
- **light → deep**：设备本地计时（"无变化 X 分钟"）→ 回深前发一次 HTTP 通知
  （`POST /deep` 或 pull 带参）→ 深睡；通知失败照睡，桥按"预期联系时间 +
  宽限"判离线。
- **手动唤醒**：deep 下 BOOT 单击 → light（不常驻，走同一 idle 规则）；再次
  单击 → BLE 会话（`enterBleOn`）；关 BLE 后仍留 light。桥从 announce/pull
  得知设备醒来并重置 idle 计时。0.15.2 起唤醒流程：不显示 `Connecting:` 页，
  唤醒立即以正常模板替换 Zzz；Wi-Fi 图标由模板按 `device.state` 条件仅在连上
  后显示，进 deep 时 Zzz 恢复、Wi-Fi 图标消失；连接失败先恢复睡眠帧再按退避
  回 deep（quad v11）。
- **自适应拉取**：桥可达时恒为 `next_contact_s=60`（升 light 延迟 ≤1min；
  deep 期间同样按分钟接触）。`1m×3 → 5m×3 → 15m` 只用于**设备侧连不上
  Wi‑Fi/桥的失败退避**（`retryDelaySec`，成功后复位），不是桥的"安静期"决策。

### 13.4 桥侧改造

- 维护 `last_change_at` 与 `usage_rev`（指纹变化才 +1；指纹沿用现有
  `usage_fingerprint`，剔除 `server_time`）；pull 响应按它算 mode。
- **deep 期间推送预期失败**：不计 `push_fail_streak`、不告警；收到 pull /
  announce / claim 视为在线；lease 在 pull 机会里续或深睡期放宽。
- **OTA/模板排队**：deep 时不能主动推；pull 响应带 `pending`，
  设备保持窗口 + 起 server，桥在窗口内 `POST /doUpdate`（现有路径不变）。
- 推送信封同时带 `mode`（light 期间的降级通道；设备也可本地超时兜底）。

### 13.5 时间与持久化

- 深睡时间用 `esp_rtc_get_time_us()` 差分（RTC 域连续）；当前
  `CONFIG_RTC_CLK_SRC_INT_RC=y`（内部 RC，有漂移），可选
  `CONFIG_RTC_CLK_SRC_EXT_CRYS`（需确认板上 32.768k 晶振）或使用板上
  **PCF85063**（I2C，现固件未用）。
- RTC/NVS 清单：`mode`、`usage_rev`、`last_change`、`next_net_at`、
  `retryStage`、`epochAtSleep`+`rtcTimeAtSleepUs`、时钟 rect/旧像素/
  `partialCount`；NVS 兜 OTA/软复位。

### 13.6 功耗估算（按 A/B 实测修正）

时钟唤醒 ≈ boot + ~800ms 面板操作；网络唤醒另加关联/DHCP 1.3–1.5s + HTTP。
时钟周期 ~0.7–0.8mA。**注意（0.15.0 起）**：桥可达时 deep 也按 60s 接触
（§13.3 修订，900s 仅失败退避），网络窗口成为主要开销——每次 ~0.03–0.04mAh
× 1440 ≈ **45–55mAh/天**，400mAh 电池约 7–9 天（未含深睡底流）。此前
"网络 15min ≈ 3mAh/天、12–18 天"的估算仅在长时间失败退避时成立。

### 13.7 开放问题 / 风险

1. 面板 mode1 + reset 后 0x24/0x26 的长期保持（A/B 单轮通过，需长期观察）。
2. 窗口局刷残影累积由 `CLK_GHOST_LIMIT=90` 次触发网络窗口全刷控制；长期效果待观察。
3. `device.offline_mins`/`OFF N M` 与 claim/lease 在 deep 语义下：deep 期间推送
   失败不计 `push_fail_streak`；announce/pull/claim/HTTP 状态读都视为在线并清
   除"预期 deep"。
4. 旧固件/旧桥兼容：旧桥缺 `mode` → 设备按缺省保持 deep（§13.7 原条目）；
   旧固件忽略新字段，走原 light 推送路径。
5. 时区/夏令时（`device.now` 用 localtime）：每次 pull 成功都用桥
   `server_time` 强制校时，RTC 漂移只在两次联系之间累积；时区随桥
   `tz_offset_min`（0.15.0，§13.9），不再固定 `CST-8`。
6. 板级深睡底流未实测（功耗模型最大不确定度）；T6 待回滚/长测。

### 13.8 实现映射（0.14.0）

固件 `src/main.cpp`：

- `rtcMode`（RTC + NVS `pm`）默认 light；timer 唤醒且 `rtcMode=deep` 且未插电
  时：未到 `rtcNextNetAt` 走 `deepThinWake()`（`epdThinBegin` + `clockTickWake`
  + 睡到下一分钟），到期走 `deepNetworkCycle()`（`deepFastConnect` 缓存
  BSSID/信道 + `GET /usage?...` + 执行 `mode/next_contact_s/usage_rev/pending`）。
- 冷启动/EXT1（BOOT/PWR）/插电一律 light；light 空闲 `idleDeepS`（默认 600s，
  `POST /diag?idle_deep_s=N` 可调）→ `POST /deep` 通知桥后深睡。推送信封
  `mode:"deep"` 给 60s 宽限（`applyBridgeModeHint`）。
- 时钟窗口从激活模板的 `device.now` 元素计算（prefix/suffix 存在则不预留），
  上限 64B；`clockTickWake` 同时用于 light 分钟 tick（省去整帧 diff）。
- `pending.ota`/`pending.templates` 非空 → 保持在线 `DEEP_PENDING_WINDOW_MS=180s`，
  桥在窗口内 POST /doUpdate 或 /template。
- `/status.json` 新增 `mode/idle_deep_s/next_contact_s/next_contact_in_s/usage_rev/
  clk/clk_partials/deep{clock_wakes,net_windows,net_fails,clock_ticks,last_code,...}`；
  串口 CLI 新增 `deep`/`light`。

桥 `bridge/crates/core/src/activity.rs` + app 接线：

- `usage_rev` 仅在指纹真变化时 +1（poller）；`last_change_at` 还被显式动作
  （模板保存/推送、claim、手动同步）刷新。
- pull 决策：pending 或被占/5min 驻留（`LIGHT_HOLD_S=300`）或静默 <600s →
  `light`；静默 ≥600s → `deep`；两种情况 `next_contact_s` 都是 **60**（拉长到
  15min 只是设备侧 Wi‑Fi 失败退避，桥不主动拉长）。
- `POST /deep`、pull、announce、HTTP 状态读、claim、推送成功都 `note_contact`；
  deep 预期离线时 push 跳过且不计失败。推送信封加 `mode/next_contact_s/usage_rev`。
- 排队：deep 期间 `profile_push`/`firmware_ota` 失败即入队（`pending`），下一个
  pull 接触由后台任务重放（模板直接 POST；OTA 走原 MCP 流程）。设备只在自身
  pull 时醒来，故排队最坏等一个 `next_contact_s`。
- `GET /usage` 无 pull 参数时行为与旧版一致（兼容）；Python 测试桥实现同样的
  pull/`POST /deep` 语义。

### 13.9 0.15.0：桥时钟/时区同步 + 深睡切换历史

- **pull 响应的 `server_time` 取响应生成时刻**：桥 `http.rs` pull 分支用当前
  时间覆盖缓存信封里的轮询时间（缓存可能滞后数十秒~分钟）；推送信封也在发送
  时重写 `server_time`/`tz_offset_min`。设备"每次接触强制校时"语义不变。
- **`tz_offset_min`**（本机 UTC 偏移，分钟，东为正）：桥 pull + 推送都带；
  固件 `applyTzOffsetMin()` 按 POSIX 反向符号生成（+480 → `UTC-8:00`）并
  持久化 NVS `pm/tz`；`/diag?tz=` 仍可手动覆盖，下次接触被桥值更新。缺省
  `CST-8`，旧桥缺字段则保持现值；`configTzTime()` 不再硬编码 `CST-8`。
- **深睡切换历史**：RTC 内存环 120 条 × 12B（`epoch` u32、`ev` u8、`stage`
  u8、`batt` u8、`aux` u16），掉电清零、零 flash 磨损。记录点：boot/唤醒分类、
  enter-deep（aux=`next_contact_s`）、每分钟 thin、网络窗口 net-ok/net-fail
  （aux=HTTP code，未知 0）、to-light。`GET /history`（JSON 数组，时间序，
  `?since=<seq>` 增量）；`/status.json` 暴露 `hist_count`（最新记录序号）与
  `hist_head`（下一写槽），深睡 RAM 日志丢失后仍可取证。
- 事件码：1 boot（aux=`esp_sleep_wakeup_cause_t`）、2 enter-deep、3 thin
  （aux=1 已画时钟）、4 net-ok（aux=200）、5 net-fail、6 to-light
  （aux=1 按钮唤醒）。
