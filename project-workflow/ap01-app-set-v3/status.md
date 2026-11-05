# 2026-11-05 App OTA 迁回 firmware

2026-11-05 实机版本部署：AP01 当前运行槽 1，CUKTECH ROM `0.1.0-cuktech` / 基线 `1.0.2_0041`，build `677bc30246d7ae8f7cd73698fe4086d6`；槽 0 改为同版本无载入器救援基座，后续更新保留。业务 App 从本仓源码构建 `0.18.108-ap01`，205360 B，SHA256 `feab260c87fc293d7df426ecddfcc266c7d4e120019d1a8f7bdd6e4a88370cc4`。集合序号 2，重启后 succeeded/healthy、cleanup_pending=false；诊断版本列表与真实运行列表一致。执行载荷与此前 d1.0 相同，仅版本元数据/摘要更新。App 版本声明现归 `ports/ap01/build-0041.json`，固件构建器读取配置或显式 --version，不再内置业务版本默认值；基座版本独立发布。

槽 1 ROM 6935752 B，SHA256 `cf0f39c78129b571d6d6534daac02fd7f0fe86de9e0cb7cc3c5fadbdf117a4e6`；槽 0 ROM 6885128 B，SHA256 `39b4d342b0d2de99852f2c5416ce3bbd74d386cd9b94f751ce1258c576ce3c5d`。两者在兄弟固件 `build/version-protocol-20261105/`；App 在 `build/app-upgrade-018108-20261105/app/`。槽 0 未写入，头/构建标识/尾部抽样一致；Wi-Fi 3 个网络、revision 7 和当前网络在两槽一致。非整槽读回，不扩大为界面/旋钮或长期运行验收。App 客户端遇到不含 MAC 的 409 maintenance_busy 时中止，保留原事务恢复后确认成功；错误分类待固件仓库修复。私有验收：兄弟固件 `build/app-upgrade-018108-20261105/deployment/acceptance.json`；版本协议与部署记录见[固件状态](../../../cuktech-ap01-firmware/project-workflow/ap01-app-update/status.md)。本次未启停 Bridge、修改业务协议或重新发布 Bundle。

按用户要求移除本仓 App OTA 专用 Rust 模块、Bridge UI/worker、Tauri/MCP 命令及专用测试；firmware 已有多包 CLI 与生产 HTTP 交叉验收，作为维护入口。五个原始 OTA-only 源码补丁保存在 firmware/docs/history/app-ota-bridge，可追溯但不再作为本仓构建依赖。业务 Bundle/C2/BLE、配对和 ABI 3.1 App 源码保留；未删除任何本机运行数据或旧回执，未启停 Bridge。迁移验收：边界守卫、Node 页面及 WK80 场景通过（窄/宽截图审阅）；Rust core/app/mcp 347 passed、5 既有条件忽略。Fake BLE 初次被系统代理返回 503，测试进程绕过 loopback 代理后通过，业务协议未改。firmware CLI 五事务回环和 AUT3 103164 检查通过。11 个业务/适配关键文件字节保持，记录在 firmware/build/commit-check/ota-boundary-before.json。未操作本机 Bridge 数据或设备。

以下均为历史证据，不再要求本仓完成 App OTA 产品化。

# 状态

2026-11-05 业务可写恢复接入 ABI 3.1 写作用域：Busy 不执行 begin、不锁存失败，解除后恢复一次；真实 I/O 失败仍保持失败。bootstrap 固定只读，新运行库拒绝无扩展平台。真实 Bundle 宿主回归及 james.local Docker 实际 worker 双基址通过，固件仓库报告 `build/scope-resume/qemu-final.json` SHA256 `4cce51dbcce47944cd7f9ae804b9f86d671b301e218e7ecbc3ec92f53733dd40`。Bundle/激活/配对的完整操作准入和 UI 配对保存迁至 worker 仍待完成；未部署、未操作实机。

2026-11-05 UI/Rust 与实际 RV32 同次闭环通过：生产页面经测试 IPC、Rust worker、回环 TCP 和 C++ HTTP，完成首装失败空回退、同包重新安装、A 单更、A+B 批更和整集回退。QEMU 仅运行于 james.local Docker；真实 App 双基址执行，健康不由宿主注入，20 次原事务查询等待，commit 每事务一次，boot=13/14/15/16/18，终态清理完成。页面重开恢复、结果标签、查询/归档均通过；报告位于兄弟固件仓库 `build/app-install/set-rv32-ui-v3-report-01.json`，SHA `26d67fcd842758202b07668a861420a9a82314c2463c011649c2f5ffe9fe2982`。原生 Tauri IPC/文件对话框、网络变化自动发现仍待验；未启停生产 Bridge、未连接实机。

