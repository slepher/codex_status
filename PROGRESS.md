# Codex Status 项目进度（交接文档）

## Fake ROM Stage C Bundle CHUNK 切片：2026-09-24 — 宿主验证通过

独立 `codex/fake` worktree 上，Bundle BEGIN 已提交 `bd916c7`；CHUNK 的 START 会话/offset、WRITE 边界、END append 决策也已抽入固件与宿主共享的 `src/v2_bundle_command.{h,cpp}`。设备端保留原始流与 LittleFS I/O、endpoint token/owner、重放比对和失败时会话失效顺序。宿主 render 测试 14/14 与 `git diff --check` 通过；未接主 Bridge/实机，未构建/烧录 ROM。COMMIT、Activate/claim、宿主端点仍待实现，详见 `project-workflow/fake-rom-simulator/status.md`。

## Fake ROM Stage C Bundle BEGIN 切片：2026-09-24 — 宿主验证通过

Data `02dd043`、Plan `e997a72` 已提交；接着在独立 `codex/fake` worktree 将 Bundle BEGIN 的重放、会话匹配、CRC/长度与恢复偏移决策抽为固件和宿主共用的 `src/v2_bundle_command.{h,cpp}`。设备端仍先鉴权，且仅在接收文件创建成功后写入新接收状态；重复 BEGIN 不续期。宿主 render 测试 13/13 与 `git diff --check` 通过。未碰主 Bridge、生产数据或实机，未构建/烧录 ROM。Bundle CHUNK/COMMIT、Activate/claim、宿主端点与完整 Fake ROM 尚待实现，见 `project-workflow/fake-rom-simulator/status.md`。

## Fake ROM Stage C Plan 决策切片：2026-09-24 — 宿主验证通过

Data 切片已提交 `02dd043`，随后在同一独立 `codex/fake` worktree 串行完成 PowerPlan 的共享 C++ 命令决策：形状校验、计划授予限制、ACK 分类由固件和宿主 render FFI 调用同一 `src/v2_plan_command.{h,cpp}`。原有认证、deadline、模式持久化和日志副作用留在设备端。宿主测试证明同 ID 重放不续期，冲突/旧 ID/非法形状不改变计划；provisional 300s、常规 600s、最低 30s 与 sleep 均通过。`cargo test -p bridge-render --test v2_state` 12/12、`git diff --check` 通过。未碰主 Bridge、生产数据或实机，未构建/烧录 ROM。后续仍需 Bundle/Activate/claim、宿主端点和完整 Fake ROM；详见 `project-workflow/fake-rom-simulator/status.md`。

## Fake ROM Stage C Data 决策切片：2026-09-24 — 独立 worktree 宿主验证通过

在 `codex/fake` 独立 worktree 从 `bridge-multi-instance-2026-09-24` tag 基线开始，ROM 文件已交接。`src/main.cpp::applyV2Data` 预检后的 Data 决策/ACK 分类由固件和宿主共用 `src/v2_data_command.{h,cpp}`；设备端认证、首次接受后的 checkpoint/缓存/显示原顺序保留。宿主 render FFI 覆盖首次接受、重放、冲突、旧 seq、错 context、CRC/字段顺序错误和未配置，拒绝不推进序列。`cargo test -p bridge-render --test v2_state` 11/11 通过（ArduinoJson 用已安装的只读头文件路径），`git diff --check` 通过。未改主工作树、未启动/停止 Bridge、未接设备、未构建/烧录实机 ROM、未提交。此切片仍不是可运行 Fake ROM；Stage C 后续 Plan/Bundle/Activate/claim 与宿主端点尚待实现，见 `project-workflow/fake-rom-simulator/status.md`。

## Bridge 两设备界面与推送：2026-09-24 — 主工作树已部署

模板页“推送到设备”已接入按 MAC 选择：只列当前屏幕族的已登记 v2 设备；确认时把族配置中启用的模板保存为目标设备 Profile，再调用该 MAC 的显式发布。设备页列出全部已登记设备，标出当前 MAC，平台状态卡也跟随当前设备。主工作树默认 Bridge 已重建并重启（PID 11644，HTTP/MCP 8765/8766），本地 `platform_overview` 保留 1.54 `70041DD7A340` 与 Note4 `7C4FADB93408` 两台登记；两个临时具名设备实例已停止。前端脚本语法、按 MAC 发布流程模拟、隔离 `cargo build --offline -p bridge-app` 与 `git diff --check` 通过。此轮**未实际发布模板、未 OTA**；两台设备上次 HTTP 状态读取超时。多实例代码提交 `3cfc8b8` 已打本地 tag `bridge-multi-instance-2026-09-24`，仓库未配置远端。

## Bridge 多实例启动：2026-09-24 — 主工作树实现，探针验证通过

Bridge 增加按实例名隔离的 Windows 进程锁、运行数据、owner ID 与托盘背景形状；`tools/start-bridge.ps1` 可传独立 HTTP/MCP 端口、形状与精确设备 MAC/IP。默认实例仍用 8765/8766 和原数据/owner；命名实例不争用固定 UDP 8767，使用按 MAC 的 HTTP/BLE 路径。命名 owner ID 保证与默认不同且不超过设备 claim 的 32 字符上限。两组假 MAC/loopback 探针在 8875/8876、8877/8878 与旧默认 8765/8766 同时运行，重复启动受脚本和进程锁拦截；探针已停止。Bridge app check、23 项测试和隔离构建通过。详见 `project-workflow/bridge-multi-instance/`。待替换并重启主工作树 Bridge；未 OTA、未改 ROM。

## Fake ROM 前置协议日志：2026-09-24 — 宿主验证完成

在多设备登记/BLE 改动提交 `c82487c` 后，串行完成 `project-workflow/fake-rom-simulator/` 的 Task 11a、10c、11b。Bridge BLE 现能按扫描 ID 记录候选广播、连接/GATT/完整 MAC 核验与 ACK；空窗口 info 日志每 30 秒汇总。当前已发布 1.54/Note4 ROM 确实发送 UDP，Bridge 的 v2 UDP 仅对已登记 MAC 作双状态认证改址，不写全局设备选择/IP/推送/BLE 标志，BLE 会合不依赖 UDP。HTTP/UDP/PowerPlan/Data/ACK 用安全字段和关联 ID 记录时间线；非成功 HTTP 错误不再附请求/响应正文。

