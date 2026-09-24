# Task 13a — 单设备 localhost 进程与状态端点

状态：宿主验证完成，待提交。Stage C 全量宿主回归 35/35，后续在 `codex/fake` 独立 worktree 实施。

## 决策

Stage D 从真正可启动的单设备进程开始。首切片只暴露鉴权 `/v2/status` 与只读控制状态，业务写入一律明确 `unsupported`，不能返回伪成功。每进程一个 locally administered unicast 虚构 MAC；不同进程用不同 MAC/端口。状态 JSON 必须由 Stage C 的 `v2BuildStatusSnapshot` 同源 C++ 产生。此切片是模拟器启动骨架，尚不是完整 Fake ROM。

## 进程与安全边界

- 新 crate `bridge/crates/device-sim` 加入 workspace，产物 `device-sim`。仅监听 `127.0.0.1`；CLI `--listen 127.0.0.1:<port>`（默认 0）、`--mac <MAC>`（必须显式，规范化为大写冒号格式且首字节为 locally administered unicast，即低两位为二进制 10）、`--seed <u64>`（默认 1）。错误参数启动即失败，不绑定公网/真实 MAC。一个进程只持有一个 MAC。
- 启动时要求三个非空环境变量 `CODEX_STATUS_SIM_ENDPOINT_TOKEN`、`CODEX_STATUS_SIM_DEVICE_TOKEN`、`CODEX_STATUS_SIM_CONTROL_TOKEN`，在进程内分别保存，不打印。stdout 只写一条 ready JSON：`schema_version`, `mac`, `http`（实际监听地址）、`capabilities`（此切片仅 `v2_status`）。其他诊断写 stderr，不包含 token、完整请求体或业务值。
- `GET /v2/status` 使用 endpoint token Bearer，与固件业务通道同一 401 JSON；成功用 `bridge-render` 对 Stage C `codex_v2_status_snapshot` 的安全 Rust wrapper 调用共享 C++ builder，初始状态固定 unconfigured、seq/plan=0、battery=75、无 owner、nonce 由 seed+MAC 确定且只在鉴权成功后可见。nowMs 为本进程启动后的 1x monotonic uptime；状态读取不修改时间/计划/owner。
- `GET /sim/state` 使用独立 control token，只返回 MAC、能力 `v2_status`、明确 unsupported 的 `data/plan/bundle/activate/claim/BLE/persistence/display/clock-control`、当前 uptime；绝不包含任一 token 或 nonce。`POST /v2/data|plan|activate|bundle/begin|bundle/chunk|bundle/commit` 先验 endpoint token、然后返回明确 501 `unsupported`；`POST /claim` 先验 device token、然后同样 501。未知路径 404。`/sim/*` 不能作为业务写入旁路。
- HTTP 请求体与 header 配有有限上限（例如 64 KiB body；Bundle 大块尚不支持），服务保持 localhost；不要尝试主 Bridge 连接/登记、设备、BLE、文件存储或生产数据目录。此切片不提供 `/status.json`，避免 Bridge 把仅能读状态的骨架误登记为完整 v2 设备。

## 代码边界与验证

- `bridge-render` 仅新增公开的模拟状态 wrapper（可复用现有 FFI），不改变预览 API 和已有 C++ 业务。`device-sim` 使用已在 workspace 中的 Axum/Tokio/serde_json/anyhow，不引入新 HTTP 框架或通用插件层；单进程状态用简单结构即可。
- 测试启动 loopback 端口 0 的进程/路由，验证非 loopback/真实 MAC/缺 token 拒绝、ready JSON 不泄 token、三 token 域隔离、无鉴权 401、有效 status 来自共享 builder、写端点 501、未知路径 404、两实例不同 MAC/端口互不串。`cargo test -p device-sim`、`cargo test -p bridge-render`、`git diff --check` 通过。
- 不运行/停止主 Bridge，不接真实设备，不写其他 worktree，不提交，主代理审查后提交。
