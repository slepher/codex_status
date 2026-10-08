# Bridge—设备协议文档整理（2026-10-08）

范围：整理现行源码中的业务合同、HTTP/BLE 映射、认证、事务与会合流程；确认载体解耦程度。只改文档，不构建、部署或操作设备。

追加范围（用户澄清）：将下层执行场景与具体报文协议分开，并独立记录实现符合度。先编写不依赖函数/字段/固定常量的场景合同，再按源码核对 HTTP ready→换接确认→BLE 释放、失败/超时和物理恢复例外；源码存在不等于运行验收。成果分别为 docs/bridge-device-communication-scenarios.md、docs/bridge-device-protocol.md、docs/bridge-device-communication-implementation.md。

步骤与验收：

1. 读取 PROGRESS 最新现场、文档索引、backlog 与现行设计。
2. 核对固件路由/dispatcher/共享决策函数及 Bridge HTTP/GATT 客户端。
3. 在 docs/bridge-device-protocol.md 分层说明逻辑协议与实际 wire；区分架构合同、已实现能力和源码差异。
4. 补导航与交接，检查引用存在、命令映射及 git diff --check。

状态：源码核对与用户分层澄清已纳入协议文档。Bundle/OTA/模板激活使用 HTTP 是正确分工；HTTP 可用而 BLE 未启用时直接通信，交汇与 handoff 只属必要时的下层通道恢复/收尾。载体能力、绘屏时机和信封差异不作为高层未解耦的依据；失败反馈合同另行核对。本次只修改文档，不以规范说明冒充新增运行验证。

追加成果完成：场景、具体 wire 与源码实现分为独立入口；S01–S14 对照覆盖成功换接、准备失败、丢确认、意外断链、清理卡住、物理窗口例外、到期/重启与 OTA 后恢复。链接与 whitespace/diff check 核查；没有重新运行现有测试，直达全入口及故障组合明确保留验收边界。

历史核查追加：定向阅读旧 BLE 功耗设计、会合 plan/review、Wi-Fi 协议累计记录、旧模板交付与平台设计状态、Bundle v3/字体增量草案；用当前源码、最新 PROGRESS 与 backlog 判定覆盖关系。输出独立历史对账，补充遗漏场景和事务细节；历史原件不重写，待实现方案不当作过期需求，阶段“未部署”不当作今日结论。

历史核查结果：对账文档已完成，补S15–S20与协议回执/快照/迁移条款；当前实现保留兼容分支和未验收边界。链接、whitespace及diff check通过；无测试/构建/设备操作。

Slate 比较范围：只读同父目录 slate 的模块 README、共享 schema、设备同步/HTTP client、错误过滤、缓存暂存以及小智 MQTT/WebSocket 协议边界；按业务、执行、wire 分层比较。先核对实际源码再写 docs/bridge-device-slate-protocol-comparison.md，区分可借鉴原则、已有等价机制及不能照搬的语义；不改 slate，不新增协议/依赖或实施草案。

Slate 比较完成：五项参考与四类不宜照搬边界已记录，源码引用逐项存在；仅文档验证，不声称slate测试/部署验收通过。
