# BLE Wi-Fi 实施合同与 R0 记录

存储/本地写入口的2026-10-04新实现与候选边界见 [wifi-variable-storage.md](wifi-variable-storage.md)：WFS3与按需RAM软件验证完成，AP/USB认证事务已收归；.55已提交并OTA部署，真实WFS3迁移/后续启动读取/运行镜像已确认，旧兼容读取保留。此前“变长/入口未实施”属于阶段记录。
## 2026-10-04 当前会合 Wi-Fi 合同（已实施并配套部署）

权威实施步骤、软件验收及运行镜像证据见 [实施及验收记录](wifi-rendezvous-implementation.md)。Bridge与Note4 .54已配套部署。本节覆盖下文历史 BOOT-only、重复读取/读回确认和 8s/20s 预算条款；历史审计证据保留，不作为现行限制。

- 普通已绑定/加密且认证的 Bridge 在 owner 允许、MAC/会话 nonce 正确时，借用已有 formal light 或 sync 窗口写本绑定配置；BOOT 仍只用于首次绑定、物理救援和旧槽迁移。许可截止取已准入 RF/业务窗口的较短期限，不申请/续租。物理救援保持原固定截止与独立 bond 授权。
- `wifi_save` BLE 支持 1–4 条冻结整行变更，业务载荷最多4096B、MTU分片，最终 ACK/INFO 仍≤512B；和 HTTP apply 共用原子 NVS commit、expected_revision、安装身份+稳定 request_id 与持久回执。回执含提交 revision/bond，不含密码。结果查询按本绑定隔离，ACK 丢失只查原事务，未知不会盲重写。
- 同步使用读取过一次的完整缓存和冻结修订，不在写前/写后再取整份配置。设备以 CRC/逐字节读回验证 NVS 再返回提交回执，Bridge 按冻结变更投影更新缓存和基线，保留未选项和较新的本地编辑。旧部分字段 BLE 作业暂停为 whole_network_required，需重新整行选择；不暗扩大范围。完整读取仍仅显式读任务。
- 首次会合先有界认证 HTTP；失败才 BLE wifi_match 最小诊断。正常成功路径无额外成功状态查询；用户显式“使用当前网络”或配置意图仍可核对版本。AUTH_FAIL 的报告绑定实际尝试 SSID/保存版本，不依赖断网后空 SSID；同配置认证/安全失败停止同样的自动重写/激活，其他原因有限退避。Bridge 无唯一当前环境映射先提示本地配置；缺项/不同先 BLE 保存再启用，已保存未活动只启用。设备已入网但 HTTP 不通单独 bridge_unreachable；401/身份错独立，实际认证 HTTP 恢复清旧错误。
- Bridge/设备 HTTP 交接硬上限统一30s，设备候选关联+DHCP至多25s且不超已有截止，Bridge所有配置/激活/交接子步骤共享 Plan 发出时锚定的绝对期限，不逐步重开窗口。HTTP ready 须有非零IP且候选持久启用完成，旧网络HTTP不能完成新激活；未知Plan仅查询已发事务，不发新写入。正常成功必须 HTTP 证明及同 wake/session/plan/owner handoff 后关闭 BLE，再允许 light-sleep；失败有界退出。冻结准入读集合仍在诊断 complete 前执行，close仍先释放两锁且限2s。
- **仅页面手动同步**检查缓存合并结果保留当前 host_ssid→设备2.4GHz 映射；允许删除旧活动保存项，但须有替代保存项，现场活动副本保留至显式启用。预览冻结主机SSID、local_revision、snapshot_id与所选项，提交由服务再次核对；没有当前项提示“请保留或加入当前网络后再同步”。不自动补选/切网/试连，不影响自动恢复、程序化MCP同步、离线保存或缓存导入。

以下旧阶段的“未实施/未部署”仅为审核当时记录，落实结果以本节为准。

系统权限/真实RF连续交汇、错误密码/AP/DHCP矩阵、变长存储与 AP/USB 旧写入口仍以 backlog B6 记录；模拟与部署不能替代这些实机证据。

## 2026-10-04 页面手动 Bridge→设备同步的当前网络约束（审核修订，未实施）

