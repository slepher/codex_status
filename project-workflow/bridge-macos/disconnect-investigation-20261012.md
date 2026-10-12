# 13:19 后失联：手动唤醒后的局域网取证

关联 B6/C8。用户已唤醒设备，要求直接通过本地网络读取日志。仅只读取证，未重启 Bridge、操作串口、改配置或刷 ROM。

## 现场证据

- 从实际 macOS data/platform/state.json 取登记 MAC `7C4FADB93408`、IP `192.168.31.90`；14:46:13 GET /status.json 核对同 MAC 后保存 /log、/history、/pmstats。原始证据在 artifacts/bridge-macos/manual-wake-20261012-1448-*.raw（0600，私有，不提交）。文件名标签1448不代表首次采样时刻。
- 当前 ROM仍为0.18.83-note4/ota_0，wake_generation2534412038、wake841，reset为deep-sleep；后续认证MCP status为ok、reset_reason8。没有当前醒次panic/冷复位证据。
- /history保留819–841，共23条：11次timer/no_connection、11次thin、1次当前active。11次网络会合均BLE advertising而没有BLE connected/command/IP/HTTP，醒着5298–5569ms后回睡；thin为短时钟刷新。旧753醒次不在当前环，不能用本环复原13:19首次连接失败的设备细节。
- 当前841开始于timer醒次，运行中物理按键：/log physical_ms3451、handled_ms3579，授予300s light/120s手动GATT。history记录IP6600ms、HTTP12000ms、command36700ms；按键后约3.1s取得IP。不能把6600ms误作从按键起算。
- 14:46:13 Wi-Fi connected、RSSI-24、ap_reason none；正式plan3723剩余267s，data/applied_seq500、显示displayed。Bridge日志14:45:30 HTTP claim成功，14:45:39 plan ACK300s；网络恢复后HTTP交付已恢复。
- 首次pmstats尚在手动BLE窗口，btLS/ble-rv锁有效，实际light_sleep_count0；后续uptime187s，BLE已关闭、manual hold0、light_sleep_ms44325/count2758，pmstats也记录SLEEP49.8s。证明当前已实际进入light sleep，首轮mode=light不等于已经睡眠。

## Bridge对应事件与源码缺口

- 13:17:04 wake753认证成功；13:17:05 sleep plan3722 ACK，rendezvous_period60s。
- 13:19:04 scan1034命中设备；13:19:06.870 btleplug记录“Event receiver died, breaking out of corebluetooth device loop.”；13:19:08及12两次connect失败，耗时4003/4002ms。
- 此后直到本轮取证没有新的BLE scan started/summary，HTTP/ARP循环及MCP仍存活，权限均allowed、电脑仍连Xiaomi_1EBB。不是整个Bridge退出。设备手动联网恢复也未使BLE扫描恢复。
- bridge/crates/ble/src/lib.rs DeviceConnection::connect_any的两次connect失败分支直接await peripheral.disconnect()，没有清理预算。ble_cycle/主BLE轮询串行await connect_any，故清理不返回即停止后续扫描。
- btleplug0.11.8 macOS disconnect发送消息后等待reply；internal::disconnect_peripheral只在peripheral仍在map时保存future并请求系统取消，缺失时不完成reply。这与13:19外设事件接收循环退出、两次connect失败后扫描永久消失高度吻合。
- 已有close_bounded覆盖成功建链后的结束清理；尚未覆盖connect失败等建链前分支，当前14:33候选也保留该缺口。

## 结论与验收边界

已确认设备在后续多轮定时唤醒中运行、按计划广告、没有连上Bridge，手动Wi-Fi路径可正常建立网络并实际light sleep。失联持续的首要原因是Bridge BLE扫描/恢复停滞；已发现具体实现缺口为连接失败后的无界disconnect清理，而非需要统一加长4s连接超时或设备AP未找到。

运行任务栈未抓取，故“此进程正等待该disconnect future”仍为与日志、依赖源码一致的高置信定位，不伪称已取得堆栈证明；13:19第一次连接为何失败也未由现有设备环证实。4KiB环已不保留那一醒次，不把后续日志当作原始现场。

新增待修复：统一所有建链前错误分支的有限清理，记录清理开始/结束/预算耗尽，验证外设消失后失败能返回、下一轮继续扫描；不变更现有业务超时、事务ID/未知结果围栏或PowerPlan。此前分类/关键日志候选尚未部署，本次没有修改运行代码。

## 15:59后续修复状态

用户随后授权修复并重启Bridge。所有裸断开清理统一有限2s预算，生产connect失败控制流回归red→green，15:55部署；15:56首轮写入未知后扫描继续、15:58下一自然会合完整成功、15:59认证HTTP通过。原13:19运行栈仍未取得，不改变上文历史证据等级。见[修复/部署](ble-connect-cleanup-fix-20261012.md)，固件仍0.18.83。
