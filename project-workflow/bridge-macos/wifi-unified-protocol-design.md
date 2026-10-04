# 设备在线状态与统一控制协议：HTTP 直接交互与 BLE 回退

日期：2026-10-04。状态：已完成一次 Astra medium 审核并修订；用户已选择统一严格权限，设计决策已收口，已实施核心与软件验证，部署/实体证据见wifi-unified-protocol-execution.md。依据用户最新要求：统一管理语义，设备 HTTP 可用时直接通信；通过认证 HTTP 状态获知管理可用性与剩余期限，跳过 BLE 交汇。覆盖范围还包括USB插入、BOOT物理唤醒/恢复与Bridge主动激活的在线通知、期限及通道就绪。本文覆盖旧设计中“每次 Wi-Fi 管理操作必须先通过 BLE 获取短时 permit”的限制。实施和验收待办只维护 docs/roadmap/backlog.md B6。

## 1. 产品合同

1. 设备→Bridge导入完成后，配置成为独立Bridge内容；离线编辑与编辑内手动保存只写本机。同步是用户显式动作，使用已保存内容；来源设备/绑定不限制本地修改。
2. 已登记、已认证且HTTP可达时，读取、比较所需的最小状态、保存配置、查询结果、试连及启用均可直接走HTTP。正常操作不以BLE在线、一次BLE会合或短时BLE permit为前置。
3. HTTP不可达且存在真实BLE机会时，使用BLE执行同一命令；两种传输共用目标身份、调用者身份、授权、参数校验、原子事务、修订及回执。
4. 同步成功只表示配置已持久化；显式启用才改变活动网络。HTTP启用/试连允许链路中断，不能把“连接断开”解释成失败或再次执行。
5. USB常驻是设备本地检测到的可用性条件，不是认证凭据。USB常驻HTTP可用时不依赖light倒计时；电池模式仍受现有正式light/sync/物理恢复期限约束。管理流量不改变PowerPlan、不延长RF期限、不自动发布Bundle。
6. “总是激活HTTP”指设备当前保持网络服务的场景（例如PC USB常驻且已联网）；深睡、断网或AP未关联时不承诺原STA地址可达，不为了协议统一常开电池设备的Wi-Fi。

## 2. 统一设备状态、在线通知与可用期限

### 2.1 一份DeviceStatus，不以BLE会合定义“在线”

GET /api/status与绑定加密BLE的status返回同一逻辑DeviceStatus；HTTP和GATT只负责不同组包。status_seq只在状态实质变化时递增，不为每毫秒倒计时写NVS；倒计时由采样时间和单调时钟推算。设备每次USB变化、BOOT处理、接受Bridge激活计划、Wi-Fi取得/丢失IP、通道关闭及期限变化时更新状态。状态中的来源由设备确定，不接受调用者提交plugged或BOOT标志。

核心字段：完整device_mac、boot_id、status_seq、session_nonce、power、presence、transports、wifi_control及既有owner/能力。presence描述设备为何保持可操作，以及该机会剩余多久；transports描述目前哪个通道实际就绪；wifi_control给该认证调用者操作权限。三者均来自同一设备状态采样，不能拼接不同boot或较旧seq的局部结果。

```json
{
  "protocol": 2,
  "device_mac": "7C:4F:AD:B9:34:08",
  "boot_id": "current-boot",
  "status_seq": 27,
  "session_nonce": "current-session",
  "presence": {
    "mode": "continuous",
    "reasons": ["usb"],
    "remaining_ms": null,
    "window_id": "current-availability-id",
    "maintenance_remaining_ms": 0
  },
  "transports": {
    "http": {"ready": true, "ip": "192.168.10.101", "port": 80},
    "ble": {"ready": false}
  },
  "power": {"mode": "light", "plan_id": 876, "remaining_s": 0},
  "wifi_management": 3,
  "wifi_control": {
    "mode": "continuous", "read_allowed": true,
    "write_allowed": true, "remaining_ms": null, "reason": null
  }
}
```

presence.mode可为continuous、window、ending。HTTP.ready指已取得非零IP并且服务已经监听，BLE.ready指可进行绑定认证交互；仅Wi-Fi关联或广播被看见不等于HTTP就绪。服务就绪也不等于OTA或某个敏感操作已获授权。

