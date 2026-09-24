# 通用多设备墨水屏平台：简化架构 v2

日期：2026-09-23。状态：现行权威设计；主体 v2 已实现并在 200×200 SSD1681 设备实机验证，双策略会合仍是待 spike、未实现的候选扩展。本文件以累计确认需求为准；与旧设计冲突的容量、数据模型和功耗职责由本文替代。候选协议字段不表示现有接口已支持。

## 1. 目标、非目标与现状

平台负责把多个数据源的有界字段，通过模板呈现在多台不同硬件的墨水屏上。Codex 只是一个 DataSource。Bridge 负责采集、决定投递与电源计划；设备执行已编译模板和明确命令，并守住硬件安全边界。

每台设备有一个 Profile，最多 8 个有序模板，所有已安装项都参与按键循环，任一时刻只激活一个。模板库按 `template_id + render_target` 只保留最新版。保存不发布，显式发布后才更新设备；已授权的数据同步随后按 push/pull 规则自动运行。

本版不做模板版本历史、历史选择/回滚 UI、资源图/GC、Provider/Dataset 分层、通用脚本插件、增量数据补丁、多设备原子发布或通用历史数据库。不承诺一种 ROM 支持任意 MCU，不把屏幕尺寸缩放当作硬件适配，也不承诺 deep 中即时远程唤醒。

现状依据：`PROGRESS.md` 记录 v2 主体已实现并在 200×200 SSD1681 上验证；`project-workflow/power-plan-c/status.md` 的最后部署记录为 0.17.2-bw、rv2=1。双策略会合仅完成设计同步，尚未实现、构建或部署。本次未查询设备或重新核验 ROM。现有 legacy“最多 3 个 enabled”与旧 Wi-Fi pull 是兼容输入，不是新平台约束。

## 2. 简化后的组件边界

```text
模板管理 / 设备管理（含功耗） / 数据管理 / MCP
                         │
                共享应用服务与权限校验
                         │
       模板编译/预览 ─ 每设备协调器 ─ DataSource
                         │              │
                 发布/同步/PowerPlan ← 最新 SourceSnapshot
                         │
                     HTTP / BLE
                         │
      鉴权与有界接收 → 完整 Bundle 存储 / 当前数据
                         │
             CompiledTemplate + 本地状态 → framebuffer
                         │
                 diff/刷新策略 → 驱动/BSP
```

Bridge 每设备一个串行协调器，持有当前上下文、一个进行中的 PublishJob、合并后的最新数据及 PowerPlan 状态。发布、激活、数据提交、OTA 互斥的部分串行执行；不同设备独立失败，共享 BLE 适配器采用有界公平调度。源采集独立运行，慢源不占用设备事务锁。

UI 和 MCP 调用同一套保存、预览、发布、同步开关、激活、claim/release、功耗与 OTA 用例；不允许 MCP 另写文件或绕过发布规则。保留现有 core/ble/render/app/mcp 的组织，先按职责拆函数和模块，不按下表实体机械增加 crate。

设备只负责协议校验、完整包提交、模板执行、本地时钟/电池注入、帧差分和电源执行。设备不含 DataSource，不解释 push/pull，不记录“长检查/短检查”，不计算业务全量同步阈值。

## 3. 核心数据模型

以下为必要记录和传输值，不要求各自成为服务或数据库表。

| 模型 | 最少内容与边界 |
|---|---|
| Device | `device_mac` 主键、显示名、连接线索、DeviceCapabilities、一个 Profile、同步许可、最后观察状态 |
| Template | `template_id, render_target, source`；同键保存替换最新版；源声明类型/范围、字段绑定、布局 |
| Profile | `device_mac, template_ids[1..8], initial_active_id, bindings`；顺序即完整按键循环，无 enabled/快捷子集；bindings 按模板字段绑定 source/field，不引用模板版本 |
| CompiledTemplate | `template_id, render_target, compiler_abi, requirements[], render_ops, local_dependencies, resources`；字段索引、类型/界限/缺失策略和渲染计划在同一产物 |
| DataSource / SourceSnapshot | Source 为 `source_id, kind, config, credential_ref`；Snapshot 为该源最新字段值及 `observed_at, valid_until, last_success_at, quality`；失败诊断另附，不清除最后有效值 |
| Bundle | 完整 Profile 快照、目标合同、所有模板源及 CompiledTemplate、内嵌必需资源、整体长度/校验；最多 8 项，自包含，无外部资源引用图 |
| PublishJob | `job_id, device_mac, frozen_bundle, state, last_error`；一次显式动作冻结一份临时传输包，状态 waiting/sending/succeeded/failed/cancelled/unknown |
| 运行数据快照 | `active_context_id, data_seq, fields[]`；完整包含当前 active 所需的全部远程字段及其质量/有效期，不包含其他模板的数据 |
| PowerPlan | `plan_id, mode, light_duration_s, rendezvous_period_s`；Bridge 唯一生成正式业务计划；设备回报实际接受期限 |

