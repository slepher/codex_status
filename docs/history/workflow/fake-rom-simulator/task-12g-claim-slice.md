# Task 12g — 同源 claim/renew/release 决策

状态：宿主验证完成，待提交。前置：Activate 已提交 `dc445ff`。

## 决策

设备 `POST /claim` 使用设备操作 token，不能改成 endpoint token 或绕过鉴权。将通过 `requestAuthorized` 后的参数净化、release/冲突/claim/renew 决策抽为共享 C++；`ownerGet/ownerClaim/ownerClear` 的 NVS 与 uptime 副作用仍留在设备适配层。后续 Fake ROM 使用同一决策，但必须另接独立存储和显式逻辑时间；本切片不宣称 claim 已完整模拟。

## 实现边界

- 新建 `src/v2_claim_command.{h,cpp}`（名称沿阶段 C，端点仍是 `/claim`），复用 `OwnerRec`。先由纯 `v2PrepareClaim` 从原始 id/name/host/port/lease 文本、是否提供 lease、force/release 布尔值得出净化参数或 `args`；再由纯 `v2DecideClaim` 输入准备结果与当前有效 owner 的存在标志/`OwnerRec`，输出动作 `release_empty/occupied/release/claim`、净化后的请求 `OwnerRec`、`keepSince`、是否新占用。将 main 现有 `claimText` UTF-8/控制字符逻辑原样移入共享文件；id 限 32 字符且 trim 后非空，name 16、host 32，port 1–65535 否则 0，lease 默认 300 且夹至 60–3600。
- 保留原分支顺序：空 id 永远 400 args；release 无 owner 返回 released=false；release 被他人占用且无 force 返回 409 occupied；合法 release 清空；非 release 被他人占用且无 force 返回 409；同 id renew 令 `keepSince=true`，force 抢占或无 owner 为新 claim。决策不调用 `ownerGet`、`ownerClaim`、`ownerClear`、`noteActivity`、`millis`、NVS 或响应 API。
- `src/main.cpp::handleClaim` 仍先 `requestAuthorized`，然后调用准备函数；空 id 必须在 `ownerGet` 之前返回 400，保持原副作用顺序。其余输入只调用一次 `ownerGet` 取得当前有效占用，调用共享决策，随后按动作走原 HTTP 状态/正文、owner store 和日志。renew 不调用 `noteActivity`，新 claim 才调用。安全关键的 `POST /claim` token 必须原样保持。
- 宿主 render FFI 编译共享 C++，不编译 `owner_store.cpp`。测试覆盖 UTF-8/控制字符净化与长度、空 id、release 各分支、冲突与 force、同 id renew、port/lease 边界；检查决策不修改传入当前 owner。宿主 FFI 不使用真实 NVS/时间。
- 不触主 Bridge/实机、不构建/烧录 ROM、不改 owner store 或其他命令。

## 验收

`cargo test -p bridge-render --test v2_state` 与 `git diff --check` 通过；主代理核对鉴权与副作用顺序。Stage C 的共享命令切片至此齐全，但宿主认证、owner 存储、HTTP/BLE、显示仍待 Stage D 接线；不可称为 Fake ROM 可运行。
