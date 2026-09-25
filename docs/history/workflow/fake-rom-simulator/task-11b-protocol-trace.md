# Task 11b — UDP/HTTP 与 PowerPlan/Data 时间线

状态：完成主代理评审与宿主验证。主代理负责合同与评审；执行者只改本文件授权的实现范围。

## 范围与设计

- 主要修改 `bridge/crates/app/src/main.rs`、`bridge/crates/app/src/platform.rs` 与必要的 `bridge/crates/core/src/v2_client.rs`；仅限日志与关联元数据，不改协议线格式、投递判定、截止或持久状态。不触 ROM 文件。
- UDP 基于 Task 10c 的已登记 v2 hint 路径记录目标 MAC、源地址、是否被接纳和拒绝类别（格式错、源地址不符、未登记、双状态身份不符等）；重复和无关流量做计数/限频，防止 250 ms 调度循环与广播流量刷屏。不为日志恢复任何全局旧单设备副作用。
- HTTP v2 的身份预检、Plan/Data/Bundle/Activate/claim 记录目标 MAC、操作、请求关联 ID 或 `seq`/`plan_id`/`job_id`、HTTP 状态/错误类别、耗时。PowerPlan/Data 的发送与 ACK 记录安全的关联字段和结果；不打印原始 JSON。尽量复用现有 request_id，不改变请求体以制造日志 ID。
- `v2_client::post_json` 可从请求体已有 `device_mac`/`request_id` 记录安全发送与结果；HTTP `status` 在核心层记录状态码和耗时，MAC 由调用方目标记录关联。app `platform` 在投递入口记录目标 MAC 与 `seq`/`plan_id`/`job_id`，在 ACK 处只取 `result`、`data_seq`、`plan_id`、`accepted_remaining_s` 等明确字段。`claim` 同样只记录目标、状态和耗时。不要把 `anyhow` 中可能包含 HTTP 响应体的原始错误字符串写入新增协议日志。
- 发送与接收事件使用一致字段名，便于按 MAC 和 ID 排序。错误分类至少区分连接/超时、身份不符、HTTP 非成功、ACK 拒绝/不匹配。日志中不得含 Authorization、token、Wi-Fi 密码、完整请求/响应体或使用量快照。现有 `%ack` 等全量 ACK 记录一并收窄。
- 默认 info 只记录实际关键操作及结果；空闲轮询和重复失败限频/汇总。debug 可以给更多安全元数据，不能增加 250 ms 一条的空闲日志。

## 验收

1. 本地 v2 HTTP/UDP 夹具或纯函数测试覆盖成功、错 MAC、失败和关联字段，不需要真实设备。
2. 隔离 `cargo test -p bridge-core`、app 定向测试或 check、`git diff --check` 通过。
3. 评审从样例事件可按 MAC 重建 UDP→HTTP/Plan/Data→ACK 时间线，确认空闲采样与敏感内容禁区。

## 实施记录（2026-09-24）

- UDP 分类由单一 `classify_v2_udp_hint` 决定 changed/unchanged/rejected；已登记 MAC 的改址与拒绝按 MAC+原因限频，未知身份聚合采样，unchanged 仅 trace。没有恢复旧全局 UDP side effect。
- HTTP status/preflight 与 POST 记录安全关联字段、HTTP 状态、耗时及静态错误类别；claim、HTTP/BLE Data/PowerPlan ACK 仅记录白名单字段。未记录 Authorization、token、响应/请求 body 或快照。
- 所有触及的 HTTP 非成功错误文本只保留状态码/offset，不含请求或响应正文；本地 503 fixture 断言 request secret 与 response secret 均不进入错误文本。409 仍返回既有 rejected/detail 结构。
- 增加纯函数分类/采样测试与本地 HTTP success/failure fixture。
- 验证：隔离 `CARGO_TARGET_DIR=artifacts/protocol-trace-target` 下 `cargo test -p bridge-core` 全绿（82 unit tests 与全部 integration tests）；`cargo test -p bridge-app v2_udp_hint_tests --bin bridge-app` 4 项全绿；仓库根 `git diff --check` 通过。仅有既有 core integration-test dead-code warning 与工作树中其他文件的 CRLF 提示。
