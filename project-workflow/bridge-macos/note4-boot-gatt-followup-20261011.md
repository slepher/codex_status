# Note4 BOOT、GATT 图标与异常启动：后续执行文档

日期：2026-10-11（香港时间）。待办归属：backlog B6，关联 A9/C8。本文是执行细化，不另建独立待办来源。

## Status：现场与交接边界

**17:42 实施交接**：基线提交 `f822531` 后，BOOT/GATT、PPL1计划恢复、启动通道及Bridge OTA调度已统一实现。Note4已认证运行 `0.18.81-note4 / ota_1`，OTA `35c89bae` 单次上传、`succeeded/image_verified`；默认Bridge与实际codex-status-a v4模板已配套部署。随后.82电量平滑已部署并获用户实体确认；两项按用户追加授权统一收尾提交，见[电量执行记录](note4-battery-smoothing-20261011.md)。具体产物、测试、资源和剩余实机边界见本文末尾；下文16:40及更早记录为历史基线，不代表当前部署。

持久时间方案：PPL1单blob，显式LE字段/长度/CRC/实际principal；保存原授予时长及RTC绝对起止。运行中仍用单调截止；恢复仅转换剩余量并保守扣除RTC整秒不确定性。外部RTC发生校时跳变前，先用当前单调剩余量原子重锚，失败则不改外部RTC；失效/回拨无法证明连续时不恢复Light。身份/原请求字段用于重放围栏，不恢复owner租约。该格式无记录0 B，有记录固定48 B+实际principal字节，最多176 B（逻辑数据，不含NVS管理开销）；正常重复计划零写入。

本轮只读调查对象为 **0.18.78-note4 / ota_1**，源码基准提交 `bda8313`，设备 Wi-Fi MAC `7C4FADB93408`。匹配 ROM/ELF、转储及原始证据见 [调查记录](note4-stall-20261011.md)。

用户告知另一 agent 已准备好新 ROM、正在刷入。本 agent 自此不再打开串口、请求设备、构建、启停 Bridge 或烧录，以免争用现场。新版本、运行槽位、镜像哈希、刷入完成与认证核验结果由部署 agent 更新 PROGRESS；本文件不把候选或用户报告的进行中操作写成已部署成功。接手以最新交接卡为准，在新 ROM 上复核下述缺口，不覆盖并行的单份 Bundle/压缩实现。

**16:40 合并交接**：部署已完成。Note4认证为 **0.18.80-note4 / ota_0**，MAC仍为7C4FADB93408；1,920,688 B运行镜像SHA256 `4CCD39B2C751C44DD6C89527742592656B3495F9700A4121D98BFDEB6C0A7B83`，匹配文件 `artifacts/note4-0.18.80-zlib-blocks.bin/.elf`。Bridge默认实例PID63064，exe SHA256 `BD641F1E5ADC0870063B329F77F6734C22F4E22F8AB418185C081850CBE7C2FB`。USB认证为continuous/light，原codex-status-a迁移后Data441 displayed；新模板未发布，G1–G7未随C9解决。证据 `artifacts/c9-ota-status.private.json`、`c9-deploy-after.private.json`，备份和部署细节见[单份内容交接](../single-copy-updates/status.md)。

用户要求把计划跨重启与OTA流程先补录本文，再与BOOT/GATT统一实施。本轮新增实现草稿已备份至 `artifacts/plan-reboot-unfinished-20261011.zip` 并逐块撤回，未构建、未部署；工作区保留C9及原有并行改动。以下G8–G11为已调查、待统一实现，不是.80已有能力。不另开独立实现支线。

## 现象与证据结论

