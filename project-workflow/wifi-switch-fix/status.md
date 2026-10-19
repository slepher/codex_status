# Wi-Fi 切换恢复（B6/C8，2026-10-19）

## Plan

用户要求独立 worktree 修正当前切网失败。基线 4868c8c，包含已部署 macOS 的事务修复 724f3a3，排除 ea64ff8 的未部署协议收敛。Windows 现场旧 exe 2026-10-18 11:42:30 / 38,095,360 B / SHA256 2CDC1AD87048C65CF7BBB2FAD98AC774CE321AC2E39B0F30B73B1A465DBC7CA5。不改 wire、固件或持久设备配置。

## Task

验证已有异步 activate → network_start 与原事务查询回归，隔离构建；冻结备份运行 exe/data 后替换 Windows Bridge，保持任务和运行数据路径，再自然会合核验认证 MAC、目标网络及原事务结果。

## Status

调查中。Windows 主机 wd21-wo，登记 Note4 7C4FADB93408，映射 home/v8 → wd21-la。运行数据存在12次 activate 的 window_closed 失败；11:44:20Z 日志收到 wifi_activate ACK 后直接 BLE disconnect，无 network_start，随后 HTTP 超时。旧 Windows 二进制早于 724f3a3；该提交已修复 operation_pending 误作错误阻止 network_start 的路径。不能用此前 macOS 恢复记录代替 Windows 验收。

## Review

复用 724f3a3，不重复编写修复；先运行真实 ble_cycle 的异步 pending/原ID确认和未知写保护回归。只读白名单核对运行数据，不记录密码/token，不清历史事务。源码验证、部署及真实 RF 结果分别记录。

## 已验证（19:52 香港时间）

运行中 MCP 只读视图确认 host=wd21-wo、matched_network_id=home、connection_failed/window_closed，Bridge 实例 PID4404、watchdog21156，均为登记的 Windows debug exe。计划任务 CodexStatusBridge 指向同路径（Ready，实际进程由 watchdog 延续）。用户目视设备 Wi-Fi OFF / Bridge OFF；缓存中的 radio 旧字段不证明当前连通。

现有完整 ble_cycle 测试改用现场 inactive + failed/window_closed/reason201 初态；定向回归1/1通过，包含异步 activate pending 后 network_start、第二轮原ID确认、历史未知记录不变。构建头文件用 CODEX_STATUS_ARDUINOJSON 显式引用原仓库现有 ArduinoJson/src，不构建固件。隔离产物使用原仓库 bridge/target/wake-contact-check，不动 debug/data；完整Bridge回归进行中。

## Windows 部署（19:55:58 香港时间）

Bridge回归332通过/5条件忽略（App111、BLE28、Core164、集成AP01模板3/device_client7/envelope7/template9、MCP3）。Windows debug候选38,739,968 B，SHA256 66E8E7F2EA64F9569A24ECBE49B6C57C0064D1F6AADC7FF7BD7CCD0A06B937E6，源4868c8c及现场回归初态变更。没有ROM重编/刷写。

按路径核对并先停watchdog，再停任务及核实的残留主进程；冻结备份 D:/Documents/PlatformIO/Projects/codex_status/artifacts/wifi-switch-backup-20261019-195530。264文件逐一哈希一致，替换exe后启动前data完全未变；原路径受管脚本重启，PID36464/watchdog15164，health204。运行principal仍8c94。部署证据 artifacts/wifi-switch-fix/deployment.json，旧现场 before.json。新增Job确认计数可能被持久化，回滚必须恢复冻结备份，不能直接让旧exe读取新data。当前等待自然会合验收，不宣称设备切网成功。

## 最终验收（19:58–19:59 香港时间）

19:57:03自然会合：wifi_activate 原ID w3_000000000000017a 收ACK，紧接 network_start accepted；19:57:14认证HTTP ready。随后原事务 completed / operation_result / revision342，receipt_ack_pending=false；完整快照active home/v8。19:58独立恢复视图 MAC 7C:4F:AD:B9:34:08、SSID wd21-la、IP192.168.3.177；认证HTTP同MAC且radio.wifi_state=connected。boot_id与冻结备份相同，固件仍0.18.104-note4，context9c3bd636edcd32e4/commit802保留、data823 displayed。原14条Job逐字段完全不变；没有清历史、猜身份、重发旧写或手动激活。白名单证据 artifacts/wifi-switch-fix/verified.json。

用户在本轮明确确认屏幕Wi-Fi OFF/Bridge OFF图标均已恢复。本次故障闭环，不代表长期RF/其他B6验收完成。实际生产代码修复已在基线内；本轮新增差异仅现场回归初态及交接证据，无wire/固件/设备持久格式变更。本轮改动随本次提交保存。
