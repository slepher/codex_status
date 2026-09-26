# 休眠设备状态与延后操作：技术设计

日期：2026-09-26；状态：实施中。以下「现状」为动手前代码核实，「方案」为目标合同；实施差异与验证见 task.md / review.md。相对源码路径以仓库根为基准，行号是设计时阅读位置。

## 2026-09-26 Q1–Q3 决策

- Q1：每 MAC 只保留一个未终态 OTA 和一个未终态发布。再次提交同类任务返回冲突；先显式取消确定未开始的 queued/waiting 任务再提交。`Sending/Unknown/awaiting_confirmation` 不允许取消为“已撤回”或覆盖。当前两步取消/新建之间仍可能有其他会合，不能声称原子替换。
- Q2：OTA 与发布各可排一项，按 `created_at` 串行；同秒以 OTA 优先。进入下一项前重新认证、占用和 PowerPlan 检查。另一个任务不会被隐式取消。
- Q3：只有精确运行镜像身份才能终态 `image_verified`。当前 ROM 未提供可与完整上传文件哈希直接比较的身份；`UPDATE OK`、版本变化只记录 `upload_ack` / `version_observed`，状态保留 `awaiting_confirmation`，绝不自动重刷。同版本或上传前版本未知仅记 `version_seen_unproven`。本轮只增加认证状态 `fw` 与镜像内目标/版本 marker，不把二者冒充精确镜像证明。
- 实机补充：`awaiting_confirmation` 若已有上传 ACK 且上传后同 MAC 认证状态读到预期版本，可解除对后续独立模板/数据业务的串行阻塞；OTA 自身仍待精确确认，仍不允许同 MAC 再排 OTA 或自动重刷。没有这两项证据时继续阻塞后续写入；下一项仍需新认证、owner 与正式 PowerPlan 检查。

## 1. 事实与约束

| 现状 | 证据 |
|---|---|
| MAC 解析接受持久登记设备，但设备页只取进程内记录，缺失直接报错；已有按需初始化助手 | `bridge/crates/app/src/main.rs:470,523,2180`；错误 `has no runtime record` 是程序缺口 |
| 普通状态及 v2 完整缓存都在内存；v2 离线时可保留旧 body，但 fetched_at 混合尝试/成功语义 | `bridge/crates/app/src/main.rs:48,83,96`；`bridge/crates/app/src/device_runtime.rs` |
| 平台已有持久设备 observed 摘要、Bundle jobs、context、seq、plans；不是完全没有状态持久化 | `bridge/crates/core/src/platform/service.rs:47,1207`；`model.rs:707`，observed 不等同设备页完整快照 |
| 模板发布已冻结并持久排队；入口仍立即尝试 deliver，返回 deferred；重启把 Sending 转回 Waiting | `bridge/crates/app/src/platform.rs:607,647,700`；`core/src/platform/service.rs:163` |
| OTA 部分 MCP 失败路径才排队，存 ROM 路径到每 MAC 内存，但唤起信号是全局，worker 再取 selected_mac | `bridge/crates/app/src/main.rs:1125,3435,3477`；选择变化和重启会破坏任务归属/存续 |
| worker 传 mac，底层 OTA 核对参数名为 device_mac；当前成功判断是 60s 内版本变化 | `bridge/crates/app/src/main.rs:3483`；`bridge/crates/mcp/src/lib.rs:366,399,475`；须统一接口，不能假定 mac 已生效 |
| `/v2/status` 用 endpoint token，OTA/claim 用设备操作 token；设备 token 在 NVS；上传返回 UPDATE OK 后延迟重启，并设置 post-OTA 窗口 | `src/main.cpp:3575,4087,4707,4748`；两个 token 不可混用 |
| timer wake 执行有界网络拉取/BLE 会合，并非长期 HTTP 在线 | `src/main.cpp:5069,5663`；旧 pull pending 分支见 `5160` |

以 `docs/generic-display-platform-design-v2.md` §§2、4、7、8、10、11 为准：每设备串行事务、显式发布、Bridge 唯一正式 PowerPlan、业务 ACK 才确认、MAC 身份与 owner 校验。`docs/power-state.md` §§9.1/9.2/13.3/13.4 提供历史发现和延后投递背景；其旧 profile_push、隐式保活、旧固件兼容文案不作为本设计合同。bridge_first 仍是候选，不能把它当现成唤醒通道。

