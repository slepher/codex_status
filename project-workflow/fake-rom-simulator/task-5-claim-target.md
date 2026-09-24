# Task 5：`/claim` 按目标 MAC 选择端点并预检身份

日期：2026-09-24。状态：完成宿主验证，未提交。

## 决策与合同

Task 3 已把操作 token 按 MAC 隔离，但 `post_claim_once` 仍读取 `ctx.device_ip`，可能将 A 的 token 送往 B 的地址。此任务只修这一条路由，不改变 owner/lease 语义，不接多设备周期调度。

`post_claim_for_mac` 收到规范化目标 MAC 后，从 `PlatformService::device_get(mac)` 的 `ip` 获取该目标端点。仅当该记录无可用 IP 且目标正是界面当前已确认的 MAC 时，回退 `ctx.device_ip`。其他情况失败，不借用界面当前 IP。调用 `post_claim_once` 时显式传入端点和目标 MAC。

`post_claim_once` 在 POST 前经 `bridge_core::device::fetch(endpoint, ...)` 读取目标状态，取 `MAC` 字段并与目标规范化 MAC 比较；缺失、无效、错配或无法读取均拒绝，不发送 token 与 POST。不要在错配时更新 owner、claim 时间或设备记录。状态预检是有限真实 I/O 超时；后续时钟阶段再区分逻辑时间。已有 token 401 处理、显式 BLE 重取、lease 参数和 release/force 行为不变。沿用现有 selected-device 的缓存更新；逐 MAC 缓存属于后续任务。

## 编码所有权与验收

- 6-luna high 只修改 `bridge/crates/app/src/main.rs`，不修改 ROM、core、MCP、BLE、前端或任何计划文件；不作设计决策。
- 新增本地 HTTP 测试：服务端返回不同 MAC 的状态时，确认没有收到 `/claim` POST；匹配 MAC 时允许 POST。若 main.rs 测试环境不适合网络测试，至少提取纯身份判断并测试匹配、缺失和错配，同时汇报限制。
- 使用隔离 `CARGO_TARGET_DIR` 运行 `cargo check -p bridge-app` 以及适用的定向测试，执行 `git diff --check`。不提交、不访问实机、不碰运行中的 Bridge。

## 结果

`post_claim_for_mac` 先解析服务内该 MAC 的端点，仅在当前选中 MAC 相同且服务端点缺失时回退界面地址。`post_claim_once` 在发送 token 前读取状态并验证 MAC。本地 HTTP 测试覆盖错 MAC 仅 GET、匹配后才 POST 两条路径。隔离 `cargo check -p bridge-app` 与 `cargo test -p bridge-app claim_target_tests`（2 项）通过，`git diff --check` 通过。未访问实机或运行中的 Bridge。
