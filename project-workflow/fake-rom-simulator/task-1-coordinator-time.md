# Task 1：Coordinator 显式时间参数

状态：2026-09-24 已完成并通过宿主验证。目标是消除 `bridge/crates/core/src/coordinator.rs` 的四处隐藏 `crate::now_secs()`，作为 Bridge 对每个 MAC 传不同时间的第一个接点；本任务不实现虚拟时钟，也不改变生产行为。

## 精确接口

- `Coordinator::enqueue_bundle(bundle, now: u64)`：`created_at` 和 `updated_at` 均取 `now`。
- `Coordinator::cancel_job(now: u64)`：仅未终结作业的 `updated_at` 取 `now`。
- `Coordinator::note_plan_ack(plan_id, accepted_remaining_s, provisional, now: u64)`：接受时间及由此算出的 light hold 使用 `now`；旧 ACK 判定顺序不变。
- `Coordinator::summary(now: u64)`：`full_sync_due` 用传入的 `now`，只读且不写状态。

只为调用这些接口所需的直接调用点补参数。已有调用者本轮传原本的真实 `crate::now_secs()` 或其已有 `now` 参数；没有已有参数的 `PlatformService` 公共入口可新增尾部 `now: u64`，再由 app 传生产当前时间。不得把系统时间藏进一个新 wrapper，也不得修改持久化结构、协议字段、业务判断或设备行为。

## 文件所有权与验证

编码代理负责 `bridge/crates/core/src/coordinator.rs`、`bridge/crates/core/src/platform/service.rs`、相关现有 Rust 测试调用点，以及为编译所需的 `bridge/crates/app/src/platform.rs` 直接调用点；其他文件需先报告。补一个有意义的测试：为两个不同 MAC 的 coordinator 传不同 `now`，验证作业/计划时间独立，并验证 summary 的 full-sync 判定由传入时间决定。运行 `cargo test -p bridge-core`，若 app 签名受影响再运行 `cargo check -p bridge-app`；运行中的 `bridge/target/debug` 不可被触碰，使用独立 `CARGO_TARGET_DIR`。最后运行 `git diff --check`。

完成时报告精确改动、测试结果、剩余隐藏时间调用及风险。禁止子代理自行扩展到模拟器、Bridge 多设备路由或实体设备。

## 结果

6-luna high 按上述接口修改 Coordinator、PlatformService 与直接调用方。`app/src/main.rs` 中一处 `coordinator_summary` 调用经主代理限定授权后同步传入生产真实时间。新增两 MAC 测试覆盖独立的 job 时间、Plan ACK/light hold 时间和传入时间决定的 full-sync 判断。`coordinator.rs` 中剩余 `crate::now_secs()` 为 0。

隔离 `CARGO_TARGET_DIR=bridge/artifacts/coordinator-explicit-time-target`：`cargo test -p bridge-core` 104 项通过，`cargo check -p bridge-app` 通过；`git diff --check` 通过。没有启动或停止运行中的 Bridge，没有访问设备，没有提交 Git。此项只解决 Coordinator 的隐藏时钟；service/app/ble 的其他系统时间读取仍属后续阶段。