## 2. 状态数据合同

在现有 DeviceRecord/平台持久状态增加有界 `last_success`，不保存任意无限原始 JSON：

- `device_mac, observed_at`（Bridge UTC）、`transport`、`schema_version`、认证来源；设备页所需字段及各字段实际采样时间，含固件、battery、active/template/context、display_state、owner、power 与下一会合线索。
- 只有 MAC 匹配且完成认证的通信可推进 `last_authenticated_contact_at`；它不自动推进未在该次收到的字段时间。BLE ACK 不冒充完整 HTTP 状态。公共 `/status.json`、UDP、ARP 只提供候选线索；需要认证合同的字段必须来自认证响应，必要时在 `src/v2_status_snapshot.*` 补字段。
- `last_attempt={at,transport,outcome,error_code}` 与 `reachability={state,reason,expected_contact_at}` 独立。失败不清空或改写 last_success，不把 last_attempt 时间显示成「更新于」。设备页直接读本地，无阻塞网络请求。
- `reachability.state`：`unknown / recently_authenticated / expected_sleep / overdue / blocked`。最近认证仅说明过去的可达证据；旧快照中的 owner、power remaining 不能当当前权限/期限。会合估计只显示预计，不承诺准点在线；缺少可靠计划时显示 unknown，不武断判断休眠。超期只说明未联系，不能证明硬件故障。
- `age_s=max(0,now-observed_at)`；时钟倒退注明时间异常，进程内超时使用单调时间。无快照显示「尚无成功通信记录」，保留登记 MAC/名称。已登记缺 runtime 时从平台初始化，不凭缓存制造在线。

持久化沿用 `platform/store.rs` 原子替换。建议快照变化及受控时间检查点写盘（复用现有节流思路）；UI 显示实际保留下来的时间，绝不编造重启前最后一刻。磁盘失败保留内存值并显示 persistence_error；任务未持久成功不得返回 queued。快照历史不无限追加。重启加载快照后 reachability=unknown，再依据仍有效的会合线索标注预期，旧认证会话不可恢复为可写会话。

## 3. OTA 与发布任务合同

保留现有 PublishJob、frozen_bundle 与其 job_id；新增小型持久 `OtaJob`，挂在现有 PlatformService，不引入第二套任务框架：

`job_id, device_mac, bridge_id, created_at, updated_at, state, reason, attempt_count, last_attempt_at, frozen_rom{relative_blob_path,sha256,size,firmware_target,expected_version,image_identity?}, upload_ack?, confirmation?, last_error?`。

源 ROM 在入队时读取并验证，然后复制到 `<实例 data>/platform/ota/` 的不可变内容文件；任务只引用该副本。先完整写文件并计算哈希，再原子保存任务，成功后返回 job_id/queued。上传前重验副本 hash/长度/格式/目标及新认证设备能力，源路径后来修改/删除不影响队列。不得仅凭文件名推断硬件目标；无法可靠识别目标时拒绝入队并说明缺少元数据，不向设备试刷。终态后清理无引用副本，保留有界结果摘要；启动只清理无引用临时/孤儿文件，不能删活跃 ROM。文件与任务不含 token。

所有 UI/MCP OTA 路径进入同一 app service，MAC 入队时冻结。未给 MAC 仅允许唯一登记设备；多台时拒绝，不读 UI 选择或 worker selected_mac。直接端口/IP参数不能绕过目标 MAC 认证。重复提交相同 request_id 返回原 job；相同键不同内容冲突，重启后仍成立。

| 状态/显示 | 进入条件与后继 |
|---|---|
| queued / 等待下一次联系 | 已持久冻结；无法通信只更新等待原因，不算任务失败 |
| transferring / 正在传输 | 认证、owner 与计划满足后开始真实写入；尝试前持久记账 |
| awaiting_confirmation / 等待设备确认 | 已发出可能提交的操作，ACK丢失或 OTA 已上传等待重启；禁止盲目重传 |
| succeeded / 已确认 | 发布业务 ACK 或认证 committed_job_id 与目标一致；OTA 满足所声明的确认等级 |
| failed / 失败 | 明确拒绝/坏镜像/目标不匹配/不可恢复本地错误，保留原因 |
| cancelled / 已取消 | 确定尚未产生远端副作用且移除队列成功；不是回滚承诺 |

