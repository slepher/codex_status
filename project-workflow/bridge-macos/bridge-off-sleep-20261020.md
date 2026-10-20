# Bridge OFF：换地点与主机休眠调查（2026-10-20，B6/C8）

## 现场与反馈环

用户说明设备下班带回家、上班带回，主机期间合盖休眠。目标为登记 Note4 MAC `7C4FADB93408`，固件缓存 `.104-note4`，登记 IP `192.168.31.90`。只读比对真实运行日志、state 白名单字段、pmset 及进程采样；没有按 BOOT、访问串口、刷 ROM 或修改设备配置。

反馈信号：认证联系时间是否推进、BLE 扫描是否继续/找到目标、BLE 隔离是否产生自动恢复记录。当前没有可重复执行的主机睡醒故障复现，不能宣称已定位具体代码根因。

## 时间线（UTC+8）

- 10-19 18:18:49 主机 Clamshell Sleep；18:19:22 DarkWake。
- state 最后完整认证联系 18:19:30/HTTP；日志另有 /api/status 18:20:08.829 HTTP200。后者不等于完整分页认证快照更新。
- 18:20:07 Maintenance Sleep，后续多次后台唤醒。18:49 起日志扫描未发现目标，HTTP 不可达。
- 10-20 06:04:24 再睡眠，06:19:54 DarkWake；06:19:54.099 BLE 报 `ble_native_quarantined_restart_required`。此前最后扫描结果 06:04:06，没有目标。
- 此后 HTTP 持续失败，但没有本次 `BLE service isolated; recovery scheduled` 或后继扫描；持久 ble-recovery.json 仍为昨日14:12故障/14:13恢复，不能当此次恢复。
- 09:47:56 FullWake；09:53–09:55 原受管主进程28876、supervisor28872及MCP8766存活，HTTP仍失败。本机en0=192.168.31.24/24，设备登记.90 ARP incomplete，不能据此证明真实设备未联网或 IP 未变化。
- 09:56:20 同一受管默认Bridge停止/重新启动（保留data/身份/原事务）；09:56:24 扫描恢复，80个周边广播、target_candidate_count=0。这只证明本机扫描恢复。

## 假设与边界

1. 主机睡醒时中断原生BLE操作，进入隔离且自动恢复未运行：隔离与DarkWake同秒支持时间关联；尚不能证明具体native失败或主循环退出原因。
2. 换地点导致目标Wi-Fi/IP变化或设备未恢复当前网络：HTTP不可达与扫描无目标符合；需要同MAC新认证结果或设备网络现场，不将旧IP当实时地址。
3. 主机休眠造成初始失联，回来后两者叠加：先有昨晚失联、今晨才隔离；隔离不能解释全部起点。

源码recover_ble位于run_services的BLE循环顶部；run_services JoinHandle未保存/观察，受管子进程stderr为null。主循环若panic会缺失错误证据，属于可观测性缺口；现无panic文本，不断言已发生panic。线程采样未提供足够async任务状态来证明卡点。

## 证据与后续

私有运行日志：macOS data/logs/bridge-app.log.2026-10-19、2026-10-20。线程采样 artifacts/bridge-macos/bridge-off-20261020-sample.txt。运行exe SHA256 `cc81bf028101309f00b4b472417b737c4b317378fd7d911f8adc766c99e535d6`。没有生产代码改动或重新构建；重启不是根因修复。自然会合结果见下，B6/C8继续开放。

## 重启后第一次自然会合（09:57）

09:57:15无需BOOT即发现目标，GATT连接/服务/info读取成功，完整MAC核对通过，wake_generation=125178307/wake_seq=1100。随后status命令56ms完整写入，4943ms等待后仍无ACK，结果AckUnknown/window_budget_exhausted；断开1ms确认完成，无新增原生隔离。此轮不能更新完整认证联系时间，也没有恢复HTTP。

这证明设备已经回到本机BLE范围，单纯“设备不在场/扫描不到”不能解释当前症状；主机睡醒隔离是一个已解除阻断，设备status ACK链路仍另有失败。generation与缓存不同不能单凭此断言异常重启原因。没有将链路连接或info读取冒充业务恢复。

09:58:27第二轮自然会合再次身份核对成功（同generation，wake1102），status完整写入后4793ms仍无ACK，AckUnknown/device_rf预算耗尽。两轮相同业务失败，可作为下一阶段设备ACK链路的反馈环；不是再次native隔离。当前无USB串口（仅cu.BLTH与Bluetooth-Incoming-Port），不能取得设备侧处理/复位日志。物理屏幕是否仍OFF未独立确认，当前业务联系未恢复。

## 用户授权的 Mac C2 更新（10-20）

### Plan / Task

