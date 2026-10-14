# Fake ROM BLE 独立设备生命周期

## Plan / task

用户要求将BLE作为独立可操作设备、ROM通过proxy调用，在Fake ROM配合假数据源复现关闭竞态。延续diagnose红→绿及Ponytail最小共享逻辑，禁止只在Fake复制成功分支。

- [x] 从btleplug提取纯逻辑的外设关闭/状态请求处理，CoreBluetooth与Fake proxy共用实际future。
- [x] Fake ROM独立BLE对象支持控制端断开/关闭回调丢失，ROM经proxy操作。
- [x] Bridge模拟原生生命周期走相同有界关闭/NativeLease隔离，和真实蓝牙状态分开。
- [x] 假数据连续投递：远端先断开、重复关闭、重新连接、旧代际不能关闭新连接、真正未知关闭阻断下一连接。
- [x] 移除修复分支红、恢复后绿，相关回归通过并更新边界。

## Status / review

实现及验收完成；运行Bridge/ROM保持既有部署。这是对之前“Fake无法复现”结论的补正：原实现没有建模此路径，不代表此逻辑缺陷无法模拟。

## 结构与操作

- `device-sim/src/ble_device.rs`：独立BleDevice持有连接代际、GATT属性及关闭回调策略；ROM使用BleProxy连接、复位、断开和写属性。现有GATT处理器经同一共享对象读取/维护属性，无第二份设备状态。
- `/sim/ble/device`：GET（endpoint/control凭据）只读generation/connected/close_callback；POST只接受control凭据，op可为disconnect、drop_close_callback、restore_close_callback。设备控制不经过ROM业务命令，不修改真实设备或绑定。
- `ble/src/sim_peripheral.rs`：消费该设备的连接事实，同代际存在则等待回调，已断开/换代际则已从central表移除。HTTP关闭响应和模拟原生回调明确分开，丢回调仍Pending。旧句柄不关闭新代际。
- `btleplug/src/connection_lifecycle.rs`：CoreBluetooth和模拟proxy共用同一close/state分支及原future；不是在Fake复制修复。
- 模拟Bridge设置`CODEX_STATUS_SIM_BLE_LIFECYCLE=1`后使用生产NativeLease和disconnect_bounded，模拟radio的AtomicU8与真实radio分开。已有仅测GATT/业务、故意丢弃连接句柄的fixture保留旧模式，不能算生命周期验收。专用回归强制开启新模式；未知状态只有测试进程结束才清除，没有自动重置隔离捷径。

## 回归与证据

专用测试：`cargo test --offline -p device-sim --test bootstrap fake_ble_device_remote_close_reconnect_and_unknown_quarantine -- --nocapture`（在bridge目录，按操作细则使用隔离target）。测试自动启动独立Fake ROM及假StaticJson数据源，全部使用合成凭据和loopback地址。

- 生产DataSource.snapshot生成Plus/Pro/Team三组值，经生产HTTP Data客户端投递；每次用同源渲染器逐字节比对Fake屏幕帧。真实DeviceConnection/GATT建立会话，独立设备先断开，再关闭Bridge句柄，接着重新连接。
- Fake模板Bundle的context不是16字节BLE context，因此Data沿既有HTTP接口；BLE测status/close/reconnect。不声称跑了完整App采集调度或BLE Data传输。
- 重复远端断开、旧句柄遇新连接代际、普通回调关闭、回调丢失后NativeLease阻断新连接均覆盖；隔离后的连接尝试不得改变设备代际/状态。
- 负向对照只临时删除**共享实现**中缺失外设的完成回复；同一Fake测试在第二次连接稳定报ble_native_quarantined_restart_required。源码在finally恢复。证据：`artifacts/bridge-macos/fake-ble-lifecycle-red.log`；保留修复的专用测试通过，`fake-ble-lifecycle-test.log`。
- 原4项原生handler回放继续通过，工具改为导入共享生命周期模块。完整回归App94通过/3条件忽略、BLE26通过、Fake67通过，合计187通过/3忽略；日志`artifacts/bridge-macos/fake-ble-lifecycle-regression.log`。最终模拟连接拒绝/未知分类补充后，专用回归再次通过，`fake-ble-lifecycle-final.log`；`git diff --check`通过。

## 验收边界

生命周期逻辑可以在Fake中确定性覆盖。系统callback的RF时序、TCC授权、真实绑定/驱动仍需实机；这不妨碍在软件中注入相同先后顺序并验证逻辑。此次未重启/重新部署Bridge、未刷ROM、未操作现场设备、未提交；AP01工作保持。
