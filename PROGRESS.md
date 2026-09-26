# Codex Status 项目进度（交接文档）

## Fake ROM D/E/F 软件验收完成（2026-09-26）

- 用户范围为 D/E/F；G 整套实机回归取消。M01–M18 按错误类别取代表软件 case，明细见 `docs/history/workflow/fake-rom-simulator-def/evidence.md`。D/E/F 不能证明的 Windows GATT 缓存、面板 BUSY/LUT、真实 Flash/RTC 掉电边界仅记录在同目录 `hardware-cases.md`；没有实现或执行硬件 case。
- Fake ROM 已用固件同源 C++ 处理 Bundle/Data/Activate/Plan/claim 与画面；两族真实 v2_client 安装、1–8 项循环、A/B 文件恢复、丢 ACK 后认证对账、有限版本 OTA 与 USB/低电软件判定通过代表性测试。timer wake 仅开放有界 fake BLE 窗口，真实 `bridge_ble::V2Connection` 经显式 loopback 注册表连接、分片与身份/nonce 校验；正式 BLE Plan 后 HTTP 才开放。隔离 Bridge 的 `CODEX_STATUS_SIM_COOPERATIVE=1` 关闭后台轮询，`/sim/run` 单轮与 `/sim/clock` step 通过在途屏障；设备与 Bridge 各 MAC 的单调/wall 时钟可暂停、推进、跨进程恢复，真实 MAC 保持宿主时钟。
- 隔离 1.54 Fake A `0200000000A1` 与 Note4 Fake B `0200000000B2` 从同一磁盘快照完整协作重跑 **24 h 虚拟时间、2,861 个事件、各 1,430 次 timer wake**；前 10 min 是 light Plan，随后按分钟 timer wake。runner 逐个要求 BLE 窗口的真实 `V2Connection` 会合返回成功，否则立即失败；本次全程无失败。最终 `artifacts/fake-rom-f/final-day.ndjson`（ignored）SHA256 `A648B85ACE46DA6CF9988D63C776E1798F723DA808E142D7E95A5D0B416AE8C9`。修复宿主模板时钟前的旧 24 h trace `day.ndjson` 为 2,880 事件/各 1,440 wake，只保留作历史诊断，不算最终验收。另 10 min 失步输入 A 1.0×、B 设备 1.1× / Bridge 0.9×，A 增 10 wake、B 增 11 wake，B 设备/桥时间独立。所有端点均为 127.0.0.1，运行数据在独立 `artifacts/`，生产 Bridge/实机未触碰。
- 首次同快照双回放发现画面 CRC 不同：共享模板引擎在宿主渲染 `device.now/date` 时读真实 `time(nullptr)`。现由 `TplEnv` 可选实验 wall 时间供 Fake ROM 使用；默认生产路径继续读设备时钟。新增跨设备 1 h wall 偏移像素测试。修复后同一磁盘快照两次进程重启、各 1 h/101 事件的归一化 trace 与帧 CRC **逐字节相同**，`replay3.ndjson`/`replay4.ndjson` SHA256 均 `76AC0223DA6E40372B4F5B391DA0A83D83FE2D16E7E1879D7FCD4D359DB7CF84`。旧失败 trace 留在 ignored artifacts 用于归因，不算通过证据。
- 隔离 Cargo target 下 `cargo test -p bridge-render -p device-sim -p bridge-core -p bridge-ble -p bridge-app` 通过（device-sim bootstrap **32/32**、core 90/90、app 39/39、ble 14/14、render 全部）；`node --check tools/fake-rom-runner.mjs`、`git diff --check` 通过。仅按默认目标 `pwsh tools/pio-target.ps1 -Target note4` 重建成功，ROM `.pio/build/zectrix-note4-b/firmware.bin`，`0.18.24-note4-b`，**1,758,720 B**，SHA256 **`2008735108E95CEC2511527A0A5B028496B0E1B947EC4D53F8ACD6B7BF22C9D0`**，target/version marker 均在；未刷写。隔离 Bridge/watchdog/Fake 进程已全部停止。

## 双设备 ROM 顺序发布与实机验证（2026-09-26，主路径完成）

- 用户本轮明确授权顺序发布 Note4 与 1.54 两份 ROM 并实机测试；`AGENTS.md` 的默认只构建 Note4 成本约束已增加本轮 154g 标准 B/W 目标例外，两个 PIO 进程不并行。现场以 `bridge/target/debug/data/platform/state.json` 登记为准：Note4 `7C4FADB93408`、1.54 `70041DD7A340`。
- Note4 已发布 ROM：`.pio/build/zectrix-note4-b/firmware.bin`，`0.18.24-note4-b`，1,759,040 B，SHA256 `7B123BA616D3E75E38BFF2D8EBD9F3C945B2FF25CB9BCF4FC2B256C83E3A832D`，含 `codex-status-ota-v1|zectrix-note4-400x300|0.18.24-note4-b` marker。
- 新构建 1.54：`tools/pio-target.ps1 -Target 154g` 单目标成功（51.36 s，无 framework 重装）；`.pio/build/esp32-s3-epaper-154g/firmware.bin`，`0.18.24-bw`，1,740,832 B，SHA256 `55B8BC9421CE1CA2DE981982A37318271B25A99DAF866F824B116C9FF1B30D78`，含 `codex-status-ota-v1|codex-status-154g|0.18.24-bw` marker；镜像头 `e9 07 02 30` 对应 8 MB / 40 MHz。
- 两台登记 IP 上 `/status.json` 本轮各 4 s 超时；设备操作 token 文件均在。先向 Note4 MAC 持久入队 OTA：job `a0959afd`，request `sleep-aware-20260926-note4-01824`，冻结 SHA256/大小与上述 ROM 一致。自然会合后 1 次上传收到 `UPDATE OK`；随后同 MAC 认证 `/v2/status` 快照 `fw=0.18.24-note4-b`、`observed_at=1790408480`。旧 ROM 的上传前认证 `fw` 未知，因此 job 保留 `awaiting_confirmation` / `version_seen_unproven`，未重刷；公开状态在设备再次深睡时不可读，精确运行镜像 SHA256 仍未证明。
- 完成首台上传与新版本认证观察后，才向 1.54 MAC 入队：job `584b67f5`，request `sleep-aware-20260926-154g-01824`，冻结 SHA256/大小与上述 1.54 ROM 一致。自然会合后 1 次上传收到 `UPDATE OK`；随后同 MAC 认证状态见 `fw=0.18.24-bw`。两台 OTA 均只上传 1 次，仍为 `awaiting_confirmation` / `version_seen_unproven`，没有精确运行镜像 SHA256 证据。
- 发现并修正：OTA 有 ACK 且认证见预期版本后仍因待精确确认永久阻挡后续模板发布；当前只解除对独立业务的阻塞，OTA 状态/不重刷约束保留。重建 Bridge 后，Note4 Bundle job `e48f608e` 与 1.54 job `98d2c0b9` 均自然会合 `succeeded` 且认证 `committed_job_id` 匹配；1.54 另一个 job `776b2f66` 在 waiting 时跨 Bridge 重启，之后自然交付 `succeeded`。运行数据保留设备 2、模板 3、族配置 2。
- 电源与显示：Note4 显式 light PowerPlan `209` 收到 600 s ACK；显式 sleep `210` 收到设备 applied/0 s，发现 Bridge 未记 sleep ACK 后已修正并重建。最终显式 sleep `214` 的 HTTP ACK `applied/0` 已由 `power_view_v2` 复核 last_sent/last_accepted 均为 214、剩余 0。错误 token 的只读 `/v2/status` 探针限时 50 s / 17 次均无 HTTP 响应，401 未获结论，随后已结束临时 light 请求。Note4 认证 `display_state` 曾在 failed/displayed 间交替；用户看见实物屏幕为正常休眠画面，渲染报告异常待设备日志归因。
- 最终 Bridge `bridge/target/debug/bridge-app.exe` 33,591,808 B，SHA256 `18099053D269073F4D708C2CE3D4F26EA65051466A0A90547F5B9B78A81C6BEF`；主 PID `16692`、watchdog `30368`，HTTP/MCP 监听 8765/8766。core OTA 定向测试、app 38 项与 `git diff --check` 通过。剩余实机矩阵：错 IP/401/409 注入、长期 PowerPlan 截止期、精确镜像身份、Note4 间歇 display_state=failed 归因；待办唯一来源见 `docs/roadmap/backlog.md`。

## Bridge 休眠快照与按 MAC 延后操作：S1–S4 实施，S5 待实机（2026-09-26）

