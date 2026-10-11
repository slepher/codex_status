# WIFI OFF 终态与会合协议审核

2026-10-11，香港时间。范围：Note4 同步后持续 WIFI OFF、随后恢复联网；固件/Bridge 会合、诊断准入与显示状态。只读调查和源码审核，未修改实现、未构建/部署、未重启 Bridge、未打开串口。关联 backlog A9/B6。

后续用户要求修正模板元数据：第一项已有源码/模板候选，进度与验收见[实施记录](wifi-template-metadata-20261011-plan.md)。本文保留审核时证据；另两项协议问题仍待处理。

## 现场与证据

登记 MAC `7C4FADB93408`，当前地址经恢复视图 MAC 及认证 Status 双重核对为 `192.168.3.177`；运行 `0.18.74-note4`。用户说明 OFF 持续了一段时间，后来 ON；没有 OFF 起止时刻的屏幕录像，不能把下面时间差当作精确屏幕持续时间。

Bridge 日志时间为 UTC，下表已加八小时：

| 香港时间 | 证据 |
|---|---|
| 11:48:07 | 本轮 `needs_http=false / delivery=ble_data / diagnostics_due=false`；联合 Plan/Data ACK：seq398 applied，plan3413 接受600s |
| 11:48:56 | `sync-v1 transfer pending: sync Wi-Fi association unavailable`，随后本轮 rendezvous complete |
| 11:50:16–18 | 下一轮 `needs_http=true / diagnostics_due=true`；同 plan3413 剩470s，network_start accepted |
| 11:50:21 | Bridge 收到 ready_report |
| 11:50:23 | ready_report 的认证 HTTP 读回 verified |
| 11:50:47 | 持续 HTTP Status 200 后，会合仍报 http_or_transport 失败 |
| 11:53–54 | 直接采样：Wi-Fi connected，Light，seq398/applied398/displayed；认证 rendezvous.ready=false/remaining_ms=0，而正式 Light 仍剩227s |

当前 boot 的恢复视图为 deep-sleep/timer、非插电；这是采样时的启动原因，不证明 OFF 是异常复位。未访问 USB、未改网络或发新事务。

私有证据（不入库、不复制凭据）：
- `artifacts/wifi-terminal-20261011-public-status.private.json`
- `artifacts/wifi-terminal-20261011-auth-status.private.json`（现行48字节二进制 Status，核对响应头及MAC）
- `artifacts/wifi-terminal-20261011-device.private.log`
- `bridge/target/debug/data/logs/bridge-app.log.2026-10-11`
- 设备诊断归档 `bridge/target/debug/data/platform/diagnostics/7C4FADB93408/482-cee28010774d1fb7cd03b1f11946ed5e.json`

`/log` 返回的有限内容含17条 `info bytes=0 precheck_failed`、一次局刷失败升级全刷及认证拒绝，没有完整网络转场。不能据此断言RF故障、密码错误或屏幕残影；INFO失败原因也未证明。

## 审核结论（待修复）

### P1：WIFI OFF 没有终态判据

`src/firmware/radio.cpp:68–85`：deviceStateText 只检查 `!wifiUp` 就返回 WIFI OFF；templateStateText 仅以 wifiConnActive 覆盖。普通 BLE 会合/等待选择网络不设置该标志，startNormalMode 又立即清零。`pollWifi` 在连接丢失时立即绘图，尚未等待丢失处理期限。故“尚未联网”和“本轮已结束且关闭”共用同一终态画面。

本次11:48数据成功走BLE且明确未准入HTTP，因此“刚同步”本身不证明Wi-Fi曾打开。代码足以解释随后显示OFF；仍缺精确屏幕与RF同步采样，不能断言用户所见每一帧的触发点。

修复合同：WIFI OFF 只用于本轮网络机会明确结束且无未完成开网意图；会合/等待网络选择/关联中/HTTP准备中各保留真实中间态。正常BLE-only完成与Wi-Fi失败分开；不能为了隐藏OFF无条件开Wi-Fi，也不能把等待BLE伪装成正在关联AP。状态投影与重绘依据需统一，目前 NetworkDisplayState 只观察 wifiUp 与 Bridge 联系分钟，若只改字符串，同为false的阶段转移仍可能不触发重画。

### P1：BLE-only 准入被诊断收尾绕过

`bridge/crates/app/src/rendezvous.rs`：needs_http 只在诊断due/pending等条件下准入HTTP；但末尾 `sync_opened = opened || (enabled && light)`，不检查本轮HTTP准入/可达证明，随后进入 sync_http_inner_budget。`sync_diagnostics.rs` 又执行最多3次、每次15s Status探测并等待1/3s。

现场11:48:07的 `needs_http=false` 与11:48:56的关联不可用构成直接证据：BLE Data已完成，却继续占用约49s的无网HTTP诊断路径。不能把这个错误归为本轮Data失败，亦不能以延长Light掩盖。

修复/验收：诊断执行沿用本轮冻结准入；未准入HTTP的BLE-only成功立即收尾，新增诊断原因等下一合法机会。覆盖BLE-only+sync enabled+Light时HTTP调用数为零、已有HTTP路径、显式SyncOpen、下一轮due及丢ACK原事务恢复。

### P1：瞬时会合 ready 被当作持续 HTTP 可用条件

`src/rendezvous_protocol.h/.cpp`：ready 仅在 HttpReady 阶段为真；finishExchange 立即切 AckGrace，再到Done。`radio.cpp` 在ready报告收到或达到尝试上限后调用finishExchange；`diagnostics.cpp:1305` 据该瞬时状态返回 rendezvous.ready。Bridge `rendezvous.rs:matching_http` 对显式会合始终要求ready=true，`wait_for_device_http` 对http_not_ready继续循环到预算耗尽。

设备可已完成报告并保持认证HTTP服务，此时Bridge迟到的验证必然读到ready=false。现场11:50:23 verified、11:50:47失败以及之后直接认证ready=false但网络可用与此竞态吻合；该次循环未逐条记录matching_http拒绝码，不能声称每个200的具体拒绝原因均已捕获。

修复/验收：本地HTTP可用事实与会合phase分开；用同boot/session/MAC/owner/plan及原请求关联证明换接，结束会合不撤销仍合法的HTTP。保留旧boot/错误owner/过期Plan拒绝，不简单删除安全校验。覆盖报告ACK先完成、验证后到、已有HTTP直达、报告ACK丢失、到期及重启；应停止无效轮询并保留具体失败阶段。

## 验收边界与执行顺序

1. 先统一上述终态/中间态合同和HTTP准入，补生产路径回归；再修显示与ready投影。
2. 同一wake记录BLE ACK、开网意图、关联/DHCP、HTTP认证、会合结束、显示提交，验证OFF仅在终态出现；BLE-only成功不能被HTTP尾部拖延。
3. Note4构建、匹配ELF栈门禁、运行ROM核验和连续实机周期另行执行；本轮没有候选ROM，不改变当前部署记录。未做全面协议安全审计，未完成B6的RF/功耗/掉电矩阵。
