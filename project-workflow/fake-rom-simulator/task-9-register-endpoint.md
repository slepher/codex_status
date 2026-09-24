# Task 9：按 MAC 显式登记 v2 设备端点

日期：2026-09-24。状态：完成宿主验证，未提交。

## 设计决策

新增 app 内建 MCP 工具 `platform_device_register_v2`，参数 `mac`、`endpoint`、可选 `name`。此工具只登记/更新一个 v2 设备，不选择界面设备、不 claim、不发布、不推送、不设置 fake 时钟。现有生产 Bridge 可以用它登记多台真实或以后启动的 fake 进程；fake 身份与实验时钟另由测试环境显式配置，不能因 loopback 地址自动推断。

`endpoint` 仅接受裸 IPv4 或显式 `IPv4:port`，拒绝 URL、DNS、IPv6、空地址、`0.0.0.0` 与端口 0。先通过 `bridge_core::device::fetch` 读取 `/status.json`，要求结构化 JSON 和有效 `mac`，用已有 `caps_from_status` 得到能力，要求 `legacy=false`。再用桥的 endpoint token 认证读取 `/v2/status`，要求其 `device_mac` 与参数及 `/status.json.mac` 全部一致。任何读取或身份/能力失败都不得调用 `PlatformService::device_upsert`。超时用现有有限真实 I/O 超时。登记成功后 `DeviceIdentity.ip=endpoint`、`discovered_via="manual"`、`last_seen_at=now_secs`。未传 `name` 时优先保留现有同 MAC 记录名称，否则用 `CodexStatus-<MAC后6位>`。返回登记后的设备记录。

在 `bridge-mcp` 的 tools/list 增加此工具及 schema，并标记为写入型、幂等；其独立库模式返回“只在 bridge-app 可用”。在 `bridge-app` 的 MCP 转发白名单接入相同工具，调用 `platform::tool`。不新增 UI 控件，不改固件，不自动登记发现到的任意地址。

## 编码所有权与验收

- 单名 6-luna high 只修改 `bridge/crates/app/src/platform.rs`、`bridge/crates/app/src/main.rs`、`bridge/crates/mcp/src/lib.rs`；不设计其他接口或修改其他文件。
- 添加端点解析与双状态身份校验测试：匹配才构造登记信息；参数、状态页或认证状态任一错 MAC、缺能力、非 v2 时失败。验证状态不依赖界面当前选择。不得连接实机。
- 隔离 `CARGO_TARGET_DIR` 运行 `cargo check -p bridge-app`、适用的 app/MCP 定向测试及 `git diff --check`。不提交、不启动生产 Bridge、不访问硬件或运行数据目录。

## 结果

6-luna high 实现了严格 IPv4(:port) 端点校验、`/status.json` 与认证 `/v2/status` 的三方 MAC 和 v2 能力校验，随后才调用 `device_upsert`；增加了 app 内建 MCP 工具及 bridge-mcp schema。主代理要求撤回工具文件的无关格式变化，最终差异仅三个归属文件。隔离 `cargo check -p bridge-app`、app 登记测试 3 项、MCP 测试 1 项和 `git diff --check` 通过。未部署、未连接设备、未提交。
