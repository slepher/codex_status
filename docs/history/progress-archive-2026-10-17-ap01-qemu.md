# Codex Status 当前交接

更新时间：2026-10-17（AP01 QEMU原件运行及独立OTA续作；实机与Bridge部署未变）

## 部署与工作区

- Note4 **7C4FADB93408**：**0.18.104-note4 / ota_0，17:50已OTA部署并认证image_verified**。BIN `artifacts/bridge-macos/note4-0.18.104-epd-reinit.bin`，1,853,856 B，SHA256 `1dabbbc96a8667b0c653d59f5cf6a5c0e28ac625a173fcf3301e6e9fba34a76c`；匹配ELF/map同前缀，.103保留ota_1。正式OTA d1199896一次成功；独立镜像hash与完整认证状态一致。
- Bridge **10月16日11:58已部署**隔离自动恢复/错误提示及发现失败回执：79,575,872 B，SHA256 `466bc079ea937598528e42447f91f4f2f016f870d5207be30a000e1810e07e79`；托管supervisor66915、health204。App/data备份 `~/Library/Application Support/dev.codexstatus.bridge/backups/ble-recovery-20261016-115638`，154文件哈希一致。此前恢复/UI/倒计时已提交 **21a98a5**；隔离恢复/反馈已提交 **e216f54**。

## 活跃事项与下一步

**C9/AP01 QEMU→独立OTA续作**：原件125在必要Mock中连续观察50宿主秒/8628虚拟ms，无guest异常，原生MiIO/LVGL回调运行；LCD仍为Logo，未验完整主界面。独立HTTP原生200/401/EOF通过；127已实际安装B005完整6769704B，原件CRC/ff13提交0及payload复位，128独立核对分区age/切槽/逐字镜像/旧槽与Boot2保护通过，129 B复启、130 B→A回刷及132 A复启后OTA均通过，模拟闭环完成。A004 SHA `8570f4cd0ffe631802be16da3b11d27683fcfe6d5c0c9b24f4e360d3dca539a6`，B005 `4f71fd731f45c9c45cc1d010e8d2326cbfdfd1bc136df8f8f6073f2dd03ed806`（均artifacts/ap01-research/qemu-0041-20261017/ota-channel-*.bin，模拟候选非部署）。最终引擎7246b77a…18b9d；DMA6/PWM3/NOR三profile通过。网络为payload限定loopback、无物理Wi-Fi/Boot2等价保证；031无备份，不刷实机。[闭环验收/复现](project-workflow/ap01/qemu-ota-final-20261017.md)、[硬件模型](project-workflow/ap01/qemu-rtc-display-20261017.md)。

**C9/AP01转向保留原厂后台＋Patch，QEMU已执行0041，整ROM未启动**：固定QEMU10.1.0最小machine/INSB及有限功能模型落地，18项回归通过；原件执行至RAM `0x62fc20b6`，OTP触发写`0x20056800`明确拒绝，64 B匹配原件。入口12 B范围Patch＋44 B追加代码的非零寄存器对照通过；候选仅QEMU用、不可安装。冻结引擎SHA256 `ddd44e11b2d2eb4b9b5d59c628943ad5439c7a8b383772b2f00d64aa2ed6cf90`，产物/报告在`artifacts/ap01-research/qemu-0041-20261017/`。下一步OTP事务/显式fixture，再补中断/Flash等；NuttX/原厂UI/独立App未运行，完整MiIO/MiBLE恢复不再是接入前置。无设备/Bridge改动、无刷机。[复现与边界](project-workflow/ap01/qemu-0041-runtime-20261017.md)。

**C9/AP01 0041接手入口**：[当前交接](project-workflow/ap01/0041-handoff.md)。SIM已验收；私有no-return/T-Head模型387检查/286次p-code执行通过。本轮REA＋GPT-6.1 Sol补齐MiIO全部162已知根的局部实现审阅，90命令关联合同；MiBLE新增传输/计数器/认证等9份合同，同profile采集315函数。537个直接依赖、161处未定向调用及原浮点ABI仍开放，无完整可编译源码。[上层发现/证据/下一步](project-workflow/ap01/upper-layer-0041-20261017.md)。官方SDK已克隆，确认E907/MHINT；原件、实机、Bridge未改。

