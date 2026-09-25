# sleep-modes 实现状态（2026-09-20 深夜二）

> 新窗口交接请看 `prompt.md`；本文是详细进度/证据。

## 结论速览

- **T1/T2/T3（固件）代码完成**，双模式/时钟直写/反向拉取/离线重试已实现；
  但**电池深睡唤醒路径尚未通过现场回归**（见 §3），修复已进 0.14.2 待验证。
- **T4/T5（桥/Python/协议）代码完成**，`cargo test --workspace` 全绿、
  Node quad 测试全绿；真实设备端到端（pending 排队、pull 决策）未跑完。
- 固件已迭代到 **0.14.2-bw**（USB 刷入，当前 light/插电在线）；回滚 ROM
  `artifacts/codex-status-0.13.8-bw.bin`。
- 未提交（等用户确认）。

## 1. 固件（src/main.cpp、src/EPD_SSD1681.*、src/template_engine.*、src/usage_client.*）

版本：0.14.0（双模式首版）→ 0.14.1（面板电源保持）→ **0.14.2（瘦唤醒防卡 + 阶段码）**。

- 双模式：`rtcMode`（RTC+NVS `pm`）；默认 light，空闲 `idleDeepS`（默认 600s，
  `POST /diag?idle_deep_s=N` 可调）→ `POST /deep` 通知桥 → 深睡。
- 深睡窗口：`deepNetDue()` 到期走 `deepNetworkCycle()`（缓存 BSSID/信道快连 →
  `GET /usage?next_contact_s=&mode=deep&usage_rev=` → 执行 `mode/next_contact_s/
  usage_rev/pending`）；未到期走 `deepThinWake()`（`epdThinBegin` + 时钟窗口直写）。
- 时钟窗口：从激活模板 `device.now` 计算（无 prefix/suffix），60B（quad v9/v10）；
  light 分钟 tick 也走直写；`rtcClkPartials>=90` 在网络窗口全刷清影。
- 新绑定 `device.mode`（deep/light）；deep 渲染时 `device.state="DEEP"`（隐藏
  WIFI OFF 斜杠图标）。
- 睡眠图标：`enterDeep()` 在 rtcMode 切到 deep 后**立即重绘**（Zzz 出现）；
  deep→light（pull 返回 light）在 `startNormalMode(true)` 后重绘隐藏。
- 0.14.1：`holdPinsForDeepSleep()` = `gpio_hold_en(6/17)+gpio_deep_sleep_hold_en()`
  （保面板电源与 VBAT latch）；wake 时 `releaseWakeHolds()`。
- 0.14.2：`deepSleepRaw()` 只做 armWake+hold+睡，**thin 路径不再调用
  WiFi.disconnect/BLE/面板驱动**；`deepSleepFor()` 里 `WiFi.getMode()!=WIFI_MODE_NULL`
  才 disconnect。RTC 阶段码 `rtcStage`/`rtcLastWake` 随 `/status.json`
  暴露为 `stage`/`last_wake_code`。
- `/status.json` 新增：`mode/idle_deep_s/next_contact_s/next_contact_in_s/usage_rev/
  clk/clk_partials/stage/last_wake_code/deep{...}`；CLI 新增 `deep`/`light`。
- usage 缓存只在内容真变化时写 NVS（5h 100% 的滚动 `resetsAt` 不再触发）。

## 2. 桥（bridge/crates/{core,app,mcp,render}）

- `core/activity.rs`：`usage_rev`（指纹变化才 +1，`resetsAt` 在 used==0 时剔除）、
  `last_change_at`、接触事件、迟滞（light 驻留 300s，静默 600s 降 deep）、
  pending 模板/OTA 队列、`expects_deep()`、`snapshot()`。
- `core/http.rs`：pull 检测（query 有 `next_contact_s|mode|usage_rev`）→ 回
  `mode/next_contact_s/usage_rev/pending` 并打 `device pull:` 日志；新增
  `POST /deep`；普通 GET 行为不变（兼容）。
