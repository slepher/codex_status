# Plan原事务激活Wi-Fi

## Plan

用户授权恢复Plan发放激活，而非再增加network_start业务步骤。证据：11:26 Data872/Plan5880获Light600，needs_http=false；11:27下一周期才开网。

1. Plan携带版本化network意图（回报地址/端口、当前Host对应目标的有界匹配摘要），固件验证后与已接纳Plan/wake/principal关联，BLE配置完成/链路结束时执行。
2. 同Plan原意图重放不续窗口，改意图冲突；设备只连接与冻结摘要匹配的当前/已授权待激活配置，不回退旧SSID。成功Plan ACK仅证明授权，联网仍须同MAC HTTP证明。
3. Bridge发现新能力后仅发送Plan，不再发network_start；旧设备保留原维护路径，升级顺序先ROM后Bridge能力切换。Sleep周期全量仍不授予正式Light，必要临时窗口独立。
4. 共享C++逻辑/生产Bridge红绿测试、边界和故障回归；Note4唯一目标构建、栈门禁、候选哈希；可用后部署同机验证。保留现有数据/绑定/回退ROM，不提交。

## Task / status

- [x] 原bug反馈环与协议shape/重放/目标匹配/失败测试
- [x] 固件、Bridge、Fake共用逻辑配套实施
- [x] 构建/资源/栈/身份与回退门禁
- [x] 配套部署及真实同轮Plan→联网认证

## Review

不以Light ACK冒充Wi-Fi成功。不删除旧维护通道；同机新通道未验前保留。无新NVS格式：网络意图仅本wake有界RAM状态；完整Plan电源持久化沿用PPL1，重启重新认证原Plan/boot事实，不能重放旧wake的网络动作。

## 实施与验收证据（候选）

- 能力 `plan_activation:1`；Plan增加 `network:{v:1,bridge_addr,bridge_port,target:{network_id,open,metadata_key,credential_key?}}`。复用既有Wi-Fi匹配摘要，无明文密码；新增能力明拒坏shape/同ID改意图，不降级补发network_start。旧ROM继续原通道。
- Bridge在传输准入前计算正式Plan：小数据新push授Light并携带network，联合Data ACK后关闭BLE，固件随链路结束执行。已HTTP可达则复用；周期同值BLE Data保持Sleep。准备/修复配置仍在同轮有界BLE尾部，开始RF前共享wifiPlanTargetMatches核对当前/已授权待执行配置。
- 生产ble_cycle四项对照通过：60→59新push同轮HTTP证明、periodic同值Sleep、unchanged无网络、缺映射在发Plan前明拒；新能力路径零network_start。测试故障曾因夹具未报告已ACK的applied_seq而触发设备丢数据重同步，补齐真实状态后对照通过，未放宽产品断言。
- C++ PlanActivation共享shape/不可变重放测试；Fake复用同一解析器和当前/待激活目标检查；device-sim cargo check通过。完整core/app/render 401通过/5条件忽略（App123/Core181，其余共享/集成）；最终合同/边界8项通过，最大联合ACK含48 B头502 B，最大INFO新增能力仍合规。
- NVS增量0；空意图不申请网络字符串堆，实际接纳时按内容申请（上限384 B JSON + String分配取整/终止符）。String为16 B，PlanActivation含u64约24 B常驻，单份意图；不预留384 B固定数组，不复制配置库。最大字段/credential摘要由共享测试覆盖。JSON解析/规范化临时堆随函数释放，精确运行峰值尚未实测，不能以长度上限冒充整机峰值。
- 栈检查修复两项检测问题：长demangle标签挤占首250 B、工具链.xt.prop把函数错误当literal；仅对临时ELF移除映射做缺失帧补解码，原ELF/ROM不改。保留退役入口应缺席的显式检查，增加PlanActivation/RF tick调用链；最终门禁/哈希另记候选元数据。

证据在 `artifacts/bridge-macos/plan-activation-{regression,tests,shared,sim-check,note4-stack}.log`；最终ROM及app状态以部署小节与PROGRESS为准。