2026-11-04 暂存清理：stage 新增 128 MiB 逻辑容量限制；无活动回执时清理超过 24 小时的精确命名缓存/.tmp，复用刷新时间。有活动或损坏回执则保留全部缓存；超限拒绝新增，不触碰原始选择文件。5 项应用测试、职责边界检查及 UI→Rust→C++ 真实双包四链复验通过（首装/单更/批更/整集回退，测试 IPC、模型健康/重启）。原生 IPC、自动网络变化及固件资源/孤儿回收/RV32 全链仍开放，未部署或操作实机。


2026-11-04：core 复用 bounded transport 新增 AP01 v3 请求适配，校验菜单码、MAC/事务、块摘要/边界和成功响应身份；未知保留 dispatch 分类，不重发。隔离 target 下 cargo test --offline -p bridge-core device_client:: --lib，12 测试通过（含路由与真实回环 TCP 的身份错误/截断响应）。application service、AMF3/APX2 校验、持久回执协调和 UI 未接。未启停/部署 Bridge，未连接实机。

2026-11-04 产物校验：`core::ap01_app_set` 解析 AMF3 与资源汇总、规范 ABI 3 APX2 和 BASE32，按变更位图选择已校验包；使用真实业务包/peer 执行 2 项测试（含全部 AMF3 截断、缺包/重复、重算摘要后的非法结构）通过。命令：设置 AP01_MANIFEST/AP01_PACKAGE/AP01_PEER 到固件仓库实际产物后，cargo test --offline -p bridge-core ap01_app_set:: --lib -- --include-ignored。未接持久协调/application service/UI；总资源能力匹配仍待实现。

2026-11-04 能力匹配：Manifest::check_capabilities 核对设备 app_max、包/对齐镜像/堆/栈/任务/页面、服务位图与 RAM 扣驻留预留。真实双包恰好边界、缺字段、超限、预留反转测试通过（同上隔离 Rust 命令）。固件已发布对应 limits，Python CLI/C++ TCP 模型交叉通过；不是硬件容量实测。持久回执协调与 UI 仍待实现。

2026-11-04 持久回执：新增 Receipt 类型，首次独占创建/fsync，后续复用平台 atomic_write；保存原设备/端点/事务 ID、Manifest 原字节和旧集合围栏，不保存设备码或任意响应。恢复读限 32 KiB，严格字段；进度核对身份、单调序号/偏移/Ready、终态与文件一致性，写入失败不推进内存。3 项定向测试（含真实集合）通过；尚未接异步上传/重连协调器和 UI，不宣称 Bridge 端到端安装完成。

2026-11-04 单步异步协调：新增 advance，先重载回执并查询原事务，每次最多发送一次修改；按已确认偏移选择变更包、Ready、commit 或显式 cancel。落盘失败保留 AckUnknown，Succeeded 还需新 boot 和完整健康运行集合一致。7 项集合定向测试（含真实双包、commit 回复丢失后恢复且不重复 commit 的 TCP 测试）通过。尚未接 application worker/UI，也未完成 Rust 与 C++ 服务交叉验收。

2026-11-04 Rust/C++ 交叉验收：复用固件 TCP 中继，Rust core 真实传输/回执/advance 对接生产 C++ HTTP、AUT3 选择与清理，使用真实业务包和 peer 连续首装、A 单更、A+B 批更、试运行失败整集回退。四次分别丢 begin/chunk/commit/ready 回复，均从原回执查询恢复且每事务只发送一次 commit；单更不上传 B，回退全部运行身份等于旧集合，boot_id 12/13/14/16。报告：兄弟固件 build/app-install/set-rust-tcp-v3-report.json。只回环 TCP；健康注入、CPU 重启模拟、内存 VFS，不执行 App 指令。application worker/service/UI 未接。