**Note4 增量资产扩展（待设备/Bridge 双方确认）：**上表的自包含 Bundle 与下文完整 A/B 包仍是现有设备的已实现路径。新协议的完整发布目标由一份 manifest 和它引用的已校验内容对象构成；传输仅发送未被当前/回退已提交清单引用的对象，设备在全部引用可用后原子激活新 manifest。当前与回退清单的引用集合必须同时保留，清理只能在持久提交后进行。此处的清单 A/B 与应用 ROM A/B OTA 是两层独立机制；详细字段和断电顺序见 `project-workflow/note4-bridge-publish/protocol.md`。设备尚未实现该扩展，不能将 Note4 ROM OTA 成功视为资产协议可用。

配置完成的设备恰有一个 active；出厂/两槽均不可恢复时为 unconfigured，显示 ROM 恢复页。空 Profile 可留作 Bridge 编辑草稿，但不能发布。

`active_context_id` 是唯一运行上下文标识：设备每次发布提交、有效激活切换和不能恢复上下文的冷启动生成一个不可复用的值，关联当前 Bundle、模板和 CompiledTemplate。A→B→A 必须得到三个不同值。正常 deep 唤醒恢复已保存的同一个值。无需再同时携带 template revision、manifest revision、plan hash、activation generation 等运行比较字段。

`job_id` 用于事务去重，整体 digest/CRC 用于包完整性，`compiler_abi` 用于格式兼容；它们都不是模板版本。`data_seq` 仅在一个 active_context 内递增，用于拒绝乱序数据。Bridge 重启保留每设备已分配计数；丢失本地记录时先认证读取设备状态，再从实际计数之后发送，不能盲目从 0 开始。

本地时钟、电池和连接状态由设备注入，远程字段不得覆盖。文本保持 ASCII 净化；任何未知 type/font/bind 或越界引用整份拒绝。Codex 缺 5h 桶显示静态 100/隐藏 reset、RC<=0 隐藏、无用户名整行隐藏由 Codex 映射及模板合同保留，通用运行时不硬编码这些桶名。

## 4. 模板与 Profile 生命周期

1. 保存：严格验证源和 render_target，替换库中同键当前内容；预览正常/缺失/过期/边界状态，不触碰设备。编辑 Profile 同样只保存本地。
2. 发布：用户对目标设备显式执行发布，预检其能力、8 项上限、绑定、空间和初始 active；冻结该时刻完整 Bundle 到一个 PublishJob。后续保存不改变排队内容。再次发布需显式替换尚未开始的任务，或等待当前提交确定后再发布，避免隐藏任务链。
3. 安装：接收至非当前 A/B 槽；完整校验并编译所有模板。Bridge 预览与设备使用同一编译/渲染核心。序列化产物必须无指针、具有固定宽度/边界；若携带 Bridge 预编译产物，设备仍验证 ABI 和所有索引/资源，不能信任原生结构体内存。
4. 提交：所有项可用才提交整个槽和初始 active，生成新 active_context_id。只切换完整旧包或完整新包，不逐模板覆盖当前包。提交成功和屏幕成功分开报告。
5. 日常：从持久存储加载已编译产物，按 requirements 接收有界字段并执行 render_ops；正常唤醒、同步、按键切换及重绘均不解析模板 JSON。激活时验证已有产物，只有缺失/损坏/ABI 变动才进入受控重编译；不能把每次唤醒伪装为恢复。
6. 按键：在全部已安装项中依序循环，原子保存新的 active 及上下文，作废旧接收事务。显示该项可用的缓存或缺失态并通知 Bridge；Bridge 以观察到的 active 为准，不自动改回初始项。后台只同步当前 active 所需字段。

显式发布界面同时明确数据持续同步是否开启；保存、发现、claim、恢复读取不产生同步许可。安装包冻结模板和绑定，运行数据始终在投递时从最新源快照组装，不将排队时预览值冻结数日。暂停同步停止后续投递，已接受的有界事务可完成。

包内保留模板源以支持异常重编译和恢复导出；这是冷路径材料。正常恢复仅需要 A/B 当前与上一完整包，无模板历史库。任务完成后释放临时传输包；终态任务可留简短结果，不保存可浏览内容版本。

从设备读取模板仅放恢复/迁移入口：等自然可读机会，读源/顺序/active；不自动唤醒、claim、发布或改 active。缺源标 unresolved，同 ID 不同内容要求选择是否替换 Bridge 当前内容；恢复得到的本地 Profile 默认不开同步。读取前后上下文变化则重试一次或报告不一致，不声称原子导出。

## 5. 硬件适配与容量

`DeviceCapabilities` 至少报告：firmware_target、render_target、尺寸、像素格式/颜色或灰阶、compiler ABI、最大模板数/包字节/字段数/快照字节、保留能力、BLE/Wi-Fi 能力、功耗协议与安全最大期限。设备对收到的 target 自行校验，Bridge 预检不能替代设备拒绝。

| 边界 | 责任 |
|---|---|
| BSP / MCU 适配 | 引脚、总线、供电、按键、RTC/唤醒、RAM/flash/保留内存、无线能力 |
| 控制器驱动 | SSD1681/SSD2683 等的窗口、RAM plane、BUSY 成败、休眠恢复、已验证波形 |
| 面板配置与刷新策略 | 实际面板组合、方向、对齐、温度范围、局刷限制和清影预算 |
| render_target | 可画尺寸、stride、像素编码、颜色/灰阶、字体图元能力；模板适用范围 |
| firmware_target | 上述可构建组合及分区；每种组合独立 ROM |

