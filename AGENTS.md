# AGENTS.md — Codex Status

## 项目概览

便携墨水屏显示本机 Codex 余量：ESP32-S3（Waveshare 1.54" 200×200 B/W，SSD1681，局刷 ~300ms）本地渲染；Rust 桥接（Tauri v2 托盘进程）通过 `codex app-server` JSON-RPC 取数，经 Wi-Fi HTTP（主通道）或 BLE GATT（备选）下发 usage 与模板。换样式只换模板 JSON，不刷固件。现行通用平台：每设备 Profile 1–8 全按键循环、CompiledTemplate、完整 A/B Bundle、单一 active context、Bridge 生成 PowerPlan、Codex 只是 DataSource；关 mDNS；设备身份 = Wi-Fi MAC + 可编辑显示名；显式 claim/lease 占用，桥按空闲自动占用。设计见 `docs/generic-display-platform-design.md`。旧版 ≤3 槽通道与桥侧 legacy profile 存储已删除（2026-09-25）；不得把 8 项静默裁剪成 3 项。工作树中的 `/api/*` 一次性协议改名尚未部署，实机/运行 Bridge 版本以 `PROGRESS.md` 最新节为准。

先读 `PROGRESS.md` 最新一节（权威交接文档，只保留最新现场，含设备现场、ROM SHA256）；待办的**唯一事实来源**是 `docs/roadmap/backlog.md`。文档导航见 `docs/README.md`。历史背景见 `docs/history/request.md`、`docs/device-setup-experience.md`；早期讨论见 `docs/history/discussion-summary.md`（历史资料）；已结项专项见 `docs/roadmap/archive-digest-legacy.md` / `-recent.md`，原件在 `docs/history/workflow/`。

## 目录导航

| 路径 | 内容 |
|---|---|
| `src/` | 固件（PlatformIO/Arduino）。入口 `main.cpp`；`template_engine/template_xfer/template_store`（模板）、`ble_bridge`、`EPD_SSD1681`、`GUI_Paint`、`dev_log`、`bridge_store`、`owner_store`（claim/lease） |
| `bridge/crates/core` | app-server 客户端、usage 信封、模板库（canonical JSON + CRC32）、设备 `/api/*` HTTP 客户端 |
| `bridge/crates/ble` | btleplug central：endpoint 身份、认证命令与会合 |
| `bridge/crates/render` | 把固件同一份 C++ 引擎编进宿主，像素级预览/离线对拍 |
| `bridge/crates/mcp` | MCP 工具：bridge_status/template_render/firmware_ota/pm_stats/device_* 及 platform_*（模板、Profile、数据源、PowerPlan、发布、状态与恢复）；由托盘内建 HTTP 端点 `http://127.0.0.1:8766/mcp` 提供（见 `opencode.jsonc`；平台工具由 app 侧实现，与 UI 共用 application service） |
| `bridge/crates/app` | 生产形态：单实例托盘 + 内建 HTTP/BLE/MCP，运行数据在 `<exe>/data/`；`src/discovery.rs` 为 ARP 发现回退；身份/发现/占用见 `docs/power-state.md` §9.1/§9.2 |
| `tools/test-bridge` | Python 测试桥（`start.ps1`/`stop.ps1`）、模板库 `templates/*.json` |
| `tools/device-auth` | `request_token.py`：经已绑定 BLE 链路协商 Wi-Fi 操作 token |
| `tools/*.mjs` | Node 预览生成/场景测试（`generate-quad-preview.mjs`、`test-quad-preview.mjs`） |
| `project-workflow/` | **进行中**专项的计划/task/评审记录。已结项的归档在 `docs/history/workflow/`，摘要见 `docs/roadmap/archive-digest-*.md` |
| `docs/roadmap/` | `backlog.md`（待办唯一来源）、`archive-digest-legacy.md`、`archive-digest-recent.md` |
| `docs/history/` | 只读历史：进度归档、旧设计、已归档专项原件。**不要通读** |
| `artifacts/` | 本地证据（ROM、日志、截图、预览），已 gitignore，不入库 |

