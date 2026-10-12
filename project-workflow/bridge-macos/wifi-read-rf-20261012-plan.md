# Wi-Fi 读取等待通道：计划、任务、状态与审查

关联 B6/C8。用户报告 2026-10-12 10:01:08 读取任务持续等待，要求沿当前网络同步调查继续处理。

## 计划与任务

1. MCP 核对原任务、认证 BLE 联系和 HTTP 可达性，日志只摘状态，不保存凭据。
2. 用现有 Fake BLE 生产 ble_cycle 路径复现首次 INFO 射频截止在 Plan 后失效、完整读取未发送。
3. 最小修复复用 DeviceConnection::refresh_info，仅同 MAC/同 wake 的设备报告更新截止；共享预算仍有界。
4. 针对性回归及 bridge-app/bridge-ble 测试，记录源码候选与运行现场的边界。

## 初始状态与证据

- 原读取 job `768642b2741be4e4`，缓存 revision=4/2026-10-05；2026-10-12 10:03 MCP 显示 3 次尝试、http_status_unavailable。
- 同时认证 BLE 可联系、wifi_control.read_allowed=true，设备 HTTP ready=false/ip=-，电脑 Xiaomi_1EBB。
- 运行日志 10:02:18 INFO 身份已验证；10:02:20 Plan accepted_remaining_s=211；wifi_status 分页成功；10:02:21 wifi_snapshot 只出现 characteristic found，无 written。10:04 再次出现同样模式。
- 最新原任务出现 transport_unavailable，仍未完成；这证明 BLE 路径参与，但确切拒绝点尚待测试。
- 工作区此前 PROGRESS 和三份交接记录改动保留。未改变 Bridge 实例、网络配置、ROM 或模板。

## 状态

新增生产 ble_cycle 回归已 red→green：短 INFO 截止 700ms，Plan 后设备报告 30s，分页状态读耗时超过初始截止；旧实现等待 transport_unavailable，新实现完整读 revision=8 成功。设备未延长 RF 的对照保持未完成且不发送 wifi_snapshot。

生产改动只在 Plan/SyncOpen 后复用 refresh_info，1s 上限且受共享绝对预算约束；MAC/wake 不符或刷新失败即有界关闭并返回 ble_info_refresh_failed。

首轮并行应用回归 84 passed/1 failed/3 ignored；失败是旧 recovery_timeout 用例的 120ms 预算在并行负载下于 wifi_save 发送前耗尽（sent=false），正在串行复核。未改该用例/生产超时。

串行完整复跑通过：bridge-app 85 passed/3 ignored，bridge-ble 17 passed；原 recovery_timeout 通过。忽略项要求指定 device-sim/私有 report IP，本轮未以宿主回归宣称这些验收完成。git diff --check 通过，生产仅 8 行行为改动，无固件/UI修改。

2026-10-12 10:15:37 默认 LaunchAgent/同路径子进程已退出，data/installation-id 私有备份并核对 state 哈希；候选 release 构建中。部署结构化记录和构建日志在 artifacts/bridge-macos/wifi-read-20261012-{deployment.json,build.log}，不含凭据。

10:19:38 候选 release（3m17s）部署到原默认 App；x86_64/20,796,112 B，exe SHA256 `7171cf74c2ff9e5043957ecd5aebba2881b69d8b72809f4ad2071ad642d3fb9b`，候选/部署相同、签名通过。supervisor90675/child90682，health204/MCP43工具；安装身份未变，旧 App 与私有数据备份见结构化部署记录。

部署后系统 TCC 对新签名重新返回 bluetooth/location=not_determined；用户通过设备页显式授权后，10:21:36 MCP 确认双 allowed、电脑 Xiaomi_1EBB。BLE 扫描已恢复；原 read job 仍保留，等待真实会合验收。

## 现场扩展与最终候选

10:22:03 原设备同 MAC/wake_seq=638 重新认证，Plan3659/600s 已 ACK；SyncOpen 在首次 INFO 截止下提前 ACK timeout。首轮修复后的 INFO 读取成功，但后续 wifi_status ACK 又超时，原 read 尚未完成。这是首轮实机未验收的明确证据，不宣称已解决全部故障。

扩展生产回归加入慢 SyncOpen：首轮候选 red（read仍queued）；提前到 Plan ACK 后刷新得到 green。最终候选还允许确认 Plan、且 SyncOpen 明确 ACK 丢失（整帧已发送）时的原只读 job 继续走 BLE snapshot，跳过冗余 status 分页；写入、未知Plan及未知分片发送不能借此旁路。发送阶段不能证明整帧完成时关闭链路，保持未知结果并不再发送后续BLE命令。

