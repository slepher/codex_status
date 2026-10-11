# Note4 OFF / ON / BOOT / blink 现场调查（A9/B6/C8）

更新时间：2026-10-11 20:31 香港时间。用户要求先解释现象，暂不再修改固件/Bridge或版本。以下为只读证据与现有实现分析，不是新修正验收。

## 已部署与测试边界

.83 已通过 OTA 单次上传、image_verified，并认证核对运行镜像 SHA256；配套 Bridge 已替换，原 exe/data 已备份。20:31 默认 Bridge PID75952、watchdog PID44608，8765/8766 均属于前者，exe SHA256 为 `5A205444A78893ADBBA6629195EA447A14EB5B0C9A8101DA4B668CDBAC6D18EB`。

跨正式 Light 到期驱动测试无效：为隔离并发投递而暂停默认 Bridge，也移除了设备必须回报的 HTTP session/ready 接收方。驱动本身没有实现该接收方。设备实际已连 Wi-Fi，但 Bridge 回报失败，固件提前主动断网；不能把后来超时归因为正式 Light 到期，不能据此判定 .83 窗口修正通过或失败。Data463 没有由驱动发送。用户 BOOT 又引入 300s 物理 Light 下限，不能算纯自然 timer 场景。

## 时间线与证据

- 20:17:48：BLE 认证 wake_generation1708851262/wake_seq5，BOOT provisional 剩299s。正式 Plan3547/Light60 ACK；reason=data/seq463 的 sync_open 与原 open_id 重放均 ACK；network_start accepted。
- 20:17:54–55：认证 HTTP 已连接，RSSI -34；正式 Light 剩55s；临时 `WIFI_SYNC_ONCE` 剩84430ms、data_pending_seq463；物理 Light 剩293s，但手动 BLE 与 BLE 射频均为 false。wake_cause=ext1，reset_reason8。
- 已归档诊断 batch524，经逐记录 CRC32 核对，wake5 uptime9318ms 记录 `attempt failed: bridge_http_unreachable; BLE report next`，而不是 wifi_connect_failed。这比60s正式到期早，不能作跨到期失败证据。wake5 另有91.858s临时窗口耗尽、146.363s Bridge 不可达记录。
- 驱动20:18:51读取失败，20:18:54超时退出；只有到期前快照，没有到期后/完成快照。
- 默认 Bridge 恢复运行后，20:20:58 BLE 联合 Plan/Data ACK seq464/plan3548；20:20:59 wifi_match、wifi_owner、wifi_activate、network_start ACK，20:21:01.996 收到 ready_report。用户没有更改配置，Bridge自动激活已有配置并重新关联，足以解释无按键时 OFF→ON。
- 20:21:34 生产诊断传输 complete，checkpoint/client_serial457 完成；20:24:54 Wi-Fi connected、blink=null、BLE manual/radio=false，sync无pending/due，正式 plan3549剩80/240s，物理Light剩75s。屏幕计数24次写入、3次BUSY失败，不能保证每个短暂图标都及时呈现。
- 后续生产日志20:28:05出现同generation/wake_seq7、Sleep plan3551/remaining0 ACK。存在后续wake，但缺完整新wake原因/退出记录；HTTP不可达本身不能证明Sleep，暂不扩大实机验收结论。

私有原证据：[驱动事件](../../artifacts/note4-83-live-cross.private.json)、[驱动日志](../../artifacts/note4-83-live-cross.log)、[认证观测](../../artifacts/note4-83-observe.private.json)、[设备错误流](../../artifacts/note4-83-phenomena-log.private)、[PM统计](../../artifacts/note4-83-phenomena-pmstats.private)。生产 Bridge日志位于真实data/logs，诊断batch524位于真实data/platform/diagnostics/7C4FADB93408；不复制凭据到文档。

## 解释与产品语义

1. OFF并不证明用户改过网络配置。`radio::networkFailed` 对 bridge_http_unreachable 同样调用 WiFi.disconnect(false) 并设置wifiTerminal；`wifiPhase` 还把正常深睡关网映射Off。现有文档也包含“已决定深睡关网”语义。它与用户现在明确要求“OFF仅表示Wi-Fi无法成功连接”不一致，是跨层状态与显示语义缺口；现场错误记录已确认Bridge不可达路径，不能把深睡分支说成刚才OFF的唯一原因。
2. 初次从深睡按BOOT使用`BleRecoveryWindow::startup`：300s物理Light、120s临时配对授权，不设置active所需的until；当前协议明确不显示手动GATT图标。进入Wi-Fi网络阶段时自动BLE可正常释放，保持唤醒期限不因此消失。清醒后短按才toggle手动120s GATT：再次按可关闭，期限到也会消失。与首次快照一致；归档文本只保留fail/error/sync等，不保留逐次BOOT日志，无法证明用户随后每一次按键的具体开关方向。屏幕BUSY失败亦是显示不及时的可能因素，不作已证根因。
3. 用户确认blink指Wi-Fi图标闪烁/交替消失，并明确“蓝牙消失正常，随后blink异常”。`serviceWifiMetadata`在关联/等待IP时交替wifiBlinkOn；`templateStateText`暗相位为WIFI BLINK OFF。实际codex-status-a v4只在WIFI CONN亮相位显示连接图标，暗相位没有命中图标。20:24恢复视图blink_ms1000、blink_ticks3证明运行中发生过blink绘制，不能证明最后一次短按的独立因果。不能因为它符合连接中显示代码，就把健康Wi-Fi下出现blink判为正常。