## 常用命令

固件（仓库根目录）：

```powershell
pio run -e zectrix-note4-b            # 默认构建目标；勿省略 -e
pwsh tools/pio-target.ps1 -Target 154g # 仅本次已授权的 1.54 标准 B/W ROM；见下方例外
pio run -e zectrix-note4-b -t upload  # USB 烧录（用网页 OTA 后先擦 otadata：erase_region 0xD000 0x2000）
pio device monitor
```

### 固件目标：默认只构建 Note4；本次双 ROM 实机测试例外

- **默认唯一目标：`zectrix-note4-b`。** 2026-09-26 用户明确授权依次发布 Note4 与 1.54 两份 ROM 并实机测试；本次允许为 1.54 构建且仅构建 `esp32-s3-epaper-154g`，使用 `tools/pio-target.ps1 -Target 154g` 的隔离包目录与完整 sdkconfig 快照。两个目标不得并行构建；不得构建 `-gray4`/`-btpm` 等变体。构建前核对目标，发布前核对 ROM marker、大小、SHA256、登记 MAC 与设备自报身份。该例外不代表以后默认可以构建其他 env。
- **目标切换只通过隔离脚本。** 项目根 `sdkconfig.defaults` 是**全项目唯一**生成物，被各目标争写；pioarduino `arduino.py` 会把当前环境的 `custom_sdkconfig`、MCU 与板卡指纹同该文件首行的 `# TASMOTA__...` 比对，`check_reinstall_frwrk()` 不匹配时会清理生成的 sdkconfig 文件并重装两个 Arduino framework 包。**没有**每目标 sdkconfig 快照时，交替切换必然失配、必然重装（已双向实测，单次约 15 分钟量级；`tools/pio-target.ps1` 现已带该快照，见下方 ③）。多目标并存的正确解法是按目标隔离 `packages_dir`（`tools/pio-target.ps1` 支持 `-Target note4|154g`，按目标生成 `.pio-pkgs/<target>` + 仓库内 `.pio-core`；**两侧都已在 2026-09-25 实测**，见 `docs/history/progress-archive-2026-10-03.md`「多 env 实测」与 backlog C6）。⚠️ 三点实测结论：① 只做包目录隔离**不足以**免重装：判据在仓库根那个共享文件上，切回另一目标会命中 `*** Reinstall Arduino framework ***`（隔离只保证重装发生在目标自己的包目录里、不伤对方）；② 受限沙箱下该脚本曾因 `pio` 继承到错误 cwd（`D:\Documents\project`）而假失败——现已由脚本自身 `Set-Location $RepoRoot` + `pio run -d $RepoRoot` 修掉（历史记录见 `docs/history/progress-archive-2026-10-03.md`）；默认只构建 Note4 是成本约束，本次例外由用户授权。③ 脚本另按目标**整份快照 `sdkconfig.defaults`**（`.pio-core/sdkconfig.defaults.<target>.snapshot`，构建前还原/构建后再存）来消除交替触发的 `*** Reinstall ***`：还原的是该目标自己生成过的完整文件（内容+指纹自洽），**不是** `next.md` §10 禁止的"伪造首行"；安全判据是重建 ROM 与记录值逐字节相同（154g `91937B18…`）。实测 priming 后 note4 39 s / 154g 42 s 交替**均无重装**。priming 那一次仍需重装，且该路径要工作区外写权限（沙箱内需提权）。
- 不要同时运行两个及以上 `pio` 进程；单个目标内部允许 PlatformIO 并行编译源文件。
- **重编范围（2026-09-25 实测）**：改 1 个 `.cpp` → 只重编该 TU + 重链接（note4 46 s）；改被广泛 include 的头（如 `src/template_engine.h`）→ 10 个 TU（41 s）；改 `custom_sdkconfig`/`memory_type`/`flash_size` → `*** Reinstall ***` + 重编 IDF 库（"整个项目重编"那一类）；改 `platformio.ini`（哪怕注释）→ `project.checksum` 失配 → 删 `.pio/build/<env>` 全量重编应用（不连带 framework 重装）；删 `.pio/build`/包目录/`-t clean` → 全量。⚠️ ESP-IDF 把**编译日期时间**写进镜像，所以**任何重编都会改变 ROM 哈希**——要复现记录里的哈希必须"零编译重建"（改完源码记得把 `PROGRESS.md` 的 ROM 行更新成新哈希，别让记录与产物对不上）。
- 保留 `.pio/build/<env>` 与 SCons 缓存，不要因只改应用源码而例行清理或删除构建目录。
- 每次构建后核对环境名、固件版本、产物大小与 SHA256，并把最终 ROM 路径与哈希写入 `PROGRESS.md`。

