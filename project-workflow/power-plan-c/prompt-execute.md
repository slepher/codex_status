# Prompt — Plan C 代码执行（交新窗口）

你是本仓库（Codex Status，ESP32-S3 墨水屏 + Rust 桥）的实现代理。请先读：

- `AGENTS.md`（不变量、命令、工作流约定）
- `project-workflow/power-plan-c/plan.md`
- `project-workflow/power-plan-c/task-1-firmware.md`
- `project-workflow/power-plan-c/task-2-bridge.md`
- `project-workflow/power-plan-c/task-3-telemetry.md`
- `project-workflow/power-plan-c/status.md`

## 背景（自包含）

- 工作树里已有一批**未提交**的 v2 收敛改动（固件 0.16.9-bw + 桥）；本任务在其之上继续，不要回退它们。
- 现场：设备 0.16.7-bw / `rv2=0`；运行中的桥为旧构建（`bridge/target/debug`，勿停止/勿覆盖）。
- Plan C 冻结设计：
  - 会合周期 60s；BLE 窗口 **3s 硬上限**，收到回复即提前关；
  - 渲染在 **回复到达或窗口超时之后** 执行一次（此时 BLE 已关）；
  - bridge 回复携带 `server_time` + `tz_offset_min` → 设备校时后再渲染；无回复用本地 RTC 外推；
  - 广播参数显式 30–60ms；DFS `min_freq` 40→80MHz（与 flash 的 40MHz 无关）；
  - 小数据走 BLE、大更新走 light，数据路径不做微优化。

## 实现范围（task-1 / task-2 / task-3）

1. **修复 v2 会合唤醒时钟停更（正确性，优先）**
   - `deepNetDue()` 为真时跳过 `deepThinWake()`（`src/main.cpp:4512`），而 v2 无 light 路径直接 `sleepToNextEvent()`（`src/main.cpp:4625-4628`）从不渲染时钟；
   - 每次会合后 `rtcNextNetAt = now+60` 且唤醒点对齐整分钟 → 时钟会一直停更；
   - 修复：会合唤醒在"回复后或超时后"渲染一次时钟（复用 `clockTickWake` / `clkCaptureFromFramebuffer` 的时钟窗口逻辑）。
2. **窗口**：`v2Rendezvous()`（`src/main.cpp:3683-3699`）deadline 15000 → 3000ms；plan ack 后 500ms → 150–200ms；无连接不重试。
3. **渲染时机**：从窗口前/并行改为 BLE 关闭后；BLE 小数据更新时数据+时钟同帧一次；收到 light 计划时时钟并入 light 首帧。
4. **校时**：应用桥回复中的 `server_time` + `tz_offset_min`（复用既有 time/tz 路径与 NVS `pm/tz`）；字段缺失/无回复退化为本地 RTC，不阻塞渲染。
5. **广播**：`src/ble_bridge.cpp:327` 显式 `setMinInterval/setMaxInterval`（0x30–0x60，30–60ms），不依赖 NimBLE 默认。
6. **DFS**：`src/main.cpp:363` `min_freq_mhz` 40 → 80。
7. **桥（task-2）**：BLE 的 `status`/`plan`/`data` 回复增加 `server_time` + `tz_offset_min`；扫描改连续或间隔 ≤0.75s；同一会合窗口只连一次（去重）；无工作也连一次发 sleep plan；HTTP 交付不得报成 BLE。
8. **遥测（task-3）**：`/status.json` 增加 `awake_ms`、`ble_on_ms`、`render_count`、`time_source`；`/history` 记录每次 wake 时长与结果。

## 约束

- AGENTS.md 的全部不变量适用：token/claim/owner/MAC 身份、模板三端哈希、渲染像素一致、ASCII 净化、便携数据布局、不提交密钥等；
- 不提交、不 OTA、不部署、不重启/停止运行中的服务；
- 固件最终 ROM 构建与归档由独立"编译窗口"负责（见 `prompt-build.md`）；本窗口可以跑一次 `pio run -e esp32-s3-epaper-154g` 做编译验证，但不要长时间占用 `.pio/`，也不要两个窗口同时构建；
- Windows 控制台 GBK：不要用 `pio run -v`；后台进程启动必须分离且立即返回；
- Rust 测试用隔离 target：`CARGO_TARGET_DIR=artifacts/cargo-target-powerc`（避免锁住运行中的桥）。

## 验证

- `node tools/test-quad-preview.mjs`
- `cargo test --workspace`（隔离 target）
- `git diff --check`；新增/修改的 Rust 文件 `cargo fmt`（不改既有格式漂移文件）
- 固件 `pio run -e esp32-s3-epaper-154g` 通过（或说明未跑原因）

## 交付

- 代码 diff 摘要（文件:行）、测试输出摘要、未完成/风险项；
- 更新 `project-workflow/power-plan-c/status.md`（任务状态与证据）；PROGRESS.md 暂不动；
- 明确列出"留给编译窗口"的构建项（基础 ROM + btpm 实验 env）。
