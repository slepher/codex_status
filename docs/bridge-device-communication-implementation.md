# Bridge—设备下层通信：场景与当前实现对照

核对日期：2026-10-11，源码基线9c38e45。本次只同步文档，不新运行软件测试、访问设备或部署。下列运行证据引用已完成的.76/.81/.82验收，不能推广为全部场景、其他设备或macOS通过。行为见[场景](bridge-device-communication-scenarios.md)，wire见[协议](bridge-device-protocol.md)，当前镜像见[PROGRESS](../PROGRESS.md)。

## 1. 当前路径

当前为48 B frame加业务载荷，业务HTTP使用POST。普通BLE接纳network_start并分项反馈后提前结束；原网络工作保留，设备在合法期限准备HTTP并报告。旧WaitHandoff/challenge/显式handoff不再是正常执行合同。

BLE-only本轮尾部继承_need_http，不能因Light或开启诊断而擅自转HTTP。已有健康HTTP直接执行业务；BOOT手动GATT、自动交汇和实际BLE射频独立。USB/有效Light重启直接恢复HTTP，Sleep普通重启一次有界BLE，实际电池Sleep重启矩阵仍未完成。

## 2. 场景对照

| 场景 | 当前源码机制 | 已有证据及边界 |
|---|---|---|
| S01/S02 HTTP直达 | app调度认证状态，continuous直接OTA准入；Bundle/Activate/Data通过device_client | .81/.82 OTA单次image_verified，实际模板发布成功；不能概括所有断链/权限组合 |
| S03 BLE小数据 | 生产ble_cycle判断_need_http；固件Data/Plan共享处理器，显示结果独立 | BLE-only+Light+sync enabled软件零HTTP断言通过；不是物理功耗验收 |
| S04/S05 准备HTTP | BLE network_start绑定原Plan/principal/回报地址；ACK grace后释放普通BLE，radio继续联网及报告 | 生产Fake验证close先于HTTP工作，.76一轮自然报告verified→complete→Data displayed |
| S06 失败/超时 | 原绝对deadline限制网络/报告；连接与Bridge HTTP失败分别记录；下轮恢复 | .76有三轮connect_timeout和后续恢复，不把失败计为成功或推断全部RF根因 |
| S07 身份/权限 | frame会话、MAC、principal、owner及业务上下文核对；新写401/403确定失败，历史未知查询仍保留 | 真实HTTP客户端与生产Fake拒绝回归；409保留revision_conflict而非一律occupied |
| S08/S09 丢ACK/断链 | 保留原plan/data/job身份；HTTP独立会话报告及认证原事务查询，不回旧BLE handoff收尾 | 丢Plan/network_start及原事务软件恢复覆盖；完整物理丢ACK矩阵未验 |
| S10 清理 | 主循环有界结束普通BLE，网络状态与BLE连接生命周期分离；Bridge有界close | 生产调度顺序测试；控制器功耗释放需实机测量 |
| S11 手动窗口 | startup临时窗口、手动active及实际radio分开；物理事件ISR有界捕获，元数据独立重绘 | 用户.81六次实体开关符合预期；.82追加按键验证电量无跳变；不替代所有忙时竞态 |
| S12 到期/权限变化 | 单调期限、会话代次、owner和持久Plan围栏；时间无效不恢复旧Light | PPL1故障/重放/重启生产测试，计划3490工具复位恢复同ID并扣时；无USB完整矩阵未验 |
| S13 双通道不可用 | 冻结作业、有限窗口/重试、确定错误及unknown分开 | 有软件边界测试；历史间歇启动/认证HTTP延迟仍未知 |
| S14 OTA后恢复 | 持久OTA队列、独立确认唤起/轮询、旧boot排除、目标镜像证明 | .81/.82均单次上传并运行SHA一致；不承诺所有弱网/掉电恢复已验 |
| S15–S18 网络选择/配置/恢复 | 真实host映射、保存与显式启用分开、有限重试，认证HTTP成功清失败计数 | 既有Wi-Fi软件/Note4样本沿用；跨主机权限/环境/长期RF仍开放 |
| S19 准入顺序 | 冻结admitted_wifi；完整读取先于会关闭临时窗口的诊断complete；尾部不新增未准入HTTP | 生产ble_cycle/Fake顺序和错误回归通过 |
| S20 取消/过期 | 本地终态保留sent/may_have_committed及原ID，查询历史未知不能当作新写拒绝 | 401/403和冲突回归；不把取消当远端回滚 |
| S21/S22 重启通道 | application先恢复PPL1；USB/有效Light选HTTP，否则普通Sleep有界BLE；物理事件另算 | USB工具复位后restored_light剩417/600秒，自动BLE关闭；电池Sleep实体未验 |
| S23 单份发布 | incoming下载时旧内容仍有效；预检后维护/覆盖，BCS2小记录提交；ACK后使旧data_contract失效并认证回读 | 压缩/中断/重放宿主测试及真实发布53cab015成功；Data453认证displayed，但当次HTTP丢响应不声称首发ACK成功 |
| S24 在线OTA | Notify+10秒兜底；continuous或至少120秒合法Light准入，确认宽限120秒；不写新OTA续期marker | .82 OTA46986171 succeeded/image_verified，原计划持久化与OTA作业恢复独立 |

