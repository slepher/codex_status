# Power State v0.12

权威规格：`docs/power-state.md`（2026-09-18 定稿）。本专项只执行，不改规格；
需要偏离时先改 `docs/power-state.md` 再动代码。

## 目标与基线

- 常开单模式：空闲自动 light sleep（BLE 必须先 deinit），不主动 deep sleep
  （仅三条路径：WIFI OFF·电池、AP 无凭据电池超时、低电断电）；BLE 只在
  会话期存在；T10 平均电流 ≤2mA（对照 0.11.x）。
- 基线 HEAD `302f682`，固件 0.11.9-bw（PM light sleep + T9 已过），设备
  ota_0 LIVE（`stay`）、`/status.json pm_light_sleep=true`、电量 87%。
- 设备：192.168.1.50 / `home-wifi`，BLE `CodexStatus-AABBCC`；USB 插拔以
  SOF 实测为准，测试前需退出 `stay`。

## 约束

- 三方模板一致：固件引擎、`bridge/crates/core/template.rs` canonical JSON、
  Python 测试桥（+ `tools/generate-quad-preview.mjs`/`test-quad-preview.mjs`
  预览）；`type/font/bind/mode` 语义必须三端同时改，完成后哈希一致。
- 预览逐像素一致（`crates/render` 共用固件 C++ 引擎）。
- token 401、BLE 绑定语义、OTA 流程不变；推送仍是显式动作。
- 密钥不进仓库/日志；运行中的桥/设备服务不擅自停止。
- 固件构建遵循 AGENTS.md（40MHz flash、勿 `-v`、勿在 pm 构建中删
  `sdkconfig.defaults`）。

## 任务

### task-1 固件 0.12.0：单模式运行/电源状态机（固件侧协议）

按 `docs/power-state.md` §3–§8、§10：
- 删除 DEEP/LIVE/窗口/退避/`stay`/`/sleep`/CLI `sleep|mode`/idle 模板槽/
  idle 双渲染/`device.state=IDLE` 原因文本/`idle_template` 处理。
- 新状态：AP / BLE ON / IDLE(BLE OFF) / WIFI OFF（插电/电池）/ 低电；
  `device.state` 绑定值 `AP`/`BLE ON`/`BLE OFF`/`WIFI OFF`（WIFI OFF 优先）。
- 插电判定 `usb_serial_jtag_is_connected()`；GP3 绿灯；BOOT 单击/2s/15s/30s；
  BLE 会话 120s 策略与 `NimBLEDevice::deinit(true)`；Wi-Fi 30s 失联判定、
  插电 60s 重试、电池 1min×3→5min×3→15min 深睡重试；低电 5% 断电。
- 模板引擎去 `mode`、去 `device.idle_reason`、`device.state` 取新状态字、
  `device.offline_mins` = 距上次成功同步分钟数（RTC 持久化时间戳）。
- UDP 通告 `{magic,mac,ip,port,proto,ble,fw}` → 255.255.255.255:8767；
  envelope `bridge.host/port` 自愈 endpoint。
- `/status.json` 去窗口字段，增 `ble_on`/`plugged`/`wifi_state`/`retry_stage`/
  `last_push`/`state`；CLI 增 `pmstats`。

DoD：`pio run` 成功；`git diff --check` 干净；不 OTA（等 task-2 三端同步）。

### task-2 宿主侧协议同步（Rust/Python/Node + quad 单布局）

- `core/template.rs`：去 `mode` 白名单、去 `DeviceIdleReason`；状态字新值。
- `core/envelope.rs`、`runtime.rs`、`core/main.rs`、`render/`、`mcp/`、
  `app/config.rs`、`app/main.rs`、`app/ui/index.html`：去 `idle_template`
  与 idle/offline/reason 渲染参数，改 state 语义；测试同步
  （`tests/template.rs`、`tests/envelope.rs`）。
- `tools/test-bridge/bridge.py`：去 `idle_template`（保留 `active_hold` 决策
  见任务文档）；`tools/generate-quad-preview.mjs`、`test-quad-preview.mjs`
  去 mode/idle 分支、改 state。
- `tools/test-bridge/templates/quad.json` 改单布局：去 7 处 `mode`、合并
  4 对 live/idle 重复元素、保留 `device.state`/`device.offline_mins`。
- 更新 `full/mini/quad` 哈希常量与 `PROGRESS.md` 记录。

DoD：隔离 `cargo test --workspace`；`node tools/generate-quad-preview.mjs`；
`node tools/test-quad-preview.mjs`；`bridge-render --diff` 逐像素一致；
`git diff --check`。

### task-3 桥运行形态：去待机模板 + 状态缓存 + UDP + 按需 BLE

- `core`（envelope/runtime/main/tests）、`app`（config/main/UI）、`mcp` 删除
  `idle_template` 及其命令/下拉/持久化。
- 缓存设备最后状态（fw/模板清单/endpoint/在线），面板读缓存。
- 不做周期 BLE 扫描：UDP 8767 监听 `ble=1` 或用户显式动作才连一次。
- envelope 带 `bridge.host/port`；UDP 更新对应 MAC 的 device_ip。

DoD：cargo 测试；面板回归；UDP 换 IP 实测（需设备 task-1）。

### task-4 桥守护：watchdog + app-server 恢复

- 同 exe `--watchdog <pid>`：父进程 exit 0 → 退出；非 0 → 重新拉起；
  5min 内 3 次异常退出则停止并写日志。
- app-server 断开不走退出路径；spawn 失败重新发现新版路径；启动时
  `locate_codex` 失败不阻断 HTTP/poller。

DoD：cargo 测试；杀父进程/watchdog 自愈实测。

### task-5 整机验收（需用户配合）

- OTA 新固件；T10 电池斜率 + `pmstats`；T9 push 延迟；状态机/按键/LED/
  WIFI OFF 深睡节奏/低电/AP 回归；三端模板哈希一致；watchdog/UDP 实测。

### task-6 桥 OTA 与设备 token 缓存

- 设备 token（`/doUpdate` 门控）经已绑定 BLE 链路获取并持久化到
  `<exe>/data/device-token.json`；401 时自动重取一次。
- MCP 新工具 `firmware_ota {rom, device_ip?}`：校验 ROM 大小 → multipart
  上传 `POST /doUpdate?token=` → 轮询 `/status.json` 等待版本变化；
  单实例运行保护；总时长 ~20–90s。
- 首次取 token 需设备处于 BLE 会话（0.12 单击 BOOT；0.11.x 按住 BOOT 2s）；
  无会话时返回可执行指引，不上传。
- DoD：`cargo check`/测试通过；`tools/list` 含 `firmware_ota`；真机 OTA
  与重启版本校验在 task-5 完成。

## 证据与恢复

- ROM/日志/截图进 `artifacts/`（gitignored），每任务后更新 `PROGRESS.md`。
- 实现顺序：task-1 → task-2 → task-3 → task-4 → task-5；task-2 完成前
  不 OTA 0.12 固件（旧 quad 的 `mode` 键会被忽略而叠画）。
- 未提交改动属于工作区现状，提交需用户明确同意。
