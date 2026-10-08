# Fake ROM Wi-Fi activate复现（2026-10-08）

用户先授权复现，随后授权更新Bridge；未授权刷机或提交。先取得失败证据，不将模拟RF成功当实机证明。

## 范围与顺序

1. 使用现有Fake ROM及共享生产wifi_setup.cpp，核对模拟器的连接/重启/电源适配边界。
2. 测试正确配置已保存、未启用时不联网；新activate从accepted到WiFi.begin/正确SSID密码，再覆盖成功、有IP前未完成、超时、窗口结束与同ID重放。
3. 通过真实Bridge应用/事务服务联测启用、查询、HTTP确认及PowerPlan收尾；复现实机32条已完成记录满队列及未派发却扣预算，保留失败输出。
4. 核对反复联系是否无实际HTTP工作仍新建activate；将结果区分为同源已证、适配缺口、未复现。
5. 更新证据/PROGRESS/backlog，明确根因与局限。若发现必要模拟接线缺口，仅补最小可观测或故障输入，不在Fake中改生产语义。

## 隔离

- 测试LAA MAC、合成配置与凭据；不复制生产data/密码/owner/token。
- 不停止或重启当前默认Bridge，不操作真实设备。
- 新目录先提权创建；使用独立现有或预建cargo target，保留bridge/target/debug/data；不clean。
- 复现断言直接检查生产调用和状态，避免只断言测试夹具自造成功。记录真实网络客户端与FFI路径边界，不能声称编译了整个main.cpp。

## 状态

2026-10-08 用户追加授权更新 Bridge：先修复已确认历史的安全回收、容量错误反馈与发送前失败不扣预算；反转复现断言并补未知提交/待ACK保护；构建本工作区的Bridge，核对默认实例进程/任务，私有备份运行data后按规定停watchdog/主任务、替换exe并启动；使用应用服务显式重试现有home配置并检查认证设备状态。无需更新ROM，不直接编辑生产state或删除未知事务，不提交。

复现阶段已完成；后续修复和部署亦已完成，记录如下。下方场景表描述修复前证据；容量测试与app错误反馈断言已反转为修复后的应有行为。无实际 HTTP 工作仍新建 activate 的完整调度复现留在 backlog B6，不据本轮启用测试推断。

## 更新与实机结果

- 队列只退役terminal且无待ACK、无未知提交的记录；未过TTL但满容量时移除最旧可退役记录。next_job_seq保留，不改变设备原事务查询/重放围栏。未知/已发送未确认即使本地cancelled/expired仍保护；已确认待ACK也保护。
- unified自动启用入队失败明确返回并存连接错误，首次实际发送才扣attempts；查询同事务不再扣除，发送前失败保留预算。既存错误耗尽通过use_current正常服务显式重试解除，不手改运行state。
- 回归core18/app6/device-sim8通过；显式同源Fake BLE保护满队列错误测试1通过，共33项。git diff --check通过。输出wifi-bridge-update-{tests,app-test,build}-20261008.log；新exe SHA256 38B7CE11F6C9A940481420DC6AEF7D1C57219915C8CDD9296DEDED41DB9FCDDE。
- 按默认实例路径/任务/端口核验后停止watchdog和任务；私有backup保存旧exe与完整data，替换本工作区隔离构建exe，13:55:13通过计划任务启动PID30392/health204/8765和8766。未清data、未刷固件、未提交。
- 真实Bridge应用服务use_current/home/revision1：13:56:04发送activate w3_000000000000004a；随后BLE handoff applied、认证HTTP已连wd21-la/IP192.168.3.177。status查询原事务完成operation_result/error null/receipt_ack_pending=false，connection attempts0/http_ready=true；未更改已有SSID/密码/version8。
- 13:57:34直接认证HTTP再次验证MAC7C4FADB93408/.69-note4/wifi_connected=true/RSSI-20/http.ready=true，boot_id4fc7df6e93b3ac8ae099f560e054ee17。普通PowerShell HTTP曾503，-NoProxy直连成功；不当成设备故障或身份错配。证据artifacts/wifi-bridge-recovery-proof-20261008.json、wifi-after-update-view-20261008.json与wifi-bridge-deployment-20261008.json。长期多轮稳定性仍未由本次验收证明。

## 已运行的复现

