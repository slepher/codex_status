# Codex Status 项目进度（交接文档）

## 2026-10-05 11:26 已完成任务仍显示排队的界面提示已修复部署

- **现场与根因**：用户11:19两截图，同read39a7a49ab08b980c历史完成/complete_snapshot，底部仍“已排队 · accepted”。只读运行state确认completed/attempt1，非设备未完成。生产HTML VM将get入队→view完成稳定复现相同残留（wifi-job-feedback-red.log）；任务列表刷新但操作提示只保存初始响应。原测试缺少初始响应→被动刷新终态的断言。
- **修复/验证**：操作提示跟踪同job_id最新作业，完成/失败/进度均随view更新；统一静态反馈清除旧关联，后续保存/导入/错误/设备切换不被旧任务覆盖。accepted/complete_snapshot等阶段中文化，保留任务ID供核对；迟到入队响应通过紧接的view校正。Node回归通过（wifi-job-feedback-green.log），原生WK52场景通过（wifi-job-feedback-wk.log），560/1000 job-feedback及job-feedback-error四张截图已审阅。WK实际点击生产读取按钮+合成IPC，覆盖队列→进度→成功/失败和新保存反馈保留，不冒充实际Rust/设备RF通信。
- **默认Bridge部署**：app artifacts/bridge-macos/Codex Status Bridge.app，SHA256 `664B86AF8CC2013690C78E79B2EACE1E9E725AFAB957636BC873ECF5A27F06F5`，supervisor2756/health204/MCP8766监听，候选/部署hash及签名匹配。安装身份与data保留，截图任务39a7a49ab08b980c原记录完整且completed/complete_snapshot；私有备份 `/Users/slepher/Library/Application Support/dev.codexstatus.bridge/deployment-backups/wifi-job-feedback-20261005-112614`，回滚app `/Users/slepher/project/codex_status/artifacts/bridge-macos/Codex Status Bridge-before-job-feedback-20261005-112614.app`，证据wifi-job-feedback-deployment.json与build.log。仅重建/重启默认实例，未操作命名实例、另建实机作业或修改网络。
- **固件与边界**：本轮无固件改动/构建/刷写。沿用此前核验Note4 MAC7C4FADB93408、0.18.61-note4/ota_0，ROM artifacts/bridge-macos/0.18.61-note4-wifi-unified.bin，1874080 B，SHA256 `578EEDE7CEBEF30AC3DB016B40B78F3F204C30E78AEF3F9305AEA3DED703E3F4`；匹配ELF SHA256 `7F3A79599D553D95C15B864A8213C0B490C1DE7B275467BB664697136A24E0BD`，本轮未重新查询设备运行镜像。界面修复不结B6，其余RF/电池/BOOT/命名实例/掉电容量矩阵仍见backlog。本轮界面修复、回归及现场记录已按用户要求提交。

## 2026-10-05 01:49 新事务旧缓存冲突反馈已修复部署，用户重新读取后同步成功

- **用户纠正与实体结果**：a7efde70624b7097是新事务，expected revision1引用过期设备缓存，设备已revision3；此前称“旧事务”不准确。用户取消该待确认任务，read 9a99ea2ee94c5faa获得revision3，新sync 83cecdbef6a0e7c6/request `w3_0000000000000013`以3提交4，completed/durable_receipt且receipt_ack_pending=false。实体WFS4 ready/uncertain=false/369B/2网络/1legacy回执/1cursor/high_watermark19，同boot USB continuous/BLE OFF；证据wifi-budget-sync-success.json。旧请求retired仅证明围栏已退役，不证明它曾成功。
- **此次修复**：App在实际发送前发现认证设备revision与新任务冻结expected_revision不同，除完整意图已被认证证明满足外，明确返回conflict/revision_conflict，sent=false，不apply，不自动改写缓存或覆盖Bridge编辑。conflict停止自动调度，UI显示“设备方案已变化，缓存已过期，请重新读取设备后再同步。Bridge 配置已保留。”；仅实际发出且无终态证据才保留结果未知。此前USB continuous错误引用sync0预算及发送前误标sent修复继续保留。
- **Fake缺口与验证**：Fake此前缺少认证status的wifi_storage.revision，App未获得与实机相同的缓存冲突预检输入；现通过共享生产C++ wifiSchemesStorageStatus导出，修订变化更新status_seq。新增真实App机会→HTTP预检不apply、真实Fake生产设备拒绝revision_conflict→core停止调度、UI反馈用例均red→green。受影响最终262 passed/1既有ignored（app68/core121/HTTP21/Fake52），最新Fake错误返回形状另有focused复测；Node通过、原生WK48场景通过，560/1000 stale-cache截图已审阅。WK使用合成IPC，不冒充实际通信/RF证据。
- **规则与部署**：AGENTS.md关键不变量写入用户要求“任何请求都必须有明确返回，包括失败的请求”，覆盖结构化错误/阶段/有限超时/缓存冲突/真实结果未知及失败链路测试。默认Bridge SHA256 `B3AC01D2730D4F16EA6AEDBD8C4895202011A728B525378BD808A2DCF2F6C7D9`，supervisor38549/health204，候选与部署hash/签名一致；安装身份、data及用户成功任务完整保留。私有备份 `/Users/slepher/Library/Application Support/dev.codexstatus.bridge/deployment-backups/wifi-stale-20261005-014859`，回滚app `/Users/slepher/project/codex_status/artifacts/bridge-macos/Codex Status Bridge-before-wifi-stale-20261005-014859.app`；证据wifi-stale-deployment.json、wifi-stale-final.log、wifi-stale-fake-final.log、wifi-stale-ui-green.log、wifi-stale-wk2.log。
- **设备与剩余边界**：未更改固件或另建实机任务。Note4 MAC7C4FADB93408/IP192.168.10.101仍0.18.61-note4/ota_0，ROM1874080 B，artifacts/bridge-macos/0.18.61-note4-wifi-unified.bin SHA256 `578EEDE7CEBEF30AC3DB016B40B78F3F204C30E78AEF3F9305AEA3DED703E3F4`；匹配ELF SHA256 `7F3A79599D553D95C15B864A8213C0B490C1DE7B275467BB664697136A24E0BD`。完整RF/电池/BOOT/命名实例/掉电容量矩阵继续归backlog B6，B6不结项。用户已要求工作区分批提交；RTC批次 `1f37727`、NimBLE批次 `33e39e8` 已提交，剩余Wi-Fi源码/回归/设计及交接记录随本批提交。请求返回规则已单独提交 `1ff5c7a`。

## 2026-10-05 01:10 USB常驻预算与HTTP核对信封修复已部署，旧未知事务保留

- **用户现场**：00:53:24新任务a7efde70624b7097/request `w3_0000000000000010`，起初http_status_unavailable，后awaiting_confirmation/window_budget_exhausted/sent=true。旧f287ac24cd546159现场已cancelled，本轮未操作取消。创建期间HTTP连续超时，随后设备reset_reason8自然唤醒；现认证绑定principal56a6/namespace f0:18:98:6b:98:c6/0有效，USB continuous/HTTP正常/BLE OFF读写允许。原冻结slot0不匹配/slot1匹配，不能宣称同步成功。
- **定位与修复**：Bridge协议3continuous误落入旧sync.remaining_ms=0预算分支；在shared service先持久sent后，HTTP回调因0预算未发出，留下虚假“可能已生效”，后续查询又套旧截止。continuous及sent query现在只遵守调度的有限工作预算，新window写入仍绝对截止；引入明确NotDispatched只恢复本次零发送，传输中超时/旧未知sent仍保守查询。HTTP只读match/active/setup_status/scheme_get补关联request_id，修复shape；写事务ID永不生成/改变。阶段在Bridge发送前预算和标记/读回信封，不是已证明的设备保存/回执协议失败。
- **验证与实际部署**：三个生产回归seam取得red→green，受影响完整260 passed/1既有ignored（app67/core121/HTTP21/Fake51）；日志wifi-budget-green2.log与三份red证据。Bridge唯一默认实例SHA256 `D6DC32B2CF29B4E3FA076D60219239D0A3D107754E4FEE3F79FD32DA313FA2AD`，supervisor35567/health204，签名/部署hash通过；身份、data、当前任务ID/frozen保留，私有备份wifi-budget-20261005-0108及旧app回滚包保留。未改UI/固件，不重刷ROM。部署前实机USB continuous/BLE OFF/light0已记录；重启默认Bridge后同boot/BLE OFF，原事务认证HTTP查询200/135ms，现error=receipt_unknown、stage=query_result、attempt17，预算故障已消除。
- **当前Note4**：MAC7C4FADB93408/IP192.168.10.101，0.18.61-note4/ota_0，1874080 B；ROM artifacts/bridge-macos/0.18.61-note4-wifi-unified.bin SHA256 `578EEDE7CEBEF30AC3DB016B40B78F3F204C30E78AEF3F9305AEA3DED703E3F4`，匹配ELF SHA256 `7F3A79599D553D95C15B864A8213C0B490C1DE7B275467BB664697136A24E0BD`，既有独立运行hash已验。当前boot e7f23298fbeb9a538ef9d0c2014b7923/reset8；WFS3 ready/321B/2网络/1legacy回执/revision3/cursors0，未写真实Wi-Fi配置。
- **用户决策与边界**：已询问取消当前待确认任务→认证读最新修订→新同步已保存两条（只保存不切网），或继续保留原事务只查询；用户目前只问unknown/故障阶段，未选择新事务，因此不取消/重发/清旧sent。软件修复完成，当前同步未确认，其他RF/电池/BOOT/命名实例/掉电容量矩阵仍待B6。无提交，dirty修改保留，详见project-workflow/bridge-macos/wifi-unified-protocol-execution.md。

## 2026-10-05 00:34 统一协议已配套部署，原同步任务等待系统权限完成绑定修复

- **实现与验证**：U0 WFS4确认退役+永久sequence围栏先行，legacy任意ID安全保留；HTTP/BLE共用身份/claim/窗口/事务处理器，BRS2稳定principal/canonical peer/namespace原子关联与无LRU淘汰。USB continuous不受light0/BLE关闭阻挡；异步RF先持久pending、跨通道/重启只查询原ID。HTTP优先持久作业、在线/通道/倒计时、认证状态序号及定向通知hint→认证探测已实施。最终Rust/Fake275 passed/1既有ignored、生产C++ ASan/UBSan、JS、WK46场景/窄宽截图及真实ELF栈门禁通过；证据和资源预算见project-workflow/bridge-macos/wifi-unified-protocol-execution.md。
- **实际运行ROM**：唯一zectrix-note4-b，Note4 MAC7C4FADB93408/IP192.168.10.101，0.18.61-note4/ota_0，1874080 B；artifacts/bridge-macos/0.18.61-note4-wifi-unified.bin SHA256 `578EEDE7CEBEF30AC3DB016B40B78F3F204C30E78AEF3F9305AEA3DED703E3F4`；匹配ELF SHA256 `7F3A79599D553D95C15B864A8213C0B490C1DE7B275467BB664697136A24E0BD`。OTA a9811cd3 succeeded/image_verified，独立设备操作token认证运行镜像MAC/fw/target/大小/hash全匹配。静态RAM118476B，HTTP局部Wi-Fi链2896B/通知帧688B/loop496B；未clean/切env/重装。
- **当前Bridge**：默认管理app SHA256 `B65A64FE736746E22108511AA2ACA889E0B2622FC6E2E6416DD3A686D3F24D60`，默认LaunchAgent supervisor33585/health204，签名及部署hash核验通过，其他实例未操作。私有data/installation-id备份wifi-unified-20261004-235651，旧app同时间戳回滚包保留，安装身份未变。原任务f287ac24cd546159/request ID及冻结内容保留，waiting_rendezvous/sent=false，未取消/另建。
- **实机阻挡与处理**：USB continuous/HTTP ready，BLE随实际窗口开关；认证wifi_control仍binding_required、principal/namespace为空。Bridge合法authorize返回bluetooth_permission_required，已请求用户在最终app授权蓝牙；一次raw USB pair打开120s物理窗未复位（无DTR/RTS ioctl、清HUPCL、同boot），原文私有保存。未绕过系统权限或设备绑定。原任务expected revision1而设备revision3；只允许认证逐目标完整匹配并再次核对boot/revision后确认已满足，否则保留CAS冲突，不盲覆盖。当前WFS3 ready/321B/2网络/1legacy回执/revision3/cursors0，首次新写入的WFS4迁移尚未实机验证。
- **边界/剩余**：原任务USB+BLE OFF/light0+Bridge重启HTTP闭环仍待绑定修复；完整电池/BOOT/命名实例/真实试连换网/掉电容量矩阵未结案。运行hash及软件回归成功不等于B6关闭；无真实测试改网、无提交，现有dirty修改保留。剩余唯一事实来源backlog B6。

## 2026-10-04 22:37 统一协议Astra medium一次审核完成，设计已按用户权限选择修订