presence.remaining_ms在USB常驻时为null，BOOT/正式激活窗口为设备单调时钟计算的剩余时间；维护授权单列maintenance_remaining_ms，USB常驻不能把BOOT120s变成无限物理授权。多种保活原因可并存：USB为continuous，物理窗口仍独立倒计时；拔USB后重新计算是否还有正式/物理窗口，不把拔线直接等同掉线。wifi_control进一步受实际RF/操作截止约束；两个remaining_ms分别是在线机会与配置操作机会，不能互换。

### 2.2 三种触发走同一状态事件链

| 触发 | 设备行为与状态 | Bridge行为 |
|---|---|---|
| 插入PC USB（SOF已确认） | 进入现有USB常驻策略，reasons包含usb，presence continuous；Wi-Fi未就绪时报告正在建立而非捏造HTTP ready | 获知设备出现后认证读取状态；HTTP ready则直接通信，无需BLE交汇 |
| 单击BOOT或GPIO0物理唤醒 | 开启已有固定120s恢复机会，reasons包含boot；初期可只有BLE，随后按当前网络能力提供HTTP | 知道设备已在线及剩余时间；HTTP ready直接使用，否则通过绑定BLE建立/确认通道 |
| Bridge显式主动激活 | 复用协调器、现有正式PowerPlan/有界OPEN_WIFI控制；ACK含激活request_id、window_id及剩余时间，reasons包含bridge | 不等设备“自然”会合；收到受认证ACK即知道控制通道在线，HTTP ready后继续交互 |
| 定时会合 | 保留既有电池有界机会，reasons包含timer | 采用同一状态、通道与期限规则，没有另一套业务权限 |
| Wi-Fi成功/断链/关窗/拔USB | 更新同一状态seq及通道可用性，发送状态变更提示；终止前尽力报告ending | 更新显示/调度，不继续使用过期的ready/期限 |

BOOT窗口按物理事件起点计算；处理/连接/轮询不续期。这里不把BOOT恢复120s与PWR/冷启动既有provisional300s混成一条倒计时，也不强迫一次BOOT去延长正式PowerPlan。BOOT离线/AP时仍可能只提供BLE，协议统一不伪造Wi-Fi连接。

Bridge主动激活本身是显式业务意图，只有协调器能发正式PowerPlan；控制/配置请求不会偷偷激活或续窗。HTTP已经可达时直接通过HTTP发控制/Plan；HTTP不可达时使用已具备的绑定BLE或已部署的认证控制接收机会。整机deep sleep且没有任何接收通道时无法即时远程唤醒，显示“激活请求已排队，等待控制接收机会”；不能凭Bridge发出请求就显示设备在线，也不设计不存在的HTTP唤醒能力。未部署的bridge_first控制能力必须按capability显示，不当作现行实机功能。

### 2.3 在线事件通知，与认证确认分开

复用现有UDP announce/BLE广告作为发现hint，同时给已登记Bridge endpoint提供定向HTTP POST /api/device/presence提示（默认及命名实例各自端口）。三种发现来源使用同一轻量事件结构：event/device_mac/boot_id/status_seq/trigger/候选HTTP地址/可用通道hint；BLE广告只携可装入现有广播的路由hint，完整状态经绑定GATT读取，不能往31B广告硬塞整份JSON。

- USB插入/BOOT/激活ACK/HTTP就绪等状态变化主动提示，不依赖BLE=ON标志才发。定向通知在设备loop有界执行，不能阻塞BOOT处理/无线deadline；发送失败不会触发复位/延長窗口。
- 定向通知复用已存endpoint凭据，发往该记录的host/port；只发送状态提示，不发送密码/操作token。不把所有设备共享的endpoint凭据认证或未签名UDP hint当作某MAC的权威证明。
- Bridge收到事件后合并重复boot/seq提示，只对登记MAC发有界认证HTTP探测；若HTTP未ready而BLE可用，则绑定GATT读取同一DeviceStatus。事件不能自动claim、发布配置、申请新PowerPlan或触发敏感副作用。
- 只有认证状态/命令ACK建立可信在线状态。通知回复202表示收到hint，不表示设备业务成功。跨boot的提示需重新认证；同boot较旧seq不能覆盖新状态；地址变化必须认证完整MAC才更新登记。
- 窗口内有界重试事件通知，沿用已有announce节奏，并在USB常驻保留低频状态校验；Bridge启动/重启时主动核对已登记设备，不能依赖过去那一次USB事件。最后通知丢失、Bridge临时退出、UDP仅默认实例监听均不能造成永久失联。
- 有限机会倒计时按本机单调时钟保守推进，过期显示“窗口已结束/预计休眠”；USBcontinuous没有期限但也没有永久在线证明，仍需近期认证联系。超时先标“状态待确认/通道暂不可达”，与预期深睡、Wi-Fi断网、认证失败分别显示。

