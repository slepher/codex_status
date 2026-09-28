# 状态与交接

## 2026-09-28：sync-v1实施合同定稿（仅文档）

Status：文档完成，功能待实现。四份文件为design.md（协议）、plan.md（P0–P7顺序）、task.md（S01–S14可执行断言）、本文件。待办唯一来源仍为docs/roadmap/backlog.md C8。

Result：已固定Bridge→设备HTTP；第15轮due通过BLE sync_open开网，未认证不自主开网；light入/离场完整同步成功清零；正常批次不设总时长截止，90s无进展/三次失败进入明确异常；统一4096B RTC流，begin时生成有界LittleFS冻结副本，Bridge持久页ACK和最终complete收据分开；OTA/Bundle确认分级，镜像采用当前运行分区前N字节SHA256明确合同；能力/owner/恢复/缓存语义已定。

Changes：替换原讨论稿中的“评审时再定”事项，给出精确端点/信封/错误、容量/存储顺序、序号/游标/幂等与异常；P1起即共用固件/宿主逻辑，Fake ROM是主要回归；模板页仅声明归属，不实施布局迁移。

Evidence（只读核查，未重跑历史测试）：

- AGENTS.md、PROGRESS.md顶节及Fake ROM D/E/F节、backlog C8、总设计v2 §7/§11。
- src/main.cpp的认证/owner/v2Command、serviceV2Ble、rendezvous、OTA/Bundle收尾；src/v2_command_envelope.cpp、v2_plan_command.cpp、v2_state.h、v2_status_snapshot.*、dev_log.*。
- core平台model/service与app device_runtime/platform/wake_history，设备UI refreshDevice。
- device-sim/src/main.rs、tests/bootstrap.rs、ble/src/lib.rs、tools/fake-rom-runner.mjs；docs/fake-rom-simulator-design.md及归档D/E/F evidence/status/review。
- 当前本地Note4旧产物map：RTC SLOW使用0xF90/0x1E00；当前1.54 map路径不存在。此信息仅用于设计可行性线索，未构建、未证明新布局。
- 仓库git diff --check退出0，仅现有src/ble_bridge.cpp的CRLF提示。四文档当前未跟踪，另用git diff --no-index --check对空文件逐份核查，无空白错误输出（退出1表示存在新增内容）；该检查不是协议测试。

Caveats：

- Clarification required: none。已作合理工程定值，未留下需要用户才能选择的产品语义。
- 资源门槛仍需实现证据：两目标新map与512B余量、40KiB文件预留、实际IDF运行分区字节域、Flash写边界恢复；不通过不得缩容量/降确认或假成功。
- Fake ROM历史D/E/F不证明sync-v1已完成；当前HTTP门控、runner失败模型、诊断generation与新协议均需补接。
- RTC未冻结记录不能跨冷断电保全；gap必须可见。Flash冻结副本不是第二套日志流，正常事件不逐条写Flash。
- 运行分区prefix证明定义明确，不是硬件远程认证；加密读取字节域不可对应时禁止启用该算法。
- 总设计§7仍有旧“截止必关网”描述。实施P6须同步修订，当前仅编辑被授权四文件。
- 未改源码、未启动/停止Bridge、未运行测试二进制、未访问设备、未构建/OTA/提交/再委派。现有RF1与其它未提交工作保留。