- **审核方式与输入**：用户明确要求Astra medium审核一遍，确定性问题直接修改、需要决策先弹问题。只读子代理gpt-6-astra/medium完成一次审核，原稿SHA256 `71F46C18A80DD44540DB696E9580A6F22D5820E346D4D1D1B330D6972FCDA2A8`；方向可行，4类确定性修订、1项权限决策。记录project-workflow/bridge-macos/wifi-unified-protocol-review.md。
- **直接修订**：BLE同peer多实例需peer+principal endpoint token双证明，generation仅防旧消息；旧namespace不自动共享。激活先预留回执/持久pending，active与完成回执同原子提交；RAM trial重启缺终态返回结果未知。普通候选以principal/request归属，传输断开不取消，旧网恢复也受原deadline；分别保持8关联、3 namespace×4/总12配置、8回执容量。验收矩阵已补对应掉电/跨传输/满容量反例。
- **用户决策已应用**：先弹出BOOT救援权限问题，用户选择统一严格权限：允许合法绑定/身份修复，保存/试连/启用先取得本机claim，其他owner占用拒绝。设计第3.1节与逐操作权限表已冻结；不保留现行物理保存/试连豁免，不自动转移owner，不扩大namespace权限。统一协议稿、关联设计/合同与backlog已更新。
- **验收/现场**：文档差异检查通过；审核与设计修订完成，不等于实现/软件/实机验收。本轮未改运行代码、构建/部署、设备读写/串口、取消/重发任务或提交；运行Bridge/Note4沿用前节，USB写许可现行缺口尚待按新设计实施。待办唯一来源backlog B6。

## 2026-10-04 22:17 统一在线状态与HTTP/BLE控制协议已设计（未实现/未部署）

- **用户合同**：USB插入、BOOT及Bridge主动激活后，Bridge应知道设备已在线、可用通道与剩余时间；HTTP可达可直接交互，不限BLE、不等待BLE交汇。OTA已具有Wi-Fi HTTP上传及运行hash核验路径，保持现行设备操作token与预算。
- **设计决定**：复用认证/api/status及GATT同一DeviceStatus，presence continuous/window/ending+原因+剩余时间，区分HTTP ready/BLE ready/正式PowerPlan/物理维护期限。USB常驻即使light剩余0也可管理；BOOT120s独立不续期；Bridge显式激活复用正式协调器，整机深睡无接收机会仍诚实排队。统一发现事件、定向endpoint通知/默认UDP/BLE hint→认证确认→继续已提交任务，补查覆盖丢通知/Bridge重启/命名实例；BLE关不等于失联。
- **身份/事务**：持久principal↔endpoint凭据↔canonical BLE identity↔namespace关联，两通道共用即时授权/owner/事务处理器；普通命令取消BLE permit依赖，跨通道查询同request回执、不盲重写。旧namespace/配置保留，迁移有歧义明拒，不能client bond冒充权限。HTTP试连/启用accepted不冒充完成；敏感OTA仍独立凭据和流式传输。
- **文档/验收边界**：project-workflow/bridge-macos/wifi-unified-protocol-design.md含U1–U6、兼容/资源门槛和软件+实体矩阵；wifi-redesign/protocol及docs导航已链接，待办仅backlog B6。文档自审/git diff --check通过，不把设计当实现验证。本轮未执行设备命令、取消/重发真实任务、开串口、构建/部署或提交；运行Bridge仍21:58版本hash175F5595…421A39F/默认PID19267，设备沿用.60/ROM E529A7FA…AFFB4，前节未完成同步仍是现行缺口。

## 2026-10-04 22:03 USB 常驻 Wi-Fi 写许可缺口已定位（尚未修复）

- **真实证据**：Note4 MAC7C4FADB93408/.60，公开plugged=true/deep_usb=0/BLE=false、同boot连续运行；22:02:54任务f287ac24cd546159仍waiting_rendezvous/window_required/sent=false，attempt14。期间见HTTP新正式Plan875剩余119s，说明不只是永远没有正式light窗口；最新Plan876已到0。认证状态上次会合handoff=true、remaining_ms0、WIFI_LIGHT，同boot/wake_seq35，随后主要认证HTTP联系。只读证据wifi-task-time-usb-status.json，无写入/拔线/串口/复位。
- **代码链**：wifi.rs permits仅note_link的BLE wifi_authorize填充；HTTP opportunity只查内存permit，有正式窗口也不能补取，core缺permit直接waiting_rendezvous。固件v2RvBleReleased在交接后关闭并阻止自动重开BLE；USB/deep_usb0跳过正常深睡和下一timer-wake周期BLE会合。Bridge重启丢短时许可后，USB常驻HTTP可达却无法取得新写许可。没有旁路授权，也不是用户点击时机错误；“等待下次真实会合”不能保证当前USB路径自动前进。
- **范围/剩余**：本轮已完成任务创建时间UI部署与状态核对；USB常驻获取有效绑定写许可的流程尚未修复，实体同步仍未完成。后续须补USB常驻路径及Bridge重启/许可过期回归，保留目标绑定、owner与既有窗口期限；不要要求用户拔USB或修改deep_usb作为正常工作流程。唯一待办见backlog B6。

## 2026-10-04 21:59 Wi-Fi 任务状态已核对，创建时间已显示并部署

- **真实任务状态**：截图f287ac24cd546159于2026-10-04 21:48:32创建；最新只读检查state=waiting_rendezvous、attempt_count=11、error=window_required、sent=false、may_have_committed=false。尚未发出本次写入；HTTP机会因内存中无有效permit返回window_required，等待真实绑定会合取得许可。Note4公开状态可读、.60、当时light剩余0/BLE OFF；没有据“可联系”宣称写入许可可用，也未保证下一会合一定完成。没有取消、手动重发或改网。
- **界面变更与验证**：复用已有created_at，当前任务与执行记录均显示本地完整日期/时分秒；缺失明确“时间未记录”。设备页JS通过、原生WK42场景通过，窄宽waiting/error任务区截图审阅（IPC合成）。仅UI时间展示，未改Rust任务协议/状态机；git diff --check通过。
- **部署**：默认app路径沿用，exe76147808B/hash`175F55952420F1AD6544B19A696A43DF2B4BF800E44D75F4B4CD734FD421A39F`，supervisor19267/health204；签名/内嵌Brotli HTML/部署hash核验通过。targets2/local_revision13/snapshot/baseline、11项任务及各created_at/installation-id保留。旧app `/Users/slepher/project/codex_status/artifacts/bridge-macos/Codex Status Bridge-before-wifi-task-time-20261004-215820.app`、私有data/身份备份 `/Users/slepher/Library/Application Support/dev.codexstatus.bridge/deployment-backups/wifi-task-time-20261004-215820`，0700/0600。证据artifacts/bridge-macos/wifi-task-time-{deployment.json,status.json,js.log,ui.log,build.log}与wifi-ui的560/1000-waiting/error.png。未提交、构建固件、开串口或操作命名实例；许可取得与实际同步完成仍待后续现场确认。

## 2026-10-04 21:48 Wi-Fi 同步来源独立修复已部署，完整往返回归通过

- **设计偏差与确定复现**：之前把源绑定writable当成Bridge→设备发送资格；导入并正式保存后checkbox仍禁用。同步确认形成本机同ID版本后，比较/再次导入还把外部来源混入并判ambiguous。原测试缺少设备→Bridge→编辑保存→设备完整点击与服务链路；新增原生WK与真实Rust服务用例均取得red，不能把此前分段测试通过称为完整同步验收。
- **现行合同**：导入生成独立local配置，编辑保存不继承源绑定限制。发送基于已保存Bridge内容和目标设备授权/修订；目标设备当前本机绑定同ID版本优先用于比较与再次导入，旧来源不阻挡选择，也不覆盖当前目标版本。源记录保留，设备认证与事务隔离照常生效。成功文案简化为“Bridge 配置已保存。”；编辑内一次手动保存及checkbox对齐保持。
- **验证**：core120 passed，app66 passed/1既有ignored；生产HTML JS通过，原生WK560/1000共42场景通过，包含本机与其他绑定的读取→导入→编辑→保存→勾选→同步→再次比较/导入；窄宽截图已审阅。生产C++管理测试ASan/UBSan通过（含跨绑定同ID写入隔离/原子/回执），宿主render辅助库未全量插桩。WK为合成IPC、Rust设备端为stub；没有写真实网络，不冒充实机RF往返已验。git diff --check通过。
- **已部署默认Bridge**：app `/Users/slepher/project/codex_status/artifacts/bridge-macos/Codex Status Bridge.app`，exe76147808B，SHA256 `A07B507C86D50E3D424AD117E23EB9DE59AB83DE11AA0C077BBE8AF38ADFAD05`；默认supervisor18640、health204。候选签名、内嵌Brotli生产HTML和部署hash核验通过；重启后两条targets/local_revision13/snapshot/baseline与最新备份一致，installation-id保留，无需重新导入。未操作命名实例、固件或串口。
- **回退/证据**：旧app `artifacts/bridge-macos/Codex Status Bridge-before-wifi-roundtrip-20261004-214751.app`（hash32A0EC3C…55883B2）；私有data及安装身份备份 `/Users/slepher/Library/Application Support/dev.codexstatus.bridge/deployment-backups/wifi-roundtrip-20261004-214751`（0700/0600）。`artifacts/bridge-macos/wifi-roundtrip-{deployment.json,core-red.log,red.log,js.log,core.log,app.log,ui.log,management.log,build.log}`与wifi-ui的560/1000-roundtrip-own/other截图。未提交，既有RTC/NimBLE修改保留；实体矩阵仍归backlog B6。

## 2026-10-04 21:29 Wi-Fi 导入编辑与一步保存已修复并部署，checkbox 已对齐

- **用户症状与根因**：导入的两条Bridge配置沿用其他绑定origin，wifiEdit允许打开而wifiSubmit在前端拒绝；两次保存均未到IPC，提示在底部任务区。原测试仅覆盖local条目，漏掉真实导入形状。生产HTML VM与原生WK取得red；用户澄清导入即为可编辑本地配置，不应要求复制。
- **现行交互**：import生成local配置；旧导入项显式save时规范为local，保留network_id/版本/密码/内容/基线。UI直接编辑/删除导入项，编辑表单内一次手动“保存到 Bridge”；去掉“加入/更新草稿”及外部保存按钮，输入不自动持久化。结果/错误在表单旁显示，失败保留修改。设备快照bond/writable和设备授权不变。checkbox排除文本input的34px最小高度，许可/比较label用flex对齐。
- **验证与覆盖边界**：core119 passed，app66 passed/1既有ignored，JS通过；WK560/1000共38场景、一步点击/无自动保存/导入原地更新/失败留修改/checkbox几何与截图审阅通过。新增真实Rust import→save→restart与缺permit不发送apply回归。WK使用合成IPC，未改写真实Wi-Fi或宣称RF/设备同步矩阵完成；B6仍开放。git diff --check通过。
- **已部署默认Bridge**：app `/Users/slepher/project/codex_status/artifacts/bridge-macos/Codex Status Bridge.app`，exe 76144784B，SHA256 `32A0EC3C65D9C9147E7FA42294FE5EBAB47EF3B3950E65E4DEC6D0A2C55883B2`；LaunchAgent默认supervisor17004，health204。签名、内嵌Brotli生产HTML、候选/部署hash核验通过。更新后targets2/local_revision11/snapshot/baseline与备份完全一致、installation-id保留；旧origin将在用户正式保存时修正，无需重新导入。未操作命名实例。
- **回退与证据**：旧app `/Users/slepher/project/codex_status/artifacts/bridge-macos/Codex Status Bridge-before-wifi-save-20261004-212436.app` / hash `23DFF946FA555849D40E494B0D74B5293A6ACE9F151BD403FECEE7FCB3DD4D87`；私有data及installation-id备份 `/Users/slepher/Library/Application Support/dev.codexstatus.bridge/deployment-backups/wifi-save-20261004-212436`（0600/0700）。`artifacts/bridge-macos/wifi-save-{deployment.json,js.log,ui.log,core.log,app.log,build.log}`及wifi-ui的窄宽imported-edit/save-error/delivery-alignment截图；实施记录wifi-redesign.md。
- **设备现场保持**：未构建/刷写固件、读取完整设备配置或开启串口；Note4仍沿用下节已认证`0.18.60-note4`及ROM hash `E529A7FA69B9D091D638DED5783BAA919EAF622CB96B3DE92CD02ADA692AFFB4`。未提交，原有RTC/NimBLE等工作区修改保留。


## 2026-10-04 Note4 BLE 关停 panic 已修复，0.18.60-note4 已 OTA 部署