隔离 `bridge-ble` 11 项、`bridge-core` 全套、app UDP 4 项定向测试与 app check 通过，`git diff --check` 通过。未启动生产 Bridge、连接实机或触碰 ROM。Fake ROM 同源入口及独立时钟尚未开始；现场协议日志仍待实机/模拟器联测。

## Fake ROM 多设备 BLE：2026-09-24 — app 逐 MAC 会合

Task 10b `project-workflow/fake-rom-simulator/task-10b-ble-cycle.md` 完成宿主验证：Bridge 从已登记 v2 MAC 一次扫描 BLE 候选，连接后复核完整身份，每台独立 55 秒尝试节流。UDP 通知在已有 v2 登记目标时优先走该路径。隔离 `cargo check -p bridge-app`、2 项候选测试和 `git diff --check` 通过。Task 9 登记与 Task 10a/10b BLE 本轮尚未提交、部署或接实机；多设备发现、Fake ROM 与实验时钟仍待完成。其他代理 ROM 改动未触动。

## Note4 400×300 图标与状态栏校正：2026-09-24 — 模板源码完成，待发布实机

按用户澄清，400×300 `codex-status-a` 已恢复原 `sleep-20` 为 zzz，深睡时与 BLE 共用 x=248 格；BLE OFF 留空。Wi-Fi/Bridge 的 On/Off 改为完整位图互斥绘制，不再叠画。日期到时间的可见间隔约 12→6 px；电量文字右对齐。用户提供的 Bridge 预览截图和实机都显示电量文字偏上，根因是 `ntthin18` 行高 26 px 被放进高 20 px 的区域，渲染器整体上移 3 px；现区域改为 `[344,5,44,26]`，对照渲染中 `--%` 墨迹中心从 y=15 到 y=18，电池外框中心 y=17.5。

五种状态的宿主 JSON/编译/序列化渲染逐像素差均为 0，图标格与预期原位图逐像素相同；49 操作、14 位图资源均在 ABI2 上限内，`git diff --check` 通过。仅改模板与项目文档；**未修改 ROM/Bridge 代码，未保存到桥模板库、未发布、未操作实机、未提交**。详见 `project-workflow/note4-icon-correction/`。

## Fake ROM 多设备 BLE：2026-09-24 — 一次扫描入口

Task 10a `project-workflow/fake-rom-simulator/task-10a-ble-any-target.md` 完成宿主验证：BLE 一次扫描覆盖登记目标的广播候选，连接后仍核对完整 Wi-Fi MAC；单目标连接复用同一实现。隔离 `bridge-ble` 7 项测试、`bridge-app` check 与 `git diff --check` 通过。尚未接 app 逐 MAC 会合调度、未访问设备或提交本 Task。Task 9 显式登记也已通过宿主测试，尚未部署。

## Fake ROM 多设备路由：2026-09-24 — 显式端点登记

Task 2–8 的 Bridge 多设备路由与计划已提交 `d3ba464`，没有纳入其他代理的 ROM 改动。Task 9 `project-workflow/fake-rom-simulator/task-9-register-endpoint.md` 现完成宿主验证：内建 MCP `platform_device_register_v2` 用参数 MAC 与 IPv4(:port) 显式登记目标，先核对结构化状态、认证 v2 状态及能力，成功后才保存；不自动 claim/推送。隔离 app check、登记测试 3 项、MCP 测试 1 项及 `git diff --check` 通过。尚未部署、接设备或提交本 Task；BLE 多设备机会、Fake ROM 与实验时钟仍待实现。

## Fake ROM 多设备路由：2026-09-24 — 逐 MAC HTTP 周期调度

Task 8 `project-workflow/fake-rom-simulator/task-8-v2-cycle.md` 已通过宿主验证：Bridge 按已登记 MAC 的认证在线缓存逐台执行占用、计划和投递；单台刷新失败不发送其本轮 Plan/Data。用户纠正当前没有 legacy 设备，本轮测试只用 v2 多设备；历史协议标记仍作防御筛选。隔离 `cargo check -p bridge-app`、3 项定向测试和 `git diff --check` 通过。BLE 多设备机会、显式登记、Fake ROM 与时钟接线仍待完成。未部署、未接实机、未提交，也未修改其他代理的 ROM 文件。

## Fake ROM 实施改为串行：2026-09-24 — 逐 MAC 占用完成

依用户要求取消并行开发；`project-workflow/fake-rom-simulator/plan.md` 改为 A→B→C→D→E→F→G 严格串行，每个任务由主代理设计、单名 6-luna high 编码、主代理评审后推进。Task 7 的 v2 owner/claim、yielded、续约时间已按 MAC 隔离；评审修正 409 不得记为成功续约。隔离 `cargo check -p bridge-app`、v2 测试 2 项、claim 回归 2 项与 `git diff --check` 通过。尚未接周期投递、未部署、未接实机、未提交。其他代理的 ROM 文件未触动。

## Fake ROM 多设备路由：2026-09-24 — 逐 MAC 认证状态轮询

Task 6 `project-workflow/fake-rom-simulator/task-6-v2-status-poll.md` 完成宿主验证：Bridge 每 10 秒只读轮询已登记的非 legacy 设备，认证状态按 MAC 独立缓存；错 MAC/离线仅改变该目标条目。隔离 `cargo check -p bridge-app`、缓存测试 1 项、claim 回归测试 2 项和 `git diff --check` 通过。此缓存尚未接逐台 claim/投递；Fake ROM 与实验时钟也未接通。未部署、未接设备、未提交。ROM 由另一代理修改，本任务未触动。

## Note4 屏幕供电双模式：2026-09-24 — 固件实现，待构建与实机

