# 配网与异步会合：下一步执行文档

日期：2026-10-03。状态：W0–W6已有局部软件通过记录；2026-10-04审查确认W4交接未闭环、W6组合验收不足（见第14节）。W7已按追加授权构建/部署Note4 .45和新版Mac Bridge，运行镜像证明见第12节；真实Wi-Fi配置与完整硬件验收仍未结案。本文接续 [BLE 真实性方案](fake-rom-ble-fidelity-plan.md) 的剩余 P2/P3/P4，落实 [Wi-Fi 双侧设计](wifi-redesign.md)。任务总状态仍维护在 backlog B5/B6；本文不另建一份独立 backlog。“异步交汇”在协议和代码中统一称“异步会合”。

## 1. 执行前基线与范围

先读最新 PROGRESS，再核对 B5/B6；旧 remaining-plan 的 .41/.42 现场与旧“试连成功即保存”交互仅作历史，不能覆盖本文和最新 Wi-Fi 设计。

| 已有基线 | 下一步保留并复用 |
|---|---|
| 最新 PROGRESS 记录 Note4 .44 / ble_wire=1 已部署，运行镜像和 USB 读回已核对；配套 Bridge 已升级 | 不重复实施 BLE 编码首批，也不为开始本计划重新刷机。后续现场以执行时最新记录为准 |
| INFO/Status 512 B 保护、共享 C++ 编码、Fake 属性读写分离、真实/Fake 共用 ACK 等待已实现首批 | 每项新回复继续经过同一最终字节边界；不恢复 HTTP JSON 直接当模拟 BLE ACK 的捷径 |
| BOOT 固定 120s、bond 隔离、旧 protocol=1 配网、Wi-Fi HAL 已有实现 | 保留恢复入口、legacy 网络、当前活动版本；protocol=1 本绑定摘要与试连/提交耦合不等于新版完成 |
| 发布、OTA、数据、PowerPlan 已有调度/确认机制 | 在既有 service、每设备互斥和会合事件上扩展；不重写这些机制 |
| 首批软件记录 278 passed / 1 ignored，真实一次 status/plan/sync_open ACK 成功 | 不替代剩余 Fake Wi-Fi/history 支持、跨 session/bond/分页矩阵、持久作业及全量管理验收 |

原 635 B 帧归属仍缺 op 级直接证明；原试连跳屏原因未结案。最新现场仍有 connect/INFO 超时及 HTTP data 超时，不因短摘要通过就宣布会合全部修复。诊断与本计划并行记录；Fake 验收通过不能关闭这些硬件问题。

本轮交付目标：离线准备 Bridge 方案 → 必要时用物理 BLE 维护恢复网络 → 显式读取设备完整快照 → 比较 → 按选中差异同步 → 查询/回读确认。任一暂时不可达只结束本次尝试，保留可恢复任务。

## 2. 用户动作、通道与副作用合同

| 动作 | 通道 / 执行条件 | 完成与副作用 |
|---|---|---|
| 打开设备页、刷新缓存、编辑/保存 Bridge 方案 | 本地；无需设备在线 | 不扫描、不读设备、不推送；完整展示已保存目标及旧快照时间 |
| 读取设备方案 | 提交读任务；实际会合后走认证 Wi-Fi | 取得全设备无密码完整快照才替换旧快照；不自动导入 Bridge |
| 比较、导入已有快照到 Bridge | 本地；校验快照完整性及本地修订 | 密码未比较；Bridge 缺项不推断设备删除；导入不写设备 |
| 重新读取后导入 Bridge | 读/导入任务；Wi-Fi | 读取后重验本地修订/草稿，冲突时暂停，不覆盖新编辑 |
| 同步选中项到设备 | Wi-Fi 原子变更；有效维护授权 | 保存方案/待启用版本，不切网、不重启、不改变显示 Bundle/context |
| 断网恢复配网 | 已绑定加密 BLE，物理 BOOT 维护窗口 | 只进行恢复所需的候选输入、有界试连、显式保存和启用；必要摘要分页。不能把整套日常比较转到 BLE |
| 临时试连 | 当前用户发起，维护/正式窗口有足够预算 | 候选 RAM；成功/失败/取消均恢复原连接，不保存、不自动启用。无原连接时返回原断网状态 |
| 显式启用 / 自动选网 | 已保存网络 ID+版本；owner 与有效窗口 | IP 成功后才确认活动引用，失败恢复原引用/连接；自动选网只用已确认同步的映射 |

日常 BLE 继续只交换身份、会话、关键序号/修订、短 ACK、预算内的小数据和 Wi-Fi 窗口请求。完整状态、全部方案、批量同步、详细诊断、Bundle/OTA 走 Wi-Fi。512 B 是硬上限，不是目标大小；申请窗口不能冒用 periodic sync 条件或自动抢 owner。

断网首次接入必须能完成“试连 → 保存 → 启用”三个明确动作。试连成功后的临时 IP 不是持续在线证明；恢复旧状态后也不能偷偷把新网络设为活动。页面提供同一流程的后续按钮，减少重新输入，保持含义独立。

## 3. W0 必须先冻结的设备合同

在 wifi-protocol.md 增补新版合同并标明能力；不要把既有 wifi_setup_protocol=1 改名后宣称已有。以下是要落定的语义清单，具体端点/字段名在 W0 定稿：