Fake覆盖正常ACK、丢ACK、不确定写入和RF未延长；原SyncOpen各只发送一次，未知写入不读取/不发后续命令并释放链路。最终串行完整回归通过：App85 passed/3 ignored，BLE17 passed；日志 artifacts/bridge-macos/wifi-read-20261012-tests-final.log。四种生产 ble_cycle 回归全部通过，未改生产超时；git diff --check通过。第二轮默认实例已于10:38:55停止，私有data/身份备份读回校验通过。部署记录 artifacts/bridge-macos/wifi-read-20261012-final-deployment.json。

## 审查边界

不得伪造或无条件延长 RF 截止；只允许设备实际报告的同唤醒阶段变化。宿主 Fake 成功不能代替现场读取成功；部署前保留 data，现场未验则写未知。

## 最终部署与现场核验

10:44:26 最终release构建部署（4m07s），x86_64/20,804,352 B，SHA256 `d1fd0fda57a813cd95870fa5c889138b5be8eec3b612fe3bf6c8685c87cd58b1`；候选/默认App哈希一致、严格签名通过，supervisor1655/child1662，health204/MCP43工具，安装身份一致，回滚App/私有data备份均保留。构建/部署证据 artifacts/bridge-macos/wifi-read-20261012-{build-final.log,final-deployment.json}。

部署后MCP发现现场在本轮测试期间已变化：原读取768642b2741be4e4被取消（10:24:56）；另一个486e48e4b974defa于10:25:10完整读取r234完成，后续recovery_save、activate完成到r237，sync ed3884afab4a992c以already_current完成。这些发生在最终部署前，不能归因于最终修复；本轮没有主动创建上述任务、取消原任务或提交Wi-Fi写入。当前缓存r234/observed_at=0/complete=false，不能宣称方案完整最新。10:44:57认证缓存设备HTTP=192.168.31.90/ready，10:45主动认证读取超时。最终App签名再次导致蓝牙/定位not_determined，已请用户系统授权；最终BLE实机读取验收待权限恢复。

## 10:51 最终实机验收通过

用户再次系统授权后，10:49:21 MCP确认蓝牙/定位均allowed、电脑Xiaomi_1EBB；已部署最终App通过BLE正常Plan/Data ACK。同一设备无新Wi-Fi配置提交，本轮新建一次只读job e0c0b00d10f96924（旧job已取消），首次HTTP不可用后第二次BLE完成完整snapshot r237，error=null/complete_snapshot。10:50:12 Plan3663已ACK，同wake_seq644 SyncOpen applied；随后15页wifi_snapshot全部written/ACK，10:50:30 BLE会合完成，证明最终RF窗口修复真实读取通过。

后台已有自动流程随后activate完成r238（job4e1712e7046269fb，本轮未显式调用写入）。10:51:10 MCP platform_status_refresh返回ok，认证HTTP=192.168.31.90/ready，active=office（Xiaomi_1EBB）。为读回启用后最新修订，再次HTTP只读job4fe306d4227b0a3f在第一次尝试完成，完整r238/observed_at=1791773495、error=null，权限双allowed/automatic_state=connected。安全证据 artifacts/bridge-macos/wifi-read-20261012-final-{mcp-safe.json,http-safe.json}，不含密码/token。

本次读取故障最终Bridge实机验收完成，B6/C8长期RF/功耗及其他实体矩阵仍开放；未提交、未刷ROM或发布Bundle。

## 用户补充：10:09–10:10 退出zzz后约一分钟联网

用户于最终权限部署后补充旧现场，明确发生时间为10:09–10:10，随后Wi-Fi连接成功并进入light-sleep；不能归于11:08新App行为。旧日志10:09:11 Plan3657已ACK、remaining415s，10:09:12 wifi_status ACK失败；network_start只有characteristic found，没有written/ACK，10:09:41本轮HTTP交接失败。10:10:42同wake_seq632再次联系，10:10:44同Plan剩余323s，随后同样network_start未记录发送、HTTP超时。此路径也受旧INFO RF截止影响，与本轮已复现修复的实现缺陷吻合；协议已具备独立启动/就绪报告/认证回读，暂无新增协议缺口证据。

Bridge当时持续HTTP超时，不等同于设备未连路由器；用户观察到约一分钟后Wi-Fi成功记为用户实机证据，具体关联/DHCP/设备fallback时间无当时设备日志，根因不能进一步定死。zzz退出/计划ACK不证明Wi-Fi或HTTP已就绪。证据 artifacts/bridge-macos/wake-network-20261012-1009-safe.log。11:10当前认证HTTP正常与此旧事件分开记账。未因此改ROM/协议、重发历史事务或改变电源计划。