- **根因与修复**：.59真实dump的nimble_host PC0直接指向host timer事件；真实NimBLE 2.5.1生产函数的确定性交错稳定复现提前callout_deinit清空queued回调。STOPPING只stop，最终ble_hs_deinit销毁timer；PlatformIO post脚本`tools/nimble_host_timer_fix.py`在依赖解析后编译前严格校验/幂等应用，未知形状拒绝。不吞空回调、不常开BLE、不改PowerPlan/授权。旧版red、修后ASan/UBSan2000轮queued/dequeued/重复stop/no rearm/最终释放与重新init通过。
- **当前实机与ROM**：登记MAC `7C4FADB93408` / IP `192.168.10.101`，`0.18.60-note4` / `zectrix-note4-400x300`，ota_1，1739088B；ROM `/Users/slepher/project/codex_status/artifacts/bridge-macos/nimble-stop-fix/0.18.60-note4.bin`，SHA256 `E529A7FA69B9D091D638DED5783BAA919EAF622CB96B3DE92CD02ADA692AFFB4`；匹配ELF同目录`.elf`，SHA256 `15D589BB886D92CA0C9D7173F9898328FFB6BB5785533427D80D7FB683BCF65D`。OTA217e2ce2 succeeded/image_verified，独立设备操作token认证运行镜像MAC/fw/target/大小/hash全匹配，最新release同hash已登记。
- **实机验证**：新ROM正常timer会合已见handoff=true、BLE关闭。现有认证诊断十次真实init→scan→deinit均成功返回、同boot_id连续264115→285621ms/BLE OFF、无panic。另经既有pair命令打开120s恢复窗，采样见真实BLE连接/断开，115.4s内boot未变；到期串口明确controller released后deep sleep 51s，HTTP/USB暂不可达。随后自然timer唤醒认证reset_reason=8/wake_seq4，末次窗口wake_seq5/uptime5357ms仍reset_reason8，确认是正常深睡，WFS3 249B/2条/ready/uncertain=false和RTC ready/valid保持。恢复窗未观察到handoff，不冒充此次交接成功；USB软件检测/电池长期矩阵仍开放。
- **构建与资源**：唯一Note4环境111.09s，通过marker、实际ELF stop无早销毁/final cleanup保留、匹配ELF Wi-Fi栈门禁和git diff --check；未clean/切env/重装framework，sdkconfig哈希未变，静态RAM仍117924B。无新增存储/常驻分配，原timer暂留至host停止完成；宿主分配=销毁。局部Wi-Fi管理链1920B、reset/deinit帧各32B，不代替整机峰值/功耗/长期RF。
- **证据与现场保护**：详见`project-workflow/bridge-macos/note4-panic-20261004.md`与`artifacts/bridge-macos/nimble-stop-fix/`。被动串口跳过DTR/RTS更新并清HUPCL，没有主动USB复位；原文私有保存，存在输出截断/交错，十次HTTP完成与逐轮boot检查不依赖全文条数。原.59 dump/匹配ELF保留且设备dump未擦；私有data/installation-id部署备份`~/Library/Application Support/dev.codexstatus.bridge/deployment-backups/nimble-stop-fix-20261004-205838`。未提交、重建/重启Bridge、改分区/网络/Profile；1.54未构建部署。剩余保首份dump策略与长期矩阵只维护backlog A0-dump。


## 2026-10-04 20:38 Note4 .59 再次 panic：真实 dump 已解码，串口复位已对照定位

- **真实设备崩溃**：用户确认USB一直连接且没有按键/拔插，20:27前后公开reset=panic、持久诊断131 reset_reason=4，随后用户报告又一次崩溃，串口曾自行消失/重枚举。均与本轮后续工具复位分开。MAC7C4FADB93408/IP192.168.10.101，仍 `.59`/ota_0；ROM `artifacts/external-rtc/0.18.59-note4.bin`，SHA256 `14CC1FBA207EABECC9885B01A31FA3028B232AC75E1B689DDFE5099A6E1FB8DF`，1739088B；匹配ELF SHA256 `32DCA064D2CF0D6E4C59A391B90664844475E76798BE7C15FCCA580BD3DA4A62`。串口实读应用前缀精确匹配，不是只按版本推断。
- **真实dump验收**：只读实机分区表及1MiB coredump，有效23972B，CRC32 `8B1C6C0A`通过，原始有效dump SHA256 `6B21209FDCA3CB10FEFFD4F08ABB2A933E23159504734EB7976B625A25BD6082`。官方esp-coredump+匹配ELF解码：nimble_host InstFetchProhibited/PC0，事件地址0x3fca9558精确匹配ble_hs_timer.ev，fn空；loopTask同时在会合关BLE→NimBLEDevice::deinit→host stop/HCI中。指向已入队timer事件被反初始化清空后的生命周期竞态；精确清空交错/修复仍待定向复现，未归因于附近Wi-Fi snapshot。
- **串口调查已实测**：20:36–20:38 raw open/read/close和跳过modem更新的pyserial各15s均不复位；普通false/false预设open立即USB_UART_CHIP_RESET。分步DTR=false可复现，随后RTS=false不再复位（限当前主机/初态）。esptool只读flash仍主动usb-reset/hard-reset；结束认证reset_reason11。AGENTS.md已记录区分/保现场监听recipe/私有dump流程，不再用“串口可能复位”跳过授权诊断。
- **范围与证据**：`project-workflow/bridge-macos/note4-panic-20261004.md`；原始分区/dump/ELF/解码/串口原文与对照在私有0700目录 `/Users/slepher/Library/Application Support/dev.codexstatus.bridge/backups/note4-panic-20261004-2027`（0600文件）。未擦dump、改固件/分区/NVS、改网络/模板、重启Bridge或提交；BLE故障未修复。20:42认证状态仍.59、reset_reason11（DTR对照）、同boot持续234238ms/Wi-Fi在线，串口已关闭；仅结束点值，非稳定性验收。待办归backlog A0-dump新增记录。


## 2026-10-04 Note4 版本后缀简化，0.18.59-note4 已部署

- **用户命名要求已落实**：Note4版本只采用`版本号-note4`，去掉a/b及功能后缀；src/main.cpp统一Note4 FW_VERSION，模拟设备默认目录同用note4。AGENTS.md已记录后续约定；构建env/硬件target标识继续沿用，旧镜像、实测记录及历史版本夹具保留原名。
- **当前实机**：登记MAC `7C4FADB93408` / IP `192.168.10.101`，`0.18.59-note4` / `zectrix-note4-400x300`，ota_0，1739088B。ROM `/Users/slepher/project/codex_status/artifacts/external-rtc/0.18.59-note4.bin`，SHA256 `14CC1FBA207EABECC9885B01A31FA3028B232AC75E1B689DDFE5099A6E1FB8DF`；匹配ELF `/Users/slepher/project/codex_status/artifacts/external-rtc/0.18.59-note4.elf`，SHA256 `32DCA064D2CF0D6E4C59A391B90664844475E76798BE7C15FCCA580BD3DA4A62`。OTA `e082ac3e` succeeded/image_verified；下一个自然联网窗口的设备操作token认证独立镜像MAC/fw/target/大小/hash全部匹配，已登记最新release。
- **验证与现场**：仅版本文本修订，RTC实现沿用下节；唯一Note4环境增量47.57s，无clean/并行pio/目标切换。marker、匹配ELF Wi-Fi门禁、RTC/main帧与.58一致、git diff --check通过。自然深睡唤醒后reset_reason=8、pcf8563 ready/valid=true，最新可信来源ble；WFS3仍249B/2条/0回执/ready/uncertain=false，Wi-Fi正常。未以短时现场宣称24h离线准度或功耗达标。
- **证据与边界**：`artifacts/external-rtc/rom-manifest.json`、`version-{preflight,ota-queue,ota-status,deployed-status,running-image,release-published,frames}.json`、`pio-note4-version.log`、`wifi-stack-version.log`；.58 manifest保留rom-manifest-rtc3.json。本轮未提交、未重启Bridge、未更改模板/实际网络/分区或开启串口；1.54未构建部署，长期准度/功耗仍归backlog时钟行。

## 2026-10-04 Note4 独立 RTC 已 OTA 部署 .58，实机读写及周期采样确认

- **授权与实机修订**：用户“更新固件”明确授权Note4更新。初次.56运行hash已确认，但Wire不接受16B缓冲；补Arduino真实最小32B约束复现red并修.57。第二次实机short_read，厂商BoardI2cForcePowerOn/i2c_device每次访问开启GPIO42，本项目显示初始化将42关闭；最终.58以Note4-only作用域在完整RTC读/写期间开启42、所有返回路径恢复原电平，不常开音频电源。两故障现场/回归证据保留，未把中间镜像当成功RTC交付。
- **最终运行固件**：MAC `7C4FADB93408` / 登记IP `192.168.10.101`，`0.18.58-note4-b-rtc3` / `zectrix-note4-400x300`，ota_1，1739104B。ROM `/Users/slepher/project/codex_status/artifacts/external-rtc/note4-0.18.58-rtc3.bin`，SHA256 `F6971A63C7507FE43AB92EA9C72ABBFC78C64EB43DF31FCB18350104207545A1`；匹配ELF同名，SHA256 `7543B30F7553D5B72B0ECFFA42A932DB9A20AC3CBCB1D4A03687EE1650C9AFBD`。OTA `b2e990a9` succeeded/image_verified，设备操作token认证独立运行镜像MAC/版本/target/大小/hash全部匹配；已登记target最新release。
- **RTC实机证据**：认证及公开状态均报pcf8563/ready/valid=true，SNTP可信校时写回成功；同一boot_id的后续RTC采样1791115997→1791116040，读数推进43s且仍ready，访问结束恢复42原电平。末次uptime104860ms、sampled_wall与主机差2s，source=ntp表明在线最近可信来源；尚未证明离线深睡恢复、完全断电保持、24h漂移或板级功耗。WFS3仍ready/uncertain=false/249B/2条/0回执/旧scheme已退役；Wi-Fi正常，短时heap_free109228/min32580仅现场点值。
- **修订验证与资源**：两型号驱动ASan/UBSan、生产main helper与NTP交错/30s延迟回归通过；增加GPIO42事务前提与成功/失败/原先高低电平恢复测试。只构建Note4，最终40.67s，无clean/目标切换/并行pio；匹配ELF Wi-Fi栈门禁、git diff --check/node语法通过。静态RAM仍117924B（较.55+48），Wire64B/已识别堆请求1500–1516B，driver read96B/write128B、loop448B。可解码局部链与平台/allocator/ISR边界见rtc3-stack.json，非整机高水位或功耗验收。
- **范围与后续**：本轮未提交；未重建/重启Bridge、改实际网络凭据/模板/分区/NVS布局或开串口。1.54没有构建/部署。剩余实机准度/功耗与1.54事项仅维护backlog时钟准度行。
- **证据**：`artifacts/external-rtc/{rom-manifest,ota-queue,ota-status,running-image,deployed-status,deployed-public-status,rtc-periodic-status,release-published,resource-budget,rtc3-stack}.json`；`driver-tests-rtc3.log`、`clock-tests-rtc3.log`、`pio-note4-rtc3.log`、`wifi-stack-rtc3.log`，.56/.57现场保留`*-rtc1.json`/`*-rtc2.json`。实施/复核见project-workflow/external-rtc。

## 2026-10-04 独立 RTC 接入软件完成：Note4 .56候选已构建，未部署

- **用户授权与范围**：按“开始为我修改”接入板载时钟；Note4 PCF8563、1.54 PCF85063A分target实现，两者0x51/SDA47/SCL48。启动/深睡唤醒优先RTC，light60s核对、≤1s保留亚秒，Bridge/SNTP可信校时写回；内部RC仅兜底。日期时间统一UTC，时区和单调超时/PowerPlan语义独立；未更换ESP32定时唤醒源。
- **失效与并发**：OS/VL/STOP/test/非24h/非法日历与传输失败明确拒绝；STOP完整写回+读回+启动确认，未确认校时阻止旧值覆盖可信新时间。SNTP callback短临界区保存UTC/单调锚点，loop/深睡前补经过时间并写RTC，避免RTC轮询交错或长处理延迟丢校时；新增clock_rtc缓存状态，未改BLE结构/授权/网络/发布边界。
- **软件证据**：两芯片真实生产驱动ASan+UBSan（逐日UTC/时区/2038/闰日/失效/短传输/9阶段故障与重试）通过；生产main helper+真实驱动确定性交错及30s延迟通过，pending-only对照模型如预期在fresh-NTP断言失败（不是旧部署ROM实板red）。仅Note4最终39.87s构建，git diff --check/node语法及匹配ELF Wi-Fi启动门禁通过。资源：静态RAM117876→117924B（+48B）；RTC/I²C主要库堆请求1468–1484B（allocator/JSON/RF等另计），读取80B/写128B帧，局部最大416B。不是整机高水位、RF或功耗验收。
- **最终候选**：`0.18.56-note4-b-rtc1` / `zectrix-note4-400x300`，1738848B；ROM `/Users/slepher/project/codex_status/artifacts/external-rtc/note4-0.18.56-rtc1.bin`，SHA256 `E06CE478153103AE4DC4E939D7E01F2602904CD2E90B8C7D382028A467BA5217`；匹配ELF `/Users/slepher/project/codex_status/artifacts/external-rtc/note4-0.18.56-rtc1.elf` / SHA256 `CD474224AFEA96A0BE5805C561066B3207808C4E3ECD5220091AAE32EE94B562`。marker已核对，未OTA/登记最新release。1.54仅驱动测试，源码0.18.35-bw-rtc1未构建ROM。
- **运行现场沿用下节**：Note4登记MAC7C4FADB93408仍已验证.55 / hash3A552561…642FF；本轮未操作运行设备、Bridge、运行data/安装身份/密钥/分区/串口或提交。硬件真实性与后续验收仅归backlog时钟准度行；源码通过不等于独立RTC已在实机走时。
- **设计与证据**：project-workflow/external-rtc/{plan,task,status,review}.md；artifacts/external-rtc/{driver-tests,clock-tests,clock-pending-only-red,pio-note4-final,wifi-stack}.log及rom-manifest/rtc-stack/library-abi-sizes/resource-budget.json。

