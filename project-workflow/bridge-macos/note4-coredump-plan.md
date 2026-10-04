# Note4 dump 分区与恢复计划（2026-10-04）

## 优先级与范围

用户顺序：①重新分区保留dump；②查清状态同步阻塞；③完成原交汇设计及生产组合验收。本计划接续现有bridge-macos工作，不开新的会合专项。

## 已确认的现场

Note4 7C4FADB93408 / 16MiB flash，运行 .45 / ota_1。约01:00自行重启，认证reset_reason=4及公共reset=panic；当时用户没有插拔或按键。此前panic状态/会话已存restart-event-*.json。01:21附近USB串口短时读取，即使打开前DTR/RTS=false仍出现USB_UART_CHIP_RESET；这次工具诱发复位与此前panic分开记账。串口明确No core dump partition found。

USB已只读备份完整16MiB闪存，私有备份与匹配ELF路径见artifacts/bridge-macos/note4-repartition-backup.json（不提交）。实际分区表与旧partitions_note4.csv一致，无dump；ota_0镜像1,697,632B、ota_1镜像1,720,784B，后者与已部署.45逐字节一致。源配置已启用flash/ELF/CRC32及开机检查，但缺少分区，配置不完整。

## 推荐布局：1MiB dump，保留现有数据位置

地址范围采用左闭右开。

| 分区 | 起始 | 大小 | 结束 | 迁移 |
|---|---|---|---|---|
| nvs | 0x009000 | 16KiB (0x4000) | 0x00D000 | 原址原大小 |
| otadata | 0x00D000 | 8KiB (0x2000) | 0x00F000 | 原址原大小 |
| phy_init | 0x00F000 | 4KiB (0x1000) | 0x010000 | 原址原大小 |
| ota_0 | 0x020000 | 5.9375MiB (0x5F0000) | 0x610000 | 原址原大小 |
| ota_1 | 0x610000 | 4.9375MiB (0x4F0000) | 0xB00000 | 仅缩短尾部1MiB |
| coredump | 0xB00000 | 1MiB (0x100000) | 0xC00000 | 从旧ota_1空闲尾部划出 |
| storage | 0xC00000 | 4MiB (0x400000) | 0x1000000 | 原址原大小 |

0x10000–0x20000保留原64KiB空隙；启动区/分区表位置保持。OTA槽不同大小允许；所有未来可双槽更新镜像必须≤较小槽0x4F0000=5,177,344B，当前.45占33.24%，余3,456,560B。不得只按16MiB flash或较大槽判断可OTA。

### 容量依据与限制

不将64KiB空隙直接当最终dump容量：现有loop/NimBLE/timer栈各8KiB、tcpip等4KiB，多任务、TCB、寄存器及ELF元数据需余量；当前上限64任务，实际任务数及panic时总栈长度未测。1MiB较512KiB多一倍余量，对16MiB flash成本6.25%，且应用现有大小足够，文件系统无需搬迁/缩容。

IDF Kconfig说明CAPTURE_DRAM额外保存内部.bss/.data/heap，至少预留128KiB且实际取决于运行用量，外部RAM段不保存。当前CAPTURE_DRAM关闭；预留1MiB支持后续在实际转储测量后启用，但不宣称转储整个8MiB PSRAM，也不承诺任意64个64KiB外部栈均可装下。CONFIG_FREERTOS_TASK_CREATE_ALLOW_EXT_MEM已开启；需验实际任务栈来源与SDK flash转储兼容性，单有空间不代表转储成功。

源依据：本机IDF components/espcoredump/Kconfig、core_dump_elf.c、core_dump_flash.c（会在完整长度+padding+CRC超容量时拒绝）。不用新框架或修改全局生成sdkconfig.defaults伪造生效。

## 执行与验证