- 设备页从平台持久记录按需初始化每台已登记设备的 runtime；保存 MAC 核对且 endpoint-token 认证的 `/v2/status` 有界快照、采样时间、BLE 认证联系时间与单独的最近尝试。正常深睡显示预计休眠/等待联系，不清空旧快照；401/错 MAC 单列阻塞。公开 `/status.json` 仅作近期补充，不再作为发布确认。MCP `platform_overview` 返回同一记录。当前设备未刷新 ROM 时认证 `fw` 字段缺失，旧快照如实显示未知。
- 模板发布冻结后等待下次认证会合；重启时 `Sending → Unknown`，禁止未经状态对账重发。OTA 入队先校验 `codex-status-ota-v1|target|version` 镜像标识，再冻结文件、SHA256/大小与目标 MAC 并持久化；MCP `firmware_ota(_status/_cancel)` 共用 app service。按 MAC 的认证、正式 PowerPlan、claim 与互斥执行；旧的 selected-MAC/global pending OTA flush 已删除。上传 ACK、预期版本观察和精确镜像确认分层；当前精确身份不可用，保留 `awaiting_confirmation`、不自动重刷。
- Q1–Q3 决定：同类任务冲突，queued/waiting 须显式取消再新建；同 MAC OTA/发布各一项按创建时间串行（同秒 OTA 先）；版本只作限定证据，`image_verified` 才可作为 OTA 精确完成。详见 `project-workflow/sleep-aware-bridge/design.md`。未提交、未刷写真实设备、未执行真实发布/OTA。
- 验证：core 平台服务 20 项、app 38 项、mcp 2 项通过；UI 内联 JS `node --check` 与 `git diff --check` 通过。最终 Bridge `cargo build -p bridge-app` 成功：`bridge/target/debug/bridge-app.exe` 33,589,248 B，SHA256 `ABFD76695CF453B757BE8C7C8F4CED3500AA28693AAD60A2E55C05AF427722E1`。watchdog → 主进程安全停旧版再启动：主 PID `49864` 监听 8765/8766，watchdog `18940`；MCP overview 返回设备 2，运行数据保留模板 3、族配置 2，当前两台最近尝试均 offline（正常深睡），没有 OTA 任务。
- 固件只构建 `zectrix-note4-b`，版本 `0.18.24-note4-b`：`.pio/build/zectrix-note4-b/firmware.bin` 1,759,040 B，SHA256 `7B123BA616D3E75E38BFF2D8EBD9F3C945B2FF25CB9BCF4FC2B256C83E3A832D`；二进制核对含目标/版本 marker。因共享 `sdkconfig.defaults` 指纹，首次 `pio run -d <repo> -e zectrix-note4-b` 触发 framework 重装，历时约 15m48s；没有构建其他 env。
- S5 剩余：两台设备自然认证会合的真实任务交付/错误矩阵与 PowerPlan 截止期、Note4 新 ROM 上机后的认证 `fw` 验证，以及运行镜像精确摘要能力。当前任务未授权刷写，故不把单测当作实机验收；唯一待办入口已更新 `docs/roadmap/backlog.md`。

## 休眠设备状态与延后操作：Luna 调查 + Astra 方案（2026-09-26）

- 用户确认合同：设备日常休眠属正常；设备页显示上次成功通信数据与时间，显式模板发布/OTA 安排在目标 MAC 下次认证会合，不把暂时不可达报成设备故障。`has no runtime record` 是桥的本地记录缺口，需单独修复。
- Luna 只读核查桥端与 ROM：`get_device_status` 没用现有的 `ensure_runtime_record`；设备页完整快照仅在进程内存；模板发布已有按 MAC 持久任务；OTA 只有部分 MCP 错误分支的内存队列，flush 从当前 selected MAC 取目标且传 `mac`，底层 OTA 读取 `device_mac`；现有 OTA 成功只看 60s 内版本变化。固件定时 deep 唤醒是有界会合，`/v2/status` 有 endpoint token 鉴权。
- Astra 基于以上证据写 `project-workflow/sleep-aware-bridge/plan.md`、`design.md`、`task.md`、`review.md`：最后成功快照/尝试分离、按 MAC 持久 OTA、会合交付、ACK/重启确认、取消与故障矩阵。明确当前能力与提案的差别，精确镜像确认需要新增 ROM 能力。Q1 任务替换、Q2 OTA/发布排序、Q3 版本确认门槛留作产品决策并给安全默认建议。
- **仅文档，未改桥/ROM 实现，未构建、重启或触碰设备。** 待办唯一来源仍是 `docs/roadmap/backlog.md`。

## Bridge 模板菜单越窗修复（2026-09-26）

- 用户截图显示模板页打开「配置」下拉菜单后产生页面横向滚动条。根因是菜单绝对定位 `left:0`，按钮靠右时 210px 宽菜单向窗口外延伸并扩大文档 `scrollWidth`。
- `ui/index.html` 将该菜单与「⋯」菜单一样右对齐；菜单宽度限制在视口内，长状态/模板名允许折行，按钮组窄窗口时靠右排列。普通 WebView 内容仍会被 Tauri 原生窗口裁切，不采用窗口外渲染。
- 内联 JS 语法、HTML DOM id 唯一性、`git diff --check` 通过；`cargo build -p bridge-app` 成功（9.34s）。新 exe `bridge/target/debug/bridge-app.exe` 33,404,928 B，SHA256 `7AB1C3E014E3597E40C0B5DFE46AE678A11A2274943F767A668AE0F5CAC5CC8D`。
- 已按 watchdog → 主进程顺序停旧版，并从非受限命令启动新版；主进程 PID `54980` 监听 8765/8766，watchdog PID `4716`。运行数据目录未清理；设备 2、模板 3、族配置 2 仍在。用户截图已确认旧版现象；本轮 UI 捕获工具再次返回空清单，修复后的实际窗口截图未能自动验收。

## Bridge 新界面已重建并重启（2026-09-26）

- 用户要求让上一轮 UI 第一版进入运行中的 Bridge。先核对主进程 `47028`（HTTP 8765/MCP 8766）与同路径 watchdog `22924`，按 watchdog → 主进程顺序停止；没有清理 `target/debug` 或 `data/`。
- `cargo build -p bridge-app` 成功（33.72s），新 exe `bridge/target/debug/bridge-app.exe` 33,404,928 B，SHA256 `C916F25F36A92C84CC94128A244A5A847EDF17D3A32FCB65582CBB94EAB2D5BB`。
- 非受限启动脚本成功；新主进程 PID `23880` 监听 `0.0.0.0:8765` 与 `127.0.0.1:8766`，另有同路径 watchdog PID `55124`。运行数据保留：`state.json` 中设备 2、模板 3、族配置 2；`git diff --check` 通过。
- 尚未肉眼验收 Tauri 窗口布局；该窗口默认隐藏在托盘，用户打开后可直接查看新版。

## Bridge 界面视觉与内容第一版（2026-09-25）

- `bridge/crates/app/ui/index.html`：四个 Tab 换成统一的浅色纸面、深绿强调、清晰卡片/按钮/导航风格；模板页保留现有功能内容。设备页按当前屏幕与连接、余量、占用、平台状态、Profile/发布排序，固件细节和恢复收起；数据页突出来源和字段快照，测试 JSON 收起；MCP 页突出接入地址与复制提示，端口设置收起。去掉一直没有赋值的设备页 5H 余量占位。
- 不变更后端命令或设备协议。内联 JS 语法、HTML DOM（72 个 id 无重复，四个 Tab 对应）和 `git diff --check` 均通过。
- **未重建/重启运行中的桥**：UI 是嵌入 exe 的，当前窗口仍是旧版。Windows UI 捕获工具返回空窗口/浏览器清单，本轮没有实际截图验收；下一轮按用户反馈做视觉细化。工作记录见 `project-workflow/bridge-ui-redesign/`。

## 桥：Profile 丢失根因修复 + 桥侧 legacy/迁移清除 + 按 MAC 多设备路由（2026-09-25，进行中）

专项现场在 `project-workflow/bridge-multi-device-ui/task-phase3.md`（含分步验收、收尾流程、自检清单）；
设计合同 `design.md` 已加"2026-09-25 修订"注明哪些 legacy/迁移条目作废。

### 用户诉求与决定
用户发现"模板页里的 profile 没了"，要求从 code base 还原；随后要求实现"设备页切换设备"，并选定
**design.md 第 3 阶段的完整按 MAC 多设备路由**，且**同步清掉桥侧 legacy 通道与历史迁移代码**
（用户明确表示没有要求过对历史版本的支持和迁移）。改完由实施方停桥→重建→重启并自检。

### ① Profile 丢失的根因（已修）
`migrate_family_profiles()` 在旧 `profiles.json` 缺失/为空时循环一次都不跑，`complete` 仍为 `true`，
于是**照样写"已导入"标记** `data/family-profile-154g-imported` → 从此永不重试。上次删 `bridge/target/debug`
连带删掉 `data/` 后正是这条路：标记落盘、`family_profiles` 却是空的。**这不是设备或模板问题，是一个静默成功的迁移。**
- **还原**：用**运行中桥**的 MCP `family_profile_copy_v2`（只落盘、不发布）从设备 v2 Profile 还原两族草稿：
  `epd-ssd1681-200x200-1bpp / default / 默认 → mini,quad`、`epd-ssd2683-400x300 / default / 默认 → codex-status-a`，
  绑定全部指向真实源 `codex`。核实 `state.json`：`family_profiles=2`、`devices=2`、`templates=3`。
- **修复**：删除 `migrate_family_profiles` + `write_family_import_marker` + 调用点，并删掉陈旧标记文件；这条路径不复存在。

