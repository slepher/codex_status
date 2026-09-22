# Prompt — Plan C 续作 3（bridge_first 实现 + 统一 A/B）

你是本仓库（Codex Status，ESP32-S3 墨水屏 + Rust 桥）的实现代理。先读（**最小集，勿整读历史**）：

- `AGENTS.md`（不变量、命令、工作流约定）
- 本文件（自带当前现场，读完即可开工）
- `project-workflow/power-plan-c/task-6-bridge-first-impl.md`（实现分解 §1–§4 + 统一 A/B 排期 §5）
- `project-workflow/power-plan-c/status.md` 的「实现与证据（2026-09-23）」节
- 需要协议/预算细节时再查 `task-5-advert-rendezvous.md`、`plan.md`（调研补充）

`PROGRESS.md` 已精简为最近四节；2026-09-22 及更早全部在
`docs/history/progress-archive-2026-09-23.md`，只在追溯具体旧结论时按标题检索。
上一版 `prompt-execute-2.md` 已过时，勿按它执行。

## 当前现场（2026-09-23 晚）

- 设备：`192.168.3.163` / MAC `70041DD7A340`，运行 **0.17.9-bw**（已 OTA 链
  0.17.2→0.17.5→…→0.17.9），`rv2=1`；BLE 名 `CodexStatus-D7A340`。
- 桥：`bridge/target/debug`（工作树重建件，含 task-2 §4 定稿 + post_ota_window），
  MCP `http://127.0.0.1:8766/mcp`，数据目录 `bridge/target/debug/data/`，
  token 缓存 `data/device-token.json`，pidfile `artifacts/bridge-app-run.pid`。
- 已提交 **`99ef3c0`**（上一基线 `e1e93fc`），本次提交包含：`src/main.cpp`、`src/ble_bridge.{h,cpp}`、
  `bridge/crates/ble/{lib.rs,Cargo.toml,examples/adv-spike.rs}`、
  `bridge/crates/{app/src/{main,platform}.rs, core/src/{coordinator.rs,platform/service.rs}}`、
  plan/status/task-2/4/5 文档、新增 task-6；`docs/generic-display-platform-design-v2.md` 与
  `PROGRESS.md` 已被上游同步（勿回退）。**勿动** `docs/fake-rom-simulator-design.md`。
- ROM：0.17.5 `6D46A6A0…`、0.17.6 `C1C43933…`、0.17.7 `9EF5981B…`、0.17.8 `31F7EA73…`、
  0.17.9 `36359E9D…`（`artifacts/codex-status-0.17.*.bin`）。

## 已定稿、勿再回退的实现结论（实测）

1. **桥 BLE 生命周期**：每次机会**新建 adapter**、事件驱动发现（命中即 `stop_scan`）、
   每周期 `disconnect`+`discover_services`、ACK 20ms。常驻 adapter+持续扫描、
   不 disconnect/跳 discovery 均实测隔次 connect 失败（status.md 09-23 节有数据）。
2. **OTA 可靠性**：上传期 `WIFI_PS_NONE`（0.17.5+）；post-OTA 5 分钟 light 窗口由
   **NVS** 标记（0.17.7+，`/status.json.post_ota_hold_s`）+ 桥 `post_ota_window` 的
   300s 显式 light 计划双层控制。0.17.6→0.17.9 连续 OTA 均一次通过。
3. **离线显示**：v2 下 `markSynced()` 已接入 BLE 认证命令与 `/v2/status|data|plan`
   （0.17.9），`offline_mins` 不再误报。
4. **设备侧 BLE 扫描**（诊断/接收端）：`/diag?blescan=N&company=`（token）+ 串口
   `blescan N`；`NimBLEScan::start()` 单位**毫秒**且**异步**（须等 `isScanning()` 结束），
   扫描期需 `WIFI_PS_NONE` 防共存饥饿。PC beacon 实测：non-connectable、AD 28B、
   应用 payload 恰 24B。证据 `artifacts/blescan-device-2026-09-23e.json`。
5. 会合节奏仍是 ~2 分钟/次（整分钟对齐吃掉一分钟，task-5 §5 待修，A5 臂）。

## 任务顺序（勿跳步）