用户明确本项仅指比较与同步页面手动点击“Bridge→设备同步”，不是前述自动配网/自动同步方案。允许勾选删除设备原有方案，但提交前必须检查本次变更合并到缓存设备方案后的结果，仍包含当前网络环境对应的一条完整配置；未选中项按现有语义保留，不因本约束隐式扩大选择或整库覆盖。当前环境沿用主机host_ssid到设备2.4GHz SSID的映射，允许两侧SSID不同。

- 检查发生在预览/提交：目标项不能标记删除；新增或替换须具备所需配置，保留设备已有项可沿用现有凭据；没有当前项则阻止提交并提示“请保留或加入当前网络后再同步”。主机网络/设备缓存未知时明确无法确认，不猜测当前项存在。
- 预览冻结当前环境、所选项、local_revision与snapshot_id；提交时仅从缓存检查这些条件没有变化，不为页面校验另读设备配置。Bridge换网或草稿/快照变化须重新预览。
- 此约束不限制Bridge离线编辑/标记删除/本地保存，不改变设备→Bridge缓存导入，不自动选中当前项或补写未批准方案；用户自行调整本次选择。
- 撤回上一条回复中为此约束附加的“先自动切换、确认替代联网后才能删除”表述：本项是页面手动同步的结果集合校验，不新增自动切网/试连步骤，也不改前面的自动配网规则。设备已有版本/原子事务/活动配置保护仍沿用，其变更需另明确。

验收须覆盖：删除其他项且当前项保留可提交；删除所有当前项且没有替代则拒绝；同批加入完整当前项满足集合条件；只保存Bridge草稿/缓存导入/自动配网入口不受本项校验影响；预览后换网/版本变化拒绝旧预览。当前仅记录方案，没有修改或部署实现。

## 2026-10-04 协议审核补充：HTTP不可达的配置/启用/连接分支（尚未实施）

此判断仅在首次交汇认证HTTP无法访问时进入；正常HTTP通信继续同步，无额外成功报告。先定位Bridge当前主机网络对应的保存方案，再经本次BLE诊断核对设备保存项/版本、活动项/版本、实际尝试或连接网络以及有限错误。配置比较与活动网络比较独立，不能把“已保存”当成“已切换”。

| 判断顺序/条件 | 协议处理 | Bridge页面 |
|---|---|---|
| Bridge没有当前环境的可用配置（含缺必要密码） | 先提示在Bridge补全，配置前不写设备、不猜网络；可使用已有完整缓存，不另读配置 | 请先配置Bridge当前网络 |
| Bridge有配置，设备尚未保存本环境项 | 本次合法有界交汇通过BLE保存Bridge整条缓存配置，持久回执确认后启用 | 等待同步并启用当前网络 |
| 设备已有一致配置，但活动项/版本仍属于旧网络或旧版本 | 只启用已保存的对应ID/版本，不重复写配置 | 等待设备切换到当前网络 |
| 设备保存项与Bridge不同 | 通过BLE同步当前Bridge方案，确认后按本次当前网络意图启用 | 等待更新并启用当前网络 |
| 一致方案已经启用，设备明确报告认证/安全模式错误 | 提示修正Bridge配置；保留错误与关联版本，不把同一错误方案反复写入/启用 | 设备认证失败，请核对Bridge密码/安全模式 |
| 一致方案已经启用，但网络未发现、信号弱、DHCP失败或仅超时 | 显示实际类别，采用既有有界重试；不直接断言Bridge密码错误 | 展示对应设备连接原因 |
| 设备已连接预期网络且取得IP，Bridge仍无法访问HTTP | 使用BLE报告的候选IP进行有界身份认证；认证通过才更新登记地址，否则展示访问/认证/身份问题 | 设备已连接当前网络，但Bridge无法访问设备 |

“当前环境”按已有host_ssid→设备2.4GHz SSID映射判断，允许主机/设备SSID不同；主机权限不足、重复映射或设备信息不足时显示未知，不跳到相同/未配置结论。错误需关联实际尝试项/配置版本，新Bridge编辑或新尝试不能被旧错误覆盖。保存成功、启用成功和HTTP访问成功分别确认；配置作业回执不以HTTP可达作为提交前置条件。已绑定Bridge的日常写入应在既有合法会合窗口完成，不额外要求BOOT，BOOT仍承担离线救援/首次绑定。