复现：在固件仓库设置 PATH 包含 cargo、CARGO_TARGET_DIR 为已预建隔离目录，运行 `python3 -B tooling/scripts/test-app-set-client-http-v3.py --tcp --rust-bridge ../codex_status/bridge --binary build/app-install/test-set-journal-v3 --manifest build/app-install/d2-resume-set/set.amf3 --package build/app-install/d2-business-resume-01/app.apx2 --package build/app-install/d2-peer-beats-02/app.apx2 --report build/app-install/set-rust-tcp-v3-report.json`。单独跑全部产物单测的 `--include-ignored` 命令需追加 `--skip production_cpp_http_collection`；交叉测试由上述中继设置独立临时路径/环境后以 `--ignored --exact` 调用。

2026-11-04 应用入口：新增 ap01_app_update MCP 工具与 platform_ap01_app_update Tauri 命令，共用 AP01 操作锁和登记校验。prepare 先校验 Manifest/维护能力，再验证提供包并独占持久回执，不发送设备修改；inspect 只读原回执，advance/cancel 必须携带相同 operation_id，每次最多推进一次。每个设备当前只保留一个原回执，存在时拒绝 prepare；终态归档/再次更新入口、地址变化后的认证迁移、自动 worker 和 UI 仍待实现。文件读取限普通文件、Manifest 3584 B、提供包合计 64 MiB（桌面输入上限，不代表设备容量）；设备码不落盘。隔离 bridge-app 编译及回环准备/推进测试通过，未运行或重启托盘应用。

验证补充：`cargo test --offline -p bridge-app app_update` 3 项通过；`cargo test --offline -p bridge-mcp --lib` 3 项通过，使用 `/private/tmp/codex-ap01-bridge-v3-target`，未触及运行实例/data。

2026-11-04 回执生命周期：inspect 离线加载回执，不依赖当前 IP；query 只查询原事务，404 不发 begin。archive 先查询确认 terminal/cleanup=false；Succeeded 还需新 boot 与全体运行身份通过，再独占写入并同步历史副本、校验活动回执未变、删除活动文件并同步父目录，允许下一次 prepare。归档副本冲突拒绝且保留活动文件；复制完成后中断可重试。core 8 项真实产物/回执/协调回归、app 3 项通过；应用回环覆盖 receiving 拒绝（无额外 POST）、cleanup pending/旧 boot 拒绝、成功归档及下一事务准备。查询 404 测试夹具最初错误返回 JSON null 被传输层正确拒绝，修成对象后通过。当前地址变化迁移、prepare 后尚未 begin 的取消、worker/UI 仍待实现；不声明完整恢复生命周期完成。

2026-11-04 地址恢复/未确认取消：reconnect 只查询已登记新地址的原事务；匹配 MAC/事务/旧与候选 Manifest/单调进度后，将地址与进度一起原子保存。若无确认进度且结果 404，另外认证 status 的 MAC/协议/非零 boot 后保存新地址，原 operation/Manifest/original_boot 不变；不发送 begin。cancel 对无确认进度的 404 同样认证设备，独占同步 .abandoned 副本后移除活动回执，返回 abandoned/local_only，不伪造 Cancelled；已确认 404 或 503 保留回执。core 10 项通过：12 次全 GET TCP 矩阵含错误 MAC/Manifest、已确认过期、prepare 后变地址、503 与本地写失败；app 3 项通过，含新地址 prepare/cancel 无额外 POST。MCP 定义增加 reconnect；自动 worker、自动发现重连策略与 UI 仍未接，未部署或访问实机。

2026-11-04 Background worker: run explicitly resumes the durable receipt (default 180 s, maximum 3600 s, individual HTTP calls capped at 5 s). The task outlives the UI request, holds the AP01 operation lock per step, and retains credentials and validated package bytes only in memory. Cancellation intent is saved before cancellation is requested; both manual advance and restarted workers honor it. Transient failures back off and query the same operation; identity/protocol/persistence failures stop. A distinct run_id prevents an old task's exit guard from changing a newer task. Existing authenticated discovery is invoked after three temporary failures, with a bounded wait; changed endpoints still require v3 receipt identity validation.

Validation: 4 app_update tests passed, including duplicate run, durable cancellation after application-state recreation, and bounded unknown-result exit. The worker also passed all four real-package C++ TCP flows (first install, A-only, A+B, whole-set rollback), recovering automatically from lost begin/chunk/commit/ready responses with one commit per transaction. Report: sibling firmware build/app-install/set-worker-tcp-v3-report.json. Reproduce with the documented cross-test command plus --rust-worker and that report path. Health and CPU reboot remain injected models; no device or production Bridge access. UI and automatic discovery across network changes remain open.