## 部署

Note4候选 `.106-note4`，env `zectrix-note4-b`，1,865,680 B，SHA256 `36ca4c3aacdc7b27c9c7cebe4cb18df0bee4048461e189f51050b6332b24ceb4`；匹配ELF SHA256 `40cbc05fcaccdd9f144132f2ffb40db61d956119e636df69be7591073e8fe21a`。88项栈门禁通过，最大受检本地链2784 B，PlanActivation::prepare256 B、RF tick352 B。最终ROM/ELF已冻结复制至 `artifacts/bridge-macos/note4-0.18.106-plan-activation.{bin,elf}`。

OTA原job `f7186b2f` 已排队，仅该事务观察；不得重建/覆盖冻结ROM或换ID重投。Mac候选7ad7ee30…（详见candidate.json）签名验证通过，原部署app eb4d56a4…已核验备份；切换前再停实例并备份运行data/installation-id。当前设备无USB串口，BLE已核对登记MAC并按约66秒自然会合；未做串口操作。

12:09原OTA已`succeeded/image_verified`：认证同MAC运行`.106-note4`，running-prefix-v1完整应用镜像1,865,680 B/SHA与上述冻结bin完全相同。旧运行槽保留；未改配网/owner/模板/Bundle。Mac 12:09:10已切换，exe79,089,552 B、SHA256 `7ad7ee3049e26cc0ed209f8240cc8eed945a4bc68c4d3b0666dceeedf70d0b40`；正式LaunchAgent supervisor1834，health204。停桥后完整data160文件及installation-id逐文件哈希一致，备份目录mode700；旧app SHA已核验。

### 实机验收阻挡（不冒充通过）

Data875已获真实HTTP applied/displayed。Sleep5902获ACK后原90秒临时同步窗口仍合法，正式remaining_s=0、临时物理mode=light，不能当作Sleep失败；12:11:10确认HTTP不可达。显式Light5904已按原ID排队，HTTP发送前连接失败（not_sent），尚未BLE投递/核验Plan自主激活。

新部署进程的macOS定位/蓝牙均`not_determined`，BLE主循环在真实权限检查处停留；Wi-Fi host也返回permission_required，不能假造SSID映射继续。已调用正常permission_all入口调起系统授权并向用户询问；需要用户在系统弹窗允许两项，之后继续观察原Light5904→同MAC HTTP报告/认证及图标。未修改TCC、未替用户批准系统权限。屏幕目视、自然下一次源用量变化、Windows部署与长期RF仍未验。证据 `plan-activation-{note4-deployed,macos-deployed,pre-contact,validation-status}.json`；原请求/回执保存在同目录。

### 14:20 用户授权后实机同轮闭环通过

用户确认已允许两项；实际permissions均allowed，Host connected且原映射恢复。继续原Plan5904，没有重新排队/换Plan ID。

- 14:20:07.881 BLE本轮准入needs_http=true。
- 14:20:08.690621 原Plan5904 ACK applied，network_activation=plan、无ack错误。
- 14:20:11.994731 同Plan5904 ready报告received，距Plan ACK **3.304110秒**。
- 随后同MAC认证HTTP状态：fw .106-note4，Plan5904、formal Light remaining591并继续递减；Wi-Fi connected、HTTP ready，普通BLE radio/connected均false。14:20:32.147本轮device rendezvous complete。
- 14:20:47同ID HTTP重确认仍applied；14:21:16.837 Data876获真实HTTP applied/displayed，无错误。没有为测试伪造源数据。

证据 `artifacts/bridge-macos/plan-activation-physical-proof.json`、`plan-activation-validation-status.json`；运行原日志UTC06:20对应本地14:20。这证明原Plan在当轮启动网络，无需下一轮另发network_start。用户随后明确回复“已显示”，Wi-Fi图标目视验收通过；自然新余量事件从源到屏幕的完整延迟、Windows与长期RF仍未验，保留A9余项。
