# Fake ROM、多设备 Bridge 与独立实验时钟实施计划

日期：2026-09-24。状态：执行中。本计划采用 `docs/fake-rom-simulator-design.md` 的同源 ROM 方案，并以本节的多时钟合同修正其中“Bridge 和设备共用一个逻辑时钟”的表述。当前没有可运行的 Fake ROM；`core/tests/v2_client.rs` 的 FakeDevice 只是 HTTP 契约夹具。

## 已定决策

1. 一个 `device-sim` 进程模拟一台设备；多台设备使用多个进程、独立虚构 MAC、loopback 端口和数据目录。暂不把 C++ 全局状态改为进程内多实例。
2. 一个生产 Bridge 实例管理多台真实或 fake 设备。运行目标、认证材料、owner/claim、状态缓存、调度、发布任务和时钟上下文均按规范化 MAC 隔离。现有 `PlatformService` 按 MAC 的 coordinator 和设备记录继续复用。
3. 每个 fake 目标有两条独立时钟视图：设备侧时钟与 Bridge 对该 MAC 使用的时钟。两条视图分别配置倍率、epoch 偏移和漂移；相同参数给出同步场景，不同参数给出失步场景。其他真实设备继续使用真实时钟。时间视图的配置只属于测试运行环境，不写入生产 `platform/state.json`。
4. 每条视图区分跨重启单调实验时间、单次启动 uptime、可跳变的 wall epoch。PowerPlan、owner lease、会合和事务截止最终使用各自一侧的单调时间；`server_time`、屏幕时钟和观测时间使用 wall epoch。现有 Bridge 持久状态有 epoch 秒截止字段，阶段 A 只显式传参、保持旧格式和生产行为；阶段 E 才处理单调截止与旧持久状态的迁移/重启恢复，不能直接把旧 epoch 数字解释为 uptime。校时不能续租或移动单调截止。Bridge 的计划镜像只是观察值，设备本地截止仍是权威。
5. `Nx` 的数学定义为锚点后的虚拟增量等于宿主 monotonic 增量乘倍率；偏移只作用于 wall epoch，漂移可使两侧倍率不同。改倍率先结算旧锚点，单调时间不得倒退。`step/max` 通过显式的按 MAC 场景驱动与在途 I/O 屏障协调，不能靠缩短 `sleep` 或扫描超时实现。
6. 真正的 TCP/GATT/Windows 操作保留有限真实时间 watchdog；协议中的租约、定时器和 fake 链路等待按目标逻辑时间。超时结果须标明逻辑超时还是宿主 I/O 失败。
7. `release` worktree 可隔离源码开发，但不是多设备运行模型。联测只启动一个 Bridge；端口、数据目录、单实例插件和 UDP 发现冲突不靠复制第二个生产 Bridge 解决。fake 端点不接触真实 MAC、token、Codex 源或现场设备。
8. 当前现场设备均为 v2；后续多设备验收只构造 v2 目标。代码内历史 `legacy` 标记继续作为协议路径防护，但不把不存在的 legacy 设备写入本轮场景或测试。

## 路由与失步语义

- 设备运行记录以 MAC 关联 `host:port` 端点；`ip` 不承担身份判断。fake 身份由测试配置显式登记为该 MAC 的 loopback 目标，不能因为地址是 `127.0.0.1` 就自动启用虚拟时间。Bridge 对返回的 MAC/target 仍按真实设备规则校验。
- 数据源采集本身是 Bridge 级过程；每台设备的可见字段、质量、过期和 full-sync 判定在该设备的交付决策中使用目标时间。测试 fake 数据优先用已有 Static 源，不让模拟器调用真实 Codex。
- 倍率、偏移、漂移不一致时，双方不强求截止时刻相同。设备返回的剩余秒数和 ACK 是 Bridge 重新估算的依据；错过会合窗口、租约提前到期、401/409 或待重试必须可观察，不能用自动延长 PowerPlan 或隐式 claim 掩盖。`server_time` 可以校正设备 wall epoch，但不得改变任一单调截止。
- 同一 fake MAC 的 Bridge 时钟可与设备时钟分别设置；两个 fake MAC 也可使用完全不同参数。`step/max` 驱动每个目标有明确的时域 ID、推进目标和 I/O 屏障，禁止一台设备推进时暗中推进另一台的截止。