Note4 新增设备端 NVS `pm/panel_pwr`：默认 `keep`，可通过鉴权 `POST /diag?panel_power=keep|off_cache` 或串口 `panelpower keep|off_cache` 切换，`/status.json.panel_power_mode` 回读，供后续配置界面复用。`keep` 在深睡保持 GPIO6 屏幕逻辑电源，`off_cache` 保留断电方案；两者均关闭 SSD2683 内部高压。进入深睡保存经校验的整帧缓存，分钟薄唤醒以 RTC 时钟窗口叠加恢复驱动旧帧，无须每分钟写闪存；缓存无效时保守全刷。`0.18.21-note4-b` 终版和 1.54 回归构建均通过；1.54 首次失败系中断后遗留的单个零字节 ESP-IDF 生成对象，Luna 删除该文件后重构成功。候选 ROM `artifacts/codex-status-0.18.21-note4-b-panel-modes.bin`，1,742,144 B，SHA256 `917FAFADA6886A797FC9C2390990286A90E8DB173BF1DF1A50E3778879C9FD6A`。见 `project-workflow/note4-panel-power/`。尚未刷机，功耗与局刷需实机验证。

## Fake ROM 多设备路由：2026-09-24 — claim 目标端点核验

Task 5 `project-workflow/fake-rom-simulator/task-5-claim-target.md` 完成宿主验证：claim 按目标 MAC 取端点，并在发送设备 token 前用状态页 MAC 预检；本地 HTTP 测试覆盖错 MAC 零 POST 与匹配 MAC 正常 POST。隔离 `cargo check -p bridge-app`、2 项 `claim_target_tests` 和 `git diff --check` 均通过。尚未部署、接实机或提交；按 MAC owner/在线缓存、周期调度、Fake ROM 与时钟接线仍待实现。ROM 由另一代理修改，本任务未触动。

## Fake ROM 多设备路由：2026-09-24 — 显式端口状态读取

Task 4 `project-workflow/fake-rom-simulator/task-4-device-endpoint-port.md` 完成宿主验证：`bridge-core::device` 支持 `127.0.0.1:port`，裸 IPv4 仍默认 80；本地 TCP 测试取得结构化状态与 MAC。隔离 `cargo test -p bridge-core` 各组全通过，`git diff --check` 通过。未部署、未接实机、未提交；下一步按目标 MAC 隔离 `/claim` 端点与状态，再接多设备调度。ROM 由另一代理负责，本任务没有触动。

## 1.54 时钟局刷交替问题：2026-09-24 — 源码修正，待实机验收

- 用户实见 1.54 屏每分钟局刷、全刷交替；这不是对该行为的认可。既有 v2 记录显示 60 秒 BLE 会合与整分钟时钟唤醒可交错。源码确认完整会合启动会重载同一个 CompiledTemplate，却无条件清除 RTC 时钟旧像素有效标志，导致会合时按“无基线”全刷；纯时钟唤醒则可局刷。
- 已在 `src/main.cpp` 仅当 v2 context 和时钟窗口全部参数相同才保留 RTC 旧像素；成功绘制后重取屏上窗口。新模板/上下文、窗口变化或绘制失败仍保留全刷兜底。1.54 `pio run` 构建通过、`git diff --check` 通过；**未刷设备，交替是否消失尚未实测**。详见 `project-workflow/clock-window-retention/`。

## Fake ROM 多设备路由：2026-09-24 — 操作 token 按 MAC 隔离

Task 3 `project-workflow/fake-rom-simulator/task-3-device-token-mac.md` 已通过宿主验证：`/claim`/OTA 的设备操作 token 使用带 MAC 的独立缓存，BLE 在发送取 token 命令前核对绑定 info 的 Wi-Fi MAC，OTA 在使用 token 前核对目标状态页 MAC。旧单份无 MAC 缓存保留但不再读取，后续首次操作可能需要重新经绑定 BLE 获取。隔离 `cargo test -p bridge-mcp` 1 项、`cargo test -p bridge-ble` 4 项、`cargo check -p bridge-app` 和 `git diff --check` 均通过。未部署、未接实机、未提交本轮改动；另一代理正在修改 ROM，当前任务不触其文件。Bridge 按 MAC 状态缓存、claim/周期调度与 Fake ROM 进程仍待实现。

## Fake ROM 多设备路由：2026-09-24 — 目标 MAC 写入守卫通过

前一里程碑的计划与 Coordinator 显式时间已提交 `cc6a6c1`。本轮 `project-workflow/fake-rom-simulator/task-2-target-mac-guard.md` 已完成：Bridge v2 HTTP 写入前用认证 `/v2/status` 的 MAC 核对目标，`send_plan`、`request_light`、`refresh_status` 改取指定 MAC 的设备端点；错 MAC 状态不写入缓存。测试对 Data/Plan/Activate/Bundle 各试一次错目标，只产生 4 次 GET、0 次 POST。隔离 `cargo test -p bridge-core` **105 项通过**，`cargo check -p bridge-app`、`git diff --check` 通过。未部署、未操作硬件、未提交本轮改动。多设备周期调度、按 MAC token/claim 与 fake 时钟仍待实现；另一代理正在修改 ROM，本轮不触其文件。

## Fake ROM、多设备与实验时钟：2026-09-24 — 计划落地、首个时间接点完成

已建立 `project-workflow/fake-rom-simulator/plan.md`：一个 fake 进程一台设备；生产 Bridge 按 MAC 隔离运行目标和时间上下文；每台 fake 设备的 Bridge 视图与设备视图分别配置倍率、偏移、漂移，可测试同步和失步。原 `docs/fake-rom-simulator-design.md` 的全局共同时钟表述已修正。`release` worktree 仅用于源码隔离，不用多开生产 Bridge 替代多设备路由。

Task 1 已由 6-luna high 编码并经主代理核对：Coordinator 四处隐藏 `crate::now_secs()` 改为显式 `now`，生产调用仍传真实时间；跨 MAC 作业/计划/summary 测试新增。隔离 `CARGO_TARGET_DIR=bridge/artifacts/coordinator-explicit-time-target` 的 `cargo test -p bridge-core` **104 项通过**，`cargo check -p bridge-app` 与 `git diff --check` 通过。未部署、未操作设备、未提交。**仍待** Bridge app 多设备运行路由、service/app/BLE 其余时钟接点、同源 Fake ROM 进程与联测；当前没有可运行的 Fake ROM。细节见 `project-workflow/fake-rom-simulator/status.md`。

## Note4 同步、时钟与 96px 模板：2026-09-24

