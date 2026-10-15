# 存量网络重连与冲突写入分离（B6/C8，2026-10-15）

## Plan

用户授权修复、回归、部署Bridge及无BOOT恢复验收。仅在设备认证wifi_match证明matches=true/active=true/有效version，且本机无密码replace意图时，重连当前存量配置不创建activate作业；由既有已确认Plan的network_start恢复运行链路。inactive/mismatch/replace保持事务围栏；旧未知作业原样保留，不清理或补造confirmation。认证错误、错误身份/owner不能放宽。

## Task / status

- [x] 真实ble_cycle回归先红：取消/过期但提交未知历史均不能挡已核对active网络，Plan→network_start→认证HTTP，并证明无新增配置作业/旧结果不变。
- [x] 对照：inactive/unknown-active、metadata mismatch、password replace及认证失败仍拒绝冲突写；失败不能假装准备成功继续开网。
- [x] 最小实现与相关Rust回归/审查。
- [x] 备份App/data、核验、托管停止/构建/部署Bridge；不改固件。
- [x] 电池自然会合与认证HTTP核验，无BOOT；更新交接及证据。

## Review

已完成存量网络恢复及实机验收。原11:09写失败原因尚未知，未知写隔离保护不放宽；本次部署通过重建托管进程结束旧原生实例，而非清内存标志。

## 实施与证据

生产Fake ble_cycle先红：已匹配active网络+已取消/提交未知历史，返回wifi_confirmation_pending，只有status/match/Plan/sync而无network_start。修复后通过同入口进入network_start与认证HTTP；历史job序列化逐字节保持、无wifi_activate/save/apply/trial。最终回归扩成连续两次真实掉载体/会合。

反例先红：metadata mismatch时原queue_current_recovery吞掉wifi_confirmation_pending并返回Ok。改为Result透传到准备/自动恢复/显式use_current调用者；配置不匹配不能再作为成功准备继续开网。新增inactive、active缺失、version未知、密码替换、authentication_failed对照。

仅自动恢复的已匹配active分支免新activate，不修改core Job::confirmation_pending/queue围栏。清除的只是Connection中不适用当前运行重连的wifi_confirmation_pending展示错误，不修改旧Job。重新核对host SSID；Plan/owner/session/MAC/预算与network_start原鉴权不变。本轮未新增协议字段，不改固件。

红证据：existing-network-red-20261015.log、existing-network-conflicts-red-20261015.log（后者修正夹具为生产wifi_expire入口后确认metadata吞错）。最小绿：existing-network-green-20261015.log。完整回归App98/BLE27/core153+22集成，共300通过/3既有忽略（existing-network-regression-20261015.log）；新增两次真实ble_cycle重连及6类反例通过。

## 部署与自然恢复验收

11:35首版Bridge部署：79,438,992 B，SHA256 `3a02d27d9dc0e1e5e220329e763ed63cc96ccf7f9f86cc9f4475869f81d05838`。App/data备份 `~/Library/Application Support/dev.codexstatus.bridge/backups/existing-network-20261015-113439`，153文件哈希一致。部署元数据 `artifacts/bridge-macos/existing-network-deployed-20261015.json`。

首轮11:36发生独立的原生连接4s超时，尚未进入本次修复分支；保留原生隔离保护。11:36:45重启已核对的托管Bridge，结束旧原生实例（未清未知写标志）。11:38自然timer wake1074，11:38:09.776528出现existing_network_reconnect，Plan4005/SyncOpen已确认，network_start写入81ms、ACK135ms，随后ready_report与认证HTTP proof成功。11:39完整认证状态确认MAC/.102、timer/reset8、HTTP ready/Bridge reachable、manual/provisional均0；原9条作业哈希全部不变，无新增配置作业。用户确认设备已成功联系。未按BOOT、未连接USB、未刷固件。

证据：`artifacts/bridge-macos/existing-network-natural-recovery-20261015.json`；首轮独立连接超时 `existing-network-first-connect-timeout-20261015.log`。长期RF/多轮实体恢复矩阵仍开放，不能把一次成功扩大为全部原生连接问题已解决。

## 卡片与任务空状态修正

用户截图11:38显示“待处理1项”，Wi-Fi仅9条历史无当前任务。只读MCP确认attention为firmware_upgrade：登记发布.61低于运行.102，但原投影把任何版本差异当升级。修为同系列数值版本比较；运行更高不提示升级，不可比较版本明确需核对。设备卡直接显示提示类型；当前作业显示MAC/ID；静态“等待操作”改为“操作结果将在此显示”，避免冒充排队。

版本投影3项、Node设备页回归通过，原生WKWebView66场景通过，560/1000宽截图已人工核对任务ID及布局。原生合成IPC仅证明UI，不代替前述实机链路。证据 `existing-network-version-tests-20261015.log`、`existing-network-ui-node-20261015.log`、`existing-network-ui-wk-20261015.log`、`artifacts/bridge-macos/wifi-ui/{560,1000}-waiting.png`。最终含UI Bridge已于11:45部署，79,459,600 B，SHA256 `52601c2f0ea34d90933d7bc85d4760e6b69cab200c8a25bcef527df51cab784f`，supervisor33114、health204。153份数据文件哈希核验；备份 `~/Library/Application Support/dev.codexstatus.bridge/backups/existing-network-ui-20261015-114439`。最终只读MCP投影attention=[]、newer_than_release，认证HTTP仍为wake1074/Plan4005、Bridge reachable、manual/provisional0；9条历史作业哈希仍一致。

最终证据：`artifacts/bridge-macos/existing-network-ui-deployed-20261015.json`、`existing-network-ui-final-view-20261015.json`、`existing-network-ui-authenticated-20261015.json`；自然恢复日志摘录 `existing-network-live-recovery-20261015.log`。本轮不提交，未触碰固件或设备控制线。

## 后续：每秒本地倒计时

用户要求在线窗口剩余秒数逐秒递减。复用最近认证采样和原有observed_at计算；新增可见设备页1s本地绘制，不改变原3s缓存刷新频率，不增加IPC/HTTP/BLE。切换设备清空采样；超过30s仍显示待确认，归零不越界。回归覆盖112→111、延迟恢复106、30s过期、切设备清空及零新增IPC；执行Node/WK、窄宽截图审阅后打包部署。

逐秒倒计时已完成并于11:49部署：79,459,600 B，SHA256 `64ec3b64f72378f6dd5046b77c821b3e54f2485abcae3d0c0f32202db5079a50`，supervisor33701/health204。备份 `~/Library/Application Support/dev.codexstatus.bridge/backups/countdown-20261015-114835`，153份数据文件哈希一致。Node通过（另含归零/隐藏窗口），WK66通过，560/1000 online-boot截图已审阅；Tauri Brotli压缩生成HTML已解码确认1s定时器及切设备清空，且该压缩资产确实内嵌于部署exe。证据 `artifacts/bridge-macos/countdown-{node,wk,build}-20261015.log`、`countdown-{deployed,embedded}-20261015.json`。仅UI变化，无固件修改，无设备通信负载测量或功耗定量结论。
