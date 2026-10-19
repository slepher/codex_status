# 协议评审核验与重构方案

日期：2026-10-19。源码基线：`4868c8c`，开始核验时工作区干净。关联现有 backlog B6/C8、protocol-redesign 的 P3/P5，不新增平行待办。

本轮为静态调用链核验与设计；未运行测试、构建、访问设备、启停 Bridge、修改实现或提交。以下“可构造路径”不表示已在实机复现。现场版本仍以 PROGRESS.md 为准。

## 1. 总体判断

原评审指出的状态混用和收敛方向大体正确，但不能原样作为实施指令。它夸大了锁问题的范围、把已有类型与共享执行核心漏算，并把消息关联和业务幂等视作可一并删除的重复字段。

| 原判断 | 核验结论 |
|---|---|
| 会合用 13 个私有 JSON 字段编排 | 成立；统一执行器尚未覆盖全部业务，但 Wi-Fi 共用执行核心已存在 |
| Wi-Fi 三轨状态、预算耗尽后槽位泄漏 | 状态职责混用和自动回收缺口成立；不是每次未知都必占设备槽，也不是完全没有新写围栏 |
| 48 B 帧头只叠层、可直接删除内层身份 | 重复字段存在；“直接删除”不成立，外层会话尚不能等价代替所有认证及业务身份 |
| 两处锁倒置导致死锁、同步 HTTP 留僵尸 | ble_cycle 全局队头阻塞成立；recover_save 不等待，未证实锁环死锁；worker 有总截止 |
| 全链路只有 String、pending 被当错误 | app 边界类型信息丢失成立；core 已有 OperationReply/CommandFailure，不应从零重造 |

## 2. 逐项证据与修正

### 2.1 会合编排：成立，但不是尚无共享核心

`bridge/crates/app/src/rendezvous.rs:128–320,342–482` 写入并读取 13 个不同的 `_` 字段，混合设备原始观察、本轮意图、Plan/SyncOpen 确认和 HTTP 证据。字段缺失被 Value 默认行为隐去，类型系统无法约束状态组合。

但 `bridge/crates/core/src/platform/service.rs:668` 的 `wifi_attempt_async` 已被 HTTP/BLE 使用；`bridge/crates/app/src/wifi.rs:697,713` 已有 BLE Transport 适配。`protocol-redesign-status.md` 的 2026-10-13 01:51 记录也明确“共用异步业务核心”已经落地、全业务执行器仍未完成。

结论：提取本轮强类型上下文并继续收敛调用入口合理；新建与 wifi_attempt_async 平行的通用 TransactionTracker 会增加第二份权威状态，应避免。Plan、Data、Bundle、Wi-Fi 的提交证明不同，不能为了“统一”强行共享一个业务终态枚举。

### 2.2 Wi-Fi 事务与容量：自动收尾缺口成立，原因需精确描述

可从源码构造如下路径：

1. 原写已发且 `may_have_committed=true`；后续确认失败。
2. `wifi.rs:515–540` 在 begin 计确认次数，finish/reconcile_execution 在三次预算后置 `failed/confirmation_budget_exhausted`（`:328–342`）。当前源码不是评审所说的 record_confirmation_attempt 函数。
3. `runnable` 排除终态，`wifi_attempt_async` 必经 wifi_begin，因此普通执行驱动不再查询该历史 ID。
4. HTTP ACK 扫描 `app/wifi.rs:582` 与 BLE ACK 扫描 `:757–772` 只挑 `receipt_ack_pending=true`；预算耗尽本身不会设置此标志。
5. `safe_to_retire`（`core/platform/wifi.rs:311`）又要求已知结果且 ACK 完成。queue 的容量回收（`:494–505`，当前没有 prune_jobs 函数）保留这类未知记录，32 条均不可退役时返回 wifi_job_capacity。
6. **如果**设备已有原事务回执，`src/wifi_management.inc:91–118` 要显式 result_ack 才删除；apply/activate 分别在 `:154–155`、`src/wifi_setup.cpp:450` 执行 8 槽容量限制。因此持续发生可导致设备 result_capacity；未实际送达、发送前保守记 sent、设备未落回执时不能推断占了一个设备槽。

