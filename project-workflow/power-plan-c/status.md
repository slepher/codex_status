# Plan C — status

Updated: 2026-09-22
Status: task-1/2/3 已代码落地并实机上线；固件已推进到 0.17.9-bw（OTA），桥已重建重启，rv2=1；
发现并修复了 0.16.9 收敛引入的 v2 Data 校验回归与 rv2 离线显示同步遗漏。30–60 分钟基线
（task-3）、task-6 实现与 A1–A5 A/B（含 DFS/btpm）待跑。提交：`99ef3c0`（上一基线 `e1e93fc`）。

## 当前状态

- 第一阶段目标：回复/超时后渲染一次、带时间校时、DFS min 80MHz、第二阶段 BT modem sleep。
  实际等待连接 3s、连接后另给 6s，尚不满足总 wake 目标，见下方实机记录。
- task-1/2/3 实现见下文；最后部署记录为 0.17.2 / rv2=1，桥已重建。此处为记录核对，未重新探测现场。
- task-5 双策略需求与候选协议已整理并同步权威 v2 设计，需 Windows/空口/安全 spike，尚未实现或启用。
- 本计划不改变安全语义（token/claim/owner），不做 light 会话优化。

## 任务状态

| Task | 状态 | 备注 |
|---|---|---|
| task-1 固件窗口/渲染/校时/时钟修复/DFS | 已上线（0.17.1，后续 0.17.2） | 含连接期窗口扩展；见下实机记录 |
| task-2 桥时间下发/连续扫描/窗口去重 | 已上线；§4 提速已实测定稿（见 09-23 节） | 事件驱动发现 + 每周期 disconnect/discover + 20ms ACK；常驻 adapter/扫描与跳 discovery 实测失败 |
| task-3 遥测与验收 | 埋点+预估工具已上线；30–60min 基线 pending | 实测 wake 6.2–8.6s 待调参；见下功耗预估 |
| task-4 BT modem sleep 实测定值 | 三臂 PM 驻留短测完成；板级电流待测 | 2026-09-28 Note4 A/B/C 各 34/32/34 deep 周期，回答 33/32/30；B 为下一轮电流仪候选，C 需复测连接率；见 task-4 末节 |
| task-5 双策略会合/窗口对齐 | 权威设计已同步，需 spike | device_first / bridge_first 独立 A/B；未实现 |
| task-6 bridge_first 实现 + 统一 A/B | spike 首轮完成（见 09-23 节） | 计划见 `task-6-bridge-first-impl.md` |

## 现场记录

- 设备：192.168.3.163 / MAC 70041DD7A340；本文件末次部署记录 0.17.2-bw 已 OTA（rv2=1 沿用前次记录）；
- 桥：`bridge/target/debug` 已重建重启，下文部署时记录 parent 15012 / watchdog 56212；不是本轮实时进程检查；
- 原 0.16.7/rv2=0、14:03 旧桥及 0.16.9 未 OTA 是启动基线，已被后续部署记录取代；
- 实测结论与日志进 `artifacts/`，不入库。

## 实现与证据（2026-09-23，task-2 §4 定稿 + bridge_first spike）

### 桥侧实现（`bridge/crates/ble/src/lib.rs`，已提交 `99ef3c0`）

- **发现**：`find_device` 改为事件驱动（`adapter.events()`，命中即 `stop_scan`），替代
  400ms 轮询；每周期新建 adapter（`Manager` 为 ZST，Windows watcher handler 无法注销，
  复用同一 adapter 反复 start/stop 会累积 handler）。
- **实测否决两条“提速”路线（本机 Windows）**：
  - 常驻 adapter + 持续扫描 + connect 期间不停扫描 → 隔次 connect 两次 4s 超时（≈50% 丢窗）；
  - 不 disconnect/复用 GATT 缓存并跳过 discover → 同样隔次失败；
  - 结论：每周期 `disconnect` + `discover_services` 是硬要求；INFO 读取改为每会话探测
    （MAC/绑定仍每次核对），缓存只在不破坏身份校验时才有意义。
