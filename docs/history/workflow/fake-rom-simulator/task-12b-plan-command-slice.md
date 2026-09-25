# Task 12b — 同源 PowerPlan 命令决策

状态：宿主验证完成，待提交。前置：Task 12a 已提交 `02dd043`；本任务仅在 `codex/fake` 独立 worktree 执行。

## 决策

`V2PlanState::accept` 已是固件与宿主共用的计划状态机，但 `src/main.cpp::applyV2Plan` 独占 JSON 形状校验、最大 light 授予值选择和 ACK 分类。将这三项抽为一条共享 C++ 命令决策。设备端 `v2Command` 的认证/owner/MAC/nonce/request 预检保持在前；设备端的 `v2PlanReason`、`v2LightDeadlineMs`、`v2Provisional`、`rtcMode`/`persistMode` 与日志仍按原顺序留在 main。一个进程只模拟一台设备。

## 实现边界

- 新建 `src/v2_plan_command.{h,cpp}`，提供 `v2DecidePlan(JsonDocument &doc, V2PlanState &state, uint64_t nowMs, bool provisional)` 与显式结果：是否接受、`V2PowerPlan`、ACK `result/display/error`、`planId`、`grantedS`、是否附 context。形状无效返回 `rejected/unchanged/plan_shape`、`planId=0`、无 context；旧 ID 返回 `stale_plan`，同 ID 异内容返回 `plan_conflict`，二者不修改正式计划；新计划和完全重放返回 `applied/unchanged`，保留现有授予和截止行为。`V2_PLAN_REJECTED_LIMIT` 如可达，必须显式映射为拒绝，不得当作成功。使用现有 `V2_BOOT_PROVISIONAL_S`/`V2_MAX_LIGHT_S`，不复制计划状态机。
- `src/main.cpp::applyV2Plan` 在 `v2Command` 后调用一次共享函数；拒绝立即以原 ACK 回复，成功仍按现有顺序执行副作用并回复。不要改变设备 token、owner、请求和 PowerPlan 时间语义。
- `bridge/crates/render/build.rs` 编入新 C++ 文件；`ffi.cpp` 用最小入口解析 JSON、传入宿主显式 `nowMs`/`provisional`，返回决策及 `V2PlanState` 观察值。宿主测试验证形状拒绝、light 授予上限与下限、完全重放不续期、冲突、旧 ID、sleep 接受；每类校验 ACK 字段和状态。测试使用临时 `CODEX_STATUS_ARDUINOJSON` 指向已安装的只读头文件。
- 不接宿主 HTTP/BLE/认证/owner/存储/休眠控制，不修改 Bridge 生产行为，不构建或烧录实机 ROM，不启动或停止主 Bridge。

## 验收

`cargo test -p bridge-render --test v2_state` 与 `git diff --check` 通过；主代理核对 main 的副作用顺序与 ACK 无回归。产物仍不是可运行 Fake ROM，后续按 Bundle、Activate、claim 顺序继续。
