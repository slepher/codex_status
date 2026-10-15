# Note4 移回办公室自动恢复失败调查（2026-10-15）

关联 backlog B6/C8；仅调查，未改生产代码、未构建/部署、未重启 Bridge/设备、未按 BOOT、未写网络配置。

## 结论与证据

用户补充：约09:50带回，电池供电，当前屏幕/设备正常处于睡眠。用户时间为估计，与日志09:45自动联系相差约5分钟；不强行认定精确移入时间。睡眠本身不构成故障，当前阻断在Bridge会合后的写入取消及隔离。

设备 MAC 7C4FADB93408，Bridge 登记 IP 192.168.31.90。当前 MCP bridge_status 返回 offline / Host is down；Bridge 主机地址192.168.31.24、主机 Wi-Fi 已连接办公室网络。当前无 USB 串口可读。

2026-10-15 09:45:08.880（北京时间），生产 Bridge 自动扫描3832发现目标；09:45:09.439认证INFO核对MAC，wake_generation=951072247、wake_seq=1010。status、wifi_owner、claim、sync_config依次收到成功回执。无需手动BOOT即已发生自动联系。

09:45:10.178366 wifi_match开始write_dispatch；09:45:10.878563（约700ms）进入失败收尾，随后的plan返回before_write / previous_write_unknown / not_sent，射频预算仍剩3871ms。09:45:10.880718断开确认耗时1ms，但write_uncertain仍为true，09:45:10.880754触发ble_native_quarantined_restart_required；之后日志没有新的BLE扫描，HTTP继续有界失败。

代码链条：bridge/crates/app/src/rendezvous.rs:178 对 prepare_connection 套700ms timeout且丢弃结果；wifi.rs:371执行wifi_match；ble/src/lib.rs:1472写前设write_uncertain，仅完整write返回成功清除；外层取消保留未知；下一plan在1397拒绝；close_bounded的1729即使disconnect成功也因写未知不释放NativeLease，Drop进入隔离。日志准确对应外层700ms取消，而非设备射频预算耗尽。未完成写入的底层延迟原因仍未知，不以此断言固件panic/密码错误/设备主动断链。昨天修复的是pending connect回调，本次为已连接后的写入取消，属于不同触发路径。

## 页面提示

platform_wifi_view.online.observed_at=1791971897，即10月14日17:58:17；wifiRenderOnline（ui/index.html:1223）只用此旧在线快照显示“最近认证16小时前”，未反映今早BLE status成功。历史connection.state=connected也不是当前Wi-Fi在线证明；http_ready=false、bridge_error=not_sent。原始MCP视图可能携带密码，即使工具描述称无密码；本次保存的JSON证据已递归脱敏，后续只输出白名单字段。

## 假设核对与验收边界

1. 原生写入未结束：成立，实际由外层700ms取消触发未知状态；隔离是后续无法自动恢复的直接阻断。
2. 设备主动断链：没有本次断链先于取消的证据；cleanup发生在plan拒发之后。
3. Wi-Fi配置不匹配：缓存双方office同版本235且字段一致；本轮wifi_match无结果，不据此判断当前设备配置/密码/射频状态。

本次以真实生产日志时间线与代码取消路径交叉核对，未新增回归测试，不宣称已修复。后续最小修复应让准备阶段与命令共享同一有界预算并明确处理失败，避免外层中途取消原生写入；不得仅清除未知写标志或放宽隔离。需要在真实调用点注入超过700ms的分片写，覆盖plan准入、关闭及下次会合。恢复现场可在保存证据后只重启已核实的托管Bridge，再等待自然会合；设备无需为此手动BOOT。当前尚未执行恢复。

证据（本地，不入库）：artifacts/bridge-macos/return-office-20261015-before-restart.log；return-office-20261015-{platform_overview,platform_wifi_view,bridge_status}.json（脱敏）。