## 2026-10-04 Wi-Fi WFS3 已提交并 OTA 部署，真实迁移与运行镜像已确认

- **提交与授权**：用户明确要求“提交，然后ota并部署”；源码提交 `f8426cb`（Implement variable Wi-Fi storage and authenticated recovery writes）。此前OTA审批阻断已由该明确授权解除；不是仍待部署。源码检查/生产C++内存检查、Rust受影响25例与匹配ELF栈证据沿用下节，部署未重编ROM。
- **Note4实机运行**：登记MAC `7C4FADB93408` / IP `192.168.10.101`，`0.18.55-note4-b-wifi-store3` / `zectrix-note4-400x300`，ota_0，1735744B；ROM `/Users/slepher/project/codex_status/artifacts/bridge-macos/note4-0.18.55-wifi-store3.bin`，SHA256 `3A552561A88B5572FD901FD6ACF2D65E75BD47BE9AF337262CD43065247642FF`，匹配ELF同名。OTA `bfd4e115` succeeded/image_verified；设备token认证独立读取镜像MAC/fw/target/大小/hash全部匹配。已登记该target最新release，同一hash。
- **真实存储迁移**：认证wifi_storage为WFS3、ready=true、uncertain=false、bytes=249、networks=2、receipts=0、old_key_present=false；业务RAM请求460B（另加数组cookie8B），record56B/receipt72B。迁移已写回验证并退役旧scheme；两次不同boot_id的深睡唤醒（reset_reason=8）仍加载同一249B/2条新仓库且Wi-Fi连接。末次uptime9537ms、heap_free40872/heap_min35200B仅为短时现场，非长期RF或整体峰值验收。
- **Bridge与备份**：默认Bridge健康204、exe SHA256 `23DFF946FA555849D40E494B0D74B5293A6ACE9F151BD403FECEE7FCB3DD4D87`不变；本轮没有生产Bridge改动，无需重建重启。与部署前备份比较targets/snapshot/baseline/local_revision完全相同，未再次读取完整网络配置或改实际SSID/密码。私有data及installation-id备份 `/Users/slepher/Library/Application Support/dev.codexstatus.bridge/deployment-backups/wifi-store3-authorized-20261004-190915`（0600/0700），旧.54 ROM保留。未开串口/清NVS/改分区/TCC/钥匙串。
- **待办仍归B6**：真实WFS3迁移/启动读取和运行镜像已验；AP/USB真实写入、错误密码/离线BLE/AP/DHCP/连续RF睡醒/Flash切电/功耗/跨平台/其他硬件及兼容读取代码最终移除仍留项，不结B6。
- **证据与设计**：[设计与资源验收](project-workflow/bridge-macos/wifi-variable-storage.md)；`artifacts/bridge-macos/wifi-store3-{authorized-preflight,ota-queue,ota-status,deployed-status,running-image,release-published,current-runtime}.json`。交接与backlog已同步，部署记录另提交。

## 2026-10-04 Wi-Fi 变长存储与 AP/USB 入口收归软件交付完成；.55 候选未部署

- **实现与计划**：[设计、资源预算及验收](project-workflow/bridge-macos/wifi-variable-storage.md)。WFS3显式magic/version/有效长度/网络与回执实际数量/CRC、小端整数及字段/记录长度；实际record和字符串按需分配，只有一个长期仓库，无空槽/原生struct持久化/压缩新写。标签与版本随record；未编辑活动版本引用保存项，编辑/删除后仅保留旧SSID/密码。setup保存工作集move到堆候选，不增加全量仓库栈副本。
- **事务与恢复**：单blob原子提交+完整读回；包括NVS已提交index却在退役旧chunk时返回错误的场景，任何put尝试未确认先重新加载，原事务result确认，不用旧RAM修订盲写。12网络/3bond×4与8持久回执上限保留，满容量明拒。WFZ2/raw-v2/raw-v1验证转换后写store3，读回成功或后续独立加载新格式成功才退役scheme；remove失败保留待重试。旧sN/pN读取/物理迁移恢复与兼容代码仍保留，未静默回落旧格式。
- **AP/USB软件收归**：AP `/save`/`/api/wifi/apply`复用既有HTTP认证与事务，`/api/wifi/result`查原结果；旧裸表单与GET清空退役。USB旧`wifi`只返回退役提示，`wifi-json`要求endpoint token并经过共享MAC/owner及permit/合法窗口/冻结修订/持久回执校验。保存不自动启用/试连/重启，不伪造bond。源码审计确认无sN/pN凭据写入者；原last使用提示保留。真实AP/USB写入未执行，不称实机验收。
- **资源证据**：空/单条1回执/典型2条2回执/满12条8回执有效字节33/225/387/5036；活动旧凭据独立时最多5134。Xtensa仓库120B、网络56B、回执72B、数组cookie8B；对应常驻存储组件RAM120/433/670/5986B、克隆提交峰值306/1316/2114/22044B，旧活动快照上限22436B。含实际字符串/候选/编码+读回，allocator/JSON/传输/NVS/RF驱动另计，非整机实测峰值。干净NVS entry成本与碎片/GC/原子旧值预算见计划。固件静态RAM125412→117876B（-7536B）。
- **验证**：生产setup/management/storage ASan+UBSan通过；迁移/所有截断/逐字节损坏/有效CRC非法长度数量版本/原子旧新边界/读回与退役失败/分配故障/commit后错误查询/容量/绑定版本与活动快照覆盖。最终受影响Rust core12/app8/device-sim5通过，旧恢复夹具通过。匹配ELF启动192B/读取240B/提交64B/poll144B/命令624B；可见Wi-Fi/NVS局部链最高2448B≤4096B；.50原5312/5344B仍red，新候选green。仅zectrix-note4-b、无并行pio/clean/分区修改；最终增量37.56s构建，git diff --check通过。
- **最终候选ROM（未部署）**：`0.18.55-note4-b-wifi-store3` / `zectrix-note4-400x300`，1735744B，`/Users/slepher/project/codex_status/artifacts/bridge-macos/note4-0.18.55-wifi-store3.bin`，SHA256 `3A552561A88B5572FD901FD6ACF2D65E75BD47BE9AF337262CD43065247642FF`；匹配ELF同名。marker/注册MAC/public与认证身份已核对；固件source/source ABI/ROM对应，未提交。
- **真实运行保持.54**：独立认证MAC7C4FADB93408 / IP192.168.10.101、fw `0.18.54-note4-b-wifi-rv2` / target `zectrix-note4-400x300`，ota_1，1737776B/hash `1923E9B0C9138F5DAF4CF1BFC4A508904990EEA6C2364CE1EA9D63015F122604`。运行ROM仍旧WFZ2/固定RAM，不能把候选说成已运行WFS3。Bridge app路径沿用下节，exe SHA256 `23DFF946FA555849D40E494B0D74B5293A6ACE9F151BD403FECEE7FCB3DD4D87`未变；缓存targets2/networks2，未重新读取完整网络配置或改写实际网络。现有OTA21192869仍是.54 succeeded，无新作业。
- **备份/部署边界**：私有data和installation-id备份 `/Users/slepher/Library/Application Support/dev.codexstatus.bridge/deployment-backups/wifi-store3-20261004-183948`，0600/0700、身份一致；旧.54 ROM保留。auto-review明确拒绝OTA排队：本次实现/验证请求没有明确授权替换固件并重启设备；未执行、未绕过。用户随后明确授权提交、OTA与部署；下一步先提交源码，再OTA并验证真实旧格式迁移、认证wifi_storage与运行镜像。
- **保留边界**：系统location/bluetooth权限仍待用户界面授权；真实SSID/错误密码/离线BLE/AP/DHCP/连续RF睡醒/Flash切电/功耗/跨平台与其他硬件、兼容代码最终移除仍B5/B6。新AP/USB行为实机验证与WFS3真实迁移未验，B6不结项。未开串口/清NVS/改钥匙串或TCC。
- **证据**：`artifacts/bridge-macos/wifi-store3-{rom-manifest,deployment-preflight,current-runtime,resource-budget,entrypoint-audit}.json`，`wifi-store3-layout.s`、`wifi-store3-stack.log`、`wifi-store3-stack-regression-red.log`、`wifi-store3-cpp-final.log`、`wifi-storage-tests.log`、`wifi-store3-recovery.log`、`wifi-store3-rust-final.log`、`wifi-store3-pio-release.log`。

## 2026-10-04 Wi-Fi 会合修订已实施并配套部署：普通窗口 BLE 同步、按需诊断、页面当前网络保护

- **现行合同与实现**：[实施及验收记录](project-workflow/bridge-macos/wifi-rendezvous-implementation.md)、wifi-protocol/wifi-redesign 顶节已覆盖历史 BOOT-only、重复读配置及分步重开预算。日常已绑定/认证且 owner 允许的配置写入复用已有正式 light/sync 窗口，无额外 BOOT；离线 BLE 支持最多4条整行原子变更，与 HTTP 共用冻结修订/NVS事务/持久回执。BOOT 仍用于首次绑定、物理救援和旧槽迁移。不隐式续租。
- **缓存与诊断**：完整配置只由显式读取任务获取；同步不重复读整份配置，持久回执投影冻结变更到缓存与基线。丢 ACK 只查询原事务，未知不盲重发。首次交汇认证 HTTP 失败才 BLE 查询最小诊断：Bridge缺配置先提示补全；设备缺项/不同先保存启用，已保存未活动只启用；同活动版本认证失败提示修正并停止重复写入。空SSID不吞认证失败；已入网但HTTP不可达单独显示，401/MAC错误独立。正常HTTP恢复清旧错误，显式启用意图不被旧网络HTTP成功吞掉。
- **交接及 Astra 审核收口**：共享 Plan 发出时锚定的30s绝对交接期限；关联+DHCP至多25s且受既有期限约束。HTTP ready 须取得非零IP且候选持久启用完成，旧网络HTTP不能完成新激活；未知Plan仅允许查询已发事务，不允许新写入。保留持久Plan ACK、高水位与同wake/session/plan/owner交接证明、已准入读任务先于诊断complete；HTTP交接确认后关BLE再light，close先放两锁且限2s。失败有界退出，作业保留恢复机会。
- **页面范围**：手动Bridge→设备同步可以删除旧方案，但缓存合并结果必须保留当前host_ssid→设备2.4GHz映射；预览冻结主机网络/local_revision/snapshot_id/选择，服务原子复核，缺项提示“请保留或加入当前网络后再同步”。不自动补选、切网或试连；自动配网、程序化MCP同步、本地保存、设备→Bridge缓存导入不套用页面守卫。整条覆盖、历史折叠、密码眼睛及删除/重命名修复保留。
- **软件验收**：core118 passed；app全量65 passed/1既有ignored，加最终页面/MCP范围回归1 passed；device-sim受影响5 passed，完整读取→写入丢ACK→重启→result查询，读取次数仅1。生产C++管理/setup/rendezvous通过，最终setup帧418B；生产HTML JS、原生WKWebView32个560/1000场景通过并审阅截图。WK合成IPC不代表实机换网。Note4唯一环境构建通过，启动栈64/80B均≤1024B，git diff --check通过。
- **当前Bridge**：默认supervisor63095，health204/MCP正常；app `/Users/slepher/project/codex_status/artifacts/bridge-macos/Codex Status Bridge.app`，exe76,063,040B，SHA256 `23DFF946FA555849D40E494B0D74B5293A6ACE9F151BD403FECEE7FCB3DD4D87`。备份app `Codex Status Bridge-before-wifi-rv2.app`，私有数据/安装身份备份 `/Users/slepher/Library/Application Support/dev.codexstatus.bridge/deployment-backups/wifi-rv2-20261004-172942`，installation-id保留。最终scope修正已重建重启。只操作默认实例。
- **Note4运行镜像已独立认证**：MAC7C4FADB93408，IP192.168.10.101，`0.18.54-note4-b-wifi-rv2` / `zectrix-note4-400x300`，ota_1，1,737,776B。ROM `/Users/slepher/project/codex_status/artifacts/bridge-macos/note4-0.18.54-wifi-rv2.bin`，SHA256 `1923E9B0C9138F5DAF4CF1BFC4A508904990EEA6C2364CE1EA9D63015F122604`，匹配ELF同名。OTA21192869 succeeded，认证运行镜像MAC/fw/target/大小/hash均匹配；仅构建zectrix-note4-b，未开USB串口/改分区/清NVS。
- **尚待实机与资源验收**：被动缓存仍完整2网络/Bridge targets2；没有为验收重新读取或重写真实网络配置。系统location/bluetooth仍not_determined、host permission_required，真实SSID/错误密码/离线BLE/AP/DHCP/连续睡醒RF矩阵未验；权限仍待用户界面授权，不修改钥匙串。变长Wi-Fi仓库/按需RAM、AP/USB旧写入口收归、旧兼容代码退役及跨平台/功耗/掉电矩阵仍归backlog B5/B6，不因模拟或部署结项。本轮源码与文档随本次提交归档。
- **证据**：`artifacts/bridge-macos/wifi-rv2-deployment.json`、`wifi-rv2-running-image.json`、`wifi-rv2-rom-manifest.json`、`wifi-rv2-passive-view.json`；测试日志 `wifi-protocol-tests-final.log`、`wifi-app-tests-final.log`、`wifi-page-scope-test.log`、`wifi-rv2-sim-tests.log`；界面截图 `wifi-ui/560-sync-guard.png` / `1000-sync-guard.png` / `1000-current-unreachable.png`。