事件通知是同一状态协议的发现机制，不是第二套业务消息/授权系统。HTTP/GATT认证确认后，经过认证确认后自动继续用户已明确提交的排队任务，无需再次点击同步；仅通知本身不产生副作用。所有排队管理任务都从同一个DeviceStatus驱动，避免Wi-Fi、OTA、模板发布各自维护互相冲突的在线标志。OTA/bundle实际传输继续保留各自有界流式格式。

### 2.4 通过HTTP判断是否可以直接操作

复用认证GET /api/status，不新增单独“轻睡接口”，避免两份状态语义漂移。现有power.mode/remaining_s描述正式计划，不足以表达USB保持在线；CPU自动light-sleep、业务light模式、HTTP可达及管理可用性分别记账。

DeviceStatus中的wifi_control按当前认证调用者计算，下例只摘取相关字段：

```json
{
  "device_mac": "7C:4F:AD:B9:34:08",
  "boot_id": "device-boot-id",
  "session_nonce": "current-device-session",
  "wifi_management": 3,
  "power": {"mode": "light", "remaining_s": 0},
  "wifi_control": {
    "mode": "continuous",
    "read_allowed": true,
    "write_allowed": true,
    "remaining_ms": null,
    "reason": null
  }
}
```

上述是协议示例，不包含实际token或配置。continuous的null表示没有业务窗口倒计时，不是无限执行时间；每次请求仍有有界超时。

| wifi_control.mode | 设备端条件 | Bridge行为 |
|---|---|---|
| continuous | PC USB保持唤醒策略生效、网络服务运行；即plugged且!deep_usb，非正在关闭/重启 | HTTP直接执行，正式light剩余0不阻挡 |
| window | 已存在正式light、合法sync或物理恢复的可执行窗口 | 按remaining_ms及单操作预算执行，倒计时不被流量续期 |
| unavailable | 网络关闭/进入睡眠/无合法窗口；或服务正准备退出 | 等待可用通道，不能以status可读推断写入可用 |

read_allowed/write_allowed同时包含调用者权限、owner、设备忙碌等条件；reason枚举auth_required、binding_required、claim_required、occupied、window_closed、ota_busy、storage_unavailable等。公开/status.json只能用于发现和展示，不据此授权业务写入。

remaining_ms取已存在的有效RF/业务截止的较短剩余时间。Bridge从HTTP请求开始的本机单调时钟锚定预算，减去通信成本及既有收尾预算，不将设备单调时间当成本机时间。continuous也限制单次调用时间；试连/关联/DHCP沿用最多25s和现有恢复期限的较短值。OTA继续使用它独立的上传预算。

状态是快照，不是许可票据。设备收到每条命令时再次校验当前权限、owner、boot/session与可用性，提交前再次核验截止；不能用读status时的write_allowed覆盖随后发生的关窗/拔USB/占用变化。事务已原子提交后丢ACK，依靠回执恢复，不倒退到再次写入。

## 3. 统一调用者身份与授权

现行endpointTokenAuthorized只找到EndpointRec.mac，Wi-Fi仓库却按blePeerIdentity分区，两者不能直接假定相同。现行result还接受客户端result_peer过滤；新协议必须由认证上下文决定归属。

采用设备侧持久关联：

