# Task 10c — UDP 通知与多设备身份隔离

状态：完成主代理评审与宿主验证。Task 11a BLE 会合路径保持独立。

## 设计

- 仅修改 `bridge/crates/app/src/main.rs`、必要的 `bridge/crates/app/src/platform.rs` 和本任务文档；不触 ROM、真实设备或生产数据。
- 当前 1.54/Note4 ROM 均会发送 UDP announce；监听仍需保留。UDP announce 的 MAC 先规范化。仅已登记 v2 设备可以把它作为地址线索；未知 MAC 不因广播自动登记。广播不改变当前设备选择、全局 `device_ip`、全局 `device_mac` 或 Bridge 运行目标。
- 已登记目标先比较记录端点；相同地址不持久化，也不做无意义的重复 HTTP 核验。改址核验用现有 `registered_device_facts` 和 `device_upsert`，沿用记录显示名，保留服务层已有 profile/同步/观测字段。不要通过全局 `set_device_ip` 或 `learn_mac` 实现。
- 地址变化前要求 `from.ip == announce.ip`，构造严格的 IPv4 端点（广播有效端口或默认 80）；核对结构化 `/status.json` 与认证 `/v2/status` 的完整 MAC 均等于该已登记目标，且 v2 能力有效。失败不改端点、不发 claim/数据；现有登记记录的 profile、同步开关和观测状态保留。已登记目标地址相同时不重复持久化。
- v2 BLE 会合已按登记 MAC 独立轮询，不能依赖 UDP `ble=1`。已验证广播至多唤醒既有 BLE 扫描作为加速提示，不创建目标、不改变设备选择、不绕开完整 GATT MAC 检查。旧单设备 UDP 首识别、全局 IP 修改与 `udp_ble` 路径应消除对 v2 的副作用，若保留代码须显式标成待移除的历史兼容路径。对并发或频繁广播只允许有界核验，不让不相关流量无限创建校验任务。
- UDP 接收循环不产生无限后台任务：对每 MAC 改址尝试限频（建议 10 秒），一次核验最多使用现有 6 秒 HTTP 超时。广播无 `ble=1` 不妨碍当前每 250 ms 检查机会、每目标 55 秒节流的 BLE 扫描调度；本任务不改 BLE 决策。

## 验收

1. 本地 UDP+HTTP 夹具或纯函数测试覆盖已登记 A 改址、已登记 B 不受影响、未知 MAC 不登记、源 IP/状态 MAC 不匹配零改址；不依赖真实设备。
2. 隔离 `cargo check -p bridge-app` 与定向测试、`git diff --check` 通过。
3. 评审确认两个设备的 UDP 互不改写全局选择/IP，多个已登记 v2 目标可独立跟踪端点；无 UDP 时 BLE 会合照常进行。协议日志 Task 11b 再基于该路径加事件。

## 实施记录

- v2 announce MAC 规范化后只匹配已登记 v2 目标；结构化状态和认证 `/v2/status` 双重 MAC/能力验证成功后，按 MAC 更新服务端点，并沿用现有名称与 `device_upsert` 的 Profile、同步、观测保留语义。
- HTTP 地址变化要求 UDP 源 IPv4 与 announce IP 相同；端口缺省或 80 使用裸 IPv4，非 80 端口保留 `IPv4:port`，显式非法端口拒绝。每 MAC 地址核验限频 10 秒，复用 6 秒 HTTP 超时；同地址不核验、不写入。
- v2 UDP 路径不读写全局选择/IP/MAC、不触发旧全局推送或 BLE 决策；BLE 多目标扫描仍按既有 250 ms 检查和每目标 55 秒尝试节流运行。历史 UDP 全局路径仅保留给已登记 legacy 设备，并注明待移除；未知 MAC 不自动识别/登记。
- 纯函数用例覆盖 A 改址线索、B 记录不变、未知 MAC、源 IP 不符、同址/端口 80 不触发核验及非法端口拒绝；注册身份用例验证两份状态 MAC 必须一致。隔离 `CARGO_TARGET_DIR=bridge/artifacts/task-10c-target` 下 `cargo check -p bridge-app` 通过，定向测试 3 项通过，`git diff --check` 通过。未启动 Bridge、未操作实机、未提交。
