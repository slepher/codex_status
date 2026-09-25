# Task 12f — 同源 Activate 决策

状态：宿主验证完成，待提交。前置：Bundle COMMIT 已提交 `b702aa3`。

## 决策

`handleV2Activate` 的 token 与 `v2Command` 预检继续留在设备入口。将通过预检后的 request/owner 重放、expected context 和 Profile 模板索引判定抽为共享 C++。实际 `v2SwitchActive` 涉及 context 随机生成、A/B 存储、DataSeq 重置、模板载入与渲染，仍由设备路径执行；它的失败 ACK 留在 main，但先前业务决策必须与宿主相同。

## 实现边界

- 新建 `src/v2_activate_command.{h,cpp}`，输入预检后的 `JsonDocument`、bundleReady、当前 `BsProfile`、上次成功 Activate 的 request/owner/template/expected/context 指纹；输出 `reject/replay/switch`、模板 index、ACK `result/display/error` 与回复 context。指纹只读；成功切换后的指纹由 main 在 `v2SwitchActive` 成功后更新。
- 保留原判断顺序：同 request+owner 首先重放；id+expected 同前次则 `applied/unchanged`、返回前次 context，否则 `rejected/unchanged/request_conflict`、也返回前次 context。其他请求在 bundle 未配置或 expected 与当前 context 不同时 `rejected/unchanged/context`；未知模板 `rejected/unchanged/unknown_template`；命中 Profile 的第一个同名模板得到 `switch(index)`。Profile 1–8 项均可选，不做 enabled 子集或 legacy 限制。
- `src/main.cpp::handleV2Activate` 调用一次共享决策；拒绝/重放使用原 `v2Ack`；`switch` 才调用 `v2SwitchActive`。切换失败仍回复 `activation_failed` 与当前 context；成功后更新上次请求指纹、renderCurrent 和 `applied/displayed`。不得在拒绝或重放时切换、重置序列、持久化或绘制。
- render FFI 编译共享 C++，测试同请求成功重放/冲突、旧 context、未配置、未知模板、Profile 1–8 顺序和有效 switch index；拒绝/重放不修改指纹。测试只覆盖决策，明确不声称宿主已实现真实切换副作用。
- 不触主 Bridge/实机，不构建/烧录 ROM，不改 claim 与其他命令。

## 验收

`cargo test -p bridge-render --test v2_state` 与 `git diff --check` 通过；主代理核对 switch 之后的设备端副作用顺序。仍非可运行 Fake ROM。