- principal = 稳定bridge_id（已有安装身份和命名实例身份）。
- 该principal关联已认证的endpoint凭据、已绑定BLE identity以及设备分配的配置namespace。旧Wi-Fi namespace保留原bond字符串作为兼容别名；新namespace用设备生成的16位hex标识，满足当前21字节上限，不把最长64字节bridge_id直接塞进Wi-Fi bond字段。
- HTTP由Bearer endpoint token及已登记关联解析principal；请求bridge_id必须与解析结果相符。BLE必须同时证明绑定/加密canonical peer identity和该principal的endpoint token，二者命中同一持久关联；bridge_id只校验一致性，不能由客户端选择有权使用的实例。连接generation仅防旧连接消息串入，不是principal证明。body中的bond/peer/namespace不具有授权作用。
- Endpoint登记增加bridge_id并在经过现有绑定加密链路时建立关联，使用canonical blePeerIdentity，不能使用可变化的当前BLE地址猜测归属。首次注册、跨peer重绑定及旧USB/AP迁移仍需既有物理恢复授权。
- 同一peer可关联不同命名实例；endpoint关联按principal管理，不能按peer地址把后一个实例覆盖前一个。重复token映射到不同principal必须明确拒绝/修正，不按第一行选取。现行endpoint满后LRU替换不能沿用为授权记录的删除策略：关联满则明确capacity，撤销/替换须显式处理，避免静默遗失长期身份而配置仍留存。旧peer namespace默认只关联一个经证明的principal；两个实例共享peer不能因此同时获得同一旧namespace，不能先占先得，歧义保留数据并要求登记修复。
- 普通HTTP调用通过已持久关联直接获得调用者权限，不先调用BLE wifi_authorize，不持久保存临时permit。Bridge重启后复用已受控存储凭据，读取一次认证status即可继续；会话nonce变更时仅重取最小状态，不重新读取整份配置。
- claim仍是显式独立协议行为，沿用设备操作token；所有保存/试连/启用副作用（包括BOOT物理模式）都要求当前owner与principal匹配；无owner时先通过独立显式claim取得占用，BOOT不豁免。只读可查看既有非秘密概要及自己的完整凭据，不能读取其他namespace密码；回执查询只能查询本principal/namespace事务，不要求当前仍拥有owner或旧nonce仍有效；查询不产生新副作用。
- token失效明确进入认证恢复，不能伪装成“等下一次窗口”。撤销绑定同时撤销对应凭据关联；Wi-Fi已存配置是否删除必须另行显式选择，不能静默抹掉用户数据。

这项关联是统一协议所需的身份修复，不是开放HTTP无认证写入，也不是把所有Bridge变成共同配置owner。OTA/claim仍采用现有设备操作token；统一服务合同不等于把业务凭据和设备管理凭据合并成一个秘密。

### 3.1 BOOT救援统一严格权限（用户已确定）

2026-10-04用户选择：允许绑定/身份修复；保存、试连、启用都先取得本机claim，他人占用时拒绝。该选择取代现行BOOT窗内保存/试连可越过endpoint/owner的行为；这是明确授权的政策收紧，不保留物理保存例外。

- BOOT只开启既有120s物理恢复机会，允许建立或修复合法绑定/endpoint关联；尚无endpoint身份时，先完成身份登记并取得所需设备操作token，再独立claim。
- 无owner也不能凭BOOT直接保存/试连/启用，必须先显式claim；有有效他人owner时明确拒绝，物理事件不隐式释放/转移owner。
- HTTP与BLE、USB常驻与BOOT模式执行相同owner规则；USB不等于物理授权，BOOT也不等于owner。身份修复不能接管别人的principal/namespace。
- 正式启用无旁路；legacy迁移保留身份/owner与物理授权要求，不由绑定修复扩大为读取/改写其他namespace。
- 当前有效身份可查询自己的已有事务/停止自己仍可取消的操作，即使owner后来改变；查询/停止不启动新的保存/试连/启用，也不操作他人事务。

| 操作 | HTTP与BLE共同合同 | BOOT的作用 |
|---|---|---|
| status/snapshot/match/result | 有效principal，仅自有凭据/事务，概要按既有可见性；查询不要求仍为owner | 提供物理恢复机会，不放宽数据归属 |
| apply/trial/activate | 有效principal、已取得本机claim且owner匹配、当前可用性/期限 | 不豁免身份/claim/owner |
| cancel | 有效principal及该操作归属，只停止自身未完成操作 | 不取消其他principal操作 |
| claim | 设备操作token，显式claim/lease，当前有他人owner时拒绝 | 不隐式转移owner |
| legacy迁移/重绑定 | 既有物理授权及明确身份/owner规则 | 不产生跨namespace旁路 |
| OTA | 设备操作token、既有owner/预算/互斥 | 不降级为普通endpoint凭据 |

## 4. 两个适配器，一套处理器

统一设备控制上下文DeviceControlContext(auth_context, DeviceStatus, operation_budget)，供status、PowerPlan、claim、Wi-Fi、Bundle及OTA调度共用；各操作保留既有业务载荷、所需credential scope和显式动作合同，避免为了统一重写已工作的Bundle/OTA流式协议。新增逻辑入口WifiCommand(auth_context, request, availability)。HTTP与BLE只负责认证、组包/分页和传输；处理器负责授权、owner、revision、操作执行及结果。逐步将wifiManagementCommand和wifiSetup的相关分支汇入该入口，删除按transport决定业务权限的分支。