模板 ID 可有不同 render_target 的最新版，但 Profile 仍只引用 ID，由目标设备选择匹配变体。没有匹配变体就阻止发布；不自动缩放、不抹掉颜色假装兼容。彩色/灰阶先验证全刷和像素合同，不支持局刷的组合明确报告 partial=false。

新目标按最大 8 项总存储、完整 A/B 两份、编译峰值、无线栈和双帧计算容量。200×200 1bpp 单帧为 5000B，仅为已知目标示例；PSRAM 不等于 deep 保留内存。编译可逐项执行并写入候选槽，运行只加载 active，不能同时展开 8 个 JSON DOM。能力不足的旧设备继续 legacy 限制并明确提示，不能静默裁剪 8 项到 3 项。

SSD2683 和某个板名不构成已验证面板/引脚/波形合同。第二块硬件参数取得证据后再建 target。OTA 必须双端核对 firmware_target、板修订、分区、镜像长度/完整性；自动回滚必须由 bootloader 实测证明，不能由双分区推断。

## 6. 数据更新判定：仅 push 与 pull

字段在 Bridge 的绑定合同中只有两种更新语义。push 字段的可见值/缺失状态/质量变化触发投递；pull 字段变化只更新缓存。观察时间、轮询时间、错误重试计数不默认成为变化；需要显示且参与触发的元数据必须明确纳入对应字段合同，避免每次采集都触发。

设备 requirements 只描述需要什么字段、类型和缺失/过期行为，不携带触发语义。采集频率由 DataSource 设置，不能与 BLE 周期、全量同步阈值或局刷频率混成一个 interval。

Bridge 对当前 active_context 保存：最新完整快照、最新 push/full 指纹、最后确认应用的 push/full 指纹、`full_sync_deadline`、下一 data_seq。指纹以规范化有界字段内容计算，比较发现冲突时可再比较字节；无需把指纹构造成持久资源身份。

```text
源变化 → 重新组装 active 所需完整快照
push 指纹 != 最后确认值 → 合并为最新完整待发快照
仅 full 指纹变化         → 只存最新值，不改 PowerPlan/deadline
Bridge 时间到 full_sync_deadline → 排完整同步（可包含 pull 的新值）
认证可达机会 → 发送当时最新完整快照 → 成功 ACK 才更新确认指纹/deadline
```

每次发送都含全部 push 与 pull 字段；缺失显式编码，不能保留上一个包里已删除的字段。到达完整同步阈值可发送同值完整包以核对状态，设备像素相同则零刷新。只有成功确认完整快照应用才把 deadline 设为该次确认时间加阈值；采集、排队、发送失败和 pull-only 变化均不得后移 deadline。ACK 不明时保留到期/待确认状态，下次查询或幂等重试。

传输中源继续变化，ACK 只确认当时那一包的指纹，不能错误确认更新后的缓存；之后重新比较并合并下一包。push 变化短暂出现又恢复为已确认值时可取消未发送数据，首版不保证每个瞬时事件都展示；需逐事件确认的业务不适合本快照协议。

上下文变化后确认基线清空，Bridge 读取已安装模板需求，立即准备该 active 的完整首包。完整同步 deadline 到期仅意味着“下一可达机会应同步”，设备不为它安排第二类唤醒。若已 light 则立即投递；若 deep 则等下一次 rendezvous。

## 7. Bridge 电源决策与设备状态机

Bridge 是正式 PowerPlan 唯一业务决策者。Bridge 根据待发布操作、push 活跃情况、用户功耗设置、当前可达性决定本轮 NOOP/SLEEP、BLE 更新、Bundle 更新或进入/继续 Wi-Fi light。pull-only 变化不改任何电源计划；完整同步到期可由 Bridge 决定传输所需计划，但不会让设备自行升档。

会合发现支持两个可切换策略，但它们只分叉“怎样建立本轮可达机会”，不复制 coordinator、owner、Data、Bundle、PowerPlan 或业务 ACK：

- `device_first`（默认与恢复）：设备发可连接广播，PC 扫描并作为 central 连接设备 GATT server；小 Data 可在既有认证 BLE 会话完成，大任务由正式 PowerPlan 转入 HTTP。
- `bridge_first`（候选、需能力协商与 spike）：PC 在每个预定窗口都重复广播认证 RendezvousDirective，无工作也明确发送 ACCEPT_SLEEP；设备扫描、验证后每窗口必发认证 StatusBeacon，再按决定休眠或打开一次有界 Wi-Fi bootstrap。PC 收到或发出 OPEN_WIFI 后并行等待 StatusBeacon 与设备 HTTP 端口，认证 HTTP 可用即进入同一 coordinator；回复漏收不阻塞已验证的 HTTP 交付。

`bridge_first` 的广播是有界、目标明确、认证且防重放的会合控制面，不是公开 hint，也不是第二套业务消息通道。OPEN_WIFI 只允许本窗口的一次短时 HTTP 可达机会，不创建 owner、不设置或延长 light deadline、不获得 BOOT provisional，也不授权 Data/Bundle 写入；随后仍须在 HTTP 中核对完整 MAC、bridge、owner、endpoint token、context，并由正式 PowerPlan 决定继续在线期限。ACCEPT_SLEEP 只结束本轮 bootstrap，不撤销仍有效的正式 light/BOOT 期限。