pm env（M3，pioarduino `custom_sdkconfig`）构建：仓库改名后路径无空格，直接在仓库目录运行 `pio run -e zectrix-note4-b` 即可（若路径再含空格，IDF 会拒绝，需用 junction 且让 pio 的真实 cwd 落在 junction 上）。勿用 `-v`（GBK 控制台会 UnicodeEncodeError 挂住构建）。生成物 sdkconfig*/CMakeLists.txt/.dummy 等已 gitignore。构建环境细节与踩坑见 `docs/history/workflow/sleep-battery/task-7.md` §7。⚠️ 下列这条只属于 `esp32-s3-epaper-154g`（1.54" 板；当前不构建，仅保留备查）：闪存必须 40 MHz（`board_build.f_flash` + `tools/bootloader_40m_fix.py`），否则 GD25Q64 在 80 MHz 下 ID 读错、写入失败。

Rust 桥（`bridge/` 目录；Windows 为已部署基线，macOS 正在实施/验收）：

```powershell
cargo test
cargo run -p bridge-core -- --once          # 拉一次真实数据并打印信封
cargo run -p bridge-core                    # 独立数据源进程，HTTP :8765 只提供 /health
cargo run -p bridge-ble -- --once           # 单次 BLE endpoint 交接；业务数据走当前设备协议
cargo run -p bridge-app                     # 托盘应用（生产形态，进程内含 HTTP+BLE+MCP）
cargo run -p bridge-render -- --template <json> --out <png>   # 离线渲染对拍
```

环境变量：`CODEX_STATUS_PORT/TOKEN/TEMPLATES/INTERVAL`、`CODEX_STATUS_CODEX`、`CODEX_STATUS_DATA`、`CODEX_STATUS_DEVICE_IP/MAC/NAME`、`CODEX_STATUS_BRIDGE_NAME`。

工具与校验：

```powershell
pwsh tools/start-bridge.ps1   # 默认实例：按需 Windows 计划任务 CodexStatusBridge 启动；立即返回，重复调用只打印 PID
pwsh tools/start-bridge.ps1 -Instance note4 -Port 8775 -McpPort 8776 -IconShape circle -DeviceMac 7C4FADB93408 -DeviceIp 192.168.3.177  # 独立实例；需先构建同一 exe
Get-ScheduledTask -TaskName CodexStatusBridge  # 查看默认实例任务状态（无登录自启动触发器）
node tools/generate-quad-preview.mjs
node tools/test-quad-preview.mjs
pwsh tools/test-bridge/start.ps1   # 后台启动 Python 测试桥；stop.ps1 停止
git diff --check                   # 提交前必查
```

## 关键不变量

