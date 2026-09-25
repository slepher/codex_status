# Phase 3：按 MAC 的多设备运行路由 + 清掉桥侧 legacy/迁移

Status: 进行中（2026-09-25 开工）。上级合同是 `design.md` §2/§3/§4，任务索引是 `plan.md` 第 3、4 阶段。
本文件是本次实施的现场记录；`design.md` 是设计合同，冲突时以 `design.md` 为准并停下来报告。

## 用户决定（2026-09-25）

1. 范围选 **C**：做完整的多设备运行路由（`design.md` 第 3 阶段），不做"只让 v2 卡跟随"的窄版本。
2. **同步清掉桥侧 legacy 通道与历史迁移代码**——用户明确表示没有要求过对历史版本的支持和迁移。
3. 改完由实施方（agent）负责停桥 → 重建 → 重启，并在面板上自检。
4. 认可 `design.md` §3「**不**切换全局设备」：设备页的选择以显式 MAC 参数实现，不改写全局身份。

## 不变量（不得违反）

- 设备身份 = 规范化 Wi-Fi MAC；UDP/HTTP/BLE 通告 MAC 不符一律拒绝，不"切换全局设备"。
- 保存只落盘、发布是用户显式单台动作；token/claim/owner/PowerPlan 语义不变。
- 多方模板一致：改模板协议必须同时改固件引擎、`core/src/template.rs` 与 Python 测试桥。
- 显示规则（配额缺失是模板属性）不变。

## 明确不做（本次范围外）

| 项 | 原因 |
|---|---|
| 固件侧 legacy 模板通道（`template_engine.h` 旧路径、`main.cpp` `[v2] no committed bundle; legacy template store active`） | 删它需要改固件 + 重编 ROM + OTA，而旧 ROM 已无在用设备；运营上零收益、风险实打实。仅桥侧清理。 |
| `firmware_ota`、`pm_stats`、`render`、`validate`、`device_*` 等 MCP 工具 | 它们不是"历史版本支持"，是独立在用的能力。 |
| MCP 工具名去掉 `_v2` 后缀 | 纯命名（`opencode.jsonc` 未钉工具名、UI 调的是不带 `_v2` 的 Tauri 命令）。属可选的收尾项，与本次语义改动分开。 |
| `state.json` 的 `#[serde(default)]` 旧状态容忍 | 无害；删它只会制造一次性读失败风险。 |

## 进展（2026-09-25）

| 步骤 | 状态 | 证据 |
|---|---|---|
| A1 迁移删除 | **完成** | `migrate_family_profiles`/`write_family_import_marker` 及其调用点已删（main.rs -96 行） |
| A2 legacy profile 存储与 ≤3 槽推送 | **完成** | 删 `core/src/profile.rs`、`core/tests/legacy.rs`、`tools/test-bridge/profiles.seed.json`、`paths::profile_seed`、`Config.profiles/profile_seed`、Tauri `get_profiles`/`save_profile`/`delete_profile`/`push_profile`、MCP `profiles_list`/`profile_save`/`profile_push`、`push_templates_http`、`remote_template`、排队 flush 的模板分支。`cargo check -p bridge-core -p bridge-mcp -p bridge-app` 通过（1m14s，exit 0） |
| A3 `legacy` 设备标志 | **完成** | `DeviceRecord.legacy`、`device_upsert` 的 legacy 参数、`migrate_legacy_profile` 及其单测、`caps_from_status` 的 legacy 返回全部删除；非 v2 固件改为**拒绝注册**。`cargo check --workspace --all-targets` exit 0 |
| A4 旧固件回退 | **完成** | `claim_unsupported`（字段/初始化/读写/identity JSON/UI 分支）与 `device.rs` 的 HTML `/status` 回退全删；`fetch()` 只读 `/status.json` 并给出明确错误。测试夹具 `v2_fallback_server` 同步为 `/status.json` → `/v2/status` |
| A5 文件模板库 `Library` | **暂缓（需决策）** | `Library` 仍支撑 `get_status`、`preview_template`、`reload_templates`、MCP `list/get/validate/save/render`；删它要把这些改指向 v2 登记表，属能力变更，不与本次混做 |
| B1 按 MAC 运行记录注册表 | **完成并接线** | `bridge/crates/app/src/device_runtime.rs`（`DeviceRuntime` + `DeviceRegistry` + 8 个单测）；`AppCtx.devices` 已换成它 |
| B2–B5 按 MAC 路由 + 显式 MAC 契约 | **完成并实机自检** | `ctx.device_*` 在 app 内引用为 0；发现链按认证 MAC 路由；两个入口共用 `sole_registered_mac`：多台已登记设备缺 MAC 一律拒绝；`cargo test -p bridge-core -p bridge-app` = 84 + 38 全绿；两台实机自检见 `PROGRESS.md` 顶部节 |
| B6 设备页选择器 | **完成（前端）** | `ui/index.html` 选择卡 + 切换确认 + localStorage；15 处调用带显式 MAC；内联 JS `node --check` exit 0。**未人眼验收**（无法操作 Tauri 窗口） |

