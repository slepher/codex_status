# Task 10b：app 按登记 MAC 调度 BLE 会合

日期：2026-09-24。状态：完成宿主验证，未提交。

## 设计决策

Bridge 当前 BLE v2 循环只取界面当前 `ctx.device_mac`，并用一个 `last_v2_ok` 节流。改为从 `PlatformService::devices()` 取已登记、标记 v2 的规范化 MAC 集合；无端点 IP 的目标仍可参加 BLE 会合。`last_v2_attempt` 改为按 MAC 的内存映射，某目标最近 55 秒内已实际连接尝试时暂不再次进入候选，另一目标不受影响。空候选不扫描。调用 Task 10a 的 `connect_any`，一次扫描 3 秒覆盖所有候选；连接后以其返回的完整 MAC 调用该设备的状态、Data/Plan/ACK 服务。不得用界面选择或广播名推断身份。持续 250 ms 唤醒机制不变，真实 GATT watchdog 不变。

将 `platform::ble_cycle` 改为显式接收候选 MAC 列表，内部调用 `V2Connection::connect_any`。结果须区分：未见设备；扫描/连接前错误；以及已连接某 MAC 后的会合结果（成功或失败）。app 对任何已连接尝试均记录该 MAC 的尝试时间，防止失败时在同一个窗口反复连接；错误仍按原等级记日志。连接返回 MAC 后再次确认该 MAC 仍在已登记 v2 设备集合，确认前不得发送命令。单台 ACK 仍只记到该 MAC。

已有 UDP `ble=1` 与显式 force 通知应促使扫描，但当前选中 v2 时不得退回旧 `Pusher::cycle_once` 旁路。旧路径仅保留给尚未登记为 v2 的当前设备，避免改动历史协议行为；本轮测试和验收只用 v2 设备。整个任务不修改发现协议、token、固件、Fake ROM 或时钟。

## 编码所有权与验收

- 6-luna high 只修改 `bridge/crates/app/src/main.rs`、`bridge/crates/app/src/platform.rs`；不改 BLE 库或其他文件，不设计新行为。
- 添加纯候选选择/节流测试：两个已登记 v2 MAC，A 刚尝试仍应选 B；55 秒到期 A 恢复；未登记 MAC 和错误 MAC 不参加。无需真实蓝牙设备。
- 隔离 `CARGO_TARGET_DIR` 运行 `cargo check -p bridge-app`、定向测试、`git diff --check`。不启动生产 Bridge、不接实机、不提交。

## 结果与评审

6-luna high 将 app BLE 候选及 55 秒节流改为逐 MAC，`ble_cycle` 一次扫描并在命令前复核目标仍登记。主代理要求删除不存在的 legacy 设备测试，并确保 UDP 通知不会在已有 v2 登记目标时走旧 30 秒扫描；均已修正。隔离 `cargo check -p bridge-app`、2 项 v2 候选测试和 `git diff --check` 通过。未接设备、未提交。
