# Task 12a — 同源 Data 命令核心的第一条切片

状态：ROM 文件已交接，在独立 `codex/fake` worktree 串行编码。前置：`bridge-multi-instance-2026-09-24` tag 与本 worktree HEAD 同为 `6d131ac`；本任务不得与其他代理在本 worktree 同时编辑 `src/`。

## 决策

第一条同源路径选 `/v2/data`。现有 `src/v2_runtime.cpp` 的 `v2AcceptData` 已被宿主编译，包含字段、CRC、context 与 seq 校验；但 `src/main.cpp::applyV2Data` 仍独占命令分支、ACK 选择、checkpoint/缓存/显示副作用，因此现有宿主测试还不能称为 Fake ROM 端点。先抽出 **Data 决策核心**，再逐步把认证、owner、存储和显示适配到宿主。Plan、Bundle、Activate、claim 不在本切片内；任何未覆盖请求须明确 unsupported，不返回伪成功。

## 精确实现边界

- 新建 `src/v2_data_command.{h,cpp}`。定义显式输入（是否已配置、编译模板、当前 context、完整 Data JSON、预检读出的 seq、`V2DataSeq`）和结果结构（ACK result/display/error/seq/context、usage、是否首次接受）。唯一实现调用既有 `v2AcceptData`，并统一 `unconfigured/context/rejected/unchanged/conflict/stale/applied` 的分支与 ACK 语义；不能复制一份 Rust 状态机。fields 仍由主路径已有的预检 JSON 序列化，避免多一份输出。`V2DataSeq` 只在原行为允许时推进。
- `src/main.cpp::applyV2Data` 保留已有 endpoint token、owner、protocol/MAC/nonce/request 预检顺序；预检后调用新函数。首次接受时仍按原顺序记录 fields/lastAck/checkpoint、保存 usage、按 HTTP/BLE 分支渲染或标记 pending，最后通过原 `v2Ack` 回复。拒绝与重放不得写缓存或触发显示。**本切片不宣称认证和 side effect 已在宿主共享**。
- 复用 `bridge/crates/render/build.rs` 的现有 C++ 编译与 shim，把新文件编入同一宿主库；在 `bridge/crates/render/src/ffi.cpp` 增加最小 Data 决策入口供 Rust 测试调用。不要 `#include main.cpp`、复制 ROM 业务分支或新增独立模拟器状态机。一个进程只模拟一台设备，全局宿主 shim 仍可在本切片使用。
- ROM 文件已交接；只在当前独立 worktree 修改，并以当前 `applyV2Data` 的入口和副作用顺序为准。不得覆盖其他工作树的 ROM 改动。

## 验收

1. 同一组 Data 输入在宿主执行新 C++ 函数，覆盖合法首次接受、完全重放、同 seq 异内容、旧 seq、错 context、字段 CRC/顺序错误和未配置状态；检查 seq、usage、结果/原因及拒绝时无状态推进。
2. 固件原 `applyV2Data` 走同一新函数；现有 render/v2 测试与新增宿主 ACK 分类测试通过。本切片不构建或烧录实机 ROM；完整设备构建在 Fake ROM 可运行前的独立验收阶段完成。
3. 产物说明明确：这是共享 Data 决策切片，尚未支持宿主 HTTP、claim/owner/session、真实持久恢复、显示副作用、Plan/Bundle/Activate、fake BLE/时钟；不得称为可运行 Fake ROM。下一 task 才继续抽取端点完整链路。