- **任何请求都必须有明确返回，包括失败的请求（用户2026-10-05要求）。** HTTP/BLE/MCP/UI共用此合同：成功返回真实结果，失败返回可识别的结构化错误、失败阶段及可操作反馈；不得静默丢弃请求或把确定性拒绝、发送前失败、设备缓存/修订冲突留在“待确认”无限重试。未收到响应必须在有限超时后明确报告超时；已实际发出且无法确定是否生效时，明确返回“结果未知”和后续处理入口，查询原事务，不冒充成功或自动换ID重发。缓存过期必须提示重新读取设备并保留Bridge编辑；覆盖无效参数、认证/占用拒绝、过期缓存、预算耗尽、设备拒绝、丢响应等完整调用链，Fake ROM及UI测试不得只测成功路径。

- 三方模板一致：固件引擎、Rust canonical JSON、Python 测试桥的模板哈希必须相同；`type/font/bind` 未识别即整份拒绝（dry-run 校验，不半渲染）。改模板协议必须同时改固件、`bridge/crates/core/template.rs` 与测试。
- 渲染一致：`crates/render` 与固件共用同一份 C++ 引擎，预览须逐像素一致；缺省 battery=75。
- 所有绘制文本先做 ASCII 净化（防字库越界）。
- `/update`、`/doUpdate`、ArduinoOTA、`POST /claim` 必须携带设备 token（开机自动签发并存于 NVS，仅经绑定 BLE 链路取用/轮换）；无/错 token 返回 401，不要绕过或放宽。
- 设备身份 = Wi-Fi MAC（学习并持久化），显示名可改但非主键；UDP/HTTP/BLE 通告 MAC 不符即拒绝。发现链：属性 IP → UDP → ARP（Windows）→ BLE（手动兜底）；mDNS 已关，勿再依赖 `.local`。
- 占用只走显式 claim 请求（HTTP `POST /claim` 与 `ble_claim=1` 的 BLE `claim` 共用处理器；BLE另需绑定加密、当前principal/会话/MAC和设备操作token，普通BLE入口不支持force/release）：`/api/data`、`/api/plan`、`/api/bundle/*`、`/api/activate` 永不创建/转移 owner（仅刷新匹配 id 的 `last_seen`）；owner 有效且命令 `bridge_id` 不符一律 409，`activate` 无旁路；lease 到期只清空。桥空闲自动 claim、60s 续约、他人占用不推送；模板发布仍是用户显式动作（claim 是协议行为，不等于发布）。
- 改 GATT 特征表后 Windows 会缓存旧属性，需解除配对再重配（或后续评估 Service Changed）。
- 便携数据布局：Windows 运行数据 `<exe>/data/`，种子 `<exe>/seed/`（开发回退 `tools/test-bridge/`）；macOS 数据在 `~/Library/Application Support/dev.codexstatus.bridge/data`，命名实例在同根 `instances/<name>/data`，安装身份在同根 `installation-id`，打包种子在 `.app/Contents/Resources/seed/templates`，显式 `CODEX_STATUS_DATA` 优先。程序不向 `.app` 内写数据。⚠️ **`<exe>/data/` 就在 `bridge/target/debug/data/` 里**——`cargo` 报 `os error 5`（`target/debug/.fingerprint/...` 这类沙箱旧文件缺能力 ACE，`Set-Acl` 也改不动）而想"删 `target/debug` 重建"时，**先把 `data/` 备份出去**：它装着 `state.json`（设备登记/Profile/jobs/contexts/plans）、模板库、`device-token-*.json`、`bridge-app.json`（endpoint token）。2026-09-25 有一次真实事故（见 `docs/history/progress-archive-2026-10-03.md`「桥：契约刷新修复 + ACL 闸门绕过」节）：删目录连带删掉运行时数据，靠写回配置 + 从 `tools/test-bridge/` 与仓库夹具重导模板 + 重新登记设备/重建 Profile 才恢复。**更安全的做法是 `cargo build/test --target-dir <仓库内新目录>`**，完全不碰现有 `target/debug`。
- 推送是用户显式动作：Profile = 1–8 个有序模板（全部参与按键循环，无 enabled 子集），显式发布冻结一个完整 Bundle；保存模板/Profile、MCP save、UI save 都只落盘，不得自动发布。旧版 ≤3 槽通道与 `profiles.json` 存储已删除（2026-09-25），桥只服务现行平台设备。
- 数据语义：字段仅在 Bridge 绑定合同中分 push/pull；push 可见值/缺失/质量变化发送完整最新快照，pull-only 变化只更新缓存、不推送、不改 PowerPlan；只有成功 ACK 才更新确认指纹与 full_sync_deadline。设备不接收 push/pull 分类。
- 运行字段需求以设备认证 `/api/status` 的 `data_contract` 为准，不从 Bridge 本地模板库或 Profile 推导。BLE 同一 context/active 可复用该设备缓存；切换或未知合同先经有界 Wi-Fi 读取。设备级数据投递许可独立于本地模板/Profile，未报告合同不发空字段包。
- 电源：只有 Bridge 的正式 PowerPlan 改变 light deadline；读取/传输/claim/owner renew 都不隐式续租；BOOT provisional 300s 从物理唤醒起算，timer wake 不获得。
- 版本号 `FW_VERSION` 在 `src/main.cpp`；固件发布后在 `PROGRESS.md` 记录 ROM 路径与 SHA256。 Note4版本只使用 `版本号-note4`，不叠加a/b或功能后缀（用户2026-10-04要求）；构建env与硬件target标识不因此改名。
- 显示规则：配额/账号缺失的表现是**模板属性**，由各变体自己的 `when` 分支决定，不用全局规则统一。已实现的两种：200×200 `quad` 在 5h 桶不存在时显示静态 `100` 并隐藏其重置时间；400×300 `codex-status-a` 在 5h 桶不存在时隐藏 5h 块并把 weekly 提升到主位。共享的数据侧约定：桥在 `resetCredits.availableCount<=0` 时不下发该字段，模板按 `exists` 隐藏 RC 行；`bridge.label` 取不到用户名时模板不绘制该行。新增变体按该 target 的产品意图选择分支，并同步更新本行。

