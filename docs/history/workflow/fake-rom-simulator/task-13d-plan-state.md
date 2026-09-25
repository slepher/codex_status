# Task 13d — 未配置模拟设备的 PowerPlan 协议状态

状态：宿主验证完成，待提交。基线 `e591951`。

## 决策

先让未配置 Bundle 的模拟设备处理 `/v2/plan`，复用 ROM 的共享解析、session、Plan 决策和 ACK。真实固件仅在 `v2BundleReady` 时运行 light deadline 进入 deep 的循环；模拟器当前仍未配置 Bundle，因此本任务只对齐此阶段的协议状态，不宣称已模拟无线休眠/会合。Data/Bundle/Activate 仍 501；`/status.json` 仍不提供。

- 使用现有 `bridge-render` C ABI：`codex_v2_command_parse`、`codex_v2_command_check`、`codex_v2_plan_new/decide/free`、`codex_v2_build_ack`。增加窄 Rust 安全封装，在模拟器中持有每进程独立的 C++ `V2PlanState`；不可把 Plan ID/重放/上限在 Rust 再实现。Rust 可保存供快照重建的接受模式与首次接受时刻。宿主 render 编译的默认 ROM target 是 `codex-status-154g`；本切片 ACK 的 `fw_target` 与该构建一致，不假称 Note4。多 target 留待 Bundle/显示接入。
- HTTP 顺序与 `main.cpp` 一致：endpoint bearer 401 → JSON 解析失败时 200 command ACK `json` → 当前 owner 检查（无 owner 允许；匹配 `bridge_id` 允许并只在 RAM 刷新 last_seen；不匹配 409 occupied，且不推进 Plan）→ 共享 session 检查 MAC/nonce/request，失败时 200 command ACK `session` → 共享 Plan 决策 → 200 plan ACK。控制 token 与 device token 不能通过业务鉴权。所有判定在一个逻辑时钟采样下进行；同 ID 相同内容的重放保留原 deadline；旧 ID/冲突/坏形状不改状态。
- `v2BuildAck` 返回设备同形状 `op/result/display_state/retention/error/plan_id/accepted_remaining_s/fw_target`，无 context 时不加 `active_context_id`。JSON/session 预检失败使用 `op=command`；owner 冲突 HTTP 409 含 `result=rejected`、`error=occupied` 和当前 owner。Plan ACK 的 `accepted_remaining_s` 必须传共享决策的 `grantedS`，其名称虽叫 remaining，但重放 ACK 仍是原授予秒数，拒绝 ACK 为 0；真正递减的 remaining 只在 status 中。`/v2/status` 的 `power.plan_id/remaining_s/granted_s` 由该真实 C++ Plan 状态生成（通过现有 status builder；传入首次接受时刻、接受模式、授予秒数），不允许 Rust 自造剩余时间。`/sim/state` 可加 Plan 摘要，但不泄 token、请求原文。
- 当前 cold boot 保持已有 `provisional=false`、`boot_ms=0`，因为此进程没有设备物理按键唤醒事件；不能凭启动模拟出 BOOT 300 秒特例。owner 的有效性使用设备逻辑 uptime；wall 跳变不影响 Plan deadline。当前没有 Bundle，因此有效 Plan 不改变 `deep_sleep`；能力加 `plan_state`，从 unsupported 移除 `plan`，另标 `power_lifecycle` unsupported。若将来 Bundle 安装后需要 sleep/rendezvous，须先接共享 ROM 生命周期并扩大能力，不沿用本切片的“永远可达”行为。
- 不修改 `src/`、Bridge app/core、其他 worktree、实机或生产服务；不提交。只改 `bridge/crates/render/src/lib.rs` 的窄封装、`bridge/crates/device-sim/` 和直接 Cargo 依赖，已有 C ABI 不足时先报告主代理决策。

## 验收

- loopback 测试真实调用 `/v2/status` 拿 nonce，构造 v2 Plan 命令，覆盖首次 light 接受、逻辑 step 后剩余递减、同 ID 重放不延长、冲突与旧 ID、sleep 模式、坏 JSON/session/MAC、owner mismatch 409 且无状态推进、匹配 owner 的 RAM touch 不落盘、三 token 域隔离、wall 跳变不改 deadline、两进程 plan ID 独立。对拒绝与重放读取 status 证明状态未意外改变。
- `cargo test -p device-sim`、`cargo test -p bridge-render`、`git diff --check` 通过。只要会对真实睡眠/会合产生假阳性的测试就不要写；此切片尚不验证 power lifecycle。

## 验证记录

`device-sim` 16 项 loopback 集成测试、`bridge-render` 全套（含 19 项 `v2_state`）、`cargo fmt -p device-sim --check` 与 `git diff --check` 通过。主代理审查修正过 JSON/session 的 command ACK、409 owner 回显、Plan 重放 ACK 授予秒数，以及 status 中递减剩余与 ACK 的区别。当前 Plan 只覆盖无 Bundle 状态，不验证睡眠或会合。
