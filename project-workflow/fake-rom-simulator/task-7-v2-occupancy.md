# Task 7：v2 逐 MAC owner/claim 运行状态

日期：2026-09-24。状态：完成宿主验证，未提交。

## 设计决策

Task 6 的 `v2_status_cache` 扩展为每个 MAC 的运行状态：认证状态在线/时间/上次 JSON，加上 owner、owner_observed_at、last_claim_at、yielded。`update_v2_status_cache` 更新成功状态时同步取 `owner`（null 表示空闲）；离线更新保留诊断状态和 owner，但明确标离线。claim 结果只更新该 MAC 的 owner/claim 时间，不改变另一 MAC 或 legacy 全局缓存。所有键先用 `DeviceIdentity::normalized_mac` 规范化。

新增 `v2_occupancy_gate(ctx, mac)`，暂不接周期循环。它只允许最近 30 秒认证状态在线的目标参与自动 claim；离线或状态过期返回失败，不发送 POST。yielded 返回 Yielded。有效他人 owner 返回 Other；自有 owner 仅在距上次成功 claim 满 60 秒时续约；空闲/到期 owner 尝试非 force claim。claim 用 Task 5 的目标端点和 MAC 预检。401、网络失败、404 都返回失败，不将 v2 设备视为 legacy Unsupported，也不得旁路投递。409 记录该 MAC 的他人 owner。成功更新该 MAC 的 owner、owner_observed_at、last_claim_at。正式 PowerPlan 的截止不因状态读取/claim 变化。

现有 legacy `occupancy_gate`、`device_cache`、全局 `owner_cache` 与 `yielded` 保持原行为。将 claim HTTP 响应解析拆成不写全局状态的纯解析函数；legacy 当前设备调用成功时才更新全局 `last_claim_at`/activity。MCP 当前设备的显式 claim/release 同时更新该 MAC 的 v2 owner/yielded 状态，以免下一步周期循环重新 claim 用户刚 release 的设备。禁止把界面当前 IP 当成另一 MAC 的地址。

## 编码所有权与验收

- 6-luna high 只修改 `bridge/crates/app/src/main.rs`；不修改 ROM、core、BLE、MCP、UI 或计划文件，不自行设计下一步周期调度。
- 添加纯状态决策测试：两个 MAC 分别 owner/yielded/last_claim；A 离线、他人占用、续约时，B 状态不变。保留 Task 5/6 回归测试。
- 隔离 `CARGO_TARGET_DIR` 验证 `cargo check -p bridge-app`、定向测试、`git diff --check`。不接设备、不运行 Bridge、不提交。

## 结果与评审

6-luna high 完成逐 MAC owner/claim 状态及纯决策函数，v2 gate 尚未接周期循环。主代理发现 409 错误更新 `last_claim_at`，已要求修正：冲突只更新 owner 与观察时间，续约时间保持原值。隔离 `cargo check -p bridge-app`、v2 状态测试 2 项、claim 回归测试 2 项及 `git diff --check` 通过。无关的 4 个 Bridge 文件纯格式变动已撤回。未访问设备、未提交。