1. **能力**：分别报告全量投影、原子选中变更、持久结果查询、临时试连与显式启用。旧能力不足返回明确 unsupported；不自动降级成旧试连/保存/启用组合。
2. **全量快照**：精确 MAC、schema、配置 revision、snapshot_id、complete、归属/可写范围、稳定 scheme/network ID、网络元数据、legacy、容量、当前活动 ID/版本。完整分页冻结，游标过期/跨修订重开，部分结果不能用于缺项或删除判定。
3. **归属与权限**：全量可见不等于跨 bond 可写。其他 bond/legacy 初版只读；普通配置编辑需维护授权，启用/自动选网检查 owner。HTTP endpoint token 本身不冒充物理授权或 bond。
4. **BLE → Wi-Fi 授权桥接**：若维护授权需跨链路使用，采用绑定 MAC、已验证 bond、Bridge 安装身份、允许操作及原物理截止的短期设备许可；断链/重连不得自动扩大范围或重置期限。许可只在受认证链路使用，重启/过期后重新授权。W0 明确实现，不靠请求自报 bond/授权。
5. **原子变更**：expected_revision + 稳定 request_id + 用户批准范围；整批验证容量、权限、字段、凭据意图和活动版本保护后，一次提交。无原子能力时只能明确单项结果，不能标整批成功。
6. **持久结果**：查询同一事务的 not_committed / committed(revision) / rejected / unknown 等结果，定义可保留条数、保留期限、重启恢复及容量满处理。保证等待中的请求不被无提示挤掉；在业务提交边界记录结果，避免“配置已保存而结果记录不存在”的空隙。
7. **重放**：同一安装身份/归属、request_id 与同一业务内容复用结果；同 ID 不同内容拒绝冲突。会话 nonce/wake 等认证封装会变化，不能当作不同业务变更或允许更换目标。结果过期/无法判断时先回读，只能报告已验证的目标状态，不能伪造原事务成功。
8. **试连与启用**：共用既有 Wi-Fi 状态机/HAL，拆开 RAM 候选、持久版本和活动引用；错密码、DHCP、存储失败、断链/到期都保留旧记录。取消已提交写入不能假回滚；低电关机优先。
9. **迁移**：旧单 blob、legacy namespace 与活动配置保留。新 revision/ID 持久稳定，启动失败不清库；损坏/不兼容明确报错。试连不切模板、不调用启动画面路径；复位后标结果未确认。

W0 输出：端点与字段表、错误分类表、各能力兼容行为、存储提交/结果查询边界、维护授权与窗口预算表。实现者可自主定稿这些技术细节；遇到改变上述用户语义的选择才需要补充说明。

## 4. Bridge 三份数据与持久作业

按精确 MAC 与本机绑定归属保存三份记录：Bridge 目标（本地 revision）、设备完整快照（设备 revision/snapshot/time）、已确认同步字段基线。运行 IP/连接状态不进入配置相等比较。UI 草稿与已保存目标分开，普通 state、日志和测试产物不存明文凭据。

在 core platform model/service 中增加最小 Wi-Fi 数据和作业记录，复用既有原子持久化。逐项落地：

- job_id、目标 MAC、归属/安装身份、任务类型、创建/到期时间、稳定业务 request_id。
- 冻结的本地修订、同步方向、选中项/字段、原设备基线与非秘密变更；本地后续编辑不改在途任务。
- 状态、attempt_count、最近阶段/时间、下一次最早尝试时间、结构化错误码、确认修订/已确认范围。
- 凭据仅持有内存输入或用户主动选择的系统安全引用；重启缺失转 waiting_input。发送过的任务必须先查结果，不能用同 ID 发送重新输入但不同的密码。无法恢复原输入且确认未提交时，显式建立关联的新任务。
- 同 MAC 的相同读范围合并；相同冻结意图复用任务。未发送任务可显式替换，已发送任务先完成确认/取消处理，不能静默覆盖。
- 不存认证 token/原始载荷/凭据摘要作普通审计；输出仅阶段、错误码和脱敏关联。

### 状态与判定

| 状态 | 进入 / 退出规则 |
|---|---|
| queued / waiting_rendezvous | 持久意图已接受；等真正联系机会。设备不可达、深睡、扫描无目标不判终态 |
| waiting_authorization / waiting_input | 缺维护许可、凭据或需要用户处理占用；保留任务，条件补足再继续，不强制 claim |
| connecting / reading / applying | 本次有界执行；UI 可查询任务，不能长期持有全局连接锁等待下一会合 |
| awaiting_confirmation | 请求已发出但提交结果不确定；下一会合先查原 ID/回读，不能直接重写 |
| conflict | 会合时所批准字段或本地导入目标已变；暂停并保留双方值 |
| completed | 完整读取已验证；写入须确认持久修订及选中字段。导入仅证明本地已保存 |
| cancelled / expired / failed | 明确取消/意图到期/无效请求或确定不支持等终态；已经发送的请求仍保留 unknown 或实际确认范围，不能当作未生效 |

只读失败返回 waiting_rendezvous；写入丢 ACK 返回 awaiting_confirmation；明确业务拒绝分类后暂停/终止，不能一律重试。job cancelled/expired 仅停止后续调度，并不抹掉远端可能提交这一事实；必要时由另一个显式查询/回读动作澄清。