```text
DEEP --timer--> 本地必要时钟/维护 → RENDEZVOUS(strategy)
  ├─ device_first：设备广播 → PC GATT 会合
  │    ├─ 无响应/超时/NOOP/SLEEP → 关无线 → DEEP
  │    ├─ 小完整数据包          → 校验/业务 ACK → 关无线 → 本地显示 → DEEP
  │    └─ 正式 PowerPlan(light) → 关 BLE → 有界 Wi-Fi 连接 → WIFI_LIGHT
  └─ bridge_first：设备扫描 PC Directive → 必发 StatusBeacon
       ├─ ACCEPT_SLEEP/NO_DIRECTIVE → 关 BLE → 本地显示 → DEEP
       └─ OPEN_WIFI → WIFI_BOOTSTRAP（本地硬截止）
            ├─ HTTP 认证 + 正式 PowerPlan → WIFI_LIGHT/有界交付
            └─ 超时/拒绝/低电 → 关无线 → DEEP

DEEP --BOOT--> 立即建立 provisional 截止(t_boot + 300s)
              → 手动 Beacon 通知 Bridge → 有界 Wi-Fi 连接 → WIFI_LIGHT
              → 正式 PowerPlan 可缩短/保持/延长 provisional

WIFI_LIGHT --新正式 PowerPlan--> 更新明确期限/模式
WIFI_LIGHT --截止/通信失败硬限/低电--> 有界收尾 → 关无线 → DEEP/保护关机
```

BOOT 的通知先走短 BLE 会合，Wi-Fi 连上后也上报 wake_reason；“立即”表示立即尝试通知，不保证 Bridge 已收到。provisional 从物理按键唤醒开始计时，包含 BLE、建连与事务时间；Bridge 不可达时最迟 300 秒结束在线状态，连接失败/低电可更早退出。不得以重连、重复通知或 timer wake 再获得 300 秒。Bridge 正式保持原兜底时发送剩余时长，不重置为新的 300 秒。

**物理唤醒窗口是下界（取大）。** 一次按键/BOOT 唤醒建立 `t_boot + 300s` 后，后继正式 PowerPlan 只能保持或延长它：effective deadline = `max(plan, t_boot + 300s)`。设备只在首个正式 plan 被接受前上报 `provisional_remaining_s`，因此 **Bridge 必须自己记住该窗口**（`PlanState.manual_until`）并在其有效期内即使没有待办也不下发 sleep；只有显式入口（`explicit_plan` / `queue_explicit_light`）可以提前结束。没有待办时 Bridge 按剩余窗口下发 light（而不是 `MAX_LIGHT_S`），否则每 60 秒一次会合会把窗口无限延长。

设备执行安全约束：最大 light lease、最大 rendezvous 周期/无线窗口、事务总截止及停滞截止、低电保护、重复计划幂等。它可以缩短或拒绝危险计划，并报告原因；不能根据数据内容或“用户大概仍活跃”续租。读取、数据传输、claim/owner renew 都不隐式改 light deadline。

light deadline 使用设备单调时钟，`accepted_at + granted_duration`；Bridge 收到实际剩余期限后决定是否发送新 plan_id。新计划可延长、缩短或立即 sleep，重复计划返回原接受结果和当前剩余时长。校时不能移动期限。事务必须在接受前检查剩余期限，到点中止暂存并睡眠；若某硬件操作必须有限收尾，其最大收尾预算包含在事先接受的截止内，不再额外延长 BOOT 300 秒兜底。

owner lease 与 light lease 完全分开。空闲设备只能通过 token 保护的显式 POST /claim 建立 owner；不能借 BLE 数据创建 owner。owner 过期时，已绑定并通过设备 token 的 Bridge 可请求有限 light 会话以执行 HTTP claim，该例外只给电源机会，不给数据/Bundle 写权限；有效他人 owner 拒绝此机会和写入。claim 成功后仍需原来的同步/发布授权。owner 续约不影响电源期限。

双策略切换通过当前可用的认证通道提交完整配置并由设备持久化 ACK，在约定的未来窗口生效；Bridge 收到 ACK 或认证读取确认前不能显示“已切换”。默认始终为 device_first。bridge_first 即使持续收到有效指令也按固定日程开放 device_first 恢复窗口，连续无有效指令可提前恢复；BOOT 始终允许 device_first。恢复窗口不自行改持久策略，正式回退仍走认证配置与 ACK。

初始实验参数沿用专项候选：rendezvous 60 秒、广播约 1.5 秒、BLE 总窗口约 15 秒、Wi-Fi 建连约 15 秒、正式单次 light 最大 600 秒。它们必须按 target 和实测定值，只有 BOOT 300 秒兜底是本次已确认产品语义。PC USB/配网等模式若需要不同策略，必须成为明确配置和有界入口，不混入普通数据活动。

### “实时”的上界

