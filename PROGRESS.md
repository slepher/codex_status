# Codex Status 当前交接

更新时间：2026-10-16 16:46（中国标准时间；提交记录更新，未追加部署）

## 部署与工作区

- Note4 **7C4FADB93408**：**0.18.104-note4 / ota_0，17:50已OTA部署并认证image_verified**。BIN `artifacts/bridge-macos/note4-0.18.104-epd-reinit.bin`，1,853,856 B，SHA256 `1dabbbc96a8667b0c653d59f5cf6a5c0e28ac625a173fcf3301e6e9fba34a76c`；匹配ELF/map同前缀，.103保留ota_1。正式OTA d1199896一次成功；独立镜像hash与完整认证状态一致。
- Bridge **10月16日11:58已部署**隔离自动恢复/错误提示及发现失败回执：79,575,872 B，SHA256 `466bc079ea937598528e42447f91f4f2f016f870d5207be30a000e1810e07e79`；托管supervisor66915、health204。App/data备份 `~/Library/Application Support/dev.codexstatus.bridge/backups/ble-recovery-20261016-115638`，154文件哈希一致。此前恢复/UI/倒计时已提交 **21a98a5**；隔离恢复/反馈已提交 **e216f54**。

## 活跃事项与下一步

**B6/C8 自动恢复修复已部署，授权后自然恢复已验**：两项权限allowed；14:03 timer wake2104完成Plan4050→认证HTTP proof/sync-v1，独立认证MAC/.104、Bridge reachable、manual/provisional0。14:04显式Sleep4051确认后14:05/14:07自然wake2106/2108连续BLE会合成功，同supervisor66915，无BOOT/刷机/手动重启。9历史Wi-Fi作业哈希不变。隔离后可见error/stage/action、5/30/120/300s退避，由supervisor确认旧child退出后重建；App107/BLE28/回放3/Node/WK68及supervisor负向/正向回归通过。**实机尚未再次触发隔离，自动child重建仍只有故障回归证据**。初次授权会合host读取耗尽RF，下一轮恢复，保留not_sent；11:40原生加密失败719与昨晚服务发现未完成分别记账，不宣称RF根因已消除。[调查/部署/边界](project-workflow/bridge-macos/sleep-resume-disconnect-20261016.md)。

**停15:36/无响应：生产路径SPI自锁已复现并修复，.104已部署（B6/C8）**：网络醒先刷分钟后完整初始化，DEV_Module_Init防重检查位于GPIO_Config之后；SCK被拆总线时重新获取本任务持有的SPI mutex而永等。真实生产/HAL链路red→green、100次重入ASan/UBSan、分钟/模块回归与匹配ELF栈门禁通过，实际.103/.104指令核对。与现场日志/全接口无回应高度吻合，原现场CPU栈未取得，不冒充实机PC证明。dump23620B/CRC有效但属另一构建；17:37工具复位单记，17:39认证.103恢复。候选 `artifacts/bridge-macos/note4-0.18.104-epd-reinit.bin`，1853856B，SHA256 `1dabbbc96a8667b0c653d59f5cf6a5c0e28ac625a173fcf3301e6e9fba34a76c`，ELF/map同前缀；17:50正式OTA及独立完整运行hash通过，修复已提交 **756c30d**；电池深睡跨分钟/网络醒的实机复测仍开放。归档误覆盖旧通用名note4-0.18.*已明示，.103专用证据/dump不受影响。[调查与审阅](project-workflow/bridge-macos/clock-stall-20261015.md)。

**B6/C8 统一预算修复已部署，14:00自然恢复实机通过**：自动恢复/分页/token/history不再外层取消GATT写；carrier取阶段/5s命令/RF最早截止，错误准确分类。token失败停止本轮；history单份2s诊断预算。HTTP快探测用compact，最终proof完整分页共用原期限；claim预检/回退/worker同预算。RPC锁/写/应答统一20s，取消清pending、半帧退役；恢复GET总截止/64KiB上限。310测试通过/3既有忽略，真实慢分页/管道背压/取消/GET慢流覆盖。详见[实施](project-workflow/bridge-macos/timeout-budget-followup-20261015.md)。

**最终现场**：用户完成系统授权，两项均allowed。14:00 timer自然醒1186，MAC/.102核对，Plan4011已确认；SyncOpen/network_start均为完整写后ACK未知，如实保留，再由认证HTTP proof证实恢复，随后sync-v1 complete。14:00:55独立完整认证确认HTTP ready/Bridge reachable、manual/provisional均0，9条历史作业哈希不变。未按BOOT/接USB/刷ROM；此前权限门禁与45s验证失败仅作过程记录，已解除。证据 `artifacts/bridge-macos/timeout-budget-live-20261015.json`、`timeout-budget-natural-recovery-20261015.log`。

**此前已验**：11:38自然timer wake1074存量网络恢复，9作业不变；卡片误报/作业ID/空状态/本地逐秒倒计时已部署并保留。[记录](project-workflow/bridge-macos/existing-network-reconnect-20261015.md)。独立原生连接4s超时及长期RF/多轮实体矩阵未结，B6/C8保持开放。

**独立分钟时钟已部署**：网络醒先刷到期时钟、会合期间1Hz独立检查、深睡取分钟/联系较早绝对截止并扣除清理耗时，成功显示才记分钟。Fake ROM :58会合跨:00先red后green，69全套及固件宿主/栈门禁通过。14:48 OTA任务2a2782f6 succeeded/image_verified，完整运行哈希、MAC、版本/ota_1核对一致。用户14:49明确“时钟已及时更新，不做进一步复测”，已停止追加测量；现场及时更新由用户确认，本轮未取得非整分会合跨界的秒级配对，不宣称量化消除17s。[实施/部署](project-workflow/bridge-macos/independent-clock-20261015.md)。证据 `clock103-ota-verified.json`、`clock103-live-deployed.json`（artifacts/bridge-macos）。

**C9/AP01**：维护核心/工具已提交 **1af830e**；维护C核心宿主/RV32同组故障测试通过，代码3330 B、状态136 B、核心栈链256 B。[构建/指纹](project-workflow/ap01/maintenance-build-20261015.md)；核心ELF `artifacts/ap01-research/maintenance-verified-20261015/maintenance-core.elf`，8928 B，SHA256 `7f6f855525d51f8d6fb5184ef1394e2b27ef98a48ff9ccbe52a7ebb29be389de`，**不是ROM**。现有破解包无原厂完整源码，缺独立任务/HTTP、持久认证、原厂排他/安装/UI适配，rom/service/ui ready均false；保持不额外逆向，无新ROM/刷机，原厂OTA未改。

**C9/REA工具计划（10月16日）**：同父目录`rea`已克隆并核对6.5.0与当前源码；[RISC-V裸BIN计划](project-workflow/ap01/rea-riscv-bin-plan.md)已登记，覆盖准入/profile及AP01基址与入口分离。仅计划，未实现/安装/真实Ghidra验收，不恢复额外逆向。

待办唯一来源：[backlog](docs/roadmap/backlog.md)；[操作细则](docs/agent-operations.md)；[此前快照](docs/history/progress-archive-2026-10-15.md)。
