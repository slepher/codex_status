# Task 12d — 同源 Bundle CHUNK 接纳决策

状态：宿主验证完成，待提交。前置：Task 12c 已提交 `bd916c7`。

## 决策

CHUNK 使用 WebServer 原始流，设备端不能把整块读入 String。把会话身份、十进制 offset、重放/新写入判定、每段边界及 END 时的接纳判断抽成共享 C++；文件打开、逐字节重放比对、写入与关闭仍由设备端流式适配。宿主随后使用同一决策，但本切片不假装拥有 HTTP 或文件副作用。

## 实现边界

- 扩展 `src/v2_bundle_command.{h,cpp}`：
  - START 输入当前 `V2BundleRx`、`X-Request-Id`、`X-Session-Nonce`、`X-Offset`、一次取得的 `nowMs`；输出显式 `allowed/replay/offset/error`。请求或 nonce 不匹配、会话过期、offset 为空/非十进制时保持原 `session` 错误；offset 大于当前 offset 为 `offset_or_size`；小于为 replay，等于为新写入。拒绝不修改 `V2BundleRx`。
  - WRITE 接收 START 的 offset/replay、已成功处理字节数、当前片大小和当前 `V2BundleRx`，用 64 位和判断边界；超限返回 `offset_or_size`。重放比对失败 `chunk_conflict`、文件写入失败 `write` 仍由 main 产生。
  - END 接收当前 `V2BundleRx`、offset/replay、已成功处理字节数、`nowMs`；0 字节或 `V2BundleRx::append` 拒绝返回 `offset_or_size`。成功时给出新 offset（replay 保持不变），由 main 在文件关闭后写回。文件 I/O 失败仍按原逻辑让新写入会话失效，重放失败不破坏现有状态。
- `src/main.cpp::handleV2BundleChunkRaw` 保留 RAW_START 的 endpoint token 与 owner 检查顺序，以及 RAW_WRITE 的原始缓冲区与 LittleFS I/O。用共享决策替代相应条件，保持原错误文本、文件关闭、deadline 失效和 `handleV2BundleChunk` 回复。COMMIT 不动。
- 宿主 render FFI 编译并调用三个入口；测试匹配/错身份、过期、非十进制/越界 offset、replay 与 append、大小超限、0 字节、超过 16KB、过期 END，以及拒绝不修改接收状态。每个入口输入显式时间，不能读取宿主墙钟。
- 不接宿主 HTTP/BLE、认证、文件 I/O、COMMIT 安装或实机；不运行主 Bridge。

## 验收

`cargo test -p bridge-render --test v2_state` 与 `git diff --check` 通过；主代理核对 raw callback 顺序与状态失效路径。仍非可运行 Fake ROM。