建议读/配置同步意图默认 24h；5/15/60s 退避只是再次尝试的最早时间，且由真实会合事件触发，不按时强扫/唤醒。临时试连与启用是较强时效动作，不套用 24h 自动执行：试连只在当前维护窗口内发起，错过机会转待用户重新发起；启用明确显示有效期，执行前检查版本、owner、主机网络和窗口，条件变化暂停。W0 固定最终期限，测试用虚拟时钟。

## 5. 实际会合调度步骤

1. 用户提交任务，先校验本地请求并持久化，再返回 job_id；不能先连设备再决定是否排队。
2. 等既有 device_first/BLE、UDP 指向或已验证 HTTP 可用事件。广播/IP 只是候选，先核对精确 MAC、绑定加密/认证、能力及本次可用预算。
3. 优先完成现有身份、必要 Data/Plan/ACK；任务借用合法正式 Wi-Fi 窗口或明确授权的维护机会。只读不要自动抢占 owner，不能为任务延长 RF。
4. 按设备 delivery lock 执行一次，重读配置 revision。已批准字段仍匹配基线、仅无关字段变化时可重建 expected_revision；批准字段变了则 conflict。不能自动扩大用户勾选范围。
5. 全量读取使用认证 HTTP 冻结分页；成功前旧完整表保持可见。写入使用冻结意图与稳定 request_id；重连更新会话封装不更换业务身份。
6. 明确提交后读取 revision/目标字段，再记录已发送修订对应基线。丢 ACK/断链先查原事务；查询未知就回读并保留不确定性，不能“重试直到成功”重复改网。
7. 单次结束立即释放连接/互斥，保存结果或等待状态。面板关闭、切换设备不取消；异步 UI 回报核对 job_id/MAC/页面 generation。

授权窗口过期只暂停新的写入；已经可能提交的任务仍可在合法读取机会查询结果。Bridge 暂停/系统睡眠时不偷偷触发设备，恢复后接续持久任务和期限。

## 6. 已设计的 Bridge 配网界面：执行要求

本节纳入此前界面，不另起一个只有单网络表单的配网页。权威产品语义仍是 wifi-redesign.md；以下 HTML 是示例数据与交互参考，不是实际功能或读回证据：

- 最新双侧示意：[wifi-plan-sync.html](/Users/slepher/Documents/Codex/2026-10-03/bridge/wifi-plan-sync.html)。当前任务目录副本用于可视化访问，仓库参考在 artifacts/bridge-macos/wifi-plan-sync.html；不得再以不可访问的旧引用作为交付入口。
- 前版完整表格/绑定分组：[wifi-manager-redesign.html](../../artifacts/bridge-macos/wifi-manager-redesign.html)。保留分组、归属与只读展示参考；其旧设备唯一来源、直接连接失败结束等行为不覆盖最新双侧/异步语义。
- 当前生产入口为 app/ui/index.html 的设备页 Wi-Fi 区域，UI、MCP使用共用application service；不要把示例HTML直接复制成一个静态“成功”页面。

### 页面与布局

设备页保留“Bridge 方案 / 设备方案 / 比较与同步”三个独立页签（必须实现 tab，不以纵向堆叠替代）；默认 Bridge 方案。切换仅改变可见性，保留编辑/草稿/选择/预览，不调用设备。选中页签内完整网络列表可见。比较区以双侧表格为主，列“选择/差异、Bridge目标值、设备实际值、本次处理”，小屏双方值上下排列；设备侧可按本机bond、其他bond、legacy分组，组内仍展示完整网络项与数量。只显示当前连接或当前bond的一项不通过验收。

页头固定目标设备名/MAC；状态区显示主机网络及读取权限、设备当前连接/活动版本（含观测时间）、Bridge保存时间、本地未保存修改、设备快照revision/完整性/读取时间。只有运行状态缺失时标未知，不阻止离线编辑，也不把旧IP当在线证明。

| 区域 / 控件 | 必须实现的交互 |
|---|---|
| Bridge方案 | 离线新增、编辑、复制、明确删除网络；稳定ID与显示名分开，主机SSID→设备SSID映射、凭据处理方式；“保存到Bridge”仅本地。取消编辑不改已保存版 |
| 设备方案 | “读取设备方案”创建读任务；未读为空态但Bridge可编辑；完整快照可查看本机/其他绑定/legacy，后两者标只读，不隐藏 |
| 比较与同步 | 展示一致/仅一侧有/单侧修改/双方冲突/无基线/明确删除/快照未知；按字段展开。选择方向和项后预览，列清新增/更新/删除/凭据操作 |
| 双向同步 | Bridge→设备修改远端配置；设备→Bridge导入本地。导入已有快照立即完成，“刷新后导入”创建任务；不能用一个按钮混合两个方向 |
| 试连/启用 | 独立于保存和同步；目标ID/版本明确，试连报告关联/DHCP/HTTP并恢复，启用显示当前活动与目标版本及独立结果 |
| 离线添加/恢复配网 | 设备页原入口接蓝牙发现/精确身份/绑定能力验证；已绑定正常重连，不默认重配。未知MAC发现待用户选择后才建立指定设备任务 |
| 作业状态条 | 同页显示任务类型、目标MAC、阶段、次数、最近错误/尝试、等待条件、到期时间、取消/后续动作；保持编辑、选择和旧完整表 |
| 联系诊断 | “刷新缓存”和“从设备读取”明确不同；广播发现/身份核对/业务ACK/完整状态分开，最近失败阶段可见，不只有尚无联系 |