| 场景 | 实际结果 | 证明范围 |
| --- | --- | --- |
| 已保存正确配置但未 activate | begin=0、connected=false、active 为空 | 同源保存不会自动启动 RF |
| 合法 activate + deferred poll | begin=1，SSID=FakeDevice，实际 RF 参数匹配候选凭据 | 状态机确实执行了启用；仅布尔比较，不输出密码 |
| 已关联但无 IP | 原事务结果 accepted、Bridge awaiting_confirmation；25s 后 rejected/dhcp_timeout，active 仍为空 | 接收/关联不等于启用成功；失败明确、查询原 ID |
| 关联+IP、同 ID 重放 | committed、active=home；重放 begin 计数不变 | 事务提交与重放；不证明真实 AP 可达 |
| 直接注入正式窗口关闭约束 | rejected/window_closed、connected=false、active 仍为 home | 已选配置与当前连接分离；未证明完整 main.cpp 自动电源联动 |
| 32 次真实 Fake activate + result_ack | 全部 completed/may_have_committed/ACK 标记清除；重开服务及越过 TTL 后，第 33 次 wifi_job_capacity | 已确认历史被永久保留的 Bridge 实现 bug |
| 真实 app automatic_on_link + 同源 Fake BLE | 三次入队拒绝返回 Ok、attempts=3、RF begin=0、无第33次回执，下一次 wifi_retry_exhausted | 未发送就扣预算及吞错误的 Bridge 实现 bug |

app 联测历史使用服务 API 与合成 committed 回复预置；32 次真实设备回执另由 device-sim 集成测试验证。主机网络读数仅在 cfg(test) 提供合成 FakeHost，BLE match/owner 与自动启用控制流来自真实实现。没有伪造 app 自动启用函数的结果。

新增 Fake 控制端点 RF 观察返回 begins/SSID/connected/has_ip/busy/stage/error 与候选凭据匹配布尔值，`window_ms` 调用生产 wifiSetupConstrain；保留控制 token，不输出凭据。没有修改 src/wifi_setup.cpp、core 队列策略或生产 app 自动启用代码。

## Fake 的现有限制

- `/api/status` 的 `radio.wifi_connected` 和 `transports.http.ready` 硬编码 true；无 DHCP 时仍如此。测试直接观测同源 RF HAL/事务结果，不把测试载体 HTTP 200 当真实 Wi-Fi 联网。
- outcome=1/2 是 HAL 状态注入；它在 poll 前注入，可以让延迟启用认为已存在目标连接。RF begin 验收选择新启动、无旧连接的场景；同 ID 重放按前后计数比较，不假设每次新 ID 都必须增加 begin。
- 只编译 Wi-Fi 状态机/存储及已有共享组件，没有编译整个 main.cpp、ESP-IDF Wi-Fi 驱动或真实 AP/DHCP。窗口约束由测试显式注入，未自动还原全部电源/lease/handoff 调度。
- 当前设备等待 Bridge、容量与预算阻挡已形成同类复现；初次网络关闭原因、无 HTTP 工作仍反复启用、设备端真实 RF 异常仍需独立验证。

## 重跑命令（Windows，提权/隔离 target）

在仓库目录依次执行，不并行使用同一 target：

```powershell
cargo test --manifest-path bridge/Cargo.toml --target-dir bridge/target-note4-deploy -p device-sim --test bootstrap wifi_activate_repro -- --nocapture --test-threads=1
cargo test --manifest-path bridge/Cargo.toml --target-dir bridge/target-note4-deploy -p bridge-app wifi_activate_repro -- --ignored --nocapture --test-threads=1
cargo test --manifest-path bridge/Cargo.toml --target-dir bridge/target-note4-deploy -p device-sim -p bridge-app wifi_ -- --nocapture --test-threads=1
```

app 测试要求同 target 先构建 device-sim，因此显式 ignored，避免单独 cargo test -p bridge-app 时缺少外部测试二进制。生产托盘不启动。输出保存在现有 `artifacts/wifi-activate-fake-{device,bridge,regression}-20261008.log`。

相关回归发现旧夹具仍以4条存储上限构造容量拒绝，实际共享仓库已支持12条。将第二条新增配置改为无可复用凭据的 credential=keep，得到真实 credentials_required，保留第一条候选更新后整批拒绝/持久回执/不部分修改断言，不更改生产容量或错误语义。不能用5条批量输入替代：它属于进入事务前的直接拒绝，不生成本测试所需的持久拒绝回执。

最终验证：Wi-Fi 相关常规回归 app 6/6、device-sim 8/8 通过；app 外部 Fake ROM 联测显式 --ignored 执行 1/1 通过（常规套件跳过该项，不计为通过）。git diff --check 通过。仅有既存 bridge-ble endpoint_payload 未使用警告；未运行全 workspace/实机验收。