| 现象 | 已知证据 | 不能推出的结论 |
|---|---|---|
| 15:19 时钟停、连续 BOOT 无响应 | 网络失败；15:28 原生被动串口 18 秒无输出 | 不能区分死锁、阻塞、睡眠异常或崩溃 |
| 疑似“重启”后仅版本号界面 | 用户修正为疑似启动页；未看到 IP/BATT；历史含 undefined 唤醒启动 | 未捕获该次 ROM/reset_reason，不可定为 panic、断电或 USB 复位 |
| 首次 BOOT 即出现蓝牙 | .78 初始化明确打开手动 120 秒窗口 | 不是已有证据证明的睡前开关残留 |
| 后续按键有时无蓝牙图标 | 用户依据屏幕判断；16:08:21 认证手动窗口剩余 100475ms，BLE ready=true，automatic_ble=false | 无图标不等于实际 BLE 未开启；物理按键准确时间未对齐 |
| BLE 交互失败、Wi-Fi 空白 | 16:04 WIFI WAIT 无 IP，认证要求加密和绑定；16:07 自行恢复认证及联网 | 不能认定永久丢失绑定；不得直接解除配对来掩盖根因 |
| 显示刷新异常 | 16:08:02 局刷失败升级全刷，随后渲染完成 | 软件 trusted/写屏成功不证明肉眼显示正确 |
| 内存偏低 | 16:08:21 heap_free=16652、heap_min=1648 | 未记录分配失败或新 panic，内存只是调查线索 |

已导出的有效 dump 与 10 月 7 日旧 .68 dump 一致，不属于本次 .78 故障。授权导出时已主动复位，原 15:19 RAM 现场不再存在。后续采样能工作不等于排除了原始间歇故障。键鼠 USB 接收器断连按用户要求另案处理。

## Plan：必须落实的行为合同

1. 首次从休眠按 BOOT：获得从物理唤醒计时的 300 秒 Light 窗口；临时 BLE 向 Bridge 获取当前网络选择，使用设备已有凭据，无选择则回退上次成功网络。该临时交互不能自动等同用户 GATT 常开开关。
2. 后续 BOOT：手动 GATT 常开开关按开/关交替；OFF→ON 获得 120 秒 GATT 窗口并续 300 秒 Light 窗口，ON→OFF 只撤销常开偏好，不关闭当前 Wi-Fi、不取消 Light、不打断必要失败恢复。下一次 ON 再延期。timer 唤醒不授予人工窗口。
3. 自动 BLE 交汇需求、手动 GATT 窗口、实际 BLE 射频状态独立。屏幕蓝牙图标只表示手动窗口有效；自动交汇不点亮该图标。独立元数据交给模板决定画法，不能由固件统一硬编码图标。
4. 正常 Light 且 Wi-Fi/Bridge 可达：主动 HTTP 联系，手动 GATT 开启也不触发自动 BLE 交汇。Wi-Fi 无法连接或 HTTP 联系 Bridge 失败均进入失败恢复，但保留不同失败阶段；按已约定流程有界汇报本次失败，关闭后等待下一 BLE 周期。不能因报告失败无限延长或立即循环。
5. USB 维持长期 Light 可达，不改变 BOOT/GATT 开关语义。产品 Light 模式与芯片实际浅睡区分：USB Serial/JTAG 的禁止浅睡锁须单独记功耗，不以它证明按钮语义有例外。
6. 日常 HTTP presence 不属于 BLE 交汇。当前恢复交汇内部包含 HTTP 就绪汇报，执行时不得把两种 HTTP 路径混称；只在有界恢复路径讨论交汇中的重绘延迟。
7. **正式PowerPlan跨重启有效**：保存原计划身份、内容与原截止时间。软件/OTA重启后仍有效的Light计划恢复现有Wi-Fi，直接通过HTTP回报；Sleep计划完成一次有界BLE交汇回报，再按Bridge回复或原计划继续。重启、读状态和重放同ID不重新起算Light期限。USB常驻Light优先走HTTP，不因计划期限、重启或OTA而强制先做BLE。
8. 物理BOOT是单独事件：仍按第1/2条授予人工窗口，不能把任意软件重启当BOOT，也不能把恢复的正式Light计划当手动GATT ON。Sleep计划下的纯本地时钟薄唤醒保留既有节电路径，不把每个分钟刻度当成需要BLE的新启动报告；到达联系周期、普通重启或物理唤醒才进入相应联系流程。
9. OTA的持久队列用于冻结镜像、幂等及断线恢复。在线且认证HTTP可用时直接调度上传；USB continuous不等待下一BLE周期，也不额外要求先获得一个120秒计时Light计划。上传成功、重启上线和运行镜像验证是三个阶段；未取得目标运行镜像证明时不能报成功，不能换请求ID重刷。

### 正式计划恢复与通道选择

