# Plan C — status

Updated: 2026-09-22
Status: 设计冻结，未开工；全部代码改动 pending。

## 当前状态

- 设计已确认：窗口 3s、回复/超时后渲染一次、回复带时间校时、DFS min 80MHz、第二阶段 BT modem sleep。
- 前置依赖：未提交的 v2 收敛工作树（0.16.9 固件 + 桥修复）尚未收尾；
  设备现场 0.16.7 / rv2=0，桥为 14:03 旧构建。
- 本计划不改变安全语义（token/claim/owner），不做 light 会话优化。

## 任务状态

| Task | 状态 | 备注 |
|---|---|---|
| task-1 固件窗口/渲染/校时/时钟修复/DFS | pending | 时钟停更修复是正确性问题，优先 |
| task-2 桥时间下发/连续扫描/窗口去重 | pending | 3s 窗口命中率的前提 |
| task-3 遥测与验收 | pending | 依赖 task-1 埋点 |
| task-4 BT modem sleep 实测定值 | pending | 决定 3s 窗口能量收益 |

## 现场记录

- 设备：192.168.3.163 / MAC 70041DD7A340，0.16.7-bw，rv2=0；
- 桥：`bridge/target/debug`（14:03 构建）运行中，PID 17096/32248；
- 0.16.9 固件源码在树、本地构建通过，未固化 ROM、未 OTA；
- 实测结论与日志进 `artifacts/`，不入库。

## 下一步

1. 收尾 v2 收敛工作树（ROM 固化/文档/桥重建）或确认与其合并方式；
2. 实现 task-1 与 task-2；
3. 按 task-3 做 30–60 分钟基线采集，再评估 task-4。
