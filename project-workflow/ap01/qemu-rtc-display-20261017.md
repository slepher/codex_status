# 0041 RTC、CLIC 与显示 Mock 续作

2026-10-17，C9。用户要求原厂0041在QEMU启动到界面，全程禁止刷实机。

## 结果与当前阻塞

**续作更新：用户确认设备ROM为031、没有Boot2/完整Flash备份途径，未要求/实施实机采集。** 现以调用方合同建立独立、明确标注的PEC UART2 API状态模型：只替换声明的8个表绑定，不声称解密原库；实际校验配置、保持空RX、发送经有界TX队列和chardev。11组合成native wrapper/API overlay回归通过（`tests-pec-01.json`），strict profile仍不重定向。CLIC扩展至原厂80槽，IRQ66/79的真实trap/mret/WFI也已通过（`tests-clic-08.json`）。

启用显式PEC/Flash fixture与v10 native shim后，原厂已进入NuttX、完成文件系统格式化和JD9853初始化，**真实显示CUKTECH开机Logo**。`lcd-icount-paced-24.json`/`.json.png`记录76800 pixels、153743 TX bytes、32 commands；这是原厂启动画面，`lvgl_ui_main`仍未到达。最新 `wifi-tsf-high-85.json` 已创建原厂WiFi平台与固件任务，日志 `6 task is now initialized`；MAC本地状态/双timer/GEN→INTC54→raw70原生7组通过，SYSCTRL独立clock6组通过。继续到 `a0138faa` 的P指令 `ad890977`，CPU支持待补；manager与主UI仍未到。RF base124个明确合成配置槽，PM按实际selector区分；P此前77项通过。固定icount shift2、alignoff、sleepON作为原件推进假设；sleepOFF对照触发空闲快跳/看门狗复位，保留失败证据，不改WDG期限。WDG/双TIMER4组、P77（`tests-p-05.json`）、Flash9、PEC11、LCD7组native检查已通过；SPI MISO明确为合成上拉采样，经真实TX clock进入32B RXFIFO、读取消费，非屏幕ID模拟。

以下PEC入口停点属于**strict profile**，保留作可对照基线：

**尚未启动原厂界面。** 原件已越过原来的RTC停点，完成早期GPIO/时钟及UART0/1初始化；当前真实停点为PEC调用入口`0xa066fd06`，调用方`0xa002a448`，参数`(1,0,0)`。此前直接执行该区域，在`0xa066fd08`因`0x25583e2f`非法取指进入原厂公共trap`0x62fc2500`。这不是应补RV64 AMO.D的证据。

最新原件证据：`artifacts/ap01-research/qemu-0041-20261017/rtc-display-reviewed-original.json`。独立QMP截图是黑屏，LCD `commands=0 / tx_bytes=0 / pixel_writes=0`；NuttX调度、rcS和UI均未验收。合成像素测试只证明显示模型，不代替原厂UI。

原件6769704 B，SHA256 `972db4c136c7ed9e24a83c07c1a7fd62040ca018b08ca285216d26b1fee3c6b9`，文件和取指映射未Patch。设备、Bridge及旧冻结实例未改；所有有限时测试进程均退出。此前PID11338的RTC停点只属于旧引擎快照，不能代表本轮最新模型。

## 已实现及验证

| 模型 | 行为与边界 |
| --- | --- |
| RTC/MTIMER | `20009014`门控/复位/源/分频；CSR time/timeh与`e000bff8/c`同源；`e0004000/4`分半compare，实际虚拟时间截止与MTIP。不可达的有符号ns截止不调度，避免icount死循环。 |
| 原生P指令 | 保留INSB，新增观察到的opcode77 PKBB16/ADD64/MULR64/SUB64，操作数顺序/别名/x0/禁用及非法变体用真实TCG验证。 |
| CLIC公共trap | CPU显式`x-ap01-clic`，RV32、64B对齐mode3；board选择原始IRQ0..79。MIE、局部mie及CLICIE门控；IRQ31/60无需mie11也能唤醒真实WFI并经mret返回。固定NLBITS4、零阈值；不实现SHV/嵌套优先级状态。IRQ66/79也有真实trap/mret/WFI回归，IRQ80拒绝。 |
| BootROM叶函数 | 原生RV32独立shim，slots185/132/131/149/147/161/162/163按固定SDK执行GPIO及外设时钟配置。未知槽拒绝。 |
| HBN RAM | `20010000..20011000`真实4KiB存储；合成cold零，普通system reset保留。 |
| UART0/1 | 32B TX FIFO、chardev实际字节、状态/中断mask/W1C；无输入时RX空。RX DMA门控可配但不产生虚假请求；TX DMA等未支持动作拒绝。 |
| SPI/JD9853 | 原厂命令/窗口/MADCTL/RGB565字节流更新QEMU framebuffer；CPU和DMA共用发送路径。原件flush确认为16-bit frame。非LCD/RX/未知请求拒绝。 |
| DMA0 | 有界M2M、SPI request11、UART空RX等待；16B LLI，最多1024块/4MiB；实际传输后TC/ERR/W1C及IRQ31。非法地址、循环及宽度拒绝；错误允许已经发生的部分传输，不伪称回滚。 |
| GPIO23 TE | 面板TE启用、sleep-out和display-on都有效时才启动理想60Hz边沿，经原厂GPIO状态/清除/屏蔽与IRQ60。关闭/复位取消timer；物理频率及面板顺时针输出旋转仅为显式模型假设。 |