- Bridge 的 Codex 数据源保持 `good`；此前 Note4 Profile 的四项配额绑定仍指向 `static1` 且 `sync_enabled=false`。已只对 Note4 改为 `codex` 并启用同步；实机在本轮模板发布后确认 `data_seq=16`、`applied_seq=16`、`display_state=displayed`。1.54 的旧 ABI1 能力仍被 Bridge 接受用于数据通道，当前该设备有待投递状态，未将 Note4 的 ACK 误算给它。
- Bridge 投递按目标 MAC 解析设备链路，不再借用界面当前选择的设备 IP。MCP `template_save_v2` 现通知界面刷新模板缩略图；界面预览在未显式传入测试用量时使用 Bridge 当前缓存的用量信封。Note4 模板已通过 MCP 保存，`source_crc=11455c65`、`compiled_crc=442007ea`，并已显式发布完整 Bundle，作业 `4905c76c` 获 `applied/displayed` ACK。
- Note4 B 经鉴权 BLE 令牌和单次 HTTP OTA 从 `0.18.19-note4-b`/`ota_1` 升至 `0.18.20-note4-b`/`ota_0`，原模板与作业在升级后保留；随后安装新 Bundle，设备报告 `compiler_abi=2`、`partial=true`、`commit_seq=257`。ROM 为 `artifacts/codex-status-0.18.20-note4-b-abi2.bin`，1,739,488 B，SHA256 `DDA7F214127814FC93B03B06D4C50818D77847CCCDF18938701E12C69C3F094B`；OTA 证据见 `artifacts/note4-ota-01820-b-abi2.json`。
- 模板全部用量数字使用 `ntreg96`；weekly 独占居中，移除可见 `RESET` 但保留重置时间；蓝牙、Wi-Fi、连接图标和深睡 `zzz` 按状态显示。ABI2 的上限为 64 操作、16 位图资源；本模板用 57/16。六种 400×300 宿主预览的 JSON/编译/序列化像素差均为 0，Note4 B 构建成功。
- **已知问题，按用户要求仅记录、不在本轮修正：Note4 时钟更新目前表现为全刷。** Note4 深睡会切断屏幕电源，深睡唤醒后可能失去局刷基线并走本地全刷回退；实机刷新质量、残影与后续局刷行为另行验收。时钟本地更新不依赖数据同步开关。
- 其他现场与设计见 `project-workflow/note4-live-sync/`、`project-workflow/note4-template-96/`。

## Bridge 后台运行：2026-09-23

当前工作树的 `bridge-app` 经 `cargo build --offline -p bridge-app` 构建后，使用
`tools/start-bridge.ps1` 在 Codex Desktop 进程外分离启动。界面范围修正后已重启：
主进程 PID 46796，watchdog PID 15716；本地 `127.0.0.1:8766/mcp` 对 GET 返回
405，表明端点已监听；
启动后两进程持续存活。日志为 `artifacts/bridge-app-run.out` 和 `.err`，PID 文件在
`artifacts/bridge-app-run.pid`。未发布模板、未改设备、未提交 Git。

族下拉只有「1.54 黑白」「Note4」两个短名；模板页两族统一使用族 Profile：
旧行界面、最多 8 项、启用开关、无初始项设置；单独模板库卡片已移除。启动时
无损迁入旧 1.54 配置和唯一 Note4 设备配置，原文件/设备 Profile 保留，见
`project-workflow/bridge-multi-device-ui/status.md`。按 MAC 的多设备路由与族发布菜单
仍是待办，两个族的统一推送项暂禁用。

## 新增待办：2026-09-23 — Bridge 多设备与模板页迁移

用户要求 Bridge 同时支持多设备；把设备 Tab 的 v2 模板/Profile 操作迁至模板 Tab；
「推送到设备」使用兼容设备子菜单选择明确目标。当前 Profile 对应类型只有一台兼容设备
时，在按钮下方以小字 note 显示该设备。实现顺序、现有全局 `device_mac` 与 UI 首项选择
问题、0/1/多设备验收见
`project-workflow/bridge-multi-device-ui/plan.md`。本节是待办记录，尚未改 Bridge/UI、
未部署、未实机验证。

补充界面要求：在截图中「默认 ▾」Profile 下拉框左侧加入「族 ▾」；族明确以
`render_target` 为键。选择族后只显示该族 Profile，并联动模板变体、预览和推送目标。
同名 Profile 可分属不同族；切族不改设备。
族内草稿与设备当前 Profile/PublishJob 的分离及旧配置迁移要求已补入上述计划。

## Bridge 显式 light 计划实机验证：2026-09-23 — 深睡后 BLE 会合唤醒通过

Bridge 的 MCP/UI 显式 light 现先持久化正式 PowerPlan，离线时排队，在下一次
BLE v2 会合发送同一计划并以 ACK 确认；重启可恢复，读取状态不延长时限。
Note4 `0.18.19-note4-b` 电池供电（USB 已拔、94%）实测：sleep 计划 53
获 ACK 后 HTTP 离线，light 计划 54 排队并经 BLE 获 ACK；设备回读
`reset=deep-sleep`、`wake=timer`、Wi-Fi 在线、`power.plan_id=54`、
`mode=light`，原模板仍在。用户随后插回 USB，设备回报 `plugged=true`。
隔离 Rust 测试 126 项通过；当前 Bridge 已更新并运行，未提交 Git。
详见 [Bridge 状态](project-workflow/note4-bridge-publish/status.md)。
字体 manifest/CSFN 增量发布合同仍见待双方确认的
[protocol.md](project-workflow/note4-bridge-publish/protocol.md)，不能据本次
电源验证推断字体发布端点已可用。

## Note4 实机里程碑：2026-09-23 — USB 防深睡修复与 A/B OTA 再验证

精确 MAC 核验后，USB 刷入 `0.18.19-note4-a` 到 `ota_0` 并仅擦除 otadata；
A 经串口/HTTP 启动，已安装的 400×300 模板保留。随后经绑定加密 BLE 获取
token，只发送一次受鉴权 OTA，收到 HTTP 200 `UPDATE OK`；
`0.18.19-note4-b` 经 HTTP 与串口确认从 `ota_1` 启动，模板/PSRAM 保留。
USB 接入且 Bridge 的 sleep 计划到期后，A 仍保持 light 并提供 HTTP。
B 也在计划与 OTA 保活均到期后持续在线 610 秒，HTTP/串口与模板正常。
ROM SHA256、双槽证据、客户端初始误判原因和恢复
操作详见 [Note4 状态](project-workflow/note4-ota-bringup/status.md)。未提交 Git。

