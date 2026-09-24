# Fake ROM 与多设备实验时钟状态

## 2026-09-24：计划落地，Task 1 完成

主代理定下一个模拟器进程一台设备、Bridge 按 MAC 维护独立目标时钟、设备侧和 Bridge 侧可分别配置倍率/偏移/漂移的合同；同步与失步都列入验收。阶段、路由、实机隔离与时间迁移约束见 `plan.md`，原提案 `docs/fake-rom-simulator-design.md` 已同步修正。当前仍没有可运行的 Fake ROM。

6-luna high 完成首个编码任务 `task-1-coordinator-time.md`：Coordinator 的四个隐藏系统时间入口改为显式 `now`，生产调用传原真实时间；新增跨 MAC 时间测试。隔离测试 104 项通过、app check 通过、`git diff --check` 通过。没有部署、设备操作或提交。

下一步：主代理先给出 Task 2 的精确按 MAC 运行目标模型和测试合同，再交 6-luna high 实现。现有 `PlatformService`、app、BLE 仍有隐藏时间读取；不能把 Task 1 当成全链 fake 时钟已接通。
