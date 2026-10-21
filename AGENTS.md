# AGENTS.md — Codex Status

## 项目与阅读入口

ESP32-S3 本地渲染墨水屏；Rust/Tauri Bridge 经 `codex app-server` 取数，Wi-Fi HTTP 为主、BLE 为备选。平台使用 CompiledTemplate、单份完整 Bundle（允许临时下载包，覆盖时进入内置界面）、单一 active context、Bridge 生成的 PowerPlan；原生固件 OTA 双槽保留，Codex 只是 DataSource。现场是否已升级以 PROGRESS.md 为准。

- 接手先读 `PROGRESS.md` 当前交接卡；待办唯一来源是 `docs/roadmap/backlog.md`，只查相关条目。导航见 `docs/README.md`，不要通读历史或全部专项。
- 构建/烧录前读 `docs/agent-operations.md`「构建与工具」；启停/重建 Bridge 前读「Bridge 启停与实例」；运行数据、模板缺省、串口/dump 操作各读其对应节。该文档保留原 AGENTS 操作细则，不需每轮全读。
- 权威设计：`docs/generic-display-platform-design.md`、`docs/power-state.md`。当前版本/部署以交接卡为准，历史“未部署”描述不能代替现场。

目录：`src/` 固件（`main.cpp` 入口）；`bridge/crates/core|ble|render|mcp|app` 分别负责数据/设备客户端、BLE、同源预览、MCP、托盘应用。MCP 在 `127.0.0.1:8766/mcp`，平台工具与 UI 共用 app application service；`tools/` 为工具与测试桥；`project-workflow/` 为未结专项，结项在 `docs/history/workflow/`；`artifacts/` 为不入库的本地证据。

AP01 源码边界：本仓库 `ports/ap01/` 只拥有 Bridge 配对/通信 App、Bundle 与模板显示；OTA、切槽、维护鉴权/HTTP 策略、原厂系统菜单及 Wi-Fi 切网/恢复由兄弟仓库 `cuktech-ap01-firmware/tooling/firmware-modules/` 拥有。App 只提供配对、绑定 principal、健康回调并读取系统忙门；本仓库可保留构建/集成测试入口，不复制维护实现或 ROM 补丁。改边界运行 `python tools/test-ap01-firmware-boundary.py`；现存 Note4/1.54 源码职责不受此 AP01 约定影响。

## 交接与工作流

- `PROGRESS.md` 目标约 2 KB：更新时间、部署/工作区、活跃事项与下一步、风险、验收边界、backlog 编号和证据链接；无证据写未知。每个里程碑更新卡片，详细进度按日期归档；日志/私有原文留在 `artifacts/`。不重复待办/设计，不追加过程流水账；单次调查写对应专项或 backlog。
- 多步实现先在 `project-workflow/<initiative>/` 写 plan/task/status/review。未经要求不提交；提交用英文祈使句，提交前 `git diff --check`。
- 新目录（含构建/脚本自动创建）必须 `require_escalated` 提权预建，无法预建时提权运行创建命令；不先在沙箱创建再修 ACL。拒绝后停止创建并说明，已有目录无需重建。
- macOS Wi-Fi UI 改动须运行 `node tools/test-device-page.mjs`、`bash tools/test-wifi-ui.sh` 并审阅窄/宽截图；原生 WKWebView 的合成 IPC 不证明真实 Rust/设备通信。说明见 `project-workflow/bridge-macos/wifi-tabs-plan.md`。

## 构建与运行保护

- 默认唯一固件目标 `zectrix-note4-b`，`pio run` 必须指定 `-e`。既有双 ROM 授权限原任务的 `esp32-s3-epaper-154g`，不成为以后默认；不构建 `-gray4`/`-btpm`。切目标只走 `tools/pio-target.ps1 -Target note4|154g`；同时最多一个 pio 进程。
- 隔离脚本同时隔离包目录并还原目标自己生成的完整 sdkconfig 快照；不得伪造指纹首行。保留 `.pio/build`/SCons 缓存，不例行 clean。不用 `pio -v`；路径含空格时按操作细则处理。
- 每次构建核对 env、版本、大小、SHA256；最终 ROM 路径/哈希写交接卡，候选与已部署明确区分。发布前再核对 marker、登记 MAC 与设备身份。重编会改变镜像哈希。`FW_VERSION` 在 `src/main.cpp`；Note4 版本只用 `版本号-note4`，不加 a/b 或功能后缀，不改 env/target 标识。
- Rust 在 `bridge/` 运行。锁定/ACL 构建失败优先隔离 `CARGO_TARGET_DIR`；`bridge/target/debug/data/` 有真实运行数据，删除 target 前必须备份 data，不能为测试擅停其他实例。
- 默认 Bridge 从提权命令运行 `pwsh -File <repo绝对路径>\tools\start-bridge.ps1`，使用按需计划任务 `CodexStatusBridge`；不得从受限工具会话起后台桥。停止前核对路径/端口/任务；先停同 exe watchdog，再停任务，必要时停核实的残留监听 PID。重建前两者退出，保留 data，成功后脚本启动；不用 `restart-bridge.ps1` 代替。UI 嵌入 exe，改 `ui/index.html` 后从包含改动的工作区重建/重启。其他后台服务立即返回且不继承工具 stdio。

