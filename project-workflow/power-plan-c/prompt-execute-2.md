# Prompt — Plan C 续作（新窗口）

你是本仓库（Codex Status，ESP32-S3 墨水屏 + Rust 桥）的实现代理。先读：

- `AGENTS.md`（不变量、命令、工作流约定）
- `project-workflow/power-plan-c/plan.md`（含「调研补充」与补充验收）
- `task-1-firmware.md`、`task-2-bridge.md`、`task-3-telemetry.md`、
  `task-4-modem-sleep.md`、`task-5-advert-rendezvous.md`
- `project-workflow/power-plan-c/status.md`（权威现场与实测记录）

## 背景（自包含）

- 基线提交：`e1e93fc`（master）= Plan C task-1/2/3 代码 + 0.17.2-bw 固件 + 桥提速前的状态。
- 设备现场：`192.168.3.163` / MAC `70041DD7A340`，运行 **0.17.2-bw**（已 OTA），`rv2=1`；
  deep 周期 ~60s，BLE 会合已打通（`v2 rendezvous complete transport=ble`）。
- 桥：`bridge/target/debug`（已含 task-2 全部改动），MCP `http://127.0.0.1:8766/mcp`，
  数据目录 `bridge/target/debug/data/`，pidfile `artifacts/bridge-app-run.pid`。
- 已实测（首份 5 周期采样）：
  - thin 时钟 wake ≈0.98s；**rendezvous-sleep ≈4.93s**（目标 ≤3s）；历史最差 6.2–8.6s；
  - 估算 39.9 / 54.6 / 76.9 mAh/day（low/base/high，1000mAh ≈18 天 base）；
    awake 占空 4.26%，boot 内 light-sleep 占 63%。
- 已定位瓶颈：Windows central 的扫描器冷启动 + connect 首败重试 + GATT discovery
  （btleplug #301/#360/#155/#182/#453，见 plan.md 调研补充）。

## 任务顺序（勿跳步）

1. **task-2 §4 桥侧提速（先做，不改设备协议）**
   - 常驻 adapter + 持续/窗口前扫描（避免每次 `Pusher::adapter()`+`start_scan` 冷启动）；
   - 复用同一 `Peripheral` 对象跨断连；首次成功后尝试跳过重复 `discover_services`
     （失败必须回退重发现）；已知设备缓存 INFO（MAC/`rendezvous_v`）；
   - ACK 轮询 80→20ms；保留“同窗口 connect 重试”和 55s 去重；
   - 重建并重启桥，用桥日志 + `node tools/estimate-power.mjs 192.168.3.163` 复测；
     **目标：无数据 rendezvous ≤3s**；把改前/改后各 ≥30 周期的数据写进 `status.md`。
2. **task-3 30–60 分钟基线**：逐周期记录（桥日志 + `/history` + 估算输出），
   日志存 `artifacts/`（不入库）；核对 `light` 次数=0、每窗口连接 ≤1、时钟每分钟更新。
3. **仍 >3s 才启动 task-5 评估**：先做 Windows“广播+扫描并发”可行性 spike，
   再决定是否实现苏醒心跳（`next_wake_in_s`）+ 预测窗口；失败要能回退到现行路径。
4. **task-4 btpm A/B**（基线通过后）：构建 `-e esp32-s3-epaper-154g-btpm`，归档 ROM+SHA，
   OTA（OTA 前必须升 `FW_VERSION`），每臂 ≥30 个 deep 周期，对照
   `tools/estimate-power.mjs` + 连接成功率；注意 esp-idf #15891 的 coex 残余（A/B 要含
   “经历一次 Wi-Fi light 后”的基线）；结论记入 `status.md`。

## 操作手册（本机特有）

- **OTA**：MCP HTTP `tools/call` `firmware_ota`（`Invoke-WebRequest -TimeoutSec 240`）；
  设备 deep 时会被排队——先用 MCP `template_activate {"id":"quad"}` 制造 pending，
  等下一次 rendezvous 下发 light 计划（≤2 分钟）后再 OTA；不要连续重试。
- **读遥测**：`/status.json`、`/history`、`/log`；估算脚本只对非 btpm/btpm 同口径对比有效；
  OTA/软复位会清 `deep.acc_*`，读数前先跑 ≥10 个 deep 周期。
- **桥重启**：先结束 watchdog（`CommandLine like '%--watchdog%'` 的进程），再结束其父进程；
  `pwsh tools/start-bridge.ps1` 分离启动；启动脚本记录的 pid 可能是子进程，核对后把 pidfile
  写成父进程。调试日志用 `$env:RUST_LOG='bridge_app=debug,bridge_ble=debug,info'`（仅影响新进程）。
- **构建/验证**：固件 `pio run -e esp32-s3-epaper-154g`（非 btpm；勿 `-v`，GBK 控制台会挂）；
  Rust 用隔离 target：`$env:CARGO_TARGET_DIR='D:\Documents\PlatformIO\Projects\codex_status\artifacts\cargo-target-powerc'`；
  回归 `node tools/test-quad-preview.mjs`、`cargo test --workspace`、`git diff --check`。
- 既有噪声：`udp announce ignored (mac mismatch)`（不阻塞，勿顺手改安全语义）。

## 约束

- AGENTS.md 全部不变量适用（token/claim/owner/MAC 身份、模板三端哈希、渲染像素一致、
  ASCII 净化、便携数据布局、不提交密钥）；
- 未经用户明确要求不提交；OTA 属于本计划授权范围，但每次必须记录 ROM 路径/SHA256/版本；
- 不停止非本构建目录的服务；后台进程必须分离启动、立即返回；
- 工作树里 `docs/fake-rom-simulator-design.md` 不是本计划产物，勿动。

## 交付

- 每步完成后更新 `project-workflow/power-plan-c/status.md`（任务状态、ROM/SHA、分阶段计时、
  估算表、证据日志路径）；测量原始日志进 `artifacts/`；
- 报告格式：改动文件:行、命令与输出摘要、未完成/风险项、下一步。