## 嵌入式数据结构与存储设计指导

- **按实际使用量付费。** 嵌入式 RAM、栈与 NVS 必须分别预算；容量上限用于校验，不等于预分配全部空槽。Wi-Fi 方案仓库沿用模板仓库的原则：变长记录、使用多少存多少；不得将大量空槽、定长字符串、重复凭据或回执作为整份原生 struct 长期保存。压缩不能代替合理的数据结构，也不能作为 RAM 浪费的豁免。
- **持久化格式带显式头。** 至少包含格式标识/版本、总长度或有效载荷长度、实际记录数量及完整性校验；字段与记录使用显式长度，读取先检查长度、数量、上限和校验，再按需分配。不得依赖编译器 struct 布局、padding 或 sizeof(struct) 作为存储协议；损坏、截断、超长及未知版本必须明拒，不静默裁剪。
- **RAM 同样按需使用。** 优先按实际条目分配或逐记录读取，明确记录所有权与生命周期；固定数组仅用于有证据支持的硬件/实时性需求或有界工作缓冲，不为方便而预留整个仓库。回执和版本元数据保留协议正确性所需的最小集合，删除冗余前验证重试、幂等和未确认提交的语义。
- **避免完整仓库副本压栈。** 不在小任务栈上创建大型局部对象、聚合赋值临时对象或嵌套全量副本；采用原地初始化、按需堆工作区或流式处理，并处理分配失败。涉及启动/恢复/存储链路时检查目标 ROM 的真实栈帧和调用链预算，不能以宿主测试通过替代嵌入式栈验收；Wi-Fi 启动读取须通过 `python3 tools/check-wifi-stack.py <matching.elf>`。
- **变更必须有资源证据。** 给出空仓库、单条、典型使用量和容量上限的持久化字节数、峰值 RAM 与栈占用，解释每个主要成本；保持原子提交、读回校验和掉电恢复。旧键仅在新格式验证成功后退役，兼容代码的移除须单列验收；凭据不得落日志或仓库。

## 工作流约定

