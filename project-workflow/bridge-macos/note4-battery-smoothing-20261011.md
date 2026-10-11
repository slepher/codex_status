# Note4 显示电量平滑

日期：2026-10-11。归属B6，用户已授权实现、测试和Note4刷机验证；不修改其他设备或Bridge运行程序。

## Plan

Note4每5秒在既有主循环采样一次8次ADC平均电压，转为原有3300–4200mV百分比后做90%历史+10%新值的整数EMA。内部保留百分之一百分点并四舍五入，避免整数截断造成明显偏差；首次有效路径直接初始化，0%是合法值。渲染只读缓存，不采样、不推进滤波。主循环忙时错过的周期只采一次，不伪造历史样本；深睡/软件重启后首读重新初始化。没有新任务/定时器/库/NVS写入，也不单为采样触发墨水屏刷新。

原始采样接口与低电保护保持独立；诊断battery/battery_mv为未做时间平滑的读数，新增battery_display与显示滤波采样时刻/原始mV便于核验。USB状态不参与滤波。只有Note4启用，其他目标保持原显示行为。

## Task / Status

- [x] 实现纯计算/采样节奏状态与Note4显示接线。
- [x] 生产代码测试：首读0/100、95/96/97、上下阶跃、固定节奏/反复读取、阻塞跳周期、millis回绕、低电原始值独立。连同compiled/boot_plan共12 passed；固件模块接线检查通过。
- [x] Note4构建/66项匹配ELF栈门禁通过。版本0.18.82-note4，静态RAM119324 B（+16），Flash1873567 B（+436），BIN1926256 B（+464）。SHA256 `B2C9E1C1FA8E8A01FE2CB66985875155E61B6F6ADF08F046E8B98C69D6E286E7`，文件 `artifacts/note4-0.18.82-battery-smoothing.bin/.elf`，marker核对一致。
- [x] 已核对登记与认证MAC，OTA46986171单次上传、succeeded/image_verified。认证运行0.18.82-note4/ota_0，boot eb3cc78d6243e38e5feb871b29efcb77，运行镜像哈希与上方产物一致。模板ready/displayed，空闲堆94084 B、最大连续40948 B、loop栈余量6324 B。证据 `artifacts/note4-82-ota-status.json`、`note4-82-after.private.json`。
- [x] 用户17:53反馈已按键，确认现在没有跳变；完成本轮实体显示验收。

## Review

状态预算16 B（32位周期锚、实际采样时刻、EMA整数和最新采样mV），无堆分配、无持久数据；空/首读/典型/上限均16 B、持久字节均0。验证不改变真实电池电压，不人为触发低电关机；低电隔离通过生产计算/保护函数测试。屏幕数字稳定不证明SoC校准准确，真实BLE开关的肉眼验收与软件/帧验证区分记录。

匹配ELF实测BatteryDisplay=16 B，serviceBatteryDisplay/displayBatteryPercent各32 B栈帧；采样复用既有ADC路径，不把局部栈帧当动态完整调用链。证据 `artifacts/note4-82-tests.log`、`note4-82-build.log`、`note4-82-stack.log`、`note4-82-object-size.log`、`note4-82-image.json`。原始电量检查周期与低电关机判据未改，USB/保护决策不消费EMA；三处屏幕入口统一读取缓存，诊断不推进滤波。

实机自动观测20次/约40秒，0错误；原始电压4156–4174mV，原始百分比95/96/97，平滑缓存95/96。7个可见采样间隔4927–5073ms，符合主循环5秒目标节奏；诊断读取不推进采样。EMA仍可能在整数边界缓慢变化，并非冻结百分比。HTTP观测未捕获短暂BLE ON，用户实体开关反馈作为独立证据；期间EPD写入5→9、BUSY失败0。证据 `artifacts/note4-82-battery-observation.json`。

用户追加授权完成后提交，本次连同此前尚未提交的BOOT/GATT、计划恢复及OTA配套改动统一收尾。未解决的历史启动/HTTP间歇延迟保留在B6/C8；本次电量验收不替代RF/功耗长期矩阵。