固定SDK `41bf338a57be53919d99f6c6715a96100428c97d`；QEMU v10.1.0 commit `f8b2f64e2336a28bf0d50b6ef8a7d8c013e9bcf3`。主模型在`tools/ap01/qemu-ap01.c`，UART/SPI/DMA各有独立头文件；CPU变更合并于`qemu-ap01-cpu.patch`，构建脚本幂等校验。

模型检查按各自阶段证据记录：core55、RTC6（`tests-rtc-mac-01.json`）、CLIC7（`tests-clic-08.json`）、P77（`tests-p-05.json`）、UART6、DMA5、LCD7（`lcd-clocked-rx-20261018-review.json`）、PEC11（`tests-pec-01.json`）、Flash9（`flash-new-leaves-dev-20261018-review.json`）、WDG/TIMER4（`tests-timer-01.json`）、AON5（`tests-aon-02.json`）、RF LOFCAL8（`tests-rf-cal-shared-42.json`）、ACAL8（`tests-rf-acal-tick-44c.json`）、TX/RX PM14（`tests-rf-pm-selector-60.json`）。它们绑定各阶段引擎，并非全套已在同一最终引擎重跑。TE在icount下16667us；独立像素测试不代替原厂UI，原厂Logo证据另列。

## 合成NOR与当前执行

原件`fs-sub64-13.json`已完成分区读取和文件系统autoformat，Flash状态为实际读/写/擦除计数；随后在原厂看门狗解锁写`0x2000a59c`（16bit，PC`0xa0131104`）停下。`/data [710000,720000)`与`/resource [EB0000,FF0000)`为原件确认区域，其余NOR不可写。合成16MiB fixture v3与v2 BIN同字节/SHA，v3 manifest修正资源区说明；磁盘文件保持只读，只有进程内副本改变。v9 shim含35个BootROM绑定及8个PEC绑定，不是AP01 Boot2或可安装ROM。

默认界面字体和图像已在0041 XIP内部，构造器`a00afc88`直接引用`a025488c/a02548d4/a02549f4`字体和ROM图像，不为启动造/resource素材。是否进入首启/默认页仍以真实执行及像素为验收。

## 当前RF模型与验收边界

RF base只保存已从原件连续RMW、RAM策略表写入确认的配置字段；未知地址、宽度或未知字段变更仍报错，cold/warm零值是明确的合成假设。第一项VCO frequency calibration使用独立`--rf-calibration-mock`开关（默认关闭）：40MHz/div0x855，触发时锁存候选码，2µs虚拟延时后产生`count=42688+(128-code)*64`，完成/只读结果、取消与复位有状态。原件阈值表为42656/42720/44256；native测试候选127/128/129计数42752/42688/42624，二分收敛128，邻码27次扫描退出。这个传递函数是虚拟环境参数，不是实机射频测量。原件已完成该校准及 ACAL；后者以共享 MAC 微秒计数器的下一刻度为稳定边界，合成比较阈值24，21通道原生搜索均得到24。严格1000ns模型曾误拒绝原厂udelay(1)，失败证据保留。

`MIX020=0` 为仅在 RF Mock 下启用的只读选择 fixture，明确不选 ROSCAL/RCCAL；未确认物理含义或reset值，不宣称这两组已运行。TX PM 使用独立有状态采样：mode5、count1024、21个已知40MHz通道，候选低11位相等；bit29上升锁存候选/trim，2us后发布done与signed25结果。显式虚拟函数 `raw=(112+candidate-256-2*(trim-11)^2)*count`（count只1024/4096），TXIQ只允许mode5/count4096/selector9200或12400，反馈为074/600字段的独立线性残差；RX只允许mode域1/count20480/selectorf0000，以614候选构成两路残差。此前抄录的f000已由原件58真实指令修正，报告56为错误fixture验证，报告60保留f000负向拒绝。未知mode/count仍拒绝。候选及trim实际搜索由原件执行；该模型不提供无线连接、不证明RF物理校准。