| 启动条件 | 首选行为 | 期限/恢复边界 |
|---|---|---|
| USB常驻Light，已有可用网络配置和Bridge端点 | 恢复既有Wi-Fi，启动HTTP服务，主动presence；Bridge认证回读 | USB不续手动GATT、不伪造新正式Plan；拔USB后再按原计划/人工窗口判断 |
| 无USB，正式Light仍有效，普通软件/OTA重启 | 恢复上次成功网络并HTTP回报，不先做BLE | 仅使用扣除重启耗时后的剩余期限；恢复过程中到期要重新判定 |
| 正式Sleep，普通重启或到期联系 | 一次有界BLE回报；仅按获准回复启动HTTP | 无回复则回原睡眠/联系节奏，不自动授300秒Light |
| 正式Light已过期、记录损坏/未知版本或时间不可验证 | 不复活旧Light；有界BLE获取/核验当前计划 | 明确诊断原因，保留可验证的请求身份用于重放判断 |
| Sleep中的物理BOOT/PWR | 既有物理唤醒窗口及临时BLE配置联系 | 不把本次临时BLE记为手动常开；与普通重启分开 |
| HTTP路径缺凭据/端点、关联/DHCP失败或Bridge不可达 | 沿现有有界BLE失败回报/下一周期恢复 | 分开记录失败阶段，不无限连接，不延长计划 |

持久记录采用单条、小型、显式版本/长度/校验的原子记录，不增加项目自管A/B。保存实际principal、plan_id、模式、原请求参数、实际授予期限和可跨重启核对的时间锚；同ID同内容返回原剩余量，同ID异内容冲突，旧ID拒绝，principal变更按授权边界隔离。只有新计划发生有效变化时写入，重复HTTP/BLE回执不写闪存。先提交并读回验证再返回持久成功；写入或读回无法确定时返回unknown，恢复后查询原事务，不重设截止时间。工厂重置清理记录。

时间设计需在统一实施时验证：优先使用可证明连续的RTC计时；Note4外部RTC可提供跨软件/OTA启动的锚，但必须处理RTC失效、回拨、校时与整秒精度，不能直接以“当前墙钟+原时长”恢复。深睡扣除已过时间；真正断电且时钟不可证时不续旧Light。计划恢复不恢复已失效owner租约；HTTP仍认证MAC/token并显式claim，Sleep BLE仍要求绑定加密。首版升级没有新记录时是迁移/初始化路径，不能以第一次启动证明跨重启持久化已通过。

资源与恢复预算在实现前冻结：只存一个实际计划，不预留计划仓库；候选布局可从固定头约40 B加实际principal字节起算，正式字段/版本以验收后的设计为准，不把未验证草稿格式当已定协议。分别记录空/单条/典型/最大记录、NVS物理开销和写频率、峰值RAM及完整调用链栈；记录丢失/截断/CRC错误、写失败、提交成功但ACK丢失、读回失败和掉电恢复均必须覆盖。

### 本次OTA流程调查（.80配套Bridge，尚未修正）

- `platform::enqueue_ota`持久化镜像和任务后立即返回queued；`main.rs::device_status_poll_loop`每10秒检查queued任务，认证状态/owner后通过HTTP上传。因此“入队”等的是调度机会，不代表一定等设备唤醒或BLE。
- `platform::cycle`目前即使USB continuous，也先发正式计划并要求ACK剩余至少120秒才准入OTA。统一实施改为识别认证的连续在线状态；已有足够Light剩余量可直接使用，USB无需额外定时窗口。保留鉴权、目标MAC、镜像校验和单设备互斥。
- 快速轮询只包含queued，遗漏awaiting_confirmation。后者依赖其他状态/会合刷新路径；一般调度还有30秒刷新、离线60秒探测，不能简单描述为“一定等900秒”。增加有界、独立的只读确认调度，和上传互斥、暂停/取消/错误终态一致，不重传镜像。
- `post_ota_window`目前只要一次HTTP成功就退出等待，可能读到尚未重启的旧ROM并提前发新计划。应去掉因OTA自动重置期限的旁路；运行镜像证明独立完成，只有正式新决策才能替换原计划。旧固件的post_ota_s兼容迁移要显式处理，不让它覆盖已恢复的Sleep计划。
- 固件`V2PlanState`仅在RAM，NVS只保留旧模式；`application::initialize`无条件`v2RendezvousBegin`。这是USB仍经BLE恢复网络的启动路径问题，USB产品Light本身不能保证重启后Wi-Fi已关联。

