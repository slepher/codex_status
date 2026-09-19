# power-state.md — v0.12 运行/电源状态机（设计已定稿，待实现）

状态：2026-09-18 需求讨论定稿，尚未动代码。实现计划随后写在
`project-workflow/`（v0.12 专项）。本文件是运行/电源行为的权威描述，
与 `AGENTS.md`、`PROGRESS.md` 冲突时以本文件为准（实现完成后回归更新）。

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
参数见 `project-workflow/pmstats/task-2.md`。

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