## 3. 三种ready证据

1. rendezvous.ready：设备当前交汇已到本地HTTP准备阶段，交汇结束可恢复false。
2. transports.http.ready：设备报告的HTTP服务能力；Bridge matching_http使用这一能力，同时核对MAC/session/wake/Plan及所需owner，不再要求已结束交汇的ready仍true。
3. report_received：Bridge接收到了绑定本轮关联的报告，仍不是业务成功。共享endpoint Bearer不能单独证明声明MAC的结果；Bridge认证回读设备/查询原业务后才记verified。

设备POST /api/device/session申请独立报告会话，随后POST /api/device/report发送kind=report、op20；载荷关联原BLE session/request、plan业务ID、boot/context、IP/port和剩余量。报告不继承旧BLE授权。HTTP出站成功不证明设备入站可达，普通presence也只是hint。

## 4. 期限、资源与状态投影

- RendezvousProtocol当前状态：Idle / Exchange / HttpStarting / HttpReady / AckGrace / Done。RV_CONNECTED_MS=6000、RV_NETWORK_MS=30000、RV_ACK_GRACE_MS=200；始终夹紧合法在线期限，不借网络步骤重设正式Plan。
- serviceReadyReport最多3次，单次连接/响应最多1000ms并进一步夹紧剩余预算，退避250/1000ms；无窗口不继续。
- 正式计划PPL1逻辑49–176 B，重复同ID不写NVS。恢复保存的ID/内容/期限，不恢复旧owner或session；RTC不可验证不复活Light。详见[电源合同](power-state.md)。
- BLE普通请求整帧8192 B、Wi-Fi管理4096 B；业务响应单属性512 B含48 B头。HTTP整帧4096 B；完整Status大于4048 B时使用有界冻结分页，不能裁剪data_contract。
- HTTP power.plan_storage反映恢复/存储状态，radio.ble_manual_active / ble_radio_on与network.automatic_ble独立。模板手动BLE绑定21、ABI2，Wi-Fi阶段为文本；电量显示EMA不改变认证原始电量和低电保护。
- .81之后匹配ELF66项栈检查通过；.82新增显示状态16 B、BIN+464 B。局部帧/一次堆样本不证明长期峰值和功耗。

## 5. 源码及既有证据入口

| 内容 | 入口 |
|---|---|
| HTTP/GATT frame路由、安全检查及network_start | [transport.cpp](../src/firmware/transport.cpp)、[v2_command_envelope.cpp](../src/v2_command_envelope.cpp) |
| 设备会合/联网/报告 | [radio.cpp](../src/firmware/radio.cpp)、[rendezvous_protocol.cpp](../src/rendezvous_protocol.cpp) |
| 启动与正式计划 | [application.cpp](../src/firmware/application.cpp)、[plan_store.cpp](../src/plan_store.cpp) |
| Bridge报告接收、入站验证及调度 | [app main.rs](../bridge/crates/app/src/main.rs)、[app platform.rs](../bridge/crates/app/src/platform.rs)、[rendezvous.rs](../bridge/crates/core/src/rendezvous.rs) |
| 单份Bundle/压缩及合同失效 | [bundle_store.cpp](../src/bundle_store.cpp)、[bundle_payload.cpp](../src/bundle_payload.cpp)、[platform service.rs](../bridge/crates/core/src/platform/service.rs) |
| .76协议回归与恢复样本 | [协议闭环验收](../project-workflow/bridge-macos/protocol-implementation-20261011.md) |
| .81生产回归、计划复位与手动图标 | [BOOT/GATT统一验收](../project-workflow/bridge-macos/note4-boot-gatt-followup-20261011.md) |
| .82电量测试、镜像和用户反馈 | [电量平滑验收](../project-workflow/bridge-macos/note4-battery-smoothing-20261011.md) |

历史.78停滞和USB工具复位后的间歇延迟根因未确定；不能因新版局部通过而结案。B6/C8的电池Sleep重启、RF/掉电、跨平台和长期功耗仍以backlog为准。

### 2026-10-11 临时同步窗口修正（源码候选）

周期全量Data不授予正式Light。BLE小快照在会合内完成，大快照用既有sync_open新增reason=data/data_seq准入临时90秒WIFI_SYNC_ONCE；该窗口与正式Light绝对截止独立，跨正式截止可继续，不续计划、不续窗口。正式/临时剩余预算取合法较晚截止后再受本轮工作预算约束；Data及冻结完整读取先于诊断complete，成功收尾或硬截止退出。报文新增sync.data_pending_seq状态；48 B头/模板格式/哈希不变。详细合同见[协议总览](bridge-device-protocol.md)，实施/测试/候选部署边界见[执行记录](../project-workflow/bridge-macos/sync-window-fix-20261011.md)；现场以PROGRESS为准。
