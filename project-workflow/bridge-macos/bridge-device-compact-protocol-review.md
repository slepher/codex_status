# Astra High 协议文档审核

日期：2026-10-08。审核模型：用户指定gpt-6-astra/high，agent `/root/astra_protocol_review`。第一轮为只读文档/定向源码审查；不执行测试、构建或设备操作。下述行号是审核时源码位置，文档随后修订，因此文档问题用章节引用。

## 结论

方向可实施，48 B布局算术没有问题；第一轮不能直接批准按原文编码，须先修正文档冲突及认证/恢复合同。主agent已按审核整合文档和分阶段计划，尚未实施。审查区分目标行为、旧实现证据、未验资源/硬件能力。

## 问题与处置

| 优先级 | 审核问题 | 证据 | 文档整合结果 |
|---|---|---|---|
| P1 | 协议总览/场景正文仍规范性要求保持BLE直到handoff，与新规则冲突 | 协议§7、场景§2/S04–S10/时序、实现对照§3 | 最终场景正文与矩阵改为提前结束；旧顺序仅在实现对照明确标历史；执行详规从信封文档迁至场景§6 |
| P1 | BLE断线session失效后，HTTP报告不能用原session授权 | 原信封§3与执行顺序 | HTTP独立认证握手/new session；原BLE session+request仅报告载荷中的历史关联，跨重启靠持久业务ID |
| P1 | 已有HTTPclient/receiver被误称整体新增；共享token不能证明声明MAC的业务ACK | src/main.cpp:3893、app/main.rs:1097 | 复用presence设施，旧hint不升级为真实结果；先受控通知，再设备认证回读/原事务查询；直接强报告需独立方向认证 |
| P1 | 全组校验/一致提交容易造成Data错误阻断Plan或无谓原子事务 | powerplan-data-execution-plan.md:69、:76 | 采用已有A9 Plan-first/partial；Data错误不回滚合法Plan，大Data走HTTP，结果分项 |
| P1 | 编号缓存容量/退役/多连接顺序及反向回调锁不清 | 原信封§4 | 首版每方向single-flight、最高号+一份当前结果；响应优先；HTTP串行适配，report不等待原delivery锁 |
| P1 | 900秒联系维持不了300秒lease；ownerTouch实际续lease | app/main.rs:216、src/owner_store.cpp:185、:203 | 默认深睡自然到期，需副作用显式claim；Status/report无续期检查；真实权限调用链作为实施验收项 |
| P2 | 当前context不是128位；alias增状态无必要 | src/main.cpp:2118、:4483；bundle_store.h:24为容量 | 16hex完整ID原样16 B ASCII，协商格式，不新增alias或截断 |
| P2 | bleOff清理可取消已接纳网络任务，NimBLE销毁必须独立安全状态 | src/main.cpp:3509、src/ble_bridge.cpp:392 | 收尾前独立保留尝试/结果；主循环安全释放、保留物理窗；生产生命周期与timer回归及实机门禁 |
| P2 | 报告次数/时钟域/重启恢复/临时与持久地址优先级未冻结 | 原信封恢复段 | 提案3次、连接/响应各1000ms、退避250/1000ms，夹紧各自单调deadline；临时地址优先，旧地址重新验证；服务可达可查询原结果 |

## 实施建议

先冻结业务/认证/恢复合同，再复用A9交付Plan+Data；随后升级presence为有关联、有响应的report及提前关闭BLE状态机；48 B编码独立协商与迁移；最后组合故障、资源、匹配ELF栈和授权Note4实机。计划见 [实施计划](bridge-device-compact-protocol-implementation-plan.md)。

## 验证边界

源码证明存在相应分支，不证明新规则已实施。文档尺寸检查不替代C++/Rust黄金向量或固件验收。尚未执行生产回归、测量并行HTTPclient/server峰值、长期NimBLE/RF、电池电量或多平台实机。


用户后续约束：取消审核建议中的“旧设备能力降级/迁移兼容”。Bridge与设备分别更新软件版本，仅维护唯一最终协议；原审核的认证、重放、生命周期和资源问题继续有效。实施计划已删除兼容分支和版本历史，并用固定marker替代信封version。


## 最终复核及收束

Astra/high第二轮确认单协议方向符合用户要求，提出一处信任等级冲突、两项P0边界和一处文字残留；主agent已逐项修正：report先只确认通知接收，设备/业务真值认证回读；request_no小于或等于最高已接纳号都不得在结果退役后重执行；accepted响应释放消息single-flight槽，异步结果由原业务ID继续；移除context映射/编码版本残词。无需扩大缓存或新增兼容框架。最终文档本地引用、固定marker48 B及512 B边界、git diff --check通过；仍无生产代码、测试或设备验收。
