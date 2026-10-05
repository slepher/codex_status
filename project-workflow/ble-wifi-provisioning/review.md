# 设计评审记录

2026-09-30：核对 `src/main.cpp` 的 AP 配网、Wi-Fi 槽位与 BOOT 长按路径，`src/ble_bridge.cpp` 的 Just Works 配对和 GATT 命令入口，Bridge 设备页与 BLE 客户端，以及 `quad` / `codex-status-a` 模板顶栏资源。

结论：流程可沿用现有 BLE 服务、命令特征、设备 MAC 身份和 Bridge 设备页。按用户修订，已有蓝牙绑定即可信，旧 Just Works 绑定无需迁移；屏幕码确认只用于新配对。配网写入仍检查已加密绑定、物理窗口和 MAC。NVS 三槽满额和断电提交需由实施测试证明；当前源码直接覆盖 slot 0，不能沿用该行为处理重新配网。

Windows 11 配对入口核对：Bridge 可在用户点击「配对」后用 `DeviceInformation.Pairing.PairAsync` 主动发起，Windows 负责系统弹窗；如需自定 PIN 界面才用 `Pairing.Custom.PairAsync`。设备仅广播不能强制弹窗；Swift Pair 需额外广告格式和系统支持，本方案不依赖它。依据：[Microsoft Pair devices](https://learn.microsoft.com/en-us/windows/apps/develop/devices-sensors/pair-devices)、[DevicePairingKinds](https://learn.microsoft.com/en-us/uwp/api/windows.devices.enumeration.devicepairingkinds?view=winrt-26100)、[Swift Pair](https://learn.microsoft.com/en-us/windows-hardware/design/component-guidelines/bluetooth-swift-pair)。

平台边界：Just Works、Passkey Entry 等是蓝牙标准，不是 Windows 协议；Swift Pair 广告提示是 Microsoft 扩展。macOS Core Bluetooth 可连接 LE 外设并访问要求加密的特征，但没有与 Windows `PairAsync` 同形的当前 Bridge 入口，跨平台客户端须另适配。依据：[Bluetooth Security Manager](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core_v6.3/out/en/host/security-manager-specification.html)、[Apple Core Bluetooth](https://developer.apple.com/documentation/corebluetooth)、[Apple encryption permissions](https://developer.apple.com/documentation/corebluetooth/cbattributepermissions)。

协议顺序收束：用户要求设备协议不分平台。两端统一为广播发现、连接读取 INFO、仅未绑定时配对、加密 GATT 配网；广播可独立进行供扫描，但不能承载配对和凭据。当前 NimBLE 2.x 名称可能在 scan response、128-bit service UUID 未必放入同一个传统广告包，因此发现不能假设主广告包同时有完整名称与 service UUID。

Windows 通知栏核对：普通 BLE 广播不足以触发 Swift Pair 通知。Microsoft 要求配对窗口内的**主广告包**带 Microsoft Vendor ID `0x0006` 的 Swift Pair 数据段及 Beacon ID/LE-only 场景，名称或类型也要在同一包（Windows 不为该通知主动读 scan response）；电脑还要支持并启用 Swift Pair，设备距离/RSSI 和仅每会话一次的通知策略会影响是否弹出。当前固件的 `CodexStatus-*` 名称广播和普通服务 UUID 不含此数据段，因此仅可被 Bridge/设置扫描发现，不会凭现有广播触发 Swift Pair 通知。本方案继续以 Bridge 显式「配对」为主，不增加平台专有广告。依据：[Microsoft Swift Pair 规范](https://learn.microsoft.com/en-us/windows-hardware/design/component-guidelines/bluetooth-swift-pair)。
