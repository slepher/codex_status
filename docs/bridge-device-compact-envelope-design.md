# Bridge—设备 48 B 信封设计

核对日期：2026-10-11，基线9c38e45。固定48 B外层信封已在当前固件及Bridge实现，Note4 .82现场见[PROGRESS](../PROGRESS.md)。请求、响应及报告共用布局；业务JSON仍保留共享处理器需要的身份字段，不能把设计中的去重目标写成已经完成。业务合同见[协议总览](bridge-device-protocol.md)，完整实机故障矩阵仍未结项。

## 1. 范围与分层

48 B 指应用消息的信封编码，不含业务载荷、HTTP 请求行/头、TLS、ATT 或 BLE 应用分片头。握手及认证凭据交换不受此上限约束。不宣称整个网络包小于 50 B。

业务层继续定义设备身份、owner、Data 顺序、PowerPlan、完整快照、幂等、持久回执与明确结果。下层负责认证会话绑定、编码、分片及重组。HTTP 可直接通信，不依赖 BLE；BLE 能独立完成的操作不启动 Wi-Fi 或 HTTP。

不使用 JSON 短键、十六进制字符串或 Base64 包装信封。48 B 经 Base64 后为 64 字节，不满足本目标。HTTP 消息体和 BLE 重组结果直接使用原始二进制；业务载荷可以暂时保留 UTF-8 JSON，后续是否修改载荷编码是独立决策。

## 2. 固定布局

多字节整数使用大端序；禁止按 C/C++ struct 的 sizeof、padding 或宿主内存布局序列化。

| 偏移 | 字节数 | 字段 | 定义 |
|---|---:|---|---|
| 0 | 1 | marker | 固定帧标记0xC1，仅校验唯一格式，不是协议版本协商 |
| 1 | 1 | kind | 0=request，1=response，2=report；其他值拒绝 |
| 2 | 2 | op | 操作编号，独立登记；不因 HTTP 路由不同改变含义 |
| 4 | 2 | result | request/report 必须为 0；response 使用下述结果码 |
| 6 | 2 | payload_len | 信封后业务载荷的字节数；不是字符数 |
| 8 | 16 | session_id | 认证会话的随机 128 位标识，原始字节 |
| 24 | 8 | request_no | 非零 u64 请求关联编号；response 原样回显；report 使用报告者的编号 |
| 32 | 16 | context_ref | 固定格式内完整 context 的 16 字节 ASCII；不涉及上下文的操作填全零 |
| **合计** | **48** | | 不设置可变扩展头 |

