# BLE claim 网络准入审查

2026-10-30，主代理审查（未调用子代理）。

- 共用claim入口覆盖空owner创建与当前owner续约；yield/他人owner仍先拒绝。只读身份/status/owner允许诊断，准入失败后不执行claim、Plan、Data、SyncOpen或network_start。
- 新鲜CoreWLAN/WLAN网络映射与本机活动接口分别判断，不把持久配置当网络事实。Windows核对GetIfEntry operational；macOS核对UP/RUNNING及SystemConfiguration Link.Active，排除点对点接口。共享现有LAN枚举器，无新依赖；AP01发现也不再枚举未活动接口。
- 网络检查使用原BLE命令预算、最多2秒；HTTP self-health禁代理和跳转、要求204，复读宿主网络确认未变化；claim使用已检查地址。reqwest超时/网络检查失败属于发送前失败，记录stage/error_code/action。原已发claim未知结果语义不变。
- 不探测公网，不要求休眠设备先提供HTTP；本机链路/监听就绪不等于设备与Bridge双向HTTP已成功，后续仍由原会合协议验证。已通过后瞬间断网属于运行时故障，本次不改变设备300秒租约策略。
- 仅Bridge宿主瞬态检查，无固件变更、持久格式变更、设备RAM/栈/NVS新增成本。已有未提交的10月27日修复保留，未提交git。
- Windows构建覆盖共享Rust与原生适配器API；macOS新增SystemConfiguration FFI/链接仍须Mac原生编译和休眠/拔网线实测。这里不能把Windows模拟通过视为Mac实机通过。
