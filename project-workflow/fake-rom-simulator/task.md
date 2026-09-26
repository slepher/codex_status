# C2 执行分解（Astra 2026-09-26）

本表是 backlog C2 的执行展开，不是第二份完成状态表。历史 Task 1–13d 原件不改、不重编号。当前用户范围为 D1–F，G 整套实机回归取消；M01–M18 各类别用代表性 case 验证，不穷举错误组合。

| 项 | 顺序与主要影响面 | 可执行验收 / 停止条件 |
|---|---|---|
| D1.1 配置 | `device-sim/main.rs`、render FFI、现有 bundle_command/bundle_store；先接 begin/chunk/commit 及实际安装/恢复 | 用真实 v2_client 建 1 项和 8 项包；错 token/session/context/CRC/ABI/空间均拒绝；提交后状态对应同 job/context，不能从常量拼 ACK |
| D1.2 数据/激活/显示 | D1.1 后，Data/Activate 应用副作用和共享渲染；补状态 fw/target/Profile 与公共注册引导 | 同包 Data → 像素/CRC，Activate → 新 context；重放无重复绘制；未知字体/绑定整包拒绝；未配置路径保留；两族夹具不互冒 target |
| D2.1 保留域 | D1 后，宿主文件/KV/RTC 接点、owner 恢复、实例锁/身份；复用真实存储顺序 | 冷启动/deep/软重启保留清单逐项核对；进程 kill/restart；每个 A/B/metadata 切点短写与断电分别测；损坏不静默清空 |
| D2.2 生命周期 | D2.1 后，main 电源/会合编排小切片及 scheduler；按键/USB/电量/显示完成/radio | 共享代码决定最近截止与退出；配置后能睡/醒，控制面可达而设备面失联；BOOT 与 timer 不混用 provisional；读取/claim 不延期；8 项循环 |
| D3.1 有限 OTA | D2 后，流式上传、2–3 项目录、active/pending、受保护 override、trace | 正常完整上传才 pending/重启切换；错/截断不切换；接受后丢 ACK、断电、同版本与 override；来源可辨，Bridge 不能读控制面作弊 |
| D3.2 近期交付 | D3.1 后，以隔离应用服务/Bridge 数据目录运行 1x HTTP 联测；Static 源、两 fake | M01–M15/M18 的 HTTP 子集；冻结任务跨 Bridge 重启、100 次另一 MAC 联系 0 误传、Unknown 先对账；说明 BLE 前半链未覆盖 |
| E1.1 时钟审计/接线 | D3 后，app runtime/platform/main、core coordinator/service、OTA、快照、BLE 调用时间 | 每个直接 now/SystemTime/Instant/sleep 标注目的和时域；每 MAC 可独立配置；真实目标不受 fake 加速；新增 sleep-aware 路径无遗漏 |
| E1.2 重启/校时 | E1.1 后，实验锚点、boot/epoch 分离、持久字段读写 | 校时与变倍率不续单调 lease；重启不把 epoch 读成 uptime；旧格式测试；暴露的生产缺陷另修共享路径，不通过适配掩盖 |
| E2.1 fake BLE | E1 后，真实 V2Connection 业务接点与 sim 接收队列/分片 | 扫描/身份/连接/命令/关闭、认证/nonce、fragment/队列限额；真实不支持操作仍 http_required；不访问 Windows 蓝牙 |
| E2.2 step/max | E2.1 后，复用 Bridge 单轮工作、runner、scheduler、在途登记与完成事件 | idle/in_flight 与响应消费屏障；注入丢 ACK 后推进逻辑超时无死锁；真实 I/O watchdog 分类；相同时间顺序与 ROM 一致 |
| F 联合验收 | E2 后，场景使用 M01–M18 的代表性行为、两族/双 MAC、sync/失步与恢复 | 每类有证据或明确限制；24 h 虚拟时间和 1000 wake；同输入 trace/帧可重放；D/E/F 无法覆盖的硬件特有错误只补针对性 case 文档，不实现或实测 |

## 每项验证纪律

先写一条纵向行为的预期与当前 characterization，再改共享入口与适配。复用 `render/tests/`、`device-sim/tests/bootstrap.rs` 和现有 core/app 测试，不另建通用测试平台。按影响面执行 `cargo test -p bridge-render -p device-sim`、core/app 定向测试；涉及固件共享源码时按授权目标编译。每项 `git diff --check`。

测试目录/target-dir 首次生成按 AGENTS 提权预建；不删生产运行数据、不启动/停止现场 Bridge、不从生产登记导入 MAC/token。联测脚本需显式列明隔离目录、loopback 目标、Static 源；使用可等待/可终止的有界测试进程，独立后台服务遵守仓库启动规则。

阶段失败保留最小重放输入、预期/实际、副作用数、trace 与对应代码；同源逻辑缺陷必须修两端共用路径。已知失败不能整体 skip 后通过门槛，下一阶段依赖的缺口先解决。每个真实实施里程碑更新 PROGRESS 与 C2，其他专项待办不并入本表。