原件RF失败会日志后继续，但后续无条件等待WiFi-manager完成信号量。单独返回无线不可用不能证明UI可启动，因此未插入该API替换、未伪发manager完成事件。

## PEC strict基线调查（历史）

`file 66f000 → VA a066e000`是独立`PEC:`块。`+30`保存相对入口`1d06`，原件显式执行base+offset；额外加减1000不成立。body从+1000起连续4KiB窗口熵约7.95bit/byte。0031、0031-opt及0031-opt-setting与0041的该块除最终校验尾外相同，没有可直接使用的明文。尚未确认具体编码/保护算法或Boot2变换，不能直接宣称AES。

已确认PEC承担UART2/扩展外设后端：原件依次初始化UART0、UART1、调用PEC+30，再用`62fc378c`配置UART2。`a0025ebe`端口0/1走UART MMIO，端口>1转PEC slots88/40/44；UART2 getc转slot4c(handle,out,1)。TX/RX配置指针为`62fc37a8/62fc37bc`。新SDK PEC v2不是这份blob的准确合同。

不能把全部PEC调用统一返回-1：slot48返回经snez，-1会被当true；slot4c/50有输出参数，其他槽返回用于状态字段。slot30返回虽被忽略，必要副作用未知。

这一早期缺口现通过显式PEC API功能模型继续执行；strict模式仍保持故障。SDK通用Boot2不是AP01现场Boot2，不冒充其解码行为。当前继续事项为轨迹中的历史P指令及后续外设，未为此访问设备或要求刷机。

## 复现与审阅

现有输出目录内选新报告名，提权WSL运行：

```bash
bash tools/ap01/run-qemu-mock.sh \
  artifacts/ap01-research/qemu-0041-20261017/NEW.json \
  --rom-shim artifacts/ap01-research/qemu-0041-20261017/qemu-bootrom-shim-v12.bin \
  --pec-uart-mock --flash-fixture artifacts/ap01-research/qemu-0041-20261017/flash-synthetic-0041-v3.bin \
  --clic-mock --rf-calibration-mock --mac-empty-mock \
  --cpu rv32,x-ap01-clic=on,x-ap01-p=on,xtheadba=on,xtheadbb=on,xtheadbs=on,xtheadcmo=on,xtheadcondmov=on,xtheadmac=on,xtheadmemidx=on,xtheadmempair=on,xtheadsync=on \
  --breakpoint 0x62fc2500 --pass-interrupts 10000 --pass-ecalls 1000 \
  --icount-shift 2 --icount-sleep on --timeout 45
```

`run-qemu-ap01.py`严格验证原件SHA、运行期限及16MiB日志上限，独立QMP超时暂停诊断，保存UART、LCD状态和实际PPM；不把捕获截图自动标为启动成功。新BootROM shim构建必须使用新的输出前缀，防止覆盖历史证据。

GPT-6.1 Sol实现/审阅发现CLIC64..79初期语义缺失，现已补齐实际中断派发与负向回归；另修复部分CFG/THRESH写绕过校验、machine/CPU能力不匹配和面板关闭时多余TE timer调度。无提交、无实机写入。

## Wi-Fi平台续作模型

`mac-empty-mock`为独立opt-in空无线源模型：INTC为64源双bank，24910000/04只读masked pending，24910010/14仅W1S写入unmask状态，空IRQ_INDEX读取拒绝；+14是高bank而非clear。没有外部无线源；本地MACHW FSM/timer会按明确请求产生GEN，经原件index54/raw70路由，未伪发manager成功事件。真实manager任务和事件链保持由原件执行。CRM/SYSCTRL字段是明确zero初值的功能配置，非硬件reset/PLL-ready实测。

BootROM v11新增slot134 EM选择（SDK枚举0/1/2→2000060c低4=0/3/15），native5组`tests-em-01.json`通过；共享480KiB RAM不作物理EM carveout，不清内存。v12 BIN SHA256 `6752a1a51f973b2f917e779e12edd5a6419cccacde2ff69dfcc2d23e5434e886`，6671 B，40个BootROM及8个PEC绑定。`tests-clock-gates-69.json`5组与`tests-mac-empty-69.json`6组native检查通过。v12另加slot107受限uint64门控子集0x33及slot89仅WiFi参数4，真实guest发出540.bit4 clear/set/clear。board仅复位已建WiFi配置、INTC及本地计时状态，保留RF计算结果/RAM/其它外设；不是完整芯片复位域模型。

