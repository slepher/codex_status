# Note4 时间停在15:36（2026-10-15）

## Plan
先保留网络/Bridge现场，再通过已核对身份的USB原生只读监听区分供电、唤醒停止、panic/阻塞。未取得设备反馈前不猜定根因、不改ROM。

## Tasks
- [x] 核对交接、登记MAC/IP和Bridge最后成功通信
- [x] 绕过代理只读探测、保存停滞前后Bridge日志
- [x] 接USB后枚举VID/PID/serial并核对登记MAC；保现场监听与后续授权dump工具复位分开记账
- [x] 生产代码/HAL边界确定性复现、最小修复及宿主/实际ELF验证；dump已由用户明确授权

## Status
用户报告屏幕停在15:36，确认设备在Mac附近、使用电池。17:27本机Bridge /health响应；绕过HTTP_PROXY直连登记192.168.31.90的/status.json、/pmstats、/log均Host is down。Bridge持续扫描可见其他BLE设备，目标候选为0。当前无Note4 USB串口，无法取得设备当前boot/uptime/reset_reason。

MAC7C4FADB93408；最后完整HTTP认证15:28:07，.103-note4/ota_1，reset_reason8，电池52（仅历史样本，不代表当前供电）。最后BLE状态ACK15:34:58.996，wake_generation14040343/wake_seq43；最后Plan4044于15:34:59.499确认applied，15:34:59.509会合完成。state.json observed仍为此前状态/Plan4043，故精确最后通信以日志为准。之后未见目标候选，不能将缓存当实时状态。

证据：artifacts/bridge-macos/clock-stall-1530-1550-bridge.log。未重启Bridge、未按键、未刷ROM、未读取dump。

## Review / ranked hypotheses
1. 睡眠/唤醒或阻塞使通信与分钟显示一同停止：USB监听/恢复后的boot与诊断可检验。
2. 供电中断或电池异常：接USB后的供电/启动及reset文本可检验；历史52%不能排除。
3. 设备仍运行而时钟显示及RF共同异常：串口有持续运行/分钟日志、独立认证时钟与屏幕对照可检验。

当前只复现目标失联，屏幕15:36来自用户目视，未取得稳定可运行的原始停钟反馈循环，不进入代码修复阶段。不宣称panic、掉电或.103时钟算法已确认有错。接USB可能改变唤醒/供电现场，前后分开记账。B6/C8保持开放。

## 17:29起USB只读诊断
用户已接USB；system_profiler核对Espressif VID303a/PID1001，serial7C:4F:AD:B9:34:08，/dev/cu.usbmodem14101，与登记MAC一致。打开前/status.json与/pmstats均3s超时，当前boot/uptime/reset未知。

17:29:47–17:30:32原生os.open(O_RDONLY|O_NOCTTY|O_NONBLOCK)监听45s，不调termios/控制线ioctl，不写命令/不清缓冲，共140B：baseline cache restored、clock tick total763127us、GPIO sleep retention17/6/42。原文无时间戳，用户确认屏幕仍15:36，不能证明接USB后产生新刷新，按残留日志处理。

17:30:44–17:31:19原生O_RDWR发送一次只读status\n（7B），监听35s零响应；没有DTR/RTS/主动复位或下载模式。后续继续只读监听，BOOT短按观察请求已向用户发出，尚不能假定用户已执行。

证据artifacts/bridge-macos/clock-stall-usb-{passive,status}-20261015.{json,raw.log}。无ROM reset/panic文本；不能以日志沉默证明无复位，也不能确认死锁具体位置。串口收到三行证明此USB读通道至少可收到数据，主循环当前无status应答仍待进一步取证。

17:32:07–17:33:02继续原生只读监听55s，零字节。17:32:53经独立原生写描述符发只读log\n（4B），至监听结束无回传；不改termios/modem控制线。该次实际发送log，非仅等待自动日志。用户尚未确认BOOT短按，不能将本次沉默记作按键无效。证据clock-stall-usb-boot-20261015.{json,raw.log}。

## 17:37用户授权检查设备crashdump
用户确认BOOT短按屏幕无变化；按键后15s只读log无回应、HTTP Host is down。随后用户明确要求检查dump，允许按既定只读导出流程执行；操作前已说明进入下载模式/结束会工具复位。用户观察的本次重启与该工具操作一致，不登记为新panic。

