# Task 6：逐 MAC 认证状态轮询与在线缓存

日期：2026-09-24。状态：完成宿主验证，未提交。

## 决策与合同

保留当前单设备缓存供 legacy UI/推送使用。在 `AppCtx` 另加一份仅用于 v2 的内存缓存，以规范化 Wi-Fi MAC 为键；每条记录包含 `online`、`fetched_at`、上次成功的认证 `/v2/status` JSON。失败时将该 MAC 标为离线但保留其上次状态供诊断；不得写到别的 MAC 条目或全局 `device_cache`/`owner_cache`。

现有 `platform::refresh_status(ctx, mac)` 已按目标端点读取并核验状态 MAC。成功核验并写入 PlatformService 后，更新该 MAC 在线缓存；等待链路、网络失败、错 MAC 都只标该 MAC 离线。缓存不作为认证来源，后续写操作继续现有实时 MAC 守卫。

在 `run_services` 启动一个独立 v2 只读轮询，每轮从 `PlatformService::devices()` 取已登记的非 legacy 设备，取其规范化 MAC。每 10 秒依次调用 `refresh_status`；跳过缺 IP/无 token 目标，不阻塞现有单设备缓存或旧推送循环。轮询只读，不自动 claim、发 Plan、Data、Bundle、Activate 或 BLE 命令。暂停状态下跳过。不要用 loopback 地址自动推断 fake 身份。

## 编码所有权与验收

- 6-luna high 只修改 `bridge/crates/app/src/main.rs` 和 `bridge/crates/app/src/platform.rs`。不得修改 ROM、core、MCP、BLE、UI、计划文档，也不得自行改变合同。
- 用局部纯缓存操作测试两个 MAC 的在线、离线与错配隔离；若可复用现有本地 HTTP fixture，也可加 2 设备轮询测试，但不引入复杂 mock 框架。
- 隔离 `CARGO_TARGET_DIR` 运行 `cargo check -p bridge-app`、适用定向测试及 `git diff --check`。不提交、不碰实机和运行中的 Bridge。

## 结果

`AppCtx` 新增按 MAC 的认证 v2 状态缓存；`refresh_status` 对成功、错 MAC、离线分别更新目标缓存。独立 10 秒轮询逐个读取已登记的非 legacy 目标，仅做认证状态读取。隔离 `cargo check -p bridge-app`、缓存隔离测试 1 项、claim 回归测试 2 项及 `git diff --check` 全通过。`online`/`fetched_at` 暂无消费者，将由逐台调度接入；未访问设备或运行中的 Bridge。