以下为前序里程碑记录，其“当前/未实施/未部署”描述只适用于记录当时；最新运行与剩余工作以本节和backlog为准。

## 2026-10-04 Wi-Fi 删除/重命名界面回归已修复并部署

- **确定复现与根因**：生产HTML VM实际“编辑→标记删除→保存”提交deleted=false；“编辑→改标识→保存”提交2条而非1条。前者由本地保存自动提交开放表单且wifiSubmit强制deleted=false引入，后者按可编辑network_id查找/新增，未记录编辑原条目；同时版本被重置0。正确反馈环锁定表单提交，非设备删除协议错误。
- **修复**：显式记录当前编辑原ID，已有network_id只读；修改显示名/主机或设备SSID更新原条目，保留密码、版本、归属和删除标记。新建重复ID明拒，不覆盖旧项；复制仍显式新建。标记删除/撤销删除明确状态与保存/同步边界，保存按钮移出折叠编辑区；取消编辑/切设备/新建重置身份，不清理或猜测用户已有重复配置。
- **验收**：两个生产HTML回归先red后green；JS包含删除→保存、重命名保持1条、撤销/取消删除、重复ID、密码/版本保留及MAC隔离。原生WKWebView28场景（含新增rename/delete实际按钮点击与IPC提交断言）通过，560/1000两动作截图已审阅；合成IPC不是实机改网。真实core服务新增整条重命名→删除→提交确认→重启回归1项通过，git diff --check通过。没有修改固件或为测试删除实际网络。
- **当前Bridge**：默认supervisor53115，health204/MCP可用，被动缓存targets2/snapshot完整2网络；运行exe75,878,000B，SHA256 `666BA31BEB354981719718A15E9FF25D0194A5766DE4A0FAA563EE0FAB94531D`，app `/Users/slepher/project/codex_status/artifacts/bridge-macos/Codex Status Bridge.app`。旧app `Codex Status Bridge-before-editfix.app`，私有data及installation-id备份 `/Users/slepher/Library/Application Support/dev.codexstatus.bridge/deployment-backups/wifi-editfix-20261004-161723`；安装identity不变，未提交。
- **设备沿用已验证.53**：Note4 MAC7C4FADB93408，`0.18.53-note4-b-wifi-cache1` / `zectrix-note4-400x300`，1,735,296B，ROM `/Users/slepher/project/codex_status/artifacts/bridge-macos/note4-0.18.53-wifi-cache1.bin`，SHA256 `E1702FC835BC0BBF6EE3D21380336A585326F1B7DC8CCE0051791C2CDA1751CE`（匹配ELF同名），运行镜像证明沿用下节；本轮未重新部署/读取设备配置。系统权限仍not_determined，真实RF/新密码缓存首次读取/变长资源与AP/USB收归仍B5/B6边界。证据artifacts/bridge-macos/wifi-edit-*，接续计划wifi-redesign.md。


## 2026-10-04 密码可保存/编辑/显示隐藏，Bridge 与 Note4 .53 配套部署完成

- **用户最新需求已实现**：Wi-Fi密码作为可选网络配置保存在Bridge本机运行data，编辑时回填，眼睛按钮点击显示/再次隐藏；换编辑项/换设备/取消后恢复隐藏。本地保存直接提交表单，删除“不保存在Bridge/不从设备回读”文案。Network Debug隐藏密码、Unix配置文件0600；密码不进仓库或日志，不新增钥匙串/授权步骤。
- **首次完整读取与缓存**：认证完整快照与本机绑定wifi_scheme_get包含该绑定密码；其他绑定/legacy保持未知，现有MAC/bond/owner/维护权限不变。设备→Bridge导入、复制、本地保存/重启保留密码，导入不联系设备；配置作业可直接使用已缓存密码，无额外补读密码协议。密码单独不同标冲突，wifi_match可选credential_key核对已缓存密码。设备写事务原有修订核验/回执确认仍保持。旧快照None不伪造密码，未自动触发读取。
- **验收**：core116 passed，app64 passed/1既有ignored；补充缓存导入/重启/无需再次输入同步/密码单独差异/Debug隐藏/文件0600回归通过；共享device-sim受影响5项串行通过。生产Wi-Fi C++两入口通过，setup最大最终帧418B；设备页JS与原生WKWebView24场景通过，560/1000编辑截图已审阅。Note4唯一目标98.03s构建通过，真实启动栈64/80B通过≤1024B门禁。WK合成IPC不代表实机换网。
- **Bridge先部署**：兼容旧快照后再部署固件。默认supervisor51307，health204/MCP可用；exe75,878,000B，SHA256 `39FC70E1D6422A20556F8BFF620F4E479BDA3E9674C66A7B4E91E00988A20E96`，app `/Users/slepher/project/codex_status/artifacts/bridge-macos/Codex Status Bridge.app`。旧app `Codex Status Bridge-before-password.app`、私有data/installation-id备份 `/Users/slepher/Library/Application Support/dev.codexstatus.bridge/deployment-backups/wifi-password-20261004-160220` 保留，安装identity不变。
- **Note4运行确认**：MAC7C4FADB93408 / IP192.168.10.101，`0.18.53-note4-b-wifi-cache1` / `zectrix-note4-400x300`，ota_0，1,735,296B。ROM `/Users/slepher/project/codex_status/artifacts/bridge-macos/note4-0.18.53-wifi-cache1.bin`，SHA256 `E1702FC835BC0BBF6EE3D21380336A585326F1B7DC8CCE0051791C2CDA1751CE`，匹配ELF同名。OTA35cbded8一次succeeded/image_verified，独立认证运行镜像MAC/target/fw/大小/hash全匹配。未开串口/改分区/清NVS。
- **边界**：被动Bridge缓存仍完整2网络/password_cached=0，保留旧无密码数据，没有为验证再读设备配置；首次新版完整读取的实际密码缓存待用户显式读取检验，不伪报已取得。location/bluetooth仍not_determined、host permission_required，真实SSID/错误密码/离线BLE闭环及长期RF矩阵未验。变长Wi-Fi仓库、AP/USB旧写入口收归仍B6待办，本轮未实施。计划wifi-redesign.md、协议wifi-protocol.md、证据artifacts/bridge-macos/wifi-password-*；git diff --check通过，未提交。

## 2026-10-04 Wi-Fi 三项接续已实现并配套部署：整条覆盖 / 历史折叠 / 当前主机网络

- **补齐此前遗漏**：比较页每个网络仅一项选择，两个方向整条覆盖；设备→Bridge仍只处理已读取缓存，不新读设备。预览冻结MAC/local_revision/snapshot_id/方向，旧部分字段作业继续兼容但UI不再提供拆字段选择。终态执行记录默认折叠，当前任务与需处理状态简要显示；不再展开已完成作业的到期字段。
- **当前网络路径**：实际主机SSID唯一匹配已保存Bridge targets，不依赖设备完整快照/baseline。无配置→预填当前SSID，已有方案→在合法会合核对并启用；“保存并用于当前网络”把本次密码仅存RAM，BLE单条匹配决定是否需要既有稳定recovery_save事务，配置相同不重复写入。缺密码/BOOT/权限/占用明确提示；授权与MAC/bond/owner/版本/回执约束沿用，不放宽。新BLE能力wc=1及wifi_match使用无密码元数据指纹、网络ID/版本/连接/有限原因；超时不冒充密码错。
- **交接修复**：普通启用在BLE保持连接时启动Wi-Fi，关联+非零IP后才确认；既有认证HTTP同session/wake/plan证明和handoff成功后显示交接确认、关闭BLE、允许light。冷却60s、最多3次自动激活；认证/安全模式拒绝停止自动重试，等待配置修正；已确认凭据意图不吞掉较新的本地编辑。未引入延长物理授权或无限工作窗口。
- **软件/构建核验**：core114项加新增恢复确认1项、app64 passed/1既有ignored、render共享协议回归通过；最新wc最大INFO/ACK512B门禁4项通过；生产Wi-Fi C++两入口通过（BLE仍加密时启用、失败恢复、DHCP、错版本/bond、原子/回执/秘密隐藏）。device-sim受影响5项串行通过；设备页JS与原生WKWebView24场景/26截图通过，560/1000的当前网络失败/比较图已审阅。WK合成IPC不是实际改网证据。Note4唯一环境57.82s构建成功，真实启动栈64/80B，门禁≤1024B通过；未清构建缓存/改分区/清NVS/开串口。
- **生产核验**：OTA9a9c40d5一次succeeded/image_verified；独立认证运行镜像 fw/MAC/target/大小/SHA256全匹配。Note4当前 `0.18.52-note4-b-wifi-ui1` / `zectrix-note4-400x300`，1,734,672B，ROM `/Users/slepher/project/codex_status/artifacts/bridge-macos/note4-0.18.52-wifi-ui1.bin`，SHA256 `EF077917F6E2244AAEFB92E019BCABDF8FD8D8A1837B64317C23A8B19CBE1075`，匹配ELF同路径改.elf。Bridge默认supervisor43341，health204/MCP可用；exe75,855,872B，SHA256 `B84AC56BB7BC80C8B3F42CB608DCECD997C1D770D03DF9A87A33FE8B837EDEF0`，`/Users/slepher/project/codex_status/artifacts/bridge-macos/Codex Status Bridge.app`。旧包`Codex Status Bridge-before-wifi-ui.app`、私有data备份`/Users/slepher/Library/Application Support/dev.codexstatus.bridge/deployment-backups/wifi-ui-20261004-152358`保留，installation-id不变，缓存完整；未改钥匙串，未提交。
- **未完成边界**：生产location/bluetooth仍not_determined，主机permission_required，未取得真实SSID/BLE换网/错误密码/断网闭环；没有伪报用户已授权、BOOT或实机新协议已验。Wi-Fi变长仓库/按需RAM及AP/USB旧写入口收归仍B6资源待办，本轮仍固定结构+压缩NVS；原连续RF/功耗/掉电/Windows/其他架构/1.54矩阵不结项。证据`artifacts/bridge-macos/wifi-ui-redesign-*`，接续计划见`project-workflow/bridge-macos/wifi-redesign.md`；待办唯一维护backlog B5/B6。

## 2026-10-04 macOS 权限引导已部署：打开界面一次性引导，未改钥匙串

- **用户最终选择**：只完成权限界面；打开app可一次性请求或切到对应界面。本轮选择缺权限时第一次打开界面切设备页权限区域，后续轮询/再次开窗不重复跳转；授权按钮才请求macOS，拒绝/受限/全局定位关闭提供对应系统设置入口，真实授权后自动收起。
- **请求时机修复**：公开API被动检查；定位请求等待实际前台激活并处理delegate变化；Bluetooth未授权时后台不构造central，HTTP继续服务。有限12×750ms跟进只读取，不重复request、不伪报grant；不采集坐标。不改TCC数据库/系统设置。签名证书导入+用户级codeSign信任先被auto-review以未明确授权持久安全变更拒绝，未执行；用户随后明确不改钥匙串，相关签名工具/预检已撤回，仍ad-hoc签名，不能承诺跨部署永久免授权。
- **验证**：设备页JS通过；原生WKWebView16场景/18截图（560/1000）通过，权限窄宽图已审阅；合成IPC不作为实际系统弹窗证据。App64 passed/0 failed/1 ignored（既有诊断需模拟器），新权限门禁测试包含其中；生产候选构建31.99s/签名校验通过。git diff --check通过。
- **当前Bridge**：默认app已保留旧包/私有data后替换，supervisor39432，health204/MCP正常，安装identity不变。exe75,817,232B，SHA256 `92B1BB244243D1C90D77D7BEE0E7ED131CD3A754004DEA0311F9689E8DC8CD76`，路径`/Users/slepher/project/codex_status/artifacts/bridge-macos/Codex Status Bridge.app`；旧包`Codex Status Bridge-before-permissions.app`。生产回读location/bluetooth均not_determined、主机permission_required、设备完整缓存保留；待用户点击授权后的实际系统回读，未声称已授权。
- **设备未变**：Note4 7C4FADB93408，.51 / zectrix-note4-b，1,730,672B，ROM `/Users/slepher/project/codex_status/artifacts/bridge-macos/note4-0.18.51-stack1.bin`，SHA256 `A7AB14F74786EB1FCA5C824144784F8053CD0B3A8B34F031E87453831C2B3D7A`，沿用上次USB/认证镜像核验。Wi-Fi整条覆盖/历史折叠及当前主机网络驱动仍仅设计，未混入本补丁。
- 计划/证据见`project-workflow/bridge-macos/permissions-plan.md`与`artifacts/bridge-macos/permissions-*`；待办唯一维护B5/B6，未提交。