新能力wifi_management=3；外层平台protocol=2沿用。HTTP POST /api/wifi/command与BLE wifi_command承载同一业务JSON。现有/api/wifi/snapshot、apply、result及wifi_save等命令只作兼容适配，映射到同一逻辑操作；兼容入口不能绕开新身份校验。API最终退役单列验收，不为统一协议顺带改名所有平台API。

```json
{
  "protocol": 2,
  "device_mac": "7C:4F:AD:B9:34:08",
  "bridge_id": "stable-principal",
  "request_id": "stable-transaction-id",
  "session_nonce": "current-device-session",
  "op": "apply",
  "expected_revision": 12,
  "changes": [{"network_id":"home","host_ssid":"Home-5G","ssid":"Home-2G","label":"Home","credential":"keep","deleted":false}]
}
```

| op | 语义 | 成功证据 |
|---|---|---|
| snapshot | 显式完整读取并分页，返回本调用者可见配置 | 所有页属于同snapshot_id/revision且完整 |
| match | 单条内容/活动版本/连接诊断，无整份配置或密码返回 | 关联network_id/version的结果 |
| apply | 1–4条冻结整行变更，原子保存，不切网 | NVS验证成功的committed回执 |
| result | 查询同request_id，跨HTTP/BLE恢复 | committed/rejected/unknown，或异步操作状态 |
| activate | 显式启用已保存network_id/version | accepted仅表示开始；后续结果和认证网络状态证明完成 |
| trial | 有界RAM试连，失败/取消恢复，不自动保存 | 关联尝试id/version的结果 |
| cancel | 取消尚可取消的异步试连/启用 | 明确取消结果；已提交apply不回滚 |

所有返回含完整device_mac/request_id/操作/有限error；owner与nonce等校验按逻辑操作复用现行规则。去重键为principal+namespace+request_id，业务指纹包含op及目标版本/冻结内容，不包含token、nonce、generation或传输类型。相同事务id及业务内容重试只返回原结果，不重复执行；同id不同业务内容拒绝request_conflict。认证后才能查回执，回执查询/已提交回执重放不要求当前owner或旧session_nonce仍有效，但必须具有当前有效principal及对应namespace权限，且不能产生新副作用。

HTTP允许JSON有界完整载荷；BLE继续MTU分片，命令上限4096B，INFO/ACK单个属性仍≤512B。snapshot分页使用同一cursor/snapshot/revision语义，response_budget只决定每页可装多少内容；每帧按真实序列化字节检查，不能静默裁剪身份或条目。读取次数由显式用户get决定；提交/查询结果不重复读整份配置。

### 4.1 异步接纳、持久提交与结果证据

HTTP试连/启用先返回accepted再由设备loop执行，避免HTTP handler阻塞到切网结束。accepted只表示接纳，不是已保存/已启用。两类操作都先完成身份、目标、容量和期限校验，再产生RF副作用；相同事务不会并发开启另一份候选。

| 阶段 | activate | RAM trial |
|---|---|---|
| 接纳前 | 校验目标版本；预留既有回执容量，满容量直接拒绝，尚不切网 | 校验参数/期限及有界RAM资源，尚不切网 |
| accepted | 切网前原子记录必要的pending事务身份/op/目标版本，active保持旧引用；该记录不含重复密码 | 接纳/去重元数据留RAM并含boot_id；不持久化试连密码 |
| 执行 | principal/request_id归属，关联/DHCP受统一截止约束 | 同一归属与截止，结果只证明该次试连 |
| 成功终态 | active namespace/id/version与该事务committed完成回执在同一次WFS3原子提交生效，随后才报告完成 | 当前boot内保存RAM终态，丢ACK仍可查询 |
| 失败/取消 | 不留下成功回执；按实际证据记录失败/取消，旧持久active仍有效 | 按当前boot实际结果返回失败/取消，不保存为活动配置 |
| 掉电/重启后 | pending与原子完成回执可区分“未持久启用”与已提交；不能仅凭当前SSID/version推断某事务成功 | 没有持久终态证据时返回“结果未知，原尝试不再运行”；不能统一断言interrupted_by_reboot，更不能自动重试 |

apply沿用WFS3原子提交、CRC/读回和持久回执。activate扩展既有回执的op/目标/阶段等最小元数据，与active在同一存储事务内完成，不另建全量配置日志；需要新格式版本并保留旧apply回执的指纹验证规则，不能把旧格式强行当作新操作回执。真实字节/原子恢复必须在实施时测量。现有8项回执容量及result_capacity明拒仍有效，不静默删除未确认回执；退役机制独立验收。

