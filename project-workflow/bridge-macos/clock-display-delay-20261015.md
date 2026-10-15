# 时钟跳分钟约晚10s：只读核对（2026-10-15）

用户在14:00自动恢复后观察屏幕分钟切换约晚10s，询问显示调度还是内部时钟偏慢。本轮只读认证与代码核对，未修改/构建/刷固件。

两次platform_status_refresh记录Mac请求前后时间，设备sampled_wall必须落在该请求区间才是零偏；整秒分辨率额外留1s。第一轮RTT1.957s，设备相对Mac偏差区间[-2.687,+0.269]s；第二轮RTT4.052s，区间[-2.739,+2.312]s。交集排除当前内部时钟慢10s，不能由此推断此前两小时离线期间也无漂移。设备source=ntp、PCF8563 ready/valid；clock_rtc.sampled_utc是最近RTC采样，非每次响应时现读，不可将它落后sampled_wall的秒数直接当RTC偏差。证据：artifacts/bridge-macos/clock-bound-samples-20261015.json。

源码：application.cpp主循环每秒检查nowSec/60，分钟变化才触发clockTickWake，未设置固定第10秒刷新。秒检查并非屏幕可见完成期限：v2InRendezvous分支在该检查前提前return；HTTP/渲染等主循环工作与墨水屏BUSY等待也可延后。display.cpp先取时间构建HH:MM，再唤醒面板、写窗口及等待刷新结束，因此可见时间一定在采样之后。深睡使用secsToNextMinute对齐设备整分钟，仍有唤醒/绘制成本。

结论：当前测量更支持显示链路延后，不能确认恰好10s的耗时发生在哪一段。尚未取得该次跳分钟的主循环触发与EPD完成时间配对，不能断言是特定BUSY等待、固定相位错误或累计RTC慢10s；若继续修复，应围绕分钟边界记录设备epoch、渲染开始/结束和面板完成，再决定是否在会合期安排有界时钟刷新。


## 14:08–14:10 实机测量（用户要求继续测定）

只读130秒轮询 `/status.json`，逐份核对登记MAC，共65份有效响应、0次失败；未改Plan/配置/固件、未按BOOT。设备始终为timer wake1186的Wi-Fi light状态，Bridge正常业务自行将Plan4011续为4012。本轮覆盖在线light，未覆盖此前离线deep/会合现场。

| 分钟边界 | 时钟渲染计数变化 | 累计render_ms差值 | 以Mac时间界定完成区间 |
|---|---|---|---|
| 14:09:00 | clock_ticks1356→1357，render_count14→15 | 774ms | 14:09:00.278之后、14:09:03.128之前 |
| 14:10:00 | clock_ticks1357→1358，render_count16→17 | 761ms | 14:09:59.583之后、14:10:02.728之前 |

区间采用前一份未完成响应的请求开始、首份已完成响应的接收结束，包含HTTP耗时；不是相机拍到的像素翻转时刻。两次只有一个clock tick和一个render，累计耗时差可归因于clockTickWake，其计数在面板写及sleep结束后增加。没有复现10s，测得普通时钟面板路径约0.77s。

14:09:39.479–43.579间另有非clock渲染一次，render_ms增加1362ms、epd_busy_fails28→29、clk_partials归0，符合局刷失败后成功全刷回退。这个计数也覆盖软件基线/窗口等拒绝，不能据名字断言发生硬件BUSY超时；该次总耗时不足5s，不能解释为5s硬件等待。初次14:06完整认证读取6s超时；随后状态采样均成功。初版独立紧凑探针误用OTA设备token头返回401，改用已绑定业务endpoint Bearer后通过；不属于设备鉴权故障。

14:10:29–34三次紧凑认证MAC一致，HTTP ready、Bridge reachable、network.error=null，固件.102，电量54–55%，manual/provisional0。时间偏差区间交集[-2.139,-0.139]s，排除当前系统钟慢10s。证据 `artifacts/bridge-macos/clock-current-auth-20261015.json`。

另有5份采样报告PCF8563 short_read/valid=false，随后自行恢复ready；NTP系统时间源未改变。成功的[clk]耗时日志被syncDiagnosticText过滤，故网络/log不能提供逐次驱动阶段细分；当前以累计计数取差测量。原始安全字段采样 `artifacts/bridge-macos/clock-minute-measure-20261015.jsonl`，筛选日志 `clock-current-log-20261015.txt`。

结论：10秒不是本轮在线light的正常时钟刷新成本，用户此前观察仍待在离线deep/会合现场复现，不以本轮0.77s替代该现场验收；RTC短读和局刷回退为另两项真实异常，但尚不能归因为那次10s。未改ROM，未宣称修复。


## 14:22:17用户现场观察与会合记录对应

用户明确观察设备显示14:22发生于Mac14:22:17，作为外部目视时间证据（约数）。Bridge日志对应：14:22:14.555首次发现该轮广告；15.792核对MAC及wake1188；16.571 Plan ACK，16.580记录Plan4014、accepted_remaining_s=0；16.581会合完成。与约17秒屏幕更新高度吻合。证据：artifacts/bridge-macos/clock-1422-rendezvous-20261015.log。

代码确认：application.cpp仅deepClockOnlyBoot提前走deepThinWake；网络到期的deep timer wake跳过该提前时钟路径，完成加载后开始BLE会合。tick会合分支提前return；radio.cpp v2RendezvousTick会合收尾才调用display::v2RendezvousRender，睡眠分支最终调用v2RendezvousClockRender。因此这次不能沿用在线light的0.77s来否定延迟：确有“网络会合先、时钟刷新后”的调度依赖，用户更新时刻与会合完成一致。

边界：14:22:14.555是Bridge发现广告时间，不是物理唤醒时间；目前未取得wake1188设备端启动epoch/uptime与分阶段耗时，不能把整段17s全部记为BLE耗时，也不能排除该次深睡恢复的RTC/启动偏差。此前两组NTP在线时钟偏差不能证明这次deep恢复仍然准确。下一步需在deep网络唤醒同时测量启动时钟、开始会合和显示完成，验收时钟刷新与网络进度解耦。未修改固件。