“全选差异”不自动包含删除、只读、未处理冲突、未知快照或缺必要凭据项；设备密码永远未比较，不显示或导入秘密。选中项同步不自动激活；新设备缺密码可保留目标并等待输入，不能偷用来源设备凭据。

### 异步交互替换与界面验收

现有双侧示意的离线分支只提示等待、没有真实持久任务。实施时改接job_id状态，不沿用“按钮invoke失败→操作失败结束”的行为。点击提交后立即显示已排队；下次会合进入连接/读取/同步，写后断链显示待确认；任务成功必须来自第5节定义的确认链，示意数据的成功文字不能作证据。

关闭页签/切设备不取消任务，返回后恢复任务视图。MAC、job_id、页面generation与请求序号校验后再更新；异步读回不抹草稿，同步仅确认发送版。再试一次复用符合条件的任务，不悄悄扩大批准范围或反复改网。

设备离线、修订冲突、维护授权过期、密码缺失、仅设备项、其他绑定只读、部分分页、发送后丢ACK、重启恢复、主机定位未授权都要有可复现页面状态。JS测试沿 tools/test-device-page.mjs 扩展真实渲染/事件与service返回；UI验收至少覆盖A01–A06、A09–A13，按钮状态、预览范围、草稿保留及旧结果隔离。人工观感可参考原示意，但生产数据链和错误语义必须按本文实现。

## 7. 分阶段文件与完成条件

| 阶段 | 主要工作 / 文件 | 完成条件 |
|---|---|---|
| W0 接续和新版合同 | 本文、wifi-protocol.md；核对 PROGRESS/B6 与 fidelity §11/§12 | 明确第3节全部合同及兼容边界；旧现场/旧功能不混用。不重复已完成首批 |
| W1 本地目标与作业最小切片 | core/platform/model.rs、service.rs、store.rs；app/src/wifi.rs、platform.rs | 离线保存目标不连设备；提交读任务立即返回ID，首次不可达保留；持久恢复/取消/期限/多MAC隔离软件通过 |
| W2 设备事务和全量读取 | src/wifi_scheme.h、wifi_setup.{h,cpp}、main.cpp；必要最小共享 HTTP/Wi-Fi 合同；core/device_client | 全量无密码快照、revision、原子选中变更、持久查询、迁移/授权实现；试连/保存/启用独立。仅已有 protocol=1 不能通过 |
| W3 Fake Wi-Fi/HAL 与真实客户端 | device-sim/src/main.rs、tests/bootstrap.rs；render 宿主/FFI与 Wi-Fi test HAL；BLE/src/lib.rs | Fake 用生产 Wi-Fi 逻辑/编码，支持真实能力，不硬开 capability；属性与 HTTP 模型可组合，跨bond/session/分页/写后丢ACK矩阵通过 |
| W4 读取/同步与会合闭环 | app wifi/platform、core service、现有会合调度；共用 UI/MCP application service | 第5节完整执行，稳定ID、授权转接、冲突/待确认、Bridge重启后续办；既有发布/OTA/Data/PowerPlan无行为回退 |
| W5 完整界面与主机映射 | app/ui/index.html、host_wifi平台实现及 test-device-page | 第6节原设计界面全部落地：双侧表格、设备绑定分组、差异预览/方向、离线恢复、状态条/失败阶段/缓存时间明确；换网只用已确认映射，草稿不暗切 |
| W6 软件验收与对账 | 宿主、BLE、Fake bootstrap、core/app及设备页测试；PROGRESS/B6 | 第8节逐行有证据；真实客户端走Fake而非直接HTTP成功断言；无凭据泄漏，未实现项不标完成 |
| W7 需要时的构建与现场确认 | 既有 macOS打包、Note4标准发布/回退流程 | 实现/软件放行后再按授权进行；精确目标、备份、ROM/App哈希与运行证明记录。硬件问题和缺席平台另列 |

W1 可先交付“异步读取接受/等待/重启恢复”这一小切片，并在设备无新版能力时如实报 unsupported；不能为演示闭环伪造完整快照。W2 的事务/投影应逐个用宿主回归，W3 对接同一生产逻辑；W4 再接真实作业闭环。有界配网恢复流程先于自动选网，自动选网先只用已有已确认 ID，不依赖未完成全量UI。

实施前 Fake 的 wifi_setup_protocol=0；本批已接生产 Wi-Fi/HAL 并报告 protocol=1、wifi_management=2，证据与剩余边界见第11节。现有宿主 Wi-Fi 测试通过不能冒充 Fake 全流程已验。复用已有 build/FFI/存储边界，优先小纯函数/HAL；不为本文建立通用插件框架、第二任务引擎或整板模拟器。

## 8. Fake ROM 主验收矩阵

所有测试以隔离临时数据/虚拟时钟/假凭据运行，检查最终配置、任务状态、事务次数、活动引用和显示/context；不能只看 API 200 或一个 result。设备能力由实现支持的功能决定。

