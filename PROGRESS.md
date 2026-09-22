# Codex Status 项目进度（交接文档）

## 实机推进：2026-09-23 — 桥提速定稿 + bridge_first spike + 0.17.9 固件（未提交）

工作树推进，未提交。权威细节与证据见 `project-workflow/power-plan-c/status.md`（09-23 节）
与 `task-6-bridge-first-impl.md`（含 §1 spike 结果、§5 统一 A/B 排期）。

**桥（`bridge/crates/ble/src/lib.rs` 等，已重建运行）**
- task-2 §4 定稿：事件驱动发现（`adapter.events()`，命中即停扫描）、每次机会新建 adapter
  （Manager 为 ZST；复用同一 adapter 反复 start/stop 会累积 WinRT handler）、每周期
  `disconnect`+`discover_services`、ACK 20ms、`timings=[find,connect,discover,info,cmd]`。
- 实测否决两条路线（本机 Windows）：常驻 adapter+持续扫描（connect 隔次 4s 超时）、
  不 disconnect/跳 discover（同样隔次失败）。复现数据在 status.md。
- 新增 post-OTA 控制：`platform::post_ota_window`（coordinator `light_hold_until` + 300s
  显式 light 计划），OTA 成功（直连或排队 flush）后由桥接管在线窗口。

**bridge_first spike（task-6 §1，Go 通过）**
- host：`bridge/crates/ble/examples/adv-spike.rs`（dev-dep `windows`）——`Start→Started`
  P50 17.5ms、`Stop→首条广播` P50 98.9ms、发布期间 watcher ≈24 条/s；同机 watcher 收不到
  自身 beacon（Windows 过滤）。
- 设备第二接收端：`/diag?blescan=N&company=`（token 鉴权）+ 串口 `blescan N`；
  12s 扫描 `total=386 / matched=14 / scan_end=0`；beacon `company=0xFFFF`、
  **non-connectable**、AD 28B、应用 payload 恰 24B、无 name/UUID；证据
  `artifacts/blescan-device-2026-09-23e.json`。

**固件 0.17.3→0.17.9（已 OTA：0.17.2→0.17.5→0.17.6→0.17.7→0.17.8→0.17.9）**
- `0.17.5`：OTA 上传期 `esp_wifi_set_ps(WIFI_PS_NONE)`（CPU light sleep 早有 `otaPmLock`），
  修弱链路大上传被 modem sleep 拖垮的问题。
- `0.17.7`：post-OTA 5 分钟 light 窗口改用 **NVS** 标记（本板 RTC 内存不跨软复位），
  `/status.json` 增加 `post_ota_hold_s`（实测 ~255）。
- `0.17.9`：**修复“屏幕误报桥离线数小时”**——`markSynced()` 原先只在 legacy HTTP
  push/pull 调用，rv2 下 `rtcLastSyncEpoch` 冻结导致 `device.offline_mins` 误报；现 BLE
  已认证会合命令与 `/v2/status|data|plan` 都刷新（`last_push` 实测 6s 内）。
- blescan 实现坑（已修）：`NimBLEScan::start()` 单位是毫秒且**异步**（须等 `isScanning()`
  再摘回调）、扫描期 Wi-Fi PS 会饿死共存 BLE 扫描、诊断 JSON 根节点类型。
- ROM（`artifacts/codex-status-0.17.*.bin`，构建日志同名）：0.17.5 `6D46A6A0…`、
  0.17.6 `C1C43933…`、0.17.7 `9EF5981B…`、0.17.8 `31F7EA73…`、0.17.9 `36359E9D…`；
  0.17.3/0.17.4 本地构建未 OTA。

**现场与结论**
- 设备 `192.168.3.163` / `70041DD7A340`，现运行 **0.17.9-bw**、rv2=1；桥
  `bridge/target/debug`（parent 2032 / watchdog 50320，pidfile=2032，未提交重建件）。
- OTA 失败归因：与 ROM 大小/内容无关；失败为 `[ota] abort (aborted) err=0`（TCP 中断），
  ping RTT 5–12ms/1s 交替（PS listen=10）在弱信号下拖垮 1.7MB 上传；挪近后 rssi -42
  一次通过。0.17.5+ 的 PS-off 已把这条修掉。
- **未完成**：task-6 §2–§4（bridge_first 协议/切换实现）；A1–A5 统一 A/B（含 DFS 40/80
  与 btpm 全因子）；A5 需核对“会合偶见 2 分钟”的整分钟对齐问题。
- 下一步交接提示词：`project-workflow/power-plan-c/prompt-execute-3.md`。

## 设计同步：2026-09-23 — PowerPlan C 双策略会合进入权威 v2 设计（未实现）

