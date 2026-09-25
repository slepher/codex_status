# Task 8：v2 周期调度遍历已登记 MAC

日期：2026-09-24。状态：完成宿主验证，未提交。

## 设计决策

现有 v2 定时循环以 `ctx.device_mac` 与全局 `device_cache` 为唯一目标，Task 6/7 的逐 MAC 状态尚未用于投递。当前现场设备均为 v2；本任务不构造或验收不存在的 legacy 实机场景。此任务只改 HTTP 周期调度，不改 BLE、发现、UI、MCP、模拟器或时钟。

每轮维持现有 envelope/DataSource 更新；界面当前设备的 pending activity 提示暂保留原行为。随后从 `PlatformService::devices()` 取已登记且标记为 v2 的规范化 MAC，并依据 `v2_status_cache` 的该 MAC 条目筛出最近 30 秒认证在线的目标。无记录、离线、过期、无端点的目标跳过。`legacy` 数据字段仅用于防止历史配置误进 v2 协议路径，不代表当前存在 legacy 设备。逐目标顺序调用 `platform::cycle(ctx, mac, refresh, deliver_now)`，保留现有 30 秒 plan/状态节奏和 60 秒自动投递节奏、`CODEX_STATUS_PLATFORM_AUTODELIVER` 开关。设备 A 失败不能阻断 B。不得使用全局 `device_cache` 判定 v2 在线，也不得将未登记 MAC 加入投递。

`platform::cycle` 显式接收目标 MAC，并使用 Task 7 的 `v2_occupancy_gate(ctx, mac)`；只有 Owned 才能继续。401、409、404、yielded、离线/过期都不写入该设备。若本轮 `refresh_status` 失败或 MAC 错配，不发送该轮 Plan/Data；下轮重新判断。所有 HTTP 写入仍通过 Task 2 的实时 MAC 守卫，claim 仍用 Task 5 的预检。周期循环暂沿用真实时间，实验时钟在阶段 E 接入。

## 编码所有权与验收

- 6-luna high 只修改 `bridge/crates/app/src/main.rs` 与 `bridge/crates/app/src/platform.rs`，不作设计决策，不改 ROM、core、BLE、MCP 或其他文件。
- 添加纯目标选择测试：两个已登记 v2 MAC 中一台在线、一台离线/过期；当前选中设备不影响结果，缺失/未登记 MAC 不参与。
- 隔离 `CARGO_TARGET_DIR` 验证 `cargo check -p bridge-app`、定向测试、`git diff --check`；不启动生产 Bridge、不访问设备、不提交。

## 结果

6-luna high 完成逐 MAC 目标筛选及 `platform::cycle(ctx, mac, ...)`。周期循环不再用界面全局状态作为 v2 在线门槛；刷新失败会停止该目标本轮 Plan/Data。修订后的 3 项目标/状态测试、隔离 `cargo check -p bridge-app` 和 `git diff --check` 通过。当前没有 legacy 实机，测试不构造该场景；历史数据字段的协议防护保留。未部署、未访问设备。
