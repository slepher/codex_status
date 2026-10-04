# 会合协议模块实施与 Astra 收口审核

2026-10-04。用户授权范围：计划并实现 Bridge/设备协议模块和 Astra 审核发现。此轮没有部署、启动生产 Bridge、烧录、串口或真实 Wi-Fi 写入；没有提交。待办唯一归属 backlog B6。

## 模块与当前流程

- 设备 `src/rendezvous_protocol.h/.cpp`：阶段、RF截止期、当前轮plan事件、HTTP就绪/handoff证明、200ms ACK宽限、重放与后续更短计划。`main.cpp` 留硬件、GATT/HTTP认证和owner查询适配器、Wi-Fi异步关联、PM与绘制。
- Bridge `core/src/rendezvous.rs`：准入、绝对预算、有限错误分类。`app/src/rendezvous.rs`：生产 ble_cycle、认证HTTP证明、显式claim/handoff、锁与关闭、准入工作调度。`platform.rs` 留业务/传输适配，复用已有 envelope/data/plan/bundle/claim/sync 实现。
- 点击完整读取：持久读意图→本轮冻结准入ID→正式有界light计划→设备保持BLE与CPU awake、异步开Wi-Fi/HTTP→Bridge认证HTTP核对MAC/nonce/wake/plan及挑战→必要时通过既有token规则显式claim→BLE handoff→设备关闭普通BLE→Bridge先释放投递两锁再close_bounded(2s)→重取每MAC锁，分页读完整方案→诊断complete。只有正式PowerPlan改变light期限，读取/claim/handoff不续租。
- BLE实际开启时持有独立NO_LIGHT_SLEEP锁，关闭控制器后释放。物理恢复的固定授权仍按原期限保持BLE/awake；USB自动BLE在交接后不重新开启，物理操作可重新开启。

## 初审逐项与证据

| 项 | 实施 | 回归证据 |
|---|---|---|
| A1 重放ACK报原grant | 固件报remainingS；Bridge重复确认只能缩短已有估计 | 生产C++ t60重放；device-sim 120→10秒后110；coordinator重复过报不延长期限 |
| A2 诊断先关窗 | 冻结准入任务集；完整读取先于complete；临时读失败时延后诊断关闭 | 生产ble_cycle多页读取，每页220ms，真实HTTP客户端与诊断归档；断言最后一页早于complete；临时失败不complete |
| A3 普通plan ACK丢失 | 持久last_sent；同MAC/会话/wake/plan认证HTTP读回，未确认不放行 | plan ACK lost、plan+handoff双ACK lost完成；plan根本没生效时有界退出、读仍待办且无handoff/分页/complete |
| A4 序号/持久化 | 新ID追设备/本地/已确认高水位；发前落盘；重启复用同ID | 设备100→Bridge101；重开PlatformService后复用与HTTP确认 |
| A5 预算/交接缺口 | 新能力字段、HTTP挑战及owner绑定的handoff；HTTP阶段20s硬限、已准入窗90s上限、Wi-Fi业务8s上限；采样前锚定、所有子操作只缩短 | 生产C++慢就绪/不可达/错证明/短计划/第二轮；真实ble_cycle ACK未知与有界失败；两锁释放回归 |
| A6 错误折叠 | HTTP状态类型与业务错误/分页形状/身份分类有限；未知文本不持久化；不把失败读作空方案，已发写保持待确认 | 真实HTTP客户端401/200业务错误/坏JSON/错MAC/错offset；service保留旧完整快照与任务分类 |
| A7 非执行任务申请light | 排除recovery_save、等授权/输入、终态、冲突、退避和过期；输入/授权后可重新准入 | admission/service回归；恢复超时在同进程可再次wifi_begin |
| A8 close持锁/无界 | ble_cycle及Wi-Fi三路径先释放全局/每MAC锁再有界close | disconnect永不完成注入，deliver/下一轮不被卡，两锁释放 |

最初序号/ACK剩余行为测试出现red后已修复。原审查中的手动“先complete再读”Fake用例仍用于刻画旧协议关闭语义；现代app顺序的修复证据是上述生产ble_cycle组合，不能将负例刻画通过当作修复通过。

## Astra 复审追加与处理

