# 统一设备状态与HTTP/BLE协议执行

2026-10-04。现有dirty修改保留，未提交。权威设计wifi-unified-protocol-design.md及一次审核wifi-unified-protocol-review.md；待办唯一来源backlog B6。

顺序：U0先实施WFS4回执高水位/确认退役、生产C++及Bridge持久作业回归；U1共享auth/availability入口；U2稳定principal与canonical peer/namespace原子关联；U3认证HTTP直通/status；U4 BLE适配/异步终态；U5 Bridge调度/UI/通知与恢复；U6资源、唯一Note4构建、配套部署及实体证据。

既有任务f287ac24cd546159保持原ID，以实际受控data为准，不取消/另建重发；不改真实网络进行测试，不删除data/构建缓存。每阶段记录验证与未验边界，宿主/WK不冒充实体。

状态：U0–U5核心已实施并通过相关软件回归；U6配套Bridge/Note4已部署，运行镜像已核验；原同步任务的实体闭环被macOS蓝牙权限阻挡，待一次合法绑定修复。B6仍开放，未将宿主/WK/一次部署算成全部RF验收。

## 软件证据（2026-10-05）

- Rust核心120、App66/1既有ignored、BLE17、HTTP集成21、Fake51：合计275 passed/1 ignored，日志artifacts/bridge-macos/wifi-unified-final-rust.log。新增协议3生产客户端USB无permit、丢ACK、Bridge数据重开只查原ID、设备进程重启永久退役围栏、principal伪造拒绝；共享BLE现有512B属性/丢ACK/代际及旧协议回归保留。Fake BLE用例的进程环境变量现以互斥锁隔离，避免并行注册表覆盖。
- 生产C++ ASan/UBSan：wifi-unified-management2.log/storage2.log/endpoints2.log。确认退役/旧ID永久保留、跨principal、提交后错误恢复、严格claim、stale nonce只允许回执重放、pending先于RF、active与终态同提交、重启trial未知、截止后不恢复RF；WFS3/raw-v2/WFZ2/v1、损坏/截断/长度/容量/分配失败、BRS2原子迁移/同peer多principal/物理token证明修复/满8项不淘汰均通过。
- 生产HTML JS通过，原生WK46场景通过（wifi-unified-js2.log/ui3.log），560/1000在线USB、BOOT及任务/完整导入编辑同步截图已审阅；使用合成IPC，不是实体网络证据。
- 真Xtensa ELF门禁wifi-unified-stack-final.log：setup640、共同handler400、manager688、HTTP入口448、DeviceStatus592、定向通知688、endpoint upsert128/auth64 B。最大Wi-Fi局部链HTTP入口2896 B，loop帧496 B；NVS/间接库/ISR内部与allocator overhead单列，不用宿主栈替代。静态RAM118476 B（较.60增加552 B）。

## 存储及RAM预算

WFS4按实际网络/回执/cursor动态分配；store3原子blob支持WFS3只读，首次成功写入才变WFS4，老键只在验证新格式后删除。下表为生产编码与请求分配测量；宿主峰值包含宿主NVS向量成本，不等于ESP整机heap测量。

| 网络/回执 | 编码B | 字符分配B | 宿主仓库heap B | clone+save额外峰值B |
|---|---:|---:|---:|---:|
| 0/0 |33|0|0|290|
| 1/1 |235|169|417|1127|
| 2/2 |407|278|774|1828|
| 12/8（最大字段） |5116|4602|7002|17474|

叠加8个最大cursor、8个activate元数据及独立旧active凭据：6310 B blob、宿主heap8452 B、额外峰值21320 B；格式允许16B operation时保守编码上限6374 B，小于8192门槛。ESP真实Network56/Receipt96/Cursor24 B，动态成本依条目计；clone+2份encode/readback是主要峰值。旧兼容大结构仅堆上短期读取，不压小栈。cursor容量独立最多8，确认后不删除，防止过时请求重执行；未发送legacy任务保持ID，执行后其legacy回执不冒险删除。

BRS2：空仓库0持久字节、无空slot分配；单条典型108 B、两条典型187 B、8条典型697 B；所有字段上限8条2856 B（4096门槛）。target EndpointRec120 B；按Arduino String实际SSO/16B容量向上取整预算，空/1条/典型2条/上限8条动态RAM约0/216/368/4416 B，同规模更新旧+新+两份blob的请求峰值界0/648/1110/14544 B，另计NVS和allocator内部。实际BRS与WFS都未预分配容量上限。新记录和身份关联一次原子写入读回，旧分键验证后退役，不LRU淘汰。

## 部署记录

最终冻结：artifacts/bridge-macos/wifi-unified-candidate.json。唯一zectrix-note4-b/0.18.61-note4，ROM 1874080 B，SHA256 `578EEDE7CEBEF30AC3DB016B40B78F3F204C30E78AEF3F9305AEA3DED703E3F4`；匹配ELF SHA256 `7F3A79599D553D95C15B864A8213C0B490C1DE7B275467BB664697136A24E0BD`。OTA a9811cd3 succeeded/image_verified，独立设备操作token读取运行前缀hash、MAC7C4FADB93408、target zectrix-note4-400x300、版本及ota_0均匹配（wifi-unified-live-final.json）。

