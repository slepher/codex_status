# Bridge macOS 后续实施任务与验收

新窗口使用 [remaining-plan.md](remaining-plan.md) 接续未完成部分；下表保留原 T0–T7 任务定义，不表示每一项均未实现。真实状态以 PROGRESS/backlog 为准。

本文件是 [plan.md](plan.md) 的执行拆分；2026-10-03 用户已授权开始执行，先进入 P0/T0。当前状态与排期以 [backlog B5](../../docs/roadmap/backlog.md#b5-bridge-macos-菜单栏支持2026-10-03-新开) 为准。

现场范围更新：用户仅携带 Note4，1.54 不在此处。当前硬件验收只安排 Note4；双设备身份与故障路由可用软件模拟，1.54 实机单列未验。离线 BLE 配网及 BOOT 打开蓝牙按 [wifi-setup-plan.md](wifi-setup-plan.md) / backlog B6 作为首版接入前置，不能把现行能力对等当成已支持离线配网。

用户确认 UI 入口：B6 的蓝牙配 Wi-Fi 和本机绑定独立方案在现有 Bridge 设备页实现，覆盖离线进入、网络映射管理、主机当前网络/设备所选网络与设备报告的配网结果；与 T4 共用 application service，具体实施归 W3。

| 任务 | 主要文件/责任范围 | 验收 |
|---|---|---|
| T0 基线与最小编译 | `app/Cargo.toml`、`autostart.rs`、macOS Tauri 配置；BLE Windows 示例条件编译；构建环境 | 列出锁定依赖、SDK、两设备运行 ROM/能力与 Windows 对照版本；最小 `.app` 可启动，纯 Bridge 构建不触发固件构建 |
| T1 BLE 权限与首次绑定 | macOS Info.plist、`ble/src/lib.rs` 的 info/配对/取 token、应用接入提示 | 真 `.app` 的授权、拒绝、撤销恢复；仅 pairing window 内首次绑定；bonded+encrypted 后取 token；错 MAC/未绑定不能操作 |
| T2 扫描身份与短窗口 | `ble/src/lib.rs` 的 peripheral ID 去重、adapter 生命周期、事件/重试/清理 | 同时两设备不会合并；缺名称、晚 scan response、重启广播和重连可恢复；自然 timer 会合有完整 ACK/radio-off 证据 |
| T3 菜单栏与本机路径 | `main.rs`、`icon.rs`、`config.rs`、`instance.rs`、`core/paths.rs`、`core/codex.rs` | Retina/深浅色/0–100/无数据/错误；打开原 UI、关窗常驻；Finder 可找到 CLI；data 可写、Resources 只读、身份稳定、命名实例隔离 |
| T4 HTTP/MCP 与业务合同 | `discovery.rs`、`platform.rs` 的必要平台差异、现有服务和 UI | 按 MAC 发现/claim/续约/释放、409 不抢占；push/pull/ACK；8 个 Profile 条目完整冻结发布；诊断/Bundle/OTA 延后与重启恢复 |
| T5 常驻生命周期 | macOS autostart、`instance.rs`、启动/停止脚本、`main.rs` 的 managed 入口 | 按需启动不默认登录自启；重复启动不重复写数据；退出不重启、异常重启有限制；关 Codex/终端存活；不同实例互不停止 |
| T6 睡眠与网络恢复 | BLE 状态重建、LAN/endpoint 刷新、现行调度 | Mac 睡眠/唤醒、蓝牙关开、拒权后授权、换 IP、VPN/多网卡；不补发过期 Plan，不把旧 ACK 记为成功，不阻止系统睡眠 |
| T7 回归与打包 | 独立 Bridge CI、资源打包、macOS 使用/恢复文档 | Intel/Apple Silicon 各自构建与支持证据；Windows 回归；应用升级/回滚保留 data；签名/公证状态明确 |

## 验证矩阵

| 层次 | 建议验证 | 证据边界 |
|---|---|---|
| 宿主自动化 | `bridge-core/app/ble/mcp/render` 现有测试；路径/身份/去重/配对安全闸门的少量行为回归；已有 device-page JS 检查 | 后续使用预建的隔离 target；Windows 实验 examples 限定平台，不用全平台 all-targets 强行编译 |
| Fake ROM | 双 MAC、401/409/错 MAC、ACK 丢失、push/pull、Plan 过期、8 项 Bundle、进程重启与作业恢复 | 只能证明业务逻辑，不能证明 CoreBluetooth、TCC、RF、实际功耗 |
| 原生桌面 | 真 `.app` 的托盘与窗口、Finder/LaunchAgent、拒权恢复、关闭 Codex、应用升级、正常退出与崩溃 | 终端启动和 terminal Bluetooth 授权不替代原生应用包测试 |
| 真实会合 | 当前 Note4 在可用 Windows/macOS 现场各 ≥30 次自然 timer 会合，加 BOOT、关桥超时、蓝牙关开、主机睡眠恢复；1.54 与真实双设备同时广播另标未验 | 相同设备/ROM/位置尽量交错比较，含失败轮，记录分阶段耗时、成功/失败、radio-off 和设备 wake identity；软件模拟不冒充另一板实机 |
| 长时 | macOS ≥24 h 常驻，期间隐藏 UI、暂停/恢复、主机睡眠、网络切换和队列重启 | 检查 adapter/扫描任务泄漏、错误洪泛、错误续 light deadline、队列停滞与会合恢复；每平台架构单独标已验/未验 |

会合首轮筛查的建议目标是每设备每平台 ≥29/30 有效回答，且没有持续性重连失败、跨设备误操作或越过设备 radio 截止期；这是拟定验收目标，不是已有测量结果。若 Windows 对照也未达到，先明确共同基线问题，不能据小样本宣布 macOS 对等或放宽设备窗口。P95 与完整失败分类一并提供，≥30 个样本只作筛查，不推导长期可靠率。

低功耗验收先证明沿用正式 PowerPlan、radio 截止期和 ACK 后收尾；若宣称与 Windows 相同的能耗，须另有电流仪的同条件整周期证据。主机实际睡眠时的会合缺失须独立统计，不伪装成 RF 失败或成功。

OTA/发布是未来端到端验收动作，不是本次操作；须在获准实机验证时明确目标 MAC、已有正确 ROM 与恢复证据。优先用 Fake ROM 验证失败矩阵，避免为托盘移植重刷设备。