- poller 每次刷新 envelope 时 `note_envelope`（`usage changed (rev N)` 日志）。
- app 推送循环：deep 预期跳过推送（`device expected deep; pushes paused`，不计
  fail streak）、接触后强制补推、push 信封带 `mode/next_contact_s/usage_rev`、
  `contact_generation` 变化触发补推；pending 冲刷任务（模板直推；OTA 走原 MCP
  流程重放）；`profile_push`/`firmware_ota` 在 deep/不可达时排队并返回“已排队”。
- `device.mode` 加入 Rust 校验（`core/template.rs` + tests）与渲染 FFI
  （render/ffi.cpp、lib.rs Env、render main `--mode`、mcp env_from_args）。
- 测试：隔离 target `artifacts/cargo-target-sleepmodes` 跑 `cargo test --workspace`
  全绿；`node tools/test-quad-preview.mjs` 全绿（含 deep 图标断言）。
- 运行桥：debug 构建，父 PID 25184 + watchdog 33412，:8765/:8766/:8767。

## 3. 未解决问题（P0）

**电池深睡后设备失联**：根因 #1（唤醒断电）已定位并修复（0.14.4）；
根因 #2（pull 成功后卡死）定位中（0.14.5 细码）。完整证据链见 §6。

- 0.14.0：深睡后花屏 + 不可达（面板 RAM 丢失 + 窗口局刷）。已定位补 0.14.1 的
  GPIO6 hold；屏幕问题在 0.14.1/0.14.2 冷启动后未复现。
- 0.14.1 电池回归：20:19:11 `POST /deep` 后 8 分钟无 pull、BOOT 无反应，直到
  power-on/插 USB 恢复（RTC 计数清零，无法事后取证）。
- USB 插电下用 CLI `deep` 验证：**定时唤醒本身正常**（52s 后唤醒并按插电路径
  light 启动、连网成功）。
- 0.14.2 的防卡改动与阶段码**尚未在电池上验证**。验证步骤见 `prompt.md`。
- **0.14.4 电池实测：首个 pull 成功、thin 唤醒成功，但 pull 收尾路径卡死
  （NVS=12）**；0.14.5 加细码待复现定位（见 §6）。
- 屏幕图标：用户报告 Zzz “覆盖了蓝牙标识”（可能是 BT 在深睡渲染时未被擦除）。
  未取证（需照片/复现），可能与 0.14.0 花屏同源，也可能只是共用格子的观感。

## 4. 现场与证据

- 设备：`192.168.3.163`，SSID `wd21-la`（BSSID 已变为 `E2:DA:CF:50:15:5C`），
  0.14.2-bw（ota_0，next ota_1），QUAD v10 active（hash `86a51357`），
  owner=桥 `8c94`；USB 串口 COM4。
- ROM：`artifacts/codex-status-0.14.2-bw.bin`（1619392 B，SHA256
  `B5162217F6504052818D4F26A6C2E6023DDC467F4968C4132332085C68F5F00B`）；
  回滚 `artifacts/codex-status-0.13.8-bw.bin`（`2E5CF982…1C5A`）。
- 模板：`tools/test-bridge/templates/quad.json` 与运行时
  `bridge/target/debug/data/templates/quad.json` 均为 v10（min_fw 0.14），
  Zzz 图标 16px @ (101,6)，与 BT 图标同格互斥条件。
- 图标生成器 `artifacts/gen-sleep-icon.py`（手工像素网格，另有 12/20px）；
  预览 `artifacts/quad-preview-deep.png`、`icons-sleep-sheet-10x.png`。
- 关键日志：`bridge/target/debug/data/logs/bridge-app.log.2026-09-20`
  （`device entered deep sleep`、`device pull:`、`device expected deep; pushes paused`）。

## 5. USB 刷写 / 串口要点

```powershell
# 只写 app 到 ota_0 + 清 otadata（保留 NVS：Wi-Fi/token/endpoint；勿刷 factory 镜像）
$env:PYTHONIOENCODING="utf-8"
pio pkg exec -p tool-esptoolpy -- esptool.py --chip esp32s3 --port COM4 --baud 921600 `
  --before default_reset --after no_reset write_flash --no-progress -z `
  0x10000 .pio/build/esp32-s3-epaper-154g/firmware.bin
pio pkg exec -p tool-esptoolpy -- esptool.py --chip esp32s3 --port COM4 --baud 921600 `
  --before no_reset --after hard_reset erase_region 0xd000 0x2000
