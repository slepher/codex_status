# Note4 / 书桌屏频繁激活调查

2026-10-27 21:22，Asia/Hong_Kong。用户要求检查；本轮只读网络/运行数据和源码，未操作串口、改计划、重启 Bridge 或刷机。

## 现场结论

两台均运行 `.107`，不是持续不能休眠。Bridge 的正式 Light 和周期诊断共同造成频繁显示激活：

- 20:49:21 Note4 Plan8552、20:50:41 书桌屏 Plan1674 各接受600秒；同一联合BLE事务分别确认Data1239/Data323。真实push变化按当前规则获得10分钟Light；本轮未比较变化前后字段原值，不断言具体哪个配额字段变化。
- 21:01–21:15日志有多次 `delivery=idle / needs_http=false` 和BLE Sleep ACK，证明期间回到普通休眠联系流程。
- 21:16:40 Note4、21:17:20书桌屏均为 `delivery=idle / wifi_jobs=0 / ota_state=succeeded / diagnostics_due=true`，随后分别接受Plan8568/1690的120秒正式Light。这部分是周期诊断引发的不必要正式激活。
- 相同ID重复发送未续期：Note4剩余87→58→28秒，书桌屏93→66→38→7秒。USB未连接、手动窗口为0；没有新的用户显式Light待办。reset_reason=8为深睡复位，未见本次样本支持panic。
- 21:19:57 Note4 Plan8570、21:20:53书桌屏 Plan1692均已收到Sleep ACK/remaining=0。这是Bridge收到的认证BLE回执；未测量板级电流。书桌屏正式期限归零后曾进入单独有界 `WIFI_SYNC_ONCE / light_exit` 诊断收尾，不能把临时mode=light等同正式计划仍有效。

## 源码原因

`bridge/crates/app/src/rendezvous.rs` 把 `sync.due/pending` 纳入 `needs_http`，将其传入 `plan_for_rendezvous_network_work(..., network_work)`。`bridge/crates/core/src/platform/service.rs` 在 `network_work=true` 时，允许已有网络连接视图的 `waiting_for_window/connecting/...` 参与 `wifi_window_wanted`；即使没有Wi-Fi操作、没有push变化，也会授予 `WIFI_READ_LIGHT_S=120`。

`ConnectionFacts::view` 在没有新鲜HTTP/网络观察时默认返回 `waiting_for_window`；已有连接事实与网络目标匹配即可进入上述分支。这是源码解释，与两台“仅诊断却获120秒”的日志一致；本轮未给运行程序增加内部布尔日志。

现有 `wifi_idle_connection_does_not_authorize_periodic_light_but_real_work_does` 回归只断言 `network_work=false` 时Sleep，同时明确把 `true` 当作正式Light工作，未区分“只需诊断HTTP”和“确有正式激活意图”。修复应区分这两种意图，保留已有诊断临时窗口与真实Wi-Fi操作/OTA/物理窗口；不通过关闭同步或缩短正常push计划掩盖问题。

## 证据与边界

私有脱敏提要：`artifacts/wifi-families/activation-diagnosis-20261027.json`；原日志：`bridge/target/debug/data/logs/bridge-app.log.2026-10-27`（UTC）。认证读由本机MCP `platform_status_refresh / platform_power_view` 完成，Note4在睡眠时HTTP超时，后续BLE Sleep ACK可见；书桌屏HTTP核对同MAC成功。未把HTTP超时单独判定为休眠。

本次完成原因检查，未修生产源码/部署。后续验收归既有B6/A9电源与会合边界：无push/无Wi-Fi操作的纯周期诊断不应创建120秒正式Light，同时真实工作仍能激活。
