# 独立 OTA HTTP：稳定槽与直接回切

2026-10-19，用户明确要求OTA必须具备切换到稳定槽的功能，并放在独立OTA HTTP service中。此项延续“不刷机”；旧候选16没有此功能，不能用旧镜像替代新产物。

## 接口与隔离

维护服务18080独占以下入口，业务18081不提供槽位接口：

| 入口 | 行为 |
| --- | --- |
| `GET /api/firmware/slots` | 当前运行槽、已确认稳定槽、稳定记录修订、镜像CRC/长度/build |
| `POST /api/firmware/stable` | 用户明确确认**当前正在运行**的槽为稳定槽；完整校验该槽及其与运行XIP的一致性 |
| `POST /api/firmware/switch` | 不上传镜像，重新校验已记录稳定槽，提交下一次启动槽 |
| `GET /api/firmware/<request_id>` | 查询原切换事务，区分已提交、结果未知和重启后确认 |
| `POST /api/firmware/reboot` | 对已提交原事务显式重启，响应发完/连接关闭后执行 |

所有入口都要求已有绑定token并解析principal。槽位执行器`Ap01Firmware`不读取Codex owner、context、Profile或Bundle；维护路由已取消业务owner占用检查。绑定只提供设备认证，业务未begin或未上线仍能维护。服务线程、端口、存储和事务由OTA服务持有；切换期间业务只接收维护忙状态，不参与槽位判定或持久提交。

稳定槽是显式确认的运行版本，不把“CRC有效”或“刚启动”冒充稳定。确认请求带device_mac、request_id、slot、expected_stable_revision和confirm=true；不支持把尚未启动的另一个槽直接标为稳定。再次确认新的当前版本会更新稳定槽，允许下一次升级复用旧槽。

切换请求带device_mac、request_id、target_slot、expected_revision、expected_stable_revision和expected_image_crc。错误MAC/token、过期修订、目标非稳定槽、目标已运行、镜像变化或损坏均明确拒绝。相同请求精确重放不再次调用提交；unknown围栏只查询原请求。重启确认同时比较**目标槽和build**，即使两槽装同一ROM，也不会在旧槽误报已完成。

## 升级保护与持久化

独立OTA升级前必须已有稳定槽确认，且目标非活动槽不能是稳定槽；否则返回stable_confirmation_required或stable_slot_protected。因此从新版本回刷/再升级之前，必须选择回到旧稳定槽，或明确确认当前版本为新的稳定槽。本策略不自动触发回滚，也不以短暂可达作为稳定证明。

稳定记录APS1单独存放在`/resource/codex-status/stable.bin`；包含显式标识、长度和CRC，JSON最多512 B，文件最多524 B，临时+最终最多1048 B。记录包含MAC、槽号、完整镜像恢复CRC、长度、build、修订及确认来源事务。先写临时文件、读回、原子rename并核对最终字节。未知格式/损坏/不可读明确拒绝，不静默忘记稳定槽。

切换复用APF1原子事务文件，增加明确operation及target_slot字段；既有无字段APF1按原upload记录读取。新事务提交前保存Committing；原厂提交不确定或后续持久失败进入unknown，不能新ID重试。未写任何固件镜像字节；只允许原厂槽表提交和受控元数据更新。稳定记录按实际需要分配，不为两个空槽预留完整镜像或仓库。

RV32新增成本：OTA服务84 B、单事务504 B；稳定对象按字段布局76 B，另有实际owner/request字符串，最多130 B。稳定记录编码/读取临时栈约525 B；原生镜像校验以1024 B块流式读取，另有ota_image状态，不把7 MiB镜像放入RAM。候选18匹配audit的`ap01InspectSlot0041`栈帧为1456 B；它是单帧，不声称物理整链高水位已测。

## 0041 原厂接线依据

原件SHA256仍为`972db4c136c7ed9e24a83c07c1a7fd62040ca018b08ca285216d26b1fee3c6b9`。已有隔离Ghidra项目导出`0xa0055a14`：`boardctl(0xff13,path)`接受显式`/dev/fw1`或`/dev/fw2`，更新RAM槽表并以原厂CRC/erase/store叶函数写另一份分区表。不使用0031函数地址，不直接改Boot2。

0041槽表RAM在`0x62fc8cf8`，表副本编号另在`0x62fc8cf4`；读取前校验magic、数量、两级CRC和精确FW双槽布局，再取FW条目activeIndex。运行槽在本轮启动时捕获，提交后不将新的“下次启动槽”冒充当前运行槽。**不用ro.boot.current_slot推断运行槽**，因为原厂环境串可能反映槽表副本编号；普通升级仍校验原厂update_slot与已确认非活动槽一致，冲突时拒绝写入。

目标镜像从原厂/dev/fw1或/dev/fw2只读流式读取，验证0041型号、启动头、完整长度、尾标记、全镜像恢复CRC；当前槽另外与实际XIP启动头和应用manifest身份对比，不遍历未映射的ROM空隙。回切通过显式目标路径调用原厂ff13并核对RAM表，重启后查询原事务。真实Boot2选槽/LCD/Flash电气及掉电窗口仍需另行实机验收。

传统厂商OTA字节保持不变；按既定人工顺序使用两通道。本服务的稳定槽写保护不等于传统OTA也已接入该策略，不声称跨通道硬件写保护或自动救砖。

## 验证

原生候选21在回切前发现真实UI问题：根事件回调将绘制事件也送入OTA输入分支，隐藏控件触发`lv_inv_area: !rendering_in_progress`断言。随后panic处理器访问未建模ROM API，不是Flash跨槽读取根因。候选22已仅允许按下/释放/旋钮键进入输入逻辑；通过匹配ELF实际UI定时器和原厂帧缓冲验证，不靠串口单行启动文字，也不修改冻结模拟器来跳过断言。

宿主ASan/UBSan覆盖业务从未启动时的认证确认/回切、稳定槽禁止覆盖、目标损坏/身份变化、重复请求只提交一次、unknown恢复与损坏稳定记录拒绝。完整原生RV32更新、实际ff13回切、两槽镜像完全不变及从同NOR回切后启动的证据，完成后登记到状态卡与新ROM manifest；不沿用候选16的验收代替本功能。
