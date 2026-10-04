# BLE→Wi-Fi 会合协议审查（2026-10-04）

后续实施更新（2026-10-04）：本报告保留审查时的坏行为证据与因果边界。当前协议模块候选、全部Astra发现和生产app组合回归已实现，未部署；见 [实施审核](rendezvous-protocol-review.md)，待办仍归backlog B6。下文“未实施”均指本次历史审查时点。

## 结论与范围

确认存在跨任务的窗口生命周期缺口，可使 Wi-Fi 方案任务长期等待；普通 BLE ACK 本身并不承诺 HTTP 已就绪。此次只审查、复现与记录，未改生产协议/应用、刷机、打开串口或写真实网络。新增两个行为刻画测试，不把“测试通过”解释为问题已修复。

## 审查计划与假设

沿真实日志→Bridge BLE协调→固件入网→HTTP→持久Wi-Fi任务→关窗核验：
1. 排队读任务是否进入 PowerPlan 工作集合。
2. 诊断 complete 是否在后续 Wi-Fi 作业前结束可用窗口。
3. 入网时序与就绪探测是否一致；Fake 是否模拟该交接。
采用现有生产 PlatformService 与 DeviceSimulator 的生产协议/门禁实现，避免创建新测试框架。


**归属更正：本工作属于原W4/A02等会合任务及W6组合验收，原设计第5节、第8节已要求完整交接。审查报告是证据载体，不是新增专项或新需求。已纠正执行文档完成状态，详见其第14节；BOOT只用于调查/物理维护授权，不作为日常交接修复。**

## 真实日志可证明的事实

- Note4 MAC7C4FADB93408，登记IP192.168.10.101。
- 第5次读任务实际最近尝试为 **10-03 23:43:01**（1791042181），waiting_rendezvous、error=http_status_unavailable、snapshot=null、sent=false。
- **23:44:02**另一次普通BLE会合：status applied，plan264 applied，但 accepted_remaining_s=0；wifi_authorize仅记录transport acknowledged，未记录业务ok/错误；该次无sync_open命令。
- **23:44:07–13** /api/status 超时；之后继续传输失败，独立公共/status.json探测也不可达。
- 不能把23:43任务尝试与23:44普通轮询当作同一个事务，更不能从 acknowledged 推断 wifi_authorize.ok=true。
- 这次实机没有授予正式light窗口的直接证据；尚无设备端同wake Wi-Fi关联/DHCP/HTTP日志，因此具体为何不可达仍不能在“深睡、入网、IP、RF”等候选中定案。

## F1：读取意图与可执行机会没有完成闭环

生产 ui action(get)只queue读任务；service.plan_for_rendezvous的work_want_light检查合同、data、activation、Bundle/OTA等，**不检查wifi.jobs**。ble_cycle也只因周期/诊断pending发sync_open；普通Wi-Fi作业最多触发一个短wifi_authorize，不能开Wi-Fi。

证据：
- bridge/crates/app/src/wifi.rs:46–57、283–299。
- bridge/crates/core/src/platform/service.rs:2367–2408。
- bridge/crates/app/src/platform.rs:2184–2223。
- 新测试 rendezvous_audit_queued_wifi_read_does_not_change_powerplan：queue后在101/161/1001三次真实生产service计划仍sleep，任务未结束。

“借用已有窗口、不自动延长RF”是执行文档§5的明确策略，因此不能单独把不自动grant判成违反既定策略；但界面只说等待下次会合不足以说明普通BLE会合不能执行。若没有其他业务/BOOT/周期窗口，该意图没有独立完成保证；与F2组合使自然周期机会也不可靠。

## F2：诊断完成与其他任务的关窗条件冲突（已复现）

Bridge ble_cycle在link.close后按 **sync_http_inner → wifi::opportunity_locked** 顺序执行。固件handleSyncTransfer遇到完整诊断complete且无新义务，设置syncExitAtMs=millis()+200；loop到期且无正式light期限就enterDeep。关窗只知道诊断已完成，不知道Bridge尚有方案任务。

证据：
- bridge/crates/app/src/platform.rs:2223。
- src/main.cpp:4119–4120、5783–5787。
- Fake同义门禁：device-sim/src/main.rs:1271–1278，sync_complete后立即关闭sync_open，无light则deep。
- 新测试 rendezvous_audit_sync_completion_closes_window_before_wifi_snapshot：真实生产HTTP客户端在窗口中可读完整快照，drain诊断后同一客户端读取超时。**Fake立即关闭，实板有200ms宽限**，因此证实的是窗口归属/顺序冲突和竞态，不声称实板每次必败；快请求可能偶然赶上。

该缺口是应用调度与协议生命周期的正确性问题，不是本次证明的越权/密码泄露安全漏洞。23:44实机无sync_open，不能把F2直接套成截图那次超时的唯一根因。

## F3：接受与就绪分离是正确的，但后续等待不统一（静态证据）