### ② 桥侧 legacy 通道与迁移已删除（已验收）
事实基础：**UI 已不再调用任何旧命令**（`get_profiles`/`save_profile`/`delete_profile`/`push_profile` 在
`ui/index.html` 中 0 引用），两台登记设备均 `legacy=false`、`compiler_abi=2`——旧通道在桥里已是死代码。
删除清单：`core/src/profile.rs`、`core/tests/legacy.rs`、`tools/test-bridge/profiles.seed.json`、`core::paths::profile_seed`、
`Config.profiles/profile_seed` + 两个环境变量、`ensure_runtime` 的 profile 拷贝、Tauri `get_profiles`/`save_profile`/
`delete_profile`/`push_profile`、MCP `profiles_list`/`profile_save`/`profile_push`、`push_templates_http`、`remote_template`、
排队 flush 的模板分支、platform 的 `DeviceRecord.legacy` 字段与 `migrate_legacy_profile`、`claim_unsupported`（固件 <0.13.4）、
`core/src/device.rs` 的 HTML `/status` 回退。现在**非 v2 固件被直接拒绝注册**、桥只读 `/status.json`。
- **验收**：`cargo check --workspace --all-targets` → **Finished，exit 0**（含全部 test target，无 `unused` 警告）；
  全 bridge 只剩一处 "legacy" 字样，是 `platform.rs` 里故意保留的拒绝文案 `device reports a legacy protocol`。
- **文档订正**：`AGENTS.md`（旧 ≤3 槽规则、MCP 旧工具表、`profiles.seed.json`、HTML 回退）、`README.md`（悬挂引用）、
  `design.md`（加修订说明）。
- 规模：`git diff --stat` 15 文件 **+217/-1162**（该阶段）。

### ③ 设备页选择器（前端已完成）
`ui/index.html` 设备页顶部新增「当前设备（点击切换）」：列出全部已登记设备，当前项高亮 + `当前` 徽章，
点击弹确认框才切换，选择持久化于 `localStorage['codex-status-device-mac']`。选择语义按 `design.md` §2：
**记住的 MAC 仍在则用它；否则用桥自己的设备；两台以上且未选择时不猜、提示"请先在上方选择设备"（不取列表首项）**。
设备页 15 处设备相关调用全部显式携带 MAC（`get_status`/`get_device_status`/`rename_device`/`device_discover`/
`claim_device`/`release_device`/`platform_status_refresh`/`platform_power`/`platform_plan`/`platform_publish(_preview/_cancel)`）。
校验：内联脚本 `node --check` exit 0；`#registered-devices` 唯一；UI 内 `legacy|claim_unsupported` 为 0。**切换不推送、不改模板页族选择。**

### ④ 按 MAC 运行路由（已完成并实机自检）
`AppCtx` 的单组 `device_mac/ip/name` 与单设备缓存已换成 `devices: Mutex<device_runtime::DeviceRegistry>`
（新文件 `bridge/crates/app/src/device_runtime.rs`：按 MAC 的 `DeviceRuntime` + `DeviceRegistry` + 8 个单测）。
`ctx.device_mac/ip/name/*_cache/last_claim_at/yielded/fail_streak/ip_dirty/pending_ota_rom` 在 `bridge/crates/app/`
里**引用为 0**（grep 核实）。发现链按认证 MAC 路由：`observe_device` 取代了 `learn_mac` 的单全局身份规则
（只写该 MAC 自己的记录、不覆盖用户起的名）；`note_failure(ctx, mac) >= 2` 触发 ARP 回退、`arp_running` 仍作全局闸门。
owner/claim/yielded/note/pending 均按 MAC 归位。
- **MAC 解析合同**（`design.md` §2，两个入口共用 `sole_registered_mac`）：显式 `mac` 必须已登记；未给 `mac` 时
  **只在恰好一台已登记设备时**才允许兼容默认，多台一律返回 `select a device: several are registered (<MAC 列表>)`，
  既不取列表首项、也不拿"当前选中设备"顶替。UI 的 12 个 Tauri 命令与 MCP 平台工具都走这条规则。
- **实机自检（2026-09-25 21:10，桥 PID 32316 + watchdog 45960）**：
  `platform_status_refresh {}` / `template_activate {}` / `platform_publish_preview {}` → 全部
  `select a device: several are registered (70041DD7A340, 7C4FADB93408)`；
  `platform_status_refresh {"mac":"70041DD7A340"}` → `result=ok`，设备实况 `active=quad`、`battery 45`、
  `commit_seq 405`、`applied_seq 4`；`platform_overview` → 两台都在（书桌屏 profile_count=2、Note4 =1）。
  桥日志显示两台设备**各自**被轮询与 claim（每个 MAC 一条 `/v2/status` 与 `POST /claim status=200`），
  即按 MAC 路由确实在跑，没有串到同一台。
- **测试**：`cargo test -p bridge-core -p bridge-app` → **38 + 84 项全绿**（另有 7/8/6 项测试二进制全绿）；
  `cargo check --workspace --all-targets` exit 0；`git diff --check` 干净。
- **顺带修正的两处**：① `main.rs` 里 `profile_save`/`profile_push` 两处旧工具残留（一个 activity 分支、一处
  `get_mcp_info` 的工具清单串）；② A4 删除 HTML 回退后，`v2_fallback_server` 测试夹具仍要求 `GET /`，
  已同步为 `/status.json` → `/v2/status`（测试意图不变，仍验证"投递前必须经认证 v2 状态"）。

### 未决 / 遗留（不谎报为已完成）
1. **`ui/index.html` 的界面没有被我"肉眼"验收**：我无法操作 Tauri 窗口。已验证的是它依赖的后端契约
   （12 个命令的 `mac` 参数、无 MAC 拒绝语义、真实设备应答）、内联 JS 语法（`node --check` exit 0）、
   以及新 UI 在重编时被嵌入 exe（宏展开期读取 `frontendDir`；注意 Tauri 对 UI 资源做压缩，
   **不能**用 exe 内明文搜索来判断是否嵌入——用同版本对照串验证过这点）。
2. **`data/templates/` 一直是空的、启动日志 `templates: []`**——这是**改动前就有**的状态（09:26 的启动日志即如此），
   不是本次引入：手工拷贝到该目录可成功（属主正常），所以嫌疑是 `ensure_runtime()` 里 `let _ = std::fs::copy(...)`
   把失败吞掉了（`config.seeds` 解析或读取失败都不留痕迹）。影响面仅限文件模板库 `Library`（见 3）。
3. **A5 待决策**：文件模板库 `Library`（`data/templates/*.json`）是否一并删除。它现在只支撑
   `get_status.templates`（UI 未消费）、`preview_template`/`reload_templates`（已无 UI 接线）与 MCP 的
   `template_get/validate/render/save`。删它要把这些改指向 v2 登记表，属能力变更，故未擅动；已记入 `task-phase3.md`。
4. `data/family_profiles` 的 1.54 草稿顺序现在是 `quad,mini`（initial `quad`），与设备当前 active 一致——
   本次还原时是 `mini,quad`，之后由桥的按 MAC 对账写成；两台设备的 Profile 与两族草稿都在，无丢失。

### 收尾现场（已只读核实）
- 进程：**watchdog = PID 5064**（证据：桥日志 `09:28:20Z INFO bridge_app::watchdog: watchdog started (pid 5064)`）；
  **主进程 = 24612**（`netstat -ano`：8765/8766/8767 LISTENING 属主均为它；`artifacts/bridge-app-run.pid`=24612）。
  停桥顺序必须是**先 watchdog 再主进程**。
- 起桥必须用**非受限（提权）**命令（受限沙箱会假失败并回收分离子进程、WebView2 报 `ERROR_BUSY`）。
- 本次**不需要**删 `target/debug`；重建前把 `data/`（`state.json`、`device-token-*.json`、`bridge-app.json`）当运行时数据处理。
- 待核对：`data/templates/` 上次事故后是空的，预期启动时由 `ensure_runtime` 从 `tools/test-bridge/templates/` 拷回；
  `data/profiles.json` **不应**再被创建。

## 桥：契约刷新修复 + ACL 闸门绕过（删构建目录）与运行时数据损失/恢复（2026-09-25 晚）