| 编号 | 注入 / 操作 | 必须观察到的结果 |
|---|---|---|
| A01 | 离线编辑保存；仅打开页面/刷新缓存 | 无设备请求、无发布/切网，Bridge修订保存，旧快照标时间 |
| A02 | 离线提交读取，连接/INFO/HTTP超时；推进下一会合 | 返回稳定job_id；第一次未结案；下一机会完整读取并完成 |
| A03 | 重复点击、Bridge重启、关闭面板/切换MAC | 读任务合并/恢复，不串设备，不覆盖新草稿，旧UI回报不污染当前页 |
| A04 | 多bond和legacy全量投影；密码未知；只读项 | 所有方案可见，归属正确；密码不回读/不比较，跨归属写入拒绝 |
| A05 | 冻结分页中断/过期/跨revision | 不替换旧完整快照、不推断缺项；新快照重新读取，complete后才比较 |
| A06 | 无基线/仅一侧有/双侧修改/显式删除 | 正确差异；无默认整份覆盖、无隐式删除；活动项与只读项受保护 |
| A07 | 保存/试连/启用分别执行，SSID关联/DHCP/HTTP失败 | 保存不切网；试连恢复原连接/画面；启用成功才改活动引用；失败不丢旧记录 |
| A08 | 批量容量/权限/凭据校验失败、存储失败、重启恢复 | 整批不半提交，旧/新一致；持久revision及事务结果可查；不清legacy |
| A09 | 设备提交成功，BLE属性写失败或HTTP ACK丢失，再会合 | awaiting_confirmation；原ID查结果+回读；配置提交次数仍为1 |
| A10 | Bridge发出后崩溃；密码只在内存；事务结果过期 | 元数据恢复，先确认；缺密码待输入，不输出秘密，不同密码不重放同ID；不伪报原事务成功 |
| A11 | 排队期间目标字段变/无关字段变；本地再次编辑 | 目标变则conflict；无关变只执行已批准范围；仅发送版进入基线，新编辑仍待同步 |
| A12 | 取消/过期发生在发送前、发送后、已提交后 | 前者不发送；后者停止后续动作但保留远端可能生效；不假回滚 |
| A13 | 维护许可缺失/过期、owner变化、错误MAC/session/wake | 正确等待授权/处理占用；不强claim、不重置窗口、不消费过期ACK |
| A14 | 511/512/513、合成635帧、转义UTF-8/最长字段 | 生产编码与Fake属性路径一致；超限不当成功；读失败/丢通知与无ACK分别验证 |
| A15 | 主机换网事件连续发生，无权限/无映射/有线 | 合并旧事件，执行前重验当前主机与owner；不使用草稿，不来回切网 |
| A16 | Wi-Fi工作窗口不足、休眠/恢复、OTA或发布在途 | 本次及时释放资源，任务保留；Data/Plan优先和既有互斥/截止无回退 |
| A17 | 当前只读状态刷新/恢复导入/诊断读入口 | 复用已做好的会合基础，明确新采集与缓存；导入前重验本地，诊断限频 |

A02/A09/A14 至少从 DeviceConnection、app共用service 和 Fake属性/HTTP端点组合走实际调用链；直接POST模拟接口返回JSON不能替代。新增阶段覆盖真正变更的外部行为，既有通过项不无条件反复跑。

诊断展示最少区分：发现广播、身份核对、请求已发送、ACK确认、完整状态采集；记录最近阶段与错误。它们与“最近认证成功”分别显示，广播不伪报成功，也不能把持续ACK失败藏成尚无联系。

A17 是后续可复用扩展：主动状态刷新、读取后恢复导入、pmstats等诊断读优先；模板启用/PowerPlan/占用释放另核对时效与既有机制。Bundle/OTA/数据已有延后逻辑仅补一致性回归；本地名称、模板/方案保存、导入已有快照继续即时完成，不全部改成队列。

## 9. 软件检查、交付证据与现场边界

按阶段从仓库已有入口选相关检查：

- tools/test-wifi-setup.cpp 的生产状态机HAL及新增共享合同宿主测试。
- bridge-core、bridge-app、bridge-ble、bridge-render 的相关测试；模拟端点跑 device-sim 的 bootstrap。
- tools/test-device-page.mjs 与嵌入JS语法检查；若共用工具参数/返回变更，再运行对应MCP测试。
- git diff --check，仅检查本次新增内容并辨认已有差异，不撤销别的工作。

Cargo 从 bridge/ 运行，使用全局工具链、既有 ArduinoJson 路径及隔离 artifacts target；细节沿 remaining-plan 的现有命令。HTTP模拟测试保留 NO_PROXY=127.0.0.1,localhost，不因代理失败修改业务。Fake虚拟时钟覆盖长等待，不用前台真实长睡眠替代调度模型。保留 .pio缓存、sdkconfig、AppSupport数据/安装身份/许可/owner/字段合同，不clean、不重配token、不改不在场设备。

每阶段交付：实现文件/合同变更、对应A编号的命令/结果与日志路径、实际未完成项。更新PROGRESS和B6当前事实，原计划保留历史；软件通过、构建、部署、运行确认分别记录。

主要软件验收交给Fake ROM。RF/TCC、真实USB副作用、NVS断电、电流、连续重连/自然会合成功率、试连实体屏幕跳变及缺席Windows/1.54仍独立标未验；不让这些缺席现场阻止可完成的软件工作。需要部署时先备份现有App/ROM/data，仅按精确路径替换应用、按MAC/target核对ROM并取得运行镜像证明，不仅看版本字符串。