2026-11-04 UI：AP01 设备页加入清单/多包选择、设备码、校验后显式开始、逐包进度/复用、取消、查询、地址恢复和归档。文件按内容 SHA256 暂存；清单先解析，包暂存检查头，prepare 再完整校验。单包 16 MiB、UI 合计 64 MiB；桌面暂存清理仍待实现。普通页面轮询仅本地 IPC，不联系设备；切设备清设备码并拒绝旧响应，归档后不复活旧任务。

Node 设备页通过；原生 WKWebView 88 个窄/宽场景通过，已审阅 560-app-update-upload.png 和 1000-app-update-ready.png，无横向溢出。Rust app_update 4 通过/1 外部条件忽略，含暂存字节读回/坏包头拒绝；跨仓边界检查通过。截图在 artifacts/bridge-macos/wifi-ui。UI IPC 仍为模拟，UI→Rust/C++ 同次全链尚未验证；未启停生产 Bridge 或访问设备。

2026-11-04 UI/Rust/C++ 同次链：生产 index.html 的 appUpdate 函数读取真实双包，测试 IPC 适配器调用真实 Rust stage/prepare/worker/query/archive，再连接生产 C++ HTTP 模型。首装、A 单更、A+B 批更、失败整集回退四次通过；每条注入一次丢回复、页面重开后恢复原 operation、最终查询并归档，UI 不将回退显示成功，归档后不复活旧任务。设备侧 commit 各一次，B 单更未上传且身份不变，回退运行列表与旧集合一致，boot_id 12/13/14/16。

报告：兄弟固件 build/app-install/set-ui-rust-tcp-v3-report.json；复现用已有 TCP 交叉命令加 --rust-ui（替代 --rust-worker），报告改为该路径。测试入口 tools/test-ap01-app-ui-rpc.mjs 与 Rust ignored production_ui_rust_cpp_collection；隔离临时数据、回环随机端口和随机测试令牌。只替换 DOM/Tauri IPC 传输及固定测试端点，Rust/C++ 响应真实执行；尚未验证原生 Tauri IPC/桌面文件选择对话框，健康/CPU 重启仍为模型。生产 Bridge 与设备未访问。

### 2026-11-05 分批提交边界

HTTP 变更请求已接整请求 writeScope，准入早于 session/replay 更新；当前只读恢复宿主回归和跨仓库边界检查通过。认证 C2 Busy 后原请求重试、提交回执重放尚待专项验证，既有 RV32 包证据不覆盖此次 HTTP 接入；本地激活及配对确认写入仍待完成。未部署或操作实机。

2026-11-05 更新：固件仓库真实 C2/Bundle + AppFilesV3 宿主专项已补齐 claim 与 Bundle begin/chunk/commit 的 Busy 原请求重试及无额外写入的成功回执重放；status 在 Busy 时仍成功。ASan/UBSan 通过，证据见兄弟仓库 `build/commit-check/readonly-direct-report.json`。未覆盖 RV32 HTTP、HTTP 激活、本地激活与 UI 配对确认迁移。

2026-11-05 激活推进：localActivate 已在完整 writeScope 内执行，Busy 返回 retry，原生 worker 重排且不覆盖更新请求。两模板真实业务宿主测试通过远程激活 Busy 原请求重试/回执重放、本地 Busy 不写盘后切页及存储失败不重试。原生 UI 队列并发未执行验证。新 App 包 204928 B，SHA256 `94cf12d85e5cb59609b93c34df8388816806d829014cadc07336102191e4bdf7`，固件仓库 `build/app-install/d2-business-activation-scope-01/app.apx2`；零外部导入、双基址归一化一致，尚未执行 QEMU/部署。配对确认线程迁移待完成。

2026-11-05 配对确认：ABI 3 UI 只保存有效松键 event/revision，worker 在 writeScope 内落盘；Busy 保留，取消/替换/到期作废。宿主 ASan/UBSan 已验证 UI 零写入、Busy 后一次提交、旧确认不授权新请求、过期无写入。新包 `d2-business-pair-worker-01/app.apx2` 205360 B，SHA256 `27bb07332aff6eef0726f48ea09c3638eadee8df2be23b652324f6fed0e0b2cb`，交叉编译零导入/双基址一致；未执行新包 QEMU。原生并发/RV32 HTTP/配对/激活链待验，不代表硬件准入。