本次任务0972b435：16:25:39入队，16:25:48开始上传（排队9秒），16:26:06收到上传确认并首次读到旧.78镜像；16:26:27 post-OTA计划超时；16:27:17 BLE network_start获准，16:27:20 HTTP恢复；16:27:53目标.80镜像SHA256验证成功，总计134秒、仅一次上传。证据见上方任务JSON及 `bridge/target/debug/data/logs/bridge-app.log.2026-10-11` 对应UTC08:25–08:28。不能把总时长全部归因网络传输，也不能据此声称.78间歇故障已修复。

## 已确认缺口与待验证项

| 编号 | 分类 | .78 位置/证据 | 执行要求 |
|---|---|---|---|
| G1 | 已确认语义偏差 | application.cpp captureStartupBoot / GPIO0 EXT1 分支 open 手动窗口 | 分开首次唤醒临时配置联系与用户常开偏好，避免同一物理按键重复计入 |
| G2 | 已确认显示合同缺口 | radio.cpp deviceStateText/templateStateText；模板 device.state == BLE ON | 独立暴露手动 GATT 窗口元数据，不受 Wi-Fi 阶段遮蔽；自动 BLE 不冒充常开 |
| G3 | 已确认刷新覆盖缺口 | enterBleOn、serviceBleSession 在交汇中抑制刷新；serviceNetworkGlyph 未监视手动窗口 | 开/关/到期均产生可追踪的显示更新需求；忙时有界延后并补画，失败重试不丢状态。不能据此单独解释 16:08 健康 HTTP 时缺图 |
| G4 | 已确认输入风险，现场归因未定 | application.cpp 初始化 detach ISR 后主循环轮询 GPIO0 | 让短按不因 HTTP/EPD 阻塞而丢失，正确区分按下/松开、短按/长按、唤醒首按去重；ISR 不做协议/显示工作 |
| G5 | 状态所有权耦合 | bleOff 同时 close 手动窗口；现有自动关射频调用点已检查手动有效性 | 审核所有关闭路径，确保实际射频停启不擅改用户偏好；不声称已观察到自动交汇覆盖手动窗口 |
| G6 | 诊断缺口 | history 将 undefined 唤醒标 power-on；原故障复位日志缺失 | 保留真实 reset_reason、boot_id、唤醒原因及操作时间，明确 unknown；同 ROM dump 匹配后才能归因 |
| G7 | 待调查 | BLE 认证短暂失败、低堆、局刷失败与系统停滞 | 在新 ROM 上取证，分别验证恢复、分配失败处理和显示；不把线索合并为已确认单根因 |
| G8 | 已确认持久化缺口 | .80 V2PlanState仅RAM，persistMode只保存旧mode | 正式计划身份、请求参数、原期限跨重启保持；原子提交、读回和unknown处理 |
| G9 | 已确认启动路径偏差 | .80 initialize无条件v2RendezvousBegin | USB/有效Light恢复HTTP回报，Sleep一次BLE；物理BOOT与薄唤醒单独分类 |
| G10 | 已确认OTA确认竞态/调度缺口 | post_ota_window接受旧ROM响应；10秒队列轮询未包含awaiting_confirmation | 独立有界确认，核对目标镜像；去掉因重启自动续期的旁路 |
| G11 | 已确认USB准入冗余 | cycle的ota_window仅依据新Plan ACK>=120s | 认证continuous直接准入；有限Light使用真实剩余量，不为上传重复续期 |

## Task：执行顺序与交付物

### T0：接收部署基线

部署基线已在Status补齐。统一实施前重新读取认证结果：MAC、版本、target、槽位、运行镜像 SHA256、匹配 ELF、Bridge 版本、实际活动模板/合同。记录本轮工具复位和 ROM 升级时间，旧、新证据分开。先检查新代码是否已覆盖 G1–G11，再决定最小改动范围；不得重复刷旧 .78 只为复现。

### T1：复核并更新权威合同

在 docs/power-state.md 和 docs/generic-display-platform-design.md 落实上述独立状态、窗口起点及 USB 语义。确定一个独立的模板本地字段（建议 device.ble_manual_active，正式命名以权威文档为准），布尔或枚举类型须与现有绑定类型系统一致，不临时假设协议支持任意新类型。诊断分别可观察常开剩余时间、自动交汇原因、实际射频开启及连接状态。