## 10. 可复制的下一窗口执行提示

```text
请在 /Users/slepher/project/codex_status 接续配网与异步会合实现。
先读 PROGRESS.md 最新现场、docs/roadmap/backlog.md B5/B6，
再读 project-workflow/bridge-macos/wifi-rendezvous-execution.md。
同时以 wifi-redesign.md 的双侧方案语义、
wifi-protocol.md 的最新 ble_wire=1 合同、
fake-rom-ble-fidelity-plan.md §11/§12 的实际交付为基线。
从 W0 核对新版合同和未完成能力，再交付 W1 离线目标/持久读任务切片；
沿 W2–W6 完成全量方案、Fake Wi-Fi HAL、异步读/同步和界面验收。
日常BLE只用短摘要，完整读取/复杂同步走合法Wi-Fi窗口；
断网维护BLE保留有界配网，首次暂时失败保留任务，丢ACK先查询/回读。
原Bridge配网界面按本文第6节执行，纳入双侧表格、完整归属列表、差异预览、
离线添加/恢复和任务状态；HTML仅示意，不冒充真实读回或成功。
Fake必须走生产编码、512B属性边界和真实客户端等待，不直接返回业务JSON。
保留全部既有未提交改动、运行数据、安装身份、许可、字段合同、legacy网络、
活动版本、ROM/应用备份与构建缓存；不要求缺席设备，不重复已完成BLE首批。
每阶段记录实现、A编号验收证据及未完成项，B5/B6仍是总待办来源。
部署/串口等现场动作只按当前会话授权执行，本文不是额外授权。
```

原编写记录：仅新增本文并补导航/接续入口；当时未实施。后续软件交付见第11节，勿将本文旧规划时点当作当前状态。


## 11. W0–W6 软件交付与验收对账（2026-10-03，未部署）

本批实际修改生产协议/存储、Bridge service/调度/UI 与 Fake，而非只交付计划。W0–W5 源码已实现，W6 可运行软件矩阵通过；此处的“通过”只指以下调用链与注入，不表示实板、整板链接、TCC/RF 或所有时间交错已放行。W7 本轮未执行。没有运行 pio、重建/重启运行 .app、打开串口、写真实网络、操作缺席 1.54 或提交。

### 实现与阶段证据

| 阶段 | 本批交付 |
|---|---|
| W0 | wifi_management=2 独立版本；认证 HTTP 冻结分页/原子 apply/结果查询；RAM bond+安装身份许可，沿原 BOOT/正式窗口；v1 兼容边界及8项结果容量写入 wifi-protocol |
| W1 | core/platform/wifi.rs 与 service：非秘密本地目标、完整快照、字段基线；稳定 job/request ID、24h、5/15/60 最早重试、重复读合并、取消/期限/重启。复用 platform/state.json 原子存储，不另起引擎 |
| W2 | wifi_setup.cpp/wifi_management.inc/wifi_scheme.h：生产单 blob v2，v1只读迁移，损坏拒绝；完整12 bond项+至多3 legacy项、原子本bond变更/拒绝回执；活动凭据/版本独立于待启用保存。trial/save/activate 分开 |
| W3 | render/sim_wifi.cpp 编入同一生产 Wi-Fi 源码，文件型 Preferences/association/DHCP 故障 HAL；Fake 真实 DeviceConnection→生产编码→512B Status属性等待；HTTP分页/apply/result；生产48B wake记录及 compact history 投影共用 HAL，历史 fixture 明示合成 |
| W4 | UI/MCP共用 action/service；先落盘返回job；已有机会、每MAC锁、OTA/发布优先、合法 Wi-Fi 窗口与8s预算。Data/Plan先完成，后置可选250ms授权；丢ACK先query/readback，绝不再apply；明确拒绝结束，未知凭据仍待确认；本地预览修订原子校验 |
| W5 | 生产设备页离线编辑/复制/墓碑、完整本机/其他bond/legacy分组、双方字段/凭据操作选择与方向预览、已有快照导入/刷新后导入、任务状态/输入/取消、有界恢复保存和独立启用。异步保留草稿，MAC/generation/请求序号隔离；自动选网只用已确认设备映射 |
| W6 | 五crate239 passed/1 ignored；Fake bootstrap48 passed，最后修改影响的3个Wi-Fi用例复测通过；总计287 passed/1 ignored（不重复计focused）。3个C++宿主入口与设备页JS通过，git diff --check通过。只有软件产物，无新版ROM/App发布产物 |

### A01–A17 证据范围

缩写：C=`wifi-w6-regression.log`，F=`wifi-w6-fake.log` / `wifi-w6-fake-final-focused.log`，H=`wifi-w2-host.log` / `wifi-w2-legacy-host.log` / `wifi-w2-recovery-host.log`，U=`wifi-w5-ui.log`；全部位于 ignored `artifacts/bridge-macos/`。app最后调度调整另在 `wifi-w4-final-app.log`（51 passed/1 ignored）。