这是“停止用户任务后，没有独立的历史对账/回执退役路径”，不是应该让所有历史任务永远继续执行。已有取消/过期/三次预算行为是 10 月 19 日有意实现并留有回归的合同，不能简单撤销。

“永久”应限定为**当前正常自动调度无法自行闭环**；迟到在途确认、未来显式修复可以改变记录。固件对 pending/result_unknown 回执也可能拒绝退役，单加 Bridge ACK 循环不能覆盖全部情况。

“放行无围栏新事务”不准确：现有认证、owner、原 ID 内容摘要、principal/namespace 高水位、apply expected_revision、activate version 和 running/busy 仍在。它们是否足以证明某个历史未知与某个新意图不冲突，必须按操作验证，不能凭 `terminal()` 一刀切，也不能凭“历史未知”一刀切全设备禁写。

原建议中的普通认证 status/revision **不能普遍证明原事务完成或未发生**，更不能自动释放回执。应使用原 TransactionKey 查询的结果或协议明确规定的充分证明；当前同值/同版本不等于知道谁写的。原文的 `wifi_Transaction_Status` 也不是本次找到的现行字段；不能按该名称设计客户端。

状态“三轨”也应拆开看：Connection 包含 requested 用户意图、attempts/backoff 策略、RF 观察和 HTTP 证明，确实职责混合；但 INPUTS 是临时敏感输入，SCHEMES 是缓存，OPERATIONS 主要覆盖 add/无缓存能力的入口，不是三份完全等价的事务账本。把三者全部改为 jobs+status 投影会丢失无法从设备观察推导的本地意图与策略。

### 2.3 帧头与 JSON：有冗余，不能删掉业务身份或弱化认证

`src/firmware/transport.cpp:431–513,633–718` 与 `src/v2_command_envelope.cpp:70–87` 证明存在两层 session/context/op/关联校验。`src/wifi_management.inc:239–246` 确有 wifi_ 前缀、command、save→apply 归一化。

但以下概念不同：

- frame request_no：单次消息交换/同会话重放围栏。
- Wi-Fi request_id（w3 序号）：跨连接、跨重启的业务身份；原写 JSON 的 request_id 本身就是该 ID，不能当“重复关联号”统一删除。
- plan_id、data seq：业务顺序/恢复语义，不由消息号替代。
- endpoint token、BLE peer/principal、owner：权限，不由随机 session_id 替代。

原评审把 HTTP session_id 描述成 Bridge 随机生成也不完整：`core/device_client.rs:225–278` 发送的是 opening nonce，随后保存响应 session；`transport.cpp:479–483` 的实际 HTTP session 由设备生成。HTTP 与 BLE 的开会话方式不同，不等于多余。

可行方向是入口先构建经过验证的 CommandContext，再让业务 handler 消费它；只有证明现有安全不变量均由这个上下文覆盖后才删重复 wire 字段。Wi-Fi 持久 ID、namespace、内容摘要、高水位和结果 ACK 必须保留。当前设计明确不维护双协议/旧格式降级，最终改 wire 需要 Bridge、固件、Fake/协议测试配套交付。

### 2.4 锁与 HTTP：队头阻塞成立，死锁与僵尸说法过强

- `app/rendezvous.rs:61–74`：全局 BLE 锁→connect_any→设备锁 await。持连接等 HTTP 释放设备锁，确实消耗 RF 机会并拖住其他设备；10 秒 command budget 到 `:93` 才创建，未覆盖前面的等待。
- `app/wifi.rs:139–148`：两个 try_lock 都在 connect 之前，设备锁失败会立即返回并 drop 全局 guard。顺序不符合设计，但该路径不存在评审描述的等待。
- 原评审漏掉 `authorize`（`:156–157`）和 `run`（`:222–224`）：同样持全局锁 await 设备锁，虽发生在连接前，仍拖住整个 BLE 调度。
- 当前这些证据证明 convoy/队头阻塞，不证明存在相反等待边组成的永久死锁；不能标为已复现死锁。
- `core/device_client.rs:74–154` 有 connect_timeout、逐次读写剩余时间和绝对总截止。外层取消后 worker 继续运行是事实，称为无限僵尸不准确。
- `app/platform.rs:40–75` 保留 worker 的设备执行许可是必要安全措施：否则迟到写会与下一操作并发。设计 §3.1 明确允许同步过渡实现采取此措施；不能先删许可再迁移 I/O。

