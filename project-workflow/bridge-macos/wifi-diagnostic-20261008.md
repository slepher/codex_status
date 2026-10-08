# Note4 Wi-Fi 再次离线诊断（2026-10-08）

初轮只读诊断；未改设备配置、claim、配对、运行data或代码，未复位、重启Bridge、刷ROM或提交。后续用户要求先代码再USB实机核对，补充如下。时间为香港时间。

## 用户纠正后：设备配置与USB实测

前述任务容量是恢复阻挡，不足以单独解释设备图标或初次掉线，不能据此声称选错配置、密码错误或完整RF根因。

- USB枚举COM5/VID303A PID1001，Composite Device身份`7C:4F:AD:B9:34:08`。原生CreateFile/ReadFile/WriteFile，仅SetCommTimeouts，未SetCommState、未DTR/RTS、未PurgeComm。第一次35秒发送status/log/diag/timers；原文`artifacts/wifi-usb-20261008.private.log`。日志fw.69/ota_1/WIFI OFF/ip-/RSSI0，并明确`[wifi] awaiting Bridge-selected network over BLE`。样本无ROM reset/panic文本；不能用此替代完整打开前后boot_id证明。
- 串口log是有限ring，部分行已被字节环截断；未出现可确认的Wi-Fi密码/找网失败原因。另有BLE认证拒绝/unbonded及后来authenticated bonded peer、INFO precheck_failed与EPD busy失败，不能将这些混称Wi-Fi认证错误。
- 为读取配置，通过原生串口`pair`打开现有120s BLE联系/物理窗口，未新增配对或更改绑定，未发Wi-Fi activate/save。此为诊断机会改变，初轮“无设备控制动作”不再适用于追加阶段。原文`artifacts/wifi-usb-contact-20261008.private.log`。
- **12:21:02实时绑定加密BLE成功查询**：wifi_active返回home/version8/revision39；wifi_owner返回8c94；wifi_match在包含Bridge元数据及密码SHA256比较的请求中返回matches=true/active=true/connected=false/stage=saved，无error/reason。待比较目标为host wd21-wo→SSID wd21-la/label wd21。此证明设备活动配置与Bridge当前目标及缓存密码一致，不能证明路由器此刻仍接受该密码。脱敏成功帧记录`artifacts/wifi-device-match-proof-20261008.json`。
- 完整wifi_status分页及后续scheme_get探测未取得完整响应，均按查询超时记账；不能当设备无线连接失败或密码错误。早期独立脚本误用退役Usage特征，代码确认其回调直接忽略；修正TPL_CTRL后才取得上述成功帧。没有用这些失败读请求更改Wi-Fi或业务数据。

## 设备代码判断

- `src/main.cpp:5579`起普通启动关闭autoReconnect，并从BLE联系开始；startNormalMode与pollWifi都不会按旧配置自主WiFi.begin，断网后等Bridge选网。插USB本身也不会选网。
- `src/wifi_setup.cpp:528`起activate按全设备唯一network_id查找、校验version，然后复制该条SSID/密码；真正WiFi.begin在同文件354/594行。成功后才将该条持久设为active（308行）；普通配置读取只返回active，不遍历备用项（160行）。本轮实时active/匹配证明排除了“与Bridge目标不一致”的假设。
- `src/main.cpp:964`的WIFI OFF只依据wifiUp，不表示认证失败；templateStateText同源使用该状态。因此图标不足以将saved/offline区分为错误密码、未派发启用、窗口结束或实际RF失败。
- 当前样本已证：正确目标已保存并处于active，但设备未连接，等待合法Bridge启用；当前恢复链因Bridge容量与预算无法下发新activate。初次断网与0s Plan时序吻合，仍未获得初次RF失败的完整设备日志；不扩大为“所有断网都因队列”或“路由器没有问题”。

## 实测现场

- 默认Bridge健康端点204，运行exe SHA256 `BDC2818BA2EF3DB65482AF94230B366CFD89C512967E5D463C654468EB158F52`，与上一轮部署记录一致。主机WLAN为`wd21-wo`、192.168.3.111；保存的home映射指向`wd21-la`，本轮没有改变凭据。
- Note4登记MAC `7C4FADB93408` / IP192.168.3.177；该IP公开状态5秒超时，当前ARP表没有对应MAC。这单独不能证明设备未入网或判断密码错误。
- MCP被动wifi_view确认`automatic_state=retry_exhausted`、attempts3、http_ready=false、state=waiting_for_window、error=null。没有非终态Wi-Fi事务。
- 运行state中Wi-Fi任务恰好32条，全部`completed / may_have_committed=true / receipt_ack_pending=false`；包括已确认的启用历史，最后job2553628b2a7079c8于10-07 19:01:53完成。next_job_seq73。
- 最后完整认证HTTP状态10-07 19:02:25：fw0.18.69-note4、reset_reason8（深睡唤醒）、wifi_connected=true、RSSI-67、HTTP192.168.3.177 ready。此为历史状态，不能当本轮实时在线证明。
- 10-07 19:02:25随后Bridge下发plan2045，设备ACK accepted_remaining_s=0；紧接HTTP持续超时。19:04:08 / 19:05:40 / 19:07:14三轮plan2046–2048均ACK120s，却只有wifi_match/wifi_owner与HTTP探测，没有wifi_activate发送记录；之后预算耗尽。
- 10-08 12:12:14仍能BLE身份确认、status、claim及plan2334 ACK147s；说明当时设备仍能通过BLE工作。读取现场时Bridge原有后台调度仍自主运行，本轮未手动新发这些命令。

