# 状态

2026-10-27 21:41，Asia/Hong_Kong：共享判定修复完成，默认Windows Bridge已重建、部署并重启，PID13576/8765+8766/按需任务归属核对通过。

- 删除`plan_for_rendezvous_network_work`及HTTP需求布尔参数，所有调用合并到`plan_for_rendezvous`。已保存/过期连接观察不再授予Light；显式requested、真实Wi-Fi任务、Data变化、OTA、BOOT及有效计划仍按原规则处理。诊断复用原SyncOpen临时窗口。
- 新生产ble_cycle回归先在旧逻辑得到Light120而失败；修复后新旧Plan激活路径都为Sleep0，SyncOpen恰好一次、无Wi-Fi任务，HTTP诊断完成。Fake HTTP按实际收到的Plan返回power，原读页先于诊断complete的断言仍在对应组合测试中。
- core/app/mcp回归341通过、5条件忽略；最终恢复Fake原有BLE关闭后联网顺序后，纯诊断回归和真实push同轮激活两项再次通过。`git diff --check`通过。未改UI，无固件构建/刷机。既有btleplug生命周期和测试unused/dead-code警告保留。
- 停核实的默认watchdog→任务→残留主进程后，旧exe与315文件data完整备份并SHA256逐文件核对；停止期间隔离构建成功后替换默认exe，通过`tools/start-bridge.ps1`计划任务启动。旧SHA256 `0b8b843414d97904fb21dac3f341bfc269edcc147d42e759e6e4039fd4899a47`；新39038976 B，SHA256 `8de4dba546326be09303825ea1a28c868749479c0935ab0f3319ecd039e8309c`。health204/MCP44。
- 3设备名称/Profile/同步许可、共享Wi-Fi库和每设备网络目标保持一致，4个凭据/Bridge配置文件逐字节一致。Note4重启桥后认证`.107-note4`、content ready/error空/bridge_reachable，Data1243获ACK并进入原600秒业务Light；这不作为纯诊断验收。书桌屏21:40:38认证HTTP记录仍为`.107-bw`、bridge_reachable/error空；21:40:46诊断complete，21:41:53自然BLE为idle/no HTTP/no diagnostics并确认Sleep Plan1701/remaining0。显式HTTP读取曾超时，未据此判定睡眠；后续Sleep以BLE ACK为证。

证据：`artifacts/wifi-families/activation-fix-{red,green,rust,final-regression,build}.log`、`activation-fix-{deployment,verification,device-readback}.json`。完整私有备份目录由deployment.json登记。纯诊断实机自然周期与长期RF/功耗边界仍归B6/A9，不宣称宿主回归代替实机。