同时统一正式计划持久化/时间连续性、普通重启与物理BOOT的优先级、Light/USB直接HTTP及Sleep回报合同；消除“所有启动必须BLE”与新合同的矛盾。先冻结存储和错误语义，再修改代码，不把本文未验收方案写成已部署事实。

### T2：输入与状态修改

围绕 G1/G4 处理 BOOT 事件捕获与去重；围绕 G5 明确窗口状态由物理操作/到期管理，射频由需求合成管理。沿用现有有限窗口与失败重试周期，不新增重复状态机。健康 HTTP 的 BOOT 操作不应进入 v2RendezvousBegin。检查按键恰逢 timer、初始化、EPD BUSY、HTTP 超时的顺序。

G8/G9在同一启动路径实现：先恢复并验证计划，再合并USB/物理事件与期限选择通道；复用既有凭据和HTTP presence，失败复用有界恢复。必须在薄唤醒/睡眠判定之前处理适用的正式计划，避免读到计划前就睡回去；有效Sleep普通重启不画成或执行新的人工Light窗口。

### T2b：Bridge升级调度与确认

与T2配套实现G10/G11：冻结单一OTA请求ID与镜像，在线立即唤起一次现有调度（保留持久队列兜底），USB/合法Light走HTTP；awaiting_confirmation只查原任务/认证镜像。重启前旧boot/旧ROM响应不得结束确认；断线、Bridge重启、取消后已提交未知等保持真实阶段。取消重复300秒续期机制，Sleep回报后由普通正式决策决定是否转Light；不增加新的OTA服务或第二套联系状态机。

### T3：模板合同与刷新修改

同时修改固件模板环境/编解码、bridge/crates/core/template.rs、宿主同源渲染及相应协议测试；如持久 ABI 变化，显式版本化并定义旧 Bundle 的拒绝或兼容行为。BLE 元数据变化与 Wi-Fi 变化分别触发更新，模板仍负责是否显示。实际部署模板必须显式发布完整 Bundle，不能把本地模板保存或固件 OTA 当作发布完成。

### T4：诊断与资源核查

保留有界事件证据：物理按键时间、分类、手动窗口截止、自动交汇原因、射频变更原因、显示请求/完成/失败、boot_id/reset_reason。禁止记录 token/密码。测 BLE 开启前后、HTTP 状态分页、模板渲染时的空闲/最低/最大可分配堆与任务栈；判断是否有泄漏或瞬时峰值，不只比较总空闲。新增持久数据必须记录空/单条/典型/上限成本与掉电恢复；尽量不增加持久状态。

### T5：软件与实机验收

先跑覆盖实际状态转移的生产代码测试及同源渲染，再按操作细则构建唯一 Note4 目标、核对版本/大小/哈希和匹配 ELF 栈。已有通过结果不得冒充新协议的验收。实机矩阵见下；实现、构建和模板发布均完成后，按该轮明确授权执行部署，用户未要求不提交。

| 场景 | 必须同时观察的结果 |
|---|---|
| 电池深睡首按 BOOT | Light 300s；配置临时 BLE 不冒充手动图标；成功转 HTTP |
| 健康 Light 连按开/关/开，间隔 >3s | 常开图标与手动窗一致；Wi-Fi 不断；第二次 ON 延期；自动交汇计数不增 |
| USB 健康 Light，同上 | 与电池相同的开关语义；超过原 300s 仍可达，不自动重开 BLE |
| 有效Light计划下软件/OTA重启 | 原ID/内容/截止保持；扣除重启耗时；HTTP回报，自动BLE不增加；同ID重放不续期 |
| Sleep计划普通重启 | 一次有界BLE回报；不自动转300秒Light、不冒充手动GATT；完成后按有效计划运行 |
| USB插着重启/OTA | 恢复Wi-Fi后直接HTTP；不等下次BLE；队列不受虚构的120秒计划门槛阻塞 |
| Light期限在重启/关联期间到期 | 不重算期限；回报真实过期状态，并执行睡眠/有界联系规则 |
| 新格式首次升级、损坏/截断/未知版本、RTC失效或回拨 | 不把未知旧计划当有效Light；恢复路径有明确原因；首刷与第二次持久恢复分别验收 |
| 同ID冲突、旧ID、principal切换、持久写/读回失败、丢ACK后重启 | 原事务幂等；失败或unknown明确；不伪造成功、不借重启更新owner/期限 |
| 上传ACK后旧ROM仍响应、HTTP短暂断线、Bridge重启 | 旧响应不确认新镜像；同一任务只读确认恢复，目标SHA256一致才成功；无重复上传 |
| Wi-Fi 未连/连接中/已连/失败，分别开启手动窗 | 每种 Wi-Fi 状态都不遮蔽手动图标；关闭或到期正确隐藏 |
| 自动恢复期间手动开/关/到期 | 自动需求继续有界执行；图标只跟手动窗；忙时补画且无丢更新 |
| Wi-Fi 连不上与 Bridge HTTP 不通 | 原因分开；失败有界汇报后等待周期，无无限 BLE/重复联网 |
| EPD BUSY/HTTP 超时期间短按 | 按键不丢、不误判成长按切模板或重置；图标最终与窗口一致 |
| 局刷失败 | 可核对补画/全刷，软件确认与实体显示分别验收 |
| 异常版本页或停止响应复现 | 先保存身份、日志和任务现场；不先复位；真实复位原因与工具行为分账 |