## Note4 实机里程碑：2026-09-23 — 掉线证实为深睡会合，USB 修复 ROM 已构建

Note4 旧版 `0.18.18-note4-b` 在 PC USB 连着时仍因 v2 计划到期进入深睡；
重插 USB 后 COM5 短暂重现又掉线。72 秒 BLE 扫描发现精确设备广播，Bridge
在最近两个约 60 秒窗口成功下发 `sleep` 计划，证明设备仍在定期会合。
固件现已统一在深睡入口检查 USB，并对 USB SOF 消失增加 10 秒消抖；
`0.18.19-note4-a/b` 修复 ROM 均构建成功，待稳定 USB 下载模式刷入 A，
然后受鉴权 OTA 验证 B。ROM SHA256、现场证据和恢复步骤见
[Note4 状态](project-workflow/note4-ota-bringup/status.md)。未改 Bridge 源码或工作流，未提交 Git。

## Note4 首次 Bridge Profile 实机发布：2026-09-23 — 模板已安装并显示

设备 `0.18.18-note4-b` 启用 OPI PSRAM 后，Bridge 用原持久化作业 `b462a509` 发布 ABI1 完整 Bundle。修正冻结 Bundle 顶层缺失的 `bridge_id` 后，设备返回 `applied/displayed` ACK；鉴权状态回读为 `configured=true`、`committed_job_id=b462a509`、活动模板 `codex-status-a`。设备任务的 [实物照片](artifacts/note4-first-bundle-display-20260923.jpg)可见非空白 400×300 模板与数值。Bridge 当前正常运行，活动发布队列为空，本地历史为 `succeeded`；未提交 Git。具体过程和测试见 [Bridge 状态](project-workflow/note4-bridge-publish/status.md)。设备侧正补同作业重放的持久幂等保护（本次 `commit_seq=2`）；**字体 manifest/CSFN 增量发布仍未实机就绪**，合同在待双方确认的 [protocol.md](project-workflow/note4-bridge-publish/protocol.md)。

发布后的设备后续又进入深睡，USB 仍接着但局域网/串口暂不可达；固件任务正修正 USB 接入信号与深睡入口。这不撤销已取得的提交 ACK 和屏幕照片，但后续实机状态回读须待设备重新唤醒。

## Note4 实机里程碑：2026-09-23 — PSRAM 与流式 Bundle 已跑通

用户按键唤醒后，Note4 B 槽通过 USB 刷入 `0.18.18-note4-b`，串口与
HTTP 确认启动；`psram_free` 从 0 升至 8,351,272 B。Bridge 原冻结作业
的 27,086 B Bundle 已按 4096 B 分片全部写入，设备完成正文读取和 CRC；
因冻结 Bundle 缺顶层 `bridge_id`，设备按 owner 校验拒绝提交，仍未显示模板。
该字段由同期 Bridge 工作修复，本任务不改其源码。证据和下一步见
`project-workflow/note4-ota-bringup/status.md`。

## Note4 实机里程碑：2026-09-23 — 定位 Bundle 断连与未启用 PSRAM

空白 Note4 接收 4096 B Bundle 分片时，设备实际可在 LittleFS 写完全部
27,086 B（每片 10–30 ms），但 Bridge 收到连接复位且无提交 ACK。独立
32 KiB 文件写入跨越 16 KiB 边界成功；HTTP 不存在 1024 B 上限。
诊断状态显示最大连续内部堆约 15 KiB、`psram_free=0`，尽管芯片板载
8 MiB PSRAM。Note4 独立构建环境已启用 OPI PSRAM，并把 Bundle CHUNK
改为受鉴权的流式接收；对应 Bridge 请求头由同期 Bridge 工作负责。
启用 PSRAM 的 B ROM 已构建但因设备进入休眠尚未刷入；原发布任务保留、
Bridge 已暂停。现场与 ROM SHA256 见
`project-workflow/note4-ota-bringup/status.md`。

## Note4 Bridge 实机发布排障：2026-09-23 — 冻结作业保留，设备暂离线

Bridge 已用真实 400×300/ABI1 能力与绑定令牌对 Note4 显式 claim，并尝试发布内建字体 `codex-status-a` 完整 Bundle。首次作业 `5c6d63fc` 的第 4 个 4096B 分片在 offset 12288 遇到设备断连；旧完整 Bundle 队列仅存内存的问题随后已修复。当前持久化作业 `b462a509`、Bundle 27086B；Bridge 重启后验证同一冻结作业恢复，降为 1024B 分片再试，于 offset 14336 超时，随后 Note4 HTTP 整体不可达。设备未返回提交 ACK，先前 `commit_seq=0`，**不能认定已发布或显示成功**。已只停止当前 Bridge 主进程及 watchdog，保留原运行数据与冻结作业，等待固件任务排查设备后再恢复。详情及测试见 [Bridge 状态](project-workflow/note4-bridge-publish/status.md)；新字体 manifest 协议仍见待双方确认的 [草案](project-workflow/note4-bridge-publish/protocol.md)。

后续诊断版 `0.18.18-note4-b` 显示 LittleFS 连续写 32 KiB 正常，而固件未启用板载 8 MB OPI PSRAM（`psram_free=0`、最大连续堆约 15 KiB）。设备任务正编译 PSRAM 与流式分片接收固件；Bridge 已为旧 Bundle 分片增加三项校验头，并改为按完整 `Content-Length` 读取 ACK，宿主隔离测试 122 项通过。**Bridge 仍暂停，原作业 `b462a509` 未提交；待 PSRAM 实测可用后重试同一作业。**

最新现场：设备端修复 ROM 已构建，但 Note4 等待期间休眠，LAN 与 USB 下载握手暂不可用。固件任务需用户短按屏幕下方任意唤醒键，随后刷入并确认 `psram_free`；Bridge 再部署已构建客户端，恢复同一冻结作业。当前没有实机提交 ACK 或显示验收。