MAC微秒计数器扩展124高字：原件a016785c..7878读取high/low/high并mask高16位，统一由独立MACepoch的48bit µs视图生成，低32仍与udelay/kernel timer共享。SYSCTRL24900040不是MAC alias：独立1MHz虚拟计数，真实频率/gate未知；MAC软复位不归零、WiFi/global复位归零，`tests-sysctrl-83b.json`6组通过，未验证真实长时间wrap。

`tests-machw-fsm-timers-81.json`7组通过：2us finite ACTIVE/IDLE、kernel/watchdog绝对timer、signed32 deadline、取消重排/ACK，以及真实CLIC70 trap/index54/mret；不是直接调用原厂软件事件。RX队列初始时长字段0由原件自行选择150，`mac-queue-config-78.json` RAM62fd6f90捕获四个0096。MAC capability0/PHY1111与RX highbyte9为明确virtual profile，不宣称物理芯片参数。

PHY version只读profile为01020000，原件a0010f16..0f46及QEMU实际th.addsl实现确认断言是`M+10*(H+2)==32`，该profile满足。`phy-config-89.json`已完成主体2048 B AGC表复制（ROM a000c890→24c0a000..a7ff）及配置，`phy-clock-91.json`进入phy_hw_set_channel，当前停a0010a54读取24c0084c。CRM24940008为bits4..7配置latch，crm_mdm_reset的24940018仅接受0/11软件脉冲：保留已编程PHY配置/表/RF/RAM与独立时钟，因为原件设信道时会再次复位处理单元；无活动PHY datapath模拟。配置零值是虚拟初值，未知字段变更与非32bit访问仍拒绝。主LVGL尚未启动。

`phy-complete-95.json`已完成原生PHY/channel配置，但人工2us ACTIVE延迟触发hal_machw_idle_req:255断言，随后才在assert dump地址8188故障。修正虚拟0→ACTIVE为同步接受、ACTIVE→IDLE仍2us并产生GEN2；`mac-active-96.json`不再触发该断言，实际raw70中断被原件处理，进入vif配置，当前停a016aa96写24b0004c.bit0。`tests-machw-active-96.json`7组通过；`tests-phy-95.json`6组通过，包括完整512word guestcopy、CRM重复脉冲/保留表与配置/独立时钟、strict off拒绝与VMreset清表。两报告实际引擎SHA45e598a6a98357f752113d8948e9db64ffabf866bc510ccf6372d0efbd10d897；CRM没有物理reset域或最小hold时长验证声明。

## 独立OTA目标下的必要Mock与最新边界

后续目标以[OTA专项](qemu-ota-validation-20261017.md)为准。TRNG提供2us busy后的8字可重复测试数据，明确不具备物理熵/安全随机性；原件由此完成MiIO初始化。新增P指令UMAR64保留寄存器对加法/别名/模64位语义，108组指令回归通过。MAC DMA为空源：完成计数0、OF_FREE低16位全1、ACK清理不制造接收/发送事件；扫描改在公开API边界建模，不扩无线射频。

BLE `bt_enable`以原件真实无HCI错误-19退出其任务。精确`bflb_device_get_by_name("adc")`返回NULL，其余设备查询执行原件；原件ao_setup返回-1，UI记录ADC/temp/port不可用并继续，无温度/ADC样本伪造。扫描失败不是永久退避：`a0069356`明确安排4000ms重启，120证实这一链，先前“scan unavailable可持续运行”的推断已撤回。新EmptyScan在公开start/status/results API中保存fd/接口/事务，用真实guest tick等20ms，至少一次pending，随后完成并返回空列表ENOENT；原件容忍这一结果。它不是空口扫描或网络关联证明。

UART接收DMA允许SDK HALT bit18保留E/寄存器、清H恢复，LOCK仍ERR；PWMv2两个group提供有限2us stop/start状态及配置存储，没有波形/IRQ。显式GPIO3高选择原件非工厂分支，GPIO24高为未按压旋钮；没有伪输入事件，实机GPIO3电平未知。

最终引擎SHA256 `7246b77aa123e3b785d8c1093b4ee2bac19a3fb1b489bc0d1035963d87e18b9d`：`tests-dma-final-126.json`6组、`tests-pwm-final-126.json`3组、`tests-ota-flash-final-126.json`三profile通过；扫描13项单元仅验证事务，不冒充硬件。`ui-empty-scan-continuous-125.json`原件SHA未变，在50宿主秒观察期推进到8628虚拟ms，25次扫描均经历pending/done/results，MiIO及原生LVGL定时回调持续，无guest exception。LCD仅153600次像素写入，捕获仍是原厂Logo；**主界面实际显示尚未完成，不能以UI回调日志代替屏幕验收**。不声明完整外设、长期稳定或物理Boot2/Wi-Fi等价。
