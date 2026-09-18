# task-1 — 固件 0.12.0：单模式运行/电源状态机

规格：`docs/power-state.md` §3–§8、§10。只动固件；宿主侧（Rust/Python/Node）
协议同步在 task-2，task-2 完成前不得 OTA。

## 目标

删除 DEEP/LIVE 双模式与窗口/退避/idle 渲染；实现常开单模式状态机：
AP / BLE ON / IDLE(`BLE OFF`) / WIFI OFF（插电·电池）/ 低电；空闲
light sleep（BLE deinit 后自动生效）；唯一三条主动 deep sleep 路径。
固件侧模板协议同步去掉 `mode`、`device.idle_reason`，`device.state` 输出
新状态字，`device.offline_mins` = 距上次成功同步分钟数。

## 状态机（实现口径）

| 状态 | 进入条件 | 无线 | 睡眠 | 状态字 |
|---|---|---|---|---|
| AP | 无凭据 或 BOOT 15s | Wi-Fi AP | 插电不睡；电池 5min 空闲深睡（无定时，按键唤醒） | `AP` |
| BLE ON | Wi-Fi 已连 ∧（单击 或 插电∧>20%） | STA + BLE | 不睡 | `BLE ON` |
| IDLE | Wi-Fi 已连、BLE 会话关 | STA（MAX_MODEM） | light sleep | `BLE OFF` |
| WIFI OFF·插电 | PC USB ∧ 失联 | STA 每 60s 重试 | 不睡 | `WIFI OFF` |
| WIFI OFF·电池 | 未插电 ∧ 失联 | 全关 | deep sleep 1min×3 → 5min×3 → 15min 永久；按键唤醒清零 | `WIFI OFF` |
| 低电 | 未插电 ∧ <5% | 全关 | `powerOff()`（GPIO17 低） | — |

- 插电 = `usb_serial_jtag_is_connected()`（`driver/usb_serial_jtag.h`，
  CONFIG_USJ_NO_AUTO_LS_ON_CONNECTION 已开；连接时 USB 自身持
  NO_LIGHT_SLEEP 锁）。轮询 ~500ms，变化即触发状态迁移与 LED 闪。
- BLE 会话：进入时 `bleBegin()`（若已 deinit 则重新 init），退出时
  `bleAdvertiseStop()` + `NimBLEDevice::deinit(true)`（释放控制器 PM 锁）。
  保活：插电∧>20% 无限；否则 `bleOffDeadline = now + 120s`；任何 BLE
  连接/写入活动重置 120s；掉到 20% 或拔线后同理。
- Wi-Fi：开机有凭据 `connectBest()` 最多 30s；失联判定 30s（断线或连不上）；
  插电失联每 60s `WiFi.reconnect()`；电池失联按 `retryStage` 深睡定时；
  ext1 按键唤醒（`bootWakeCause==EXT1`）清零 `rtcRetryStage`；恢复后
  插电∧>20% → BLE ON，否则 IDLE。
- 低电：IDLE/BLE ON 每 5min 检查；<5% 且未插电 → `powerOff()`。
- AP：沿用 portal；电池 5min 空闲 `deepSleepNoTimer()`（ext1 按键唤醒）。
- WIFI OFF 不建立 BLE 会话（失联时立即关闭 BLE）。

## 按键 / LED

- BOOT：单击（<2s 释放，1s 节流）IDLE↔BLE ON 切换；WIFI OFF 插电态→立即
  重试 Wi-Fi；AP 态无操作。2s：`nextTemplate()` + BLE ON（重置计时）。
  15s：AP 配网。30s：factory reset。保留 `bootArmed` 防误触。
- PWR 3s：`powerOff()`（不变）。
- GP3 绿灯低电平点亮：BLE ON 常亮；每次状态迁移闪一下（~80ms 取反）；
  Wi-Fi 60s 重试时闪一下。

## 固件协议（引擎/存储）

- `template_engine`：删 `TplMode`/`parseElementMode`/mode 过滤与校验；
  删 `B_DEV_REASON`/`device.idle_reason`；`TplEnv` 去 `idle`/`idleReason`，
  增 `String state`（状态字）；`B_DEV_STATE` 取 `env.state`（非空即存在）；
  `B_DEV_OFFLINE` 仅 `offlineMins>=0` 时存在。
- `template_store`：删 idle 槽（`sIdle`、index `"idle"`、`tplStoreIdle/
  SetIdle`、淘汰保护、清除）；旧 index 里的 `"idle"` 键忽略。
- `applyEnvelopeMeta(doc, mac)`：去 `idle_template`；`bridge.host/port`
  存在且 `mac` 匹配 endpoint 时更新该 endpoint 的 host/port（自愈）。