## Note4 实机里程碑：2026-09-23 — 发布失败定位为待发任务与协议阻塞

截图中的“a publish is already in progress”来自首次发布留下的 Note4
待发任务 `6640f3be`；再次点击触发冲突。设备先前报告 `BLE ON`，但设备端
仍为 0 个已安装模板、`commit_seq=0`。**更正：**先前的 HTTP 失败来自
受限命令环境的 `Bad access`；局域网直连已证实 HTTP 200、Wi-Fi 在线。
真正阻碍是 Bridge 端点尚未存入设备（鉴权 401）和旧 Bridge 把 Note4
误登记成 200×200。现已用绑定加密 BLE 配置随机端点令牌，固件新增真实
400×300 能力字段，受鉴权 v2 状态读取成功。模板使用内建字体，现有
完整 Bundle 通道可发布；首次设备提交仍在实测。详细证据见
`project-workflow/note4-ota-bringup/status.md`。

## Note4 Bridge 界面核对：2026-09-23 — 独立运行数据中 v2 Profile 可见

用户截图中的顶部 `配置 ▾ / note4` 是旧版配置（空模板列表、200×200
旧模板库），并非 Note4 的 v2 Profile。为避免界面固定选多设备列表首项，
Bridge 改用 `artifacts/note4-bridge-data/` 独立运行数据，平台中只登记
Note4；原 `bridge/target/debug/data/` 和 1.54 英寸设备备份保留。
实看“设备”页已显示 Note4 在线、`0.18.13-note4-b`/`ota_1`，
`Profile（1–8 项）` 显示 `1. codex-status-a`、初始 active；
设备端仍为 0 个已安装模板，未发布。顶部旧配置菜单边缘裁切属于 Bridge UI
布局问题，本任务遵守同期边界未修改 Bridge 源码。细节与恢复路径见
`project-workflow/note4-ota-bringup/status.md`。

## Note4 Bridge 运行现场：2026-09-23 — 已切换绑定并建本地 400×300 Profile

按用户要求，先备份原 1.54 英寸设备的 Bridge 配置和设备记录到
`artifacts/bridge-before-note4-bind-20260923/`，再将当前 Bridge 实例绑定
Note4（`7C4FADB93408` / `192.168.3.177`）。可见面板与 MCP 均显示
Note4 在线，固件 `0.18.13-note4-b` 从 `ota_1` 运行；原设备及
`quad/full/mini` Profile 仍保存在平台数据中。新建 Note4 本地 Profile，
只含 `codex-status-a`（400×300），初始模板为它、自动同步关闭。
**尚未发布模板**：当前运行的 Bridge 二进制把 Note4 错登记为 200×200；
固件状态也缺少新 Bridge 源码要求的显式能力字段，需对齐后再发布。
备份 SHA256、恢复旧绑定方法和现场细节见
`project-workflow/note4-ota-bringup/status.md`。未改 Bridge 源码或其工作流文档，未提交 Git。

## Note4 实机里程碑：2026-09-23 — 受鉴权 OTA 完成，B 从 ota_1 启动

最终 ROM A/B 为 `0.18.13-note4-a/b`，SHA256、USB/HTTP 双槽启动证据和恢复步骤见
`project-workflow/note4-ota-bringup/status.md`。A 经 USB 从 `ota_0` 启动；
使用精确 Note4 身份及已绑定加密 BLE 获取的 token，仅发送一次受鉴权
`/doUpdate`，B 随后从 `ota_1` 启动（HTTP 与 USB 串口均确认）。
当前设备 Wi-Fi 为 `wd21-la`，验证时 IP `192.168.3.177`；
B 启动日志确认 LittleFS、Wi-Fi、NVS token 可读。屏幕整黑/半黑诊断成功；
调整后的高清照片可见状态页，设备相对摄像头倒置（用户确认按钮应在屏幕下方），
不属于固件旋转错误；小字像素级验收、按键/唤醒仍未完成。
本任务未改 Bridge 源码或其工作流，未提交 Git。

## Note4 实机里程碑：2026-09-23 — 40 MHz bootloader 修复 NVS/LittleFS

Note4 使用 80 MHz bootloader 时 NVS/LittleFS 写入失败；只将应用改为 40 MHz
仍失败，单独擦 NVS 也未修复。将 bootloader 和应用均设为 40 MHz 后，
文件系统初始化成功，Wi-Fi 配置与鉴权 token 可跨重启从 NVS 读取。
诊断擦除前已备份原始 16 KiB NVS；当前 NVS 是重新配置后的内容，
原厂元数据可从备份恢复。备份哈希、刷机日志与详细因果证据见
`project-workflow/note4-ota-bringup/status.md`。

## Note4 实机里程碑：2026-09-23 — A 从 ota_0 启动，整屏诊断通过，OTA 待联网

Note4 COM5 / MAC `7C:4F:AD:B9:34:08` 已用独立 16 MB / DIO / 80 MHz
环境写入 `0.18.6-note4-a`（bootloader、分区表、ota_0；NVS/otadata 保留），
esptool 哈希校验通过。USB 串口确认 `fw=0.18.6-note4-a`、`slot=ota_0`、
EPD BUSY 失败 0 次；实机全黑和半黑画面均刷新成功，已恢复配网页，
但广角照片中文字不清晰。LittleFS 尚未挂载成功。ROM B 已构建并哈希，
设备因无保存的 Wi-Fi 处于 AP `192.168.4.1`，OTA 尚未进行；
待读取/输入网络凭据的授权路径确定后再做受鉴权升级与 `ota_1` 启动验证。
详情、ROM A/B SHA256、刷写重试、照片和恢复步骤见
`project-workflow/note4-ota-bringup/status.md`。本任务不改 Bridge 源码，
不写回通用平台实现工作流。

## Note4 Bridge 增量发布准备：2026-09-23（未提交，协议待双方确认）