- **保留**：ACK 轮询 20ms（原 80ms）；connect 首败重试；55s 去重；分阶段计时
  `timings=[find,connect,discover,info,status,data,plan]` 入 debug 日志。
- **实机**：桥 15:52:56Z 重启后至 16:13Z 所有会合窗口成功（个别窗口内首败重试后恢复），
  分阶段典型值 find 0.3–2.9s（等窗口）/ connect 0.26–1.0s / discover 0.23–0.35s /
  info 0.03–0.05s / 命令 0.01–0.15s；原始日志
  `bridge/target/debug/data/logs/bridge-app.log.2026-09-22`。

### bridge_first spike（task-6 §1；`bridge/crates/ble/examples/adv-spike.rs`）

- `Start→Started` P50 17.5ms / P95 22.3ms；发布期间 watcher 持续收包（≈24 条/s）；
  `Stop→首条广播` P50 98.9ms / P95 109.4ms；状态序列 `Waiting→Started→Stopped`；
- 同机 watcher 收不到自身 beacon（Windows 过滤），非连接性/实际 31B AD/首包 P50 需第二接收端；
- 发布 170s 期间 device_first 会合 1/1 成功（connect 282ms）。
- 证据：`artifacts/adv-spike-2026-09-23.jsonl`、`artifacts/adv-spike-hold-2026-09-23.jsonl`。

### 回归

- `cargo test --workspace` 全绿（`artifacts/cargo-test-powerc-task2s4-2026-09-23.log`）；
- `node tools/test-quad-preview.mjs` 7 fixtures 通过；`git diff --check` 通过；
- 新增行 `rustfmt --check` 干净（既有 drift 未动）。

### 固件与链路修复（0.17.3–0.17.9，均已 OTA）

- **离线显示修复（用户可见）**：`markSynced()` 只在 legacy HTTP push/pull 调用，rv2 下
  `rtcLastSyncEpoch` 冻结 → 屏幕 `device.offline_mins` 连续数小时误报；0.17.9 起 BLE
  已认证会合命令与 `/v2/status|data|plan` 都刷新同步时间（`last_push` 实测 6s 内）。
- **OTA 可靠性**：上传期 `esp_wifi_set_ps(WIFI_PS_NONE)`（CPU light sleep 早有
  `otaPmLock`）；post-OTA 5 分钟 light 窗口用 **NVS** 标记（本板 RTC 内存不跨软复位），
  实测 `post_ota_hold_s≈255`；桥侧 `post_ota_window` 另发 300s 显式 light 计划
  （plan_id 86/88/90 ack）——固件保底 + 桥控制两层。
- **诊断入口**：`/diag?blescan=N&company=`（token）与串口 `blescan N`；修了
  `NimBLEScan::start()` 毫秒/异步语义、扫描期 Wi-Fi PS 饥饿、JSON 根节点类型三个问题
  （见 task-6 §1.2）。
- **ROM**：0.17.5 `6D46A6A0…`、0.17.6 `C1C43933…`、0.17.7 `9EF5981B…`、0.17.8
  `31F7EA73…`、0.17.9 `36359E9D…`（`artifacts/codex-status-0.17.*.bin`）。
- **OTA 失败归因（实测）**：与 ROM 大小/内容无关；失败时设备日志
  `[ota] abort (aborted) err=0`（TCP 中断），ping RTT 5–12ms/1s 交替（PS listen=10），
  rssi -72 时 1.7MB 上传在 131KB–1MB 处断；挪近/重启后 rssi -42 一次通过。

### 待办

1. task-6 §2–§4 实现（spike Go；设备侧接收端已具备）；
2. A1 桥提速 30 周期统计与设备 `/history`（需一次 light 会话）；
3. A2/A3 DFS×btpm、A5 节奏、A4 双策略按 task-6 §5 统一排期。

## 实现与证据（2026-09-22，task-1/2/3）

### 固件（`src/main.cpp`、`src/ble_bridge.cpp`）

- **会合窗口**：`V2_RENDEZVOUS_WINDOW_MS=3000`、`V2_RENDEZVOUS_ACK_GRACE_MS=200`
  （`main.cpp:110-112`）；`v2Rendezvous()` 到点/收到 plan 后 200ms 关窗、不重试（`main.cpp:3815`）。