结果绑定principal、namespace、request_id、目标版本与boot证据。断网/超时进入awaiting_confirmation；Bridge从任何可用通道查询原id，不自动重新activate/trial。试连在旧boot已完成后丢ACK、再重启，与进行中重启都可能表现为无终态证据，因此UI必须保留结果未知。

### 4.2 生命周期跨传输，恢复旧网也受截止约束

普通已接纳操作归属principal/request_id，不归属某条HTTP连接或BLE generation。传输断开只影响结果投递，不等于取消；generation仅约束接收命令时的认证消息。必须移除旧wifiSetupPoll中“没有当前BLE连接就取消普通候选”的业务依赖，HTTP-only及BLE→HTTP确认才能成立。

显式cancel、调用者关联撤销、owner不再允许、设备关闭和绝对deadline是独立终止条件；每个条件按统一严格owner政策及操作性质校验。query/cancel仍只针对自身事务。物理恢复授权到期不能被USB常驻或通道换接续期。

候选关联、DHCP、恢复旧网共用原有绝对deadline与收尾预算，不给每个阶段再开25s。尚有合法RF时间才能恢复旧网连接；期限已到时恢复旧持久active引用、清候选凭据并停RF，不能调用WiFi.begin重新开启无线，不能承诺立即重新联网。已原子完成的激活不因ACK丢失或通道断开回滚。

## 5. Bridge选择通道与任务状态

1. 用户提交后先持久保存冻结意图、request_id、local_revision、expected_revision及创建时间，不持久保存传输许可。
2. 在线事件与主动探测统一进入DeviceStatus协调器；优先对登记IP作有界认证HTTP状态读取（复用已有足够新且同boot的认证联系可省重复探测）；核对完整MAC、调用者、能力及wifi_control。
3. HTTP允许执行则立即提交统一命令。USB常驻模式不必发送新PowerPlan，不等待BLE；需要合法电池窗口时依照现有调度，不以管理请求续窗。
4. HTTP不可达才尝试实际存在的绑定BLE机会；不能靠持续扫描或常开BLE制造机会。HTTP返回revision_conflict、occupied、auth错误或参数错误时不切BLE规避拒绝。
5. 任何通道已可能发出副作用，后续只用原request_id查结果。换通道、Bridge重启、设备重启及ACK丢失都不生成新事务覆盖旧事务；查询unknown保留不确定性。
6. ACK确认后由冻结内容投影更新缓存/基线，保留较新的本地编辑及未选项。设备→Bridge导入只使用显式读取后的缓存。

页面先显示设备在线状态：USB常驻在线、BOOT在线（剩余时间）、Bridge激活中/已在线（剩余时间）、BLE已在线但Wi-Fi建立中、预计休眠、等待确认在线状态；再显示任务状态：正在通过HTTP同步、正在通过BLE同步、等待设备上线、等待设备工作窗口、需要恢复认证、设备由其他Bridge占用、版本冲突、等待确认、已同步。显示创建时间/最近尝试及实际等待原因；不能把HTTP可达但缺身份映射描述成“等待下次BLE会合”。业务数据投递许可不影响用户显式配置管理。

## 6. OTA的处理

已满足Wi-Fi HTTP OTA路径：当前Note4 .60经OTA成功部署，随后设备操作token认证核对target、大小与运行hash。它不依赖本次缺失的Wi-Fi permit；首次取得/轮换设备操作token仍经已绑定BLE，后续复用缓存。

本设计保留现有OTA上传、设备操作token、显式claim、目标/ROM检查、OTA互斥、上传后重启与运行镜像确认。OTA可共享HTTP可达检查与调用状态展示，但不把ROM上传编码进小型Wi-Fi command，也不改成普通endpoint token可刷固件。HTTP可达不是OTA必然成功，仍需有效token、匹配target及足够上传/重启预算。USB常驻不应要求先BLE交汇才能使用已缓存OTA凭据；电池OTA窗口仍由现有正式OTA计划提供。

## 7. 兼容、存储与实施顺序