deep 无在线无线，Bridge 无法即时叫醒。设周期 R、源变化检测耗时 S、一次成功机会的连接/传输/刷新耗时 T：若下次机会成功且队列有容量，从源变化到显示至多约 `S + R + T`，不是零延迟。连续漏掉 k 次机会还要加 kR；PC 休眠、BLE 不可用、owner 冲突或网络故障时没有有限保证。

Wi-Fi light 有效且链路可达期间，后续 push 可立即排完整数据投递，实际延迟仍包含源采集、调度、传输及面板波形。Bridge 可按 push 活跃策略显式发新 PowerPlan；数据包自身不续租。临近截止来不及传输时等下次会合。pull-only 变化仅搭车或到 Bridge 完整同步阈值，不能称为实时字段。

## 8. 最小消息与幂等规则

所有业务副作用命令在已认证会话执行，公共字段为 `protocol, device_mac, bridge_id, request_id, session_nonce`。session_nonce 绑定当前认证会话。公开发现 Beacon 仍只作 hint；bridge_first 另定义经过预配置密钥认证、防重放的紧凑 Directive/StatusBeacon，它只能选择本轮 SLEEP 或有界 OPEN_WIFI，不能承载业务数据、token、claim、Bundle、Data 或正式 PowerPlan。完整 MAC、上下文与能力仍通过认证 HTTP/GATT 状态握手核对。

| 消息 | 必需业务字段 | 语义 |
|---|---|---|
| RendezvousBeacon / Status | wake_reason；认证 Status 含完整 active_context_id、active_template_id、已提交 job_id、data_seq、能力及剩余功耗期限 | Bridge 据此重建真实状态，摘要不构成授权 |
| RendezvousDirective（候选） | 短目标、config_epoch、window_seq、ACCEPT_SLEEP/OPEN_WIFI、schedule_hint、auth_tag | PC 每个 bridge_first 窗口冻结并重复同一指令；只决定是否开放本轮 Wi-Fi bootstrap |
| RendezvousStatusBeacon（候选） | 短目标、同一 epoch/window、ACCEPT_SLEEP/WIFI_OPENING/NO_DIRECTIVE/REJECTED、reason、auth_tag | 设备每窗口必发并重复；证明本窗口活跃/决定，不是 Wi-Fi ready 或业务 ACK |
| NOOP | 可选校时与时区 | 不更新数据、不改期限；窗口结束回 deep |
| PowerPlan | plan_id、mode=sleep/light、light_duration_s、rendezvous_period_s | 明确修改电源计划；SLEEP 是 mode=sleep 的同一命令，避免另造租约实体 |
| Data | active_context_id、data_seq、完整 fields | 原子替换当前数据；字段索引按当前 CompiledTemplate 校验 |
| Bundle | job_id、expected_active_context_id、完整包长度/校验/内容 | 对当前观察上下文条件提交；期间按键切换则冲突，由 Bridge 重新预检，不覆盖用户切换 |
| Activate | expected_active_context_id、template_id | 显式远程激活；成功返回设备生成的新上下文 |
| ACK | request_id、result、active_context_id；按操作附 job_id/data_seq/plan_id、display_state、retention、实际期限 | 一种业务结果，失败带有限 error_code；非多阶段发布事件流 |

大包分片只作为传输机制：BEGIN(type/length/checksum/context)、连续 offset 的 CHUNK、COMMIT。每设备一个暂存区，长度/MTU/总时限固定；相同重复分片确认当前位置，不同重叠/跳洞/越界取消。Bundle 通常先由 PowerPlan 开 Wi-Fi 再传；BLE 只接受能力及当前预算允许的完整小 Data，不收半包后自动升档。

提交前再次检查身份、owner、上下文、长度、校验、类型和全部字段。相同 context+data_seq 且相同内容返回原结果，较旧 seq 拒绝，同 seq 不同内容冲突。seq 跳号允许合并中间快照。切模板产生新 context，旧包无论 BEGIN 或 COMMIT 在途都拒绝。

plan_id 在设备认证会话内单调递增，设备保留最高已接受值及原期限；同值同内容幂等，同值不同内容冲突，旧值拒绝。新连接先核对当前最高值，不能重新编号来重放历史计划；冷重启废止旧会话 nonce，不能恢复旧 light 承诺。request_id 重试远程 Activate 需返回先前结果，不能生成第二个上下文。

候选广播一窗口只允许一条冻结 Directive；相同 epoch/window 的重复同字节包返回同一决定，不重复开网或续任何截止，同序号不同内容、旧 epoch、过期/未来窗口和错目标全部拒绝。设备即使未收到有效 Directive 也发送基于本地预期窗口的 NO_DIRECTIVE。PC 分别记录 directive、StatusBeacon、HTTP 认证和业务 ACK：WIFI_OPENING 不等于端口已开，端口开放不等于身份通过，只有业务 ACK 更新指纹、deadline、job 或 plan 状态。

简单 ACK 区分 `result=applied/rejected` 和 `display_state=unchanged/displayed/pending/failed`；附 `retention=ram/rtc/flash`，不引入 received/validated/prepared 等产品状态。链路层 Write Response 不算业务 ACK。无线窗口不足时先应用并回 pending，之后本地完成显示；Bridge 下一次读取实际状态。显示失败由设备本地安全全刷恢复，同一 Data 重发不反复触发波形。