本节为用户审核修订，未改变生产实现；与前一节按需报告规则共同生效。待补实现仍归backlog B6。

## 2026-10-04 协议审核修订：失败报告与首次交汇按需诊断（尚未实施）

用户收敛：设备Wi-Fi密码不对/认证失败等明确连接错误应经BLE报告；正常连接不新增成功上报。Bridge首次交汇先尝试认证HTTP，无法访问时才在同次BLE会合同步最小连接诊断；当前网络相同仍不可达必须在Bridge页展示。此处为审核方案，未修改/部署代码。

1. 设备仅在连接失败原因发生/变化时提供有限错误报告，绑定设备MAC、目标网络ID/SSID、配置版本、本次唤醒/连接尝试；不携带密码或整套配置。认证失败提示核对密码/安全模式，单纯超时不断言密码错误。普通失败路径不能依赖WiFi.SSID()非空，否则认证失败清空SSID会吞掉原因。
2. Bridge建立/恢复通信的首次交汇，先进行一次有界认证HTTP尝试，沿用设备发现IP/身份校验；正常成功以实际HTTP通信为证据，不增加成功BLE报告或整份配置读取。HTTP失败后，本次BLE链路查询一次最小诊断：实际/尝试连接的SSID、是否关联/取得IP、已获IP、有限失败原因及关联版本；连接正常时这仅是按需回复。
3. 无设备配置→查询Bridge当前主机SSID对应的完整保存方案，缺方案/密码时提示配置；与当前映射不同→展示实际/目标网络并走BLE配置任务。比较按host_ssid→设备2.4GHz SSID映射，不能强求主机5GHz与设备SSID字符串相同。主机SSID权限未知则说明无法比较，不猜测网络相同。
4. 同目标网络且设备未连接→页面显示设备报告原因；同目标网络且设备已入网但认证HTTP仍不可达→显示设备已连接该网络、Bridge无法访问设备（携诊断IP/观察时间），不能据此判密码错误。HTTP401/403或MAC不符分别显示认证/身份问题，不归为网络不可达。候选IP仅在身份认证通过后更新登记，不盲信BLE地址。
5. 后续正常通信不轮询成功状态。新故障/用户重试/通信恢复再更新诊断；实际认证HTTP恢复可清除旧不可达提示。迟到的旧任务/MAC/网络版本结果不能覆盖当前状态；有界诊断不延期PowerPlan或关闭BLE前的交接期限。

实施缺口：当前prepare_connection/automatic_on_link/refresh_connection会读取wifi_match成功状态；HTTP不可达缺少单独页面状态，且设备普通失败投影受WiFi.SSID()==配置SSID限制。应在正常成功路径省去仅用于成功汇报的查询，仍保留确有配置任务的版本核对；本次不凭方案修订宣称已修复。与日常会合写入仍被BOOT阻挡、普通HTTP同步缺BLE回退、重复配置读回、统一窗口预算及RF验收共同归backlog B6。

## 2026-10-04 密码配置修订（覆盖下文旧密码限制）

Bridge网络配置可持久保存可选password（None为旧快照未知，空字符串为开放网络），编辑框眼睛按钮显示/隐藏。首次显式认证完整读取包含本机绑定密码，其他绑定及legacy不泄露；缓存导入不联系设备，无独立补读密码协议。普通ACK/result不返回密码；本机绑定wifi_scheme_get作为配置读取包含密码。wifi_match增加可选credential_key=SHA256(password UTF-8)，只核对已有配置而不读取配置；密码不同不冒充元数据一致。MAC/bond/owner/物理授权、512B分页与原子回执合同保持。密码不进仓库或日志，配置文件Unix权限0600，无钥匙串步骤。


最新设计入口（2026-10-03）：用户要求重新设计 Wi-Fi 全量方案管理与同步，见 [wifi-redesign.md](wifi-redesign.md)。当前 protocol=1 实现与下方旧规划不等于新设计已完成；新稿覆盖单方案管理/试连即保存启用的旧交互。Note4 试连跳屏异常待诊断，最新产物/未部署改动见 PROGRESS 顶节。


2026-10-03，按 remaining-plan R0–R4 执行；待办仍仅在 backlog B5/B6。本次不提交。