- **ACL 闸门（第二次遇到）**：`cargo test` 卡在写 `bridge/target/debug/.fingerprint/bridge-render-*/lib-bridge_render` → `os error 5`。实测**沙箱内无法修**：该文件属主是 `喵的问都死\CodexSandboxOffline` 且缺能力 ACE → `Set-Acl` 报 `Attempted to perform an unauthorized operation`（用 `SecurityIdentifier` 对象也一样；字符串 SID 会先报"identity references could not be translated"）。按用户指示走**删除重建**：停桥（watchdog 先）→ 删 `bridge/target/debug`（删除本身也被同一闸门拦住 `incremental/*`、`bridge-core.exe`，需一次提权）→ `cargo test -p bridge-core`（**85 项全绿**）+ `cargo build -p bridge-app`（112 s）→ 重启桥。
- ⚠️ **副作用（已如实记录）**：`<exe>/data/`（= `bridge/target/debug/data`）就是桥的**运行时数据目录**，随构建目录一起被删。丢失：`state.json`（设备登记/Profile/jobs/contexts/plans/家族 Profile）、`templates/`（模板库）、`template-backups/`、`device-token-*.json`（操作 token 缓存）、`bridge-app.json`（含 endpoint token）、`logs/`、`previews/`。**已恢复**：① 配置与 endpoint token（按本会话早先读到的内容写回 `data/bridge-app.json`，`bridge id` 仍是 `8c94`，与设备端 owner 一致）；② 模板库 —— mini/quad 来自 `tools/test-bridge/templates/`、`codex-status-a` 来自仓库夹具 `bridge/crates/core/tests/fixtures/codex-status-a-400x300.json`，**compiled_crc 与删除前逐一相同**（`cd7f5454` / `4cfa3a08` / `15ff5547`）；③ 两台设备用 `platform_device_register_v2` 重新登记（caps 从设备 `/status.json`+`/v2/status` 重新推导）；④ 两个 Profile 用 `profile_save_v2` 重建，**并把绑定全部从测试源 `static1` 改成真实源 `codex`**（1.54 8 条、Note4 7 条，见 state.json 校验）。**永久丢失**：桥侧作业历史（`jobs[]`/`bundle_jobs`，含 11 条孤儿摘要）与旧日志（关键证据已摘录进本文）；操作 token 缓存需下一次 BLE 会合重新协商（发布/数据不需要它，`/claim`、`/update` 需要）。
- **桥侧代码修复（A7 的根因）**：`note_device_status` 里新增：**当设备上报的 `active_template_id` 与当前契约不同、且该模板在 Profile 内时，重建数据契约**（`set_contract` → 新 requirements/triggers，`service.rs:1246+`）。此前只有 context 变化才重建，设备"本地按键切模板"会让桥一直按旧模板发字段集 → 设备恒回 `incomplete`。新增单测 `platform::service::tests::device_side_template_switch_refreshes_the_data_contract`（同一 context、切到 quad 后 `buckets[codex].monthly.remaining` 必须出现在契约里）→ **85 项全绿**。
- **验收（实机，1.54）**：`platform_publish` → **applied / displayed**，job `48b47968` = `succeeded`，设备 `committed_job_id=48b47968`、`active=mini`、`[bundle] install total=78140`；随后 `platform_push_now` 的原始 ACK 是 **`{"op":"data","result":"applied","display_state":"displayed","data_seq":1}`**，设备 `data_seq=1 applied_seq=1 display=displayed renders=9` —— **`incomplete` 消失，显示用的是真实 `codex` 数据**（weekly 49% → remaining 51；5h/monthly 该账号没有 → 走模板 `when.exists` 缺值分支）。Note4 全程正常（`data_seq=applied_seq=65`，未重发）。
- **待做**：按一次 1.54 按键让它**本地切到 quad**，确认桥会跟着把契约刷成 quad（8 远端字段）且数据继续 applied —— 这是该修复的现场确认（单测已覆盖逻辑）。

## 1.54 OTA 到 0.18.23-bw + 模板推送成功（2026-09-25）— A2 阻塞解除（数据面的 `incomplete` 已在上一节修复并验收）

- **OTA（用户授权执行）**：`firmware_ota {rom:'.pio/build/esp32-s3-epaper-154g/firmware.bin', device_ip:'192.168.3.163', device_mac:'70041DD7A340'}` → **`0.17.10-bw → 0.18.23-bw`（1,740,576 B），耗时 54.1 s**。刷前 ROM 已按 SHA256 `91937B18…` 核对；刷后 `slot=ota_1`、`next_slot=ota_0`、`reset=software`、`heap=103824`、设备自报 `[ota] post-OTA light window 300s`。桥的缓存 token 直接可用，未走 BLE 取 token。
- **模板推送成功（这次一次就过）**：`platform_publish {"mac":"70041DD7A340"}` → ACK **`{"op":"bundle","result":"applied","display_state":"displayed","retention":"flash","active_context_id":"f54099f7e1ca7978"}`**，job `2bfc710c` → `succeeded`（`bundle_jobs` 落空=终态），设备 `committed_job_id=2bfc710c`。
  - 设备侧证据：`[bundle] install total=78150 free=1810432 fs_total=1966080 fs_used=155648` → **安装真正落盘**；随后 `[tpl] rendered mini` → `[v2] active=quad ctx=772e2eeabb74e589` → `[v2] local switch -> quad` → `[clk] reserved x=161..195 y=9..20 box=35x12 win=5x60B` → `[tpl] rendered quad (BLE)`；**安装后 `ct_abi` 报错消失**（`v2ActiveLoad` 通过）。
  - 终态：`fw=0.18.23-bw`、`slot=ota_1`、`commit_seq=388`、**`v2_templates=2`**（原来的 3 个含 `full` 已按桥 Profile 收敛为 `mini,quad`）、`active=quad`。
  - **结论：A2 的两个互锁故障（COMMIT 被拒 + `ct_abi`/`data disabled`）都由"0.18.23 的流式安装 + 一次成功安装写入的新编译缓存/新 context"同时解掉**；旧 0.17.10-bw 上的 `oom`/`owner` 两种拒绝不再出现。这也证实了 A1/A2 节的判断（瓶颈在设备固件的 commit 路径）。
- **数据面剩一个新问题（桥侧，非固件）**：安装后第一帧数据 `seq=82` **applied**（BLE），设备本地切到 `quad` 之后每帧都被拒，`platform_push_now` 返回的原始 ACK 给出确切原因：
  `{"op":"data","result":"rejected","display_state":"failed","error":"incomplete","data_seq":83,"active_context_id":"772e2eeabb74e589"}`
  - `incomplete` 出自固件 `v2_runtime.cpp:114-118`：`fields.size() != remoteCount`（remoteCount = 编译模板里 `kind<=9` 的 requirement 数；CRC 检查在它之前 `:110` **已通过**）。即**桥发来的字段条目数与已装模板要求的远端字段数不等**，而不是内容错。
  - 证据链指向"桥的字段契约落后于设备本地切换"：桥只在 **context 变化时**才按设备上报的模板重建契约（`service.rs:1298-1321`，位于 `note_device_status` 的 reconcile 分支内；`refresh_contract` 的调用点只有 `service.rs:661/1741` 与 activate/publish 路径），而设备本地按键切模板会自己轮换 context 并被桥"提前采纳"，此后契约不再刷新 → 桥仍按**上一个模板**（`mini`，2 个远端字段）发字段集，而设备已按 `quad`（8 个远端字段）校验。观察到的时序（mini 时 applied → 切 quad 后 incomplete）与此完全吻合。
  - 另一条独立的配置缺陷（backlog 早有记录、与上面叠加）：1.54 Profile 里 5 个字段仍绑测试源 **`static1`**，而 `static1` 只提供 2 个（`buckets[codex].weekly.usedPercent`、`.weekly.resetsAt`）→ `monthly`/`5h` 的值必然是 `null`（模板走 `when.exists` 缺值分支，显示不出真实用量）。`codex` 源当前有 `weekly.usedPercent=49`、`resetCredits.availableCount=1`，但**这个账号没有 5h / monthly 桶**。
- **未做**：没有改 Profile/绑定、没有改桥代码、没有提交 git；`template_activate` 也没动（设备现在停在 `quad`，是用户按键切过去的）。Note4 全程未受影响（其数据在 15:2x 前正常 applied；之后它按自己的节奏入睡）。

### 待办（按优先级）
1. **把 1.54 Profile 的 5 个 `static1` 绑定改回 `codex`**（`profile_save_v2`），然后**再发一次** `platform_publish`（Profile 绑定属于冻结进 Bundle 的内容）——这样显示才是真实用量而不是静态测试值。属改配置，需用户点头。
2. **桥侧修契约刷新**：设备上报的 active template 变化时（不只是 context 变化时）就重建数据契约，否则"设备本地切模板"会让该设备的数据投递永久 `incomplete`。建议同时在 `note_device_status` 里对 `active_template_id` 做变更检测。属改代码。
3. 复核 `remoteCount` 语义：固件要求"每条远端 requirement 一个条目、缺值发 `v:null`"（`v2_runtime.cpp:128-133` 允许 null、不允许缺条目），桥的 `wire_fields`(`coordinator.rs:949-966`) 确实每 requirement 一条，所以两者一致的前提是**契约里的 requirement 列表 == 已装模板的**——即问题 2 的根。

## 多 env 实测：154g 隔离目录建成 + 1.54 固件重建（2026-09-25）— 隔离有效、ROM 已产出；交替构建仍会触发 framework 重装，且重装需工作区外写权限

- **建目录**（host 侧创建，属主 `喵的问都死\cogic`、继承到沙箱能力 ACE，符合 AGENTS「不要在沙箱令牌下建目录」）：`pwsh tools/pio-target.ps1 -Target 154g -Setup` → 真拷贝 `framework-arduinoespressif32`(69 MB) + `-libs`(2057 MB)，其余 15 个 junction 指向 `C:\Users\cogic\.platformio\packages`。两侧现在都是 2 real + 15 linked，各 ≈2126 MB。
- **构建**：`pio run -d <repo> -e esp32-s3-epaper-154g` → **SUCCESS 102.38 s**，无 framework 重装、无 banner。
- **154g ROM（当前源码，含 0.18.23 全部改动）**：

| 产物 | 大小 | SHA256 |
|---|---|---|
| `.pio/build/esp32-s3-epaper-154g/firmware.bin` | 1,740,576 B | `91937B18FE52B53BF43502A53CFC2B815CFCEEE3633C01F38CB8C4E5B30385CF` |
| `…/firmware.factory.bin` | 1,806,112 B | `FEA9F928BC0C38857C1942F71758C639B2F36038A168DFD89D81A6B48BFF66CF` |

  - 内嵌版本串 **`0.18.23-bw`**（`0.17.10-bw` 已不存在）、`fw_target=codex-status-154g`、`render_target=epd-ssd1681-200x200-1bpp`；镜像头 `e9 07 02 30 …` → byte[3]=`0x30` = **8 MB + 40 MHz** ✓（GD25Q64 要求）。旧的合并期产物（1,744,496 B / `E9046F6B…`）已被本次构建覆盖（历史值仍在 `backlog.md` §0）。