## 2026-10-04 USB恢复完成：.51运行核验，旧槽迁移/完整读取成功；资源设计指导已写入

- **纠正此前离线定性**：.50用户报告不停重启，USB连续两次复现loopTask stack canary；真实dump已读至私有备份并成功解析，匹配.50 ELF调用链setup→wifiSchemesBegin→readWifiStore→NVS读取。尚未进入解压。两函数真实栈帧5312/5344B，超过8KiB栈，源于空仓库/旧格式分支的聚合赋值临时对象。本次回归由我引入；此前把离线解释为正常休眠不成立。
- **修复与现场**：原地默认构造后栈帧48/80B；新门禁tools/check-wifi-stack.py对.50 red/.51 green。生产Wi-Fi HAL、device-sim受影响5例通过。Note4 .51构建41.79s，USB仅替换活动ota_0应用，不改分区/不清NVS/不擦otadata；写入自验、完整读回逐字节相同、认证运行镜像SHA256/大小/目标匹配。两次USB复位启动观察均无panic，联网192.168.10.101，实际hand-off/BLE关闭→light通过；不代表长期RF/功耗矩阵完成。
- **当前ROM**：0.18.51-note4-b-stack1 / zectrix-note4-b，1,730,672B；`/Users/slepher/project/codex_status/artifacts/bridge-macos/note4-0.18.51-stack1.bin`，SHA256 `A7AB14F74786EB1FCA5C824144784F8053CD0B3A8B34F031E87453831C2B3D7A`；匹配ELF同目录。Bridge已登记.51当前发布版本，当前缓存导入Bridge继续运行，无需重建。
- **真实旧槽迁移已成功**：用户BOOT后.50回复ok/persisted/retired=true、migrated=2、revision=1、active=wifi-slot-1、storage_bytes=172。随后.51重启成功加载持久配置，读作业5a55a29e9a41542a一次completed/complete_snapshot；两正常槽wifi-slot-0=wd21-la、wifi-slot-1=CMCC-k3v7，同本绑定可写，无legacy，active仍槽1。单独BLE重复迁移检查因设备已不广播未执行；不冒充实机幂等验收。旧sN/pN在启动日志已NOT_FOUND；旧失败作业不修改。
- **用户资源约束已进入AGENTS.md**：仓库变长头（版本/长度/数量/校验）、实际条目/字段按需存储、RAM同样按需分配、禁止完整仓库临时副本压栈、资源预算与掉电/损坏验收。**变长仓库重构尚未实施**：当前仍固定运行结构+压缩NVS，.51只修复栈副本；空槽/回执/RAM冗余不因此结项。后续资源改造、AP/USB旧写入入口收归及移除兼容代码统一归B6。缓存Device→Bridge方案选择已部署，继续仅用已读取快照，不另读设备。
- 证据`artifacts/bridge-macos/rendezvous-deploy/reboot50-*`、`reboot51-*`；NVS/dump/含内存报告在私有Application Support/deployment-backups/reboot50-rescue，未落仓库。未提交。

## 2026-10-04 13:39 .50已部署/镜像确认，等待新BOOT实机迁移；缓存导入已部署

- .49用户第二次BOOT后迁移返回storage_encode（storage_bytes=0），尚未进入NVS写入，旧键保留；证据migration-compact-physical-result.json。这不证明之前.47错误原因；NVS空间仍由受限空间模型及旧页备份支持。改为ESP ROM底层tdefl/tinfl显式工作区，压缩字典优先PSRAM、避免loop栈/便捷包装器分配问题；释放前擦除。失败阶段/有限状态无密码返回，最长请求+唤醒ID错误帧512B门禁通过；生产host存储专测仍green。实际ROM路径仍待有效新物理窗口。
- Note4 .50候选 0.18.50-note4-b-mig4，1,730,960B，ROM `/Users/slepher/project/codex_status/artifacts/bridge-macos/note4-0.18.50-mig4.bin`，SHA256 `228FD072FAC84C362BC5FEEFF36A6ADBABB809F65F77B6E68866F6F193217770`；匹配ELF保留。最终源代码构建30.65s成功，OTA7c878df3 succeeded/image_verified，认证运行镜像SHA256/大小/目标与候选一致；当前运行.50。
- 设备→Bridge缓存方案选择已部署并验证（下节缓存导入部署）；不再次读设备。legacy迁移未完成，AP/USB旧写入转换及后续ROM去掉兼容代码仍在B6。未提交、未串口复位/改分区/清空NVS。


## 2026-10-04 13:30 紧凑 NVS .49已部署并镜像确认；缓存导入已部署

- 原固定WifiStoreV2为5,248B，生产HAL限制可写1KiB时双旧槽迁移稳定red；改用ESP ROM miniz的zlib编码，同一wifi-bonds/scheme键一次提交，解压/CRC/逐字节读回通过后才能退役旧键。host HAL用系统zlib解码同一格式；不改分区、不删其他NVS、不将密码写入文件系统。测试双槽5,248→164B green；原v1/v2读取、重复迁移、冲突、CRC损坏、写后读回失败/重载通过，受影响device-sim5 passed。实际ESP ROM编码与真实NVS空间仍须物理迁移证明；不能把模型red当作那两次实机错误已证实。
- Note4候选 0.18.49-note4-b-mig3 / zectrix-note4-400x300，1,730,464B；ROM `/Users/slepher/project/codex_status/artifacts/bridge-macos/note4-0.18.49-mig3.bin`，SHA256 `36057EAEB427D912FCD8C69A68951B48FF2139F6F5604EF905FBA3E45B480A76`，匹配ELF同目录note4-0.18.49-mig3.elf；zectrix-note4-b构建63.29s成功。OTA13c4c235 succeeded/image_verified，认证运行镜像SHA256/大小/目标与候选一致；当前运行.49。
- 设备→Bridge缓存方案选择已实现/部署/验证，详情下节。legacy旧键尚保留，迁移未完成，待新BOOT固定物理窗口进行迁移与核验；AP/USB旧写入转换和后续ROM移除兼容代码仍在B6。Windowshost新增系统zlib链接尚未验，macOS源码/固件本轮通过。


## 2026-10-04 13:22 设备 → Bridge 已改为缓存方案选择并部署；legacy迁移未完成

- 用户明确：设备同步到Bridge使用之前完整读取的数据，不再读设备。UI删除“刷新后导入”，方向按钮改为“保存选中方案到Bridge”；服务立即保存所选缓存字段，不新增jobs/占投递锁/联系HTTP或BLE。预览携带snapshot_id，在同一存储事务内检查快照与local_revision，变化即snapshot_conflict。旧read_import app请求兼容为缓存导入，MCP不再广告该操作；既有持久任务保留。
- 验证：真实app入口离线/持锁导入（含旧别名）通过，core Wi-Fi8通过，设备页JS通过；WKWebView12场景通过，560/1000比较截图已审阅。合成IPC截图不作为设备通信证据。证据artifacts/bridge-macos/rendezvous-deploy/cached-import-*。
- 当前默认Bridge已重建/签名/重启，health204、MCP与缓存view可用；exe 75,588,048B，SHA256 2A216511F8B783BF4B8BB1223F6B6DBC401E790A14BB7266422ED00591C14A3B；备份Codex Status Bridge-before-cached-import.app与运行数据保留。
- Note4当前已认证.48（0.18.48-note4-b-mig2），OTA c3e3c920 succeeded/image_verified，1,729,552B，SHA2561D5D58B7511789D2B26BB2C00B9E4647472C56F3EDEC37E8BF39F6A29FEA6198；ROM artifacts/bridge-macos/note4-0.18.48-mig2.bin，匹配ELF保留。此前.47完整读取990c2e75c538ff81一次completed，确认真实旧槽wd21-la/CMCC-k3v7；旧snapshot_shape失败任务保留。
- **legacy显式迁移仍未完成**：用户BOOT后两次storage_unknown，旧sN/pN未删。诊断.48区分storage_write/readback并报告空闲entries，未再取得有效物理窗口。NVS仅16KiB，已有备份3页近满/1页擦除；空间不足为重点假设，待紧凑存储与实机证明。AP/USB写旧格式入口仍在，完成迁移后须统一新写入并在后续ROM删除兼容代码。B6不结项，原RF/功耗/连续会合矩阵仍开放。


## 2026-10-04 12:36 配套部署已启动；.46运行确认，完整读legacy重复身份修复中

用户已授权启动Bridge并部署。默认LaunchAgent supervisor18112/child18114，8765/8766监听、health204、MCP200；部署签名exe SHA2560277B95A24BCB860C4F59C55F1DD20B39DDAEAC03812E6B4CD915457076039D8，旧app `Codex Status Bridge-before-rv1.app`与私有运行数据备份保留。Note4 OTA作业1df2d89b succeeded/image_verified，.46/ota_0/1723504B，认证运行镜像9BE3591D…A756D与候选一致。设备HTTP可达、BLE OFF、pm_light_sleep=true；尚无新固件同wake handoff证据。

用户读任务e263e08c8ac205b5（及旧4279…的第15次尝试）snapshot_shape终止。实际非秘密快照出现两个legacy-1，源码确认局部字符数组被JSON借用导致投影别名；已写[部署恢复计划](project-workflow/bridge-macos/rendezvous-deployment.md)，正在两个旧槽生产HAL red→green、Note4 .47构建/OTA及真实完整读恢复。新Bridge已观测一次disconnect_timeout并继续HTTP，不能据此宣称长期GATT/功耗矩阵已完成。


## 2026-10-04 会合协议模块与 Astra 审核收口——源码/候选完成，未部署

- 用户追加要求：计划同时覆盖“认证HTTP就绪并完成交接后关BLE，再允许light-sleep”和之前Astra六项结论；[计划](project-workflow/bridge-macos/rendezvous-protocol-plan.md)已逐A1–A8对齐，[实施与复审](project-workflow/bridge-macos/rendezvous-protocol-review.md)记录证据及边界。
- 设备生命周期移入`src/rendezvous_protocol`，main提供异步Wi-Fi/HTTP/GATT/PM适配；Bridge生产ble_cycle移入`app/src/rendezvous`，core模块统一准入/绝对预算/有限错误。普通BLE保持awake直至认证HTTP实证与handoff；固定物理恢复仍按原授权期限。USB自动BLE交接后不再立即重开。
- 已修：重放plan报当前remaining且镜像不续期；新ID追设备高水位、发前持久化；plan/sync_open/handoff ACK未知走同MAC/会话/wake/plan HTTP读回；本轮冻结Wi-Fi准入，多页完整读早于诊断complete；错误分类与旧快照保留；恢复/自动操作内部有界并最终记账；投递两锁释放后close_bounded。严格owner校验保留，仅通过既有token-gated显式claim占空闲设备，不force他人owner。
- Astra复审追加问题已收口：HTTP首次确认用取样时间；同启动第二轮/旧plan、更短计划、取消状态、三种临时开窗的时间戳及90s硬限。最终只读审核无剩余源码阻断；不代表实机通过。
- 验证：core/BLE/render197 + app63（含原ignored诊断）+串行device-sim50 = **310 passed / 0 failed / 0 ignored**；另生产C++状态机/证明专测通过。证据`artifacts/bridge-macos/rendezvous-{core,app,sim}-final.log`。旧诊断夹具补INFO→transport acceptance→Status ACK后通过；非生产故障red。最终GATT新能力字段/最大标识/hand-off编码边界专测另见`rendezvous-wire-final.log`。
- **Note4候选，仅构建**：`artifacts/bridge-macos/note4-0.18.46-rv1.bin`，`0.18.46-note4-b-rv1` / `zectrix-note4-400x300`，**1,723,504 B**，SHA256 **`9BE3591DBA265DC57B595EF43197A0044431668D5740937C00724C6EA01A756D`**；匹配ELF保留`artifacts/bridge-macos/note4-0.18.46-rv1.elf`。构建`zectrix-note4-b`成功（77.99s），未构建1.54/变体。Bridge生产候选cargo build成功，未覆盖部署.app。候选 `artifacts/bridge-macos/bridge-app-rv1`，75,023,304 B，SHA256 `4C533D80E6B3A15BE3087A0FA8A50F67066BC57CBB0824AE9E5C9B65C1FCA888`；构建基于共享工作树（含既有改动），不是独立部署包。候选manifest：`artifacts/bridge-macos/rendezvous-candidate-manifest.json`。
- **现场未改变**：未部署/重启Bridge、未烧录、未打开串口/改真实Wi-Fi、未提交。默认8765/8766无监听者，生产实例沿用02:50停止状态；Note4仍此前.45运行现场，运行数据和回滚包保留。
- **待办唯一归B6**：两端配套部署后同wake证明BLE存活时HTTP可达→handoff关控制器→PM light；电池/USB/物理恢复连续多轮完整读及自然会合；慢入网/换IP/不可达/ACK丢失、自然断链再连、功耗和panic转储。02:34部署后ACK恶化与“先放锁造成会话未拆净”仍只有相关性/假设，未做A/B，不能宣称已证实或实机修好。Windows/1.54/其他CPU架构另列未验。


