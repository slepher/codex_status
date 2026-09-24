# Task 12e — 同源 Bundle COMMIT 校验决策

状态：宿主验证完成，待提交。前置：Task 12d 已提交 `fa50902`。

## 决策

BEGIN/CHUNK 的共享决策已在 `v2_bundle_command`。COMMIT 的重放、会话完整性、接收文件读回、CRC、owner、job 冲突也是 ROM 业务路径；抽入同一 C++ 文件。设备端仍负责 context 随机生成、`bsInstall` 与安装成功后的状态/显示副作用。宿主使用既有内存 LittleFS shim 执行相同校验，不另写 Rust 状态机。

## 实现边界

- 扩展 `src/v2_bundle_command.{h,cpp}`，提供已过 `v2Command` 预检的 COMMIT 入口。输入 `JsonDocument`、当前 `V2BundleRx`、已提交指纹、session nonce、一次取得的 `nowMs`、接收文件路径、body bridge_id 的 transport fallback（显式字符串）、`String &bodyOut`。结果动作 `reject/replay/already_active/install`，包含原错误文本、重放 context、CRC/length/owner/request 所需的标量。拒绝/重放绝不修改 RX、已提交指纹或存储。
- 顺序与原路径一致：先匹配已提交 owner+request 并按 content_crc/length 判 replay 或 `request_conflict`；其他请求解析 CRC（失败 `crc`）；匹配 live RX 和 length/CRC（失败 `session_or_length`）；打开文件并校验长度（`length`）、reserve（`oom`）、完整读回与 CRC（`crc`）、body bridge_id（`owner`）、过滤解析 job_id（`json`）；若 `bsActiveJobPayload` 命中且 CRC 不同，`request_conflict`，相同为 `already_active`；否则 `install` 并返回已校验 body。复用 `v2ParseCrc`、`v2CrcOf`、`bsActiveJobPayload`，不复制其实现。为避免 BEGIN/COMMIT 两份 replay 逻辑，允许在 C++ 文件内共用一个私有判重函数，但不能改变 BEGIN 行为。
- `src/main.cpp::handleV2BundleCommit` 只在 token 与 `v2Command` 后调用共享函数；`reject/replay` 原 ACK；`already_active` 与 `install` 保留原有 `bsProfile`、committed 指纹、RX 清理、文件移除、`bsInstall`、context/DataSeq/显示副作用及原先相对顺序。删除只供 COMMIT 使用的 main 内 `v2BundleReplay`；`v2BodyBridgeId` 若不再被其他处使用可删。不得在校验失败时生成随机 context、安装或清理当前 RX。
- render FFI 使用内存 LittleFS 写入测试接收文件；测试至少覆盖 replay/冲突、非法 CRC、会话不匹配/不完整、文件不存在/长度不符、body CRC 错、owner 错、JSON 错、需要安装；若可用现有有效 Bundle fixture，再覆盖已有 job 相同/不同 CRC。检查所有拒绝未清理 RX。FFI 可为本测试设置/重置内存文件，不暴露生产接口。
- 不改 BEGIN/CHUNK 行为，不接 HTTP/BLE host，不碰主 Bridge/实机，不构建/烧录 ROM。Bridge 端不得新增独立 Bundle 状态机。

## 验收

`cargo test -p bridge-render --test v2_state` 与 `git diff --check` 通过；主代理逐分支核对 main 的清理与安装顺序。仍不是可运行 Fake ROM。