关闭手动BLE的处理器本身不调用WiFi.begin/disconnect。Bridge automatic_on_link在wifi_match.connected=true时返回；固件wifi_setup即时/延迟激活均在同SSID已连接时复用连接，wifiSetupConnecting也要求未连接或无IP。健康连接下没有发现“只因关蓝牙就应重关联”的直接路径。待处理的延迟激活允许普通BLE关闭后继续，因而“蓝牙结束后启动既有恢复任务”是有代码依据的候选解释；现场已确认此前Bridge失败断网及20:20:59自动开网，但20:21–22日志没有新增wifi_activate/network_start。缺最后一次按键的准确时间、逐次BOOT记录和关联事件，无法锁定其blink是否属于这一轮恢复、旧绘制/失败重绘或另一条连接状态路径，保持未定。

4. 用户明确正确行为为BOOT保持期限内Wi-Fi ON、Bridge不可达显示Bridge OFF，暂不修。wifiPresenceUntil确实合并300s物理Light期限，但networkFailed没有检查该保持期限就主动disconnect；保持唤醒不等于保持关联，行为缺口已确认。另有独立显示缺口：模板Bridge OFF来自device.offline_mins；currentBridgeOfflineMinutes仅看rtcLastSyncEpoch，整分钟差>BRIDGE_LOST_MIN(6)才返回存在值，且接受Plan/Data会markSynced。它不读取network.bridge_reachable/error，所以明确Bridge不可达也可能继续显示新鲜联系图标。协议“联系新鲜度”与用户要求的即时Bridge OFF需进一步统一，暂不改模板或协议权威合同。

附：认证字段boot_id实际复用v2SessionNonce，新BLE连接generation会清空nonce；20:17快照与20:24快照nonce不同，但同wake_generation/wake_seq，不应仅凭boot_id变化判复位。

## 后续边界

### 20:42–20:44 只读按键对照

Bridge始终保持PID75952/watchdog44608。无USB串口；仅GET /status.json与/log，约1秒采样，已主动结束采集，未修改正式Plan、Wi-Fi、Bundle。151个成功状态、3次读取失败。20:42:59.412手动BLE开启（剩119s），20:43:00.562手动和实际BLE均关闭；相邻观测Wi-Fi一直connected、wifi_conn=false、blink_ticks=0，BUSY失败计数保持5。用户确认未复现。另20:42:23一次窗口关闭伴随BUSY计数4→5，但blink仍0，不能把EPD失败与连接blink混为一谈。1秒轮询无法排除亚秒瞬态。

归档的BOOT/正常BLE关闭日志为0条，原因是syncDiagnosticText过滤规则，不等于没有按键。旧20:24:54认证快照manual_remaining75s可反推最后一次手动开启约20:21:09（300s下限、采样有秒级误差），不能反推关闭时刻。证据：[对照摘要](../../artifacts/note4-83-boot-observation-summary.json)。

真实codex-status-a v4的when条件计算已确认：Wi-Fi connected时，manual=true/ble=true显示Wi-Fi；manual=false/ble=true不显示Wi-Fi；manual=false/ble=false再显示Wi-Fi。与Wi-Fi关联无关的BLE状态可以改变图标，是独立代码/模板缺口。尚无当时manual=false/ble=true中间帧实证，不能认定为历史根因。

用户已授权绿灯改为传输指示；现有ROM为GPIO3实际BLE射频灯，开关/USB改变触发80ms反转且依主循环结束。新需求是Wi-Fi/BLE应用数据收发闪烁、空闲灭，不等于深睡或手动BLE图标。此需求与网络状态调查分开验收，部署状态以PROGRESS为准。

保持现有.83与Bridge运行，不再停桥制造条件；未修改用户模板/Profile或网络凭据。先向用户说明测试干扰和图标合同，用户要求调查期间不发布新修正。自然全量临时Light、正式期限跨越、同步后Sleep的完整无USB实机矩阵仍未通过，A9/B6/C8保持开放。
