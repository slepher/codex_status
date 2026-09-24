# Task 4：状态读取支持设备 host:port

日期：2026-09-24。状态：完成宿主验证，未提交。

## 目标与合同

Fake ROM 每台设备监听独立 loopback 端口。`bridge-core::device::fetch` 和 `fetch_pmstats` 接收现有的设备地址字符串，支持裸 IPv4（仍默认 80）或显式 `IPv4:port`；本任务不改变设备身份判定，不引入 fake 时钟，不访问真实设备。

`http_get` 使用标准库解析地址：先将完整字符串作为 `SocketAddr` 解析；不带端口时将 `IpAddr` 配上 80。格式错误直接返回带地址的错误。保持原有 `/status.json` 优先、HTML 回退和 PM stats 的行为。此阶段不扩展 DNS/URL/IPv6 合同。

## 编码所有权与验收

- 6-luna high 只修改 `bridge/crates/core/src/device.rs`，在该文件内加最小的本地 TCP 测试。不得修改 ROM、app、MCP、BLE、计划文档或其他文件。
- 测试用 `127.0.0.1:0` 监听器响应 `/status.json`，验证显式端口能获取结构化状态和 MAC；另验证裸 IPv4 解析为 80，可用纯解析测试，不连接端口 80。
- 运行隔离 target 的 `cargo test -p bridge-core` 与 `git diff --check`，汇报结果，不提交、不操作设备。

## 结果

`device.rs` 使用 `SocketAddr` 或裸 `IpAddr` + 80 解析，新增裸地址解析与本地 TCP 状态测试。隔离 `cargo test -p bridge-core`：80 个单元测试及各集成测试组全通过；`git diff --check` 通过。未部署、未访问设备。