B 阶段实际采用**每步都能编译**的绞杀式改造（改完后确认：任何一次中断都留下可编译的树）。

## 收尾：停桥 → 重建 → 重启 → 自检（2026-09-25 现场核实）

进程身份（只读核实，`Get-CimInstance` 在受限沙箱被拒，故改用日志 + netstat）：

| PID | 角色 | 证据 |
|---|---|---|
| 5064 | **watchdog（先停它）** | 桥日志 `2026-09-25T09:28:20Z INFO bridge_app::watchdog: watchdog started (pid 5064)` |
| 24612 | 主进程（持有端口） | `netstat -ano`：8765 / 8766 / 8767 的 LISTENING 属主都是 24612；`artifacts/bridge-app-run.pid` = 24612 |

顺序：`Stop-Process -Id 5064` → `Stop-Process -Id 24612` → `cargo build -p bridge-app` → `pwsh tools/start-bridge.ps1`。
⚠️ 起桥必须用**非受限（提权）**命令：受限沙箱下 `Get-NetTCPConnection` 看不到刚起的监听者、命令结束会回收分离子进程、WebView2 建 host 报 `ERROR_BUSY`。
⚠️ 重建前先把 `bridge/target/debug/data/` 当运行时数据处理（`state.json`、`device-token-*.json`、`bridge-app.json`）；本次改动**不**需要删 `target/debug`，不要重演上次的数据损失。

### 自检清单（重启后逐项核对，✅ = 2026-09-25 已核对）

1. ✅ 两台设备仍登记（MAC `70041DD7A340` / `7C4FADB93408`，profile_count 2 / 1），两族 `默认` 草稿仍在（`mini,quad` / `codex-status-a`）。
2. ⚠️ **实测与预期相反**：`data/templates/` **没有**被 `ensure_runtime` 拷回，启动日志一直是 `templates: []`——
   但这是**改动前就有**的状态（09:26 的启动日志即如此），手工拷贝到该目录可成功（属主正常）。
   嫌疑是 `ensure_runtime()` 里 `let _ = std::fs::copy(...)` 把失败吞了（`config.seeds` 解析/读取失败均不留痕）。
   影响面仅限文件模板库 `Library`（见 A5）。✅ 同时确认 `data/profiles.json` **没有**被创建。
3. ⏳ **需人眼验收**：设备页「当前设备」列出两台、当前项高亮；点击另一台 → 确认框 → 切换后身份卡/v2 卡/功耗卡跟着变、模板页族选择不变。
   （我无法操作 Tauri 窗口；已验证其依赖的后端契约：12 个命令的 `mac` 参数、无 MAC 拒绝语义、显式 MAC 真实落到设备。）
4. ⏳ 需人眼验收：反复切换两台不串数据。
5. ✅ 后端语义已核实：多设备且未指定 MAC 时**拒绝**（`select a device: several are registered (…)`）。
   注意 UI 侧的落点是：清掉 localStorage 后，页面会回退到桥自己的当前设备（后端 `selected_device_mac`，在列表里标为「当前」），
   **不是**取列表第一项——这与后端"操作必须点名设备"的规则并不冲突。
6. ✅ 只读动作优先（本次自检只用了 `platform_status_refresh`/`platform_overview`/`family_profiles_v2` 读操作，没有发布、没有 OTA）。

进程 PID 会随重启变化，不要写死：核实办法是 `netstat -ano` 找 8765 的属主（主进程）+ 桥日志里 `watchdog started (pid …)`（watchdog）。
本次收尾实测：watchdog `5064` → 主 `24612`（旧）；重建重启后为 watchdog `45960` → 主 `32316`；停桥务必**先 watchdog 再主进程**。