- 在仓库内创建新目录必须对创建步骤使用 `require_escalated` 提权执行，避免目录属主变成 `CodexSandboxOffline`、继承到不完整的 ACL。包括 `mkdir`/`New-Item`、补丁工具隐式建目录，以及构建或脚本首次生成目录；对自动生成的目录，先提权预建，无法预建时提权运行创建它的命令。不要先在沙箱中创建再修属主；若提权被拒绝，停止该创建步骤并说明原因。已存在的目录无需重复创建。
- macOS Wi-Fi UI 改动须运行 `node tools/test-device-page.mjs` 与 `bash tools/test-wifi-ui.sh`，并审阅窄/宽窗口截图；后者加载生产 HTML 的原生 WKWebView，使用合成 IPC，不能作为真实 Rust/设备通信或改网证据。测试说明见 `project-workflow/bridge-macos/wifi-tabs-plan.md`。
- 每个里程碑后更新 `PROGRESS.md`（现场、证据、待办）；多步工作用 `project-workflow/<initiative>/` 写 plan/task/status/review，先计划再动代码。
- 提交信息用英文祈使句，沿用现有风格（如 `Firmware 0.8.0: status JSON, log ring, battery; bridge prefers JSON status`）。未经用户要求不要提交。
- **默认 Bridge 启动**：从非受限（提权）命令运行 `pwsh -File <仓库绝对路径>\tools\start-bridge.ps1`。标准默认实例（8765/8766、方形图标、无设备参数）会注册/复用当前交互用户的**按需 Windows 计划任务 `CodexStatusBridge`**，由任务计划程序启动 `<repo>/bridge/target/debug/bridge-app.exe`；任务无登录触发器、无运行时限，重复启动只报告现有 PID。2026-09-27 实测：旧 `UseShellExecute=true`/`cmd.exe` 虽分离 stdio，退出 Codex 时 Bridge 与 watchdog 仍一起退出；改由计划任务启动后主进程父级是任务计划程序的 `svchost.exe`，8765/8766 正常监听，**关闭 Codex 后持续运行仍待跨会话复核**（见 backlog）。受限沙箱的 `Get-NetTCPConnection` 看不到监听者，启动会假失败；不得从受限工具会话直接起桥。桥日志在 `<exe>/data/logs/bridge-app.log.<UTC 日期>`，计划任务不依赖 `artifacts/bridge-app-run.out/.err`。带自定义端口、设备参数或命名实例仍走脚本原有的分离启动路径，未验证其退出 Codex 后的存活性。其他后台服务仍须立即返回、不继承工具会话的 stdio；不在前台跑长轮询。
- **停止或重建默认 Bridge**：只操作当前 `bridge/target/debug` 对应的进程，不擅自停止别处运行的实例。先结束同 exe 的 `--watchdog <主 PID>`，再 `Stop-ScheduledTask -TaskName CodexStatusBridge`；若任务已是 `Ready` 而端口仍被 watchdog 拉起的主进程监听，再按监听 PID 停该主进程。重建 exe 前确保两者都退出，保留 `<exe>/data/`，构建成功后用上面的脚本按需重新启动。Bridge UI 嵌入 exe：修改 `bridge/crates/app/ui/index.html` 后须在**包含该 UI 改动的工作区**重建并重启；2026-09-26 曾从干净 worktree 构建，导致旧界面覆盖新版。停止/重启前核对路径、端口及任务状态。
- **旧的 watchdog 纯重启路径**：`pwsh tools/restart-bridge.ps1` 只强杀主进程，依赖 watchdog 拉回；2026-09-25 曾在旧分离启动方式下验证，但**改为计划任务后尚未验证任务状态与新进程归属，不要用它代替上面的计划任务启动/重建流程**。watchdog 仅在非零退出码时重启，且 5 分钟内 3 次异常退出就放弃（见 `<exe>/data/logs/watchdog.log`）。构建时运行中的 exe 被锁住，必须先按上述顺序停进程。需要反复重建/启动时，非受限权限可通过本会话权限预设管理；不要为了绕过审批而在受限沙箱中启动后台桥。
- 命名 Bridge 实例必须指定不同 HTTP/MCP 端口；各自运行数据在 `<exe>/instances/<name>/data`，默认实例保留 `<exe>/data`。命名实例不监听设备固定 UDP 8767，只走精确 MAC 的 HTTP/BLE；不同实例 owner ID 不同，同一设备仍由 claim/lease 决定占用。托盘背景形状可用 `-IconShape square|circle|diamond` 区分。
- 不提交任何密钥：Wi-Fi 密码按用户2026-10-04修订，允许作为完整配置保存于Bridge本机运行data及设备RAM/NVS，首次认证读取后复用缓存、编辑框用眼睛按钮显隐；按用户2026-10-06修订，有效 session 与 owner 可统一读取、修改、删除和启用全部设备Wi-Fi配置，创建者绑定仅作来源记录；配置ID冲突明确拒绝，事务回执及重放围栏仍按principal/namespace隔离；不落仓库、不进日志。BLE token沿用既有受控存储/传输，不进入仓库或日志。