- `docs/generic-display-platform-design-v2.md` 已同步两种可切换会合策略：默认/恢复用
  `device_first`（设备广播、PC central/GATT），候选 `bridge_first`（PC 每个预定窗口广播认证
  Directive，设备扫描并每窗口必回认证 StatusBeacon，再决定休眠或开放一次有界 Wi-Fi bootstrap）。
- bridge_first 的 OPEN_WIFI 后，PC 并行等待 StatusBeacon 与设备 HTTP 端口；回复漏收但认证 HTTP
  成功时继续共用现有 coordinator、owner、Data/Bundle/PowerPlan 与业务 ACK。广播不创建 owner、
  不续 light、不获得 BOOT provisional，设备保留独立 bootstrap 硬截止。
- 策略默认 device_first，经现有认证通道配置并由设备 ACK 后才切换；固定恢复窗口与 BOOT 始终可走
  device_first。Windows Publisher/Watcher 并发、单次 TX→RX 退化、31B 载荷/HMAC、密钥生命周期、
  多设备调度、丢包/重放/ACK 丢失均列为 spike 验收。
- 细化见 `project-workflow/power-plan-c/task-5-advert-rendezvous.md`。本轮仅同步设计与状态文档；
  **未修改源码、未构建、未 OTA、未重启服务、未提交**。

## 进行中：2026-09-22 — 第二硬件 target（4.2" 400×300 SSD2683 / ZecTrix Note4）

用户已确认面板事实：**4.2 英寸黑白、400×300、SSD2683**；按键 = 侧边 PGUP/PGDN、
正面 ENTER（原理图 net：`KEY_PGUP` / `KEY_ESP32_EN` / `KEY_ENTER`）。
本轮已完成（编译/宿主验证，未 OTA）：
- **几何参数化**：`template_engine`（`tplSetCanvas`）、`refresh_policy`（`rgnSetPanel`）、
  `platform_target.h`（`TARGET_WIDTH/HEIGHT/ROW_BYTES/FB_BYTES`）——同一引擎可服务
  200×200 与 400×300；200×200 宿主逐像素/区域一致性测试仍全绿。
- **驱动选择层** `src/epd_target.h`：`EPD_TGT_*` 别名按 target 选 SSD1681/SSD2683；
  `main.cpp` 已改为别名（200×200 行为不变）。
- **SSD2683 驱动骨架** `src/EPD_SSD2683.{h,cpp}`：400×300/1bpp（50B/行、15000B 帧）、
  窗口/双 plane/BUSY 传播/局刷窗口接口；仅 `CODEX_TARGET_NOTE4` 编译。
- **第二 ROM 环境（未启用）**：`platformio.ini` 的 note4 env、waveform LUT 与引脚
  全部以 `#error` 显式列出（不猜），故不加入默认 `pio run`。
- **ROM**：`artifacts/codex-status-0.16.8-bw.bin`（1 684 928 B，SHA256
  `7AA6A96C1B9B07B6501B7EA6C10DE758DBF1B5A52D25D0D34EC6F22AA09297A3`，含几何参数化，
  **未 OTA**；设备现场仍为 0.16.7 的 `687A611B…`）。

**继续所需的硬件事实（缺一不可，勿猜）**
1. ESP32-S3 侧 EPD GPIO：`EPD_SCK/EPD_MOSI/EPD_CS/EPD_DC/EPD_RST/EPD_BUSY/EPD3V3_EN`
   对应 GPIO 号（原理图放大截图或文字对照）。
2. 三个按键 `KEY_PGUP/KEY_PGDN/KEY_ENTER` 的 GPIO 号。
3. SSD2683 面板的时序参数（gate 数、方向/数据入口、border、温度曲线）与
   **两套 waveform LUT**（厂商样例/规格书），用于 `ssd2683_luts.h`。
4. 该板 flash/PSRAM 型号与容量（独立 ROM 的分区/帧缓存规划；400×300 1bpp 单帧
   15000B，A/B 双帧 + 编译产物仍需容量审计）。

拿到 1–4 后：填 `src/platform_target.h`/`DEV_Config.h` 的 NOTE4 映射 → 启用 env →
`TARGET_PARTIAL` 仅在波形/BUSY 实测后打开 → 模板 variant（`render_target=
epd-ssd2683-400x300-1bpp`）与 OTA 双端防错已在协议/桥侧就绪 → 逐项实机清单见
`project-workflow/generic-display-platform-implementation/status.md`。
桥侧 400×300 target 注册/画布校验/预览与 variant 路径尚未接线（下一步）。

## 实机验证：2026-09-22 — v2 平台在 200×200 SSD1681 设备上跑通（固件 0.16.7-bw）

设备 `70041DD7A340` / 192.168.3.163，桥为本次实现构建（`bridge/target/debug`）。
过程固件：0.15.10 → 0.16.0 → 0.16.7（每轮都是实机暴露问题后的修复，全部 OTA 验证）。