## 2026-10-04 当前网络单条协议（已随 .52 配套部署）

- 加密绑定/精确MAC的Template Control复用`wifi_match`：network_id、metadata_key、open。metadata_key是SHA256(host_ssid UTF-8+NUL+ssid UTF-8+NUL+label UTF-8+NUL+开放意图1byte)，不含密码。回复ok/matches/active/connected/version/stage与有限error/reason；只返回当前bond的单条匹配结果，不依赖HTTP全量快照。
- INFO用紧凑wc=1广告此能力；没有wc不得猜测旧固件支持。最终原生INFO/ACK仍≤512B，完整身份/request/wake关联不裁剪。
- Bridge只选择保存targets中与当前真实SSID唯一匹配的本机网络；未知/权限不足/有线/重复匹配均不猜测。匹配保存版本直接wifi_activate，配置不一致或新密码意图复用wifi_save稳定事务/结果查询/读回，物理BOOT授权、owner与版本约束保持。只有写回执+读回确认才消费密码替换意图，新本地编辑不因此消失。
- 普通wifi_activate在BLE仍连接时启动Wi-Fi，取得非零IP后才成功；不再等断开才能开始关联。普通会合通过既有同session/wake/plan的认证HTTP证明与handoff后关闭BLE，随后允许light。关联/DHCP总限25s且不超过已存在的物理/正式期限，失败恢复旧活动配置。
- 反馈与网络ID/版本关联：authentication_failed、handshake_timeout、network_not_found、security_mismatch、signal_weak、association_failed、dhcp_timeout、connect_timeout等有限类别；普通当前活动网络也报告已采样的明确断链原因。超时不直接判密码错误；Bridge最多3次自动激活、间隔≥60s，认证/安全拒绝等待用户修改。无密码回读、落盘、日志。
- UI整条方案选择与历史折叠已部署；设备→Bridge继续纯缓存。真实主机SSID权限/BLE配网/多次断网与错误密码矩阵仍未验，软件和镜像验收不能替代RF现场。以下R0/早期合同为历史阶段，其“断开后试连”被本节替代。

## R0 现场

Git 工作树在接续时干净，已实现基线在当前 HEAD，保留全部文件。运行 PID 98240 的路径是 `artifacts/bridge-macos/Codex Status Bridge.app/Contents/MacOS/bridge-app`，8765/8766 正常监听。AppSupport state 存在，唯一登记设备 `7C4FADB93408`，IP `192.168.10.101`，数据许可 true。应用 72,672,496 B / SHA256 `E16732894CEDA38BEE162996A2807A9FC7551496BBA15E2558BAF75EBA2D892E`；恢复 ROM 1,665,024 B / SHA256 `9B87601138B68047F0AE71A46486944E2050E8A40651453D613C1E953D8B4947`。只读 HTTP 核对 FW `0.18.41-note4-b-fields1`、完整 MAC、connected/light/ext1、电池供电、BLE 关闭；未开串口。

用户已执行一次 BOOT。当时 Wi-Fi 在线，因此不是离线失败复现。离线阻断仍为源码基线：handleBootClick 的 !wifiUp return，startup 失败直接睡眠，AP 提前跳过按钮。真实断网/电池/无 Bundle/深睡矩阵留待候选 ROM 验证，不能记为已通过。

## 固定恢复窗口（R1）

BOOT 短击：未处于物理恢复时开启恢复（即使普通 BLE 已开启），恢复中再次短击关闭。窗口固定 120s，由物理动作计时，连接/命令/传输不续期；USB 自动 BLE 不延长恢复授权。EXT1 的 GPIO0 唤醒直接开启恢复，PWR 唤醒只保留原 300s provisional；timer 不开启恢复。AP、无凭据、无 Bundle 同样支持。2s 切模板、15s AP、30s factory reset 保留。

恢复独立于正式 PowerPlan，不改其截止、owner、context 或数据联系时间；恢复期间 Wi-Fi 失败/业务截止不能关 BLE/睡眠。窗口到期先取消未完成配网，再恢复原电源策略；低电关机优先。配网异步试连，不能阻塞按键/队列服务。

## wifi_setup_protocol = 1（R2–R4）