每例记录精确时间、固件/模板身份、boot_id、Wi-Fi与Bridge状态、手动窗口/自动原因/实际BLE、显示计数和肉眼结果。用户已反馈健康USB联网下六次实体按键符合预期；采用这六次作为本轮人工验收，不再要求十次。失败场景按有界周期验证；不得用USB常连结果替代电池深睡/功耗验证。

## Review：收口标准

G1–G5及G8–G11已统一实现并完成下列软件验收和部分实体验证；完整电池/RF/掉电矩阵仍开放。G6/G7缺证据的历史异常保留未知，不用新ROM正常样本结案。待办状态仅维护backlog B6/C8，部署事实由PROGRESS负责。

### 本轮部署与验证

- ROM：`artifacts/note4-0.18.81-boot-plan.bin`及同名ELF，1,925,792 B，SHA256 `BC2ED81473FADC1EBC04DA96CE299550AFC527311FE28ED236FD392ADC109391`。唯一目标zectrix-note4-b，构建静态RAM119,308 B、Flash1,873,131 B。OTA原任务35c89bae认证目标镜像，无重复上传。
- 默认Bridge exe SHA256 `3F53CFEB953A883C1637280E7332357CC54CC44C35E989DA7551B0160E3C759D`，通过计划任务部署；旧exe/data分别备份在 `artifacts/bridge-before-note4-81-20261011.*` 和 `bridge-before-contract-fix-20261011*`，ZIP校验通过。书桌屏未刷机。
- BOOT ISR有界捕获短按，首次唤醒临时BLE与手动窗口分开；`device.ble_manual_active`为本地数值0/1，模板ABI2追加绑定。用户六次实体开/关符合预期。实际模板由原运行版本修改四处BLE条件，未用其他夹具覆盖用户布局；宿主JSON/compiled像素对拍一致。
- 首次发布12fba505后Data449先被拒绝，约80秒后重试成功，期间用户看到内置界面。原拒绝未记录设备错误码，不能断言唯一根因。源码另确认同名模板发布后可能复用旧字段索引，已修正：上下文变更立即失效旧合同，发布/激活ACK后认证回读新合同，再允许Data；补设备错误日志。回归覆盖同名索引变化及Bridge重启。验证发布53cab015已succeeded；commit后立即认证读状态的日志存在，后续认证data/applied_seq=453、displayed。期间HTTP超时使Data ACK不完整，不宣称首次Data发送即成功。
- 正式Light计划3490获600秒、ACK retention=nvs后，用esptool run进行一次明确的USB工具复位（无Flash/NVS写入）。新boot b923581cdb078739ba3e6a263b40c4c9、reset_reason=11，认证恢复同ID、restored_light、剩417秒，扣除了经过时间，没有重授600秒；startup_contact=http，automatic_ble=false。此后Bridge新正式计划3491属独立决策。计划不会在回报后删除，也没有设备端一次性OTA回报marker。
- 测试：core/app及相关集成241 passed、3条件ignored；render/MCP/device-sim126 passed。后续boot_plan/compiled重复复核11 passed，不重复加入总数。固件模块检查、匹配ELF的66项栈门禁通过。证据：`artifacts/note4-81-contract-tests.log`、`note4-81-render-protocol-tests.log`、`note4-81-focused.log`、`note4-81-stack.log`、`note4-81-build.log`。