1. 私有备份全flash及匹配ELF；核对MAC、16MiB长度/SHA、真实分区、两槽镜像长度、活动镜像与.45匹配（已完成）。
2. 更新Note4 CSV，使用当前IDF gen_esp32part生成并校验分区表；无交叠、4KiB/64KiB对齐、flash末尾=16MiB、NVS/otadata/storage及app起始不变，dump区域位于两槽现有镜像之外。
3. USB只写0x8000分区表，并擦除新dump区0xB00000–0xC00000；不整片擦除、不改NVS/otadata/storage、无需替换现有.45应用/bootloader。普通应用OTA不更新分区表。操作前后读回表和dump空白，核对精确MAC；失败则按备份恢复原表，不盲刷旧模板/凭据。
4. 正常启动验证coredump分区识别，认证身份/运行ROM哈希/Bundle不变，HTTP恢复，原文件系统可读。启动识别仅证明分区可用，不代替真实panic转储/CRC/ELF解析验收。
5. 后续异常必须先导出原始dump+分区表+匹配ELF并核对CRC，再解码栈。dump与全flash均私有（可能含凭据），不提交、不完整打印。将第一份dump保留策略纳入下一次配置变更：FLASH_NO_OVERWRITE=y，归档确认后再显式清空；当前.45默认覆盖最新dump，不伪称已启用保首份。

## 次优先：当前同步阻塞

23:46 BLE plan ACK读取失败后BLE协调日志停止；HTTP状态仍持续成功，panic后data_seq/applied_seq=0，Bridge push_dirty=true、in_flight=null，合同与最新数据已具备。生产ble_cycle持全局和每MAC锁，在10s工作期限外调用无界link.close；btleplug 0.11.8 macOS在自然断链已移除peripheral时disconnect future不会被完成。高度疑似清理持锁悬挂阻塞HTTP投递；需生产路径故障注入/恢复验证，不能仅用静态证据认定此次具体future。该阻塞与设备panic根因分开，不认为补dump直接修复同步。

## 第三优先：原交汇任务

继续原W4/W6：统一合法窗口中任务准入、就绪/预算与关闭条件；补BLE失败清理不阻塞后续会合/HTTP、诊断和Wi-Fi并存、慢入网/断链/重启生产app组合回归及实体多轮。已有rendezvous-audit.md的F1–F3与归属更正保留。

## 交接点：用户指定单独 agent 接手（2026-10-04）

本线程停止USB/分区操作，不启动另一个agent、不写设备。已完成：16MiB完整闪存只读备份及匹配ELF保存，实际分区表无coredump；两槽现有应用长度及.45运行镜像匹配验证；partitions_note4.csv已加入1MiB coredump并仅缩短ota_1尾部；IDF生成与离线迁移检查通过。**新表尚未写入设备，新dump区域尚未擦除，未替换ROM/bootloader。**本轮esptool只运行read-flash与API签名检查，没有运行write-flash/erase-region；完整备份进程已完成，串口短时读取已关闭，未启动后台串口监听。

私有备份目录：`/Users/slepher/Library/Application Support/dev.codexstatus.bridge/backups/note4-repartition-20261004-012653`；全flash SHA256 `bedd6478281cece378333eea559525b479e53bb72511ced4cb246b23d626dddc`。备份含凭据，不提交或完整打印。

候选表：`artifacts/bridge-macos/note4-coredump-partitions.bin`，SHA256 `6a11bc014301bc1d3938a7967b1a9661e99d13d9f866dbdc361ef9abf8d00a39`；验证：`artifacts/bridge-macos/note4-repartition-validation.json`。不要把.pio/build旧partitions.bin误当新表；若其他agent修改CSV，必须重新生成/验证候选表及读回。

USB目前/dev/cu.usbmodem14101，序列号7C:4F:AD:B9:34:08；访问前重新枚举并核对MAC。当前设备分区仍为旧表，.45运行ROM哈希沿PROGRESS原记录。下一步由接手agent明确执行边界、核对现场后更新设备和验收。同步/交汇工作由哪个线程接续，待用户确认，避免重复设备操作。
