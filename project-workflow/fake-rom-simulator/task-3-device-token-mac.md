# Task 3：设备操作 token 按 MAC 隔离

状态：2026-09-24 已完成并通过宿主验证。只改 `bridge/`。此任务处理 `/claim` 和 OTA 的设备操作 token；与 v2 业务 endpoint token 是两种凭据。另一代理正在改 ROM，严禁编辑 `src/`、固件构建文件或其任务目录。

## 已定接口与行为

1. `bridge_mcp::load_device_token(cfg, expected_mac)` 和 `fetch_device_token(cfg, expected_mac)` 必须接受明确的目标 Wi-Fi MAC。新缓存路径为 `<data_root>/device-token-<12位大写MAC>.json`，JSON 至少含 `device_mac`、`token`、`updated_at`。读取时校验文件名目标与正文 MAC 一致、token 为现有 32 字符格式；写入只写对应 MAC 的文件，不打印 token。原 `<data_root>/device-token.json` 保留但不再作为多设备 token 来源：它没有 MAC 绑定，不能安全地自动迁入。
2. `bridge_ble::Pusher::request_device_token(..., expected_mac, ...)` 读取绑定 GATT `info.mac` 后，先与期望 Wi-Fi MAC 规范化比较；缺失或不一致则断开并返回错误，**不得写 CHR_AUTH 的 `{"cmd":"token"}`**。仍保留原 peerBonded 检查。使用 `DeviceIdentity::normalized_mac`，不推断 BLE 地址或名称后缀就是 Wi-Fi MAC。
3. app 当前设备的 `post_claim`/`post_claim_explicit` 从 `ctx.device_mac` 取得目标 MAC，用该 MAC 读/取 token。当前设备 MAC 未确认时拒绝 claim；401 后只为同一 MAC 重新经 BLE 取 token。`post_claim_once` 的 HTTP 目标仍是当前设备 IP，本任务不改 claim/续约调度。
4. MCP `firmware_ota` 在读取 token 和第一次有副作用的 HTTP 请求前，从已取得的目标设备状态提取 Wi-Fi MAC（JSON/HTML 对应的 `MAC` 字段）；若调用参数明确给出 `device_mac`，必须与状态 MAC 相同。未指定时取设备状态 MAC；无有效 MAC 则明确拒绝，不能退回全局 token。若未覆盖 `device_ip`、且配置有 `device_mac`，状态 MAC 也必须匹配配置 MAC；显式 `device_ip` 可选择另一台设备，但 token 必须按该 IP 报告的 MAC 查找/获取。401 刷新只针对同一 MAC。升级后的版本轮询逻辑不变。MCP 工具 schema 增加可选 `device_mac` 描述。
5. 不改固件认证、token 格式、owner/claim 规则，不删除旧文件或现有生产数据。没有目标 MAC 时不得向任意 BLE 设备请求 token。运行中的 Bridge 不重启。

## 文件所有权与验证

编码代理独占 `bridge/crates/mcp/src/lib.rs`、`bridge/crates/ble/src/lib.rs`、`bridge/crates/app/src/main.rs`，以及这三个文件中的定向测试。若需其他 bridge 文件，先报告精确调用点。不得编辑其他文件、`PROGRESS.md` 或工作流文档。测试应覆盖两个 MAC 缓存互不读取、正文 MAC 不匹配被拒、旧无 MAC 缓存不被误用，以及 BLE info MAC 不匹配的纯函数核验。使用独立 `CARGO_TARGET_DIR` 运行 `cargo test -p bridge-mcp`、`cargo test -p bridge-ble`、`cargo check -p bridge-app`，再运行 `git diff --check`；若构建量过大，优先做定向测试和 app check 并报告未完成验证。严禁启动/停止服务、接硬件、部署、提交 Git 或运行 ROM 构建。

## 结果

6-luna high 仅改三个指定 Bridge 文件。设备操作 token 的缓存按规范化 MAC 分文件，旧无 MAC 文件仍在但不再读取；BLE 读取绑定 info 并核对 Wi-Fi MAC 后才写取 token 命令。claim 使用当前确认的 MAC，OTA 先从目标状态页取得 MAC 并核对可选显式/配置 MAC，再按目标 MAC 取 token；401 刷新仍针对同一 MAC。

隔离 `CARGO_TARGET_DIR=bridge/artifacts/device-token-mac-target`：`cargo test -p bridge-mcp` 1 项通过、`cargo test -p bridge-ble` 4 项通过、`cargo check -p bridge-app` 通过、`git diff --check` 通过。BLE/OTA 未实机执行，Bridge 未重启或替换；旧缓存不会自动迁入，下一次相关操作可能需要经绑定 BLE 重新获取 token。