默认app SHA256 `B65A64FE736746E22108511AA2ACA889E0B2622FC6E2E6416DD3A686D3F24D60`，默认LaunchAgent supervisor33585/health204；候选/部署hash及签名验证一致，安装身份hash保持。私有data与installation-id备份、旧app路径见wifi-unified-backup.json；只操作默认实例。原任务f287ac24cd546159/request_id同值、冻结目标保留，未取消/重建/另ID发送，当前waiting_rendezvous/sent=false。任务缓存revision1、设备revision3；新增认证逐目标摘要+凭据hash及相同boot/revision再读确认，只在冻结意图全部已满足时用verified_post_state完成原任务；否则保留CAS冲突，不强制覆盖，不把部分确认提升成完整快照。

实机当前boot3c2c02cc5c74c3311c85d6a28386e26f（OTA软件复位原因3），USB continuous/HTTP ready。BLE随实际正式/物理窗口开关，维护窗口剩余0；有样本BLE关闭且HTTP仍ready，最终任务闭环须再选BLE关闭/light0现场确认。认证wifi_control仍binding_required，principal/namespace为空；WFS3 ready/uncertain=false/321B/2网络/1legacy回执/revision3/cursors0，新格式待首次成功写入后验证，不将加载WFS3说成WFS4实机迁移。

一次既有pair USB命令使用raw端口、无DTR/RTS ioctl、清HUPCL；boot未变、无ROM reset/panic文本，原文私有保存，摘要wifi-unified-pair.json。随后Bridge authorize实际返回bluetooth_permission_required；系统权限需用户对最终已部署app允许后，才可进行合法endpoint绑定迁移。已提出用户操作问题，未绕过TCC、endpoint凭据或claim。此绑定动作不试连/启用/改写网络；保存原配置与namespace。

## 仍待实体验收

1. 用户允许最终app蓝牙权限；保现场打开有限物理窗口，完成endpoint token+canonical peer+principal绑定修复，核对旧namespace保留及同一设备MAC。
2. 保留原f287ac24cd546159，确认冻结两目标是否已达要求；若有冲突明确报告。USB continuous、BLE OFF/light0、默认Bridge重启后，经认证HTTP读取/继续任务并核验持久终态。
3. 完整电池/BOOT期限、命名实例通知、掉电、满容量及真实RF试连/换网矩阵仍归backlog B6，未以Fake/WK/一次OTA替代。未经追加测试改网意图，不擅自切换实际网络。

部署后在线提示负例：无token401、非法MAC400、未登记MAC404；不生成任务/改设备注册，证据wifi-unified-presence-negative.json。没有提交，既有RTC/NimBLE等dirty修改保留。

## 00:53新任务预算故障诊断（2026-10-05）

现场旧f287ac24cd546159已cancelled，新任务a7efde70624b7097/request w3_0000000000000010于00:53:24创建；本轮未操作取消或新建同步。00:53:24设备处于实际HTTP超时，后reset_reason8自然唤醒，认证绑定56a6/旧namespace已有效。新任务awaiting_confirmation/window_budget_exhausted/sent=true，设备同ID连续查询unknown，修订仍3。三个假设：睡眠暂不可达、发送前预算耗尽被误标sent、真实写入丢ACK；网络日志证实前者，代码预算+生产只读请求复现后两条确定缺陷。

修复计划：生产app预算函数原样提取后先回归red，协议3continuous及sent query不引用旧power/sync期限，window新写入仍绝对截止；明确发送前NotDispatched错误才能解除本次sent标记，传输中超时保持未知。HTTP只读setup查询生成临时关联ID，事务写ID不改变；Fake实际生产处理器验证shape red→green。部署仅默认Bridge，ROM不变，保留安装身份与任务。现存未知事务不自动重发；目标摘要wifi-slot-0不匹配/slot-1匹配，不能标成功。

本轮结果：三个正确回归seam均red→green：app实际预算函数continuous+旧sync0、sent query+窗口0/绝对期限；core明确零发送NotDispatched与传输中预算超时/旧sent查询的不同恢复语义；Fake实际生产C++处理器HTTP match无request_id产生shape。受影响完整测试260 passed/1既有ignored（app67/core121/HTTP21/Fake51），wifi-budget-green2.log；red证据wifi-budget-{red,dispatch-red2,match-red}.log。只读setup使用现有CRC关联ID，不为apply/trial/activate生成ID。新HTTP/BLE发送入口只在明确未发出时释放本次sent标记，旧未知事务和部分写入超时仍保留。未改UI/固件，无需重刷ROM。