## 9. 大黑块与反色大数字的局刷

Bridge 决定是否投递，设备以新旧 framebuffer 决定是否刷新；字段变化不等于像素变化。CompiledTemplate 保留静态布局和局部重绘边界，运行时先生成候选帧，再比较上一次成功显示的真实像素。黑底矩形没有变化时，不能因为覆盖它的数字变了就把整个黑块当成 dirty。

1. 计算实际变化像素、白→黑/黑→白数量和局部黑量；找数字笔画等 dirty rect。以驱动给出的 X/Y 对齐扩展并裁剪到屏幕，扩出的边缘使用真实旧/新像素，不能填白。
2. 对齐后重叠窗口合并，避免同一区域执行多次波形。刷新区域取决于实际差分；残影风险统计另按稳定的语义区域面积计算，不能拿很小的 dirty box 误判整个黑底反相。
3. 驱动根据该 target 要求装入 previous/new plane，包含未改变的窗口边缘。SSD1681 的字节对齐、坐标方向和 plane 恢复必须留在适配层；SSD2683/灰阶/彩色使用自己的合同，模板不得硬编码寄存器或 LUT。
4. 以局刷次数、累计变动面积、黑→白擦除量、距上次全刷时间、温度范围、区域极性和基线可信度决定局刷或全刷。分钟时钟有独立预算，不能因时钟更新反复驱动不变大黑块；必要清影全刷由本地缓存完成，不强制联网。
5. BUSY 成功后才更新 previous framebuffer 和预算；失败将基线标为不可信。掉电、异常复位、布局变化或保留数据 CRC 失败后先全刷。可见旧图仍在不代表控制器 RAM 或旧 plane 可靠。

大黑块局刷必须以照片验证为门槛。反白数字 89↔90、99↔00 虽仅局部变化，也会累积黑→白擦除残影；持续小变化仍要按预算全刷，不能承诺永不闪。背景反相、大面积擦黑、未知温度下未经验证的波形、预算到限都升级全刷。到达清影时间即使像素相同，也允许本地维护全刷，不能被“memcmp 相同”提前跳过。

旧专项 §8 的高墨 4 次/时钟 90 次等只是当前面板校准起点，不升级为通用模板常量。某个面板只允许全刷时按能力执行；缩小窗口不保证波形时间按面积等比缩短。双相清白再写暂缓，不把两次局刷宣称为全刷清影。验收必须包括固定曝光照片、窗口外像素不变、跨 deep 基线、温度、连续变化和 BUSY 故障；软件逐像素一致不能替代面板质量。

## 10. A/B 恢复、安全与可观测性

Bundle 两槽均为完整自包含包。写非当前槽时当前槽始终可用；编译和完整读回校验成功后写防撕裂提交记录，再以双副本选择记录切换。恢复只选“完整且已提交”的槽；选择记录损坏时以内部存储提交序号确定，序号仅为存储恢复元数据，不对用户提供版本。激活记录绑定所选槽并使用同样的防撕裂机制。

掉电必须恢复完整旧包或完整新包，不能拼接两包资源。低空间拒绝发布，不先删除唯一有效包。上一完整包只用于安装失败/ABI 恢复，恢复导致 active_context 重新生成并如实报告，不提供历史浏览。Bundle 的 flash ACK 只有持久提交后才发；日常 Data 默认按能力保留 RAM/RTC 并定期合并检查点，冷启动如实报告恢复的数据位置，Bridge 重发最新完整包。每分钟时钟不写 flash。

身份继续以 Wi-Fi MAC 为主键，显示名、IP、BLE 地址和广播摘要不是凭证；IP→UDP→ARP→BLE 发现结果均核对 MAC，不依赖 mDNS。token 保护 `/update`、`/doUpdate`、ArduinoOTA、POST /claim；新功耗/Bundle/激活写入口保持认证和 owner 校验。401/409 停写并显示原因，不静默抢占。token 仅经既有绑定 BLE 安全链路取得/轮换，不进入模板、仓库、日志；数据源凭据与设备 token 分离。

bridge_first 的每对 Bridge/设备控制密钥与策略、epoch、窗口锚点一起，只经已绑定加密 GATT 安装或轮换；不直接复用或广播 bearer token，不经普通 HTTP 传输新密钥。候选 legacy 31B 广告采用固定有界二进制布局、方向域分隔和截断 HMAC；短身份只作路由，完整 MAC/bridge_id 进入 tag 上下文。具体 Company ID、字段预算、tag 长度、密钥生命周期及冷启动序号恢复必须经过安全评审和抓包 spike 后冻结。

GATT 优先复用现有表并明确协议协商；修改特征表必须考虑 Windows 缓存和重新配对。周期会合不开放新绑定。未授权连接、半包、过大包、无限碎片都不能推迟无线总截止。BLE 回调仅入有界队列，主任务串行提交/驱动，不在回调里建 Wi-Fi 或等待面板。