## 阶段 A：清掉桥侧 legacy 与迁移（先做，编译器可验证）

事实基础：**UI 已不再调用任何旧命令**（`get_profiles`/`save_profile`/`delete_profile`/`push_profile` 在 `ui/index.html` 里 0 处引用），两台登记设备均为 `legacy=false`、`compiler_abi=2`。即旧通道在桥里已是死代码。

| 步骤 | 内容 | 验收 |
|---|---|---|
| A1 | 删 `migrate_family_profiles` + `write_family_import_marker` + 两个 `family-profile-*-imported` 标记文件 | 启动不再有任何 Profile 自动导入；Profile 丢失不可能再被"静默成功的迁移"掩盖 |
| A2 | 删桥侧 legacy profile 存储与 ≤3 槽推送：`core/src/profile.rs`、`paths::profile_seed`、`Config.profiles/profile_seed` + 两个环境变量、`ensure_runtime` 的 profile 拷贝、Tauri `get_profiles`/`save_profile`/`delete_profile`、MCP `profile_save`/`profile_push`、`McpConfig.profiles/profile_seed`、`tools/test-bridge/profiles.seed.json` | `data/profiles.json` 不再被创建或读取；编译器无残留引用 |
| A3 | 删 platform 的 `legacy` 设备标志与 `core/tests/legacy.rs`（含"旧形状不许变"的 5 个测试） | `legacy` 字段从 state.json 与 model 消失；测试全绿 |
| A4 | 删旧固件回退：HTML `/status` 解析路径、`claim_unsupported`（固件 <0.13.4） | 只走 `/status.json`；无 `claim` 的固件不再有兼容分支 |

**A1 的缺陷修复是必须项**：原实现里旧源为空时循环零次、`complete` 仍为 `true`，于是照样写"已导入"标记，从此永不重试——这正是本次 Profile 丢失的直接原因。

## 阶段 B：按 MAC 的运行路由（`design.md` §3）

| 步骤 | 内容 | 验收 |
|---|---|---|
| B1 | 新增按 MAC 的运行记录注册表（`ip`/`name`/`discover`/`last_claim_at`/`yielded`/`claim_unsupported`/`fail_streak`/`ip_dirty`/各缓存/`pending_ota` 全部按 MAC 归位） | 同一 MAC 的新 IP 替换旧 IP，其他 MAC 不受影响 |
| B2 | `AppCtx` 的单组 `device_ip/device_mac/device_name` 与相关缓存改为注册表；启动时从现有配置导入旧单设备记录，不覆盖已有设备 | 重启后两台设备同时存在，互不覆盖身份/IP/Profile/作业 |
| B3 | 发现链（属性 IP → UDP → ARP → BLE → HTTP）每条结果按认证 MAC 路由，错 MAC 拒绝 | 一台设备的通告不会改写另一台的 IP/名称 |
| B4 | claim/renew/离线标记/pending 队列按 MAC 独立 | 两台设备的 owner/ACK/队列不串 |
| B5 | `platform.rs` 全部面向设备的操作要求显式 MAC；恰有一台设备时允许兼容默认，多台未指定返回"请选择设备" | 不再取列表首项 |
| B6 | 设备页加明确设备选择器；身份/实况/v2 状态/功耗/恢复/占用/推送都按选中 MAC；刷新保持选择 | 两台设备快速切换不串数据；离线设备仍可看持久记录 |

## 验收与交付

- 每步 `cargo check -p bridge-app`（不触碰运行中的 `bridge-app.exe`）；A 阶段末跑 `cargo test --workspace`（隔离 `CARGO_TARGET_DIR`，运行中的桥会锁 `target/debug`）。
- 收尾：停 watchdog → 停桥 → `cargo build -p bridge-app` → 重启（**必需提权**：受限沙箱会回收分离子进程且 WebView2 建 host 报 `ERROR_BUSY`）。
- 重启后核对：两台设备都在、`data/templates/` 种子回填、两族 `默认` 草稿仍在、设备页可切换且不串数据。
- 更新 `PROGRESS.md` 现场节与 `docs/roadmap/backlog.md`（删掉已交付条目）。