复用加密 Template Control / Status UUID，不增加 GATT。每条请求 <=1024 UTF-8 bytes，普通命令仍保留原 8192 上限。共同参数 op/request_id/device_mac，request_id 为 1–64 ASCII ID 字符。回复 ack=wifi、request_id、device_mac、ok、stage、error、network_id、ip、persisted；SSID 只在摘要出现，密码不回读、不日志。NimBLE/ATT 属性值最大 512 bytes，不能扩大属性绕过；`wifi_scheme_get` / `wifi_capabilities` 用冻结快照分页：offset=0 创建本连接快照，后续 offset+snapshot(CRC32) 读取，chunk 每页最多 120 raw bytes 的 hex，next_offset/total 有界（总 <=4096），身份/generation/op/snapshot 不符拒绝。Bridge 重组后解析，不依赖通知 MTU。INFO 保持紧凑，完整硬件能力另经分页读取。

归属是连接的 **peer identity address + address type**，使用 NimBLE 的 peer_id_addr（不使用随机连接地址、请求自报 bond 或 Bridge 实例 ID）。队列携带连接身份与 generation，断开重连不能消费旧请求。最多 3 个 bond（与当前 SDK 一致）、每 bond 4 网络、总 12；网络 ID 为该 bond 内 1–32 ASCII ID，SSID/host_ssid 为 1–32 UTF-8 bytes，密码为空或 8–63 bytes / 64 hex。SSID 原始字节不做显示净化。相同 host_ssid 在本 bond 只映射一个 network_id。

| op | 权限与行为 |
|---|---|
| wifi_scheme_get | 已绑定加密、精确 MAC；分页返回本 bond 无密码网络摘要、选中 ID、容量及物理窗口剩余时间 |
| wifi_capabilities | 已绑定加密、精确 MAC；分页返回设备实际硬件能力，用于离线登记，不根据 BLE 名称猜硬件 |
| wifi_setup | 另需物理恢复；network_id/host_ssid/ssid/password/replace；新增不得覆盖，已存在需 replace=true；候选仅在 RAM，异步试连，获得 IP 后原子提交，再报 persisted |
| wifi_setup_status | 已绑定加密；query_id 查询本 bond 的最近事务；通知丢失/重连可查；其他 bond 仅返回 not_found |
| wifi_setup_cancel | 已绑定加密、同 bond/query_id；取消仅在途事务，释放暂存，恢复旧连接；已提交不能回滚 |
| wifi_scheme_delete | 物理恢复、显式 network_id；原子删除，选中项删除后回退 legacy，不覆盖其他 bond |
| wifi_select | 已绑定加密、本 bond 网络 ID；需 bridge_id + endpoint token，与当前有效 owner 匹配，空闲允许（但不创建 owner）；正式 light 或物理恢复才试连，否则返回 window_required，Bridge 等后续正式窗口；只传网络 ID。普通会合只接受选择，不延长 BLE 截止；连接关闭后在正式 light Wi-Fi 窗口试连，IP 成功才提交。物理恢复内则立即异步试连；新 owner 与候选选择冲突时取消 |

单设备最多一项在途事务；相同 bond/request_id/op 重放返回原结果，不重新连接/提交；同 ID 不同内容拒绝 request_conflict。在途另一请求 busy。最近结果直到下一事务/重启；重启通过 scheme_get 核对已提交记录，不能推定 RAM 事务继续。查询/取消 request_id 与原 query_id 分开。

错误码：shape/mac/unauthorized/physical_required/occupied/window_required/not_found/exists/mapping_conflict/capacity/busy/request_conflict/cancelled/session_expired/disconnected/connect_timeout/storage。关联+DHCP 总超时 25s 且不超过恢复/正式窗口；失败/取消恢复旧网络，不修改旧记录。断开取消候选并清暂存；无成功 IP 不写凭据。

所有方案、selected bond/ID、schema version 在 NVS `wifi-bonds` 的一个 blob 内提交（单次 nvs_set_blob + commit，写后读回核对）；最大 8192 bytes，解析/大小错误失败关闭，不静默清库。NVS 原子记录恢复保证旧/新完整 blob，不假称多 key 原子；真实断电矩阵仍需实机。成功保存后才变更 RAM 当前方案。USB/AP legacy `wifi` namespace 原样保留，启动优先选中 bond 网络失败再回退旧网络。解绑保留孤立方案但不可访问、不得自动继承；容量满不回收孤立方案，显式 factory reset 清全部。重新绑定同 identity 视为同一主机（系统新 IRK 不改变已认证 identity 归属），首版不提供跨 bond 清理。