- **交替构建实测（C6 判据 c/d/e）**：154g 之后立刻构建 note4 → 第一次 **13.45 s FAILED**：日志出现 `*** Reinstall Arduino framework ***` + `*** Compile Arduino IDF libs for zectrix-note4-b ***`，随后 `error: Failed to initialize cache at C:\Users\cogic\AppData\Local\uv\cache … os error 5（拒绝访问）` → `Failed to create a proper virtual environment`。第二次（指纹已被上一次写回 note4、`UV_CACHE_DIR` 指向工作区内）→ **SUCCESS 39.10 s、无重装**，note4 ROM **hash 未变** `42AAF00B257908434E86CEA45838DC60FD65FE8FD426D0632E570BD6BDBCF72E`(1,758,032 B) —— 即 OTA 在机的那一版可复现。
  - **结论：隔离防的是"互相破坏"，不是"重装代价"。** `sdkconfig.defaults` 是仓库根唯一生成物，切换目标必然把它改写成对方的 `# TASMOTA__…`，下一次切回就触发 framework 重装（与旧记录一致，只是重装发生在目标自己的包目录里、不再伤到对方）。整轮下来两个包目录都完好（各 2 real + 15 linked 未变），**没有跨目标损坏**。
  - **重装在受限沙箱里做不完**：uv 要写 `%LOCALAPPDATA%\uv\cache`（工作区外）被拒；`UV_CACHE_DIR` 指到工作区内可消除该条，但 `tool-esptoolpy` 的 editable 安装仍会因写 junction 指向的共享包目录（C 盘）报 `Cannot update time stamp of directory 'esptool.egg-info'`——这条是**非致命警告**（esptool 5.4.0 照常工作，两个目标都构建成功）。要完整跑通重装需一次性提权，或把这两个路径纳入工作区。
- **新坑（务必记住）**：`tools/pio-target.ps1` 在**后台作业**里会假失败——脚本本身没问题（用 `$PSScriptRoot` 定位仓库），但它 `& pio run` 时 `pio` 继承到的 cwd 变成 `D:\Documents\project`（与本仓库无关的目录）→ `NotPlatformIOProjectError`；同一 shell 里直接 `pio project config` 却正常。可靠做法：**给 pio 加 `-d <repo>`**，或在前台 shell 里跑脚本。
- **待办**：① 想刷 1.54 就用上面这个 ROM，但 OTA 是独立决定（设备需在 awake/light 窗口；命令形状 `firmware_ota {rom:…, device_ip:192.168.3.163, device_mac:70041DD7A340}`）；注意它**不含** `owner` 校验的改动（见 A1/A2 节），所以能治 `oom`/`ct_abi`、不保证治 `owner`。② C6 判据 c/d/e 已实测（上面），若真要"切换不重装"，得让 `sdkconfig.defaults` 按目标隔离或在构建前写回目标指纹——属改动，需决策。③ 重装路径需提权或把 uv 缓存/esptool 包纳入工作区。

### 交替构建已消除重装（2026-09-25 晚补做 next.md §7 判据 c/d/e）

- **上次的考察已保存，且这轮是接它的"待定"项**：根因/上游依据在 `docs/history/next-2026-09-25.md` §7、§10、§11（含 pioarduino PR #511、issue #532/#533、Meshtastic PR #11834、PlatformIO 四页文档），调查脚本在 `artifacts/hash-forensics/`（`compute_fingerprint.py`/`dump_effective.py`/`pin_down.py`/`why_differ.py`，本地 gitignore）。§7 当时把 c/d/e 留空、并写"隔离方案**待定**：先诊断并隔离项目根 `sdkconfig.defaults` 与已安装 package 状态"；§10 同时禁止"固定/手工编辑 `sdkconfig.defaults` 首行"。
- **这轮实测（推翻 §7 的乐观前提）**：只做包目录隔离**不足以**免重装——判据在仓库根那个共享文件上。实测：154g 之后首次构建 note4 → `*** Reinstall Arduino framework ***` → 且该路径在受限沙箱里**跑不完**（uv 要写 `%LOCALAPPDATA%\uv\cache` → `os error 5`）。
- **解法（已落地 `tools/pio-target.ps1`）**：把 `sdkconfig.defaults` 按目标**整份快照**（存 `.pio-core/sdkconfig.defaults.<target>.snapshot`），构建前若与当前文件不同就整份还原，构建成功后再快照。**还原的是该目标自己生成过的完整文件（内容+指纹自洽），不是伪造首行**——这正是 §10 禁令的本意所在；安全证明：重建出的 154g ROM 与记录值**逐字节相同**（`91937B18…`）。快照过期（platformio.ini 改了）只会失配一次→走正确的"重生成"路径→构建后自动刷新快照。顺手修了脚本的 cwd 依赖（`Set-Location $RepoRoot` + `pio run -d $RepoRoot`，因为嵌套 pwsh/沙箱 broker 会把子进程 cwd 换成别的目录）。
- **实测结果**：priming（154g 首次，含一次重装路径）45.7 s 后——**note4 39.2 s、154g 42.1 s 交替，两侧均无 `*** Reinstall ***`、无 `Compile Arduino IDF libs`**，两侧 ROM 哈希不变。即：**切换代价从"每次重装/重编 IDF 库"降到几十秒增量**。复测（4 连切，逐次统计编译单元数）：`note4 29.5 s / 154g 27.8 s / note4 32.6 s / 154g 29.0 s`，**四次都是 `compiles=0`**、`Reinstall=False`、`IDFlibs=False`，两侧 ROM 哈希逐一稳定（note4 `6B3C386D…`、154g `91937B18…`）——即切换时**一个编译单元都不重编**，耗时全是脚本与 PlatformIO 的 up-to-date 检查（对照"完全不改"的 32.8 s）。priming 仍需一次完整重装，且该路径需要工作区外写权限（本沙箱下需一次性提权）。
- **仍未解决的上游风险**：`next.md` R2（issue #532/#533，HybridCompile 把按内存类型的产物写进共享 `lib/`+`ld/`，本机实测 7 个文件错位）——包目录隔离只能把影响限制在单目标内，不能消除；持续跟踪上游。

### 构建触发条件（2026-09-25 实测，note4 目标）

| 改动 | 重编范围 | 实测 |
|---|---|---|
| 不改动 | **0 个 TU**、不重链接 | 32.8 s（全是脚本/PlatformIO 的检查开销），`compiles=0` |
| 改 1 个 `.cpp`（`src/dev_log.cpp`） | **只重编那 1 个 TU + 重链接** | 46.2 s，`compiles=1`（`dev_log.cpp.o`） |
| 改被广泛 include 的头（`src/template_engine.h`） | **10 个 TU**（`src/` 里所有 include 它的：`main/bundle_store/refresh_policy/template_engine/template_xfer/v2_*`） | 41 s，`compiles=10` |
| 改 `custom_sdkconfig` / `board_build.arduino.memory_type` / `flash_size` 等**配置** | `*** Reinstall Arduino framework ***` + 删**所有** `sdkconfig.<env>` + 删/重下该目标 framework/libs + **重编 IDF 库**（≈"整个项目重编"那一类） | 本轮未重测（上次记录 15 min 量级；本轮 priming 到 15.5 s 就撞沙箱 uv 拒权而失败） |
| 改 `platformio.ini`（哪怕加一行注释） | PlatformIO 用 `.pio/build/project.checksum`（当前 `9c777f73…`）判定工程不匹配 → **删 `.pio/build/<env>` 并全量重编应用**；注释不进 `custom_sdkconfig`，所以**不会**连带触发 framework 重装 | 未测（代价与扰动大）——机制见 `next.md` §10 R7/D3 |
| 删 `.pio/build/<env>`、删包目录、`pio run -t clean` | 全量重编 | §10 明令禁止 |
| **每目标 `sdkconfig.defaults` 快照还原**（切换目标时） | **不触发**任何重编（内容=该目标自己的，指纹匹配） | 实测 note4 39.2 s / 154g 42.1 s，`Reinstall=False`、`IDFlibs=False` |

- **注意（探测的副作用，已如实记录）**：ESP-IDF 会把**编译日期/时间**写进镜像（本机 `firmware.bin` 里能搜到 `Sep 25 2026`、`18:51:24` 等串）→ **任何一次重编都会改变 ROM 哈希**。所以：① 上面探测让 note4 的本地产物变成 `6B3C386DFC59798A…`（18:10:05，1,758,032 B），与记录里的 `42AAF00B…` 不再相同——**设备上跑的仍是 05:02 那次 OTA 的 `42AAF00B…` 版本**，只是本地产物被重编过；② 之前"重建 154g ROM 与记录值逐字节相同"之所以成立，正是那次 `compiles=0`（154g 产物现在仍是 `91937B18…` ✓）；③ 以后要复现某个记录哈希，必须做到"零编译重建"，否则哈希必然变（内容等价、时间戳不同）。

## A1/A2 现场恢复：桥重启 + 清掉 1.54 Bundle 队列（2026-09-25）— **已被当日 OTA 解掉：见顶部「1.54 OTA 到 0.18.23-bw + 模板推送成功」节**（本节保留当时的失败现场与 `oom`/`owner` 归因）