异步 socket 可以减少取消后的本地在途占用，但 Drop 本地 socket 不能撤销设备已经收到的命令，也不能取消 CoreBluetooth 原生工作；原事务 Unknown 和 NativeLease/代际隔离仍须保留。

### 2.5 类型化结果：应修 app 边界，复用已有类型

`core/platform/wifi.rs:36–73` 已有 ReplySource 与 OperationReply，区分原写和结果查询。`core/rendezvous.rs:52–77` 已有 DispatchOutcome/CommandFailure。`service.rs:715–725` 的 accepted 实际先被正常解码，再落为 awaiting_confirmation + error_code=operation_pending。

类型丢失出现在持久作业投影及 app：`app/wifi.rs:513`、`app/rendezvous.rs:245` 对 operation_pending 字符串特判。问题不是所有底层接口直接 Err(operation_pending)，而是正常 pending 通过 error_code 回传后又被上层当错误。

应扩展已有 OperationReply 以区分 AcceptedPending 与 Unknown，并向上保留结果。不要只用一个五分支枚举替代全部事实：此次查询 NotSent 可以与原事务 Unknown 同时成立；close 失败也可以与原事务 Committed 同时成立。

## 3. 建议重构顺序与验收

以下是现有 B6/C8 的实施拆分建议，不表示已经实施或另立待办源。

### R1：统一执行许可顺序，先消除全局队头阻塞

范围：app/rendezvous.rs、app/wifi.rs、platform::DeliveryLock 与 ble::DeviceConnection 发现/连接边界。

已知 MAC 的 authorize/run/recover_save 均先获得设备许可，再等 BLE 许可；所有等待使用同一个绝对截止。自动 connect_any 当前在连接后才返回匹配 MAC，不能仅交换两行：把发现候选与连接拆开，发现阶段只持扫描许可，释放后按候选 MAC try_acquire 设备许可，再取 BLE 许可连接并重新认证 MAC。忙设备跳过并保留后续公平机会；不得同时预占所有候选设备锁，也不得持全局锁等待某一设备。

验收：A 的 HTTP 被阻塞时 B 仍能会合；A 不先建 BLE 再排队；发现后身份变化拒绝；等待超时不发命令；取消/建链失败后下一机会继续；不得通过延长物理 RF/PowerPlan 期限解决。

### R2：分离执行终点与事务收尾义务

范围：core/platform/wifi.rs、service.rs、app/wifi.rs 的共同 HTTP/BLE 维护路径。先做 Bridge 内部变更，设备格式不顺带修改。

保留有限执行和取消/过期语义。把“执行已结束”“原结果 Accepted/Unknown/Committed/Rejected”“回执需查询/需 ACK/已退役/需人工处理”分开；同一持久 Job 仍是事实源，不复制到第二个账本。

已认证且身份匹配的联系机会可以为历史记录分配很小的只读维护预算，查询原 ID→先持久化证明→ACK→确认 retired→允许回收。维护不自动重发原写，不重新激活用户取消任务，不因为查询需求自动唤醒/续 Light。预算耗尽后显示结果未知及明确恢复入口；维护尝试也需有限策略，不恢复无期限的前台“待确认”。

新写准入由共享函数按旧事务作用范围、是否可能仍在执行、原身份/高水位、修订与命令语义判断。保留已核实 active 网络无配置写的重连路径；不以任意 status 成功清空未知。无法证明安全时返回结构化阻塞及操作指引。

身份丢失、设备永久 unknown、retired 但缺原结果都不能伪造成功。保留审计事实；若要将不可恢复历史移出 32 个活动槽，需另外定义有界归档与最小围栏表示及迁移测试，不能改 safe_to_retire 就删除。设备未知回执如何退役属于独立协议设计，未定义前保留 result_capacity 的真实失败。

