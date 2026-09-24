# Task 2：v2 HTTP 目标 MAC 核验

状态：2026-09-24 已完成并通过宿主验证。此任务是多设备路由的写入安全前置。只改 `bridge/`；另一代理正在改 ROM，严禁编辑 `src/`、固件构建文件或其任务目录。

## 已定接口与行为

1. `bridge-core::v2_client` 的 `data`、`plan`、`activate`、`install_bundle` 增加必填 `expected_mac: &str`。内部 `session_nonce` 从经过 endpoint token 认证的 `/v2/status` 取得 `device_mac`，把它和 expected MAC 规范化后比较；缺失、无效或不一致时返回带目标 MAC 信息的错误，且**不得发送后续 POST**。不要改设备协议或 status JSON 形状。用现有 `DeviceIdentity::normalized_mac`，不写第二套 MAC 解析器。
2. `bridge-app/src/platform.rs` 的所有 v2 写入调用都传对应 `DeviceLink.mac`。`send_plan(ctx, mac)`、`request_light(ctx, mac)`、`refresh_status(ctx, mac)` 必须由现有 `device_link_for_mac` 解析目标端点，不能借全局当前设备的 IP。`post_ota_window` 和 MCP 旧适配路径仍以当前已选 MAC 为参数，但实际写入同样传目标 MAC。
3. `refresh_status` 在把读取结果写入 `PlatformService` 前核对返回的 `device_mac` 与请求 MAC；错配时返回明确错误，不更新该设备状态。`request_light` 在每设备在线缓存尚未实现前，仅对“请求 MAC 等于当前选中 MAC 且当前缓存在线”的目标尝试即时 HTTP；其他已登记目标保持已冻结的待会合计划，不借用当前设备在线标志。
4. 正常单设备路径行为与 ACK 语义不变。现有 HTTP fake 测试中的正确 MAC 请求仍成功；新增错误 MAC 测试必须证明只发生 GET `/v2/status`，没有 `/v2/data`、`/v2/plan`、`/v2/activate` 或 Bundle POST。不要启动真实 Bridge、访问硬件或修改生产数据。

## 文件所有权与验证

编码代理独占 `bridge/crates/core/src/v2_client.rs`、`bridge/crates/core/tests/v2_client.rs`、`bridge/crates/app/src/platform.rs`。编译若需要其他 bridge 文件，先报告精确调用点，待主代理授权。不得编辑工作流文档、`PROGRESS.md` 或任何 ROM 文件。使用独立 `CARGO_TARGET_DIR` 运行 `cargo test -p bridge-core` 与 `cargo check -p bridge-app`，再运行 `git diff --check`。输出改动、错误 MAC 时实际 HTTP 方法/路径证据、命令退出码及剩余全局目标调用点。

## 结果

6-luna high 仅改上述三文件。`v2_client` 的四个写入入口统一采用 `ip, token, expected_mac, ...` 参数顺序；认证 `/v2/status` 的 MAC 与期望不符即停止。`send_plan`、`request_light`、`refresh_status` 等按目标 MAC 取设备记录中的端点；读回状态错 MAC 不写入平台缓存。错误 MAC 测试连续调用四条写入路径，fake server 仅收到四次 `GET /v2/status`，零 POST。

隔离 `CARGO_TARGET_DIR=bridge/artifacts/v2-target-mac-guard`：`cargo test -p bridge-core` 105 项通过、`cargo check -p bridge-app` 通过；`git diff --check` 通过。无实机或生产服务操作。当前 Bridge 仍只有单设备周期调度及全局操作 token 缓存；此项不宣称多设备完成。
