# 离线 BLE 配 Wi-Fi 与物理蓝牙恢复入口

2026-10-03：最初仅源码诊断与规划；用户后续已授权按计划实施。当前 BLE 配网、按绑定方案与 BOOT 离线恢复尚未实现，已有 USB 配网恢复与设备字段合同修复见 `PROGRESS.md`。待办唯一来源为 [backlog B6](../../docs/roadmap/backlog.md#b6-离线-ble-配-wi-fi-与-boot-蓝牙恢复入口2026-10-03-新开)。用户进一步确认这两项功能的操作入口均在 Bridge 设备页。

## 现有行为与证据边界

设备已保存旧网络，搬到新环境后没有可连接的已保存 Wi-Fi。此时需要独立于 Wi-Fi 的 BLE 恢复通道；现行 Windows Bridge 对等移植本身不提供这项功能。

| 场景 | 当前源码行为 | 证据 |
|---|---|---|
| 清醒、Wi-Fi 已连接、BOOT 单击 | BLE ON/OFF 切换；打开时有 120s pairing window | `src/main.cpp:3726` 的 `handleBootClick` 与 `enterBleOn` |
| 清醒、Wi-Fi 不通、PC USB 接入、BOOT 单击 | `retryWifi()` 后返回，不开 BLE；重试是发起连接，不等于已连接 | 同函数 `!wifiUp` 分支 |
| 清醒、Wi-Fi 不通、电池、BOOT 单击 | 直接返回，不开 BLE | 同上 |
| 电池深睡、物理按键唤醒、旧网络不可用 | 建立 provisional 后进入 `startNormalMode`；扫描/连接失败时仍走重新睡眠路径，不先提供可交互 BLE 恢复入口 | `setup`、`connectBest`、`startNormalMode` 的失败分支 |
| 无 Wi-Fi 槽位/AP 配网态 | 进入 AP；loop 在 BOOT 处理之前返回 | `startConfigMode` 与 `loop` 的 `configMode` 分支 |
| Wi-Fi 在运行中断开 | loss 策略可关闭 BLE；手动保持的豁免目前还以 `v2BundleReady` 为条件 | `pollWifi`、`v2ManualBleHoldActive` |
| 定时 `device_first` 会合 | 短时广播和现有认证命令可运行，但不是可供人工配网的 120s 交互会话 | `v2Rendezvous` |
| BLE 写 endpoint | 保存 Bridge host/port/token；不写 Wi-Fi SSID/密码 | `handleBleEndpoint` |
| 配 Wi-Fi | 已有 AP `/save` 和 USB `wifi <ssid> <pass>`；未发现 BLE 配网命令/能力 | `handleConfigSave`、`handleSerialCli`、`serviceV2Ble`、BLE 特征表 |

`docs/power-state.md` §3/§4 还明确写着“WIFI OFF 时不建立 BLE 会话”和“WIFI OFF 插电单击立即重试 Wi-Fi”。因此 BOOT 在断网时不开蓝牙是**现有产品规则和代码限制**，不是本次 macOS 蓝牙版本不足的证据。旧文档关于 BLE 仅身份类和 BOOT 2s 配对的部分也不能替代现行源码：当前正常态 2s 是切模板，配对会话入口是单击。

本次未开串口、未配对、未扫描或连接外设，未执行复现 harness 或诊断固件。以上结论仅确认源码控制流；实际设备是否运行同一逻辑、GPIO 状态和按键时序仍未知。用户仅授权规划，因此 Diagnose 的实机复现/插桩/修复阶段不执行，不能将静态判读称为实机已复现或已修复。

## 本机 USB 现场

只读 `system_profiler SPUSBDataType -json` 与 IOSerialBSDClient 枚举见一台 `USB JTAG/serial debug unit`，厂商 Espressif，VID `0x303a`、PID `0x1001`，串口 `/dev/cu.usbmodem14101`（对应 tty 节点也存在）。受限枚举起初未见该设备；沙箱外查询后已见。

用户确认本环境只携带 Note4，1.54 不在现场；本轮硬件规划和后续现场验收仅针对 Note4。未打开端口，候选设备的 Wi-Fi MAC、运行 FW_VERSION、设备对 PC USB 的 `plugged` 判定或 BOOT 故障现场均未读回。端口动态，未来测试应重新枚举并核对设备身份；打开串口可能复位设备，必须提前保护待观察的按键/唤醒现场。共用代码须保留另一目标的兼容性，但不据 Note4 结果宣称 1.54 实机已验，也不要求本轮取回 1.54。

## 拟定用户流程

用户已确认允许 Bridge 与设备使用不同 SSID：Bridge 可连接 5GHz，设备使用对应 2.4GHz 网络。**每个 BLE 绑定关系对应一套保存在设备上的 Wi-Fi 方案**，方案内含多组主机网络→设备网络映射，而非所有绑定共用全局网络槽。

| BLE 绑定方案 | Bridge 当前网络（映射条件） | 设备应连接的网络 | 凭据位置 |
|---|---|---|---|
| 主机 A 的绑定 | `Home-5G` | `Home-2G`（网络 ID `home`） | 设备上该绑定方案 |
| 主机 A 的绑定 | `Office-5G` | `Office-IoT`（网络 ID `office`） | 设备上该绑定方案 |
| 主机 B 的绑定 | `Studio-5G` | `Studio-2G`（网络 ID `studio`） | 设备上主机 B 的绑定方案 |

设备持久化绑定方案、映射两侧网络名称、设备网络密码与稳定网络 ID；绑定身份采用设备解析的 bond 身份，不使用可能变化的扫描地址或 MAC 主机名。BLE 绑定属于主机配对关系，同一主机多个 Bridge 实例不能被误认成多个 BLE bond；业务实例 owner 仍由原 claim/lease 区分。

1. 在 Bridge 设备页选择设备，使用“通过蓝牙配置 Wi-Fi”和“本机绑定的 Wi-Fi 方案”；已登记设备明确选择 MAC，未登记设备从设备页的蓝牙添加入口选择附近候选，再核对设备完整 MAC/硬件信息。设备离线时仍可打开这些入口，不以 HTTP 可达为条件。
2. 用户在设备上单击 BOOT 打开有界 BLE 恢复会话。正常连接、断网、没有 Wi-Fi 凭据/AP 模式都能进入；深睡时首次 BOOT 直接唤醒并进入恢复会话，先保证蓝牙可用，不让旧网络试连耗尽入口。
3. 首次使用按 macOS/Windows 原生流程绑定；已有绑定可重连。Bridge 重新确认 bonded+encrypted 与目标 MAC 后才接收配置权限。长按 2s 切模板等既有意图不顺带变成配网，具体 GPIO 启动去抖与长按分流在实施时验证。
4. 在当前绑定方案中建立“Bridge 网络 → 设备 2.4GHz 网络”的对应关系，输入设备网络密码。Bridge 网络可以自动读取或手工指定；支持隐藏 SSID和开放网络。可选择让设备扫描附近 2.4GHz 网络，结果来自设备而非 Mac 自身 Wi-Fi 接口。SSID/密码按字节长度与支持的认证类型校验，未支持的企业认证明确拒绝。
5. 设备在 BLE 保持可用时异步试连候选网络，Bridge 显示阶段：已收到 → 正在连接 → 已取得 IP → 配置已持久化；分别显示密码/认证失败、AP 不可达、DHCP 失败、超时和持久化失败。命令收到的 ACK 不能当成配网成功。
6. 成功后回读 BLE 的 MAC/IP 与结果，再经认证 HTTP 核对同一 MAC，进入原有发现/claim/同步流程。若 Mac 与设备不在可达网段，区分“设备已接入 Wi-Fi”和“Bridge 暂不可达”；不撤销已成功网络配置。
7. 失败可在固定会话期限内重试或取消；旧网络和业务配置保留。到期/用户关闭时清理 BLE/radio/临时凭据，并按原正式 PowerPlan/离线策略恢复运行。

## Bridge 设备页入口与展示

用户确认蓝牙配 Wi-Fi 和绑定独立 Wi-Fi 方案均体现在设备页，由 Windows/macOS 共用现有 application service 与设备 UI。

- “通过蓝牙配置 Wi-Fi”：展示目标设备与蓝牙连接/绑定状态、BOOT 开启恢复窗口的提示；支持输入设备 2.4GHz SSID、密码和对应的主机 SSID，提交、取消及查看试连/持久化结果。密码仅用于此次加密传输，输入遮蔽，完成或取消后清理，不从设备回读。
- “本机绑定的 Wi-Fi 方案”：读取设备中属于当前 BLE 绑定的无密码摘要，展示“主机 SSID → 设备 SSID”映射、选中网络和设备连接结果；支持新增、编辑、显式删除映射与选择已保存网络。不能操作其他绑定的方案；同一主机的命名 Bridge 实例不显示为不同 BLE 绑定。
- 展示 Bridge 当前 Wi-Fi、匹配的设备网络与自动选择结果；无 SSID 权限、无映射、等待下一次设备会合、设备已联网但 HTTP 不可达及连接失败各有明确状态。请求已接受和网络已连接分别显示。
- 已登记设备离线时保留入口，通过 BLE 完成方案读取和配网；未登记设备的添加流程也从设备页开始。方案、映射和密码的持久事实在设备上，页面不要求先发布模板、建立本地 Profile 或取得 HTTP 联系。

设备页验收须覆盖离线进入、已有绑定无需重配、5GHz→2.4GHz 映射管理、配网成功/失败/取消和动态选择状态；实际网络结果来自设备报告，不将本地表单保存或 BLE 接收 ACK 视为联网完成。

## Bridge 换网后的动态选择

Bridge 读取当前真实关联的 Wi-Fi SSID，并经已绑定加密 BLE 读取**本绑定方案的摘要**（映射条件、设备 SSID、网络 ID，不含密码）。Bridge 根据映射决定使用哪个设备网络，再下发 `wifi_select(network_id)`；设备不能依据某个外设名称或公开广播自行改网。

- 初始化、主机网络关联变化、主机睡眠恢复及 BLE 重连时重新检查映射；同一网络选择保持幂等，不反复断网。换网期间合并为最新选择，旧通知/旧请求结果不得把设备切回上一网络。
- 无匹配映射、主机未连接 Wi-Fi/仅有有线网络、读取 SSID 权限缺失均有明确状态，不猜密码、不复制主机 SSID到设备、不任意选别的绑定方案。首次使用新环境由用户通过物理恢复窗口补充映射；此项未规定有线主机自动映射策略。
- 已绑定主机可以动态选择自己的已存网络，不要求每次再人工打开 pairing window；未绑定主机仍需物理授权。设备 deep 时切换请求等待下一次正常 `device_first` BLE 广播窗口，也可由用户 BOOT 提前开启恢复入口；不能凭 Bridge 换网保证立即唤醒设备。
- 动态选择不需要密码再次经过 Bridge，Bridge 不读取或持久化设备密码，也不从系统钥匙串提取主机密码。网络名称/映射摘要仅作为本地 UI/选择所需临时数据，持久事实在设备上。
- 自动选择服从现有占用：有效 owner 属于其他 Bridge 时不得为了改网抢占或中断其业务；空闲/lease 到期时按绑定认证和正常 claim 流程协调。物理恢复入口可以编辑本绑定方案，但不因编辑/选择隐式转移 owner、发布 Bundle 或重建 context。
- 选网 ACK 区分“已接受选择”和“设备已联网”；按已有正式 PowerPlan 或物理维护窗口进行 Wi-Fi 试连与结果回报，不因命令读取/传输无限延长无线期限。失败保留旧配置，回报当前绑定/网络 ID及错误，重试有界。
- 同一 SSID 可能在多个地点重复，首版将其作为用户配置的匹配条件，不把 SSID 当设备身份或网络认证。5GHz 与 2.4GHz 映射也不证明两个网段能互通，设备联网和 Bridge HTTP 可达分别确认。

macOS 当前网络读取需设计原生 CoreWLAN 的权限/失败路径；Apple 工程师说明 macOS 14 的 SSID 读取受定位权限约束，不能靠终端命令或猜网关替代“Bridge 当前连接的 Wi-Fi”。P0 应验证真 `.app` 中授权、拒绝、恢复与后台更新；本轮未请求定位权限或读取实际 SSID。[Apple 关于 CoreWLAN SSID 权限的说明](https://developer.apple.com/forums/thread/732431?answerId=758114022)。

## 固件入口与电源合同

应把“用户物理恢复会话”和“正常短 `device_first` 会合”明确分开。建议恢复会话为固定 120s，起点是物理操作；重复状态读、连接保活、分片传输和试连不自动延长。再次物理操作的关闭/重新打开语义需统一去抖，不允许配置重试无限续期。300s BOOT provisional 的正常业务语义保持原规则。

恢复会话是一项新的**物理、有界维护入口**，不是 Bridge 正式 light plan，也不把数据/claim/pull 变成续期条件。它应在无 Bundle、AP、Wi-Fi 失败的启动/运行路径均有效；到期后若原业务截止已过立即收尾。原业务到期不得在用户恢复会话仍有效时让 BLE 突然消失。实现时须把此例外同步到权威电源文档和模型，不直接借用“连接期间一直延长”的旧 `serviceBleSession` 规则。

仅删除 `handleBootClick` 的 Wi-Fi guard 不够：还要处理 `pollWifi` 关 BLE/深睡、startup 失败睡眠、AP 提前 return、Wi-Fi 阻塞扫描/关联和无 Bundle 时 hold 不生效。配网期间扫描/连接必须分阶段服务 BLE 队列、通知、按键取消和期限检查，不能在 NimBLE 回调内调用阻塞 `connectBest`。

恢复会话不得清 Profile、Bundle、字体、tokens 或 jobs，不隐式 claim/转移 owner，不重建 active context。用于网络配置的权限独立于业务占用：物理打开的恢复窗口 + 当前绑定加密链路 + 完整 MAC 核对；未登记且无 Wi-Fi 的设备也可接入，不能依赖先经 HTTP 领取 owner。普通业务写仍沿用原 token/owner/nonce 合同。

## 最小协议与数据持久化方向

优先复用现有加密 GATT 命令/状态通道，info 加显式 `wifi_setup_protocol` 能力；不先增加 GATT 特征，避免 Windows 属性缓存额外影响。旧设备返回能力不支持，Bridge 给出 AP/USB 恢复说明，不能把现有 endpoint 当 Wi-Fi 配网命令发送。

拟定操作：`wifi_setup`（请求 ID、MAC、网络 ID、主机 SSID 映射条件、设备 SSID、凭据）、`wifi_setup_status`、`wifi_setup_cancel`、`wifi_scheme_get`（只读本绑定摘要，无密码）、`wifi_select`（请求 ID、MAC、网络 ID）；设备网络扫描可用独立 `wifi_scan`。绑定方案归属从当前认证链路确定，不信任请求自报其他 bond ID。编辑/试连需要物理恢复授权，正常动态选择已有网络走已绑定认证与 owner 协调，两类权限分开。现有 `serviceV2Ble` 先要求 endpoint token、还会 `markSynced`/更新时间，因此恢复命令需要独立的授权与路由入口，不能创造业务 owner、虚报数据联系或改 PowerPlan。不得把这个例外放宽为普通业务鉴权旁路。

所有命令与结果有有界大小、分片重组、请求 ID、去重、明确错误码和超时；支持通知丢失/重连后查询同一请求结果。首版可以每设备只允许一个在途配网事务，不引入通用事务框架。现有全局 3 个 Wi-Fi 槽位不足以代表新的按 bond 方案模型；每方案/设备总容量应按 NVS 与协议大小明确设限并返回容量错误，不静默覆盖其他绑定网络。Profile 的 1–8 模板与网络容量无关。

候选凭据先只在 RAM 中试连；失败不覆盖旧网络。成功后以能抵御断电的记录提交方式更新本绑定方案中的网络/映射及选中网络，并在提交成功后报告完成；不得照搬 AP/USB 当前“槽满就覆盖 0”的隐式策略。替换或删除由用户显式表达，NVS 跨键写入不能假称原子。现有 AP/USB 保存的全局旧网络保留为明确的 legacy 恢复配置，不未经授权归给所有 bond；解绑后的方案清理/保留规则在实施合同确定时明确。配网事务结果只保留必要 ID/阶段/错误/IP，不重复存储或回显密码；设备正常网络配置必须保留连接所需密码。

Wi-Fi 密码只出现在加密链路和设备必要 RAM/NVS；Bridge 输入暂存，不落 `state.json`、日志、MCP 参数审计、测试夹具或仓库。配网失败/断开/超时清理临时缓冲。SSID 原文默认不写诊断日志，显示与日志净化沿用项目规则；SSID 的合法编码与显示用 ASCII 净化应分开，不能为字体净化而修改网络凭据。

## 后续拆分与验收

| 阶段 | 责任范围 | 交付门槛 |
|---|---|---|
| W0 现场与合同 | MAC/目标/运行 ROM/供电/按键类型核对；按 bond 方案/映射/容量与权限；固定窗口/错误码/持久化规则 | 有真实失败与成功基线；确认不同 SSID 的 5GHz→2.4GHz 映射；不用串口复位代替 BOOT 现场 |
| W1 独立恢复入口 | `setup/loop/handleBootClick/pollWifi/serviceBleSession` 及电源模型 | 断网、AP、无 Bundle、USB/电池和深睡按键均可获得同一有界 BLE 会话；正常 timer 仍只走短会合 |
| W2 设备网络事务 | 按 bond NVS 方案与映射、BLE 路由/分片、异步试连、选网/取消/结果查询 | 绑定之间隔离，不返回密码；错密码/无 AP/无 DHCP/断电不破坏旧网络；越权网络 ID拒绝 |
| W3 跨平台 Bridge | 当前 Wi-Fi 读取与变化检测、设备方案摘要/映射选择、BLE client + 共用 application service + 设备页的配网/绑定方案 UI | 从设备页离线配置并管理本机绑定方案；5GHz→2.4GHz 动态选择只发网络 ID；SSID 未知/无映射有明确状态；旧通知不回切；动态选择服从 owner |
| W4 回归与实机 | Fake ROM 状态/错误矩阵；Note4 的 Windows/macOS RF/配对/共存，1.54 仅做软件回归并标实机未验 | Note4 在新环境从无 Wi-Fi 走到认证 HTTP；正常 `device_first`、Profile、owner、PowerPlan 与队列无回归；Windows 实机对照按可用现场安排 |

未来必须覆盖：USB 清醒断网单击、电池清醒断网单击、深睡按键唤醒、无网络槽位/AP、已联网开关 BLE、已绑定重连与首次绑定、取消配对、错 MAC、同名/同后缀候选、开放/隐藏/非 ASCII SSID、错密码、2.4GHz 网络不可见、无 DHCP、BLE 分片丢失/通知丢失/重连、槽满显式替换、持久化失败/断电、会话超时与主机睡眠。

映射专属验收：两个 bond 各自不同方案、同一主机在两个 5GHz SSID 间切换并对应不同设备网络、同 SSID 的双频网络、不同 SSID 的双频网络、无映射/无 SSID权限/仅有线、旧主机网络通知晚到、设备 deep 时延后选网、他人 owner 在用时拒绝自动抢网、断电后恢复已选网络、另一绑定网络 ID不可访问、映射/密码不落 Bridge state/log。

权威 `docs/power-state.md` 的 WIFI OFF 不开 BLE 规则须在实际实施合同确定后更新，当前仍作为旧行为证据保留。新版固件构建/发布是与托盘移植分开的后续动作：默认目标约束仍生效，1.54 构建/测试须另有授权；本轮没有新 ROM/哈希或现场行为修复。