Bridge 已加入精确 Note4 能力校验、Profile `render_target`/显式 `font_ids`、CSFN 导入、冻结的完整 manifest/对象、差量与安装峰值规划、持久化待发任务，以及 UI/MCP 的预检和字体版本选择；现有整包和 legacy 路径保留。宿主 Rust 测试与 400×300/200×200 编译渲染对比通过。`project-workflow/note4-bridge-publish/protocol.md` 是**待设备/Bridge 双方确认的草案**：当前设备仍为 ABI 1、48 KiB/8 字体实现，也没有增量端点，因此 Bridge 新任务明确等待协议，未进行 Note4 实机资产发布。A/B ROM OTA 另见 `project-workflow/note4-ota-bringup/status.md`，不能替代本协议验收。完整测试矩阵、限制和下一步见 `project-workflow/note4-bridge-publish/status.md`。

## 引擎化字体资产：2026-09-23 — 单一字体注册表 + CSFN 容器 + 设备字体库（未提交、未发布）

用户改任务：**只调整设备端与 bridge 端引擎**，字体与排版待引擎就绪后再细调；正文采用
Noto Sans Thin 18px、大文字采用 Regular 64px（字号为暂定值，由实现方选择）。同时
**停止抗锯齿（2bpp/gray4）方向**。本节即是该项工作的交接，详情与证据见
`project-workflow/generic-display-platform-implementation/task-7-font-assets.md`，容器合同见
`docs/font-asset-format.md`。

- **单一字体注册表**：`src/template_engine.cpp` 的 `TPL_FONTS`（9 项，只允许追加，因为
  `CtOp.font` 是持久化索引）。`main.cpp` 时钟快路径不再自带字体表，改走
  `tplFontClockBox`/`tplFontDrawClock`；`refresh_policy` 早已使用共享 cell helper；
  Rust 侧 `template.rs::FONTS` 同步为同一份名单。
- **新字体**（`tools/note4-fonts/rasterize_ttf.py`，FreeType 单色 `FT_LOAD_TARGET_MONO`，
  源为静态 hinted Noto TTF）：`ntthin18`（Thin 100 @18，ASCII，blob 1412 B）、
  `ntreg64`（Regular 400 @64，等宽数字，blob 2457 B）；0–9 advance 相同（不跳字）。
  TTF 置于 `tools/note4-fonts/vendor/`（已 gitignore）。
- **CSFN v1 容器**：同一工具同时产出引擎头文件与 `.bin` 资产（`bridge/assets/fonts/`，
  `18c2e4ed`/`4dc3b226`）；设备解析器 `src/font_asset.{h,cpp}`、按 Profile 的设备字体库
  `src/font_store.{h,cpp}`（tmp→校验→rename 原子写、CRC 复检、name==id、上限、prune）、
  bridge 字体库 `bridge/crates/core/src/platform/fonts.rs`（内容寻址、去重、Profile 依赖、
  inventory 差集、上限）。
- **修复了工作树里真实存在的破损**：宿主 FFI 无法链接 `rgnSetPanel`（被放进匿名
  namespace）、`compiled.rs` 调用旧 1 参 `rgn_build_compiled`、`rgn_build` 用 200×200
  几何推导 400×300 区域、`RGN_MAX=32` 小于模板的 38 个元素、`Rgn::area` 在 120000 像素
  面板上 `uint16_t` 溢出、`build.rs` 未跟踪字体头文件（预览可能用旧字形）、宿主 LittleFS
  shim 没有目录语义。
- **验证**：`cargo test -p bridge-core -p bridge-render -p bridge-mcp`（隔离 target 目录）
  **110 passed / 0 failed**（含设备解析器 3 项、设备字体库 5 项、bridge 字体库 18 项）；
  `bridge-render --compare-compiled --regions` 在重生成的 400×300 模板上
  `diff pixels: 0`、往返 `0`、两条路径均 18 个区域。测试发现真 bug：设备 inventory 会把
  名字与内容 id 不符的文件按内容 id 报出，已改为 name==id 才算已安装。
- **未做（下一步）**：模板→资产解析（`CtOp.fontRef` + `CT_ABI` 升级）、
  `font_inventory` 与 BEGIN/CHUNK/COMMIT/ACK 传输、bridge 发布预览/MCP/UI、
  最终排版与字重定稿、`assets` 分区落地。**交接 prompt：
  `project-workflow/generic-display-platform-implementation/prompt-font-assets-transport.md`。**
- **待验证的阻塞项**：`pio run -e esp32-s3-epaper-154g` 曾因 `font_store.cpp` 三处编译错误失败
  （设备 `fs::File::name()` 返回 `const char*`，宿主 shim 返回 `std::string`，代码却调用
  `String(f.name().c_str())`）。两侧已修（shim 的 `name()` 改回 `const char*`，源码改用
  `String(f.name())`），宿主 5 项测试复通过，但**固件自修复后未再编译**，下一轮第一件事就是
  重跑 `pio run`。

## Note4 模板宿主交付：2026-09-23 — A 方案已通过 MCP 保存与渲染（rev 2）

Bridge 已重建并运行，本地 MCP `127.0.0.1:8766/mcp` 可用。A 方案 400×300 模板 `codex-status-a` 通过 `template_save_v2` 保存（`epd-ssd2683-400x300-1bpp`，`source_crc=2b523381`，`compiled_crc=41c31abd`，`published=false`）；MCP 实际渲染正常数据、Pro 仅 weekly/RC=0、电量 0/25/50/75/100%、剩余量 0/100 边界、无数据离线、超长套餐/账号名六类场景，PNG 均 400×300/纯黑白。状态栏右侧图标组已收紧并靠右；主机采用用户选定的 Icons8 TV Off 轮廓，项目命名 Bridge Off，并制成配套 Bridge On。电池基于用户选定的 Icons8 图形，`device.battery` 同时驱动外框内填充和百分比；宿主 C++ 引擎已补本地电量的数值绑定。说明文字统一为 `f16`、主数字采用同系列 `f24` 两倍放大。