## 实施顺序与验收门槛

| 阶段 | 负责范围 | 完成条件 |
|---|---|---|
| A. 行为基线与显式时间 | 固定现有单设备协议行为；把 coordinator/service 中隐藏的 `now_secs()` 逐个改成调用者传入的目标时间，生产调用仍传真实时间 | 现有测试通过；两个 MAC 可传入不同 now，互不改变 full sync、job/plan 时间；不修改协议或持久格式 |
| B. Bridge 多设备运行层 | 把 app 的单个目标、缓存、占用、续约、BLE 机会和 HTTP 投递改为按 MAC 运行记录；发现结果核对 MAC 后写对应记录 | 一实例同时保留 1.54、Note4 和至少两台虚构设备；没有 MAC 的多设备请求明确拒绝；状态、token、作业和 ACK 不串 |
| C. Fake ROM 同源入口 | 从 `main.cpp` 纵向抽取实际 v2 命令/状态路径，使固件与宿主编译同一份业务 C++；先 Data，再 Plan/Bundle/Activate/claim | 宿主结果来自 ROM 路径；现有固件和 render 测试不退化；未覆盖功能显式报告 unsupported |
| D. 单设备模拟进程 | localhost HTTP、fake BLE、独立文件存储/RTC、显示效果、控制面与 1x/step/Nx；每进程一台 | 两个进程同跑，MAC/端口/存储/owner 独立；断电重启保留规则、鉴权与 ACK 有端到端证据 |
| E. Bridge 目标时钟接线 | 按 MAC 选择生产时钟或实验时钟；从 app 调度、coordinator、service 到 BLE 命令时间戳使用同一目标视图；fake 链路逻辑等待可推进 | 真实设备仍按真实秒运行；fake A/B 可用不同倍率、偏移、漂移；切换倍率和校时不延长租约或改变已确认指纹 |
| F. 联合场景 | 1x、Nx、step/max；同步与失步、丢 ACK、离线会合、Bridge/设备重启、多设备交错 | 同倍率场景同步；异倍率/偏移场景明确呈现处理结果；每个 MAC 的 seq、deadline、Profile/Bundle、owner 独立；step/max 在 I/O 在途时不跳时 |
| G. 实机回归 | 只在联测通过后对两台真实设备验证读取、投递和显式发布 | 真实 1.54/Note4 路径不回归；不把模拟无线/显示结果当作硬件性能证据 |

A 的公共时间入口是 B 和 E 的前置。**开发严格串行**：先完成并评审 B，再开始 C；C 验收后做 D，之后依次 E、F、G。同一时间只委托一名 6-luna high 编码代理；主代理在每个 task 开始前作出设计和验收决策，编码代理不自行改变合同。取消 B 与 C/D 的并行分工，也不建立用于并行开发的 release worktree。已有其他代理的 ROM 改动不属于本计划，不触其文件，直至其工作结束并完成交接。

## 首轮任务与约束

`task-1-coordinator-time.md` 已完成并提交为 `cc6a6c1`。多设备运行层的 `task-2-target-mac-guard.md` 至 `task-8-v2-cycle.md` 已通过宿主测试。其后完成阶段 B 的 BLE/发现/验收，再串行开始 Fake ROM 与独立时钟；不让编码代理自行补设计。除用户明确要求提交的里程碑外，不自动提交；不启动、停止或改动正在运行的 Bridge，不访问实机，不写生产数据目录。每次里程碑在 `PROGRESS.md` 更新证据与待办。