## 已确认的阻挡及调用链

### 追加Bridge网络操作日志审计

用户要求检查是否有改网操作。核对默认实例UTC日志10-05至10-08及按MAC持久事务，时间换算香港时间：

- 最近可证配置保存：10-06 03:42:25.904，BLE wifi_save/request `w3_0000000000000023`，recovery_save job186d46fb025afc0a，03:42:22持久记录completed/durable_receipt（记录时间取处理开始的now，与真实发送日志存在数秒差，不能据此倒置调用顺序）。冻结home元数据wd21-wo→wd21-la/label wd21，credential replace；随后版本为8。凭据不输出。10-06至10-08后续日志未见wifi_save/apply/trial/select/scheme_delete/setup发送。
- 10-07 17:56:11.403–19:01:08.868，24条不同request的BLE wifi_activate，约2–3分钟一次，全部对应持久jobs home/version8，完成operation_result且error null。这是启用已有配置，不是修改SSID/密码或切到CMCC；24个job与发送ID逐一可关联。历史总32个jobs=1 recovery_save+31 activate。
- 最后activate/request `w3_0000000000000049`：19:01:08.868发送，19:01:09.058确认接收；19:01:11.122 handoff applied；19:01:53持久完成，HTTP仍确认Plan2044剩75s。19:02:25.469 Plan2045 ACK0s，19:02:27.264 sync transfer complete，之后HTTP不可达。
- 19:02:25之后至当日日志结束，无新的BLE上述Wi-Fi写/启用，也无HTTP `/api/wifi/command` send；10-08日志截至本次核查亦没有两类发送。仍有status/wifi_match/wifi_owner/claim/PowerPlan等联系操作；不能把wifi_match当改配置。
- **日志能力边界**：HTTP日志只记录统一/api/wifi/command、部分request_id空，不记录具体业务op/目标；单条HTTP200不能判定是修改、查询或退役回执，须对照持久jobs和调用链。本轮记录未证明其他Bridge实例/路由器操作，也不把初次掉线全部归结为队列。
- 下一步协议诊断线索是同配置反复activate→hand off→正式Plan0→离线，以及容量满后再无activate；目前没有“桥改成错误网络/密码”的证据。

1. `bridge/crates/core/src/platform/wifi.rs` queue在第279行清理历史任务时，`may_have_committed`无条件保留；第280行在32条时返回`wifi_job_capacity`。全部32条已完成任务均有该标记，因此实际state必然无法创建新的activate/read等任务；24h TTL也不能清掉这些记录。
2. `bridge/crates/core/src/platform/service.rs` wifi_receipt_acked仅清receipt_ack_pending，未改变上述保留条件。已确认终态与仍待确认提交没有在清理条件中区分。
3. `bridge/crates/app/src/wifi.rs` automatic_on_link第513行先增加attempts并持久化，第516行用`if let Ok(job)`吞掉wifi_queue_control失败；随后仍返回Ok。三次没有派发新启用命令的本地失败被记成连接机会耗尽。
4. 当前MCP正确投影retry_exhausted，因此昨天修复已在运行；此次是独立任务容量阻挡，不能再次归因于成功HTTP不清零旧问题。

## 边界与修复方向

- 初次Wi-Fi关闭与正式0秒Plan的时序吻合；未读取实时设备诊断，不能证明初次离线的所有RF/设备内部原因，也没有本轮新panic证据。
- 修复须安全退役已确认且不再待ACK的历史任务或独立限制历史记录，并保留未知提交原ID/围栏；不能直接删运行state、取消或重发未知事务。
- 入队失败须明确反馈wifi_job_capacity，未派发不扣连接尝试预算。恢复须经正常应用服务在修复后显式重试；单独重置attempts或重启Bridge仍会遇到同一持久容量阻挡。
- 本轮不构建或部署修复，软件回归及实机恢复未验。待办归backlog B6。