# 串口日志（pio 自带 pyserial）
& "$env:USERPROFILE\.platformio\penv\Scripts\python.exe" -c "import serial; ..."
```

- GBK 控制台会让 esptool 进度条崩溃：必须 `PYTHONIOENCODING=utf-8`。
- `firmware.factory.bin` 会把 NVS 抹成 0xFF（Wi-Fi 密码丢失），不要整片刷。
- 设备 token 缓存在 `bridge/target/debug/data/device-token.json`（`3212570061…dbd8`）。

## 6. P0 调查记录（2026-09-20 深夜三，0.14.3–0.14.5；供代码审核）

### 6.1 现象与取证难点

电池深睡后失联：屏幕时钟冻结、BOOT/EXT1 无反应、桥无 pull；恢复只能插 USB，
且恢复 boot 是 `wake=power-on`（RTC 域掉电）。因此 RTC 阶段码（`stage`）在事后
必丢，此前的 0.14.1/0.14.2 现场都无法定位卡点。

### 6.2 诊断设施（0.14.3 加入）

- `POST /diag?...&nvs_stage=1`（需 token）把 RTC 标志 `nvsStageEnabled` 置 1；
  `nvsStageMark(code)` 在阶段跃迁时把码写进 NVS `pm/stg`（**默认关闭**，避免
  常态 NVS 磨损；单 boot 写上限 `NVS_STAGE_MAX_WRITES=40`）。
- 开机时读 NVS 到 `nvsStageAtBoot`，经 `/status.json` 暴露为 `nvs_stage_boot`。
- 细码：setup `1/4/5`（read NVS 之后）、enterDeep `19/21/22/20`、
  thin `2/30/3`（30 = epdThinBegin 返回）、deepSleepRaw `90`、
  网络窗口收尾 `41..47`、deepSleepFor `48/49/50`、`51`=即将 sleepToNextEvent。
- 实现位置：`src/main.cpp` 顶部 RTC 阶段码区（`nvsStageMark`/`setStage`）、
  `handleDiag`、`/status.json`、`releaseWakeHolds`、`deepNetworkCycle`、
  `deepSleepFor`、`enterDeep`、`deepThinWake`、`setup`。

### 6.3 已确认根因 #1：唤醒时释放 GPIO hold 拉低 VBAT 锁存 → 断电（0.14.4 修复）

- 0.14.3 实测 `nvs_stage_boot=90`：写完"准备深睡"后再无推进（连 thin 的 `2`
  都没写）→ 怀疑唤醒即掉电，而非软件卡死（BOOT 无效、插 USB 为 power-on、
  USB 供电下 CLI `deep` 唤醒正常，全部吻合）。
- 机理：GPIO17 = BAT_Control（HIGH 锁存开机，`powerOff()` 拉低即断电）。
  deep 睡眠期间 6/17 被 `gpio_hold_en` + `gpio_deep_sleep_hold_en` 冻结；唤醒后
  GPIO 数字域复位、输出寄存器=0，而 `releaseWakeHolds()` **先放 hold 再设电平**，
  GPIO17 被瞬间驱动 LOW → 锁存释放 → 断电。
- 修复（0.14.4）：先 `pinMode(17,OUTPUT)+digitalWrite(17,HIGH)`、
  `pinMode(6,OUTPUT)+digitalWrite(6,LOW)`，再 `gpio_hold_dis`/
  `gpio_deep_sleep_hold_dis`（无毛刺释放）。
- 验证（0.14.4 电池实测）：13:19:42 `POST /deep` → 13:20 thin 唤醒（码 2/30/3
  已写入）→ **13:21:08 桥收到 `device pull: requested_next=60 rev=0 ->
  mode=deep next=900 pending_tpl=0 pending_ota=false`**（首个真实 pull 成功）。
  断电问题不再出现。日志：`bridge/target/debug/data/logs/bridge-app.log.2026-09-20`。

### 6.4 根因 #2（已修）：pull 成功后卡死 = 重复 `epdBegin()` 触发 SPI 自锁

- 0.14.4 那轮 pull 成功后设备再次失联；插 USB 恢复后 `nvs_stage_boot=12`。
- `12` 位于 `deepNetworkCycle()`：pull HTTP 200 + JSON 解析成功之后；
  而 `90`（下一次入睡）与后续 thin 的 `2/30/3` 都没有写入 → 卡死在
  `adoptServerTimeForce/markSynced` → 渲染块（`epdBegin(false)`/
  `renderActiveUsage`）→ `usageCacheSave` → `sleepToNextEvent` →
  `deepSleepFor`（`epdPanelSleep`/BLE/`WiFi.disconnect`）这一段内。
- 0.14.5 细码：`41`(时间/sync) `42`(解析完成) `43`(进渲染前) `44`(epdBegin 后)
  `45`(renderActiveUsage 后) `46`(capture/cache 后) `47`(DevLog 前)
  `48`(epdPanelSleep 后) `49`(BLE 后) `50`(WiFi.disconnect 后)
  `51`(setup 里即将 sleepToNextEvent)。
- **0.14.5 现场读回 `nvs_stage_boot=43`**（44 未写）→ 卡在渲染分支的
  `epdBegin(false)` 调用处。独立代码审核定位到机制：
  - setup 在进 deep 分支前已调用 `epdBegin(!woke)`（`src/main.cpp` setup 段）；
  - `deepNetworkCycle()` 渲染分支再次 `epdBegin(false)` → `DEV_Module_Init()`
    → 第二次 `SPI.beginTransaction()`；Arduino `SPIClass::beginTransaction`
    取非递归 `paramLock`（`xSemaphoreTake(portMAX_DELAY)`），而本项目从不
    `endTransaction()`（写走 `_inTransaction` 的非锁路径）→ **同一任务自锁**，
    无界阻塞。thin 唤醒正常、首次 pull HTTP 200 全部吻合。
- 修复（0.14.6/0.14.7）：
  1. `DEV_Config.cpp`：`DEV_Module_Init()` 用静态标志保证 SPI 初始化/bus 事务
     每 boot 只做一次（GPIO/Serial 照旧）。
  2. `src/main.cpp`：渲染分支不再重复完整初始化，仅在 `!frame`（OOM）时重试
     `epdBegin(false)`；`epdBegin()` 内缓冲分配改为仅当指针为空时分配（修掉
     重复初始化泄漏 2×5000B 的隐患）。
- 验证（0.14.6 电池 & 0.14.7 插电 + `deep_usb`）：pull 成功（14:38:09，
  `mode=deep next=900`）后 thin 唤醒继续、时钟持续更新；复位后读
  `deep={clock_wakes:13, net_windows:1, net_fails:0, last_code:200}`。

### 6.5 复现 / 取证流程

```powershell
# 插电在线时开启诊断并缩短空闲阈值（token 见 status.md §5）
curl.exe -s -X POST "http://192.168.3.163/diag?idle_deep_s=60&nvs_stage=1&token=3212570061e4d23600173aa04108dbd8"
# 拔 USB -> 等 1–3 分钟：桥日志应出现 device entered deep sleep / device pull:
Get-Content bridge/target/debug/data/logs/bridge-app.log.2026-09-20 -Tail 20
# 卡死后插 USB（power-on）-> 读卡点
python -c "import urllib.request,json;d=json.load(urllib.request.urlopen('http://192.168.3.163/status.json',timeout=5));print(d.get('nvs_stage_boot'))"
```

注意：桥的推送会重置设备 idle 计时（`noteActivity("push")`），若长时间不进
deep，可临时停桥（先 stop watchdog 子进程 33412/51124 再停父进程；用
`pwsh tools/start-bridge.ps1` 重启）。恢复插 USB 时本板是 power-on，RTC 丢，
NVS 保留 —— 这正是面包屑选 NVS 的原因。

### 6.6 版本与 ROM

| 版本 | 内容 | ROM / SHA256 |
|---|---|---|
| 0.14.2-bw | 瘦唤醒（`deepSleepRaw`）+ RTC 阶段码 | `artifacts/codex-status-0.14.2-bw.bin`（`B5162217…F00B`） |
| 0.14.3-bw | NVS 面包屑 + `nvs_stage_boot` | `artifacts/codex-status-0.14.3-bw.bin` `14A8EC1703CACAAF47806CBF3274F547CE584FDA2768F3EE50E4F78FC0C7BEC5` |
| 0.14.4-bw | 无毛刺释放 GPIO hold（根因 #1 修复） | `artifacts/codex-status-0.14.4-bw.bin` `CA2634113D279B3C97FE3686A72133FF5BF8D81A3541C0FC960C32B94D3D479E` |
| 0.14.5-bw | 网络收尾细码 41–51（读出卡点 43） | `artifacts/codex-status-0.14.5-bw.bin` `C3D1E484C1E03E947F749443A6CE39D53BA35610D799DFE31A9211E9C6899A00` |
| 0.14.6-bw | `DEV_Module_Init` 幂等 + 渲染分支去重初始化 + 缓冲单次分配（根因 #2 修复） | `artifacts/codex-status-0.14.6-bw.bin` `693CE5BB8DF76D481FADDEC6817574431B0B85D962C1D7F8313B3B17A3E8D2F7` |
| 0.14.7-bw | `deep_usb` 插电深睡开关 + `deep_now`（测试提效） | `artifacts/codex-status-0.14.7-bw.bin` `B963E166FE85B11988289C08A89096FAB9F833FD152CCFD8F2659AF6952F3F90` |
| 0.14.8-bw | 时区修复（默认 `CST-8`，NVS `pm/tz`，`/diag?tz=`） | `artifacts/codex-status-0.14.8-bw.bin` `F9797CC64BBDA6211BCBE9FF5CB8695DBA1F1A98F60E608C776EFC4962324213` |
| 0.14.9/10/11 | `/frame` 帧导出、`render_mode`、OA 细码诊断（中间版） | `…0.14.9-bw.bin` `090C2FBB…6FC82`、`…0.14.10-bw.bin` `B01375C3…2868`、`…0.14.11-bw.bin` `4CC2CF0B…78AF` |
| 0.14.12-bw | OTA 中止清理 + 停滞看门狗 + `/diag?ota_abort=1`；上传前预清理（桥） | `artifacts/codex-status-0.14.12-bw.bin` `DD10BC1D96CF46D8899CA53F1D985259A0395136C7446BA4840835C82AD1D135` |
| 0.14.13-bw | 进 deep 睡眠图标渲染前强制全刷（P1 修复）；当前版 | `artifacts/codex-status-0.14.13-bw.bin` `F6D9C2F085FF32A09A8A36DC66531CEAE46225F951EB1EB7803BAA0E58E05FCF` |

回滚 ROM：`artifacts/codex-status-0.13.8-bw.bin`（`2E5CF982…1C5A`）。

## 7. P1 结案与 OTA 修复（2026-09-21 凌晨）

### 7.1 P1：进 deep 后 Zzz 不显示 / 与 BT 残影叠加（0.14.13 修复）

- 取证：`frame_capture=1` 存睡前帧（`/frames/last.pbm`），`GET /frame?which=saved`
  取回；比对模板图标区 **0/256 像素差**，`deep.glyph=5`
  （bit0=模板含 `device.mode`，bit2=渲染返回）证明"渲染确实执行且帧内容正确"。
- 物理确认后定位为**局刷基线漂移**：深睡唤醒 `epdThinBegin` 的面板电源脉冲会
  重置 SSD1681 控制器，而固件 `lastDisplayedFrame` 仍按旧内容计算差异 → 图标区
  被跳过或与 BT 残影叠加（同一机制也解释早期的"Zzz 覆盖蓝牙标识"）。
- 修复：`enterDeep()` 渲染睡眠图标前 `epdPartialReady = false;` → 走 0xC7
  全刷，保证面板物理内容与帧一致。用户现场确认：**睡眠中 Zzz 显示、时钟每分钟
  更新**。
- 另一处观察（非 bug）：浅睡→深睡后一分钟内设备会按计划 pull，若桥的安静期
  不足 `QUIET_DEEP_S=600` 会回 `light` → 设备回在线、Zzz 被 light UI 覆盖。
  调试时用 `device_mode deep` 可固定 deep（见 §7.3）。

### 7.2 OTA 逻辑缺陷与修复（0.14.12 + 桥）

- **固件**（`src/main.cpp` `/doUpdate`）：
  - 原实现忽略 `UPLOAD_FILE_ABORTED`：客户端中断后 `Update` 永久处于
    "already running"，`begin()` 永远失败、OTA 锁不释放、界面停在 OTA 画面，
    此后所有上传失败。现中止/写入失败/结束失败统一
    `otaUploadCleanup()`（`Update.abort()` + 解锁 + `renderCurrent()` 恢复界面 +
    DevLog）。
  - 新增 20s 停滞看门狗（`otaLastDataMs`）与 `POST /diag?ota_abort=1` 远程急救。
- **桥**（`bridge/crates/mcp`、`app`）：
  - OTA 前探测 `device_firmware()` 2s → **10s + 重试一次**（实测 light sleep +
    Wi-Fi 省电下 `/status.json` 冷响应 3–14s，2s 常误判离线并落入队列）。
  - 检查响应体 `UPDATE FAILED`（HTTP 200 也可能是失败），不再白等 60s。
  - 上传前先 `POST /diag?ota_abort=1` 清理旧残留（新固件生效）。
  - 队列 flush 失败退避：60s→2m→4m→…≤30m；新 pending 立即重试（不再每次接触
    都撞上传）。
- 验证：`firmware_ota 0.14.12 → 0.14.13` 一次直传成功；此前的"卡 OTA 界面"
  由 `Update` 残留态导致，需复位一次后修复版才可上传。

### 7.3 桥侧调试工具（MCP，默认不改行为）

| 工具 | 作用 |
|---|---|
| `device_sleep` | 强推 `mode=deep` 且后续 pull 保持 deep（跳过 10 分钟迟滞），设备 60s 宽限后睡 |
| `device_wake` | pull 响应固定 `light`，设备回在线可读 `/status.json`、`/log`、`/frame` |
| `device_mode auto\|deep\|light` | 恢复/强制 pull 与推送信封的 mode（调试用，`auto` 即原逻辑） |
| `device_contact_s s` | 覆盖 pull 下发间隔（30–3600s，0=自动） |
| 设备 `/diag` | `deep_usb=1`（插电可睡）、`deep_now=1`（1.5s 内睡）、`render_mode=deep\|light`（在线渲染深睡帧）、`frame_capture=1`（睡前帧存 `/frames/last.pbm`）、`nvs_stage=1`（NVS 面包屑） |
| 设备 `GET /frame` | `?which=frame`（当前帧）/`last`（已推面板）/`saved`（睡前捕获），PBM（1=黑） |

快速闭环：`device_contact_s 60` + `device_mode deep` → 设备 `/diag?deep_now=1`
→ 桥日志看 `device entered deep sleep`/`device pull: ... mode=deep next=60`
→ 需要读数时 `device_wake`；结束后 `device_mode auto`/`device_contact_s 0`。

### 7.4 下一窗口任务（新发现与遗留）

1. **时钟校准**：pull 响应的 `server_time` 目前取自 poller 信封（滞后数十秒~
   分钟），设备每次苏醒按它校时 → 与 PC 有时间差。改为在 pull 响应生成时用
   `now` 覆盖 `server_time`（`bridge/crates/core/src/http.rs` pull 分支；
   推送信封可选）。
2. **时区随 PC**：桥在 pull/push 带 `tz_offset_min`（本机 `Local` 偏移），固件
   生成 POSIX `TZ`（注意符号反向：UTC+8 → `UTC-8:00`）并持久化 NVS `pm/tz`；
   `CST-8` 仅缺省。
3. **P2 端到端**：pending 模板/OTA 排队冲刷、迟滞升降级、手动唤醒/BLE 打断/
   桥不可达/token 失效/低电/旧桥兼容（用 §7.3 工具驱动，不必拔线）。
4. **深睡切换历史（用户要求，规格已定）**：RTC 内存环 120 条（≈1 分钟 1 条
   → 2 小时，掉电清零可接受，零 flash 磨损、无需开关）；记录 boot/enter-deep/
   thin/net-ok|fail/to-light，字段 epoch/ev/stage/batt/aux(~12B)；
   `GET /history` JSON + `/status.json` 的 `hist_count/head`。详见 `prompt.md`
   任务 4。
5. **长测**：深睡 ≥2h 时钟准度（修复 #1 后再评估）、残影、电量斜率。
5. **提交前整理**：诊断设施（`frame_capture`、`nvs_stage`、`device_*` 调试
   工具）建议保留但默认关；审阅 `DEV_Module_Init` 幂等与全刷新增的影响。

### 6.7 测试/诊断开关（0.14.7/0.14.8）

- `POST /diag?deep_usb=1`：允许**插电时进 deep**（`rtcDeepOnUsb`，RTC 保存跨
  deep 周期）。任何非 TIMER 复位（power-on/OTA/BOOT 按键）自动清零——即
  "重启/手动唤醒后重置"；也可 `deep_usb=0` 或串口 `deepusb off` 关闭。
  开启后插电时跳过 BLE auto-on，使 idle 计时可以到点入睡。
- `POST /diag?...&deep_now=1`：1.5s 后立即进 deep（配合 `deep_usb` 时插电也可
  复现整个 deep 循环，不必拔线）。
- `POST /diag?tz=CST-8`：设置并持久化 POSIX TZ 串（NVS `pm/tz`）。
- 串口：`deepusb on|off`；`/status.json` 新增 `deep_usb`、`tz`。
- 现场验证流程（免拔线）：`deep_usb=1&deep_now=1` → 桥日志看
  `device entered deep sleep` / `device pull:` → 屏幕时钟每分钟更新 → 读
  `deep{}` 计数；要停止测试按 BOOT（回 light 且开关自动清零）。

### 6.8 附带调查结论（用户提出）

- **15 分钟网络间隔不是 Wi-Fi 失败退避**：`retryDelaySec()`（1m×3→5m×3→15m，
  `src/main.cpp`）只用于失败路径；安静期的 15min 来自桥
  `QUIET_CONTACT_S=900`（`bridge/crates/core/src/activity.rs`），成功后
  `markSynced()` 清零 `rtcRetryStage`。现场读回 `retry_stage=0`、
  `net_fails=0`、`next_contact_s=900` 印证。
- **时钟显示 14:45 ≠ 本地时间**：固件从未 `setenv("TZ",...)`，桥
  `server_time` 是 UTC epoch（`envelope.rs`），`localtime()` 按 UTC 渲染。
  0.14.8 修复：默认 `CST-8`、NVS 可持久化、`/diag?tz=` 可改；修复后屏幕
  显示本地时间（用户现场确认）。

## 8. v0.15.0：时钟/时区随 PC + 深睡切换历史 + P2 端到端（2026-09-21 凌晨二）

现场：设备 `0.15.0-bw`（ota_1，USB 插电、light、`tz=UTC-8:00`）；桥 = debug
构建（新代码，`tools/start-bridge.ps1` 重启后 PID 45532/52908）。

### 8.1 桥：pull/push 时间戳与 `tz_offset_min`

- `bridge/crates/core/src/http.rs` pull 分支用 `stamp_pull_response()` 以响应
  生成时刻覆盖缓存信封的 `server_time`（poller 缓存可能滞后数十秒~分钟），并
  加 `tz_offset_min`（本机 UTC 偏移分钟，东为正）；`crates/core/src/lib.rs`
  新增 `now_secs()`/`local_offset_minutes()`（chrono `Local`）。
- app 推送信封同点重写 `server_time`/`tz_offset_min`（`main.rs` push 循环）。
- 实测：`GET /usage?mode=light&...` → `server_time` 与墙钟差 **0s**，
  `tz_offset_min=480`。
- 单测：`stamp_pull_response`（fresh epoch + tz 范围）、`is_pull`；Python 测试桥
  `make_usage()` 同步带 `tz_offset_min`（按本机 `time.timezone/altzone`）。

### 8.2 固件：时区随桥

- `applyTzOffsetMin()`：POSIX 反向符号（+480 → `UTC-8:00`），范围 ±14h，
  变化才写 NVS `pm/tz`；`adoptServerTime`/`adoptServerTimeForce` 都调用
  （后者返回是否变化用于强制重渲）；`startNormalMode()` 的
  `configTzTime(deviceTz, ...)` 不再硬编码 `CST-8`。
- 实测：桥重启后设备 `tz` 从 `CST-8` 变 `UTC-8:00`；OTA 往返（0.15.0→
  0.14.12→0.15.0）后仍保持（NVS）。

### 8.3 固件：深睡切换历史（RTC 环）

- `HIST_CAP=120` × `HistRec{epoch u32, ev u8, stage u8, batt u8, aux u16}`
  = 12B（`static_assert`），RTC_DATA_ATTR，掉电/OTA 清零。
- 事件码：1 boot（aux=wake cause）、2 enter-deep（aux=next_contact_s）、
  3 thin（aux=1 已画时钟）、4 net-ok（aux=200）、5 net-fail（aux=0 未知）、
  6 to-light（aux=1 按钮唤醒）。
- `GET /history`（免 token，时间序，`?since=<seq>` 增量）；`/status.json` 加
  `hist_count`/`hist_head`。
- 实测一轮深睡历史与 `deep{}` 计数一致（见 §8.4）。

### 8.4 P2 端到端（真机，调试工具驱动，免拔线）

- **pending 模板冲刷**（`device_sleep`/`device_mode deep` + `deep_usb=1` +
  `deep_now=1`）：deep 期 `profile_push ab-0133` → 桥"已排队"→ 17:02:07 pull
  `mode=light next=60 pending_tpl=1` → 17:02:15 `queued template push flushed:
  pushed 1 template(s) ... (mini; activated)` → 设备 `templates` active=mini；
  随后 `profile_push default` 恢复 quad。
- **深睡历史取证**：seq1 boot / seq2 enter-deep(aux=60) / seq3 timer boot /
  seq4 net-ok(200) / seq5 to-light 与 `deep.net_windows=1`、`last_code=200`
  一致；另一轮含 thin（ev3 aux=1，画了时钟）。
- **迟滞 leg1（活动→light）**：17:11:05 模板推送（note_activity）→ 17:13:30
  强制 deep → 17:15:08 pull `mode=light next=60`（安静期 <600s）。
- **迟滞 leg2（静默→deep）**：17:37:09 pull `mode=deep next=900`（当时实现）。
  **用户随后纠正**：900s 只属于设备连不上 Wi‑Fi/桥的失败退避，桥可达应恒为
  60s。已改 `activity.rs`（deep 分支也回 60）、Python 测试桥与单测，
  `docs/power-state.md` §13.3/§13.6/§13.8 同步修订；长测按 60s 节奏执行。
- **OTA 排队**：deep 期 `firmware_ota 0.14.12` → "已排队" → 17:08:08 pull
  `pending_ota=true` → 17:08:49 `firmware OTA ok: 0.15.0-bw -> 0.14.12-bw`；
  再升回 0.15.0（一次直传成功）。
- **断桥重试**：深睡停桥（17:54:45–17:57:54）→ 17:56:00 pull 失败
  `net-fail`（ev5, stage13, aux=0）→ 17:57:00 thin → 重启桥后 17:58:00
  `net-ok`（ev4, 200）；`retry_stage=0`、`net_fails=1`。
- **兼容**：无 `tz_offset_min`/`mode` 的旧信封 POST `/usage` → accepted 且
  `tz` 保持 `UTC-8:00`；错误 token → 401。
- **leg2 结果/长测**：见 §8.6。

### 8.5 0.15.0 ROM

`artifacts/codex-status-0.15.0-bw.bin`（1629552 B，SHA256
`FCB9CF7560B367D1B9004B2FB424098FE5C9F56DAB1B27A8FF6F6658932771FA`）。

### 8.6 待办

1. 长测 ≥2h（60s 网络节奏下的电量斜率、时钟准度、残影、`/history` 逐分钟
   thin 条目；`docs/power-state.md` §13.6 已按 60s 接触修正功耗估算）。
2. 手动唤醒（BOOT→light、再击→BLE）与 BLE 打断需用户现场操作。
3. 提交前整理（默认关的调试开关、`/history` 暴露面审阅）。
