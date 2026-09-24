# Task 10a：BLE 一次扫描选择已登记目标

日期：2026-09-24。状态：完成宿主验证，未提交。

## 设计决策

当前 `V2Connection::connect(mac, ...)` 对单 MAC 扫描最多 3 秒。多设备循环若逐个 MAC 调用会在前一个目标不广播时耗尽下一个目标的 3 秒会合窗口。新增 `V2Connection::connect_any(target_macs, token, bridge_id) -> Result<Option<(normalized_mac, V2Connection)>>`，一次扫描 3 秒，候选广播名为每个规范化 MAC 的 `CodexStatus-<后6位>`。入参空集合不扫描，非法 MAC 直接报错，重复 MAC 去重。只把广播名当候选过滤，**连接后读取 GATT info 的完整 Wi-Fi MAC，必须属于入参目标集合**，并保留原 `rendezvous_v >= 2` 校验；错配要断开且不得发 v2 命令。返回经 info 验证的规范化 MAC，而不是从广播名猜身份。真实 TCP/GATT 超时与现有 connect 保持一致。

将现有单目标 `connect(mac, ...)` 委托到上述实现，保留原签名和结果，使其他调用方不变。一个连接只代表一台设备；连接关闭规则不变。不要修改广播协议、固件、BLE token 流程或 app 调度。

## 编码所有权与验收

- 6-luna high 只修改 `bridge/crates/ble/src/lib.rs`，不做设计决策，不改 app/core/MCP/ROM 或文档。
- 提取纯广播候选和 info MAC 授权判断，测试两个目标、非法/重复 MAC、后 6 位广播名冲突时仍只以完整 info MAC 决定。无需实际蓝牙适配器。
- 隔离 `CARGO_TARGET_DIR` 运行 `cargo test -p bridge-ble`、`cargo check -p bridge-app` 和 `git diff --check`。不提交、不访问设备或运行中的 Bridge。

## 结果与评审

6-luna high 完成 `connect_any`，广播名只作一次扫描候选，GATT info 的完整 MAC 必须属于目标集合。主代理要求消除与既有 `find_device` 的重复扫描循环，现两者共用 `find_device_matching`。隔离 `bridge-ble` 测试 7 项、`bridge-app` check 与 `git diff --check` 通过。未接设备或提交。