发布的现有 `Waiting/Sending/Unknown` 分别映射前三类，不必为了 UI 新增第二套枚举。`blocked` 是待处理原因（认证、owner、能力），不当休眠也不循环写入。传输尚未可能提交的中断可退回 queued；不能证明时进入待确认。取消进行中/待确认任务只能停止未来尝试并记录 cancel_requested，先对账，再显示「已应用，停止后续操作」或确认未应用后 cancelled，不能宣称撤回已提交包。

**容量与替换待决策 Q1：**建议每 MAC 至多一个未终态 OTA，保留一个未终态 PublishJob；第二次同类提交返回冲突并提供显式替换操作。只允许替换确定未开始的 queued 任务，原任务留下 cancelled/replaced_by，新任务完整冻结后原子替换。当前 Bundle Waiting 可直接被新发布替换（`core/src/coordinator.rs:555`）；是否保留该交互，还是统一为显式 replace，需定案。Unknown/传输中的任务不得覆盖。

**排序待决策 Q2：**OTA 与发布能否同时排队？建议允许各一项，按 created_at 串行；开始下一项前重新认证和检查能力/上下文。也可选同 MAC 全部重操作只容纳一项。不得私自设定「OTA 永远抢先」，不得取消另一个已有授权任务。两 MAC 相互隔离；当前 app `v2_delivery` 为全局锁（`platform.rs:649`），改为每 MAC 事务锁，共享 BLE 仍有界调度。

## 4. 会合与执行顺序

1. 自然 timer/BOOT 会合或已有在线链路到达；公开 announce/pull hint 只触发候选检查，不标业务成功。路由到消息对应的完整 MAC，不能全局 contact_generation 唤起错误设备。
2. 完成当前通道认证，读取真实 context、job、owner、能力及电源状态；先对账上次未知提交，再选本 MAC 已授权任务。无任务不额外开网。
3. 若大任务需要 Wi-Fi，在既有认证 BLE 会合发送正式、有限 PowerPlan，再等待 HTTP 认证；HTTP 已可用时同样按计划剩余预算判断能否交付。窗口不足留到下次；禁止无限重连或自动制造新 300s。空闲 owner 的 bootstrap 遵循 v2 §12：有限计划取得 HTTP 后显式 token claim，成功才写业务；他人 owner/本地 yielded 停止等待，不强占。
4. 重验 ROM/Bundle 与目标，持久 transferring，串行发送；发布、OTA、Activate 及有冲突的数据写不能交错。同 MAC owner 续约不改电源期限。OTA 传输锁避免半途睡眠但不能变成业务 lease；锁和失败清理保持有界。
5. 接收业务结果并落盘。OTA 重启后重新发现及认证同 MAC；IP 可变。原 nonce、light/owner 剩余值不可重用；新正式 PowerPlan 以设备实际窗口和 v2 规则协调，不因状态读取而延长。

旧 pull `pending.ota` 仅作本 MAC 的待办提示；不能继续用全局布尔值给另一设备保活，也不能绕过正式 PowerPlan。本方案不要求实现 bridge_first 或修改正常 timer 日程。

## 5. ACK、版本和镜像确认

发布沿用 committed_job_id/完整包校验与 active_context：ACK 丢失后先认证读状态；同 job 幂等恢复，不生成新包 ID 碰运气。应用成功与 `display_state=failed/pending` 分开，不能把「已安装」写成「已显示」。

OTA 上传 `UPDATE OK` 仅表示上传端接受，不证明新镜像已运行。当前 `mcp/src/lib.rs:475` 用版本变化判成功；方案首先改为同 MAC 重启后认证观察 **预期版本**，报告 `confirmation.level=version_observed`，不能只看到任意不同版本就成功。同版本重刷无法用版本证明；丢 ACK、设备继续休眠或 60s 超时保留待确认，不因此自动再刷。

**精确镜像确认是新增能力，不是当前 ROM 承诺。**建议扩展认证 v2 状态报告运行镜像身份与明确算法/长度，例如与冻结镜像可对照的 ESP image digest。文件 SHA256（传输文件全部字节）与运行分区摘要可能算法/覆盖长度不同，须分别命名并用解析夹具证明一致性，不能直接比较名称相似的哈希。新 ROM 能力可用后同 MAC + 运行目标 + 预期镜像身份匹配才给 `image_verified`；版本只是展示字段。