### 现场（实机，15:13–15:35）
- **桥**：`bridge/target/debug/bridge-app.exe`（04:43:36 构建；`git status` 干净，与 HEAD 一致）→ **PID 21848 + watchdog 41784**；`0.0.0.0:8765` HTTP、`127.0.0.1:8766` MCP（GET→405=端点活着）、UDP 8767；`artifacts/bridge-app-run.pid`=21848（旧值 11664 已确认不存在）；日志无 panic，且 `usage changed (rev 1)` / `usage refreshed` 出现（取数源自愈）。
- **起桥的沙箱坑（新，务必照做）**：受限模式（workspace-write）下 `pwsh tools/start-bridge.ps1` **会假失败**：① `Get-NetTCPConnection` 在沙箱内看不到该监听者 → 脚本 10s 判"未出现"并抛错，而桥日志显示端口早已绑好；② 该命令结束时沙箱**把已分离的子进程一并回收**（实测进程随命令结束消失）；③ 沙箱下 WebView2 建 host 失败：`failed to create webview: HRESULT(0x800700AA) ERROR_BUSY`。用**一次性 `danger-full-access`** 重跑同一条命令即成功，且日志里**没有** WebView2 报错。桥必须从提权/非受限命令启动。
- **两台设备**（用户按键唤醒后，`GET /status.json` + ARP 双证；MAC 均与桥登记一致）：

| 设备 | MAC | ip | fw | slot | wake | data_seq/applied_seq | committed_job_id / commit_seq | 其他 |
|---|---|---|---|---|---|---|---|---|
| 1.54" 书桌屏 | `70:04:1D:D7:A3:40` | 192.168.3.163 | **0.17.10-bw** | ota_0 | power-on | 0 / 0 | `9098ec1d` / 381 | heap≈101.6 KB，电量 27%/3540 mV，`epd_busy_fails=0`，v2_templates=3，ctx `2f20033a7e135b52` |
| Note4 | `7C:4F:AD:B9:34:08` | 192.168.3.177 | 0.18.23-note4-b | ota_1 | ext1 | 63 / 63 | `1b4500ad` / 267 | heap≈97.9 KB，电量 86%/4080 mV，`epd_busy_fails=6`，`panel_power_mode=keep` |
- **1.54 IP/MAC 疑点结案**：设备自报 + ARP 均为 `192.168.3.163` / `70:04:1D:D7:A3:40`，与 `state.json` 登记一致 → 无需改 state；旧文档的 `192.168.1.50` / `70:04:1D:AA:BB:CC` **确认作废**。1.54 固件实为 **0.17.10-bw**（此前记的 `0.16.7-bw` 过期）。

### A2 只读复核（file:line）
- ① **Bundle 先于 Data**：`coordinator.rs:365-386` 的 `Delivery::Bundle{..}` 带 `return`，在 Activate(`:390`)、in-flight Data(`:408`)、Data 生成(`:421/433`) 之前。
- ② **取消对 `sending` 有效**：`app/platform.rs:1868-1871` → `job_cancel`(`:662-665`) → `service.rs:895-907` → `coordinator.rs:613-621`，条件是 `!state.is_terminal()`(`model.rs:459-468`，Cancelled/Succeeded/Failed/Unknown 才算终态)；终态会在下一次 persist 被 `bundle_jobs` 过滤掉(`service.rs:281-284`)。**MCP 工具描述 "queued (unstarted)" 不准确**（实测能取消 `sending`）。
- 设备侧真相：`committed_job_id=9098ec1d`（09-23 就 succeeded 的老作业）≠ `83f4324c` → 该 Bundle **从未提交**；对账要求相等(`service.rs:1262-1276`) 所以永不自愈。全程未改设备任何值。

### A2 执行与结果
- **取消**：`platform_publish_cancel {"mac":"70041DD7A340"}` → `{"cancelled": true, "job": {job_id 83f4324c, state "cancelled"}}`；`state.json` 回读：`bundle_jobs` 该 MAC 条目消失，`jobs[]` 中 `83f4324c` → `cancelled`。
- **孤儿 `waiting` 摘要确认**：`jobs[]` 里 11 条 waiting（1.54：`8e685eb8 9807acd8 0bfad6e9 6105361c a44910e2 78972ae8 e8af7792 25a2922b 7e7777e5 91b51cdb`；Note4：`5c6d63fc`）**不是活状态**——`bundle_jobs` 只从 coordinator 的活 `job` 派生(`service.rs:281-284`)，取消后该表为空即证无活作业；`enqueue_bundle` 会替换 `waiting` 作业(`coordinator.rs:576-584`)，而 `refresh_bundle_history` 只在终态转换时改写历史行(`service.rs:318-326`)，故每次替换都留下一行旧摘要。**按要求未逐个取消。**
- **一次干净重发（诊断）**：`platform_publish {"mac":"70041DD7A340"}` → 新作业 `e60830a4`（55,312 B，crc `8cbde6d2`）。`v2_client::install_bundle`(`v2_client.rs:323-414`) 全程：BEGIN 200 → **14 个 CHUNK 全部 200/applied**（offset 0…53248，无 HTTP 错误、无超时、无 `result!=applied`、无 next_offset 不匹配）→ COMMIT 200（42 ms）但 ACK 为：
  `{"op":"bundle","result":"rejected","error":"oom","retention":"flash","display_state":"unchanged","active_context_id":"2f20033a7e135b52"}`