1. **task-6 §2–§4：bridge_first 实现**（spike 已 Go，§1.2；按 task-5 §4 常量与 §6 切换规则）
   - §2 冻结 24B 布局/Company ID/config_epoch/window_seq/64-bit tag 输入域与密钥存储；
   - §3 固件：timer 唤醒 `SCAN → REPLY StatusBeacon（每窗口必回）→ ACCEPT_SLEEP |
     OPEN_WIFI（bootstrap 硬截止）`，复用 `bleScanJson` 的扫描路径与 `serviceV2Ble` 认证；
     遥测入 `/history`、`/status.json`；
   - §4 桥：窗口前 guard 起 Publisher、每窗口冻结一条 directive 重复发送；StatusBeacon
     监听与 HTTP 并行等待；结果复用 `platform::cycle`/coordinator（owner/Data/Bundle/
     PowerPlan 全复用）；默认 device_first，配置 ACK 后指定未来窗口切换，保留恢复窗口。
   - 每步先写对应 task 文档的验收再写码；控件/回调不阻塞主任务。
2. **统一 A/B（task-6 §5）**：每臂 ≥30 个 deep 周期（当前节奏 ≈2min/周期 → 约 1 小时/臂），
   同负载、记录 PC 网络形态；口径：桥 `bridge_ble=debug` 分阶段计时 + 设备 `/history`
   (`aux/dur_ms/src`) + `/status.json`(`awake_ms/ble_on_ms/render_ms/deep.acc_*`) +
   `node tools/estimate-power.mjs 192.168.3.163`；失败分母/首败重试/丢窗口单列。
   顺序 A1（桥改前 e1e93fc vs 现实现）→ A2（DFS min 40 vs 80）→ A3（btpm×DFS 2×2，
   含“经历一次 Wi-Fi light 后”）→ A5（2min vs 修复后 60s）→ A4（device_first vs
   bridge_first）。A5 修复建议：固件把“距到期 ≤5s 的 net wake 视为到期”，或桥侧
   `next_contact_s` 对齐；两案都要进 A/B。
3. **回归**：`pio run -e esp32-s3-epaper-154g`（勿 `-v`）；Rust 用隔离 target
   `artifacts/cargo-target-powerc` 跑 `cargo test --workspace`；`node tools/test-quad-preview.mjs`；
   `git diff --check`；新增行 `rustfmt --check`。

## 操作手册（本机特有）

- **让设备上线**：MCP `template_activate {"id":"quad"}`（无工具时走 HTTP：
  `Invoke-WebRequest http://127.0.0.1:8766/mcp -Method Post -Body '<tools/call JSON>'`），
  下一次 rendezvous（≤2 分钟）下发 light 计划；设备自带 post-OTA 窗口时也可直接等。
- **OTA**：MCP `firmware_ota`（rom 路径，相对仓库根）；设备 deep 时自动排队、下次
  light flush；失败重试有 backoff，不要高频调用。
- **读遥测**：`/status.json`、`/history`、`/log`、`/pmstats?diag=1`；`/diag?clean=1`
  可强制重渲染验证显示；所有 `/diag` 与 OTA 需设备 token（缓存文件见上）。
- **桥重启**：先结束 watchdog（命令行含 `--watchdog`）再父进程；`pwsh tools/start-bridge.ps1`
  分离启动；核对后把父进程 PID 写进 pidfile；`RUST_LOG='bridge_app=debug,bridge_ble=debug,info'`。
- **blescan 诊断**：设备在线时
  `Invoke-WebRequest -Method Post 'http://192.168.3.163/diag?blescan=12&company=65535&token=<token>'`；
  PC 侧 publisher 用 `artifacts/cargo-target-powerc/debug/examples/adv-spike.exe`。
- **链路注意**：设备 2.4G + `WIFI_PS_MAX_MODEM`(listen=10) 时 ping RTT 在 5–12ms/1s
  交替；大上传/大请求前先确认 rssi ≥ -60，必要时请用户挪近 AP；PC 在 `wd21-wo`(5GHz)、
  设备在 `wd21-la`(2.4GHz)，同一路由器不同频段。

## 约束

- AGENTS.md 全部不变量（token/claim/owner/MAC 身份、模板三端哈希、像素一致、ASCII 净化、
  便携数据布局、不提交密钥）；未经用户明确要求不提交；每次 OTA 记录 ROM 路径/SHA/版本；
  后台进程分离启动立即返回；不停止非本构建目录的服务；工作树里的
  `docs/fake-rom-simulator-design.md` 不是本计划产物。
- 每步完成更新 `project-workflow/power-plan-c/status.md`（任务状态、ROM/SHA、分阶段计时、
  估算表、证据路径）；原始日志进 `artifacts/`（不入库）。
- 报告格式：改动文件:行、命令与输出摘要、未完成/风险项、下一步。