## ⚠️ 2026-10-04 02:34 部署后 BLE plan ACK 失败率上升——实例已停，因果待验证

**用户问题**：读取设备方案的任务一直完不成（job `4279beb3ee99fb03`，attempt 14，`last_stage=snapshot`，`error_code=transport_or_snapshot_unavailable`），是计划改了没改完，还是协议仍有缺陷？

**回答：是第三种——本轮部署引入的回归；同时暴露一个既有协议弱点。** 两者要分开记账。

- **不是"计划改了没改完"**：`admission-fix`(task-6) 的准入改动**代码与测试已完成**（`cargo test -p bridge-core` 106 passed、`-p bridge-app` 57 passed/1 ignored、device-sim 串行 50 passed），但**尚未部署**：当前运行的 exe 是 02:34 我构建的 `59D7BA54…B0C6`，其源码快照早于 task-6 落地，**运行二进制里根本没有准入改动**。所以任务卡住与它无关。而且准入修复只改"是否给窗口/给多长"，不会修复 plan ACK 超时。
- **回归证据（部署前 vs 部署后）**：BLE `plan` 命令 ACK 结果
  - 部署前（约 5 小时）：64 次尝试中 `applied` 61 / `rejected` 2 / **`timeout` 1（1.6%）**
  - 部署后（约 14 分钟）：14 次尝试中 `applied` 5 / **`timeout` 9（64%）**，且最近 4 轮为 timeout/timeout/timeout
  - 失败 ACK 耗时几乎恒为 ~4.9s（`4916/4931/4901/4949ms`），是一次固定 5s 窗口内完全收不到 ACK，不是偶发抖动；成功与失败交替出现，说明链路本身通、是时序问题。
- **症状归因链**：`plan` ACK 超时 → 该轮会合不产生正式 light → 读作业只能借临时窗口 → HTTP `/api/wifi/snapshot` 虽**每次都 200**（日志可见每 ~63s 一次成功、无 error_category），但作业拿不到稳定窗口完成确认 → `waiting_rendezvous` + `transport_or_snapshot_unavailable` 反复重试。**HTTP 通、BLE plan 不通**是本轮的关键分工。
- **最可疑改动**：task-4 的 close 修复把 `drop(_device_delivery); drop(_delivery);` 移到 `link.close_bounded(2s)` **之前**，使"释放投递锁"与"BLE 链路真正关闭"之间出现窗口，可能让下一轮 `connect_any` 在上一轮链路/设备侧 BLE 会话尚未拆净时开始 → 表现为固定窗口内 ACK 收不到。此假设**尚未隔离证实**（时间吻合强烈，但未做 A/B）。
- **处置**：02:50 已 `launchctl bootout gui/<uid>/dev.codexstatus.bridge.managed` **停止实例止损**（进程 0、8765/8766 无监听）；**尚未回滚**。回滚路径已备：`artifacts/bridge-macos/Codex Status Bridge-before-closefix.app`（旧 exe SHA256 `B827FA658CCC1A2E3E86B44592A909550773251683A837C78E67155B7D80CF1A`）。运行数据 `~/Library/Application Support/dev.codexstatus.bridge/data` 未动，含两条作业记录：`c98bf7178356cbdc`（attempt 6，已 cancelled）与 `4279beb3ee99fb03`（attempt 14，waiting_rendezvous）。
- **未决**：①是否回滚到 `before-closefix` 并复测 plan ACK 成功率以确认因果；②若不回滚，需先隔离"先放锁"与"有界 close"各自贡献（可只保留有界 close、恢复锁在原位）再部署；③`wifi.rs:80/91/198` 同类无界 close 未处理。**在因果确认前不应把 close 修复继续留在生产。**

## Note4 dump 分区已写入并完成写后验收（2026-10-04 01:54）

- 用户优先级：①重新分区保留dump（**本轮已落地**）；②查清状态同步阻塞（并行调查中）；③完成原交汇设计（并行审计中）。计划与交接点见 [note4-coredump-plan.md](project-workflow/bridge-macos/note4-coredump-plan.md)。
- **写入与校验（USB，MAC 7C:4F:AD:B9:34:08，/dev/cu.usbmodem14101）**：`write-flash 0x8000 artifacts/bridge-macos/note4-coredump-partitions.bin` 成功且 esptool 自验；`erase-region 0xB00000 0x100000` 成功。读回 0x8000 逐字节等于候选表（3072B SHA256 6A11BC014301BC1D3938A7967B1A9661E99D13D9F866DBDC361EF9ABF8D00A39）；dump 区 1MiB **非 0xFF 字节 0**（SHA256 F5FB04AA5B882706B9309E885F19477261336EF76A150C3B4D3489DFAC3953EC）。与私有备份 flash-before.bin（SHA256 BEDD6478…DDDC）逐区比对：bootloader 0x0–0x8000 一致、otadata/phy_init 一致、ota_0/ota_1 两槽应用一致（活动 .45 仍为 B4635DC0…468C）、storage 头 64KiB 一致；**仅 NVS 有 658B 差异**（0x9037–0xA03E 单页，属运行期键写，非分区操作所致）。解析结构：nvs/otadata/phy/ota_0/ota_1(0x4F0000)/coredump(0xB00000,1MiB)/storage，无交叠、末尾=16MiB。
- 更正一处我自己的误判：分区表 MD5 尾字段为全 0xFF，我起初按"应写入 MD5"判 FAIL；核对设备**原始表**该字段同为全 0xFF，属该生成器行为，不是缺陷，已按此重记。
- **启动验收**：esptool 复位后串口启动日志显示 `v0.18.45-note4-b-wifi3 mac=7C:4F:AD:B9:34:08 slot=ota_1`，`[bundle] fs ready`、`[platform] bundle job=5d798bd9 active=codex-status-a templates=1 ctx=dea4fb9663b879f5 commit=421` 全部保持，Wi-Fi 连上 `192.168.10.101`（SSID CMCC-k3v7，bssid 98:97:CC:CD:C6:18）；HTTP `/status.json` 200（2632B：plugged=true、pm_light_sleep=true、state="BLE ON"、fw_target=zectrix-note4-400x300）。证据 artifacts/bridge-macos/note4-repartition-verify/（verify-after-write.json、partition-structure-check.json、partitions-after*.bin、coredump-region-after.bin、head-0x0-0x8000.bin、nvs-otadata-phy-after.bin）。
- 未验/遗留：**真实 panic 转储、CRC 校验与匹配 ELF 解码仍未验**，启动识别不代替转储验收；`FLASH_NO_OVERWRITE` 保首份 dump 仍属下次配置变更（当前默认可被新 dump 覆盖）；写后一次 hard-reset 后 HTTP 曾连续约 90s 无响应，恢复结论由 device-acceptance（task-2）复核。
- 约01:00自行重启已由认证reset_reason=4及公共panic确认；用户无物理操作。原始状态见restart-event-*.json；之后data/applied_seq=0。23:46 BLE plan ACK失败后协调日志停止，Bridge push_dirty=true/in_flight=null；无界link.close持锁可阻断HTTP同步，btleplug macOS自然断链后disconnect future存在不完成路径，属高可信候选，red 复现与最小修复由 ble-lock-investigator（task-1）给出，不与panic原因混为一谈。
- 写后设备验收（HTTP 只读）：`/status.json` 200，`fw=0.18.45-note4-b-wifi3`、`mac=7C:4F:AD:B9:34:08`、`slot=ota_1`、`committed_job_id=5d798bd9`、`template_count=1` 与基线一致（`active_context_id` 每次重启轮换属预期）。证据 acceptance.json / acceptance-status.json。
- **"设备一直重启"排查结论：不是崩溃重启，全部是工具诱发复位**。esptool 打开/关闭 USB-Serial/JTAG 口会按设计复位芯片；本会话写入 1 次、擦除 1 次、read-flash 5 次、hard-reset 1 次、串口抓取 1 次，加上 device-acceptance 子 agent 反复开串口轮询（已于 02:12 中断）。反证：Bridge 日志 02:11:00–02:12:55 本地时间连续 `/api/status` 200（延迟 83–250ms），设备 `uptime_s=230` 无中断，`reset=unknown`/`wake=power-on`，无 panic/`reset_reason=4`。**后续除非必要不要再开串口轮询**。
- 本线程发起的三个并行任务：task-1 BLE close 持锁 red 复现（ble-lock-investigator，completed）、task-2 Note4 写后只读验收（device-acceptance，Lead 接手完成并中断其串口轮询）、task-3 W4/W6 收口审计（w4w6-auditor，completed）。

## BLE 投递阻塞已取得 red 复现；W4/W6 收口审计完成（2026-10-04 02:12）

- **② 同步阻塞：生产路径 red 已复现（task-1，ble-lock-investigator）**。位置：`bridge/crates/app/src/platform.rs:2038` `ble_cycle` 持全局 `ctx.device_delivery.lock()`（2042）与每 MAC 锁（2054-2055）直到函数返回（2225）；唯一 10s 期限（2201-2204）**只包住 `work`**，其后 `recover_on_link`(2219)、`automatic_on_link`(2220)、**无界 `link.close()`(2222)** 与 `sync_http_inner`(2223) 全在期限外且仍持锁。`DeviceConnection::close`（`bridge/crates/ble/src/lib.rs:1421-1431`）→ `peripheral.disconnect().await`（1427-1429），无 timeout/无取消，`Err` 被吞但 pending future 不返回。同一每 MAC 锁被 `sync_http`(platform.rs:410)、`deliver`(1053)、`send_plan`(1222) 取用，全局锁被 `wifi.rs:33/86/126` 取用；而 `refresh_status`(1366) 与 `device_status_poll_loop`(main.rs:3061) 不取投递锁——这正好解释"HTTP 状态活着、投递死了"可同时成立。
- **red 证据**：复用既有生产 seam `CODEX_STATUS_SIM_BLE_ENDPOINTS`（`ble/src/lib.rs:975`），`/sim/ble/close` 接受后永不响应。`cd bridge && cargo test -p bridge-app link_close_hang_must_not_block -- --ignored --nocapture` → **FAILED(exit 101)**，日志 `red-link-close-hang.log`：`close observed=1; HTTP deliver(7C4FADB93408) blocked=true after 501ms; next ble_cycle(70041DD7A340) blocked=true; global device_delivery held=true; per-MAC lock held=true`；同流程 close 正常返回的对照 `control-bounded-close.log` 0.06s 通过 → 唯一变量是 close 悬挂。全量 `cargo test -p bridge-app` 52 passed / 2 ignored（red 用例 `#[ignore]`，不污染套件）。生产代码零行为改动，仅新增 `#[cfg(test)]` 模块（platform.rs:2669-2994）。
- **最小修复已实现并取得 red→green 恢复证据（task-4，close-fix-recovery）**：修复正是"两件同时做"——(A) `ble/src/lib.rs:1445` 新增 `DeviceConnection::close_bounded(limit)`（`tokio::time::timeout` 分别包 fake close 与 `peripheral.disconnect()`，超时 warn `event=close/category=disconnect_timeout` 后放弃句柄，**不 detach**；原 `close()` 原样保留）；(B) `platform.rs:2232-2249` `ble_cycle` 尾部先 `drop(_device_delivery); drop(_delivery);` 再 `close_bounded(2s)`，`sync_opened` 分支内重新取每 MAC 锁再跑 `sync_http_inner`+`opportunity_locked`（顺序仍是 close 先于 sync）。`BLE_CLOSE_LIMIT=2`s（`platform.rs:2045`）：close 已在两把锁之外，该值只界定对可能卡死 disconnect 的放弃时间，健康路径整轮实测 0.06s，把最坏停顿从无限压到 ≤2s。
- **恢复验证（同一注入，修复前→修复后）**：`cargo test -p bridge-app link_close_hang_must_not_block -- --ignored --nocapture` 修复前 **FAILED exit 101**（`deliver blocked=true after 509ms`、`next ble_cycle blocked=true`、两把锁 held=true，`recovery-red-rerun.log`）→ 修复后 **ok**（`deliver blocked=false after 2ms`、`next blocked=false`、两锁 held=false，2.06s≈close 上限，`recovery-green-red-case.log`）。Lead 独立复跑确认：`cargo test -p bridge-app` = **53 passed / 0 failed / 1 ignored**（既有 `sync_diagnostics` 需 `CODEX_STATUS_SYNC_TEST_SIM_EXE`），该 red 用例已由 `#[ignore]` 转为常跑并通过；`cargo test -p bridge-ble` = **17 passed / 0 failed**；`git diff --check` clean。证据 artifacts/bridge-macos/ble-close-lock/RECOVERY.md 与 lead-independent-verify.log。
- ⚠️ **该修复已在用户授权后部署（2026-10-04 02:34）**：`tools/build-bridge-macos.sh` 重建成功（46.65s，复用 `artifacts/bridge-macos/target`），旧部署先 `launchctl bootout gui/<uid>/dev.codexstatus.bridge.managed` 干净停止（LaunchAgent 定义在 `~/Library/Application Support/dev.codexstatus.bridge/dev.codexstatus.bridge.managed.plist`），随后 `launchctl bootstrap` 拉起。新运行 exe SHA256 `59D7BA54E992312F6745ED899A0847F0CE959E4C68B642698641C8242D17B0C6`（旧 `B827FA65…CF1A`），supervisor/child PID 74755/74757，`/health`=204，8765/8766 监听正常，日志显示已与 Note4 完成 BLE status/plan 与 `/api/status` 200。回滚包保留 `artifacts/bridge-macos/Codex Status Bridge-before-closefix.app`（旧 exe SHA256 `B827FA658CCC1A2E3E86B44592A909550773251683A837C78E67155B7D80CF1A`），运行数据 `~/Library/Application Support/dev.codexstatus.bridge/data` 未动（未重建/未清空）。
- **部署后仍未验**：本次部署验收只覆盖"进程/端口/健康/会合恢复正常"，**尚未取得线上真实 close 悬挂事件**（日志可见 `disconnect_timeout`/`link abandoned` 数为 0）；真实 macOS 自然断链路径仍需长期观察或故障注入。
- 未验/遗留：`wifi.rs:80/91/198` 三处同类无界 close **未改**（仅列出，避免扩大范围）；未做真实设备/macOS 自然断链验证；close 超时后 GATT 缓存与下一轮会合是否受影响未验；`automatic_on_link` 最长约 28s 窗口未缩短；btleplug 上游是否已修未查证。**不要用 `tokio::spawn` detach**（GATT 缓存未清，`ble/src/lib.rs:1416-1420` 注释记录 Windows 实测第二次会合必败）。
- **边界**：该复现证明"此故障下生产路径必然阻塞"，**不等于取证 23:46 那次具体 future**；close 超时后 GATT 是否未清影响下一轮会合需实机验证；`automatic_on_link` 最长约 28s 是否在实机扩大窗口未测；btleplug 上游是否已修未查证。
- **③ W4/W6 只读审计完成（task-3，w4w6-auditor）**：交付 [w4-w6-closure-plan.md](project-workflow/bridge-macos/w4-w6-closure-plan.md)（238 行）+ 证据 artifacts/bridge-macos/w4-w6/{code-references.md,existing-signals.md,w6-gap-matrix.csv}。要点：W4 侧根本不存在"本窗口已准入任务集合"，三套待办（plan/coordinator、`WifiState.jobs` service.rs:471、设备侧 `sync.due` main.cpp:4042）互不可见；排队 Wi-Fi 读不进入 `work_want_light`（service.rs:2366-2380，该区间无 wifi 引用）；诊断 complete 关窗不看 Bridge 侧读作业（main.cpp:4119-4120、5784-5787）。W6 现有 48 Fake + 51 app 测试中**无一条**经过 `ble_cycle→sync_http_inner→opportunity_locked` 顺序；两个会合审查测试是 PASS-现有行为而非修复证据。Lead 已抽检 `platform.rs:2215-2225`、`service.rs:2360-2382` 与"区间内无 wifi"三项，与审计一致。
- 待用户决策（触碰已部署协议/功耗语义，不可自行推进）：读任务是否允许自动申请有界正式 light（改 `service.rs:2366`），还是维持"仅借窗口 + UI 如实显示等待"；改动清单中 platform.rs:2223 顺序、wifi.rs 预算须先有 red，`service.rs:2366` 与 `main.cpp` 关窗需明确授权。