**B6/C8 自动恢复修复已部署，授权后自然恢复已验**：两项权限allowed；14:03 timer wake2104完成Plan4050→认证HTTP proof/sync-v1，独立认证MAC/.104、Bridge reachable、manual/provisional0。14:04显式Sleep4051确认后14:05/14:07自然wake2106/2108连续BLE会合成功，同supervisor66915，无BOOT/刷机/手动重启。9历史Wi-Fi作业哈希不变。隔离后可见error/stage/action、5/30/120/300s退避，由supervisor确认旧child退出后重建；App107/BLE28/回放3/Node/WK68及supervisor负向/正向回归通过。**实机尚未再次触发隔离，自动child重建仍只有故障回归证据**。初次授权会合host读取耗尽RF，下一轮恢复，保留not_sent；11:40原生加密失败719与昨晚服务发现未完成分别记账，不宣称RF根因已消除。[调查/部署/边界](project-workflow/bridge-macos/sleep-resume-disconnect-20261016.md)。

**停15:36/无响应：生产路径SPI自锁已复现并修复，.104已部署（B6/C8）**：网络醒先刷分钟后完整初始化，DEV_Module_Init防重检查位于GPIO_Config之后；SCK被拆总线时重新获取本任务持有的SPI mutex而永等。真实生产/HAL链路red→green、100次重入ASan/UBSan、分钟/模块回归与匹配ELF栈门禁通过，实际.103/.104指令核对。与现场日志/全接口无回应高度吻合，原现场CPU栈未取得，不冒充实机PC证明。dump23620B/CRC有效但属另一构建；17:37工具复位单记，17:39认证.103恢复。候选 `artifacts/bridge-macos/note4-0.18.104-epd-reinit.bin`，1853856B，SHA256 `1dabbbc96a8667b0c653d59f5cf6a5c0e28ac625a173fcf3301e6e9fba34a76c`，ELF/map同前缀；17:50正式OTA及独立完整运行hash通过，修复已提交 **756c30d**；电池深睡跨分钟/网络醒的实机复测仍开放。归档误覆盖旧通用名note4-0.18.*已明示，.103专用证据/dump不受影响。[调查与审阅](project-workflow/bridge-macos/clock-stall-20261015.md)。

**B6/C8 统一预算修复已部署，14:00自然恢复实机通过**：自动恢复/分页/token/history不再外层取消GATT写；carrier取阶段/5s命令/RF最早截止，错误准确分类。token失败停止本轮；history单份2s诊断预算。HTTP快探测用compact，最终proof完整分页共用原期限；claim预检/回退/worker同预算。RPC锁/写/应答统一20s，取消清pending、半帧退役；恢复GET总截止/64KiB上限。310测试通过/3既有忽略，真实慢分页/管道背压/取消/GET慢流覆盖。详见[实施](project-workflow/bridge-macos/timeout-budget-followup-20261015.md)。

**最终现场**：用户完成系统授权，两项均allowed。14:00 timer自然醒1186，MAC/.102核对，Plan4011已确认；SyncOpen/network_start均为完整写后ACK未知，如实保留，再由认证HTTP proof证实恢复，随后sync-v1 complete。14:00:55独立完整认证确认HTTP ready/Bridge reachable、manual/provisional均0，9条历史作业哈希不变。未按BOOT/接USB/刷ROM；此前权限门禁与45s验证失败仅作过程记录，已解除。证据 `artifacts/bridge-macos/timeout-budget-live-20261015.json`、`timeout-budget-natural-recovery-20261015.log`。

**此前已验**：11:38自然timer wake1074存量网络恢复，9作业不变；卡片误报/作业ID/空状态/本地逐秒倒计时已部署并保留。[记录](project-workflow/bridge-macos/existing-network-reconnect-20261015.md)。独立原生连接4s超时及长期RF/多轮实体矩阵未结，B6/C8保持开放。