1. 只读串口确认当前Note4为.105/C2；核对旧Mac程序编码及Windows/Note4部署记录。
2. 停默认受管实例，冻结旧app/data并逐文件哈希验证，保留安装身份。
3. 当前源码构建Mac Bridge，跑帧编解码回归，核对产物与C2；原路径打包签名后受管启动。
4. 同MAC认证状态及自然BLE/HTTP验证；保留原Wi-Fi事务，不重新配网/发布Bundle/刷ROM。

### Status / Review

用户明确要求先编译新版并部署。范围是Mac Bridge，固件/BOOT选网问题暂不改实现。USB身份303a:1001/serial7C:4F:AD:B9:34:08；10:02:36–10:03:24原始open/read，清HUPCL、不发modem ioctl、不操作DTR/RTS、不清输入缓冲，发送只读status/log；实际fw=.105/ota_1，state=off，network_not_found。串口原文存artifacts/bridge-macos/bridge-off-20261020-usb.raw.log（0600），没有观察到ROM复位文本；缺少当前可用认证boot快照，不将此夸大为完整boot不变验证。

昨晚部署记录明确只更新Windows Bridge/Note4；Mac app mtime10-19 15:26早于20:23的C2部署。原Mac缓存.104不是当前设备版本。此新证据替代早先仅依据旧缓存作的推测，休眠隔离单列，不将其认定为当前ACK无响应的唯一原因。

### 编译部署结果

- 旧Mac签名exe的Header::encode反汇编在0x1017899a2写入`-0x3f`，即C1；证据bridge-off-20261020-old-encoder.txt。这与设备.105只接收C2直接不兼容，能够解释跨地点后持续status写入但无ACK。不能再将其仅归主机睡醒/RF。
- 默认受管supervisor62252/child62255已停止，备份目录artifacts/bridge-macos/bridge-off-20261020-backup（0700），旧app+data共167文件/117453447 B逐文件哈希核验。installation-id一并保留，无清数据/回执/配对。
- 当前a908ad8源码执行tools/build-bridge-macos.sh。首轮CLT缺少cstdarg，使用已存在MacOSX.sdk及其usr/include/c++/v1后最小语法检查通过；显式SDKROOT/CXXFLAGS继续，未修改源码/安装工具链。最终构建2m15s，debug本地签名包。
- 新签名exe78169456 B，SHA256 `71c267aa76cd0d7380970a1f1b839e118d1fb9c6b093c59add1f1ecd3f35729c`；codesign --verify --deep --strict通过；实际Header::encode写入`-0x3e`即C2，证据bridge-off-20261020-new-encoder.txt。签名会修改Mach-O，签名前target/debug和签名后app哈希不要求相同。
- 已有compact_frame测试3项通过（opcode映射、golden/bounds、原请求重放围栏），其余测试为filtered，未声称全套通过。日志bridge-off-20261020-c2-tests.log。
- 10:09:14新版MCP8766监听；受管启动supervisor66982。业务自然会合结果待补。未刷固件/发布模板/更改BOOT选网实现。

### 同机业务验收（10:09:43–10:11:06）

- 自然发现并核对同MAC/wake_generation125178307/wake1106；C2 status512ms获得applied，随后wifi_owner/claim/sync_config/wifi_match/Plan全部ACK。与旧版多轮status无ACK形成同机前后对照，不再需要修改设备来解释ACK故障。
- Plan5844已应用，network_start accepted；共享应用服务合法选择已有网络并产生wifi_activate原请求`w3_0000000000000041`。未手工重发历史未知任务或清围栏，也未重新配网/保存凭据。
- 10:09:56.355收到ready report，10:10:07.266独立设备认证HTTP ready；真实.105/ota_1、MAC一致、bridge_reachable=true/error=null、startup_contact=ble。认证boot6ff58ad…/reset_reason8（正常深睡）；串口监听期间没有ROM reset文本，缺少监听前当前boot基线，不夸大为严格前后boot不变。
- 10:11:06 sync-v1 transfer complete与device rendezvous complete。缓存Data seq863/applied863；这是已有已应用数据，未据此声称本轮新Data刷新或屏幕目视通过。
- 默认supervisor66982/child66993，health204、MCP8766监听；签名exe哈希与部署记录一致。实机白名单快照bridge-off-20261020-live-verification.json；串口后置原文bridge-off-20261020-new-usb.raw.log（0600）。
- 仍观察到HTTP读取超时/延迟，未宣称长期稳定或所有错误消失。主机06:19隔离后未自动恢复的独立原因、BOOT无有效选择后旧网络回退的用户预期仍归B6/C8；本轮按用户要求只编译部署Mac C2，无生产代码/固件变化。
