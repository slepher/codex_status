# P0 审阅

## 2026-11-06 合并后修正

- `post_json` 在写请求得到 session_expired 后执行一次身份刷新；原 `?` 会把刷新失败当作原写错误返回，丢掉原事务查询要求。现在只记录有限错误分类，继续返回原写的 `HttpSessionExpired` / `AckUnknown`，不重发写入、不换业务 ID。unknown 指业务最终状态仍须查询，当前传输帧已被拒绝。
- `activate` 原先为握手和写入各分配完整 timeout；现在从入口计时，只把剩余预算交给 POST，与 data/plan 一致。Bundle 现有逐块预算语义未调整。
- 两个新增集成测试在修正前均失败：刷新失败丢掉 CommandFailure；两个 200 ms 往返在给定 320 ms 预算下错误成功。修正后覆盖刷新成功/失败都不自动重发，以及 activation 总预算耗尽。
- 无协议布局、固件或持久格式变更；新增持久字节、设备 RAM/栈均为 0。不重建固件、不改候选哈希。

## 原阶段审阅

2026-11-02：代码审阅及软件验收完成，未部署。

## 实现决策与设计差异

- HTTP 当前类型是 HttpFrameSession。缓存只保留认证 MAC/boot_id/nonce 摘要；首次写使用 compact status，后续写零预检。会话内 status 走相同串行编号，不生成新 open nonce。上下文查询不带陈旧 context。
- 会话失效只接受匹配 op/request_no 的确定拒绝：清缓存并有界重建；写操作保留原业务 ID，返回原事务查询要求，不自动换 ID 重发。OTA 现有独立读取不改维护授权。
- 大只读同号同摘要允许重执行，新快照不保证与丢失响应逐字节相同；异摘要拒绝。写回执超过缓存预算时返回可缓存 unknown/原事务查询；确定拒绝保留 rejected。缓存同时保留 HTTP 状态码。
- BLE 无可解析头时仅允许 kind=response/result=rejected/op=0/request_no=0 的错误帧；不泄露当前 nonce。Bridge 将无法关联的拒绝视为 unknown，不能冒充当前业务的确定失败。未认证链路仍受 GATT 加密权限限制，不为交付错误信息放宽安全策略。
- 残片错误只清接收缓冲，不需要无消费者的 rxFrameEpoch。增加主循环超时检查以处理不再到来的末片；接收缓冲复用既有递归锁，避免不同锁顺序死锁。
- BLE 生产路径每轮已重建 Manager/Adapter，因此复用这一机制。超时进入 3 秒冷却，连续失败进入 degraded，重建超时报告 adapter_unresponsive；不再通知 watchdog 退出整个进程。业务 unknown 围栏不解除。首读立即执行，其后通知唤醒 + 150/300/500 ms 兜底。
- AP01 App 的业务重放入口同步处理大只读/写回执上限；未修改兄弟仓库维护/OTA 实现。

## 资源边界

- NVS/Flash 持久格式不变：空/单条/典型/上限的新增持久字节均为 0。
- ESP32 四槽指针队列替代两槽，新增槽存储为 2×4=8 B（不含 FreeRTOS 控制块）；设计原文 +32 B 不能作为实际测量。
- 每个请求体上限 8192 B，四槽排队载荷上限 32768 B，不能仅计描述符。另有一个消费中载荷与一个接收/交接载荷，预算上界约 48 KiB，加 String/peer/identity、NimBLE 当前片及分配器开销；空/单/典型实际按收到长度分配。入队前检查槽位及分配失败，队列消费和完整帧转移用 move 降低峰值复制。动态峰值与任务栈高水位仍需实机验证。
- HTTP 仍只缓存 512 B 回执；新增一个状态码。BLE 错误响应使用 48 B 栈头和短 String；匹配 ELF 88 项通过；serviceV2Ble 帧 960 B、局部链 2784 B（预算 1024/4096 B），详见 status。

## 验收注意

- 系统代理曾令 loopback Fake 返回 503；测试命令加 NO_PROXY=127.0.0.1,localhost。
- 无 GUI 主循环的 macOS 单元测试不能调用原生权限读取：仅测试配置使用非原生权限摘要（不授予权限），生产权限路径不变。
- 旧 Fake 测试曾明确期望大 status 返回 retired/错误回放变 200；已改为要求大只读重执行、同会话保持、原错误状态码回放。
- 软件测试不替代真实 GATT Notify、驱动句柄回收、四请求并发内存峰值、RF 和掉电验收。

## 实测后修正

- 设计“所有写回执 <384B”不成立：多 reasons 的 sync_begin 超 512B 被新守卫检出。仅在超限时移除 ACK 的 reasons/diag_generation/from_seq/through_seq；它们仍在冻结批次及状态中，身份/batch_id/长度/哈希/offset 保留，完整 sync-v1 回归通过。
- 固件工具链使用当前配置解析的新平台/Arduino/NimBLE；上游已包含 timer 修复，脚本按精确安全实现识别，2000 轮停机测试通过。该候选不是历史 ROM 的同工具链复现。
- 额外重启用例验证：旧会话发 status 得到新认证 session 时更新缓存，避免持续 response identity mismatch；写操作仍不得接受跨 session 回执。