- **渲染时机**：`v2InRendezvous` 抑制 `enterBleOn`/`bleOff` 的 `renderCurrent`
  （`main.cpp:3161/3177`）；`v2RendezvousRender(light)`（`main.cpp:3880`）：
  - BLE 数据在窗口内只入库不渲染（`v2WakeRenderPending`，ACK `display_state=pending`，
    `main.cpp:3501`），窗口关闭后 `data+clock` 一帧；
  - light 计划时该帧即移除 Zzz 的唤醒基线（`wakeBaselineDrawn=true`），无数据时时钟
    并入 light 首帧（startNormalMode）；
  - sleep 计划只写保留时钟窗 `v2RendezvousClockRender()` → `clockTickWake()`
    （失败/首帧无基线时回退全刷，`main.cpp:3847`）。
- **时钟停更修复**：会合唤醒不再跳过渲染——回复/超时后必有一次时钟渲染（同上）。
- **校时**：`serviceV2Ble` 对已认证命令调用 `adoptServerTimeForce`
  （`server_time`+`tz_offset_min`，`main.cpp:3779`）；字段缺失/无回复保持本地 RTC。
- **广播**：`adv->setMinInterval(0x30)/setMaxInterval(0x60)`（30–60ms，`ble_bridge.cpp:330`）。
- **DFS**：`cfg.min_freq_mhz` 40→80（`main.cpp:372`）。
- **遥测**：`bootRenderCount`（每次波形写入 +1，含时钟窗）、`bleRadioMark/bleRadioMs`
  （`enterBleOn`/`bleOff`/`deepSleepFor` 记账）、`deepSleepRaw` 把 `rtcLastAwakeMs/
  rtcLastBleMs/rtcLastRenders/rtcLastTimeSource/rtcLastWakeResult` 存 RTC 并写
  `HIST_WAKE`（`main.cpp:1987`）；HistRec 12→16 B（`dur_ms`/`src`），
  `/status.json` 新增 `awake_ms`/`ble_on_ms`/`render_count`/`time_source` 与
  `deep.last_*`（`main.cpp:2389/2443`），`/history` 新增 `dur_ms`/`src`，HTML 状态页同步。

### 桥（`bridge/crates/ble/src/lib.rs`、`bridge/crates/app/src/main.rs`）

- `stamp_clock()`：每条 v2 BLE 命令（status/data/plan）写入新鲜 `server_time` +
  `tz_offset_min`（`lib.rs:442`）；单测 `rendezvous_commands_carry_the_bridge_clock`。
- v2 BLE 机会循环 5s→**750ms**（`main.rs:2342`）；`V2Connection::connect` 现返回
  `Ok(None)`=设备未广播（未发起连接），`ble_cycle` 返回 `BleOpportunity`；桥在
  `Attempted` 后 55s 去重（一个 60s 会合周期最多一次连接尝试），`NoDevice` 立即再扫。
  扫描未命中保持 debug，不污染桥状态。
- “无工作也连一次发 sleep plan”沿用 `plan_for_rendezvous`；HTTP 交付仍记
  `transport=http`、BLE 交付记 `transport=ble`（复核无错标）。

### 实机部署与验证（2026-09-22 夜，本地 UTC+8）

用户拍板：FW_VERSION `0.17.0-bw`→实际落地为 `0.17.1-bw`、立即 OTA、桥重建并重启。
现场设备已从 0.16.7-bw 升级，`rv2=1` 已开启；桥为本次重建（含 task-2 全部改动）。