| 阶段 | 内容与完成门槛 |
|---|---|
| U1 共享语义 | 提取同一auth_context/availability/WifiCommand及Fake入口；HTTP/BLE参数、owner、结果一致 |
| U2 身份关联 | Endpoint记录显式principal/canonical peer/namespace关联；可验证原子迁移，旧关联有歧义明拒 |
| U3 HTTP直通 | wifi_control状态及HTTP命令入口；USB plan remaining0可完成，Bridge重启/permit过期不依赖BLE |
| U4 BLE适配 | 相同处理器与事务跨通道，分页/512B限制/连接generation保持 |
| U5 Bridge/UI | HTTP优先调度、统一确认恢复、等待原因；当前get/import/save/sync完整链路回归 |
| U6 部署验收 | 软件、资源与实体矩阵通过后再配套构建/部署；旧设备显示能力不足，已发送旧作业先查原回执 |

旧设备wifi_management2不能误判支持新HTTP授权；不能在客户端去掉permit后直接往旧apply发送。旧存储关联只在可靠canonical peer与principal证明成立时迁移；缺失或地址身份不一致需要一次绑定登记修复，不能拿网络SSID/network_id猜principal。已存Wi-Fi条目和旧namespace保持，正常用户不需重新导入或重填已缓存密码。旧事务request_id/expected_revision/冻结内容保留，可能已发送的任务不能自动转换为新写入。

容量分别计数：身份关联最多8个，含保存条目的Wi-Fi namespace最多3个、每namespace最多4条、总计12条，持久回执最多8项。本轮保持这些现行上限。仅登记身份不占配置条目；首次apply按实际namespace/条目容量明拒。旧孤立namespace保留并占实际配置容量，迁移不得静默合并或替换它们；不能把8个关联解释成32条网络容量。

关联存储使用显式格式标识/版本、payload长度、记录数和CRC，变长字段按实际用量保存；新格式验证成功后才退役旧键。复用现有endpoint凭据，不另复制Wi-Fi密码或token。资源验收必须给空/1条/典型/8条关联持久字节、A/B峰值、迁移峰值堆与真实栈帧；网络命令复用有界JSON缓冲，无整仓库栈副本。这里尚未实现，因此没有把预算目标冒充测量值；匹配ELF仍须通过check-wifi-stack.py。分别测量8关联、3 namespace/12配置、8回执，以及迁移/pending激活/试连/snapshot组合峰值。新增持久字段、激活回执和snapshot工作区均计入预算。

## 8. 必须通过的验收矩阵

| 场景 | 必须证明 |
|---|---|
| 同peer两实例/token交叉/缺token/地址轮换 | 必须同时证明peer与principal凭据；跨namespace读取/回执/分页拒绝，旧namespace不被两实例共享授权 |
| BOOT严格权限/无owner/他人owner/缺endpoint | 可完成合法身份修复；无claim不能保存/试连/启用，他人owner时拒绝；HTTP/BLE一致，legacy迁移仍需物理授权，其他namespace不可写 |
| 激活accepted前后/切网后/active提交前后/ACK前后掉电 | 预留容量；active与完成回执原子一致，未完成与已提交可区分，无重复RF副作用 |
| 试连成功丢ACK后重启/执行中重启 | 无持久终态证据均如实结果未知，原候选不再运行，不自动重试 |
| HTTP-only试连/BLE断连后HTTP查询/拔USB/owner变化/末端恢复 | 传输断开不取消普通已接纳候选；所有RF与恢复共享原截止，关窗后不WiFi.begin |
| 8身份关联但仅3 namespace存配置/第4 namespace首次apply/满回执激活 | 登记与配置容量独立；实际超限明拒，无静默合并，激活未在容量拒绝前切网 |
| USB插入/拔出通知、Bridge未运行后重启、通知丢失/乱序 | 正确重建同一在线状态，命名实例不依赖默认UDP；旧事件不覆盖新状态，拔USB计算剩余有效窗口 |
| BOOT/物理唤醒、离线/AP、120s结束 | 主动发现，准确区分BLE在线/HTTP ready及维护剩余时间，重复联系不续期 |
| Bridge主动激活、接受/拒绝/ACK丢失/深睡无接收机会 | 同request/window关联，真实ACK才在线；窗口不重开，未送达明确排队 |
| USB接入、BLE OFF、light remaining0 | 认证HTTP完整读取→导入→编辑保存→HTTP同步成功，未要求BLE/拔USB |
| Bridge重启/旧permit过期 | 只恢复凭据并HTTP握手即可同步；不重复读整份配置 |
| 电池light/sync窗口 | 直接HTTP成功，期限未被读取/写入/claim延长；窗边界/拔USB及时停止新执行 |
| HTTP断网、BLE可用 | 同一命令经BLE完成，语义与HTTP一致 |
| HTTP写成功丢ACK→BLE查询、反向同样 | 一个事务一次提交；跨通道只查结果，不盲写 |
| HTTP试连/启用切断原链路 | accepted不冒充完成，换网后认证确认；失败恢复与重启中断可解释 |
| 设备/Bridge重启及掉电 | 原子存储、活动版本、回执与未确认任务恢复正确 |
| 来源/归属 | 其他来源导入独立编辑，回传写当前认证namespace；其他namespace密码/回执不可越权读取 |
| 多Bridge/命名实例/重复token | owner隔离、稳定principal、重复映射明拒；源BLE地址变化不误认 |
| invalid token/MAC/nonce、撤销绑定、冲突 | 两通道同样拒绝，不能换通道绕过；没有新副作用 |
| 错误密码/AP/DHCP | 有限真实失败原因，保存与启用分开，不以连接断开判断已保存失败 |
| OTA对照 | BLE OFF/已缓存设备操作token时HTTP OTA成功，运行hash核验，普通业务凭据不得OTA |
| 生产UI及资源 | JS/WK窄宽实际点击链通过；Fake调用真实共享处理器；实体USB/电池独立验收及持久/RAM/栈预算通过 |