**待决策 Q3：**缺少精确镜像能力时，是否允许 `version_observed` 作为有明确限定的成功，还是所有 OTA 都须 image_verified 才终态成功？安全默认建议：显示「已观察到预期版本，镜像尚未验证」，保留 confirmation_pending；不自动重刷，也不声称精确成功。现有 ROM 升到提供新标识的 ROM 后可完成首次精确确认。该问题不阻塞状态页和持久队列实现。

## 6. UI/MCP、错误与重启

设备页显示快照值、「上次成功通信 … / …前」、预期休眠/等待联系及单独的最近尝试说明。刷新按钮请求一次有界检查；失败保留内容。未登记、坏 MAC、存储损坏等真实错误仍显示。MCP 读状态返回同一结构；排队返回 `accepted=true,job_id,state=queued,device_mac`，绝不返回完成文案。增加 OTA 任务查询/取消入口，与现有发布查询/取消共用状态语义；任务进展不能只靠一次长工具调用存活。

| 分类 | 行为 |
|---|---|
| 正常 deep、连接超时且无提交可能 | queued + waiting_contact；保留快照 |
| 未知提交/ACK 丢失/重启暂未出现 | awaiting_confirmation；下次认证对账 |
| 401、缺 token、409/他人 owner、yielded | blocked 原因明确；不自动密钥轮换/强占；恢复授权后重新认证 |
| IP 被另一 MAC 接管 | 拒绝候选并重新发现；绝不把任务迁到该 MAC |
| ROM 损坏、目标/ABI 不兼容、设备明确拒绝 | failed；不归类为睡眠；修正后显式新任务 |
| 计划窗口不足/暂时资源不足 | 等待并保留原因；不隐式续 lease |
| runtime 缺失/持久化失败 | Bridge 本地故障；前者修复初始化，后者报告存储问题 |

新增字段用 serde 默认，现有设备/Profile/jobs/context/seq/plans 原样保留，无需重新导入历史 legacy。没有新快照时展示已有 observed 摘要并标「采样时间未知」，不拿 last_acked_at 冒充整页状态采样时间。旧进程内 OTA 队列无法在重启后复原，升级前应提示重新显式提交，不能猜源文件/目标。恢复 transferring 一律先待确认；当前 Bundle 重启 Sending→Waiting 的代码也要核对对账顺序，不能未经查询就重发。禁止删除运行 data 或从 seed 覆盖已登记状态。

## 7. 影响文件与验收

- `bridge/crates/core/src/platform/model.rs, service.rs, store.rs`：持久快照/OTA job/原子更新、默认读取和校验；`core/src/coordinator.rs`：每设备选择、未知结果、取消和互斥。
- `bridge/crates/app/src/device_runtime.rs, main.rs, platform.rs`：初始化、认证事件、去除全局 OTA flush、共享应用入口、每 MAC 锁和 PowerPlan 接线。
- `bridge/crates/mcp/src/lib.rs`：OTA 参数/结果与工具说明；把执行逻辑复用到应用服务边界，独立 MCP 路径不能绕开合同。
- `bridge/crates/app/ui/index.html`：快照/年龄/等待和任务动作；`src/main.cpp, src/v2_status_snapshot.h, src/v2_status_snapshot.cpp`：按选定确认等级补认证状态字段及 OTA 回报；必要时核查 OTA target/owner 门控。固件修改必须另行实施验证，当前不承诺零 ROM 改动。
- 对应现有 Rust 单测/集成测试与 HTTP 假设备夹具；清单见 task.md。不新增通用工作流引擎、数据库或后台服务。

量化门槛：两台持久登记但 runtime 为空时所有状态读取成功返回；一次失败尝试前后 last_success 值/时间完全相同；重启后 job_id/MAC/ROM SHA256 不变；100 次另一 MAC 会合产生 0 次误传；保存模板/Profile 产生 0 次发布；401/409/错 MAC 产生 0 次业务写；未知 ACK 产生 0 次未经对账的重刷；未获新正式 PowerPlan 时读取/claim/失败重试令 light deadline 增量为 0。网络恢复后的下一次可用且预算充分的认证机会开始执行，无法给固定秒数 SLA 时只报可观察阶段与原因。
