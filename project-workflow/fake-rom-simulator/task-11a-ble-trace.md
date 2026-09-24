# Task 11a — BLE 扫描和 GATT 时间线

状态：完成主代理评审与宿主验证。前置：Task 10b 已提交 `c82487c`。主代理负责合同与评审；编码仅执行本文件的实现，未改变协议或阶段顺序。

## 范围与设计

- 仅改 `bridge/crates/ble/src/lib.rs` 及本任务文档。使用现有 `tracing`；不改固件、ROM、生产数据和设备协议。
- 为 `find_device_matching` 的每个真实扫描窗口分配进程内递增 `scan_id`。扫描窗口在适配器选择/扫描开始后计时，记录开始/结束、耗时、结果、发现/更新数、目标候选数、忽略原因计数。适配器不可用和 `start_scan` 失败也要有对应错误事件。时间戳由 tracing 提供；持续时间用 `Instant`。
- 对 `CodexStatus-*` 广播：记录首次见到该候选的时间点、广播名、BLE 地址、RSSI（API 可用则记录）、匹配/忽略原因。广播后缀只是候选，不能标成已验证完整 Wi-Fi MAC。无关广告只计数，不记录名字或地址；重复 update 去重并汇总，避免刷屏。
- 连接、GATT service/characteristic 查找、info 读取和完整 MAC 验证、命令写入/ACK 应分段记录成功/失败及耗时；只有 info 匹配后才打 `verified_mac`。错误只记录安全的阶段/类别，避免把载荷或 token 写入日志。
- 默认 info：匹配候选或错误立即记录；无候选的空窗口最多 30 秒一条汇总，附被采样掉的窗口数。debug 可保留逐窗口细节，但即使 debug 打开也不能按 250 ms app tick 打日志。

## 验收

1. 无硬件的定向测试覆盖扫描采样边界、候选/无关/重复广播计数和匹配决策，不依赖实际蓝牙适配器。
2. `cargo test -p bridge-ble`、`cargo check -p bridge-app`（隔离 target 避开运行中 exe 锁）和 `git diff --check` 通过。
3. 评审确认默认日志足以定位“开始扫描但没广播”“见广播但未连接”“连接后 info MAC 不匹配”“GATT/ACK 失败”，且不暴露敏感内容。

## 实现证据

- `bridge/crates/ble/src/lib.rs` 为真实扫描窗口分配递增 `scan_id`，记录窗口结果与计数；默认 info 级开始事件延迟到窗口结束后，只为有候选或错误的窗口补记，空窗口仅按 30 秒采样并报告期间被省略的窗口数。逐窗口开始事件留在 debug。候选广播按 BLE 地址去重；只记录 `CodexStatus-*` 广播的名字、地址与可用 RSSI，其他广播只累加计数。
- connect、service/characteristic、info read、目标 MAC 核验、v2 命令写入和 ACK 均分阶段记录安全结果及耗时。仅在目标 MAC 核验成功后记录 `verified_mac`；status notification 改为只记录字节数，不输出内容。
- 隔离 `CARGO_TARGET_DIR=bridge/artifacts/task-11a-target`：`cargo test -p bridge-ble` 11 项通过；`cargo check -p bridge-app` 通过；`git diff --check` 通过。未接 BLE 硬件、未启动或停止 Bridge，未修改固件/ROM。