Bridge 不保存方案摘要/SSID/凭据到 state，不通过 MCP 通用参数审计传密码；UI 直接调用共用 application service。密码完成/取消/失败清理输入，不回显。动态读取主机 SSID，明确权限/有线/无映射状态；自动选网只传 network_id，变化合并，检查当前 owner，deep 等下一会合。macOS/Windows 使用平台原生读取，不猜网关/SSID。

## 实施和验证记录

R0 合同已写；R1–R4 按本合同实现后更新记录。所有设备写入前保留恢复 ROM 与 AppSupport 数据备份；当前未写网络、重配/轮换 token、部署或发布 Bundle。实体矩阵、第二 bond、Windows、另一架构、24h 与耗电证据不由软件测试替代。

R1/R2/R3 已完成第一版源码与软件验证；R4 已有 CoreWLAN/CoreLocation + Windows WLAN 原生读取、权限状态与已有映射选择，普通会合的延后结果和真换网仍须验证。R5 加入 macOS 邻居表解析/UDP 触发回退，HTTP 仍核对完整 MAC；R6 下一步按需 LaunchAgent + 同 exe 子进程监督，正常退出不拉回、5 分钟 3 次异常熔断，命名实例与数据目录隔离。R7 先完成可用平台本地回归/包与恢复记录，其余架构/Windows/长时不冒充已验。当前候选仍未部署。


## BLE wire v1（2026-10-03 软件候选；未构建/部署 ROM）

后续部署更新：用户追加刷机后，Note4 .44与配套Bridge已部署并通过运行镜像哈希、一次真实status/plan/sync_open ACK确认；本节标题保留合同定稿时点，当前现场见PROGRESS顶节。连续重连仍有超时，完整Wi-Fi同步/P3未因此实现。

这是 Fake BLE 真实性方案 P0 的确定合同。新 ROM 的 INFO 明示 `ble_wire=1`，新 Bridge 先解码；业务 HTTP 协议、GATT UUID、Wi-Fi 事务 protocol=1 均不重命名。部署必须成对包含 ROM 与 Bridge 改动。未报告能力的 .42/.43 沿原字段解码及有界等待，不将其当作支持新摘要；未知 wire 版本拒用。

所有特征值最终 UTF-8 编码 <=512 bytes。请求仍按 180-byte 片段发送，普通总长8192、Wi-Fi 1024；片段/总长与回复属性容量分开。JSON 完整即派发，断开清缓冲；Fake offset/final 仅为本地测试载体字段。

- INFO：`schema/model/ble_wire/f/mac/ip/http_port/rendezvous/wifi_setup_protocol/wake_generation/wake_seq/pairingWindow/peerBonded/peerEncrypted/bundle_configured`；`f` 为原 FW 字符串（最多64 UTF-8 bytes）的 STANDARD base64（带 padding）。`rf_remaining_ms` 可选，为读取时本物理维护/自然会合剩余毫秒；无字段代表未报告边界。诊断 stage/cause 走认证 HTTP。完整 MAC 为17字符冒号十六进制地址；IP最长15字节；没有删除/同步含义。
- command/wifi 信封：`ack`、`ble_wire:1`、`rid`、`w:[generation,seq]`。`rid` 是原 request_id（1–64 UTF-8 bytes，拒 NUL/非法UTF-8）的 STANDARD base64；不裁剪或重命名业务事务。两个 wake counter 为 uint32。普通 ACK 的 `active_context_id` 以 `ctx` base64 原32字节表示，其他确认字段保持；BLE 不放 fw_target 或 owner 的显示/主机诊断对象。
- status：另含 `op:"status"/device_mac/s`；固定16项的 `s` 顺序如下。解码必须检查项数、整数范围、布尔类型、标识符字节及32位hex nonce，再投影为原内部字段；仅合法摘要生成 `result=applied`。
- AUTH/endpoint：沿原 `ack/ok/token`（token仅AUTH成功），调用同一生产构建器和最终属性门槛。它们没有 request_id/wake 关联能力；依赖已核对 MAC、已绑定加密连接，不能冒称有事务结果查询。AUTH读属性也可完成，丢通知不等于丢 ACK。
- Wi-Fi：维护分页主体每页最多80 raw bytes的hex（160字符），为新增最终信封留空间；每页含 request/wake 后校验。Bridge接受旧页最多120 raw bytes。空/过期/越界拒绝，不完整页不替换旧完整快照。当前事务错误 op 归类为 `wifi`，不回显任意输入操作名；其他字段仍遵守 Wi-Fi protocol=1。
- history：最终信封加入后按512裁去完整 records，`more=true`；单条装不下可返回空页，Bridge既有无进展/页数/截止保护停止本次读取。详细历史仍不扩大短会合窗口。