### 资源与验收限制

PPL1逻辑持久字节：空0、最小49、当前4字节principal为52、上限176；每条仅一个计划。NVS还需条目/页/GC开销，不能把这些逻辑长度当Flash总占用。正常同ID重放不写入；校时重锚另有必要写入。匹配ELF对象PlanStore48 B、BootCapture112 B、BleRecoveryWindow24 B；principal按实际字符串分配。存储/读回各176 B有界栈缓冲；PlanStore save栈帧416 B，applyV2Plan可静态解析局部链1376 B（不含平台和间接调用，不冒充完整动态峰值）。认证样本heap_free94,728、heap_min62,040、largest51,188 B，loop_stack_free6,324 B；不据单样本证明长期无泄漏。证据 `note4-81-object-sizes.log`、`note4-81-final-auth.private.json`。

USB工具复位后启动和认证HTTP出现数秒延迟/超时，未出现新boot/panic证据。当前模板已恢复、认证data453已显示，延迟原因仍未知。HWCDC源码存在主机不读取时日志等待的可能性，但原生被动COM5读取对照没有证明因果：公共状态约0.16–0.59秒，认证回读约2.9–6秒且一次offline；后一次读取0字节。不得把它写成已确认CDC故障，也未据此再刷ROM。两次被动监听均未改DCB/DTR/RTS、未清缓冲/发送命令，认证boot_id保持。证据 `note4-81-cdc-drain-meta.json`、`note4-81-cdc-auth-meta.json`及私有串口原文。

尚未实体完成：无USB的Sleep普通重启一次BLE、Light到期竞态、电池深睡首按、Flash掉电、长期RF/功耗/动态堆矩阵；这些不由USB样本或宿主模拟替代。历史.78停滞根因仍未知，B6/C8不结项。

### 电量抖动资料调查（只读，未改电量算法）

用户观察BLE OFF显示96，ON时95/96/97。当前Note4 `src/firmware/power.cpp` 每次8次ADC平均，VBAT按3300–4200mV线性换算，9mV约1个百分点；没有跨次历史滤波。BLE负载变化或采样噪声是可能解释，当前未对齐原始电压和射频事件，不能确认实际电量随开关瞬变。

已按要求调用Luna runner检索GitHub，并复核本地 `D:/Documents/PlatformIO/Projects/slate`：

| 实现 | 代码证据与行为 | 对Note4的价值 |
|---|---|---|
| Slate | `firmware/main/bsp/battery_adc.cc:49`每次10次校准ADC平均，间隔100µs，随后二次曲线换算；无跨次平滑。`firmware/main/ui/status_bar.cc:106`确认充电隐藏百分比、充满显示100%、未知显示--。charge_status另做GPIO去抖。 | 单次平均与曲线不能直接解决跨次跳动；充电显示策略需可靠充电状态，USB在场不足以判断。 |
| CrossPoint X4 | [固定提交源码](https://github.com/crosspoint-reader/crosspoint-reader/blob/81ae9dd7797b998644cfdf54f08d687bb54c0e64/lib/hal/HalPowerManager.cpp#L118)：一个RAM整数保存放大10倍的值，EMA约90%历史+10%新读数，无NVS；每次getBatteryPercentage调用更新，无新增定时器。X3的I²C电量计另一路径。 | 最贴近现有抖动；不能原样随每次渲染推进滤波，应固定采样节奏；真0%不能兼作未初始化标志。 |
| ESPHome墨水屏示例 | [原仓库配置](https://github.com/Bedacz/esphome-epaper-weather-display/blob/main/e_paper_weather_forecast.yaml)：电压曲线+0–100夹紧+最近4样本滑动平均，ADC每5分钟采样。 | 证明小窗口也是常用方式；示例周期不适合直接照搬当前交互。 |

建议只对UI百分比添加轻量整数EMA，保留原始电压/百分比供低电保护与诊断；状态约一个整数及初始化/采样时间（估计8–16 B，非已构建实测），无须额外库、任务或NVS写入。初始化用首个有效读数；充电/USB策略单独核实。平滑改善显示稳定性，不等于校准真实荷电量；本轮没有修改或刷入电量算法。
