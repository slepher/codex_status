# 审核边界

传输层不得重发未知修改，错误须保留 NotSent/WriteUnknown/AckUnknown。成功必须核对 MAC/协议/事务 ID；最终运行集合核验属于后续事务协调器。保持旧 OTA/配对/Wi-Fi 入口，不创建业务 owner。

回执文件由单一 application worker 拥有；首次 create 必须完成后才允许修改设备。任何落盘错误均停止修改，重新加载文件并查询原事务，不能凭内存状态重发。32 KiB 读取上限，Manifest base64 最大 4780 字符；初始失败可留下损坏文件，后续必须明确报错，不覆盖重新生成事务。

Rust/C++ 交叉测试复用 Python 中继，仅转发 HTTP 字节及注入断线，不替 Rust 构造请求或确认成功。Rust 自行持久化和重载回执；Python 另核对事务唯一性、commit 次数、单更 B 未上传及回退运行列表。CPU 重启/健康注入不替代 RV32 或硬件证据。

应用入口当前为显式单步 API，不声明后台 worker 或 UI 完成。请求取消/进程退出后须重新 inspect 并推进原事务；终态归档已接，显式跨地址恢复及尚未确认 begin 的取消已接，自动发现/worker 与 UI 调度需继续实现。新增 MCP 工具不触发现有设备自动更新。

归档仅释放本机活动回执，不写设备；RecoveryRequired、Unknown、未结束清理不得归档。终态查询仍须原设备码/MAC/事务/Manifest 围栏，旧回执保留为 .completed 文件。归档持久副本后删除活动文件失败会继续阻止 prepare，避免静默覆盖。

地址变化不能仅替换字符串后重发：reconnect 先认证原事务响应，地址与类型化进度原子落盘。无确认进度的 404 仅结合认证 status 后允许迁移或本地 abandonment；若曾确认任何进度，则 404 仍按回执过期拒绝，不能生成新 ID。响应未知和落盘失败均保留原操作，客户端需重载回执。

2026-11-04: The earlier single-step-only boundary is superseded by the bounded worker above. Cancellation markers are scoped to MAC and operation ID, contain no credential, and are retained as history. Workers never create a replacement transaction or automatically archive one. UI and network-change discovery validation remain incomplete.

2026-11-04 UI：生产 HTML 已接 Tauri 共用入口，Node/WK 模拟 IPC 不替代真实 Rust 调用。暂存/校验后必须显式 run；成功只显示 confirmed，回退与 Unknown 分别显示。stage 包头检查不替代完整 Manifest/APX2 准入。

UI/Rust/C++ 同次链已通过，但 Node DOM 与带令牌的回环 RPC 适配不能证明原生 Tauri IPC 权限/窗口生命周期。设备登记门和操作锁由既有应用单测覆盖，测试 RPC 固定回环端点，不当作真实设备发现证据。

暂存清理只匹配 ap01-artifact-<64位小写摘要> 及其 .tmp；先扫描所有活动回执再删除过期缓存，不读取回执内容来判断可删。128 MiB 是逻辑字节上限，不等于磁盘物理占用预算。清理按暂存请求触发，不后台启动任务；保留窗口内不足空间会明确拒绝，待过期或完成原事务后重试。5 项应用回归通过，新增覆盖损坏回执固定缓存、超限、过期临时文件、复用刷新、原始文件保留、时钟倒退、摘要冲突及符号链接拒绝。

2026-11-05 UI/Rust 与实际 RV32 同次闭环通过：生产页面经测试 IPC、Rust worker、回环 TCP 和 C++ HTTP，完成首装失败空回退、同包重新安装、A 单更、A+B 批更和整集回退。QEMU 仅运行于 james.local Docker；真实 App 双基址执行，健康不由宿主注入，20 次原事务查询等待，commit 每事务一次，boot=13/14/15/16/18，终态清理完成。页面重开恢复、结果标签、查询/归档均通过；报告位于兄弟固件仓库 `build/app-install/set-rv32-ui-v3-report-01.json`，SHA `26d67fcd842758202b07668a861420a9a82314c2463c011649c2f5ffe9fe2982`。原生 Tauri IPC/文件对话框、网络变化自动发现仍待验；未启停生产 Bridge、未连接实机。

本轮继续使用空设备登记的测试 AppCtx，自动重连在发现网络前返回；这验证重启等待后的原事务恢复，不证明地址变化发现。DOM/IPC 仍由 Node/回环 RPC 适配；ROM 服务、存储与复位仍是模型，不能当作原生 Tauri 或硬件验收。
