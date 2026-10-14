# Fake ROM 生产逻辑覆盖清单

日期：2026-10-14。范围按用户修订：Fake ROM 验证生产逻辑在不同输入、状态和调用顺序下是否满足规则；不以还原现实外设失败为目标。待办及实施状态统一归 [backlog B6](../../docs/roadmap/backlog.md#b6-离线-ble-配-wi-fi-与-boot-蓝牙恢复入口2026-10-03-新开)。本清单记录覆盖缺口和验收场景，七项尚未因本次文档化而实施或通过验收。

## 原则

- 测试经过真实生产分支。Fake 提供输入、时间和事件顺序，不另写一套业务答案。
- Proxy 只用于接通现有生产逻辑；按具体用例补最小接点，不预建完整 Wi-Fi、RTC、屏幕或通用故障仿真框架。
- 调用返回、回调顺序、超时和状态变化可以作为逻辑输入。验证这些输入下的处理规则，不模拟导致它们的物理过程。
- 优先找 Fake 绕过或重复实现的逻辑。已经共用且已有回归的能力，先核对具体缺失场景，再决定是否增加测试。

已完成的 [BLE 生命周期回归](fake-ble-lifecycle-20261014.md) 是参考：恢复缺失外设不回复的旧逻辑，同一 Fake ROM 测试在第二次连接进入隔离；恢复修复后连续假数据投递、渲染和重连通过。它证明该逻辑可复现，不要求真实蓝牙设备。

## 按优先级补齐的七项

| 优先级 | 生产逻辑 | 当前覆盖缺口 | 最小场景与通过条件 |
|---|---|---|---|
| 1 | 会合与重试 | Fake 的 ready-report 重试另写了一套，未完整复用 ROM 流程。 | 同一会合重复执行、会话更新、旧响应到达；保持该保留的请求身份，旧响应不推进新会合，重试按原规则结束。 |
| 2 | 电源窗口与期限 | Fake 有独立睡醒编排，未贯穿生产调度。 | 读取、claim、重试不延长期限；同一计划不重新计时；timer 唤醒不获得 BOOT 窗口；到期后不继续执行仅限窗口内的操作。 |
| 3 | 连接代际与待执行命令 | 新 BLE 回归覆盖 Bridge 关闭，尚未串入 ROM 命令队列。 | 旧连接排队的命令在新连接建立后到达时被拒绝；重复停止能收敛；旧回调不修改新连接的状态。 |
| 4 | 身份、owner 与会话校验 | Fake 部分身份条件固定成立，owner 和登记逻辑存在独立实现。 | owner 更换、session 更新、绑定身份变化后，旧请求不能继续授权；匹配当前身份的请求仍按生产规则处理。 |
| 5 | OTA 请求与完成判定 | Fake 上传流程另写，部分错误返回已经与 ROM 不一致。 | 相同请求得到相同拒绝原因与失败阶段；上传完成、重启、运行镜像确认分别推进对应状态，前一阶段不能冒充最终成功。 |
| 6 | 显示确认与后续刷新决策 | Fake 共用渲染，但未完整执行生产显示编排。 | 生成新帧不等于已显示；只有正确完成确认才更新显示基线；旧完成事件不能确认新帧，下一次刷新依据最后可信状态决策。 |
| 7 | 按键事件与窗口起点 | Fake 直接执行切模板，绕过事件解释逻辑。 | 相同按键序列即使延后处理，也得到相同动作；窗口从物理事件时间起算；已消费的唤醒按键不重复执行。 |

这些是测试覆盖候选，不等同于七个已确认固件缺陷。开始某项时先核对现有测试，定位到确实未覆盖的生产分支。

## 定向阅读入口

| 项目 | 生产逻辑与现有测试 |
|---|---|
| 会合与重试 | [radio.cpp](../../src/firmware/radio.cpp) 的 serviceReadyReport/servicePresence；[rendezvous_protocol.cpp](../../src/rendezvous_protocol.cpp)；Fake 对照 [main.rs](../../bridge/crates/device-sim/src/main.rs) 的 sim_ready_reports |
| 电源窗口与期限 | [power.cpp](../../src/firmware/power.cpp)、[clock.cpp](../../src/firmware/clock.cpp)、[plan_store.cpp](../../src/plan_store.cpp)；规则见 [power-state.md](../../docs/power-state.md)，Fake 对照 reconcile_power |
| 连接代际与命令 | [ble_bridge.cpp](../../src/ble_bridge.cpp)、[transport.cpp](../../src/firmware/transport.cpp)；已有 [BLE 并发回放](../../tools/test-ble-info-concurrency.py) 与 [host stop 回放](../../tools/test-nimble-host-stop.py) |
| 身份与权限 | [owner_store.cpp](../../src/owner_store.cpp)、[bridge_store.cpp](../../src/bridge_store.cpp)、[v2_claim_command.cpp](../../src/v2_claim_command.cpp)；Fake 对照 OwnerStore 与 sim_ble_aux |
| OTA | [ota_http.inc](../../src/firmware/ota_http.inc)；已有直接运行生产入口的 [test-ota-http.cpp](../../tools/test-ota-http.cpp) |
| 显示确认 | [display.cpp](../../src/firmware/display.cpp)；Fake 对照 [ffi.cpp](../../bridge/crates/render/src/ffi.cpp) 的 simRender |
| 按键 | [boot_capture.h](../../src/boot_capture.h)、[application.cpp](../../src/firmware/application.cpp)；已有 [plan_store_test.cpp](../../bridge/crates/render/src/plan_store_test.cpp) 的 BootCapture 检查 |

## 已有能力的处理

配置事务、Bundle 存储、模板渲染已经大量共用生产代码，不另列基础模型改造。仅在现有测试确实遗漏时补组合场景，例如原事务重复查询、取消后收到结果、重启后恢复、旧 ACK 到达新数据。保持实际事务身份及生产处理路径，不通过预设成功结果来证明通过。

## 统一验收规则

每项按以下顺序留下可运行证据：

1. 写清一条规则和预期结果，指出对应生产分支及 Fake 当前旁路。
2. 用最小输入、状态或事件顺序让 Fake 执行该分支；已有接点足够时不新增抽象。
3. 用实际旧缺陷或受控的错误分支验证测试会失败，记录准确失败原因；负向对照不得留在交付代码中。
4. 恢复正确逻辑后，同一用例通过；涉及投递时配合假数据源检查结果和下一次操作，不只检查调用返回。
5. 在 backlog 对应条目记录已覆盖范围与证据，区分既有测试、新增通过及仍未覆盖部分。

通过标准是错误生产逻辑会使测试失败、正确生产逻辑使测试通过；不以模拟逼真程度或新增假设备数量衡量完成度。