| 编号 | 已验证软件行为 / 实际入口 | 实体或组合边界 |
|---|---|---|
| A01 | C离线修订保存/无发布；U实际脚本cache view仅本地、离线草稿保存不setup、不发布 | 未部署新版UI |
| A02 | F `wifi_jobs_use_production_hal_real_ble_ack_and_query_after_lost_http_ack`：共享service首次offline→waiting、Bridge重启后下一机会→真实HTTP完整读取；BLE许可由真实DeviceConnection拿生产ACK | OS扫描/INFO超时沿既有属性矩阵，未做真实无线 |
| A03 | C重复读稳定ID、重启与双MAC隔离；U刷新保草稿、旧MAC异步回报隔离 | 关闭原生应用/系统睡眠后的长时实例另验 |
| A04 | F `wifi_fake_full_projection_freezes_pages_and_preserves_bond_legacy_and_atomic_rejects`：多bond+legacy完整投影、只读归属；H持久迁移/无密码摘要；U全分组/XSS净化 | 解绑/新IRK等真实系统身份未验 |
| A05 | F跨bond/revision旧cursor拒绝，Unicode冻结多页重组；C部分/错MAC拒绝且不替换旧表；H分页超期路径 | 多进程同时抢读所有交错未穷举 |
| A06 | C字段比较/目标冲突/无关字段；H活动删除保护；U默认选择排除删除/冲突/无基线/只读、手动只读预览拒绝 | 密码始终未比较，无自动推断缺项删除 |
| A07 | F `wifi_bounded_recovery_and_history_share_production_bytes_and_keep_display`：trial不落盘、恢复save独立、activate成功才改active，Fake帧缓冲字节不变；H DHCP未成功不提交、取消/超时/断链/存储失败恢复旧链路 | Fake帧不变不能证明Note4试连跳屏根因消失；真实HTTP reachability UI单列，实板未验 |
| A08 | H v1迁移保活动、损坏不清、失败整批不半提交、NVS写失败、重启；F容量失败回执rejected且配置未变、重启结果恢复 | Preferences文件原子rename模拟；真实NVS掉电未验 |
| A09 | F HTTP提交后504→awaiting→重启原ID result+snapshot，仅1次apply/revision1；恢复BLE属性drop_ack后同ID查持久结果/本bond回读 | 未以直接POST返回JSON替代BLE ACK；实体写后断电另验 |
| A10 | C sent重启仍先query、unknown不重apply、缺RAM输入waiting_input；unknown即使元数据一致也不能确认密码；确定rejected不伪报成功 | 设备账本保守不删除，容量满明拒；无“自动过期淘汰”实现或测试 |
| A11 | C只批准ssid时保留无关远端label，目标ssid变→conflict；发送版进字段基线、后来本地编辑保留；预览local_revision不匹配拒绝 | 全部多客户端竞态未穷举 |
| A12 | C虚拟时钟发送前cancel/expire不调用transport，发送后cancel保留may_have_committed；H已提交取消不回滚；U显示远端可能生效 | 真实进程杀死恰逢每个CPU指令未验 |
| A13 | H BOOT到期不写、重复开窗不续租、跨bond/owner/select窗口拒绝；C无许可waiting_authorization；既有F错session/MAC/wake/旧ACK矩阵 | 保存本bond配置不claim；启用另校验owner。实际RF到期/owner切换交错另验 |
| A14 | 既有F/BLE/render 511/512/513、635合成、错误关联/属性写失败/读失败/通知丢失；新增F Unicode/转义分页、64B ID history最终编码trim；H最终Wi-Fi最大418B，短授权196B | 635真实帧op归属仍未知，NimBLE实板新增回复未验 |
| A15 | app真实共用 `confirmed_scheme`/`matching_network` 测试：未确认/草稿/远端新改不启用、精确唯一映射、权限未知/无映射不猜；执行前重读live host、owner和active版本 | 原生连续换网/TCC/多接口/VPN与广播触发组合未实测；未宣称整套系统事件E2E通过 |
| A16 | app共用合法窗口门控拒deep/错MAC/旧能力；源调度审阅确认同设备锁、OTA/发布优先、后置授权不计入Data/Plan10s截止；C/F原PowerPlan/OTA/发布截止回归 | 完整app进程接Fake的OTA/配网抢锁压力、自然睡眠/恢复/RF预算另验；没有延长RF或额外定时扫描 |
| A17 | C刷新后导入重验本地revision，U导入仍传选中字段、cache与read分开；原诊断缓存/限频入口和失败阶段测试保留 | 一般pmstats/主动状态读没有全部改为新持久Wi-Fi任务；它们是第8节所列后续可复用扩展，不声称已完成 |

### 可复现命令

从仓库根执行，使用已有全局工具链和两份隔离缓存。以下只启动隔离Fake临时进程，不连接设备；目录创建/编译沿仓库提权约定执行。

```sh
CODEX_STATUS_ARDUINOJSON="$PWD/artifacts/bridge-macos/deps/ArduinoJson-7.0.4/src" cargo test --manifest-path bridge/Cargo.toml --locked -p bridge-app -p bridge-core -p bridge-render -p bridge-ble -p bridge-mcp --target-dir artifacts/bridge-macos/target
CODEX_STATUS_ARDUINOJSON="$PWD/artifacts/bridge-macos/deps/ArduinoJson-7.0.4/src" NO_PROXY=127.0.0.1,localhost cargo test --manifest-path bridge/Cargo.toml --locked -p device-sim --test bootstrap --target-dir artifacts/bridge-macos/sim-target -- --test-threads=1
node tools/test-device-page.mjs
# 各 test-wifi-*.cpp 编译入口：C++17、render/shim、src、同ArduinoJson头；management/setup链接既有libbridge_render.a，recovery独立。
git diff --check
```

