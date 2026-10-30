# BLE claim 网络准入状态

2026-10-30 22:09，Asia/Hong_Kong：Windows已按用户授权重建部署；Mac构建和验收留到Mac处理。

- 共享BLE claim在发送前检查新鲜宿主网络及设备映射、活动LAN地址、该地址上的HTTP health；最多2秒且受原命令预算约束。创建/续租共用；不要求公网可达。失败明确claim_not_sent/host_network及错误码/操作提示，不发claim和后续Plan/Data，不续约。
- Windows复用原LAN枚举并核对GetIfEntry operational；macOS增加SystemConfiguration Link.Active读数，配合UP/RUNNING及地址检查，避免仅凭缓存IP/有线族配置准入。health禁代理/跳转，成功后复读宿主网络；claim使用检查过的地址。未改变设备owner鉴权、租约时长、未知事务或OTA。
- 生产ble_cycle新增24场景（创建/续租×12）：无Wi-Fi、权限不可用、歧义、宿主不可用、缺映射、无有效地址、HTTP错误/超时、检查途中掉网、有线配置但无LAN、有线健康、Wi-Fi健康。异常断言未发claim/Plan/Data且锁释放；健康断言只发一次claim且使用已检查地址。
- 红：暂时撤掉准入，no_wifi场景仍返回成功，新测试失败，随后finally恢复修复。首轮扩展有线夹具缺revision导致1失败，已修夹具。最终app/core/mcp共342通过、0失败、5条件忽略，git diff --check通过；既有btleplug生命周期、测试unused/dead-code警告保留。
- 验收边界：Windows编译/共享逻辑及下述正常网络实机链路通过；未编译macOS原生代码、未做同机休眠/恢复/拔线及双Bridge实体故障回归。Mac实际exe版本和首次target mismatch具体字段尚未核对；未刷ROM。

证据：artifacts/wifi-families/ble-claim-network-{red,regression,final}.log；现场调查见connection-incident-20261030.md；代码/边界见ble-claim-network-review.md。剩余Mac构建部署/实机项归B6。

## Windows部署与验收

- 22:05核对默认exe/8765+8766归属PID5216、watchdog21552；任务Ready但残留主进程在监听。按watchdog→任务→核实残留主进程顺序停止。旧exe及346文件data完整备份并逐文件SHA256核对；停止期间data再次核对无变化。
- 从当前含修复的工作区在bridge/运行cargo build -p bridge-app，沿用隔离target-note4-sync，26.15秒成功。HEAD ff9942c5782034676365cab5a11a58eddf75d173+dirty；此前342回归/5条件忽略适用，无追加源码改动。
- 新exe 39059456 B，SHA256 5f5eae51d3cdc122c00b4118c27a688e1fec33d006a931a311f29f0dc8f8bcbb；替换前后哈希一致。旧exe SHA256 8de4dba546326be09303825ea1a28c868749479c0935ab0f3319ecd039e8309c。使用提权tools/start-bridge.ps1启动计划任务，22:07:04主PID21156/watchdog47076，任务Running、health204、MCP44。
- 3设备身份/名称/Profile/同步许可、共享Wi-Fi库保持一致，4个Bridge配置/凭据文件逐字节保持不变。
- 22:08:03 Note4自然BLE会合核对MAC 7C4FADB93408/wake7141；22:08:04新网络准入成功后claim applied，22:08:05 Data1329/Plan8929 applied，22:08:15认证HTTP ready，22:08:23诊断complete。独立认证同MAC .107-note4、owner8c94、wifi_connected/bridge_reachable=true、error=null、Data1329已显示、content_error空。最初HTTP读取超时不作为设备故障结论。
- 本轮没有断开主机网络来注入故障；离线拒绝证据来自24场景Fake回归。Mac构建/部署/原生验收依用户指示留到Mac。

私有回退备份及验收：artifacts/wifi-families/ble-claim-network-deploy-20261030-2205/（bridge-app.exe、data、data-manifest.json、deployment.json、verification.json、configuration-check.json、note4-status-after-rendezvous.json）；构建日志ble-claim-network-build.log。