共享单元测试证明处理器等价；真实client+Fake证明完整业务往返；原生WK仅证明生产页面与合成IPC；最后必须分别拿真实USB常驻及电池设备执行。三者不能互相替代。当前f287ac24cd546159只作为现场未完成任务，设计阶段不改写、取消或重发。

## 9. 设计自审

- USB/BOOT/Bridge激活的presence与通知/认证确认已统一；BLE在线与HTTP就绪分开，任务和在线机会分开。
- 用户HTTP优先建议已落实为认证/api/status可用性判断；USB continuous避免remaining0误挡，设备提交时重验避免状态竞态。
- 去掉的是对BLE临时许可的业务依赖；绑定、owner、设备操作token及电池窗口约束保留，HTTP不是额外旁路。
- 稳定调用者与旧namespace关联已纳入迁移；跨通道回执查找不能依赖客户端result_peer。
- Astra medium审核提出的身份多实例、异步原子终态/试连未知、跨传输生命周期及配置容量条款已修订；BOOT已由用户选定严格权限，设计选择已收口；代码实现与软件/实体验收仍未完成。
- 已知工程风险集中在身份迁移、HTTP切网后确认及回执容量；这些列为实现/验收门槛，尚不宣称设计已实机通过。


## 10. U0 回执退役与永久防重放（实施合同）

先于U1–U6执行。现行任意request_id的8项永久账本不能直接删：删除后即失去幂等证据；无限任意ID不能用有限精确集合永久记录。

新事务ID为`w3_`+16位小写十六进制单调序号（非零），Bridge在同一持久作业创建提交中分配，重试/跨通道不更改。设备按principal+namespace保留最大已接纳序号，最多8个游标且不LRU淘汰；新序号必须大于游标，允许跳号。配置/拒绝结果/异步pending与游标同一次WFS4原子写入，回执仍最多8项。相同ID先查询现存回执并核对业务指纹；低于或等于游标且没有回执返回retired，不执行，也不声称此前成功。seq耗尽/游标容量/回执容量在副作用前明拒。

`result_ack(query_id)`只由Bridge已持久确认原终态后发送，可跨HTTP/BLE；删除具体终态回执的同一次提交保留高水位。重复ACK返回retired；ACK读回失败属于storage_unknown，重启重读，不回滚游标。未确认/pending回执不自动删除，满8明拒。其他principal/namespace不可确认回执。确认不递增配置revision、不RF、不续窗。

兼容：旧任意ID回执保留，result_ack返回legacy_receipt_retained，不删除；现存未发送旧作业保留原ID，可一次写入旧账本并永久占一项。已发送旧作业只查询原回执，不换ID/盲写。旧ID数量受原8项上限限制，不能假装已实现其无限退役。所有新作业使用序号ID；两适配器均使用共享账本。设备高水位不得在重绑定/撤销/更换endpoint时删除；恢复旧Bridge备份不能自动改已发事务ID，序号冲突明拒并要求状态修复。

格式WFS4在原store3原子blob位置保留WFS3读取；显式版本、长度、条目数、游标数、CRC；游标变长存principal/namespace+u64，不复制密码/token。新格式读回成功才退役旧键。掉电只允许旧blob或完整新blob，损坏明拒/保留原字节。软件必须覆盖提交前后/确认前后/读回失败/重启、旧ID保留、新ID低高水位重放、相同ID不同载荷、8项容量和跨namespace。