验收：设备实际提交而确认连续丢失，预算结束后重新联系可按原 ID 收尾；ACK 丢失/Bridge 崩溃重启仍可退役；取消后迟到成功只更新结果，不重发；未送达不假定占设备槽；8 回执与 32 Job 压力明确区分可回收和永久未知；namespace/principal 变化零越权；冲突新写有反例测试。

### R3：沿已有业务核心保留类型与分离投影

先使 accepted 成为正常类型分支，删除 operation_pending 错误特判；发送证据、业务结果、下一动作分开。持久格式若变更需显式迁移，保留旧 ID、namespace、sent 和确认事实；公开错误码只在 UI/MCP 边界转换。

Connection 拆职责：保留无法推导的本地连接意图和退避策略；设备观察带 boot/采样时间；HTTP 联系证据独立记账。对外 connection/automatic_state 由这些事实与 jobs 纯函数投影。INPUTS 保留受控 RAM 生命周期并移入 AppCtx 所有权；OPERATIONS 中确属 jobs 的 trial/activate 路径删除，add/权限操作按自身用途保留；SCHEMES 在确认所有消费者后合并到已有缓存。

验收：准入失败不变成 RF 故障；过期观察不冒充在线；重启保留用户 requested 意图和退避；AcceptedPending 可继续合法 network_start，未知 Plan 不放行新控制写；HTTP/BLE、UI/MCP 得到相同业务结果。

### R4：将会合临时状态从设备 JSON 移出，逐步统一驱动

引入一个本轮 RendezvousContext：设备观察、已冻结工作集、绝对预算、Plan/SyncOpen 证据和网络动作结果。删除全部 13 个 `_` 键；先保留原顺序和行为，用已有 Fake 会合场景验证，再将决定下一动作的逻辑提成纯函数。

复用 wifi_attempt_async；共用预算/许可/载体证据，不强行把 Data/Plan/Bundle 的不同提交合同装进新的通用事务框架。临时上下文不落进设备 observation 缓存。

验收：所有原会合场景、预算耗尽、Plan ACK 丢失、SyncOpen 未知、network_start pending、完整/临时 Data 与诊断先后顺序；读取、claim、传输均不续 PowerPlan。

### R5：HTTP 原生异步化，最后再精简 wire

HTTP 迁移保留现有编码/解析与业务合同，改用异步 I/O，并一并迁移跨 await 的 session/status 锁，覆盖连接、会话等待、写入、读取的同一截止。先处理调用面和取消证据，不仅机械替换 TcpStream。只有所有相关 worker 写路径消失后才删除 DeliveryLock 的 worker 许可机制；原生 BLE 隔离不随之删除。

随后独立实施 CommandContext 入口收敛和 wire 字段删除；枚举每个字段原本保护的不变量，明确新的验证位置。保留持久事务 ID、版本/修订和 token/owner；统一命令枚举，消除内部别名。Bridge、固件、Fake 同步改当前格式，软件不匹配明拒，不新增旧协议兼容层。

验收：零字节/部分写/完整发送后取消、断流与截断、session 过期、同号异内容、跨设备/跨 principal/跨 namespace、重启原 ID 查询、BLE 分片/512 B 属性、canonical 编码。固件变更按空/单/典型/上限列 NVS/RAM/栈预算并使用匹配 ELF；宿主通过不代替 RF/掉电/真实取消验收。

## 4. 实施边界

建议先执行 R1 与 R2 的最小可验证切片，再 R3/R4，最后做异步 I/O 与 wire 改动；每片都能独立审阅，避免同时改变调度、持久数据和设备协议导致无法归因。

实施前在现有专项更新 plan/task/status/review；运行数据迁移有冻结备份和回滚版本约束。UI 变更需项目规定的 Node、Wi-Fi UI 与窄宽截图验收。构建部署遵守现行操作细则；本次方案本身不改变已部署软件或宣称 B6/C8 完成。