**独立分钟时钟已部署**：网络醒先刷到期时钟、会合期间1Hz独立检查、深睡取分钟/联系较早绝对截止并扣除清理耗时，成功显示才记分钟。Fake ROM :58会合跨:00先red后green，69全套及固件宿主/栈门禁通过。14:48 OTA任务2a2782f6 succeeded/image_verified，完整运行哈希、MAC、版本/ota_1核对一致。用户14:49明确“时钟已及时更新，不做进一步复测”，已停止追加测量；现场及时更新由用户确认，本轮未取得非整分会合跨界的秒级配对，不宣称量化消除17s。[实施/部署](project-workflow/bridge-macos/independent-clock-20261015.md)。证据 `clock103-ota-verified.json`、`clock103-live-deployed.json`（artifacts/bridge-macos）。

**C9/AP01**：维护核心/工具已提交 **1af830e**；维护C核心宿主/RV32同组故障测试通过，代码3330 B、状态136 B、核心栈链256 B。[构建/指纹](project-workflow/ap01/maintenance-build-20261015.md)；核心ELF `artifacts/ap01-research/maintenance-verified-20261015/maintenance-core.elf`，8928 B，SHA256 `7f6f855525d51f8d6fb5184ef1394e2b27ef98a48ff9ccbe52a7ebb29be389de`，**不是ROM**。现有破解包无原厂完整源码，缺独立任务/HTTP、持久认证、原厂排他/安装/UI适配，rom/service/ui ready均false；保持不额外逆向，无新ROM/刷机，原厂OTA未改。

**C9/REA工具（10月16日）**：同父目录`rea`，基于6.5.0创建`codex/riscv-raw-adapter-6.5.0`；RV32/RV64裸BIN与base/entry分离已实现，真实CLI/MCP及旧ARM/x86回归通过，确定性110项通过。AP01完整镜像映射verified，已知入口可读，但jalr目标引用未自动恢复；Windows原生支持随后补齐，RV32/RV64真实CLI/MCP、110项回归与native ACL/Job验收通过，本轮无WSL。未提交/推送/全局安装，无现场改动。[实施、证据与边界](project-workflow/ap01/rea-riscv-implementation-20261016.md)。

待办唯一来源：[backlog](docs/roadmap/backlog.md)；[操作细则](docs/agent-operations.md)；[此前快照](docs/history/progress-archive-2026-10-15.md)。

**C9/AP01 0041应用SIM完成验收**：GPT-6.1 Sol逆向，33构造器/7组额外业务/25服务已闭环；51个widget及79个selection保留未观察空洞。55套ROM字体，最终502步/99截图、WSLg30步/15截图、服务147项ASan/UBSan及81772天农历通过。候选宿主ELF `artifacts/ap01-research/rehost-20261016/ap01-ui-0041-complete-sim.elf`，14936232 B，SHA256 `ac23b8b3b64b6959f36819f3e9824fb287729c00c4803c419964c12424a930d1`；未刷实机/改Bridge，C9硬件与维护集成仍开放。[验收/矩阵/产物](project-workflow/ap01/application-rehost-complete-0041-20261017.md)。

**C9/0041新增OTA拒绝离线候选**：按用户明确要求，仅改新增unsigned拒绝分支2 B并重算恢复CRC4 B；`artifacts/ap01-research/rehost-20261016/ap01-1.0.2_0041-unsigned-ota.bin`，6769704 B，SHA256 `1bbe8f45359eb7cdb55f337643f3ce28894999ffb21beab92fb62e995cd4854d`。8项回归含独立RV32执行通过，未刷写/验证首次安装或完整OTA。不使当前原厂0041自动接受该候选。原件/设备/Bridge未改，C9开放。部分硬件模型不作为应用重建前置，[先前探测](project-workflow/ap01/full-rom-runtime-20261016.md)。

**C9 REA Windows补齐**：6.5.0分支原生RV32/RV64裸BIN真实CLI/MCP、110项回归及native ACL/Job conformance通过，本轮无WSL；未提交/改设备现场。随后按用户要求将Codex MCP切换到自建版并移除旧全局6.4.0，实际握手/工具调用通过，需重启Codex连接新入口。[验收与入口](project-workflow/ap01/rea-windows-riscv-20261016.md)。
