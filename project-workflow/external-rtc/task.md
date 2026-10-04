# 独立 RTC 实施与证据（2026-10-04）

- 生产驱动 src/external_rtc.cpp/.h，按target选PCF8563/PCF85063A；UTC+严格日历/状态校验，STOP完整写回确认，本启动写未确认屏障；32B/方向的Wire缓冲（rtc2按真实平台修订）和25ms操作超时，无第三方库或新增NVS键。
- main.cpp启动/深睡唤醒优先RTC，light60s检查、≤1s保留亚秒；Bridge两处既有校时helper写回，SNTP回调短临界区冻结UTC与单调锚点，loop/深睡前补足经过时间并写RTC。旧内部RC恢复保留为失败回退，超时/PowerPlan/owner单调语义不变。/status.json与认证/api/status增加clock_rtc缓存诊断，不改BLE结构。
- 驱动ASan+UBSan两target通过：时区、2020–2099逐日UTC对拍、2038、闰日/跨年、非法BCD/日期/世纪、OS/VL/STOP/test/12h、短读/短写、9阶段NACK/丢ACK、撕裂/回读污染及重试/重启拒绝混合日历。复跑 bash tools/test-external-rtc.sh。
- 生产main helper按原样提取编译+真实RTC驱动的受控时钟回归通过：冷启动、亚秒、失效保留回退、NTP在读取中途到达、延迟30s仍恢复新时间、RTC修改不改monotonic。仅标志pending的对照模型在fresh epoch断言失败，最终生产helper通过；不伪报旧部署ROM的实板red。复跑 python3 tools/test-external-clock.py。
- 仅Note4构建，首轮95.23s，最终包含并发修复39.87s；无clean/多pio/目标配置切换/重装。最终marker匹配，1738848B；ROM `/Users/slepher/project/codex_status/artifacts/external-rtc/note4-0.18.56-rtc1.bin`，SHA256 `E06CE478153103AE4DC4E939D7E01F2602904CD2E90B8C7D382028A467BA5217`；ELF `/Users/slepher/project/codex_status/artifacts/external-rtc/note4-0.18.56-rtc1.elf`，SHA256 `CD474224AFEA96A0BE5805C561066B3207808C4E3ECD5220091AAE32EE94B562`。1.54仅同源驱动测试，源码版本0.18.35-bw-rtc1未构建ROM；Note4候选0.18.56-note4-b-rtc1未OTA。

## 资源预算（真实目标ABI/ELF；不冒充整机实测）

| 阶段/用量 | 新增NVS | 链接静态RAM增量 | RTC/I2C已识别堆请求 | 业务日期缓存 |
|---|---:|---:|---:|---:|
| 无有效日期（总线已初始化） | 0B | 48B | 1500–1516B | 单一16B RTC对象 |
| 首次可信校时/单设备 | 0B | 48B | 同上 | 同上 |
| 典型长light | 0B | 48B | 同上 | 同上 |
| 本机容量上限/每次deep唤醒 | 0B | 48B | 同上 | 同上 |

- 固件静态RAM117876→117924B；ROM1735744→1738848B（+3104B）。RTC对象16B；main新增锚点、锁和控制位均定长标量，与Profile/网络条目数无关。两个Wire静态对象各100B在旧.55 ELF已存在，不算新增；TX/RX新请求各32B（合64B）。
- 平台成本：master536B+SDK同步事务20×12B=776B，基础总线68B，单0x51设备32B+链表8B，Wire/HAL/IDF共5个92B信号量/队列控制块+4B队列payload，PM锁56B，IRQ handle8B+共享描述24B，必要时新vector16B。都是匹配ELF DWARF和现行SDK请求尺寸；不把SDK工作缓冲当自建仓库空槽。
- 已识别请求峰值1500–1516B，不包括allocator元数据、共用系统首次初始化、HTTP JSON/响应String、RF/其它系统堆及ISR；完整实机峰值/功耗尚未测量。无新增业务按次堆对象或持久副本；异常初始化由Wire/IDF清理，驱动处理失败并明示状态。
- 匹配ELF帧：setup160B/loop448B；readExternalRtc64B、driver read80B/write128B、flushNtp48B。可解码RTC/Wire/I2C局部链最大416B，加loop448B为864B，loopTask12288B中另留4KiB平台余量。SDK部分映射阻止反汇编解码，仅能从ENTRY字节确认I2cInit112B等，未将不完整调用链或allocator/ISR省略项冒充全链高水位。Wi-Fi真实启动/存储门禁通过（192/240B等，局部最高1920B≤4096B）。
- 证据：artifacts/external-rtc/driver-tests.log、clock-tests.log、clock-pending-only-red.log、pio-note4-final.log、rom-manifest.json、rtc-stack.json/rtc-frames.s、wifi-stack.log、library-abi-sizes.json、resource-budget.json。git diff --check及node --check tools/estimate-power.mjs通过。

## 实机修订 rtc2（2026-10-04）

.56 OTA任务956efea3及独立运行hash成功，但实机clock_rtc=buffer_allocation_failed。Wire.cpp:176要求32B/方向，替身遗漏约束；补真实限制后旧驱动确定性red（wire-minimum-red.log），修为32B后二targetASan/UBSan与生产main helper回归green。仅Note4.57增量39.84s构建，匹配ELF Wi-Fi门禁通过，RTC/loop各帧与.56相同；新Wire64B合计，已识别堆请求1500–1516B，静态RAM仍117924B（+48）。最终ROM及匹配ELF见rom-manifest.json，.56原证据保留*-rtc1.json。当前.57按MAC排队a379f30b，待独立运行与RTC实机确认。

## 最终实机交付 rtc3（2026-10-04）

.57运行hash确认但short_read，厂商zectrix_board.cc/i2c_device.cc明确每次I²C访问开GPIO42；本项目epdBegin/epdThinBegin关闭该电源。Note4作用域覆盖完整read/write及嵌套验证，恢复调用前高/低电平，1.54无电源动作；补替身供电约束red、成功/失败恢复green，两驱动/生产helper通过。仅Note4.58最终40.67s构建，1739104B/hash F6971A63C7507FE43AB92EA9C72ABBFC78C64EB43DF31FCB18350104207545A1；匹配ELF hash7543B30F…C9AFBD。静态RAM117924B，堆请求1500–1516B不变；read96B/write128B/loop448B；局部解码链与边界见rtc3-stack.json，Wi-Fi门禁通过。

OTA b2e990a9 succeeded/image_verified，独立认证镜像匹配；pcf8563 ready/valid，后续采样5997→6040推进43s，在线来源ntp，未以此宣称离线漂移/功耗验收。WFS3/Wi-Fi保持正常，已登记latest release。完整ROM/ELF路径与hash、现场、证据见PROGRESS顶节。未提交；1.54未构建/部署。

版本命名修订已完成：0.18.59-note4，唯一Note4环境47.57s构建/marker/匹配ELF门禁及RTC/main帧一致检查通过；OTA e082ac3e succeeded/image_verified，下个自然联网窗口独立运行镜像hash14CC1FBA…1FB8DF匹配，RTC ready/valid与WFS3保持正常，latest release已登记。后续只用版本号-note4；完整产物与证据见PROGRESS顶节。本轮未提交。