## 会合协议审查：窗口任务交接缺口已复现，未修改部署（2026-10-04）

- **原任务对账更正**：用户确认本交接本属原会合设计；执行文档§5/W4明确完整交接，A02及§8要求生产app组合验收。故W4窗口生命周期尚未闭环、W6组合证据不足；此前通过数量只证明其局部测试，W7部署事实保留。已更正执行文档当前状态/第14节与B6，非新增需求；BOOT仅调查/原维护授权入口，不能替代日常任务闭环。
- 审查见 [rendezvous-audit.md](project-workflow/bridge-macos/rendezvous-audit.md)。新增2个生产service/Fake门禁行为刻画测试：排队Wi-Fi读任务不改变sleep计划；临时窗口内快照可读，诊断complete后关窗使后续快照超时。最终2 passed/1.34s，artifacts/bridge-macos/rendezvous-audit-tests-final.log；PASS证明现有行为，未修复。首轮夹具虚构light_enter被正确拒range，已纠正，不能当目标缺陷red。
- 已确认Bridge先drain诊断再读Wi-Fi方案，而固件诊断complete后200ms无正式light即睡；Fake立即关闭，故是窗口归属/顺序竞态，不能宣称实板每次必败。读取只借已有窗口、不进入PowerPlan工作集合；普通BLE ACK非HTTP ready。慢入网与统一预算为静态风险，既有Wi-Fi测试直接service transport/预注维护而绕过app协调，缺组合验收。
- 现场时间校正：读job c98bf7178356cbdc 第5次尝试是10-03 **23:43:01**，http_status_unavailable；另一次23:44:02 plan264 accepted_remaining_s=0、无sync_open，wifi_authorize仅记录收到回复。23:44:07–13 HTTP超时不能与前次job当同事务，更不能用F2作为这次不可达唯一根因。串口“必然复位”更正为DTR/RTS/监控器依赖，未开串口。
- 仅增加审查测试/文档，未改生产代码、Bridge部署或ROM；当前应用与Note4 .45路径/大小/SHA256仍见下节。待办归B6：窗口任务集合/关闭条件、就绪与预算、生产app交接组合回归和实机多轮验收；未提交。

## Wi-Fi 分 tab 与原生布局测试已部署（2026-10-03 23:38）

- Bridge/设备/比较与同步三个真正tab，默认Bridge；ARIA与键盘切换、编辑保留、窄屏单列/宽屏双列、设备分组与比较表均已落地。切换不调用设备；真实用户截图已显示新布局。详细计划/验收见 [wifi-tabs-plan.md](project-workflow/bridge-macos/wifi-tabs-plan.md)。
- 原生产页面在WKWebView先失败于缺tab；修复后6场景×2尺寸=12通过，14张截图审阅，tab切换IPC=0；设备页JS与diff检查通过。证据 artifacts/bridge-macos/wifi-ui/{red,green,logic,build-final,package}.log、results.json、verification.json。渲染测试合成IPC，不是完整Tauri Rust/设备E2E。
- **当前运行Bridge** artifacts/bridge-macos/Codex Status Bridge.app，75,192,688 B / SHA256 B827FA658CCC1A2E3E86B44592A909550773251683A837C78E67155B7D80CF1A；UI SHA256 C2018DA7DB36261A35E724FAA65EDF1C2E03E523CFF4C6C88829C02E425053E3。打包与构建UUID/27代码数据段相符；原LaunchAgent supervisor33643（父1）/child33647，health204，MCP43工具与bridge_status正常。旧应用before-wifi-tabs.app、AppSupport/backups/wifi-tabs-20261003-233745私有data备份保留；安装身份/endpoint配置未变。
- **ROM未改**：Note4 artifacts/bridge-macos/note4-0.18.45-wifi3.bin，1,720,784 B / SHA256 B4635DC064378427FACBB62C6A7780DE9649E819CD0DB834B0063B90F149468C；.45/ota_1运行证明沿A8部署。未写真实Wi-Fi方案/发布Bundle/操作1.54或提交。
- **23:44读取调查**：MAC7C4FADB93408 @ 登记IP192.168.10.101，job c98bf7178356cbdc 第5次尝试仍waiting_rendezvous，snapshot=null、http_status_unavailable、sent=false。BLE status/plan ACK与wifi_authorize回复成功到达，但/api/status超时或传输失败，公共/status.json探测亦不可达；尚未进入完整快照分页。先建议短按BOOT打开120s维护窗口再看HTTP，未打开可能触发复位的串口；USB/深睡/入网/IP变化的因果未确定。真实配置/RF/窗口矩阵仍归B6。

## A8 余量 push 与重启上下文修复已部署（2026-10-03，Note4 .45运行镜像已核对）

- 用户授权推进A8，随后明确选择Bridge和Note4 ROM一起更新。共享classify_trigger复用resolve_field，使remaining按usedPercent默认push；显式源push/pull覆盖保持。另在真实OTA后发现同字段/同值的新上下文沿用旧data确认指纹，已在note_device_status的context reconcile中清空旧确认并排完整快照，序号保持递增、旧ACK不得确认新包。
- 新增生产PlatformService回归：无模板/Profile、首次ACK后93→92、HTTP/BLE、Bridge重启、显式push/pull、pull-only不推/不改Plan，共12组合；新设备context的HTTP/BLE和旧ACK隔离另2组合。两种缺陷均先取得idle/不dirty的失败信号，再修复通过。最终相关core105+21集成、app51/1 ignored、MCP3通过；不受最后上下文变更影响的BLE17/render45沿本轮通过结果。五crate唯一242 passed/1 ignored。
- Fake bootstrap首轮47通过/1失败（BLE用例126.23s后重连无窗口），以NO_PROXY/no_proxy=127.0.0.1,localhost复测该唯一失败用例18.39s通过；48项各有通过结果，不冒充首轮全绿。最后上下文变更影响的生产service Wi-Fi/丢ACK/重启Fake用例复测0.64s通过。合计唯一290 passed/1 ignored；设备页JS及git diff --check通过。证据artifacts/bridge-macos/a8-{red,green,context-red,regression,context-regression,fake,fake-focused,context-fake-focused,ui}.log。
- **A8部署时Bridge（现已由上节分tab版替换）** artifacts/bridge-macos/Codex Status Bridge.app，75,188,560 B / SHA256 FA15F8B52D383BEFB039B800900D3D91342409AE5A064B0944F9082C92627817；按原LaunchAgent/绝对路径替换，supervisor PID19571由launchd持有，health204/MCP正常。包含另一任务W0–W6 Wi-Fi候选。旧.44配套应用保留Codex Status Bridge-before-a8-wifi3.app，中间应用before-a8-context.app；私有data分别备份AppSupport/backups/a8-wifi3-20261003-222005与a8-context-20261003-223358。安装身份/endpoint配置哈希不变，MAC/许可/设备Profile与模板保留，未写真实Wi-Fi方案、发布Bundle或重新绑定。
- **最终运行ROM** Note4唯一目标zectrix-note4-b整板139.13s成功；artifacts/bridge-macos/note4-0.18.45-wifi3.bin，1,720,784 B / SHA256 B4635DC064378427FACBB62C6A7780DE9649E819CD0DB834B0063B90F149468C；marker codex-status-ota-v1|zectrix-note4-400x300|0.18.45-note4-b-wifi3，头E9070240。MAC7C4FADB93408标准OTA job63be0d96，单次上传ACK后succeeded/image_verified；独立认证/api/ota/image证明ota_1同MAC/target/版本、指定完整文件长度的运行前缀SHA256一致。wifi_management=2；旧.44 ROM/ota_0回退保留。未开串口/清NVS/操作1.54，未提交。
- **22:36:11真实自动数据ACK** HTTP seq16 / data_seq16，applied/displayed；源weekly.usedPercent=11，即剩余89，最后写屏缓冲明确89。认证data_seq=applied_seq=16，push_fp=acked_push_fp=18396089969837334073，push_dirty=false/in_flight=null，新context6c60bac854345c0b已有成功确认。原模板codex-status-a、committed_job5d798bd9、Mac本地Profile=null、许可true。此轮未调用push_now/伪造源；图证a8-verified-last.png，状态/源/指纹a8-verified-*.json。
- 边界：21:57旧版小时全量已由93更新90；本轮现场验证用最新90→89，首次重发不冒充持续同context的每次变化实测。最后公共状态plugged=false，reset_reason8为共享IDF的deep-sleep wake，不能把该成功当USB常驻实测；其后HTTP Host is down只记录不可达，不臆测复位。原USB BLE pending消费、间歇重连/INFO超时、局刷失败、真实密码试连/换网/RF/TCC/功耗和缺席平台仍独立未验；B5/B6保持开放。详情usb-sync-diagnosis.md与wifi-rendezvous-execution.md追加W7。

## 交接入口与历史归档（2026-10-03 整理）

- 最新部署、运行镜像、数据 ACK 与验证边界见上节。Wi-Fi 会合实施与 W7 部署证据见 [wifi-rendezvous-execution.md](project-workflow/bridge-macos/wifi-rendezvous-execution.md)；余量同步调查与修复证据见 [usb-sync-diagnosis.md](project-workflow/bridge-macos/usb-sync-diagnosis.md)。
- 待办唯一来源：[backlog.md](docs/roadmap/backlog.md)；文档导航：[docs/README.md](docs/README.md)。
- [2026-10-03 进度归档](docs/history/progress-archive-2026-10-03.md)：原文件较早的 68 节，覆盖 09-25 至 10-03；包含旧部署、双设备现场、构建隔离、数据恢复与历次调查的完整证据。
- 更早历史：[2026-09-25 归档](docs/history/progress-archive-2026-09-25.md)、[2026-09-23 归档](docs/history/progress-archive-2026-09-23.md)。