## 硬件现场与坑

**设备以桥的登记记录为准**（`<exe>/data/platform/state.json` 的 `devices[].identity`），启动时不要照抄本文的旧地址：

| 设备 | MAC | 登记 IP | 备注 |
|---|---|---|---|
| 1.54" 200×200 | `70041DD7A340` | `192.168.3.163` | 显示名"书桌屏"；BLE 名形如 `CodexStatus-<MAC后缀>` |
| Note4 400×300 | `7C4FADB93408` | `192.168.3.177` | 显示名"Note4"；曾用 IP `192.168.3.177`/Wi-Fi `wd21-la` |

- ✅ 已核实（2026-09-25）：设备 `/status.json` + ARP 实测 1.54 就是 `70041DD7A340` @ `192.168.3.163`，与本表及 `state.json` 登记一致。本文旧版写的 `192.168.1.50` 与 `70:04:1D:AA:BB:CC` **作废**，不要再引用。1.54 于 2026-09-25 OTA 到 `0.18.23-bw`；2026-09-26 经按 MAC 排队 OTA 后认证状态已见 **`0.18.25-bw`**。Note4 同法已见 **`0.18.25-note4-b`**；精确运行镜像哈希仍未由设备证明，见 `PROGRESS.md` 顶节。
- USB 串口地址动态（Windows COM3/COM4/COM5，macOS 当前常见 `/dev/cu.usbmodem14101`），访问前枚举 VID/PID/serial 并核对登记 MAC。Note4 使用 ESP32-S3 原生 USB-Serial-JTAG（`ARDUINO_USB_MODE=1`），不是普通外置 UART 转接器。
- **串口复位须按证据区分，不得以“可能复位/崩溃”为由跳过用户授权的串口诊断。** 2026-10-04 01:21 的 pyserial 短时读取在预设 `DTR=false/RTS=false` 后仍捕获 ROM `rst:0x15 (USB_UART_CHIP_RESET)`；证据 `artifacts/bridge-macos/restart-event-usb-{capture.json,serial.raw.log}`。这证明该主机/设备/打开流程曾触发 USB 硬件复位，不能泛化成所有串口打开必复位，也不能把它当作固件 panic。2026-10-04 20:36–20:38 在同一Note4 `.59`/macOS上完成对照：原始 `os.open/read/close`（不写modem ioctl）及跳过 `_update_dtr_state/_update_rts_state` 的pyserial各监听15s，boot_id不变；普通pyserial预设false/false后打开立即出现同一ROM复位。再分步对照：在跳过modem设置的连接上单独执行 DTR=false 即触发USB复位，随后RTS=false不触发第二次复位。结论限该设备/主机及当时起始控制线状态，不能推断RTS在所有状态均无影响；pyserial POSIX open 的DTR/RTS依次设置与最终值须分别考虑。
- **保现场串口监听已实测可用**：优先原始port读取（不发送DTR/RTS ioctl）；如用pyserial，须在open前用专用监听子类跳过 `_update_dtr_state`、`_update_rts_state`，并避免open丢弃接收缓冲；不要误以为预设false/false就安全。对照中pyserial打开后清除termios `HUPCL`，关闭前后boot_id仍不变；所有变体先核对MAC，并保存boot身份/复位文本。不要把这次15s样本当跨平台长期保证。复位报告与监听recipe见 `project-workflow/bridge-macos/note4-panic-20261004.md`；完整串口原文/boot_id对照在私有备份 `~/Library/Application Support/dev.codexstatus.bridge/backups/note4-panic-20261004-2027/`。
- **Windows例外（2026-10-07 .69核验）**：跳过上述pyserial `_update_*` 方法并不足以保现场，`serialwin32._reconfigure_port`仍经`SetCommState`配置DCB的DTR/RTS。本轮监听重开期间认证出现USB reset_reason11及boot改变，不能误判为固件panic，也不能将Mac监听子类保证推广到Windows。Windows应使用不改DCB控制线的原生读取，并以打开/关闭前后认证boot_id、uptime及ROM文本验证；未验证前优先网络连续采样。原.68 CRC有效dump的panic另有独立实证。
- esptool 默认/`usb-reset` 进入 ROM 下载模式以及结束后的 `hard-reset` 是**工具主动复位**，与被动监听分开记账；`--before no-reset` 不会让正在运行的应用自动具备 flash 读取能力。打开/关闭串口前后记录时间、工具/参数、控制线策略、boot_id/uptime/reset_reason，并用 ROM reset 文本核验；公共 `reset=unknown` 不能替代 ROM 原因。固件 `panic` / 认证 `reset_reason=4`、WDT、brownout、深睡唤醒和工具 USB 复位分别判断。
- 用户授权后可通过串口只读导出 dump：先保存可获取的网络现场，核对实际分区表及 MAC；当前 Note4 coredump 为 `0xB00000` 起、`0x100000` 字节。先私有保存原始 dump、实际分区表、匹配 ELF 与 SHA256，再校验完整性并解码；dump 可能含凭据，不提交、不完整打印。导出不得顺带擦 dump、写 ROM/分区/NVS；工具进入下载模式及恢复应用的复位单列记录。现有配置允许新 panic 覆盖旧 dump，应优先保全。
- OTA：上传成功后延迟 1.5s 重启，HTTP 先返回 `UPDATE OK`（客户端超时属既有现象）；双槽 ota_0/ota_1 轮换。
- 状态可读：`GET /status.json`（恢复/现场视图）、认证 `GET /api/status`（业务状态）、`GET /log`（统一诊断流视图）与 `GET /pmstats`（PM light-sleep 统计/锁，0.13.0+，只读免 token）。Bridge 注册须核对结构化状态和认证状态的 MAC；旧 HTML 状态页回退与 `claim_unsupported`（固件 <0.13.4）已删除。
- `cargo test --workspace` 可能因运行中的 `bridge-core.exe` 锁定 `target/debug` 失败（不是逻辑失败）；改用隔离 `CARGO_TARGET_DIR` 复测，或核实进程后由用户决定是否停桥。
- Windows 控制台为 GBK：Python 桥启动时设 `PYTHONIOENCODING=utf-8`，避免 status notify 打印异常。
- BLE 写入须按 MTU 分片（usage/模板），单次超 MTU 会 `Invalid Attribute Value Length`。
- 更多历史踩坑（1.54 闪存 40 MHz、Note4 40 MHz bootloader 修 NVS/LittleFS、PSRAM、沙箱 ACL/`git stash` 事故）见 `docs/history/progress-archive-2026-09-25.md` 与 `docs/history/next-2026-09-25.md`，不要凭记忆重做。