- **ROM**：
  - `artifacts/codex-status-0.17.0-bw.bin`（1 699 888 B，SHA256
    `1E555CD174FBB9E76193AA0554190E0669E53B48428123615AEF7B79C22FC93D`）——
    已 OTA，但随即发现 v2 数据校验回归（见下），**已被 0.17.1 取代**。
  - `artifacts/codex-status-0.17.1-bw.bin`（1 700 048 B，SHA256
    `A0932C0F657E55EFDCDE74A690E2D2286D3F81563087D263600ED3D340C5D43E`）——
    **当次运行版本，后由 0.17.2 取代**（OTA 成功：`0.17.0-bw -> 0.17.1-bw`）。
  - `artifacts/codex-status-0.17.2-bw.bin`（1 700 640 B，SHA256
    `74414AEAEC1F5A1096F288B7FE253BAC4CD8D65C3BF9057B2334289CFAC511E0`）——
    **功率预估埋点版，已 OTA**（新增 `render_ms`/`light_sleep_ms` 与
    `deep.acc_*` deep 周期累计；见 task-3 §7 与 `tools/estimate-power.mjs`）。
    日志：`artifacts/build-plan-c-0.17.2-base.log`。
  - 日志：`artifacts/build-plan-c-0.17.0-base.log`、`artifacts/build-plan-c-0.17.1-base.log`。
- **桥**：`cargo build -p bridge-app`（1m02s/14s 增量）后按约定“先 watchdog 后父进程”重启；
  现进程 parent `15012`（pidfile 已更新）/ watchdog `56212`，以 `RUST_LOG=...debug` 运行以留诊断。
- **发现并修复的真实回归（0.16.9 收敛提交引入、从未实机验证）**：
  `src/v2_runtime.cpp` 的 `applyEntries` 要求 `fields.size()==ct.reqCount`（含 device.*
  本机 requirement）且索引连续，而桥 `set_contract` 只发非本机字段 → 所有 Data 被
  `error:"incomplete"` 拒绝。0.16.7 无此校验所以此前实机通过。已改回“远端字段完整快照”
  语义（按远端计数 + 索引有序 + 本机条目拒绝），并新增 host 回归测试
  `remote_data_snapshot_covers_all_non_local_requirements`（`bridge/crates/render/tests/v2_state.rs`
  + FFI `codex_v2_accept_data_template`）。
- **会合窗口实测定值（Windows central 太慢）**：3s 硬窗口在 Windows 上 connect 常失败/
  超时，设备会在 `discover/read info/write` 中途被关链路。新增
  `V2_RENDEZVOUS_CONNECTED_MS=6000`：3s 仍约束“等桥”，连接成功后给握手 6s 有界预算，
  plan ack 后仍 200ms 关；桥侧 connect 失败立即重试一次 + 循环 250ms。
- **实机证据（桥日志 UTC，设备 /status.json、/history）**：
  - `v2 rendezvous complete transport=ble`：14:00:05、14:05:12、14:09:07（UTC，
    即本地 22:00/22:05/22:09），期间 `light` 会合 0 次、无 `net_fails`；
  - BLE 小数据：`v2 data acknowledgement ... seq=15 outcome=applied transport=ble`
    （display_state=pending，符合“窗口后 data+clock 同帧”设计），设备 `applied_seq=15`、
    `display=displayed`；
  - 设备：`fw=0.17.1-bw`、`rv2=1`、`time_source=ble`、`awake_ms`/`ble_on_ms`/
    `render_count` 上报正常；`/history` 出现 `ev=7 (HIST_WAKE)`：
    `aux=2`（rendezvous-sleep）、`dur_ms` 6178–8583、`src=1/2`（ble/rtc）；
    另有 thin 时钟 wake（`aux=1`、dur≈979ms、renders=1）。
- **观察到的偏差（留给 task-3 测量/调参）**：
  - 单次 wake 6.2–8.6s，高于计划预算（快答 1.2–1.5s / 超时 3.85s）——6s 连接预算
    被整段消耗，可能与桥侧命令轮询/握手耗时有关；
  - 会合周期偶见 ~2 分钟：`rtcNextNetAt=now+60` 与“整分钟对齐”叠加后，某一分钟先走
    thin 时钟 wake，下一分钟才 rendezvous；
  - 既有噪声：`udp announce ignored (mac mismatch)`（不阻塞）。
