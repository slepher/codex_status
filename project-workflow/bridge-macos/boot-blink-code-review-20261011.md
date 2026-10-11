# BOOT / BLE / Wi-Fi 代码审查

范围：与用户浅睡HTTP、BOOT关闭手动BLE后Wi-Fi图标blink相关。网络行为与模板只审查，不修改；源码缺口与历史现场根因分别记账。

## 已确认缺口

### R1 Bridge验证失败会主动破坏已连接的Wi-Fi

`src/firmware/radio.cpp::networkFailed` 对所有原因无条件WiFi.disconnect(false)、wifiTerminal=true，没有检查物理BOOT下限。serviceReadyReport交接失败与servicePresence证明不足均可进入bridge_http_unreachable分支。用户要求保持Wi-Fi ON/Bridge OFF，该路径直接违反要求。历史wake5 uptime9318/146363已记录这一错误；不能把后者和用户未记录时间的最后一次BOOT直接绑定。

### R2 真实模板的Wi-Fi图标依赖BLE状态

`radio.cpp::deviceStateText` 在Wi-Fi connected时按实际bleOn返回BLE ON/OFF。现场codex-status-a v4的Wi-Fi亮图标条件为manual=1或device.state=BLE OFF/WIFI CONN；因此manual=0/bleOn=1时消失，bleOn=0后再出现，关联状态可以始终保持connected。`power.cpp::handleBootClick`关闭手动偏好时允许进行中的自动BLE继续；`radio.cpp::serviceBleSession`在v2InRendezvous时直接返回，构成可达中间状态。此逻辑是图标短暂消失的候选路径，尚无历史中间帧证据。

`display.cpp::serviceNetworkGlyph`和`network_display_state.h::NetworkDisplayState`不跟踪实际bleOn；自动BLE结束于会合内部时bleOff也不直接render，v2RendezvousRender(light)无pending数据就返回。于是依赖实际BLE的旧模板可能不能随实际BLE变化及时重绘，之后其他刷新才恢复。缺口既包括显示条件，也包括重绘依赖集合不完整。

### R3 Bridge OFF不反映已知的Bridge不可达

`display.cpp::currentBridgeOfflineMinutes`仅用lastSync时间；阈值为整分钟差>6。模板Bridge图标只看device.offline_mins是否存在，不看network.bridge_reachable/error。正式请求已证明不可达仍可能显示联系新鲜状态。用户要求的即时Bridge OFF尚未落地，不能用Wi-Fi OFF替代。

### R4 缓存证明缺失被当成网络不可达

`bridge/crates/app/src/main.rs::presence_hint`先以45s内同boot_id认证HTTP缓存计算device_http_verified，再尝试异步刷新；交付锁忙直接返回先前计算值。固件radio.cpp::servicePresence只有202且verified=true才算reached，否则每30s累积失败，三次后进入R1。Bridge已经正常回应202，也可能因为缓存陈旧/会话身份变化而被分类为不可达。锁长时间占用或更新失败时有误断网风险；历史现场是否走到这一条件尚无证据。

### R5 BLE会话变化会改变对外boot_id

`transport.cpp::serviceV2Ble`在新connectionGeneration清空v2SessionNonce；diagnostics.cpp认证status把同一nonce作为boot_id。新BLE连接不代表MCU重启，却能改变HTTP会话/boot标签，影响分页事务、presence同boot缓存以及运行镜像核验关联。历史20:17与20:24认证快照nonce不同但同wake_generation/wake_seq；20:21生产日志另有device_http_session_expired。身份变化缺口已证，具体是否造成blink未证；修复必须保持会话重放安全并分开稳定boot身份，不能简单停止nonce轮换。

### R6 现有网络诊断不能还原逐次按键和图标相位

`dev_log.cpp`把正常事件写串口，但`sync.cpp::appendLog`只保留syncDiagnosticText筛选的fail/error/rejected/unauthorized/sync/ota等。BOOT manual、BLE session off、network glyph正常事件均被丢弃；Wi-Fi驱动断开reason在wifi_setup.cpp被暂存、下一次GOT_IP/attempt覆盖，未形成持续事件历史。blink_ticks仅累计blink局刷数量，未记起止时间/来源，也不覆盖所有因图标条件改变造成的可见闪现。既有日志足以证明Bridge失败路径，不足以锁定最后一次BOOT与blink因果。

## 排除与保留

- 健康Wi-Fi、普通手动BLE关闭本身没有WiFi.begin/disconnect；20:42:59–20:43:00只读对照未复现，Wi-Fi保持connected、blink_ticks0。
- 同SSID有IP的activate路径有复用判断；不能声称每次activate都会重关联。
- BLE关闭不等于深睡；物理Light下限、正式Light、临时Data窗口、手动GATT与实际BLE独立。
- EPD失败与连接blink分开：这次采样有BUSY计数增加但blink0；旧wake5一次EPD失败发生在约20:19，不能直接归因于约20:21的操作。
- Fake ROM的物理BLE/GPIO/EPD/RF覆盖边界由Luna测试报告确认；协议/时间模型通过不替代上述真实调用链与硬件。

## Review / 下一步

按测试计划汇总实际通过、环境失败和未覆盖。网络修正须分别处理关联状态、Bridge可达证明、图标依赖与观测事件；当前不发布这些修正。绿灯数据活动候选是用户另行授权的独立实现，不能用它掩盖或声称解决Wi-Fi blink。
