# Task 12c — 同源 Bundle BEGIN 决策

状态：宿主验证完成，待提交。前置：Task 12a `02dd043`、Task 12b `e997a72`。

## 决策

Bundle 传输含 BEGIN、原始流 CHUNK 和 COMMIT。先将 BEGIN 中已提交请求重放、CRC/长度、会话身份、忙碌与恢复偏移的业务决策抽为同源 C++；文件创建和 HTTP/BLE 回复仍由固件适配。`V2BundleRx` 已是同源状态，复用它，不另建接收状态机。后续切片再迁移 CHUNK/COMMIT。

## 实现边界

- 新建 `src/v2_bundle_command.{h,cpp}`，定义只读已提交指纹（owner、request、CRC、length、context）、`V2BundleBeginDecision`（显式 `reject/replay/resume/start` 动作、错误、next_offset、候选 `V2BundleRx`）。入口接收已经过 `v2Command` 预检的 `JsonDocument`、当前 `V2BundleRx`、已提交指纹、session nonce、一次取得的单调 `nowMs`。不得更改输入中的接收状态。
- 保持原顺序：仅当 owner+request 同时匹配已提交指纹时判重放；其内容 CRC/长度不匹配返回 `request_conflict`，匹配返回 `applied/unchanged` 和已提交 context。非重放先解析 CRC，失败 `crc`；已有 live 会话但身份不符 `busy`、长度/CRC 不符 `request_conflict`，一致则 `resume` 且不延长 deadline；非 live 时由 `V2BundleRx::begin` 校验长度/身份，失败 `size`，成功产生候选 `start`，由 main 创建空文件成功后才赋值当前 `v2Rx`。所有拒绝与重放不修改接收状态、不写文件。
- `src/main.cpp::handleV2BundleBegin` 在鉴权和 `v2Command` 后调用一次共享函数。`replay` 用原 `v2Ack`，`reject` 用原 `v2BundleError`；`resume` 发送原 offset；`start` 保留原 `LittleFS.mkdir/open/close` 顺序，再赋值候选并回复。原 `v2BundleReplay` 继续供 COMMIT 使用，不在本切片改动 COMMIT/CHUNK。
- 宿主 render FFI 编入同一 C++，只暴露 BEGIN 决策和候选/当前接收状态。测试覆盖新会话、重复 BEGIN 不续期、错身份 busy、同身份内容冲突、过期重新开始、非法 CRC/长度、已提交请求成功重放与冲突；核对 rejected/replay 时状态不变。主代理审查不得误使 BEGIN 自动续租或写入。
- 不接宿主 HTTP/BLE、实际文件 I/O、owner 鉴权和 COMMIT 安装；不触生产 Bridge、其他工作树或实机。

## 验收

`cargo test -p bridge-render --test v2_state` 与 `git diff --check` 通过。仍不可称为可运行 Fake ROM；下一条继续 CHUNK/COMMIT。
