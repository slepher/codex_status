# 独立分钟时钟（2026-10-15）

## Plan
整分钟显示期限独立于BLE会合。深睡选择分钟/联系更早期限；网络醒先补到期时钟，会合循环仍服务分钟时钟；仅成功显示记账，收尾不重复刷钟。沿用主循环串行面板，不增加线程。

## Tasks
- [x] Fake ROM 非整分会合跨分钟回归先失败
- [x] 同源分钟调度、固件唤醒/循环/收尾接线
- [x] Fake ROM 回归及固件构建/栈门禁
- [x] Review、证据、交接（候选与部署分开）

## Status
实现/回归完成，14:48已部署0.18.103-note4/ota_1；随本次提交归档，Bridge未重启。用户确认时钟及时更新并要求停止进一步复测。

## Review
重点：整分前不足5秒不能跳过下一分钟；时钟刷新不改变PowerPlan/下次联系时间；失败不记成功；未知时钟/无绑定不刷新；回拨需重新显示；会合截止仍按真实单调时间，不因刷屏延长。


## 结果与证据

- red：Fake ROM UTC :58开始会合，:00时display_writes不变，断言失败（independent-clock-red.log）。这是实际device-sim进程＋同源编译模板，不通过GET /sim/frame触发刷新。
- green：同一用例通过；增加深睡先到分钟后到联系的检查，clock-only不改next_contact_ms，会合跨分钟刷新真实像素frame_crc且关闭不重复刷；next_clock_ms供step/max runner独立推进。independent-clock-green.log。
- Fake ROM bootstrap全套69通过（independent-clock-suite.log）；后续仅控制面next_clock_ms/runner接线追加，目标用例复跑通过。固件serviceMinuteClock原函数＋同源RendezvousProtocol宿主验证:58跨界、:17联系、:59入睡、回拨、未知时间/无绑定、失败不确认/有界重试、清理耗时及亚秒睡眠（independent-clock-host.log）。test-firmware-modules.py、node --check tools/fake-rom-runner.mjs、git diff --check通过。
- 固件网络醒先用保留时钟窗口补到期分钟，失败后加载Bundle再本地全刷；tick在网络请求/会合return前检查。收尾clockTickWake同分钟无操作。全帧使用固定时间采样且只有面板确认一致才登记；局刷同步lastDisplayedFrame避免后续软件旧帧冲突。
- 深睡两截止取早，并将绝对epoch传过射频清理，在真正设置唤醒定时器时扣除清理与亚秒相位；已过期时1ms尽快醒，不再重置为相对睡眠。1Hz门控仍在time()前，避免每轮墙钟查询回归。
- 资源：每设备新增RTC8B displayedMinute、主循环静态8B checkAt；无新增持久NVS格式/字节，无新增线程或常驻堆。空/单条/典型/上限网络仓库均相同成本。网络醒复用既有thin baseline临时15000B缓冲，释放后再走正常帧分配，不持有额外完整帧。匹配ELF serviceMinuteClock栈32B、clockTickWake96B、sleepToNextEvent32B、deepSleepFor48B；Wi-Fi实际Xtensa门禁通过（independent-clock-stack.log），仅局部栈帧/已建调用链，不是全任务最坏栈证明。
- 尚未实机部署/测分钟可见延迟；宿主不模拟真实面板、RTC短读、ESP启动和无线拥塞。串行主循环仍受单次阻塞I/O/面板时间影响，不能宣称硬实时零延迟。

最终候选：`artifacts/bridge-macos/note4-0.18.103-independent-clock.bin`，1,853,856 B，SHA256 `60bddad40814294b8a57c4cb8e35c7eaaf809c30c04322098c9e3477e13a2efc`；匹配ELF/map同前缀，marker为Note4 .103。


## 14:48部署与用户验收

用户明确授权部署.103并测试。通过正式Bridge firmware_ota按MAC排队，request_id=note4-clock103-20261015，job_id=2a2782f6，attempt_count=1。上传ACK后继续认证，最终succeeded/image_verified：运行MAC 7C4FADB93408、版本0.18.103-note4、target zectrix-note4-400x300、image_bytes1853856、完整SHA256与上列候选一致；独立认证确认ota_1、manual/provisional均0，.102保留ota_0。未按BOOT、未接USB、未重建或重启Bridge。证据artifacts/bridge-macos/clock103-ota-queue.json、clock103-ota-verified.json、clock103-live-deployed.json。

14:49用户回复“时钟已及时更新, 不做进一步复测”。依指示停止后续控制/采样，没有下发人为错相Sleep计划。验收区分：镜像部署由认证证明；现场显示及时由用户目视确认；非整分跨界的受控回归在Fake ROM已通过，本轮未完成实机相位/屏幕秒级配对，不能把用户确认扩大为所有场景17s量化消除或硬实时保证。上文“尚未部署”为候选阶段记录，以本节为准。
