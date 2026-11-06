# P0 任务

- [x] 阅读设计、交接卡、相关 backlog 与操作细则；记录已有修改。
- [x] 建立 plan/task/status/review。
- [x] 核实 HTTP 会话与 Fake 实现并完成 P0-1。
- [x] 实现大响应重放和写 ACK 守卫 P0-2；最终整体验证通过。
- [x] 实现 BLE 拒绝、队列、通知及可恢复驱动 P0-3；宿主故障验证通过，实机待验。
- [x] 完成相关回归、workspace 测试、Note4 构建及匹配 ELF 栈检查；实机资源高水位另列。
- [x] 更新 review、status、PROGRESS/backlog 证据；不提交、不部署。

## 2026-11-06 合并后复核

- [x] 核对 HTTP/BLE P0 生产链路及合并记录。
- [x] 补会话恢复失败仍保留原写结果、activation 共用总预算的回归并修正。
- [x] Core/BLE 253通过/2忽略、串行workspace540通过/5忽略、固件宿主P0通过；更新状态、审阅、交接卡及backlog。