17:37:20起，esptool5.4.0一次连接usb-reset，ROM MAC7c4fadb93408核对一致；实际分区表coredump data/subtype3，0xB00000/0x100000。读取实际表4KiB、dump完整1MiB，结束hard-reset；只用RAM stub，无Flash写/擦除。私有0700目录 `~/Library/Application Support/dev.codexstatus.bridge/backups/clock-stall-dump-20261015-1737/` 已保存原始分区表/dump、.103 BIN/ELF、operation、validation及core ELF；证据文件0600。

结果：存在有效dump，长度23620B，version0x90102，CRC32 stored/computed均6f559cf1；完整1MiB SHA256 96ddadd52045281ae18c706db477f8fcbd0b41abcc54a80eac736e849b9bbfac；有效段SHA256 d243ca32f25555e47c13971857c6cae4f89083cbea7f07ec3dd59e00b3850961。实际分区表SHA256 eb52fbafdc2df41509611b627b32115423e11ceef6d365fe1eb4ef0e44fb8029。

官方esp-coredump1.17.2校验后明确拒绝.103符号：dump内ELF指纹d49e94411，不等于.103 ELF 704ec77b3（完整704ec77b38d9d175b9d7071a916e8210435aa8b7bf3b8b56148ee876297cf372）。已检查本仓artifacts及Bridge backups的保存ELF，无匹配；dump属于另一固件构建，具体版本/发生时间未知，不是当前.103镜像产生，不能把它归因为本次停钟。未绕过镜像匹配做符号回溯。core无符号寄存器PC403854d0仅原始地址，不能用.103源码解读。

dump未擦除，原始敏感内容不入库。工具复位后HTTP仍Host is down，当前认证boot/uptime/reset_reason未知，另做被动串口核对，不从用户“重启了”推定业务健康。

17:39:30–45被动串口收到753B：模板PULL渲染与多次status paged，说明工具复位后程序已重新处理请求。Bridge17:39:28新认证MAC7C:4F:AD:B9:34:08、.103/ota_1、boot_id7e07630b75c72f0cc3b16f38c97b02df、uptime118190ms、reset_reason11（本次工具USB复位）。这补充并替代17:37刚复位后网络暂不可达的状态；不证明原停钟根因已修复或显示时间已由用户验收。证据clock-stall-after-dump-serial-20261015.{json,raw.log}及Bridge认证缓存。

## 后续根因复现与修复计划
- [x] USB身份/只读监听、用户BOOT观察、授权dump导出
- [x] 同源DEV_Module_Init + Arduino HAL拆总线链路确定性red
- [x] 将每boot幂等检查前移覆盖GPIO/USB/SPI，green与旧形状red重放
- [x] .104 Note4唯一目标候选构建、实际ELF检查/栈门禁、资源及最终审阅
- [ ] 实机验证需区分候选/部署；当前设备仍.103，未授权本轮部署

已有反馈入口tools/test-epd-reinit.py：提取真实GPIO_Config/DEV_Module_Init/DEV_SPI_WriteByte，以及当前Arduino HAL的spiDetachBus_SCK→spiDetachSCK→spiDetachBus→spiStopBus。仅寄存器/外设管理边界及mutex HAL替身；重复take抛错替代无限等待，64位宿主仅将bus索引指针转型适配intptr_t。第一次初始化→时钟SPI写→第二次初始化，旧源码稳定报SPI bus mutex self-deadlock in spiStopBus；修复后ASan/UBSan下100次重复初始化/写通过，不拆总线、不重复USB begin、引脚所有权保持。--expect-red可在内存重放旧检查位置，不改生产文件。

.103匹配ELF实际指令4202a8ff先GPIO_Config、4202a90b HWCDC::begin，4202a911/914才查spiReady。GPIO_Config在4202a8d9配置SCK12。Arduino pinMode会经perimanClearPinBus拆SPI SCK，spiDetachBus发现sck=-1后spiStopBus再次取spi->lock；该锁由本任务beginTransaction→spiTransaction一直持有。第二次初始化不曾到达旧防重检查，永等不触发panic，所以无需产生新dump。源码全部DEV_Module_Init调用者只有epdThinBegin与epdBegin；DEV_SPI_ReadByte无调用，不扩展改动。

.103新增网络到期醒的分钟补刷会先epdThinBegin→clockTickWake，随后仍epdBegin进入完整初始化；与现场baseline restored→clk tick→GPIO retention三行顺序一致。当前未抓到故障时CPU/JTAG堆栈，结论为生产路径确定性复现、与现场高度吻合，不声称原现场PC已证明。BOOT ISR仅记事件、消费仍需setup/loop前进，因此锁死时按键不恢复；串口CLI/HTTP/BLE同理。电池/RTC/BUSY作为本次主因的证据优先级下降。

