# Codex Status 当前交接

更新时间：2026-10-18（23:22 香港时间，0041原生配对/Bundle/Data及四场景像素通过；恢复/发布验收继续、未部署）

## 当前现场

- Note4 **7C4FADB93408**：**0.18.104-note4 / ota_0**，固件未变；Windows默认Bridge已重建部署至`bridge/target/debug/bridge-app.exe`（38,095,360 B，SHA256 `2CDC1AD87048C65CF7BBB2FAD98AC774CE321AC2E39B0F30B73B1A465DBC7CA5`）。原data/凭据保留，主PID32532/watchdog36928，计划任务及8765/8766正常。历史交接按日期存于 `docs/history/`。
- AP01现场版本以旁仓最新实机记录为准；本轮未访问/刷写设备、未读凭据、未启停Bridge，未开公网/局域网服务。
- **2026-10-18 结构变更**：AP01 原厂 ROM、逆向报告与知识、QEMU/REA 证据、Ghidra 项目及构建工具已迁至同父目录 `cuktech-ap01-firmware` 仓库；本仓库的 `artifacts/ap01-research`、`project-workflow/ap01`、`tools/ap01`、`firmware/ap01` 已删除，不再保留副本。

## AP01 对接（本仓库职责）

- **本轮HTTP/存储**：用户确认AP01维护18080与业务18081独立监听/生命周期；模拟中停止业务后维护/配对仍可访问，11检查/8截图。0041自身确认/resource **1.25 MiB**；8模板含字体原始安装384575 B、保留旧内容的更新准入约827039 B（显式8KiB额外占用模型），容量条件下足够。原机实际free未知；5份萌宠GIF为ROM内2153808 B，删除不会直接释放/resource，暂不触发整页移除。[调查/证据](project-workflow/ap01-template-integration/storage-http.md)。

- **用户最新纠正：新业务ROM以0041制作。** 设备当前0031版本与此前0031审计仅为现场/历史依据，不能作为新ROM基线。代码目标已改为`cuktech-ap01-0041`；0041目标已完成能力/身份回归及10检查/8截图模拟复验，旧报告保留历史含义；当前候选哈希已更新。

- **C9本轮**：AP01常在线，Bridge跳过PowerPlan；自动发现、既有四位码/旋钮绑定、完整模板/字体Bundle与Data已接入共享核心及隔离NuttX UI。UI/MCP共用服务，token/request先保存后POST，结果未知查询原事务。缺5h自动居中、RC独立保留；配对不自动发布/开同步。

- 对接实现与设计在本仓库完成：Bridge 侧数据源/显示适配、设备端服务与认证、C9 单份更新与压缩。待办仍归 [backlog C9](docs/roadmap/backlog.md)。
- **模拟验收**：10检查/8截图、6组实际LVGL像素差异0；真实按键确认、双模板新context、持久激活及重启恢复通过。core161/render15/MCP3通过；app串行107通过/3忽略，最终AP01专项4通过；Node及Edge窄宽截图通过，WK脚本因无swiftc未验。[状态/证据](project-workflow/ap01-template-integration/status.md)。
- **Bridge候选，未部署/启动**：`bridge/target-note4-deploy/debug/bridge-app.exe`，38,732,800 B，SHA256 `24bfc2ff77525d9d08663f112508e233bddd58ac705564b685dcd4d303f88138`，含共享重构。
- **0041原生候选12（发布验收中）**：`artifacts/ap01-platform-integration/device/ap01-native-0041-candidate-12.bin`，6964808 B，SHA256 `2e5bd39ff9778c871cc202f55a5023a45e333846774693b98616ed47282f274d`。28对象只读段/完整XIP、宿主ASan/UBSan及54共享回归通过。真实RV32配对确认/绑定落盘、完整双模板Bundle、Data与原厂切页绘制通过；`native-qemu-24`双桶/缺5h/零/满四场景与原厂LVGL帧缓冲差异均0，RC独立保留。显式合成MAC、LVGL输入、fbdev就绪fixture；LCD/SPI仍Logo，不等于物理显示验证。**flashable=false**；原生重启恢复、维护升级适配及总资源待验；源新增重复熵下context恢复保护/安全时间读取，待下一候选。传统OTA按既定人工顺序策略保留，跨通道共享锁不是部署前提。GIF全保留。[记录/证据](project-workflow/ap01-template-integration/native-0041-application.md)。未刷机/启停Bridge/访问实机。
- 逆向依据、ROM 候选与模拟验收查 `cuktech-ap01-firmware` 仓库：报告在 `reverse/reports/ap01/`，ROM 清单在 `roms/CATALOG.md`，QEMU/REA 证据在 `evidence/`。
- 该仓库只把文本纳入 Git（约 33 MiB）；二进制、Evidence 与 Ghidra 环境留在本地归档，按 `checksums/SHA256SUMS.txt` 对账。

## 风险与工作区

- **2026-10-18 首轮用量已验**：11:43重启后首轮BLE Data738/CRC`cf770c57`获ACK，认证回读同MAC/.104/applied738/displayed；周剩余98%非missing，仅5h桶两项真实缺失，源重建CRC一致。285回归通过/3条件忽略，部署日志补充后core159通过。本轮首联系时源已ready，冷源等待分支由回归覆盖；未目视验屏。见[A9证据](project-workflow/bridge-macos/first-contact-usage-20261018.md)。
- QEMU 模型是显式合成（PEC/API 表/MACHW/NOR/Boot2 均为模型假设），不证明物理设备；AP01 首刷与恢复路径未验证，结论以 ap01 仓库报告为准。
- 本仓库不再持有 AP01 原件、密钥或 Flash 证据；需要核对时回到 ap01 仓库本地归档。
- 既有未提交改动保留，不回退。操作遵循[细则](docs/agent-operations.md)。
