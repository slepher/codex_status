# 状态

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
