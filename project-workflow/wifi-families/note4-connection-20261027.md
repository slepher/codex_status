# Note4网络连接调查（2026-10-27 19:26–19:30，Asia/Hong_Kong）

## 结论与现场

登记/认证同MAC `7C4FADB93408`。Bridge主机当前网络及族解析正确；BLE往返正常。认证设备状态明确报告 `network.error=plan_network_target_mismatch`，HTTP未就绪，不是已有证据支持的密码错误或2.4G不支持。

合并共享族后，Bridge当前home目标的显示label与上次完整设备快照不同，host_ssid/ssid相同、两份缓存密码均存在且相等（不记录密码）。`wifi.rs::target_query`把host_ssid/ssid/label和open标志一起计算metadata_key；`wifi_setup.cpp::wifiTargetMatches`核对该摘要，`radio.cpp`启动网络前`wifiPlanTargetMatches`只接受当前active或已授权待激活candidate。故显示名称变化也会要求配置/启用一致；本轮错误发生在网络目标检查阶段，不能把它归为路由器认证失败。旧完整快照日期较早，密码相同只证明两份缓存一致，不能代替实时RF认证。

## 延迟链与原事务

- 19:26无显式恢复作业时，Plan ACK后多次wifi_status分页占用剩余RF时间，后续匹配失败为not_sent/window_budget_exhausted。
- 19:27已有恢复保存任务 `260cfe9eabf658c6`，原请求 `w3_000000000000017c`。wifi_apply发送时只剩136ms，写完剩86ms，ACK超时；结果未知必须只查原事务。
- 19:28 Bridge自动查到原请求durable_receipt，保存completed/revision344。wifi_status每页最多80原始字节（hex），本轮约30次往返后原回执ACK又触及RF截止；这是可核对的时延放大因素，未据此修改窗口/策略。
- 19:29:59 Bridge自动发送原启用任务 `w3_000000000000017f`，ACK仅表示接纳。
- 19:30:05收到同MAC网络回报；19:30:09认证HTTP ready，19:30:23会合complete。另一次显式只读platform_status_refresh成功，同MAC `0.18.106-note4`，`network.error=null`、bridge_reachable=true、HTTP ready、登记地址对应同MAC，内容ready。已证明本轮Wi-Fi/HTTP恢复，不扩大为长期稳定或全部同步任务完成。
- 最后持久状态中启用任务仍awaiting_confirmation，保存任务已durable_receipt确认；连接恢复与启用事务最终结算分别记录，不能冒充两者均完成或重发启用。

## 版本/操作边界

此前BLE合并状态body呈现`.105-note4`；同boot后续HTTP认证明确`.106-note4`。存在缓存字段新鲜度不一致，不能凭该BLE字段判定降级/刷机需求。当前以HTTP实测为准，交接卡的`.106`未被推翻。未导出运行镜像全hash，也不声称本轮完成镜像逐字节核验。

本次只读MCP、注册数据、定向Bridge日志、ARP及登记HTTP探测；Windows串口枚举无在场端口，没有打开串口。未创建/重发保存或启用、未改密码/订阅/owner/Plan、未停桥、未刷ROM。观察期间恢复保存和启用均由既有Bridge调度推进。

证据在私有`artifacts/wifi-families/note4-connection-{investigation,latest}-20261027.json`及运行日志`bridge-app.log.2026-10-27`对应UTC11:26–11:30段；无密码/token。分页耗时和缓存版本新鲜度归B6后续，不增加刷机前置要求。
