# 独立 RTC 复核

结论：软件验收通过，候选可供后续硬件验收；没有宣称已部署或24h准度/功耗达标。

- 型号由target决定；0x51无法区分两芯片。GPIO47/48来自厂商硬件表，先保持VBAT锁存，再启动I²C；没有改面板、电源保持或IRQ唤醒配置。
- RTC统一UTC，显示TZ独立；不将内部RC回退值写RTC。读取严格拒绝不可靠时钟，校时中断保持STOP；失败校时屏障防止旧值覆盖新可信时间，成功可信重试修复。范围与世纪明确，不静默裁剪。
- SNTP callback不操作I²C/日志/分配；保存实际时间及单调锚点，主循环补偿处理延迟。生产helper确定性交错测试有pending-only模型失败对照与最终通过，但未覆盖真实lwIP调度/芯片RF。
- 所有新增I²C在setup/loop执行，BLE命令通过既有queue在loop处理；没有放宽token/owner/session、改变GATT表、保存即发布或PowerPlan期限。认证/public状态仅提供缓存RTC诊断；Bridge序列化允许新增字段，Rust/共享模板协议未改。
- 有界缓冲和真实ELF资源证据已记录；平台库部分调用/HTTP JSON/allocator/ISR及整机高水位边界明确。默认唯一Note4构建；1.54只做同源驱动测试，未刷任一设备，未碰运行Bridge/data/密钥/分区。

## 授权部署复核

.56与.57中间版本均精确运行验证，但实机RTC失败；根因分别为Wire32B最小限制和厂商GPIO42 I²C电源前提，已用平台约束回归封住。最终.58运行镜像精确匹配、RTC可信校时ready/valid和后续采样推进确认，GPIO42访问后恢复原值；所有成功/失败路径均测试。宿主不能验证真实电源保持/离线深睡/24h/峰值功耗，继续留backlog。无常开音频电源、无RTC告警/INT改动、无1.54ROM、无提交。

版本命名修订已完成：0.18.59-note4，唯一Note4环境47.57s构建/marker/匹配ELF门禁及RTC/main帧一致检查通过；OTA e082ac3e succeeded/image_verified，下个自然联网窗口独立运行镜像hash14CC1FBA…1FB8DF匹配，RTC ready/valid与WFS3保持正常，latest release已登记。后续只用版本号-note4；完整产物与证据见PROGRESS顶节。本轮未提交。