## 协议与产品不变量

- HTTP/BLE/MCP/UI 每个请求都明确返回真实成功或结构化错误、失败阶段和可操作反馈；有限超时。确定拒绝、发送前失败、缓存/修订冲突、预算耗尽不得无限待确认；已发但无法确定生效时返回“结果未知”并查询原事务，不冒充成功/换 ID 重发。缓存过期提示重新读设备且保留 Bridge 编辑；Fake/UI 回归覆盖错误完整链。
- 设备主键是学习并持久化的 Wi-Fi MAC，名称可编辑；UDP/HTTP/BLE MAC 不符即拒绝。发现：已知 IP → UDP → Windows ARP → 手动 BLE；不依赖 mDNS/`.local`。
- `/update`、`/doUpdate`、ArduinoOTA、`POST /claim` 要设备 token，无/错 token 返回 401；token 开机签发存 NVS，仅经绑定 BLE 链路取得/轮换，不放宽鉴权。
- owner 只由显式 claim 创建/转移：HTTP 与 `ble_claim=1` BLE 共用处理器，BLE 要绑定加密、当前 principal/session/MAC/token，不支持普通入口 force/release。data/plan/bundle/activate 仅刷新匹配 owner 的 last_seen；有效 owner 与 bridge_id 不符返回 409，activate 无旁路，lease 到期只清空。桥空闲自动 claim、60s 续约，他人占用不推送。
- Profile 为 1–8 个有序模板，全部按键循环，无 enabled 子集；不裁成 3 项、不恢复 legacy profiles.json。模板/Profile/MCP/UI 保存只落盘；用户显式发布冻结完整 Bundle，claim 不等于发布。
- push 可见值/缺失/质量变化发完整最新快照；pull-only 只改缓存，不推送、不改 PowerPlan。分类只在 Bridge，设备不接收；成功 ACK 才改确认指纹/full_sync_deadline。
- 需求以设备认证 `/api/status` 的 data_contract 为准，不从本地模板/Profile 推导；设备级投递许可独立于本地库。BLE 同 context/active 可复用缓存，切换/未知合同先有界 Wi-Fi 读取，未知合同不发空字段包。
- 只有正式 PowerPlan 改 light deadline；读/传输/claim/续约不延长。BOOT provisional 300s 从物理唤醒算，timer wake 不获得。
- 固件/Rust canonical JSON/Python 模板哈希一致；未知 type/font/bind dry-run 整份拒绝。改协议同时改固件、`bridge/crates/core/template.rs` 和测试。宿主 render 共用 C++ 引擎，逐像素一致，默认 battery=75；内建/v1字体文本ASCII净化，CSFN v2资产按有界UTF-8渲染。中文字库只随完整Bundle发布、不内置ROM；中文实机未验归C5。
- 缺配额/账号由模板 when 分支决定，不统一全局显示规则；现有 quad/codex-status-a、RC/label 行规则见操作细则，新增变体同步更新规则。
- Wi-Fi 密码可保存本机 data 和设备 RAM/NVS，认证读取后复用缓存，编辑框支持显隐。有效 session+owner 可管理全部配置，创建者只作来源；配置 ID 冲突明拒，回执/重放围栏按 principal/namespace 隔离。密码/任何 token 不入库、不进日志；BLE token 受控存储/传输。
- 运行数据位置遵循操作细则；CODEX_STATUS_DATA 优先，macOS 不写 `.app`。改 GATT 表后 Windows 需解除配对再重配；BLE 写入按 MTU 分片，Windows Python 桥设 PYTHONIOENCODING=utf-8。

## 嵌入式资源与恢复

- RAM/栈/NVS 分别预算，按实际条目分配/逐记录读；不预留空仓库、重复凭据或原生 struct 长期保存。固定数组仅用于有证据的硬件/实时需求或有界缓冲；压缩不豁免浪费，明确所有权/生命周期并处理分配失败。
- 持久格式显式标识/版本、长度、实际数量、校验及字段长度；先检查边界/数量/校验再分配。未知版本、损坏/截断/超长明拒，不靠 struct/padding/sizeof、不静默裁剪。
- 不把完整仓库、大局部对象或聚合临时副本压小任务栈；原地、按需堆或流式处理。启动/恢复/存储查匹配 ROM 栈帧与调用链；Wi-Fi 读取跑 `python3 tools/check-wifi-stack.py <matching.elf>`，宿主通过不代替嵌入式验收。
- 变更提供空/单条/典型/上限的持久字节、峰值 RAM/栈及成本依据；保持原子提交、读回校验/掉电恢复。删除回执冗余先验证重试/幂等/未知提交；新格式验证成功才退役旧键，移除兼容代码单列验收。