- **功耗预估（采用“CPU/射频时长 × 手册电流”，0.17.2 埋点 + `tools/estimate-power.mjs`）**：
  - 首个采样（5 个 deep 周期，其中 thin×3 / rendezvous-sleep×2）：每周期 awake 2.56s =
    BLE 0.66s + CPU/render 1.90s（render 1.29s）；thin 平均 0.98s、rendezvous-sleep 平均 4.93s；
  - 估算：**39.9 / 54.6 / 76.9 mAh/day**（low/base/high），1000mAh 约 18 天（base）；
    awake 占空 4.26%，awake 期平均电流估算 38.8–75mA；本次 boot light-sleep 占 63%；
  - 局限：时长×手册电流的估算；CPU 电流取 20–40mA、BLE 窗口平均取 93–176mA；light 会话
    未计入 acc（单列）；结论前用 USB 功率计/PPK2 对拍 ≥10 周期。

### 验证

- `pio run -e esp32-s3-epaper-154g`（**非 btpm**）：SUCCESS 多次（0.16.9/0.17.0/0.17.1
  增量 45–55s；首次全量被工具 20 分钟超时中断后断点续建 9m45.8s）；
  日志 `artifacts/build-plan-c-base-2026-09-22.log`、`...-0.17.0-base.log`、`...-0.17.1-base.log`。
- `node tools/test-quad-preview.mjs`：7 fixtures 全过（模板哈希/渲染未变）。
- `cargo test --workspace`（`CARGO_TARGET_DIR=artifacts/cargo-target-powerc`）：全绿，
  含 `stamp_clock` 与新的 `remote_data_snapshot_covers_all_non_local_requirements`
  回归测试；日志 `artifacts/cargo-test-powerc-2026-09-22.log`。
- `git diff --check`：通过（无空白错误）。
- `cargo fmt --check -p bridge-ble -p bridge-app`：本次新增行无格式问题
  （文件既有 drift 未动，符合 AGENTS 约定）。

### 留给编译窗口 / 后续

1. ~~正式 ROM（task-1/2/3 行为，非 btpm）~~：已由本窗口完成（0.17.1-bw，已 OTA，见上）。
   后续功率预估埋点 ROM 0.17.2 也已 OTA，见上；0.17.0 被取代（仅留痕）。
2. **btpm 实验 ROM（task-4 A/B）**：等 task-3 基线采集后再切
   `esp32-s3-epaper-154g-btpm` 构建（勿在任务 1–3 阶段用 btpm）。
3. task-3 30–60 分钟基线与调参（会合时长/周期）由后续窗口执行；
   `/history`、桥日志与设备 `/status.json` 已带全部所需字段。

## 下一步

0. 按 `task-6-bridge-first-impl.md` 执行：先 Windows publisher spike，再 bridge_first 协议/
   切换实现；实现完成后按 §5 统一 A/B 排期（A1 桥提速 → A2/A3 DFS×btpm → A5 节奏 → A4 双策略）；
1. 收尾 v2 收敛工作树（提交/文档/桥重建）或确认与本计划改动合并方式；
2. 按 task-3 做 30–60 分钟基线采集（本文件最后部署记录为非 btpm 0.17.2 + 新桥），
   重点核对 wake 时长（当前 6.2–8.6s 超预算）与会合周期（偶见 2 分钟）；
3. 桥侧按 task-2 §4 提速（常驻扫描/复用 Peripheral/跳过 INFO/快轮询），目标无数据 ≤3s；
4. 基线后开展 task-5 双策略 spike：device_first 与 bridge_first 独立 A/B，
   认证指令/每窗口必回 StatusBeacon、并行 HTTP 等待、设备硬截止及策略恢复见 task-5；
5. 基线通过后按 task-4 切 btpm env 做 A/B。

## 调研与文档更新（2026-09-22）

- 同类项目与社区实测、BLE 广播空口事实、Windows/btleplug 限制、回复广播 vs 静默的成本结论
  已写入 `plan.md`「调研补充」；
- `task-5-advert-rendezvous.md` 已收敛为双策略候选：PC 每窗口广播，设备扫描并必回状态；
  OPEN_WIFI 后并行等待状态/HTTP，复用现有 coordinator/认证交付，未实现、需 spike。
  明文 hasWork、设备主动连接既有 GATT、回复成本为零等旧草案不再适用；