2026-11-05 RV32 回归：`d2-business-pair-worker-01` 新包与 peer 在 james.local 既有 Docker QEMU 中双基址执行通过，覆盖生产加载、集合健康、Busy 后恢复与 worker 停滞检测。固件仓库报告 `build/pair-worker/qemu.json` 的输入哈希已与本地逐项核对。guest 未发送配对/业务 HTTP，请求路径及原生并发仍待验收；不代表完整 ROM 启动或硬件准入。

2026-11-05 RV32 HTTP：固件集合 guest 已通过模型网络向真实 worker 发送分段 C2 HTTP，双基址验证 Trial 写入拒绝、未认证拒绝及系统 Busy 生命周期拒绝；报告 `build/pair-worker/http-wire-qemu.json`。仅 writeScope 竞争时 status 可读，系统 busy 时仍为 business_offline。认证成功/绑定与 Bundle 持久写入、原厂 socket、抢占并发仍未覆盖。

2026-11-05 RV32 配对成功链：真实 resident bridge → App 配对状态机 → 模型绘制/真实 App 按键处理 → Busy 延迟 → worker APB1 验证保存/一次 rename → 令牌认证 HTTP 200，Docker QEMU 双基址通过。报告为固件仓库 `build/pair-worker/pair-http-qemu.json`；模型内存绑定文件不证明硬件持久性/掉电，配对入口未经过维护 HTTP parser，Bundle 成功链及原厂调度仍待验。

2026-11-05 配对并发：真实 Application/Binding 与生产 AppFilesV3 的宿主 std::thread 在保存 write 处暂停，UI、重复 worker、principal 查询和取消均被绑定锁排除；释放后一次提交。ASan/UBSan 和 ThreadSanitizer 通过，报告在固件仓库 `build/commit-check/readonly-direct-tsan-report.json`。不替代原厂 RTOS/设备 I/O 时序。

2026-11-05 Bundle RV32 准备：固件模型 VFS 已支持安装所需独立读/追加游标、seek、rename/remove、容量边界；宿主专项与多 App 回归通过，已有配对/认证 guest 在扩展 VFS 上双基址回归通过。尚未执行 Bundle 安装请求序列，成功链仍待接入。

2026-11-05 Bundle RV32 成功链：真实 worker 经 C2 HTTP 完成 claim/begin/chunk/commit，最小单模板 Bundle 编译/存储/读回后 configured=true；相同 commit 重放回执一致且无新增写入/rename。Docker QEMU 双基址通过，报告 `build/pair-worker/bundle-http-qemu.json`。修正测试 guest bump heap 超出映射问题，复用既有有界 arena 作 ROM malloc 替身；真实 App 包/ABI 3 分配接口未改。完整业务模板/字体、重启恢复、实际显示和硬件持久性仍待验。

2026-11-05 完整模板：仓库 AP01 codex-status-a 原始 elements 未裁剪，6496B Bundle 分 7 块经真实 RV32 worker 上传/编译/提交，逐块和 commit 同帧重放无重复写入，状态 configured=true；Docker QEMU 双基址通过。固件报告 `build/pair-worker/full-template-qemu.json`，源模板及生成头哈希已核对。未含外置字体、未验证实际显示或重启恢复。

2026-11-05 恢复证据：boot42 真实安装文件直接导出后交给全新 boot43 guest，双基址通过绑定令牌认证、只读期 configured=true/零写入、健康后可写恢复。绑定 86B 与正文/缓存 17850B 保持逐字节一致，状态 420B 更新。报告 `build/pair-worker/business-resume-qemu.json`；这是独立启动文件交接，非同 VM reset/硬件掉电。

2026-11-05 同 VM 恢复：完整模板安装/恢复两阶段在同一 QEMU 进程内通过 QMP reset 串联，双基址 PC 均回到 0xa0000000、业务文件交接完全一致，两阶段返回 0。新版运行器同时回归旧 fail-trial→rollback 日志链通过。报告位于固件仓库 `build/pair-worker/business-reset-qemu.json` 与 `journal-regression-qemu.json`。runner 仍重载 guest/注入模型文件，不代表原厂 ROM 复位引导或 flash 掉电。
