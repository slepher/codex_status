# Fake ROM 未完成部分修订计划（Astra 最终版）

日期：2026-09-26。当前执行范围：C2 的 D 剩余、E、F。用户取消 G 整套实机回归；只为 D/E/F 无法覆盖的硬件特有错误保留少量针对性 case 文档，不实现或实测。此前的 Astra 方案包含 G，以下修订以本次用户决定为准。

唯一待办入口为 `docs/roadmap/backlog.md` C2；本目录仅展开其执行顺序、验收与证据。历史 `docs/history/workflow/fake-rom-simulator/` 保持只读，已完成 A/B/B2/C、D13a–d 的记录仍有效，不把它当当前后续计划。

## 修订工作的顺序

1. 从用户要求、v2 平台合同、最新 sleep-aware-bridge 决策提取不变量。
2. 核对 device-sim、共享 C++、render、Bridge 实际入口，区分已编译决策与完整设备行为。
3. 重写 `docs/fake-rom-simulator-design.md`，给出最小可用阶段与完整 Fake ROM 门槛。
4. 用本目录 task/status/review 与 backlog C2、导航形成一致引用；检查 diff，不运行设备或构建。

## 实施顺序

| 阶段 | 交付 | 前置与出口 |
|---|---|---|
| D1 | 可配置的 HTTP 同源设备：Bundle → Data → Activate，帧和状态一致 | 延续已有 C/D；完整配置后才开放 configured 能力；不等待整机抽象 |
| D2 | 持久恢复与真实电源编排切片，按键/显示完成/会合、HTTP 可达性 | D1；冷启动/deep/软重启/掉电明确区分，不能只设置 sleep 标签 |
| D3 | 有限版本 OTA、错误注入与 1x 延后投递回归 | D2；构成近期最小可用 Fake ROM，生产 Bridge 应用路径在隔离环境交付任务 |
| E1 | Bridge 每 MAC 时钟接线，新增 OTA/快照路径一起接入 | D3；目标时钟不污染真实设备，先保留持久 epoch 含义 |
| E2 | fake BLE 与协作 step/max 调度 | E1；真实 BLE 业务构造复用，全部参与者有在途屏障 |
| F | 完整故障矩阵、恢复、失步、多设备与确定性回放 | E2；矩阵各行有证据，完整软件 Fake ROM 门槛达成 |

D1/D2 每次共享固件改动都须做相应宿主测试和授权目标构建，不能把编译问题留到 F。F 以每类代表性故障和可重放证据验收，不做全错误组合穷举。

先做一个纵向可观察行为，验收后再进入下一项。可以保留已有函数/全局状态；只有实际阻碍同源编译时才增加薄接点。没有要求建设通用 DeviceKernel、整机 HAL、独立 sys crate、场景 DSL 或插件系统。

阶段细项见 `task.md`；语义、时钟与故障矩阵以当前设计为准。若实现发现当前 ROM 与需求冲突，应新增共享路径修复和证据，不能仅在 fake 改出理想行为后宣布通过。
