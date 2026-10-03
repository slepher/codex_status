# 设备报告数据需求，Bridge 不依赖本地模板（2026-10-03）

用户指出设备页许可与“未发布模板”耦合错误。现场新 Mac 无 Note4 Profile，设备实际已安装模板。设备级同步许可必须独立；运行合同来自设备当前已安装的 CompiledTemplate，而非 Bridge 草稿或模板库。

执行顺序：
1. 复现无本地模板/Profile 时的数据帧及重启许可问题，覆盖现有 UI 保存许可路径。
2. 设备认证 HTTP 状态报告当前 active 的字段索引、路径、类型、missing/local 及合同身份；BLE 沿用已有 context/active 摘要，同一已认证上下文复用设备合同，切换或未知合同在有界 Wi-Fi 会话读取，避免把大字段表塞入 GATT 状态。
3. Bridge 持久化设备报告合同；认证状态变化时核对 MAC、active/context/ABI 和字段界限。数据投递及触发只据该合同，草稿保存不改运行字段需求。数据源匹配保留 Bridge 职责，push/pull 不下发设备。
4. 许可按 DeviceRecord 恢复，无 Profile 也能同步；合同未知显示等待设备报告，不发送空字段包或借本地模板猜测。
5. 回归无模板/Profile、同名本地模板不同字段、按键切换、重启、空/坏合同与 BLE 精简状态；构建并核对部署证据，不自动启用许可或发布 Bundle。

状态：本项修复已完成并部署。共享固件/模拟器状态导出与 Bridge 设备合同缓存已实现；无本地模板/Profile、同名草稿独立、坏合同拒绝、BLE 切换失效与许可恢复回归通过。序号接续也已补入，允许协议规定的跳号。五包回归 225 passed / 1 ignored，后续 app/core 序号修正回归通过，Fake ROM bootstrap 44/44，设备页测试通过。

Note4 `7C4FADB93408` @ `192.168.10.101` 已经正常 Bridge OTA 到 `0.18.41-note4-b-fields1`；job `3f2e8154` 为 succeeded/image_verified。运行镜像前 1,665,024 B SHA256 `9B87601138B68047F0AE71A46486944E2050E8A40651453D613C1E953D8B4947` 与产物一致。新 Mac 无本地 Profile，设备报告 active `codex-status-a` 的 14 个字段，其中 7 个需 Bridge 提供；设备级许可原为 true，本轮未由 Agent 修改。UTC 04:25:50.269198 的 HTTP ACK 为 seq/data_seq 3、applied/displayed。未发布 Bundle。版本与应用哈希见 `PROGRESS.md` 最新节。

字段表随既有认证状态返回并缓存，不新增独立长轮询。真实自然 timer 短窗口、跨平台/长时矩阵以及按 bond BLE 配网/BOOT 离线恢复属于尚未完成的 macOS 首版任务，唯一待办入口仍为 backlog B5/B6。