资源：仅移动现有1B static bool检查，无新增RAM/栈/NVS/持久格式、无动态分配；空/单条/典型/上限库字节成本变化均0。原有帧/事务生命周期不改，仍每boot持有既有transaction；无需新锁、线程或看门狗兜底。Bridge/协议/模板不变。

证据artifacts/bridge-macos/epd-reinit-{red,green,red-replay}.log及epd-reinit-103-assembly.txt。独立分钟调度和firmware modules宿主回归已通过。

## .104候选完成（未部署）
唯一目标zectrix-note4-b增量构建37.76s成功，未clean/切目标/重启Bridge。BIN `artifacts/bridge-macos/note4-0.18.104-epd-reinit.bin`，1853856B，SHA256 `1dabbbc96a8667b0c653d59f5cf6a5c0e28ac625a173fcf3301e6e9fba34a76c`；匹配ELF SHA256 `f2272877d5139abf210dde5c7c1a9becd3c448f2e3faef5e7b13352b0265521f`，map同前缀。marker/版本/target及更小ota_1容量均核对。sdkconfig.defaults与Note4完整snapshot构建前后均e31c1c12…aba81ebe，未伪造指纹首行。

.104实际指令4202a902/905先读取/跳过spiReady，4202a907才调用GPIO_Config；既有DEV_Module_Init栈48B不变。静态RAM120036B、链接Flash1800879B、BIN1853856B均与.103相同；新增持久/静态/堆成本0。实际ELF Wi-Fi局部栈/调用链门禁通过，不宣称全任务最坏栈或实机稳定。

Review：不是绕过时钟或禁用BLE/睡眠，而是把共享底层模块既有幂等边界覆盖整个初始化；重复进入不再改SCK/MOSI所有权、关闭panel rail或重启USB。两生产调用者均受保护。回归旧位置red、当前green（ASan/UBSan、100次重入/后续SPI写）及independent-clock/firmware-modules通过，git diff --check通过。原始现场已由用户授权的dump工具复位终止，缺少当时CPU栈，因此将本次归因为该已复现路径仍属高置信解释，不冒充实机PC证据。候选尚未部署或实机复测，当前设备部署仍.103。

归档事故：本次首次copy错误使用Path.with_suffix，将目标截为artifacts/bridge-macos/note4-0.18.{bin,elf,map}；stat birthtime确认其中bin/elf为10月4日已有文件、被覆盖，map为本轮新建。旧bin/elf内容无法从本轮证据恢复。已改为完整文件名保存.104并更正manifest。当前这三个通用名内容也是.104，不能再作为旧版证据；.103专用BIN/ELF/map和私有dump/匹配候选备份未受影响。该事故已向用户明示，不删除通用名冒充回收成功。

## .104正式部署（用户明确授权）
用户要求“部署.104到Note4”。部署前复核BIN 1853856B/SHA2561dabbbc9…34a76c和target/version marker；Bridge认证目标MAC7C4FADB93408、旧.103/ota_1。私有0700备份 `~/Library/Application Support/dev.codexstatus.bridge/backups/clock104-deploy-20261015/`，154文件哈希清单、文件0600，含运行data与installation-id；未重建ROM或重启Bridge。

正式firmware_ota request_id=note4-epd104-20261015、job_id=d1199896。初始HTTP间歇延迟，未换ID/重发；原生无DTR/RTS串口监听期间任务转transferring，attempt_count1；17:50上传ACK后software重启，最终succeeded/image_verified。独立操作token认证/api/ota/image核对完整SHA256、1853856B、MAC、target/.104与ota_0一致；Bridge另一次完整认证fw.104/ota_0、boot_id68f8b280c4bb4336a69f46f7b4e2eb6d、reset_reason3、uptime62611ms。.103保留ota_1，原dump未擦除。

串口记录ROM rst:0xc RTC_SW_CPU_RST及.104启动/网络恢复/PULL和分钟tick763404us；不将单次USB/light tick等同电池deep网络醒重复初始化实机验证。启动一次window write failed后全刷回退，未据此认定原锁死仍存在。此次仅部署与运行镜像验收，电池深睡跨分钟/网络醒复测和长期运行仍待后续授权/观察。所有本轮监听已关闭。

证据artifacts/bridge-macos/clock104-{before-refresh,ota-queue,ota-verified,independent-image,live-deployed}.json及clock104-ota-serial.log。以上最新部署替代前文“候选未部署”阶段状态，不回改原始过程证据。