## AP01 自用 ROM 与救援

- **0031 基础 ROM 是专用 OTA/恢复底座**：保留原厂功能，提供升级、必要诊断及恢复通道，不需要业务应用、模板渲染或 C2 协议；不得把缺少这些能力列为它的缺陷或升级前置条件。建立底座时默认保留当前可用运行槽、升级非活动槽，不预设固定编号的稳定槽，也不用旧版本的临时槽位保护阻断基础 ROM 建立。其验收只针对原厂功能、OTA/恢复可用、失败可继续救援及保留槽位不变；与带应用 ROM 分开讨论和验收。
- 后续带应用的 AP01 ROM 以**移除原厂 OTA 签名门禁的 0041**为基线，保留原厂紧急更新/恢复入口。移除签名门禁不等于删除长度、分区边界、完整性校验或设备鉴权；不要改动原版归档文件，补丁与产物须可追溯。
- 自用 OTA、手动切槽、恢复操作保持直接可用，不照搬发布系统的严格准入。**稳定槽确认只启用自动回退，不是普通 OTA 或手动切槽的前置条件**；稳定记录缺失、旧格式不识别、应用未启动或业务协议不兼容，不得封死独立维护/原厂救援通道。不要把 build/manifest 精确匹配、业务 owner/context、稳定记录修订号等非必要条件叠加到紧急恢复入口。
- 保留防止写错设备、镜像截断/损坏、分区越界、覆盖当前运行槽或用户明确要求保留槽位的基本检查；原厂完整 ROM 与带应用 ROM 都应可恢复。手动切槽按明确目标及基本可启动性检查执行，不要求目标预先登记为稳定槽。切槽不顺带覆盖 ROM、清空配置或创建稳定记录；原厂恢复本身会清配置时须事先说明。
- **先验证后路，再替换前路。** 新 ROM、协议或保护策略上线前，先证明仍有不依赖新应用/稳定记录的可用更新或回切路径；替代通道在同机实测通过前不得关闭旧通道。至少模拟普通 OTA 写非活动槽、无稳定记录的手动切槽/恢复、失败后继续救援及保留槽位不变；模拟通过不能代替同机紧急通道验收。不再制造“必须先刷入修复 ROM，才能获得刷入该 ROM 的权限”的闭环。
- 巡检错误按打开/读取/头/长度/CRC 等真实阶段报告；缺少可选元数据不冒称镜像损坏，也不伪造稳定记录。OTA/恢复回执未知只观察原操作，不能换事务重发；下载完成、ACK 或进度值不代表成功，最后须认证同 MAC 的运行版本并确认原厂界面/旋钮正常。
- 2026-10-19 教训：稳定确认 `slot_image_invalid` 与 OTA 前置门禁互锁，导致正常运行设备无法自行更新；最后通过原厂下载暂存与主动恢复救回原版 0031，并需重新配网。具体现场看 `PROGRESS.md` 和 `project-workflow/ap01-template-integration/flash-0041-20261019.md`，不要把此次应急路线当作以后依赖的正常升级流程。

## 实机诊断闸门

- 以 `<exe>/data/platform/state.json` 登记为准，访问前枚举串口 VID/PID/serial 并核对 MAC，旧 IP 不当实时现场。Note4 使用原生 USB-Serial-JTAG。
- 不以“可能复位”跳过已授权串口诊断。优先无 DTR/RTS ioctl 的原生监听；Windows 不改 DCB/SetCommState，macOS pyserial 专用监听须跳过 modem 设置且不丢缓冲。false/false 不保证安全，也不将单次样本推广到其他平台。前后记录时间、工具/参数、控制线策略、boot_id/uptime/reset_reason 和 ROM 文本；USB/工具复位与 panic/WDT/brownout/深睡分别记账，reset=unknown 不作证据。操作前读细则。
- 用户授权后只读导出 dump：先保网络现场，核对真实 MAC/分区表；私有保存 dump/分区表/匹配 ELF/哈希，校验再解码；不顺带擦 dump 或写 ROM/分区/NVS，不提交/完整打印敏感 dump，工具进出下载模式复位单记。
- `/status.json` 是恢复视图，认证 `/api/status` 是业务状态，`/log` 是诊断流，`/pmstats` 为只读免 token。注册须核对结构化及认证 MAC；不恢复旧 HTML/claim_unsupported 回退。OTA UPDATE OK 后仍需认证运行核验；具体分区/监听/OTA步骤见细则。