| 异常 | 确定行为 |
|---|---|
| Bridge/PC 离线、漏会合 | 保留显示，到窗口截止睡眠；Bridge 留待办，下次机会重试 |
| bridge_first 无有效 Directive | 设备必发 NO_DIRECTIVE 后按本地硬截止睡眠；周期/提前恢复窗口尝试 device_first |
| StatusBeacon 漏收但 HTTP 已认证 | 正常进入共用交付，单列 reply_missing；不因遥测漏包浪费已开启的 Wi-Fi 窗口 |
| OPEN_WIFI 后 HTTP 不可达 | PC 在总截止内有界重试；设备 bootstrap 硬截止独立到期关网，重复探测不续期 |
| 发布 ACK 丢失 | 查询当前 job_id/完整包状态后幂等处理；结果未定显示 unknown，不擅自重建任务覆盖 |
| Bundle 写入/编译失败 | 当前完整包继续使用，任务明确失败；不更新 active |
| 数据已应用但显示失败 | ACK 如实报告 failed；本地有界全刷修复，不能声称已显示 |
| 源失败/过期 | 保留原观测和有效期，按编译策略呈现 stale/missing；不伪造新鲜值 |
| claim/token/target 冲突 | 停写，等待对应问题解决，不绕过规则 |
| PowerPlan ACK 丢失 | 查询原 plan_id/剩余期限，重试同 ID 不延长 |
| Wi-Fi 连接失败/低电 | 提前安全收尾，绝不为完成队列无限在线 |

状态只记录必要摘要：active_context、active 模板、job/data/plan 结果、显示状态、源新鲜度、同步 deadline、owner/light 剩余时间、会合命中/延迟、刷新原因/预算、基线可信度、BUSY/清理错误。区分“预期 deep”“等待会合”“确认故障”。日志有界并脱敏；运行文件继续在 `<exe>/data`，seed 只读，不写仓库。

## 11. Bridge 四页信息架构

| 标签页 | 主内容和动作 |
|---|---|
| 模板管理 | 模板最新版、target 适用范围、编译校验、预览、保存、被哪些设备 Profile 引用；无版本选择/回滚 |
| 设备管理 | MAC/名称/能力/占用、每设备 Profile 的 1–8 项顺序与 active、绑定、显式发布、同步开关、任务/实况差异；功耗子菜单含正式/临时计划、剩余时间、会合周期与显式 light/sleep；恢复读取在诊断入口 |
| 数据管理 | DataSource 配置、字段类型和 push/pull 绑定语义、最新 SourceSnapshot、有效期/错误、采集测试、使用设备；完整同步阈值在对应设备同步设置明确展示 |
| MCP | 连接状态、工具能力/权限、调用结果；与 UI 共用应用服务，不产生第五套业务模型 |

“模板已保存”“最新版未发布”“数据待投递”“数据已应用但屏幕失败”“等待下一会合”分别展示。8 个安装项都显示在完整顺序中，不增加 enabled 和快捷容量设置。模板发布为显式按钮/工具；MCP save 不隐含 publish。多设备操作分别显示每台结果，不宣称批量原子成功。

## 12. 关键时序示例

**pull 搭车。** 已确认字段 `used=30(push), reset=12:00(pull)`；reset 改为 12:05 只更新 Bridge 缓存，PowerPlan 和全量 deadline 不变。used 改为 31 时发送 `{31,12:05}` 完整包。若 used 不变，则 deadline 到期后在下一可达机会发送 `{30,12:05}`。

**deep 首次 push，随后实时变化。** t=0 会合结束，t=2 push 变化；Bridge 排最新完整包，设备仍 deep。约 t=60 下一机会，Bridge 可先 BLE 发小包并回 deep，或因配置的活跃窗口发正式 light 计划后 Wi-Fi 送包。light 中 t=65 再有 push 则立即排送；t=66 仅 pull 变化不送、不续期。原期限将到时，只有 Bridge 新 PowerPlan 能延长。

**BOOT 与不可达 Bridge。** t=0 按 BOOT，设备建立 t=300 provisional 截止并广播 manual；t=3 Bridge 返回正式 60 秒 light，设备截止改为 t=63。重复该 plan_id 不把截止改为 t=66。若始终未收到正式计划，则无线最迟 t=300 关闭；timer 唤醒只走短会合。

**排队编辑与断电。** 用户发布模板 A/B 到设备，job 冻结当前两个源和绑定；等待期间又保存 A，只改变库。设备写入另一 Bundle 槽中途掉电，仍启动旧完整槽；重试原 job 仍发送冻结的 A/B。成功后下次显式发布才带新的 A。

**按键与在途数据。** active A/context X 的 data_seq=10 正在接收，用户切到 B/context Y。提交检查不匹配而拒绝 X 包；Bridge 得知 Y 后重建完整 B 数据。再次切 A 得 context Z，X 的包仍不能复用。没有缓存时 B 显示编译的缺失态，无 JSON 重解析。

**owner 过期。** Bridge 在会合确认 owner 空闲，但不能 BLE 写数据；认证申请有限 light，HTTP token POST /claim，成功后投递已有授权的最新数据。claim 及后续数据都不增加 light 期限。若实际是另一 owner，停止写入，不自动接管。

## 13. 分阶段落地与验收