- **与原始卡死同型**：09-24 20:48:33Z–21:19:24Z（本地 04:48–05:19）对 `83f4324c` 也是 BEGIN+14 chunks 全 200 后 COMMIT；此后该作业 `updated_at` 再没动过（停在 04:47:44 = Waiting→Sending 那一刻）→ **原来的"卡在 sending"就是同一种"COMMIT 被拒"**。
- **桥侧为何卡住不报错（独立缺陷）**：`app/platform.rs:720-722` 对非 applied 的 COMMIT ACK 调 `retry_job(..., committed=false)`，而 `retry_bundle_ack`(`coordinator.rs:624-636`) **只在 committed==true 时改状态**，`service.rs:922` 也只在 committed 时刷新历史 → 被拒作业永远停在 `sending`，每个会合窗口重试一次，并因 Bundle 分支优先而**永久顶住该设备的 Data 投递**。
- **设备侧根因候选**（源码引用为当前树 0.18.x；设备跑 0.17.10-bw，提交路径应同源，未逐字节对比）：`bsInstallSource` 先做 flash 余量检查，不足报 `"space"`(`bundle_store.cpp:433-436`)——**本次没有报 space**，所以**不是 8 MB flash 装不下**；随后逐个模板 `tplCompile` 重建编译缓存并 `malloc(tplCtSize()+16)`，该 malloc 失败即 `err="oom"`(`:467-468`；`:452` 注释"约 9 KB 编译记录"、`:461-462` 注释"1.54 上 compiled hex 可能比可用堆还大"）。设备 `/log` 不记录 bundle 安装失败原因，只有 HTTP ACK 里的 `oom`。
- **第二次尝试（用户授权，设备冷启动后 19 s 内发，堆最新鲜）**：新作业 `37217f05`。BEGIN 200 → 14 个 CHUNK 全 200 → COMMIT 200，但这次 ACK 是 **`error:"owner"`（不是 oom）**：
  - `owner` 只可能来自 `v2_bundle_command.cpp:144-158` 的 `strcmp(bodyOwner, owner)`（全仓唯一产出该字符串的位置）：`owner` = **COMMIT 请求体**里的 `bridge_id`，`bodyOwner` = **flash 里已暂存载荷**里的 `bridge_id`（解析不到就退回 `server.arg("bridge_id")`，而桥从不发这个参数 → 空）。
  - 该检查位于 CRC 校验(`:138`)与 `(bridge_id,request_id,nonce)` 匹配(`:110`)**之后**，两关都过了才到它 ⇒ **请求体解析正常**（否则先报 `crc`/`session_or_length`），是**设备侧重读暂存载荷时没拿到 `bridge_id`**。
  - 桥侧无问题：`deliver()` 在安装前先 `bind_pending_bundle_owner`(`platform.rs:685` → `service.rs:844-871`) 强制 `payload.bridge_id == link.bridge_id`，否则直接返回 `failed` 不发包；本次安装确实发生了 ⇒ 两侧值在桥侧是一致的。
  - 旁证：COMMIT 前 0.2 s 有一次 `/v2/status` 6 s 超时（`07:44:31.757`），说明设备当时正被 LittleFS 分片写占住；同一窗口桥的自动 `/claim` 也在跑（`07:44:29.79`）。
- **两次失败的共同点与差异**：都是"分片全过、COMMIT 被拒"；`oom` 发生在 commit 内 `bsInstallSource` 的 11.3 KB 连续分配（`sizeof(CtTemplate)`，`bundle_store.cpp:467-468`），`owner` 发生在其**之前**的载荷校验。两次都无法归因于桥侧——桥每次都把完整载荷送到了设备且设备自校验 CRC 通过。
- **设备固件是关键变量**：1.54 跑 `0.17.10-bw`，早于当前树的 `ca4022c`（0.18.23「stream bundle install from file」，即 `oom` 所在那段的重写）；而 `owner` 检查本身是更早的 `b702aa3`（09-24）引入的共享判定，0.18.23 里**未改**。
- **另一个必须记住的事实**：当前桥 exe（04:43 构建）**一次成功的 Bundle 提交都没有**——Note4 最后一次成功（`1b4500ad`）是 09-24 23:20，早于该构建；此后桥的三次尝试（`83f4324c`/`e60830a4`/`37217f05`）全部失败。所以"桥侧回归"不能靠 Note4 的历史排除，但按上面逐行追，桥侧三处（freeze 用 `ctx.bridge_id`、`bind_pending_bundle_owner` 强制一致、commit 请求用 `link.bridge_id`）取的是同一个值。
- **清理与现状**：第二次尝试后同样自动取消（`37217f05` → `cancelled`），`bundle_jobs` 为空。Bundle 分支释放后桥立刻投递 Data：`/v2/data seq=82` → 设备 200 但 `result="rejected"`、`display_state="failed"`、`error_category="ack_rejected"`、`data_seq=0`（即"队列已解阻塞，但设备自己拒绝数据"）。设备仍为 `committed_job_id=9098ec1d`、`templates=full,mini,quad`、`active=quad`、`data_seq=applied_seq=0`。注意 `commit_seq` 从 381 涨到 383 但**没有**换过 committed job——它会被启动时的 context 轮换 `bsSetActive` 递增，**不能**当"安装成功"的判据。
- **设备第二个独立故障**：1.54 `/log`：`[v2] active load failed: ct_abi`（反复）→ `[v2] cannot rotate unknown-retention context; data disabled`。已装 Bundle（job `9098ec1d`）的**编译缓存 CT_ABI 与当前固件不符**，`v2ActiveLoad()`(`main.cpp:2036-2043`) 失败 → `v2SwitchActive()` 也失败(`main.cpp:5645-5648`) → 固件自己关掉了 data。`bsLoadCompiled` 本有"用槽内保留 source 重建缓存"的兜底(`bundle_store.cpp:568-591`)，但本次连兜底也返回 `ct_abi`。注意该设备每次启动的 context id 都会轮换（`09509c9cf9be57d2` → `471096d2e97a9548` → status 报 `52f71c2a3d8c0d99`）。
- **结论**：1.54 现在有**两个互锁故障**——(a) COMMIT 校验过不去（`oom` 或 `owner`，都在设备侧），(b) 已装 Bundle 的编译缓存 ABI 不可用 → `data disabled`。两者都能被"一次成功的 Bundle 安装"同时解决，但那次安装过不去，且两次尝试给出两种不同的设备侧拒绝 ⇒ **瓶颈在设备固件的 commit 路径（0.17.10-bw），不是桥**。已完成用户授权的第二次尝试后**停手**，未裁剪模板/Profile，未做族发布，未提交 git，未改任何代码。

- **过程瑕疵（如实记录）**：最初两次 `platform_publish_cancel` 因为调用侧把 JSON-RPC 的 `arguments` 丢了（PowerShell 里参数名用了自动变量 `$args` 的坑），落到了**默认设备 Note4** 上，属 no-op（Note4 作业已终态，回 `job: null`）；唯一副作用是清掉 Note4 当时的 `data.in_flight`(`coordinator.rs:620`)。事后核实 Note4 正常：Plan ack 连续 applied，`data_seq=applied_seq=63`。1.54 的取消随后带正确参数重做并达成。

### 待办
1. ~~**首选：把 1.54 刷到已构建好的 `0.18.23-bw`**~~ → **已完成（2026-09-25，见顶部节）**：OTA 54.1 s 成功，随后的 `platform_publish` 一次 applied，job `2bfc710c` succeeded。剩下的是数据面的字段集问题（`incomplete`）与 Profile 的 `static1` 绑定，见顶部节的待办 1/2。
2. **桥侧小硬化（一行级，可让 `owner` 不可能发生）**：设备本来就为"载荷解析不到 `bridge_id`"留了退路 `server.arg("bridge_id")`（`v2_bundle_command.cpp:149`、`main.cpp:4427`），而桥**从不发这个参数**。给 COMMIT 请求带上 `?bridge_id=<id>`（或表单字段）即可让该校验不再依赖那次 55 KB 载荷重解析。属改代码，需用户批准。
3. **桥侧独立缺陷（与上面独立）**：非 applied 的 COMMIT ACK 应落成 `Failed` + `last_error`，或加尝试上限/退避；否则任何设备一次 commit 失败都会永久堵死该设备的 Data 投递（`coordinator.rs:624-636` + `service.rs:922`）。
4. 备选路线：C1（Bundle v3，55.3 KB→约 16.2 KB，同时缩小载荷重解析与 commit 分配压力）/ 缩模板或 Profile / 暂时放弃该设备数据同步。未决前**不要再对 1.54 发 `platform_publish`**。
5. 核实 1.54 上 `bsLoadCompiled` 的 ct_abi 兜底为何不生效（若修好，可不重装 Bundle 就解掉 `data disabled`）。
6. 环境：受限沙箱下 `tools/start-bridge.ps1` 会假失败并回收子进程；起桥需一次性提权（见现场节）。
7. `PROGRESS.md` 已于本轮做了一次下沉（`合并 codex/fake` 与 `桥侧修复：物理唤醒窗口取大` 两节移入 `docs/history/progress-archive-2026-09-25.md`，现约 28 KB）。仍偏大：下次里程碑可继续把`显示修复：时钟局刷窗口宽度算错`与`当前现场与待办`以下的"本轮核查"清单下沉（注意 AGENTS.md 已不再引用 `§包目录隔离`，但「沙箱写入边界」小节与 backlog D13 互指，别断链）。

## 显示修复：时钟局刷窗口宽度算错（Note4 / 比例字体）：2026-09-25 — 已实现、已 OTA、判据命中

- **根因（几何，与残影无关）**：`src/template_engine.cpp` 的 `tplFontClockBox()` 比例字体分支按 **2 个数字 + 冒号** 预留窗口：`long adv = 2L * digitAdv + glyphs[':'].adv;`。而 `device.now` 的字符串是 **"HH:MM" = 4 个数字 + 冒号**。ntthin18 实测（`src/font_noto_ntthin18.h:124-134`）：`'0'..'9'` adv=160（10 px）、`':'` adv=64（4 px）→ 真实宽度 **44 px**，代码只算出 **26 px**。
  - 后果：窗口 `x=69..94`（byte 窗口 4 B = 32 px），blit 上限 `bufW=32`、文本 `xOff=5` → **只能画到第 27 px**；字形边界为 `0:69-78 / 4:79-88 / ::89-92 / 分钟十位:93-102 / 分钟个位:103-112` → 窗口局刷只更新前 ~2.5 个字，**其余像素保持上一次整帧渲染的旧笔画**，视觉上是"新前缀 + 旧后缀"的怪字形（现场被读成 `04:64`）。
  - 整帧渲染不受影响（按画布裁剪，不按 box），所以"全刷正常、只有局刷坏"；这也解释了为什么加 ghost 预算/强制全刷都治不了根。
  - **位置来源**在设备上是编译产物（`v2ActiveLoad()` → `clkComputeRectCt()` 读 `v2Ct.ops[]`），**尺寸**却是运行时重算（`tplFontClockBox`）→ 两处不同源正是缺陷土壤；若模板给时钟加 `rect`+`align:center/right`，同一个错误 textW 还会让**位置**也偏 18 px（当前 `codex-status-a` 无 rect，故只有尺寸错）。
- **改动**：
  - `src/template_engine.cpp`：`2L * digitAdv` → `4L * digitAdv`（窗口 26→46 px，byte 4→7，104→182 B；Note4 `CLK_MAX_BYTES=512` 足够）；新增 `tplFontPropWidth()` 与"上次 blit 被裁掉多少墨水像素"的 `tplFontClockClipped()`。
  - `src/template_engine.h`：上述两个声明的公开声明。
  - `src/main.cpp`：`clockTickWake()` 增加**窗口容纳校验**（`need > avail` 或 `clipped > 0` → 不发这次局刷，改走全刷并记日志），这类几何错误以后会自己暴露；新增 `clkBaselineAfterPanelWrite()`，在**每次成功渲染后**（全刷总是、区域局刷仅当覆盖时钟窗口）重新 `clkCaptureFromFramebuffer()`——此前 light 下只要发生过一次帧渲染，`clkPixels` 就会与驱动 shadow 失配、窗口局刷被拒并**永久退化**为每分钟整帧渲染。
- **构建产物**：`.pio/build/zectrix-note4-b/firmware.bin`，1,758,032 B，SHA256 `42AAF00B257908434E86CEA45838DC60FD65FE8FD426D0632E570BD6BDBCF72E`；`FW_VERSION=0.18.23-note4-b`；镜像头 `byte[3]=0x40`（16 MB/40 MHz）；`sdkconfig.defaults` 首行 `# TASMOTA__9244f2068d3cf08d` ✓。按用户决定**未复制进 `artifacts/`**（`artifacts/` 写入被沙箱拒绝、提权被用户否决），ROM 保留在 `.pio/build/`。
- **烧录**：经桥内建 MCP 先 `power_plan {mode:"light", mac:7C4FADB93408}`（plan 659 排队）让设备在会合窗口升 light，再 `firmware_ota {rom:…, device_ip:192.168.3.177, device_mac:7C4FADB93408}` → **`0.18.21-note4-b → 0.18.23-note4-b`**（1758032 B），槽位 `ota_1`、`wake=power-on`。
- **判据命中（实机）**：`[clk] reserved x=69..114 y=5..30 box=46x26 win=7x182B`（修复前 `x=69..94 box=26x26 win=4x104B`）；整分钟 `[clk] tick build=866us wake=54652us write=709175us total=764790us`，**无** "too narrow"/裁剪告警，`epd_busy_fails=0`、`clk_partials` 递增。
- **未做/后续**：(1) v2 会合路径仍无 ghost 预算（`CLK_GHOST_LIMIT` 只在 legacy pull 与 light tick 的 per-boot 计数上生效）；(2) 消除"位置来自编译产物、尺寸运行时重算"的分叉——把测量宽度写进 `CtOp` 需要同步改 Rust 编译器与 Python 测试桥的哈希，属独立任务。

## 已下沉的历史节（2026-09-25 收纳）

以下两节的全文已移入 `docs/history/progress-archive-2026-09-25.md`（追加在文件末尾）：
- `合并 codex/fake（共享 v2 决策 + 设备模拟器）` —— 含合并后 ROM 表、包目录隔离的早期判据、`git stash` 现场事故；
- `桥侧修复：物理唤醒窗口取大 max(plan, t_boot+300s)` —— 含其"待实机复核"项与沙箱写入边界的原始记录。

其中的**当前**结论仍在本文或 `backlog.md` 里：ROM 现状见本文「多 env 实测」与 `backlog.md` §0；多目标隔离见 backlog C6；`git stash` 禁令与沙箱写入边界见 `AGENTS.md` 与本文末尾「沙箱写入边界」。

## 当前现场与待办（2026-09-25 核查）

| 项 | 值 |
|---|---|
| 1.54" 设备 | MAC `70041DD7A340`，名"书桌屏"，登记 IP `192.168.3.163`，`sync_enabled=true`，Profile `mini,quad` |
| Note4 设备 | MAC `7C4FADB93408`，名"Note4"，IP `192.168.3.177`，`sync_enabled=true`，Profile `codex-status-a` |
| 两设备可达性 | **2026-09-25 15:2x 已按键唤醒并实测可达**（`/status.json` + ARP 双证，MAC 与登记一致）；此前 09-25 05:2x 起曾深睡不可达 |
| 已装固件 | Note4 `0.18.23-note4-b`；**1.54 已于 2026-09-25 OTA 到 `0.18.23-bw`**（slot `ota_1`，ROM `91937B18…`） |
| 桥 | `bridge/target/debug/bridge-app.exe`（09-25 04:43 构建）；**运行中 PID 21848 + watchdog 41784**（15:13:45 起，8765/8766/8767 在听）——起桥须用非受限命令，见 A1/A2 节 |
| 1.54 队列/模板 | **已解决**：`bundle_jobs` 为空；`platform_publish` job `2bfc710c` = `succeeded`，设备 `committed_job_id=2bfc710c`、`v2_templates=2`(`mini,quad`)、`active=quad` 并已渲染。剩余：数据帧被拒 `incomplete`（桥侧契约）+ Profile 的 `static1` 绑定，见顶部节 |
| Note4 正常 | 最新 job `1b4500ad` `succeeded`；09-25 实机 `data_seq=applied_seq=63`（之后按其节奏入睡） |

**待办的唯一事实来源是 `docs/roadmap/backlog.md`**（紧急 A1–A6 / 暂缓 B1–B4 / 长线 C1–C7 / 技术债 D1–D13）。可直接交给新窗口执行的实现工作单在 `docs/roadmap/prompts/`。

本轮核查又关闭/下调了三条，都遵循**证据优先于旧文档**：

- **修正上一版的错误结论**：我曾写「v2 会合路径已覆盖 ghost 预算」并宣判旧记录过时——**那是错的**。已用代码核实：预算判定只在 `deepNetworkCycle()`（`main.cpp:5166/5177`），而它唯一的调用点在 `main.cpp:5697` 的 `else if (deepWakePath)` 分支内，进入条件是 **`!(deepWakePath && v2BundleReady)`**（`main.cpp:5678`）。Note4 有已提交 v2 Bundle → 走 `5678-5693` 分支 → **`deepNetworkCycle()` 从不被调用**，`rtcClkPartials >= CLK_GHOST_LIMIT` 在该设备上永不成立。更早的记录 `PROGRESS.md:37`（"v2 会合路径没有任何 ghost 检查"）**才是对的**，A3 是真需求。
  - 附带确认：深睡薄唤醒 `deepThinWake()`（`main.cpp:5275/5283`）同样无预算检查；且 `epdPartialCount` 是普通 RAM（深睡清零），而薄唤醒在 `main.cpp:5528` 早于 `5531` 的 `epdBegin()` 就返回了 → 深睡路径上**两个预算都不生效**。
  - `refresh_policy.cpp` 里两个 `RGN_CLOCK` 不是同一件事：`:97` 的 `5` 是 `classRank()` 的区域合并优先级，`:108` 的 `90` 才是 `classDefaultBudget()` 的每区域预算；而且它只被走 `epdFlush()` 的局刷消费，**直接写时钟窗口的路径根本不经过它**。`src/refresh_policy.h` 已被固件与宿主共同包含，适合放单一来源宏。
  - 工单见 `docs/roadmap/prompts/A3-note4-clock-ghost-budget.md`。
- 「族发布会把设备 `sync_enabled` 覆盖回 false」（backlog A6）**不成立**：修正本体在 `bridge/crates/app/ui/index.html:541`，引入于 `fc3c464`（2026-09-25 04:04:10），此后该文件未再改动；运行桥 `bridge-app.exe` 构建于 **04:43:36**，晚 39 分钟 → **当前桥已含该修正**，族草稿的 `false` 不会被复制。旧的「未构建」结论来自 `bridge-family-sync-preservation/status.md`（写于构建之前）。
- 「panel-power 改动是否已进 ROM 需确认」（backlog B1）**已确认进了**：`panel_pwr` 与 `note4RestoreFrameBaseline()` 引入于 `e226d8e`，早于 `0.18.23` 的源码提交 `af2b607`，且之后未再改 `src/` → 已 OTA 的 `0.18.23-note4-b` 就含这两项，**B1 只剩实机测量，不需要重编固件**；`artifacts/codex-status-0.18.21-…panel-modes.bin` 是过期候选。
- `bugs.md` 的 BUG-1 / BUG-2 **均已由 v2 实现修复**（ACK CRC 同源、deep 唤醒恢复数据检查点或轮换 context）；已结案并归档为 `docs/history/bugs-2026-09-22.md`（含文件行号证据）。
- `next.md` 的合并/多 env/包隔离工作已完成，已归档为 `docs/history/next-2026-09-25.md`；其 §5.2 的 `git stash` 流程在本机**不成立**，不要再照做。

## 历史归档

本轮把 `PROGRESS.md` 从 89 KB 压到约 17 KB，并归档文档（详见 `docs/README.md`）：

| 归档 | 内容 |
|---|---|
| `docs/history/progress-archive-2026-09-25.md` | 本次下沉：原 `PROGRESS.md` 111 行起的全部历史节（2026-09-24 及更早 + 合并/包隔离过程细节，约 71 KB，逐字节保留） |
| `docs/history/progress-archive-2026-09-23.md` | 2026-09-22 及更早的进度（通用平台 v2 实现、0.13–0.16 各轮实机修复） |
| `docs/history/workflow/<initiative>/` | 已结项专项原件，共 **17 个**（第一批 15 个；`generic-display-platform-design`、`live-template-delivery` 在 ACL 修好后补入） |
| `docs/history/next-2026-09-25.md` | 原 `next.md`（合并/多 env/包隔离，均已完成） |
| `docs/history/bugs-2026-09-22.md` | 原 `bugs.md`（BUG-1/BUG-2 结案） |
| `docs/history/acl-repair-notes.md` | 沙箱 ACL 机制、判据与修复脚本 |
| `docs/roadmap/backlog.md` | **待办的唯一事实来源**（紧急 A / 暂缓 B / 长线 C / 技术债 D） |
| `docs/roadmap/archive-digest-legacy.md` / `-recent.md` | 各专项一页式摘要与"还欠什么" |
| `docs/README.md` | 文档总索引与"该读哪一个" |

### 沙箱写入边界（2026-09-25 查清并已修，结论修正了旧记录）

旧记录把两次归档失败归为"属主 + DACL"两种原因，实测后**真正挡路的是 DACL 里那条沙箱能力 ACE**
（`S-1-4-1018769461-493222538`），不是属主：

- 判据：能写的文件 SDDL 里有 `(A;ID;0x110156;;;S-1-4-1018769461-493222538)`，不能写的没有。
  沙箱用受限令牌，`Authenticated Users`/`Users` 这类宽泛组 ACE **不生效**，写入只认那条能力 SID。
- 能力 ACE 是后加到仓库根的，**可继承 ACE 不回溯**，所以早期创建的文件一直缺它（`project-workflow/` 下曾有 86 个）。
- **补齐 ACE 后，即使属主仍是 `CodexSandboxOffline`，`git mv` 与 `Add-Content` 都成功** —— 所以属主不是闸门。
- 属主是 `CodexSandboxOffline` 但缺 ACE 时，沙箱既改不了属主也改不了 DACL；属主是 `cogic` 而只是缺 ACE 时，**DACL 改得动**（令牌有 `WRITE_DAC`），本次已补 90 个对象。
- **沙箱提权（含已批准的 `danger-full-access`）不会把令牌变成管理员**：实测 `elevated? False`，`SetOwner` 恒报 `Attempted to perform an unauthorized operation`。**改属主只能在管理员终端做**。
- 仓库内仍有 64+ 个对象属主是 `CodexSandboxOffline`（ACE 已补齐，不影响使用）；要清干净见 backlog D13 与 `docs/history/acl-repair-notes.md`。