**已验证（实机）**
- **legacy 回归**：装 v2 Bundle 前 `[v2] no committed bundle; legacy template store active`，
  quad 正常渲染、区域策略 `n=13`、Wi-Fi push 正常。
- **完整 Bundle 安装**：BEGIN/CHUNK/COMMIT 提交成功；`v2_bundle=true`、3 模板、
  `commit_seq=2`、设备生成 context；变更模板后再次发布走另一槽（A/B），
  `commit_seq` 递增、context 重新生成。
- **数据投递**：`data_seq` 单调（1→5），`display=displayed`，`epd_writes` 递增；
  字段 CRC 与桥逐字节一致。
- **局刷与清影**：黑块反白数字变化 `refresh=partial/ok dirty=37`（未整块重刷）；
  连续 89↔90 多次后按预算升级为 `full/clean`（实机观察到阈值行为）。
- **正式 PowerPlan**：plan_id 1/4/6/7；`remaining_s` 单调递减（跨多次状态读取与一次数据推送
  不续租）；旧 plan_id（0）被 `stale_plan` 拒绝。
- **BOOT provisional**：`wake=ext1` 后 `prov=True prov_rem=276`（从物理唤醒起算）；
  桥保持原窗口下发 `granted=267`（不是新的 300），随后用新 plan_id 延长到 600。
- **deep 与 timer wake**：上下文在正常 deep 唤醒后保持同一 `active_context_id`；
  deep 期间排队的 push 在唤醒后的首个会合窗口投递（约 60–70s）。
- **A→B→A**：远程显式激活产生三个互不相同的 context。
- **OTA target 防错**：错误 target 返回 401，设备日志
  `[ota] rejected: target codex-status-154g-gray4 != codex-status-154g`，固件未变。
- **安装中断 + 掉电**：写入半个 Bundle 后深睡/重启，已提交包与 job 完好。
- **PM**：`light_sleep_counts=2822`、SLEEP 占比 79%，无 OTA/USB 锁泄漏。

**实机暴露并修复的问题**（全部已回归）
1. `/v2/*` 认证应为 endpoint token（桥业务通道），非设备操作 token。
2. BEGIN/COMMIT/ACTIVATE 的 `bridge_id` 在 JSON body 中（此前误读 query）。
3. Bundle 槽尺寸少算 12B 序列化头 → 读回长度校验失败。
4. `bsInstall` 的 9KB `CtTemplate` 落在 8KB loop 栈 → 栈溢出（int-wdt）。
5. `LittleFS.begin` 用默认 label 覆盖挂载标签 → `totalBytes()=0`、空间检查误拒。
6. Bundle 必须能在没有 context 时投递（它是 context 的来源）。
7. 设备空 `active_context_id` 不得被当作文成 context 采纳。
8. activate 成功后未清 `pending_activate` → 周期性重复激活/新 context。
9. plan 内容相同但窗口过期后必须换新 plan_id（否则无法重新授予 light）。
10. 有 Bundle 但无正式计划时需要设备侧 max light lease 兜底。
11. v2 有待投递数据时 legacy pull 响应必须回 light（否则 timer wake 立刻回 deep）。

- **桥不可达时的 BOOT 300s 兜底**（实机，桥停机）：`wake=ext1` 后 provisional 从 288 单调
  递减（Wi-Fi 已连、`http -1` 重试），到 `prov_rem=3`（≈t_boot+293s）后设备关闭无线并回
  deep，此后 ~1 分钟无响应；全程未接受任何正式计划（`plan=0`）。
- **桥重启后的恢复**：设备 timer 唤醒后保持同一 context；桥用已持久化的计数继续
  （`data_seq=9` 跳号被接受），并下发新的正式计划（id 7，600s），`display=displayed`。

**ROM**：`artifacts/codex-status-0.16.7-bw.bin`（1 684 864 B，SHA256
`687A611B6DF655A62A3F9314328DFD8FFFDEA0C8F5E7D8D51CABCBA6ED8250CB`，与当前源码重建一致，
已 OTA 到设备 ota_0/ota_1 轮换）；中间构建保留 0.16.0–0.16.6（sha 见各自 artifacts）。
**交互验证产物**：`artifacts/panel-*.png`（电脑摄像头拍摄：清洁全刷参考 + 局刷后对比；
自动面板定位置信度不足）。用户要求残影定量照片“后续再拍”，当前以
`partial/ok dirty=37`、连续变化后自动 `full/clean`、跨 deep 基线与零刷新作为软件证据。

**剩余（非阻塞）**：固定机位残影照片定量判定；新硬件 target（面板/控制器资料未到，
`blocked_by_hardware_arrival`）。


## 历史归档

2026-09-22 及更早的进度（通用平台 v2 实现、0.13–0.16 各轮实机修复、早期里程碑）已移至
`docs/history/progress-archive-2026-09-23.md`；追溯时按标题检索，不必整读。