操作编号1–27的登记含义见[实施计划 P0](../project-workflow/bridge-macos/bridge-device-compact-protocol-implementation-plan.md#p0-冻结值2026-10-08)。管理凭据和业务事务不因编号统一而合并；编号登记不自动表示两载体均有该入口，实际路由见协议总览§5。编码源码为[固件](../src/v2_command_envelope.cpp)与[Bridge](../bridge/crates/core/src/compact_frame.rs)。

初始结果码：0=applied（读取为 completed），1=accepted（尚未完成），2=rejected，3=unknown。具体错误代码、失败阶段、实际 seq/plan_id、remaining_s 和恢复入口放在结果载荷中。不能只有一个 false，也不能把 accepted 当作最终成功。

## 3. 会话绑定与现存身份字段

会话建立时，下层验证完整 MAC、Bridge principal、当前设备启动 nonce、endpoint 凭据、载体认证信息和能力。HTTP认证 Status/OTA 开会话请求在session槽使用本次独有的16 B客户端打开标识，OTA打开还带固定载荷`{"open":true}`以区别设备重启后的旧服务端session；设备回传新生成的服务端session。同一打开标识在当前会话内重放仍受最高请求号和保留结果围栏约束。BLE Status使用零session，连接代次完成认证绑定。服务端记录完整身份及权限范围；session_id和打开标识都是关联标识，**不是认证凭据或消息签名**。

- BLE：会话绑定到实际绑定且加密的 peer、连接代次及 principal。断线或 peer/连接代次变化即失效，新连接重新认证，不复用旧连接的绑定。
- HTTP：每个请求继续使用现行所需的 Authorization/管理凭据；由认证身份检查 session_id 的绑定。单凭 IP、TCP 连接或 session_id 不能取得权限。本方案不新增 TLS，也不声称现行 HTTP 明文 bearer 具有加密或消息防篡改能力。
- 设备→Bridge报告：POST /api/device/session通过已有endpoint Bearer申请独立、短期且绑定MAC/boot/principal/instance的会话，随后POST /api/device/report发送kind=report、op20帧。共享Bearer收到的报告仍只构成hint，业务事实由Bridge认证回读设备/查询原事务确认；不继承BLE授权，也不因出站连接成功认定设备入站HTTP可达。
- 设备重启、凭据轮换/撤销或会话过期后失效。owner/lease 在每次执行时按现行规则检查，握手通过不等于永久占用；读写不隐式 claim，也不续电源期限。
- 会话状态按实际连接/会话分配，有界数量和有效期；无空槽预分配。核心固定标识需要 16 B session_id 加完整 nonce/身份及元数据，实际结构、峰值RAM与栈需按匹配版本证据核对，不能以 48 B wire 大小代替 RAM 预算。会话是临时 RAM 状态，不为每次握手写 NVS；设备重启统一作废。

当前实现增加frame会话校验，但HTTP和BLE的JSON业务载荷仍保留device_mac、bridge_id、request_id、session_nonce等字段，BLE还携token/op；尚未删除重复身份。客户端直接把该JSON放在48 B头后，共享业务处理器继续校验。HTTP认证头仍存在，设备operation token与endpoint token仍分开；48 B不代表整个认证开销或消息小于50 B。

## 4. 请求、上下文与重放

request_no 在每个方向独立单调递增、不回绕；首版每方向仅一个未决应用请求。response回显请求编号，不占对端请求槽；report使用自己的编号，业务载荷引用原请求/持久业务ID。收到任何明确响应（包括accepted）就结束该次消息交换并释放请求槽；异步业务以原业务ID继续，查询使用新消息编号。不能等业务最终完成才释放，报告接收回执同样处理。所有HTTP连接由同会话适配层串行，不能借TCP连接各自的顺序绕过此限制。

每方向保留最高已接纳编号和至多一份当前结果/内容摘要；旧结果接收确认后可退役。相同编号相同内容返回仍保留的结果，同号异内容拒绝；request_no <= highest_accepted时，仅返回仍保留的匹配结果，否则返回已退役/原业务查询入口，禁止再次执行。缓存字节上限、接收确认操作编号及超时值在实施P0冻结，不靠不限量回执保证正确性。查询用新查询编号引用原业务，不能创建新写事务；request_no不替代Data seq、Plan plan_id或Wi-Fi namespace/job_seq。

当前设备context为16个十六进制字符，在context_ref槽中原样存16 B ASCII，采用唯一格式，逐字节比较完整ID；不额外引入随机alias或映射仓库。不涉及context时填16个零字节（不是字符0）；需context的操作拒绝零槽。其他长度明确拒绝，不能截断；容量上限33字符不代表当前ID是128位。

激活后返回新完整context；旧context请求只可查询并返回标为历史的原结果，不能借重试重新应用到当前context。更换载体重新建立认证会话，旧BLE session_id+request_no可放在HTTP报告载荷作为历史关联，不提供授权；跨重启恢复仍使用原持久业务ID。

## 5. 编解码及错误处理

先检查至少 48 B、marker/kind/op/result 合法、载荷长度符合该操作及载体预算，再按需分配和解析载荷；按固定偏移逐字段读写。完整帧必须恰好是 48 + payload_len 字节；HTTP 不允许尾随字节，BLE 分片层只将完整帧交给解析器。65535 B 仅为编码最大长度，不扩大现有接收容量；普通 BLE 8192 B 和 Wi-Fi 4096 B 的迁移预算均按完整帧计算。

无法解析信封时返回明确的载体格式错误，不信任损坏请求中的关联号。能可靠提取请求编号且已认证时返回对应 rejected 及结构化原因；没有收到响应在有限超时后返回超时，确已发送而提交状态无法判定时返回 unknown 并查询原业务事务。

认证 HTTP Status 保留完整业务状态。JSON 不超过 4048 B 时直接返回；较大时按实际长度冻结一份快照，最大 16384 B，15 s 后释放，不写 NVS。首帧和续帧返回 `result/status_read/offset/total_bytes/chunk_hex/next_offset`：随机 32 位十六进制读标识，原始块最多 1800 B，以 hex 传输避免 UTF-8 截断，`next_offset=0` 表示完成。续读仍走 op1 `/api/status`，载荷为 `{"status_read":"…","offset":1800}`，精确绑定已认证 endpoint token；新首读替换旧快照，过期、读标识或偏移不符明确拒绝。Bridge 验证读标识、总长、偏移、块和最终完整 JSON，整个读取（含会话锁等待）共享一个期限，并按设备/token 串行读取。不得裁剪 `data_contract` 或诊断字段来冒充完整状态；超过总预算返回 `status_limit`，业务错误不可仅凭 HTTP200 当成有效 Status 或身份不匹配。

信封不额外添加 CRC 或 MAC。BLE 链路保护、HTTP 既有认证和业务完整性检查分别沿用；Data CRC、Bundle hash/CRC 等保留在载荷中，不能因删除信封重复字段而取消。若将来要求应用层抗篡改，应另立认证方案和字节预算，不能把随机 session_id 冒充 MAC。

## 6. HTTP / BLE 载体映射

HTTP业务路由统一POST并使用固定Content-Type application/vnd.codex-status.frame；路由必须与 op 一致，不一致明确拒绝。非 2xx 返回仍保留结构化错误；经过代理时不得误把 HTML 错误页解码为本信封。主动拉取可在认证请求/响应内承载该帧，空闲是否返回 204 或业务状态属于独立 exchange 协议，不由信封猜测。

BLE 分片层在协商 MTU 允许的容量内发送，不能固定假设 180 B 总能写入。当前业务分片按二进制定长头和长度重组，不依赖JSON括号判断结束；每方向一次只传一帧，有界超时清空未完成帧，碎片失败结束当前接收代次，禁止残片与下一请求拼接。

状态属性一次完整帧仍不超过 512 B，因此最多剩 **464 B 业务载荷**。大请求经应用分片可超过 512 B；大响应使用冻结快照分页或单独的分片通道，不能扩大单属性值。读取多段属性值期间保持同一快照，避免把不同结果拼成一个帧。

## 7. 已实施范围与验收边界

1. 一次性替换为最终frame编码、固定content type及编解码。Bridge与设备分别更新软件版本；不提供旧JSON兼容、协议版本历史、能力矩阵、双协议、猜测解码或降级重发。软件不匹配明确报错并提示更新，不绕过认证。
2. 会话绑定及完整context编码/校验已覆盖Status/Data/Plan；普通业务复用有效HTTP frame会话，Status显式读取会打开新会话。业务nonce/身份检查仍存在，失效后有界重建并查询原事务。
3. 原业务处理器接收解析后的完整身份/上下文，保留 owner、seq、plan_id、错误阶段、持久事务和未知结果合同。载体转换不创建新的业务事务，不自动换号重发可能已提交的操作。
4. 当前BLE客户端仍可在JSON载荷附校时字段；删除重复身份/改独立校时并未由固定头自动完成。业务载荷编码独立评估。Bundle/OTA/Activate的HTTP分配不变，multipart ROM上传不套此头。
5. 验收包括 48 B 精确布局与大端对拍，错误长度/错误帧标记，MAC/principal/credential 范围不符，重启/断线旧会话，owner 过期，上下文切换，同号异载荷、乱序和重放，丢 ACK 查询原事务，BLE 残片隔离、跨 Read Blob 快照冻结、512 B 边界及 RAM/栈预算。宿主尺寸计算不是固件/实机验收。

当前固定头、HTTP分页、会合报告已有软件测试和特定Note4部署证据，见[实施验收](../project-workflow/bridge-macos/protocol-implementation-20261011.md)。本次只是文档同步，不新运行测试或部署；普通BLE畸形/未授权/队列满时可能静默丢弃的边界、完整RF/掉电矩阵继续归backlog B6/C8。

## 8. 执行场景的独立入口

提前结束BLE、设备主动HTTP报告、900 s Status与Bridge请求分流的最终执行规则见 [场景文档 §6](bridge-device-communication-scenarios.md#6-提前结束-ble设备-http-报告的场景修订)。本文件只定义48 B编码及载体映射，不再维护第二份时序规范。report行为和48 B编解码可以分别开发验证，最终交付同一协议，不对外保留阶段性JSON兼容。

### 8.3.1 Bridge 请求设备数据

BLE可完整完成则交会内请求并取得结果；需要HTTP且不可用才通过BLE投递开网意图。完整执行与失败规则以场景§6.3.1为准；HTTP已可用直达。

### 8.3.2 Bridge 回报地址载荷

开网载荷带bridge_addr/bridge_port，路径由最终协议固定；IPv4原始地址+u16端口为6 B，不增加48 B信封。地址选择、实例、临时/持久地址生命周期以场景§6.3.2为准。

## 9. 用户约束：不维护协议版本历史

Bridge和设备分别更新各自软件版本即可。实施只交付当前最终协议，不设计旧设备兼容、协议版本协商/能力矩阵、双协议过渡、旧请求翻译或超时降级重发。软件不匹配明确失败并提示更新。固定marker、op和业务合同修订仍用于格式/操作/数据正确性校验，不承担历史协议兼容。认证、持久原事务恢复和BLE MTU协商属于运行正确性，不因取消版本兼容而删除。

### 2026-10-11 临时同步窗口修正（源码候选）

周期全量Data不授予正式Light。BLE小快照在会合内完成，大快照用既有sync_open新增reason=data/data_seq准入临时90秒WIFI_SYNC_ONCE；该窗口与正式Light绝对截止独立，跨正式截止可继续，不续计划、不续窗口。正式/临时剩余预算取合法较晚截止后再受本轮工作预算约束；Data及冻结完整读取先于诊断complete，成功收尾或硬截止退出。报文新增sync.data_pending_seq状态；48 B头/模板格式/哈希不变。详细合同见[协议总览](bridge-device-protocol.md)，实施/测试/候选部署边界见[执行记录](../project-workflow/bridge-macos/sync-window-fix-20261011.md)；现场以PROGRESS为准。