| 阶段 | 交付边界 | 必须通过的验收 |
|---|---|---|
| M0 合同与基线 | 记录最新现场；冻结 legacy fixture；明确 target/8 项/Profile/PowerPlan 合同 | 不把文档方案当已部署；现有 token/claim/Codex 显示规则有回归基线 |
| M1 Bridge 简化模型 | 共享 UI/MCP 服务、DataSource+SourceSnapshot、Codex+Static JSON、每设备 Profile、单 PublishJob | 两源/两设备互不耦合；保存零推送；排队编辑不漂移；push/pull 真值表与 ACK 丢失测试 |
| M2 CompiledTemplate | 合并字段需求/渲染计划，target 参数化，安装编译和持久加载 | 宿主/固件逐像素一致；三端 canonical 合同一致；正常千次唤醒/渲染模板解析计数 0；8 项资源峰值合格 |
| M3 完整 Bundle 与上下文 | A/B 全包、原子激活、一个 active_context、简单 Data/ACK | 写入/提交各阶段断电；A→B→A 旧包拒绝；丢 ACK 重试无重复刷屏；旧包恢复可启动 |
| M4 rendezvous/PowerPlan | Bridge 统一决策、设备有界执行、BOOT provisional、HTTP/BLE 同义消息；默认 device_first | timer 无 300 秒兜底；BOOT 最迟 300 秒；读取/claim/数据不续期；重复 plan 幂等；所有退出释放无线锁 |
| M5 画质与实机场景 | 按面板实施 diff/对齐/独立预算；device_first 提速与 bridge_first 候选独立 A/B；Windows 会合及电池测量 | 黑底数字长期照片、跨 deep/异常基线、温度/BUSY；双策略各自命中率/P95/丢包/恢复与24h电量；bridge_first 认证/重放/切换/截止故障注入，不能仅以软件测试证明省电 |
| M6 第二硬件与四页闭环 | 核实第二 MCU/屏幕目标，独立 ROM、模板变体、OTA 防错 | 不同分辨率/像素格式完整链路；无局刷目标正常全刷；双端错误 target 拒绝；UI/MCP 行为一致 |

每阶段先拆任务和验收，再改代码并更新 PROGRESS；本设计本身不授权部署或提交。现有显示安全修复应复用而非重写。旧桥/旧固件维持 legacy 能力，新协议须双方协商并显式启用；新协议运行期间漏会合不是自动退回周期 Wi-Fi 的理由。旧 3 项限制只在 legacy 适配中存在，新 Profile 不能被静默裁剪。

## 14. 明确删除/暂缓与剩余验证

删除旧方案的 Deployment 层、ProviderType/SourceInstance/Dataset 多层实体、独立 DataRequirementPlan/RenderPlan 身份、manifest/activation 多重版本、ReleaseIntent/ReleaseArtifact 多阶段发布模型、内容寻址资源及 GC。职责仍在对应简单记录内，不以新名字保留同样层数。删除模板 version 管理和超过 3 项要裁剪的目标规则，旧 schema 的 version 仅在兼容适配器保留其原字节合同。

删除设备根据数据或检查类别决定同步/升 light 的规则；正式期限只来自 Bridge PowerPlan。旧 wake/renew/sleep 多种业务租约命令收敛为一种 PowerPlan；保留传输分片、存储提交标记和鉴权，因为完整性与安全无法由更少状态替代。

暂缓 patch/event 协议、通用历史查询、脚本插件、资源去重、复杂布局求解器、跨设备原子发布、实验清白双相波形。恢复导出保留最小入口，不成为模板管理主流程。

待测项不阻塞保守的 device_first：8 项 A/B 的实际空间与编译峰值、各 MCU 的 RTC 数据保留、Windows 广播/扫描并发与单次 TX→RX 退化、31B 实际载荷、认证耗时、双策略命中/恢复、面板温度/黑底局刷门槛、owner 到期带来的偶发 Wi-Fi 成本、数据检查点的 flash 寿命。缺数据时采用默认策略、全包、全刷、有界等待和真实状态报告，不承诺续航天数或固定毫秒级实时性。

## 15. 设计依据与优先级

- `AGENTS.md`：身份/token/claim、保存与显式推送、三方模板合同、同源渲染、ASCII、便携数据路径；其旧容量由本次确认需求覆盖。
- `PROGRESS.md` 顶部总设计节及 0.15.10 最新现场节：区分提案和部署，续租/心跳引起常亮问题说明必须分开占用与电源期限。
- `docs/history/generic-display-platform-design-v1.md`（已归档）：其 §4–7 的模板编译、目标边界和原子安装，以及 §9–14 的竞争/恢复/安全验收输入已吸收到本文；该文件不再是现行设计依据。
- `docs/ble-rendezvous-power-design.md`：完整阅读；§3–7 提供有界会合、BOOT、owner/电源分离与传输故障模型，§8 提供黑块/旧帧依据，§9、§13 提供功耗与照片验收方法；其旧本地决策、容量与命令模型不覆盖本次确认需求。
- `project-workflow/power-plan-c/task-5-advert-rendezvous.md`：双策略会合候选的空口预算、Windows spike、切换恢复和故障注入细化；与本文冲突时以本文的业务、安全和电源边界为准。

本文是通用平台唯一现行总设计。BLE 专项文档只提供仍适用的底层证据和实测方法，归档 v1 只保留决策历史；现状事实仍以最新交接和后续实测为准。