### 保留现场及未部署限制

实际仍是已部署 Note4 `.44` / ble_wire=1，不能把源码 `wifi_management=2` 当设备能力。ROM沿 `note4-0.18.44-ble1.bin` / 1,697,632 B / SHA256 `56367D6BDBC056B597DE354B05B47320404A2712CBC4F631532A512D1A81D0C8`；运行.app二进制沿73,593,616 B / SHA256 `51FBC70C2FDA7774E7DDF38FE4E6D5364800907C259DABB0452B31BAE567F9EA`。旧ROM/App/数据/缓存/安装身份/许可/owner/legacy凭据及既有未提交改动保留。

设备结果账本仅8项，当前保守永久保留，满后 `result_capacity`，没有安全自动清理；这保证未知事务不被静默挤掉，但限制长期连续管理。回执unknown不能证明密码或开放网络凭据操作成功，任务留awaiting；非秘密字段全部读回匹配可单独标“目标匹配、原事务未知”。同ID不可补换秘密重放；取消/过期不能假回滚。

候选未整板编译/链接、未打包/启动新版.app；真实RAM/栈/射频/堆压力、试连屏幕跳变、连续重连/INFO超时、自然会合成功率、RF/TCC/功耗、真实USB副作用/NVS断电及Windows/其他架构/缺席1.54保持未验。Wi-Fi候选移除几份大型栈临时副本改为有界heap分配，但不是实板内存测量。B5/B6不结项；独立配额延迟根因调查及backlog A8不在本批改动范围。

## 12. W7 已按追加授权部署（2026-10-03）

用户在A8任务选择Bridge+Note4 ROM一起更新。Note4 .45标准OTA job63be0d96=succeeded/image_verified，运行ota_1前1,720,784 B SHA256=B4635DC064378427FACBB62C6A7780DE9649E819CD0DB834B0063B90F149468C，wifi_management=2。最终Mac Bridge75,188,560 B / SHA256=FA15F8B52D383BEFB039B800900D3D91342409AE5A064B0944F9082C92627817，经原LaunchAgent运行，原应用/.44 ROM与私有data备份保留。

A8同时修复remaining默认分类与新context旧确认失效；自动HTTP seq16 applied/displayed，源/最后写屏缓冲89一致。完整证据见PROGRESS顶节及usb-sync-diagnosis.md追加交付。没有开串口、写实际Wi-Fi方案、试连/换网、发布模板或提交。

W7完成构建/部署/镜像身份与普通数据路径；A01–A17真实配置、原跳屏/重连、USB常驻、NVS掉电/RF/TCC/功耗、Windows/其他架构/1.54及长期事务账本仍未验，B5/B6继续开放。上文未部署描述保留为W0–W6当时现场。


## 13. 分 tab 与原生布局验收（2026-10-03）

三页签、ARIA/键盘、独立字段、响应布局与分层操作已实现并部署。原生产 HTML 先在 WKWebView 失败于缺页签，修复后 12 场景通过、14 张截图；既有设备页 JS 回归通过。命令为 node tools/test-device-page.mjs 和 bash tools/test-wifi-ui.sh；详细证据、产物与边界见 [wifi-tabs-plan.md](wifi-tabs-plan.md)。原生渲染使用合成 IPC，不替代 Rust/实机改网 E2E；B6 保持开放。

当前应用 75,192,688 B / SHA256 B827FA658CCC1A2E3E86B44592A909550773251683A837C78E67155B7D80CF1A，原 LaunchAgent 与私有数据保留，health204/MCP正常；Note4 .45 ROM 未改。随后真实读任务等待原因已细化为 HTTP 状态前置不可达，未取得完整快照；不据此断言 USB 或缺模板是原因。


## 14. 原任务归属与验收更正（2026-10-04）

用户确认本审查涉及的交接工作本来就属于既有会合设计。核对原文后确认：
- 本文第5节要求真实机会、身份/能力/预算核验及任务确认链；W4明示“第5节完整执行、读取/同步与会合闭环”。
- A02要求首次超时后在下一机会完整读取并完成；第8节末要求A02/A09/A14经过DeviceConnection、app共用service和Fake属性/HTTP组合调用链，直接事务调用不替代。
- 既有device-sync-diagnostics/design.md §5.5允许诊断complete后退出；Wi-Fi扩展允许借用同一窗口。两份合同的关闭条件需在原coordinator整合，不能各自按单任务完成退出。
因此本轮发现是**W4跨任务交接实现遗漏及W6组合验收不足**，不是新功能需求或新专项。此前软件通过数量保留为实际运行过的局部测试证据，不能继续作为W4闭环完成的结论；W7构建/部署事实仍成立。
当前W4涉及窗口任务准入/就绪/完成/关闭尚未闭环，A02及相关A09/A14的生产app交接组合验收待补；总待办仍在backlog B6。
短按BOOT只是设备日志调查或原有维护授权的入口，不是日常任务交接缺陷的修复，也不能作为“任务能自动完成”的验收条件。修复应沿既有coordinator和合法窗口合同，保持授权、owner及硬截止，不引入读/传输隐式续租或伪periodic。
详见 [rendezvous-audit.md](rendezvous-audit.md)。仅改对账文档，未修改运行Bridge/ROM。