01:09默认Bridge最终SHA256 `D6DC32B2CF29B4E3FA076D60219239D0A3D107754E4FEE3F79FD32DA313FA2AD`，supervisor35567/health204，签名及候选/部署hash匹配。私有备份wifi-budget-20261005-0108、旧app回滚包及身份保持证据wifi-budget-deployment.json。部署前真实USB continuous/BLE OFF/正式窗口0（wifi-budget-device-before.json）；部署后同boot、BLE OFF、HTTP正常，原事务首次查询HTTP200/135ms，error从window_budget_exhausted变receipt_unknown/last_stage query_result/attempt17（wifi-budget-job-after.json）。当前事务仍awaiting_confirmation且ID/frozen保留，没有强制清sent、取消或重发。设备namespace f0:18:98:6b:98:c6/0/本机principal56a6/read+write allowed，绑定修复已由现场状态证明；WFS3 revision3未改。

用户询问unknown及故障阶段，已解释Bridge发送前预算、先标sent后本地拒绝、查询沿用错误预算；unknown为设备无匹配回执，不等于成功或失败。只读冻结意图核对slot0不匹配/slot1匹配，不能确认成功。已询问用户是取消待确认任务并读最新修订后同步已保存两条（只保存不激活），还是保留原事务只查询；用户目前只询问原因，未授权新事务，保持当前任务。完整其他实体矩阵继续归B6。

## 用户纠正：新事务使用旧缓存，应明确拒绝（01:29后）

前述“旧事务”措辞不准确：a7efde70624b7097当时是新事务，expected revision1来自旧设备快照，设备已revision3。用户后续取消该待确认任务、read 9a99ea2ee94c5faa获得3、新sync 83cecdbef6a0e7c6以3提交4并确认durable_receipt/result_ack；实体WFS4 369B/2网络/1legacy回执/1cursor高水位19/ready/uncertain=false，同boot/USB continuous/BLE OFF，成功证据wifi-budget-sync-success.json。预算缺陷仍是当时未发出却误标sent的直接原因，不能把新事务说成用户重复旧ID。

用户要求新任务旧缓存应明确冲突而非卡住。补测计划：App真实opportunity HTTP路径遇新任务cache1/认证设备3，需conflict并停止自动调度且不mark sent/不apply/不自动读取覆盖缓存；真实Fake生产C++返回revision_conflict，core映射conflict并停止自动调度，不留awaiting_confirmation；UI将revision_conflict翻译为“设备方案已变化，缓存已过期，请重新读取设备后再同步”，保留Bridge编辑。部署默认Bridge，ROM不改，用户成功任务不操作。

## 缓存冲突修复最终验收（2026-10-05 01:49）

新App生产机会HTTP回归证明cache1/设备3在match后即conflict/revision_conflict，sent=false、不apply、!runnable，保留任务ID、缓存及Bridge编辑。conflict不是模型terminal()，当前任务仍供用户处理/取消，停止自动调度；不可将此状态误记为终态历史。真实Fake用生产C++ apply拒绝新事务过期revision，core明确映射冲突，不待确认。

Fake认证status此前缺wifi_storage.revision，是本轮测试暴露的保真缺口；render FFI调用共享生产wifiSchemesStorageStatus，sim返回真实修订/回执及相应status_seq，失败返回HTTP500/结构化stage=status。原缺口与新鲜事务设备拒绝均已覆盖，不以测试成功路径代替完整链路。UI直接显示设备缓存已过期、重新读取及Bridge配置保留。

最终受影响262 passed/1既有ignored（app68/core121/HTTP21/Fake52）及最新Fake focused测试通过；JS通过，WK48场景通过，560/1000 stale-cache截图审阅。日志wifi-stale-final.log/fake-final.log/ui-green.log/wk2.log，red日志保留用于复现。WK合成IPC不是实际RF证据。AGENTS.md已写入任何请求包含失败都应明确返回的项目合同。

默认Bridge最终SHA256 `B3AC01D2730D4F16EA6AEDBD8C4895202011A728B525378BD808A2DCF2F6C7D9`，supervisor38549/health204；签名/hash验证，安装身份和用户成功job83cecdbef6a0e7c6/requestw3_0000000000000013完整保留。私有备份 `/Users/slepher/Library/Application Support/dev.codexstatus.bridge/deployment-backups/wifi-stale-20261005-014859`；回滚app `/Users/slepher/project/codex_status/artifacts/bridge-macos/Codex Status Bridge-before-wifi-stale-20261005-014859.app`；证据wifi-stale-deployment.json。未新增实机事务、操作命名实例或改/刷ROM，Note4仍.61及578EEDE7…703E3F4。用户真实读revision3→新sync完成4/WFS4已记录；旧未知ID退役不是成功证据。完整其他实体矩阵归B6，未提交。

## 分批提交归档（2026-10-05）

用户明确要求将工作区内容按功能分批提交。请求返回规则已提交1ff5c7a，独立RTC源码/测试/说明为1f37727，NimBLE生命周期修复/回归/串口说明为33e39e8；Wi-Fi统一协议、界面往返、预算与旧缓存冲突反馈、Fake保真回归和最新现场记录随本批提交。构建产物、运行数据、凭据和私有转储保持在gitignore/仓库外，不进入提交；不重复构建或部署，不更改真实设备网络。各批沿用既有最终验证与未验边界，B6仍开放。
