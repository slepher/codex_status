# Astra 修订评审（2026-09-26）

## D/E/F 出口复核

软件验收通过：M01–M18 每类有代表性结果或精确边界（`evidence.md`）；24 h 双 MAC 协作运行各 1,440 次 wake，失步输入保持每 MAC 独立；同一快照两次进程重启的 1 h trace 与帧 CRC 逐字节相同。首次回放失败暴露宿主模板时钟泄漏，已修共享引擎并补定向像素测试。完整 Rust 目标测试、默认 Note4 ROM 构建、marker/大小/SHA 核对通过，具体数值见 PROGRESS 顶节。这里的“完整”限定软件 Fake ROM；G 已取消，真实 GATT/面板/掉电边界只留硬件 case 文档。

## D 增量复核

29 项 bootstrap 通过；新增硬退出并非短写返回错误：slot 或 metadata 同步后进程立即终止，重启仍保留旧完整 job。OTA pending 在延迟重启前终止，重启完成切换。动态字段从 Data 写入到帧字节与预览一致。USB/电池输入只验证软件决定，不作为真实 ADC/USB/屏幕证据。仅 Note4 固件重建成功，产物 SHA256 见 PROGRESS；D 的 BLE 生命周期、E/F 尚无出口证据。

## 实施增量复核（2026-09-26，进行中）

当前改动覆盖 D1 的主 HTTP 路径、D2 文件/显示/电源局部切片及 D3 OTA 模型切片。Bundle 保存/激活采用共享 `bsInstall`/`bsSetActive`，而非 Rust 伪造成功；Note4/154g 分别测试，8 项不裁剪。宿主文件写入逐次落盘，测试在短写后强杀进程并由 `bsBegin` 恢复旧 metadata。显示失败保留最后成功帧并使基线失信；按键 8 项循环与 24 h / 1,442 次本地 timer wake 已测试。OTA 校验目录 SHA/大小，上传与 override 有独立来源；目录真相仅在控制面。真实 `v2Rendezvous` 的 timer wake 只有 BLE 机会，因此模拟器 timer wake 后 HTTP 保持失联，按键可触发 300 s provisional。隔离 Bridge 的 M07 提交后断链已复现并对账成功；D/E/F 出口仍未全部完成，设备单侧计时不能证明 BLE/协作加速闭环。

隔离 Bridge 1x 联测补证：双 Fake MAC 的状态隔离、A 冻结发布深睡等待并跨 Bridge 重启交付、B 单次 OTA 上传和非精确版本确认均经真实应用/MCP/Core/v2_client 路径。ACK 延迟故障在设备提交后被消费；后台认证状态可在 ACK 前把任务对账为成功。强停桥时发现 `next_http_delivery` 内存 `sending` 未立即写盘，导致重启可把已发送任务误判为 `waiting`。修复后发送前持久化 Bundle/Data 在途，存储失败返回 `storage_error`，回归测试直接检查磁盘 `sending` 并验证重启为 `unknown`；隔离 Bridge 同样观测到磁盘 `sending`。完整“设备已提交、桥仍为 sending、此刻重启、认证对账且零重发”的应用时序尚未捕获，不列为通过。

## 结论与权威

当前实施范围为 D 剩余/E/F，G 已取消；硬件特有错误只补 case 文档，不实现或实测。用户需求和 v2/明确产品决策优先，设计本身可继续因证据调整。历史计划不再负责定义剩余工作，历史完成事实保留。待办唯一入口为 backlog C2。

本轮复核新增证据：隔离 Bridge 的 M07 在设备提交后磁盘 `sending`，重启恢复 `unknown`，认证状态对账为 `succeeded`；强停仅作用于隔离 Bridge/watchdog。Fake 设备的实验单调时间、倍率、epoch 与 wall 偏移可跨进程恢复，boot uptime 仍独立。控制面 `/sim/frame` 可读最后成功的原始帧，两族静态模板与同源预览逐字节一致。bootstrap 25/25 与核心 M07 定向测试通过；D/E/F 尚未验收完毕。

## 本轮纠正

| 原建议或错误认识 | 最终处理 |
|---|---|
| 将 OTA 新需求当主要修订范围 | 增加全部配置/数据/显示/恢复/电源/BLE/时钟/多设备/故障矩阵与实机阶段 |
| Stage C 尚需从零设计通用 DeviceKernel | 已有独立同源 command/envelope/status；以纵向副作用切片接通，不强制新核心/抽象工厂 |
| 无持久化 | owner 文件已存在；区分局部 owner 与完整 Bundle/RTC/clock/显示恢复 |
| 简单 step 即确定性加速 | 实际仅局部计数；必须补事件调度、双方独立视图与 I/O 消费屏障 |
| 旧 A 时钟工作足以覆盖新 Bridge | OTA/快照/overview 等新增 now_secs 纳入 E，生产 epoch 格式不误解释为 uptime |
| 所有模块做好才有价值 | D3 给出 1x HTTP 最小可用门槛；F 才称完整软件 Fake ROM；硬件边界只记录 case 文档 |
| fake 版本摘要可以证明真实 OTA 成功 | 来源只供 runner；Bridge 只消费真实协议证据，当前仍不能 image_verified |
| legacy ≤3 与未来候选协议都列验收 | 当前产品只测 v2 1–8；候选/硬件边界分别明确 |

## 证据与验证范围

只读核对 `PROGRESS.md` 最新现场、C2、历史专项 plan/status、当前 sleep-aware 设计决策、device-sim main/bootstrap、render build/ffi/lib、共享命令头文件、main 调用点与 owner_store。代码事实落在设计 §2；没有把历史测试通过重报成本轮执行。

文档检查结果：`git diff --check` exit 0；活动四文件与设计/backlog/README 引用存在；历史专项与实现代码未被本次修订编辑。目录按 AGENTS 用 require_escalated 预建成功。首次行号检索误用 Windows 不展开的 `*.md` 路径而失败，改为搜索目录后 exit 0；它不影响文件内容或验证结论。Git 仅提示部分工作树 CRLF 后续转 LF，无空白错误。

## 保留限制

源码可能仍有尚未证实的协议缺陷；本设计给出会暴露缺陷的断言，不假定现 ROM 全符合目标。共享固件切片的抽取大小、OTA pending 断电保留细节、同刻事件顺序应在相应任务用真实路径证据固定，不由宿主自行理想化。它们是实现期设计核实点，不是需要用户重新确认的产品范围。

本轮是文档里程碑，应由主代理在 PROGRESS 写简短交接并链接 C2，不另抄待办清单。当前没有需要用户澄清才能继续的重大产品选择。