固件sync_open先设置标记并ACK；v2Rendezvous随后关BLE、渲染，再startNormalMode入网/注册HTTP。connectBest扫描上限15s，保存bond候选和legacy入网各最多30s；HTTP尚未开启时BLE成功回复完全可能。不能让所有status/data/plan ACK都阻塞到Wi-Fi准备好，普通深睡会合本来不打开Wi-Fi。

诊断transfer已有15s单次状态请求×3和1/3s间隔；Wi-Fi任务仅2s状态请求一次，失败退到下一机会，取得status后才开始8s预算，没把设备剩余窗口期限纳入它。HTTP客户端对connect/write/read分别设timeout，不是整体请求deadline。此处缺少统一的就绪/剩余预算判断；本轮未注入真实数十秒association故障，列为静态确认的设计风险，不冒充实机根因。

证据：src/main.cpp:4687–4821、5059–5071、2303–2400；app/src/sync_diagnostics.rs:416–431；app/src/wifi.rs:314–336；core/src/device_client.rs:37–39。

## 为什么前几轮回归没有挡住

此前分别修复字段合同/remaining默认push、重启context确认、Wi-Fi存储与原子事务、UI布局；这些有自己的通过证据，不能代表同次会合任务已闭环。
现有wifi_jobs_use_production_hal_real_ble_ack_and_query_after_lost_http_ack测试先注入120smaintenance，再直接调用svc.wifi_attempt与HTTP transport，**绕过生产app的ble_cycle、sync_http_inner、opportunity_locked先后顺序**。Fake light_plan成功后立即power.light=true，HTTP gate可用，未模拟实板BLE关闭后扫描/关联/DHCP/HTTP启动延迟。已有sync-only测试验证诊断正确关窗，但没有同时排Wi-Fi任务。缺失的正是跨模块组合与实机关闭时序。

## 实际测试与证据

新增仅测试代码 bridge/crates/device-sim/tests/bootstrap.rs 的两个rendezvous_audit_*。
命令：
```sh
CODEX_STATUS_ARDUINOJSON="$PWD/artifacts/bridge-macos/deps/ArduinoJson-7.0.4/src" \
NO_PROXY=127.0.0.1,localhost no_proxy=127.0.0.1,localhost \
cargo test --manifest-path bridge/Cargo.toml --locked -p device-sim \
  --target-dir artifacts/bridge-macos/target --test bootstrap rendezvous_audit_ \
  -- --nocapture --test-threads=1
```
最终2 passed，1.34s；日志 artifacts/bridge-macos/rendezvous-audit-tests-final.log。
首轮1 passed/1 failed：测试初始已light却虚构light_enter义务，生产sync begin正确拒range；修正为真实light_exit后两项完成。首轮不是目标缺陷的red信号，日志rendezvous-audit-tests.log保留。
两个PASS刻画并证明现有坏行为，修复后应转成“任务在合法窗口内完成/窗口关闭遵守任务集合”的回归，不能继续要求坏行为存在。

## 原任务补完与验收

1. 明确任务机会：显式只读意图借已有窗口时，显示“等待Wi-Fi窗口/需要BOOT”，而非承诺每次BLE会合均可执行；如要自动按需申请，需明确独立任务原因与有限预算，仍只能正式PowerPlan开窗口，不能偷用periodic或读/传输续租。
2. 完成共享窗口内已准入的只读作业后才释放窗口，或保证独立有效正式期限；诊断complete不能提前终止该集合。维护写权限继续按BOOT原截止，不因普通读任务增加。
3. 接受ACK与HTTPready分开；同wake/MAC/会话验证、关联/DHCP/HTTP启动有界等待，共用剩余窗口预算；入网失败/期限不足给具体等待原因。
4. 补组合测试：仅排读任务、普通sleep会合、周期临时窗口+诊断+分页快照、慢入网/换IP、200ms关窗、BOOT窗口到期、Bridge重启；应经过生产app协调，而非直接调用service transport。再做Note4电池/USB各连续多轮实测。
仍归唯一backlog B6，未实施上述修复，运行Bridge与ROM保持分tab部署现场。

## 串口措辞更正

此前“打开串口会复位”过于绝对。当前编译ARDUINO_USB_MODE=1使用ESP32-S3硬件USB Serial/JTAG；DTR/RTS控制序列可触发复位/下载模式，某些监控器/驱动打开端口会改变这些状态，但单纯打开读取不是必然复位。旧Windows现场说明不能直接当作当前macOS必现结论；本轮没开串口，不做破坏现场的实测。应控制DTR/RTS并用boot/wake标识核验是否复位。
官方依据：[pySerial open控制线说明](https://pyserial.readthedocs.io/en/stable/pyserial_api.html#serial.Serial.open)；[ESP32-S3 USB Serial/JTAG](https://docs.espressif.com/projects/esp-idf/en/stable/esp32s3/api-guides/usb-serial-jtag-console.html)及TRM自动下载控制。