| s索引 | 内部字段 / wire 类型 | 上限 |
|---|---|---|
| 0 | session_nonce / hex string | 32 ASCII bytes |
| 1 | active_context_id / base64 | 原32 UTF-8 bytes |
| 2 | active_template_id / base64 | 原16 UTF-8 bytes |
| 3 | committed_job_id / base64 | 原16 UTF-8 bytes |
| 4 | applied_seq / uint64 | UINT64_MAX |
| 5 | power.mode / bool（true=light） | bool |
| 6 | power.plan_id / uint64 | UINT64_MAX |
| 7 | power.remaining_s / uint32 | UINT32_MAX |
| 8 | power.provisional_remaining_s / uint32 | UINT32_MAX |
| 9 | sync可用（内部v=1） / bool | false时不伪造sync对象 |
| 10 | sync.enabled / bool | bool |
| 11 | sync.rounds / uint8 | 255 |
| 12 | sync.due / bool | bool |
| 13 | sync.retry_skip / uint8 | 255 |
| 14 | sync.pending / bool | bool |
| 15 | commit_seq / uint32 | UINT32_MAX |

Bridge每条 command/AUTH/endpoint从写入前建立固定截止，取5s与该连接RF截止的较早者；旧/空/错类别、ID、MAC、nonce或wake不匹配的属性不重置截止。旧帧缺wake不伪报已确认；新wire命令必须报告wake。普通新查询仍自动生成ID，持久作业可调用 `command_with_id` 保留原身份；该接口本身不提供跨重启幂等或查询能力，下一会合确认/重试策略仍归 P3。

`bleBuildReply` 超限不能报 rejected：有合法关联时返回短 `result=unknown/error=reply_too_large`；非法关联/不能编码不写新ACK。NimBLE写前门槛保留旧属性，写后比对长度和完整bytes，失败不notify；依赖原行为模型则由Fake专用故障模拟为先清空再失败。编码/写入日志只含已知op类别、关联CRC、最终/回复字节数、上限、wake、阶段与结果，不写原帧/SSID/密码/token。

P0预算证据：生产宿主构建函数最大字段样本（64字节ID，最长context/template/job/fw，最大wake/seq/plan，控制字符、反斜线与多字节UTF-8）INFO415、status452、普通ACK最大419 bytes；false比true多1字节，INFO保守419、status保守458，仍有至少54字节空间。实际Wi-Fi状态机最终页/结果样本最大418 bytes。属性511/512通过、513拒绝；635故障帧是同长度合成值。软件证据与未验范围见 fidelity plan §11。

## W0: Wi-Fi management v2 合同（2026-10-03，源码/软件候选，未部署）

INFO/状态独立报告聚合版本 `wifi_management=2`，该版本合同包含 full_snapshot、atomic_changes、durable_results、trial_restore、explicit_activate；这些能力名不是另行上报的布尔字段。旧 protocol=1 保留兼容，不能用于新版完整读取或同步；缺能力明确 unsupported。