- `renderActiveUsage(json, channel)`：单渲染路径；env.state =
  `deviceStateText()`；offlineMins 由 `rtcLastSyncEpoch` 计算
  （`timeKnown()` 且 >0，否则 -1）。

## 删除清单

窗口/退避：`windowMode/windowDeadline/windowHardStop/windowSynced/
windowHadWifi/windowEnvSwitch`、`holdWindow`、`finishWindowAndSleep`、
`rtcFailCount/rtcFastLeft/rtcNeverSynced/rtcBssidHash/windowEnvSwitch`、
`WINDOW_*` 常量。双模式：`liveMode/liveEnteredAtMs/liveLastSyncMs/
liveLastPollMs/wifiLostSinceMs`、`enterLive/exitLive`、LIVE 分支、
`runtimeMode/setRuntimeMode/MODE_NAMES`、CLI `mode`。Idle：
`IdleReason/rtcIdleReason/idleReasonText`、`screenIdle()` 的 idle 语义、
`deepSleepFor(..., renderIdle)`、idle 存储槽、`/sleep` 路由与 CLI `sleep`、
`debugStayAwake/stay`、`windowSynced/fail_count/live/mode/idle_reason/
active_mac/active_at` 状态字段。死代码：`lastOkMs/bridgeOk/screenSig/
renderedIp/renderedMinute/renderedBattery/usageOnScreen/lastSyncMs/
lastSyncEpoch`（若被新 `rtcLastSyncEpoch` 取代）。
保留：`armWakeSources`（仅 WIFI OFF 电池 + AP 无凭据）、`powerOff`、OTA
（上传期间持 NO_LIGHT_SLEEP 锁）、`usageAccepted` 的 active-endpoint 选择、
`tryWifiUsage`、token 体系、模板 BLE/HTTP 传输。

## 新增

- `bleDeinit()`/`bleIsActive()`（ble_bridge）；`bleBegin` 需容忍重复调用。
- `setupPowerManagement()`：开机即 `esp_pm_configure(240/40MHz, light_sleep)`。
- UDP 通告：IP 变化、BLE ON 进入（`ble=1`）、每 ~5min 向
  `255.255.255.255:8767` 发 `{"magic":"codex-status","mac","ip","port":80,
  "proto":"http","ble":0/1,"fw"}`（`WiFiUDP`，独立 socket `begin(0)`）。
- `/status.json`：去 `mode/idle_reason/active_mac/active_at/fail_count/
  window_synced/live`；增 `state`（`AP|BLE ON|BLE OFF|WIFI OFF`）、`ble_on`、
  `plugged`、`wifi_state`（`ap|connecting|connected|lost`）、`retry_stage`、
  `last_push`（epoch，0=无）；保留 `pwr`、`pm_light_sleep`。
- CLI `pmstats`：`esp_pm_impl_dump_stats(stdout)` + `esp_pm_dump_locks(stdout)`
  经 `DevLog`（`/log`）与串口输出。

## 实现步骤（建议顺序）

1. 头文件/常量/全局状态替换：状态枚举、时间常量、`RTC_DATA_ATTR` 简化。
2. `ble_bridge`：`bleDeinit`、状态查询、活动回调接 `bleMarkActivity()`。
3. `template_engine` + `template_store` 协议删除与 `state` 绑定。
4. `main.cpp`：setup/loop 状态机、按键、LED、Wi-Fi/BLE 会话、深睡三条路径、
   低电、UDP、status.json、CLI、信封解析、渲染路径。
5. `pio run` 到通过；`git diff --check`。

## 自测（Coding Self-Tests）

- `pio run`（仓库根，勿 `-v`）exit 0。
- `git diff --check` 无输出。
- `git status --short` 只含预期路径。
- 静态检查：grep 确认 `windowMode|liveMode|idle_template|idle_reason|
  debugStayAwake|holdWindow` 在 `src/` 中不再出现（历史注释除外）。

## 独立验证（Task verification）

- `pio run` 由另一执行者复跑。
- `bridge-render`/`node`/`cargo` 不受影响（本任务不要求，task-2 覆盖）。
- 无法在无设备情况下验证电气行为；设备级验收在 task-5。

## 完成标准

- 固件编译通过，版本号 `0.12.0-bw`，源码中无本任务删除清单残留。
- 状态字/键位语义与 `docs/power-state.md` 一致，task-2 可直接对接。
- ROM 不归档（未 OTA）、不提交（等用户）。
- 建议提交信息：`Firmware 0.12.0: single-mode power state machine`
