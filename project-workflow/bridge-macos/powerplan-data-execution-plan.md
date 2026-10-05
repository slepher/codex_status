# PowerPlan 与 Data 同轮联合下发执行计划

日期：2026-10-05。状态：计划已编写，尚未实现、构建或部署。待办唯一归属：[backlog A9](../../docs/roadmap/backlog.md#a9-powerplan-与-data-同轮联合下发2026-10-05)。

## 1. 目标与已确认现场

用户确认：数据变化触发的电源意图必须与本轮数据一起下放；Bridge 在发送前冻结计划，不能等 Data ACK 后才决定 light。小数据同请求，较大数据先 Plan 打开合法窗口，再在同轮经 HTTP 投递。任何失败都有明确、有限的返回。

2026-10-05 Note4 MAC `7C4FADB93408` 的只读核对：

- 当时登记 IP 为 `192.168.10.103`；以后执行必须重新读登记记录，不照抄本地址。
- 用户报告 weekly 66→65；本轮独立确认当前 usedPercent=35，即余量65。旧值66没有本轮独立快照，不将用户报告冒充采集证据。
- 14:18:27（北京时间）运行日志 usage changed rev2；14:20、14:22、14:24 的 BLE data ACK 各约4.86s超时，随后会合退出；截至本次采样，变化后没有 PowerPlan ACK。
- Bridge push_dirty=true、in_flight=ble_data/seq97/attempts3，确认基线仍seq96；设备后续BLE状态报告applied_seq97，但这不包含足够的context/CRC/结果关联证明，不能直接补成成功ACK。
- 最后已确认 Plan1274 为 sleep/0s。证据在本机私有运行日志 `~/Library/Application Support/dev.codexstatus.bridge/data/logs/bridge-app.log.2026-10-05`，不复制完整运行数据或凭据到仓库。
- 此处已证明 Bridge 编排阻断；数据 ACK 丢失的设备/RF原因尚未确定。此次改变不能宣称已修复RF丢ACK。

生产链路：`coordinator::next_delivery` 将小快照选为 BleData；`app/rendezvous::ble_cycle` 先等待 Data ACK，出错直接返回，后面才调用 `plan_for_rendezvous`。成功ACK还会清掉push_dirty/in_flight；若无其他工作，随后选sleep。两条路径均不能保证用户要求的活跃窗口。

## 2. 本轮范围与约束

实施覆盖共享 PlatformService/Coordinator、Bridge BLE与HTTP调用、生产固件处理器、同源Fake ROM与生产app组合回归。复用现有鉴权、claim、会话、plan_id、data_seq、CRC、原子state存储和handoff；不另建任务系统、不新增GATT特征、不改模板哈希协议。

此次授权仅生成执行计划。后续实现与候选构建、运行Bridge部署、Note4刷写/实机故障注入按届时用户授权执行；本计划的部署步骤不等于本轮已获部署授权。默认固件目标仅 `zectrix-note4-b`；本计划不要求构建或刷写1.54，也不操作其他命名Bridge实例。

正式light期限、HTTP ready、handoff成功、BLE关闭、CPU实际light-sleep分别记账。读取、claim/续约、数据确认、重试和诊断complete不隐式续期；低电/owner拒绝/预算和BOOT边界保留。保存模板/Profile仍不自动发布。

## 3. 最小协议方案

### 3.1 能力协商与请求

在认证HTTP/BLE status中新增可选能力 `plan_data_v=1`；只有明确宣告的设备才接收扩展请求。不占用现有拒绝保留字段 `protocol`/`rv`。

扩展已有 `op=plan` 与 `POST /api/plan`，添加可选 `data`。复用现有外层request_id/MAC/bridge_id/session_nonce/token；data内部保留现有context/seq/crc/完整fields。纯Plan请求与原ACK兼容。

示意（未显示鉴权值）：

```json
{
  "op": "plan",
  "request_id": "<原请求ID>",
  "device_mac": "<登记并认证的MAC>",
  "bridge_id": "<当前owner>",
  "session_nonce": "<当前会话>",
  "plan_id": 1275,
  "mode": "light",
  "light_duration_s": 600,
  "rendezvous_period_s": 60,
  "data": {
    "active_context_id": "<已确认context>",
    "seq": 97,
    "crc": "<既有字段CRC>",
    "fields": {"buckets[codex].weekly.remaining": 65}
  }
}
```
示意只展示一个字段，实际data必须是设备认证合同要求的完整快照，不能照示意下发缺字段包。

新设备联合请求返回外层关联信封和独立 `plan_result`/`data_result`：

- plan_result：applied/rejected/not_applied，plan_id、当前真实accepted_remaining_s及必要的有限错误分类。
- data_result：applied/rejected/not_attempted，context、data_seq、content_crc、display_state（pending/displayed/unchanged/failed）。
- 外层表达complete/partial/rejected，Bridge只依据经关联验证的分项结果记账，不凭HTTP200或外层applied推断两项成功。
- 无data时维持原Plan响应；未派发、收到明确拒绝、实际发送但无法确认严格区分。最终字段名在P1与两端一起冻结。

### 3.2 设备处理语义

先完整解析并校验外层形状、长度、目标身份、会话、owner和权限，任何这类失败不修改Plan或Data。
随后校验并应用Plan，再处理可选Data；复用两种现有状态机的幂等校验。

| 情形 | Plan | Data | 返回/后续 |
|---|---|---|---|
| 外层无效、认证/owner拒绝 | 不应用 | 不应用 | 明确拒绝及阶段 |
| Plan旧ID、同ID不同内容或无效 | 拒绝 | not_attempted | 不借Data绕过Plan拒绝 |
| Plan接受、Data有效 | 应用 | 应用/幂等确认 | 两项结果；BLE绘制pending |
| Plan接受、Data合同/context/CRC/seq错误 | 保留已接受Plan | 拒绝 | partial；不撤销Plan，不循环重试确定错误 |
| 接受Plan后分配失败/设备复位 | 依据真实接受事实 | 可能未应用/未知 | 返回可得分项结果；恢复时分项核对 |
| light计划已到期后的重复请求 | 不重启期限 | 禁止借过期计划首次写Data | 返回原Plan事实与当前0秒；已应用同Data可只读确认 |

这是有序、分项可确认的操作，不承诺跨RAM/RTC/NVS的全有或全无事务。数据错误不回滚合法Plan；耗电最多至原有界期限。sleep+Data不作为自动push组合：自动push固定light；明确sleep仍保留独立原入口。

BLE仍先缓存Data并ACK pending，保留现有交接后绘制机制；不得把面板刷新或慢Wi-Fi关联塞进ACK等待。HTTP如走同步绘制，也必须在有限预算内返回，必要时复用pending路径。仅data_result=applied可推进数据指纹/full_sync_deadline；显示pending不宣称已写屏。

### 3.3 大数据与旧设备

联合请求是否能走BLE，按加入全部身份/时钟/Plan/Data信封后的实际编码长度与命令/分片预算判断，不能只沿用fields<=1800的判断。超过联合预算直接冻结Plan+同一数据意图，BLE先发Plan；认证HTTP ready与handoff后经既有/api/data投递，不先发一个半包再升档。

旧设备未声明能力：明确走Plan-first，再Data；Plan接受/认证确认后才尝试Data；旧设备若在Plan ACK后关闭BLE，必须验证合法HTTP窗口后投递，不能继续向已关闭GATT写Data。旧能力降级记录可见；已发送联合请求超时不据此降级并换ID重发。不改变HTTP优先和USB continuous的有效管理语义。

## 4. Bridge 决策、持久化与恢复

1. 在真实push可见值/缺失/质量变化被识别时，记录该设备的有界活跃意图，首次派发前冻结对应截止。首版复用现有MAX_LIGHT_S=600s与light_hold_until的合并机制；不同用户显式hold/post-OTA取既有最大值，不被push覆盖缩短。pull-only、周期完整同步、读取、恢复缓存、重新设合同以及重复poll不伪造成新push。
2. 先记录真实push事件；深睡设备只能等下一自然机会。在第一次合法派发该事件时冻结有界窗口与Plan，设备期限仍从实际接受Plan时起算，Bridge依据真实remaining校正镜像，不能把投递前的离线时间误算为设备已获得light。后续会合、重试及恢复不为同一事件重新加600秒或换PlanID；期内新的真实push允许生成新的显式意图。已接受窗口到期仍未确认时进入明确window_expired/unknown并保留查询与用户重试入口，不能因旧dirty无限续开窗。首次未派发的最新数据仍保留至下一合法机会。P1为这个生命周期写出时间轴测试，禁止混用源观察时间、派发时间和设备接受时间。
3. 原有current context、确认指纹、in_flight存在时，先核对旧投递；新变化只更新最新缓存，不改冻结中的数据内容/CRC/ID。同MAC仍串行；独立MAC隔离。冻结后的Plan不因本轮Data ACK而立即撤回。
4. 发送前使用现有platform原子存储保存一份该设备未完成联合意图：request_id、会话/目标、Plan内容/原截止、Data原seq/CRC/必要完整payload、分项确认状态。现有state已保存Plan和context，但要检查并补足in_flight恢复，不能只保存next_seq丢掉原包。避免复制两套payload。
5. 使用现有BLE `command_with_id` 与HTTP session_post关联路径；查询和重试沿用原request_id/plan_id/data_seq。不把每次command()的新ID当作原业务重试。Plan同ID同内容重复返回真实剩余时间，Data同seq同CRC重复不刷新。
6. 收不到联合ACK时进入有限恢复：在仍合法的窗口中，经认证BLE/HTTP status核对目标、设备会话/boot、context、Plan身份/内容、data_seq+CRC与接受/显示事实。只证明一项就只确认一项；不得单凭applied_seq97补齐整个结果。
7. status缺少原内容核对字段时，扩展共享status为最小当前接受摘要（Plan必要内容、Data当前context/seq/CRC），或复用有界结果查询；P1选择最少持久状态方案。不建历史回执仓库；联合请求仅保留当前最小关联摘要（request_id与Plan/Data内容指纹），同request_id变更内容必须明拒。摘要被后续合法事务覆盖而无法证明旧结果时明确unknown，保留原查询入口；资源预算覆盖该有界摘要。
8. 实际已发送且结果未知时先查询；只有证明未应用且原会话/权限/预算有效才按冻结内容重试。Bridge重启保留原ID；冷启动nonce变化不重放旧会话，不自动将未知事务换ID。设备恢复同context的Data须以保留checkpoint的CRC证明；Plan跨会话接受事实单独判断。
9. 未派发、确定拒绝、过期缓存/context、窗口预算耗尽、结果未知均给出阶段及处理入口；自动恢复有次数/总时间上限和退避，不无限显示accepted。冻结旧快照确认后，如最新缓存仍有真实差异，再生成新数据序号与对应有效Plan意图。

## 5. 按依赖执行的任务

任务表记录本计划执行状态；产品待办只在backlog A9维护。

| 阶段 | 交付与主要文件 | 通过条件 |
|---|---|---|
| P1 合同与生产red | 冻结能力/信封/分项错误/查询摘要；core coordinator/service与app platform/rendezvous生产测试seam | weekly66→65：成功Data也保持light；丢Data ACK不阻止Plan送达；Bridge重启恢复原意图，均先得到目标red |
| P2 共享设备处理 | src/v2_plan_command、v2_data_command、v2_command_envelope、v2_status_snapshot；main/rendezvous adapter；device-sim生产HAL | 两通道同义处理；Plan先应用；partial明报；重复不续租/不重复绘制；有限内存/失败路径正确 |
| P3 Bridge联合冻结与发送 | core coordinator/platform model/service/store；app rendezvous/platform；core device_client与ble command_with_id调用 | 同轮Plan+Data、小包实际长度门禁、旧设备Plan-first、大包HTTP；发送前落盘、ACK后hold保留 |
| P4 恢复与状态投影 | 同一服务结果核对；设备页/MCP现有投影按需修改 | 丢ACK通过context+CRC/Plan摘要分项确认；确定拒绝终止；超时返回unknown；无盲换ID重发 |
| P5 软件与资源验收 | 生产C++/Fake、Rust受影响crate、协议/权威设计文档同步；条件性UI回归 | 下表矩阵通过；计划/HTTP/handoff/PM独立断言；原Wi-Fi作业、BOOT、owner不回归 |
| P6 候选构建与实机交付 | 仅Note4 ROM与默认Bridge候选、私有数据备份/回滚、部署现场 | 先软件/资源门禁，再按届时授权部署；ROM和运行hash核验；真实自然会合/light/deep证据，未验项不结案 |

P2/P3共同依赖P1冻结合同；采用顺序垂直切片实施，当前不要求多agent或并行固件构建。必要的新文件限定在现有模块职责内。不先删除旧独立Data/Plan入口，不改现有模板CRC/编译ABI。

## 6. 必须覆盖的验收矩阵

| 场景 | 核心断言 |
|---|---|
| weekly66→65，小数据，同次Data成功 | 请求发送前Plan已冻结为light；联合接受；Data ACK不撤销有界hold；可达后走认证handoff |
| 联合ACK整包丢失 | Plan已经能应用；原ID查询确认两项，不生成新计划续期 |
| Plan应用/Data未应用、分项ACK或设备复位 | 返回partial/unknown；逐项确认，不误更新数据指纹 |
| 设备applied_seq相同但CRC/context不同 | 拒绝伪确认；保留冲突反馈，不转成功 |
| 同request/plan/seq/CRC重试，多次连接 | 原deadline不变，重复Data不重复绘制 |
| 同PlanID内容变化、同seq不同CRC、旧ID | 明确conflict/stale，Data不得越过失败Plan |
| 新数据在旧请求待确认期间到来 | 原payload不漂移；旧ACK不确认新快照；确认后才排新seq |
| Bridge发送前/发送后/只确认Plan后重启 | 原ID与原包恢复，无丢失in_flight、无自动新600s |
| 单纯pull变化/刷新缓存/重读合同 | 不增加push活跃截止、不推空包、不生成新light |
| full-sync到期、长时间离线与已接受窗口过期 | 分开计划原因；首派发前离线不吃掉设备grant，已接受后的旧push不循环开新窗口 |
| 超过完整联合BLE预算/MTU分片错误 | 发送前选Plan→HTTP；整包拒绝有界，无半包升档 |
| 慢Wi-Fi、HTTP200业务拒绝、handoff丢ACK | ready有界等待；分项结果真实；期限到点退出 |
| 无token/错token/他人owner/lease到期 | 保留401/409及显式claim，无旁路占用 |
| BOOT、timer、USB continuous、低电 | BOOT原截止、timer无兜底；USB不误算0预算；低电优先 |
| Plan到期后的延迟数据/重复请求 | 不凭重复请求续租；只读旧结果可确认，首次写入遵守有效窗口 |
| 排队Wi-Fi分页读+诊断complete+数据 | 真实app组合顺序，complete不提前关闭其他已准入工作的有效期限 |
| 旧设备/能力缺失/未知扩展版本 | 正确Plan-first或明确unsupported；不能超时后静默降级重写 |
| light入口、静默到期、CPU睡眠 | Plan ACK、认证HTTP ready、handoff/BLE OFF、PM计数分别验证；到期停止新业务并正常deep |

必须经过生产 `ble_cycle → 共享service → 同源设备处理 → HTTP proof/handoff → 投递/恢复` 的组合seam；不能只测service或Fake happy path。实机未注入的ACK丢失只能记软件覆盖，不能冒充RF故障闭环。

## 7. 验证命令、资源与部署证据

- 先运行受影响core/app/ble/device-sim及同源C++测试；Fake串行，沿现有ArduinoJson路径和隔离target-dir。所有自动生成新目录先require_escalated预建；不删除已有target/debug或运行data。
- 如修改设备页生产HTML：运行 `node tools/test-device-page.mjs` 和 `bash tools/test-wifi-ui.sh`，审阅窄/宽截图；WK合成IPC只证明界面，不证明真实通信。
- 测量无data、典型data、联合BLE上限请求的编码字节、峰值堆、栈；设备仅保留当前Plan/Data最小摘要，分配失败明确返回。Bridge持久意图按实际payload使用空间；不在设备堆栈构造整仓库副本。
- 若改变RTC保留摘要，覆盖timer/deep校验与冷复位失效；若确需新增NVS格式，必须显式版本/长度/校验并提供空/单/典型/上限字节、掉电/损坏/读回矩阵。P1优先无需新NVS格式。
- Note4实际ROM核对真实调用链和栈预算，运行 `python3 tools/check-wifi-stack.py <matching.elf>`；宿主测试不能代替嵌入式门禁。
- 只构建 `pio run -e zectrix-note4-b`，保留增量缓存；版本只用版本号-note4，登记ROM路径、marker、大小、SHA256与匹配ELF。两个以上pio进程禁止。
- 软件过门后，按届时授权只部署当前默认Bridge并保留身份/data/原事务；OTA核对登记MAC、认证目标与独立运行镜像hash。既有回退包和私有数据备份保留，不清NVS/分区/dump。
- 实机至少Note4电池自然timer会合连续10轮，包含真实或明确标记的合成push→联合请求→HTTP验证→BLE OFF，以及静默到期→deep；另验USB continuous。CPU实际light-sleep用两次有界PM计数差与锁证明；USB阻锁单列，不把逻辑light等同CPU睡眠或电流达标。
- 保存关联MAC/context/wake/request/plan/data序号与阶段耗时的脱敏证据；不记录token、SSID密码、完整dump。报告测试数、构建与部署hash、真实/合成边界。最终 `git diff --check`，更新权威设计/PROGRESS及backlog A9完成状态，未经要求不提交。

## 8. 当前交付状态

仅完成现场归因与本执行计划。生产代码、协议能力、设备固件、Bridge运行实例仍沿最新PROGRESS已部署版本；ACK丢失原因未定位，未进行新网络/串口/配置/刷写操作。完成判据是联合电源意图与数据结果可分别证实，并通过软件与所授权实机验收；不是“改成一个JSON包”就结项。