| 请求 | 合同 |
|---|---|
| POST /api/wifi/snapshot | endpoint token；bridge_id；首次无 snapshot_id/offset=0，冻结全部 bond 与 legacy 无密码投影。后页携 snapshot_id/offset；30s TTL；schema=2/MAC/revision/complete/next_offset；仅完整且同快照可替换缓存 |
| BLE wifi_authorize | 已绑定加密、精确 MAC、bridge_id，物理 BOOT 原截止内生成 RAM 许可；绑定设备/bond/安装身份、操作范围；不能从 HTTP 请求自报 bond；重新连接不延长截止 |
| POST /api/wifi/apply | endpoint token + 许可；expected_revision、稳定 request_id、changes；至多4个本 bond 项。整批验证再单 blob 提交；保存不启用；活动网络独立版本保留 |
| POST /api/wifi/result | endpoint token + bridge_id/request_id；结果 committed(revision)/rejected/unknown；按安装身份隔离，未知必须回读，不能重写猜结果 |
| BLE wifi_trial / wifi_activate | 候选 RAM 临时试连与已保存 ID/version 显式启用；物理/正式窗口有界，启用另校验 owner；只有 IP 成功才更改活动引用 |

结果和配置同一 blob 原子提交，8项账本；已提交项保留至少24h（设备单调时钟跨启动不用于删除）；账本满不静默挤掉，拒绝 result_capacity。相同身份/ID/业务内容复用结果；不同内容 request_conflict；认证封装和 expected_revision 不进入业务身份摘要。普通 Bridge 状态只存非秘密意图与结构化错误，不存 token、密码或密码摘要。

Bridge 读/配置同步任务有效期24h，虚拟时钟可验；5/15/60s只约束真实会合重试的最早时间，不创建轮询/唤醒。取消/过期保留 may_have_committed。试连只在当前机会运行，启用授权截止不超过原窗口。缺维护许可 waiting_authorization、缺输入 waiting_input、修订目标变化 conflict、传输临时失败 waiting_rendezvous、发送后不确定 awaiting_confirmation、旧能力 unsupported（终态）；MAC/分页错配不更新旧快照。

存储迁移读取原 v1 blob/legacy，写新版另保留活动凭据版本；损坏拒绝加载/修改且不清库。HTTP读取不claim/续 RF/改PowerPlan；完整读取只借已有正式Wi-Fi/维护窗口。上述是实现合同，未实现能力不得上报；W1仅本地模型/读意图不冒充W2设备能力。


W0–W6软件交付证据见 [执行文档第11节](wifi-rendezvous-execution.md#11-w0w6-软件交付与验收对账2026-10-03未部署)。HTTP页每页至多768 raw UTF-8 bytes，服务端保留字符边界；业务总快照有16KiB上界，Bridge只有完整/schema/MAC/revision/snapshot/offset全部一致才替换。读取只用RAM许可标本bond可写，跨bond及legacy始终只读；没有有效本bond读许可也可认证查看全部只读表。

Bridge UI/MCP save/sync/import/read_import携本地local_revision，预览后变更在同一原子store锁内拒绝。刷新后导入保留批准字段；新项必须批准全部四个元数据字段，不能用部分字段选择隐式扩大为整项。现有项凭据变更另需明确credential字段，秘密只在UI内存补充，可按任务多项分次提供。未知事务只能确认可读字段目标，password/open凭据变化在无持久committed回执时仍awaiting_confirmation。确认rejected即结束且不重apply。

账本当前永久保留8项（包括持久业务拒绝），没有自动清理协议；满后明确result_capacity，避免未知事务被淘汰。24h是任务默认有效期/最小保留承诺，不代表设备能按跨重启单调时间安全淘汰。这个限制必须在长期管理放行前评估，不用后台删除绕过。

## 显式旧槽迁移（2026-10-04）

`wifi_migrate` 通过当前加密绑定的 BLE 命令入口，要求 endpoint 认证、owner 匹配及当前物理维护窗口。只读取设备自己的旧 wifi NVS，不接收/回传密码。全槽一次提交到现有 v2 CRC blob 并读回，稳定 ID `wifi-slot-<原槽号>`、当前 bond、迁移 label；保留原 SSID/密码和当前 active（没有新 active 时采用当前连接/旧 last）。不调用 WiFi.begin，不延长 PowerPlan。容量、同 host 映射或同 ID 内容冲突整批拒绝。成功后 main 逐键退役旧 sN/pN；retired=false/error=retirement_pending 表示新数据已持久化而清理未完成，显式重试从 NVS 重新加载再核验。ACK 丢失不得宣称失败已回滚；Bridge报 migration_result_unknown_retry_same_device。MCP platform_wifi_action 的 migrate 操作只允许本地显式请求，不自动在读取时迁移。