1. 首次HTTP确认镜像重新锚定：返回HTTP请求前sampled_at用于note_device_status_at，执行预算亦从sample_start起算。双ACK丢失用例重开持久服务，断言last_accepted_at+remaining及light_hold_until不晚于初始取样截止。
2. sync_open ACK未知中止：发送前保留open_id/unknown；明确拒绝终止，未知转认证HTTP。sleep候选必须证明同wake/plan的WIFI_SYNC_ONCE；正式light与sync_open ACK lost组合通过，不能仅凭超时假定开窗。
3. 同次启动第二轮/旧计划：每轮清planReceived与Wi-Fi阶段，HTTP listener单独记录；旧sleep不收新轮。更短计划可收紧已经开始的HTTP阶段，sleep可提前收尾。
4. 取消安全：recovery在内部预算内结束并最终落盘；sent保持awaiting_confirmation，unsent退回waiting_rendezvous。automatic内部超时标为awaiting_confirmation，不留下虚假connecting。
5. 临时窗三入口统一：sync_open、正式light到期、provisional到期均设置开窗时间；状态给remaining_ms，设备执行90s硬限，Bridge按取样剩余消费。

Astra 最终只读结论：本轮审核发现已收口，无剩余阻断项；未操作设备、未独立重跑Lead全部测试。

## 最终验证与产物

- core/BLE/render：197 passed；`artifacts/bridge-macos/rendezvous-core-final.log`。
- app：63 passed、0 ignored，包含原诊断归档ignored测试；`rendezvous-app-final.log`。
- device-sim bootstrap：串行50 passed；`rendezvous-sim-final.log`。
- Rust合计310 passed、0 failed、0 ignored。另生产C++生命周期/证明专测通过（`tools/test-rendezvous-protocol.cpp`）。
- 首次额外启用旧诊断测试失败，是夹具缺INFO连接且把transport acceptance当业务ACK；按现行INFO→write acceptance→Status读回更新后独立及全套通过。不是生产故障red。
- 新能力字段保持16项compact摘要，handoff ACK复用既有编码；最终共享编码属性边界回归4 passed，最大status 仍≤512 B、handoff ACK applied387/rejected415 B，见 `rendezvous-wire-final.log`。
- Note4 `zectrix-note4-b` 构建成功，版本marker `0.18.46-note4-b-rv1`、目标marker `zectrix-note4-400x300`，候选 `artifacts/bridge-macos/note4-0.18.46-rv1.bin`，1,723,504 B，SHA256 `9BE3591DBA265DC57B595EF43197A0044431668D5740937C00724C6EA01A756D`；匹配ELF一并保留。日志 `rendezvous-note4-final.log`。未构建1.54/变体。
- Bridge生产候选构建成功；`rendezvous-bridge-build.log`。未覆盖已停止的生产.app或运行数据。候选 `artifacts/bridge-macos/bridge-app-rv1`，75,023,304 B，SHA256 `4C533D80E6B3A15BE3087A0FA8A50F67066BC57CBB0824AE9E5C9B65C1FCA888`；构建基于共享工作树（含既有改动），不是独立部署包。候选manifest：`artifacts/bridge-macos/rendezvous-candidate-manifest.json`。

## 未闭环的现场

这些仍记在backlog B6，而非本轮源码阻断：

- 两端候选尚未配套部署；旧ROM不支持“HTTP实证后关BLE”的新顺序。真实机仍Note4 .45，生产Bridge保持02:50停机现场；8765/8766没有监听者。
- 同wake实机证明BLE存活时认证HTTP可达、handoff后控制器关闭、随后PM light；电池/USB/物理恢复各连续多轮完整读取与自然会合，慢入网/换IP/不可达/ACK丢失矩阵。
- 20s BLE/Wi-Fi共存窗口的RF、堆内存、长时功耗、panic转储与匹配ELF验收。软件模拟没有替代NimBLE/ESP32调度。
- 部署前后ACK统计只证明时间相关性；“先放锁导致上一链路未拆净”的假设仍需A/B和真实自然断链/再连观察，不能宣称此次已证实或已解决该生产回归。
- Windows、1.54及其他CPU架构的现场仍未验。

自动审批曾拒绝无owner直接handoff方案，因为会放宽显式claim/lease门禁。该命令未执行；最终保留严格owner一致性，HTTP实证后走既有token-gated显式claim（不force），其他owner阻断组合通过，无待批准动作。
