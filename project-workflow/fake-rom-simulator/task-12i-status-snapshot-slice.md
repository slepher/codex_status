# Task 12i — 同源 `/v2/status` 快照

状态：宿主验证完成，待提交。前置：命令信封已提交 `c154945`。

## 决策

Bridge 以鉴权 `/v2/status` 为权威设备状态。设备端当前直接在 handler 拼 JSON；宿主需要同一字段与时间语义。将状态 JSON 构造抽成纯 C++，显式输入已采集的设备状态和一次取得的单调 `nowMs`。endpoint token、`markSynced`、nonce 懒生成、实际电池读取/模式判定仍在 main；宿主只能以显式测试状态调用。

## 实现边界

- 新建 `src/v2_status_snapshot.{h,cpp}`，定义最小 `V2StatusSnapshot` 输入：MAC、session nonce、`BsProfile`、是否配置 Bundle、未配置时的 active template id、`V2DataSeq` 的 applied seq、display state、commit seq、当前深睡标志、`V2PlanState`、provisional 标志/bootMs、`nowMs`、battery。允许以只读引用/指针传 Profile、Plan；不能读取全局 `millis`/`esp_timer`、RNG、NVS、无线或显示。输出 `String` JSON，字段和值与当前 handler 一致：result/protocol/device_mac/session_nonce/context/template/job/data_seq/applied_seq/display_state/commit_seq/configured/template_ids/power 全部保留；`remaining_s` 和 provisional 剩余统一用显式 nowMs，保留 `V2PlanState` 原计算。Profile 1–8 全部输出，不裁剪。
- `src/main.cpp::handleV2Status` 保留 endpoint token 401、`markSynced()` 和在其后读取/生成 nonce，收集输入后调用共享 builder 并继续 `server.send(200,...)`。不得调用 claim 或改变 PowerPlan；重复读取不续租。
- render FFI 编译共享 C++，测试未配置与配置态、8 项 Profile、display 四态、Plan light/sleep、provisional 剩余、同一 nowMs 重复快照一致，以及字段名/类型。测试确认快照函数不修改 Plan 或 DataSeq。
- 不接宿主 HTTP/BLE、auth、owner 存储或显示；不触主 Bridge/实机、不构建/烧录 ROM。

## 验收

`cargo test -p bridge-render --test v2_state` 与 `git diff --check` 通过；主代理核对原 handler 字段与顺序。Stage C 代码审查后才进入 Stage D 单设备模拟进程。
