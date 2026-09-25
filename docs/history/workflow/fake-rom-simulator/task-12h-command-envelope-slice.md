# Task 12h — 同源 v2 命令信封与 ACK JSON

状态：宿主验证完成，待提交。前置：claim 已提交 `f030dac`。

## 决策

Data/Plan/Bundle/Activate 业务决策已共享，但 `main.cpp::v2Command` 的 JSON/session/MAC/request 检查与 `v2Ack` JSON 构造仍只在设备端。抽取纯 C++ 信封功能，设备端保留 endpoint token、owner 检查、nonce 生成与 HTTP/BLE 发送。宿主端点日后直接调用同一信封功能，避免重复协议判定和字段格式。

## 实现边界

- 新建 `src/v2_command_envelope.{h,cpp}`：`v2ParseCommand(body, JsonDocument&)` 只反序列化并返回 `json` 错误；`v2CheckCommandSession(doc, currentMac, sessionNonce)` 校验 `protocol==2`、设备完整 MAC、非空且最多 64 字符的 request_id、当前 session_nonce，返回 `session` 或成功。两者不得调用 owner/token/时钟/RNG/响应。`v2Command` 保持原顺序：parse 失败原 command ACK；然后 `v2OwnerOk`（同 id touch、冲突 409）；然后把 request_id 写入 `v2RequestId`，再校验 session（生成 nonce 仍只在此时），失败原 command ACK。不能提前生成 nonce 或触碰 owner。
- 同文件提供 `v2BuildAck`，显式输入 op/result/display/retention/error/seq/planId/context/acceptedRemainingS/fwTarget，输出原 `v2Ack` 的 JSON 字段和省略规则，不包含 transport 特有的 BLE `ack/request_id` 注入。main 的 `v2Ack` 只调用该函数再走原 `v2Response`。必须保留 HTTP 200、BLE 通知、token 处理和字段顺序/语义。
- render FFI 编译新 C++，测试 malformed JSON、owner 检查之前可解析的 bridge_id、错误 MAC/protocol/request/nonce、64 字符边界、ACK 可选字段省略/包含；对主路径旧 `v2Ack` 的 JSON 形状做断言。owner 检查不在共享纯函数中，测试不得误声称认证已共享。
- 不接宿主 HTTP/BLE 或真实 endpoint token，不触主 Bridge/实机，不构建/烧录 ROM。

## 验收

`cargo test -p bridge-render --test v2_state` 与 `git diff --check` 通过；主代理核对 `v2Command` 的 owner/nonce/request 顺序。下一步共享 `/v2/status`，再开始 Stage D。