- task-1 增“连接期预算/心跳 metadata”、task-2 增“单次 wake 压缩/窗口预测”、
  task-3 增“分阶段计时与通道对照”、task-4 增 coex 残余电流与共射频风险。

## 构建记录（预演）— 2026-09-22

- **时间**：18:54–19:10（本地）；**构建总耗时** 16m19.3s（pioarduino 两阶段：
  IDF/框架库 + Arduino 应用；Arduino 阶段 78.9s）。
- **env**：`esp32-s3-epaper-154g-btpm`（`extends` base + `custom_sdkconfig` 追加
  `CONFIG_BT_CTRL_MODEM_SLEEP=y`，随 checkpoint 提交 `1c9b5dc`）。
- **FW_VERSION**：`0.16.9-bw`（未改；源码 = 已提交的 v2 收敛工作树，不含 Plan C 行为）。
- **结果**：`SUCCESS`；RAM 32.0%（105008/327680 B）、Flash 52.8%（1660800/3145728 B）。
- **ROM**：`artifacts/codex-status-0.16.9-bw-btpm-dryrun.bin`，
  **1 696 448 B**，SHA256 `A300D57CD4F30220803578704EA48A00CD6C75209EF0E85BB89EE49435E9C91E`
  （**未 OTA**，仅预演）。
- **日志**：`artifacts/build-btpm-2026-09-22.log`（347 KB）；
  sdkconfig 差异全文 `artifacts/sdkconfig-diff-pm-vs-btpm.txt`。
- **sdkconfig**：`sdkconfig.esp32-s3-epaper-154g-btpm` 含 `CONFIG_BT_CTRL_MODEM_SLEEP=y`
  （L1760）。**注意**：基线 `sdkconfig.esp32-s3-epaper-154g` 不存在
  （base 的 `.pio/build` 已清理，按约束未重建 base env），差异用 9/17 的旧文件
  `sdkconfig.esp32-s3-epaper-154g-pm` 对照，逐项归因如下。

  **modem sleep 直接/连带变化（4 个新符号 + 2 个派生值，无其它 ripple）**
  - `CONFIG_BT_CTRL_MODEM_SLEEP=y`（新增）
  - `CONFIG_BT_CTRL_MODEM_SLEEP_MODE_1=y`（choice 默认档）
  - `CONFIG_BT_CTRL_LPCLK_SEL_MAIN_XTAL=y`（低功耗时钟取 main XTAL；RTC_SLOW 未选）
  - `CONFIG_BT_CTRL_MAIN_XTAL_PU_DURING_LIGHT_SLEEP` 未选（新可见的从属项）
  - `CONFIG_BT_CTRL_SLEEP_MODE_EFF` 0→1、`CONFIG_BT_CTRL_SLEEP_CLOCK_EFF` 0→1（派生效率宏）
  - 结论：无 BT/coexist 之外的其他 ripple；RTC 时钟源、PM、Wi-Fi 省电项均无变化。

  **非 modem sleep（9/17→9/22 基线漂移，已用 git 提交核对）**
  - 40 MHz 闪存：`ESPTOOLPY_FLASHFREQ` 80m→40m、`SPIRAM_SPEED` 80→40（9/18 `6352236`）
  - `CONFIG_PM_PROFILING=y`（9/18 `127bfb3`）
  - `CONFIG_PM_LIGHT_SLEEP_CALLBACKS=y`、`CONFIG_ESP_TIMER_PROFILING=y`（9/19 `26123c3`）
  - Wi-Fi SLP：MIN_ACTIVE 50→20、MAX_ACTIVE 10→60、WAIT_BROADCAST 15→10（9/20 `ba9629e`）
  - 框架包漂移（非配置项）：`BOOTLOADER_VDDSDIO_BOOST_1_8V` 出现、`SR_NSN_NSNET3`、
    `SR_WN_WN10_*` 5 项。
- **影响**：task-4 的 env 配置已验证可编译；task-1 落地后需在同一 env 重编正式 ROM
  （预演件不含 Plan C 行为，不用于实机验证；task-1 后来已上线，仍需重编正式 btpm 件）。