rev 2 修正（用户反馈“电量填充/文字未居中”，并要求主区 `%` 与数字垂直居中）：用 `concepts-400x300/measure-preview.py` 逐像素实测后，把状态栏日期/时间/电量百分比与电池填充对齐到图标中心线（日期 ink 中心 14.0→18.0、时间 13.5→17.5、百分比 15.5→17.5），电池填充改为外壳内腔对称 6 行并加宽到 14 px（100% 顶到内腔右缘），主区三个 `%` 的 `y` 由 149 改为 127（ink 中心与数字同为 132.5）。成因是供应商 `GUI_Paint.cpp` 的 1×1 `Paint_DrawPoint` 偏移 1 px 与矩形填充少一行，二者同时作用于固件与宿主；为保留 200×200 原有行为未改引擎，只在 400×300 模板内补偿。另按既有 `device.offline_mins` 合同补了页脚 `OFF <n>M` 行，离线/过期不再复用旧数字。宿主侧同时补上两个 400×300 缺口：compiled 渲染入口按画布分配帧缓冲（`render_compiled_bits_size`）、参考 PNG 解码支持任意画布，`bridge-render` 新增 `--compare-compiled` 与画布感知 `--diff`，400×300 的 JSON/compiled 像素一致性与 CTP1 往返均为 `diff pixels: 0`。

源码、输入与预览见 `project-workflow/generic-display-platform-implementation/concepts-400x300/`。宿主 400×300 校验、CTP1、PNG 链路及 `device.date` 已接通；`cargo test -p bridge-core -p bridge-render -p bridge-mcp`（隔离 `CARGO_TARGET_DIR`，83 passed/0 failed）、quad 的 `--compare-compiled` 回归通过。配额缺失的表现按用户澄清确认为模板属性：`AGENTS.md` 的显示规则条目已改为按变体描述（quad 静态 100 / A 版隐藏 5h 并把 weekly 提到主位），不再是全局统一规则。蓝牙/Wi-Fi/Bridge 连接图标目前是固定图形，独立连接状态绑定未定义；Note4 实机、发布、局刷仍未做。

## 设计候选：2026-09-23 — Note4 400×300 状态页

已先完成三张纯黑白、原生 400×300 的布局候选图，均采用顶部状态栏：
`project-workflow/generic-display-platform-implementation/concepts-400x300/`。
分别为双主数值、信息列表和进度仪表；图片与可重绘脚本、方案说明同目录。
另补三张 Pro 仅 weekly 桶的对应状态稿。它们将缺失的 5h 区块收起，与当前“缺失 5h 显示静态 100”规则不同；正式实现前需先确定产品规则并同步合同。
用户选定 A 双主数值方案；状态栏规格定为内容区 28 px、上下各 4 px（总高 36 px），左日期/时间，右蓝牙/Wi-Fi/主机/电池/电量百分比，图标盒 20×20 px。详见同目录 `README.md`。
本轮仅做视觉设计，未新增 JSON 模板、修改渲染协议、构建或发布到设备。
选定方向后再做缺失/离线状态稿与实际模板验证。

## 排期更新：2026-09-23 — Note4 已到货

用户确认第二硬件 Note4 已到货。到货解除实机可用性阻塞；GPIO 映射、SSD2683 面板时序及
waveform LUT、Flash/PSRAM 规格尚未在仓库资料中确认，Note4 目标头文件仍保留显式 `#error`，
设备尚未 bring-up、刷机或验收。下一阶段顺序与验收关口见
`project-workflow/next-execution-plan-2026-09-23.md`：现有 1.54 英寸设备先固定功耗基线，
Note4 同时开展硬件事实盘点；其后分别完成 DFS/btpm 对照、Note4 全刷与 v2 路径、
会合节奏核查、`bridge_first` 生产实现和双策略 A/B。
核对当前工作树时发现：虽然目标头文件含 Note4 的显式缺失项检查，`platformio.ini`
尚无 Note4 env；下方 09-22 记录的“环境已准备”与当前工作树不符，须在 bring-up 时补齐。

## 实机盘点：2026-09-23 — Note4 USB 只读评估

COM5 枚举为 Espressif USB-Serial/JTAG（VID:PID `303A:1001`，USB serial/MAC
`7C:4F:AD:B9:34:08`）；esptool 识别 ESP32-S3 rev 0.2、16 MB Flash、8 MB PSRAM。
只读分区表已保存到 `artifacts/note4-partition-table-0x8000.bin`（4096 B，SHA256
`A82133FA4CD77C180D65FA75CA3B5C27BCEBFB8CC4D419362838852E995BA9E5`）；Note4 设备表
为两个 0x5F0000 OTA 槽和 4 MB assets，不能复用当前 1.54 英寸 `partitions.csv`。
完整 16 MB 当前 Flash 刷写前只读备份已完成：
`artifacts/note4-preflash-full-flash-16m.bin`（16,777,216 B，SHA256
`366dea39643855fd5250d15bb8f23da3b363eca1705a0068b9ab5a598e01d110`）。部分 stub
读取区间断流后改用 ROM-only 小块读取恢复；最终长度与哈希已核验。本轮未擦写、OTA 或改分区。

已查到 ZECTRIX NOTE4 DevKit V1.0 官方 pin map 与公开 SSD2683 驱动/waveform source；
需目视核对本机 PCB 版本/面板排线后才能按该参考接线。无串口启动日志、屏幕/按键/睡眠
实测或照片。本轮 esptool 触发过瞬时 reset/download mode，成功分区读取后请求 hard reset；
COM5 仍枚举。详细证据和下一步见
`project-workflow/generic-display-platform-implementation/status.md` 的 USB evaluation 节。
字体检查发现 assets 分区几乎全空、无法按 LittleFS/SPIFFS 挂载；当前固件中的
`LvglBuiltInFont` 表明字形编译进了应用。已下载对应的 XiaoZhi 字体组件 v2.0.0；
30_4 common CBIN 为 2,609,092 B，占 4 MiB assets 分区 62.2%，源码/体积见评估记录。

## 实机推进：2026-09-23 — 桥提速定稿 + bridge_first spike + 0.17.9 固件（已提交 99ef3c0）

已提交 **`99ef3c0`**（上一基线 `e1e93fc`）。权威细节与证据见
`project-workflow/power-plan-c/status.md`（09-23 节）与 `task-6-bridge-first-impl.md`
（含 §1 spike 结果、§5 统一 A/B 排期）。

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
  `bridge/target/debug`（parent 2032 / watchdog 50320，pidfile=2032，本地重建件）。
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
