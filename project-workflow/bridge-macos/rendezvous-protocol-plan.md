# 会合协议模块整合与 Astra 审核收口

日期：2026-10-04。用户授权：撰写计划并实现；范围为 Bridge 与设备生产代码、自动化回归及候选构建。此轮不部署 Bridge、不烧录、不改真实 Wi-Fi 配置、不提交。待办归属 backlog B6。

## 协议与模块职责

设备决定如何启动 Wi-Fi/HTTP，Bridge 决定正式 PowerPlan 与本轮准入工作。设备保持 BLE 与 CPU awake，异步关联 Wi-Fi 并启动 HTTP；Bridge 经认证 HTTP 核实 MAC、会话及计划，发送 BLE handoff；设备关闭 BLE 后才允许自动 light-sleep。HTTP 启动、HTTP 被 Bridge 验证、正式 light 期限、CPU light-sleep 是四个独立事实。失败不会偷偷续租。

设备新增会合协议模块，持有阶段、硬期限、handoff 条件；main 只提供 BLE/Wi-Fi/HTTP/PM/渲染 adapter。Bridge 新增会合协议模块，持有 BLE→HTTP 验证、准入预算与关闭规则；platform 保留业务 delivery/status adapter。既有 envelope、wire、data/plan/bundle/claim/sync 模块继续复用，不另造传输框架。

## 审核发现与实施顺序

| 项 | 已确认缺口 | 实施与通过条件 |
|---|---|---|
| A1 / P1 | 重放 plan ACK 返回原 grant，Bridge 错把剩余期限重置 | ACK 返回 remainingS(now)，重复不续租；生产 C++ t0=120、t60 replay=60，Bridge mirror 不延长 |
| A2 / P1 | 诊断 complete 可先关临时窗，排队完整读随后失败 | 本轮 HTTP 工作准入快照；Wi-Fi 完整读取先于诊断 complete；正式 light 与临时 sync 分别限时；生产 ble_cycle 成功读完多页，诊断随后完成 |
| A3 / P1 | 普通 plan 已应用但 BLE ACK 丢失，Bridge 中止 HTTP 交接 | 持久化 last_sent；认证 HTTP 状态匹配同一 plan_id 后确认；拒绝错 MAC/会话/序号；无确认不假报成功、不重放写入 |
| A4 / P2 | 普通 plan ID 不追设备高水位，发送前未持久化 | 新 ID 大于本地/设备/已确认高水位；发送前落盘；重启仍重试同一 ID；新 Bridge 接高水位设备可推进 |
| A5 / P2 | connected/ACK grace/授权/HTTP 各自超时，无交接/统一剩余预算 | 显式能力版本与 handoff 命令；RF/HTTP 阶段均有硬上限；可选命令只消耗剩余预算；先放两把投递锁，再有界 close；HTTP 作业预算不越设备计划期限 |
| A6 / P2 | HTTP 200 业务错误、快照校验和传输错误折叠 | 有限且不含输入/凭据的错误分类；暂时故障排队，认证/协议/快照形状等确定错误终止，已发送写入保持 awaiting_confirmation |
| A7 / 审核补充 | recovery_save/waiting_authorization 也触发普通120s窗 | 准入排除 BLE recovery、等待物理授权/输入、终态、冲突、退避/过期；普通读取仍准入120s |
| A8 / cleanup | Wi-Fi UI 三条 close 仍持锁无界 | 统一有界 close，在所有相关路径释放全局/每MAC投递锁后清理；悬挂断链不阻塞下一投递 |

## 协议过渡与失败语义

新设备通过 status 的 rendezvous 能力字段声明显式交接。Bridge 对旧设备沿用既有有界 close/HTTP 探测，不发送未知命令；只有两端候选一起部署才能验收新顺序。设备 HTTP 状态必须在 BLE 未关时已可取；handoff 仅能针对当前会合/接受的计划，不能改变 light deadline。HTTP 不可达时到达 RF 硬期限退出，保留既有正式 plan 或 sync 失败处理；不把启动 server 当成 HTTP 可达证明。

## 验证与交付

按垂直切片先行为回归再实现；复用生产 C++、PlatformService、生产 ble_cycle 与 HTTP/BLE sim seam。覆盖重复 plan、设备高水位+Bridge重启、ACK丢失读回、HTTP慢启动/不可达、handoff超时、诊断+多页完整读取、错误分类、悬挂close与两锁释放。运行受影响 core/app/ble/render/device-sim 测试（sim 串行）与 git diff --check。只构建 Note4 zectrix-note4-b，核对版本/marker/大小/SHA256；缺工具或构建失败如实记录。

实机验收单列：BLE存活时认证HTTP可达、handoff→BLE关闭→PM light；多个真实完整读取作业完成；plan ACK丢失注入/慢Wi-Fi/不可达；断链后再连接；USB/电池功耗与panic dump。未部署前不称生产问题已解决；部署前后ACK统计只说明相关性，close修复回归的因果仍需A/B。

## 执行状态

- [x] 计划与用户追加的 Astra 范围对齐
- [x] A1/A4 序号、持久化与期限确认
- [x] A2/A3/A5 两端协议模块、显式交接与统一预算
- [x] A6/A7/A8 错误、准入与锁清理
- [x] 自动化回归、Note4候选构建、文档与审核
- [ ] 另轮授权后的生产部署/实机验收


## 本轮实施结果

Astra 初审六项全部实施，A7/A8关联缺口一并处理。复审追加的首次HTTP确认重新锚定、sync_open ACK未知中止、同次启动第二轮/旧plan误收尾、取消后持久作业状态、临时窗开窗时间戳亦已修复。最终复审结论：本轮审核发现已收口，无剩余阻断；限源码与软件证据，不代表实机通过。逐项实施、回归与剩余现场见 [rendezvous-protocol-review.md](rendezvous-protocol-review.md)。

软件验收：core/BLE/render197 + app63（含原ignored诊断）+串行device-sim50，共310 passed、0 failed、0 ignored；生产C++生命周期/证明条件专测通过。Note4候选构建成功，0.18.46-note4-b-rv1，ROM 1,723,504 B，SHA256 9BE3591DBA265DC57B595EF43197A0044431668D5740937C00724C6EA01A756D。未部署/未烧录，B6继续开放。
