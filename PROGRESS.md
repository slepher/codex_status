# Codex Status 当前交接

更新时间：2026-10-14 15:54（中国标准时间；Bridge连接修复/.102离线日志已部署并完成自然睡醒核验）

## 部署与工作区

- Note4 **7C4FADB93408**：15:48用户BOOT就绪后原生OTA一次刷入 **0.18.102-note4 / ota_0**，认证运行SHA256匹配。BIN `artifacts/bridge-macos/note4-0.18.102-offline-log.bin`，1,853,344 B，SHA256 `7e9c1fa6df60cfc59ffcdd316263bf17eb377cbcf0b99354f94ecc5d0755ed9d`；ELF/map同前缀。`.101`保留ota_1及本机归档，`.100/.99`本机归档；分区/NVS不变。[实施/部署](project-workflow/bridge-macos/offline-log-connect-fix-20261014.md)。
- Bridge14:44已部署连接等待修复：原生断开结束pending connect、缺失外设connect明拒、重复失败回调安全。20,826,672 B，SHA256 `50a79f187388bea00c272e611492358e681d7528cfb2c70a0519253a69b9b5d5`，supervisor82215；旧App/data私有存档，152文件哈希核验，health204，扫描恢复。原生7项/Fake旧分支红→绿及完整188通过/3忽略；[联合实施](project-workflow/bridge-macos/offline-log-connect-fix-20261014.md)。部署后多轮自然BLE会合/关闭0ms及HTTP恢复通过。
- 失联诊断逐记录LittleFS保存，24小时内/384KiB上限、认证分页/轮换围栏；.102补齐薄唤醒RTC回填与过期去重。生产存储边界/Fake68/匹配ELF栈通过（本地链1664B；调用者/平台不计，静态RAM120028 B）。实机旧27条完整保持，.102读32条/2848B CRC通过、无凭据401；.101正式Sleep后的4轮timer会合/关闭0ms及跨深睡读回已验，.102自然醒次3恢复Light3948/认证HTTP通过，40条3504B CRC正常，薄唤醒2摘要已回填、原27条完整保持。

## 活跃事项与下一步

**B6/C8 新断连**：12:20服务发现断开漏回pending connect导致永久隔离，14:28 BOOT为同一隔离延续。Bridge根因修复/.102日志均部署，HTTP正常；.102自然睡醒回填与恢复Light/认证HTTP通过，实施部署完成；本次修复随当前提交归档。首次物理链路超时详情因旧记录覆盖仍未知；真实24h保留及长期RF未验，不据此结项。[调查](project-workflow/bridge-macos/ble-connect-disconnect-20261014.md)。

**C9 / AP01**：AP01 **0031自定义App**最小XIP候选2已构建，未刷机：`artifacts/ap01-research/app0031-20261013/candidate2/ap01-1.0.2_0031-app-xip.bin`，6,804,520 B，SHA256 `61af60e00475f24f4460e60734d7b93a67fb05bb5b756077ccf9d81d4ba7f849`。ABI1/data+BSS/同步生命周期/原厂返回路径完成，宿主ASAN/UBSAN、RV32模拟12/12及独立审阅通过；[验收](project-workflow/ap01/app-implementation-20261013.md)含map/ELF/指纹。heap31 B，304 B模拟栈不含原厂logger（该链至少864 B+嵌套），日志同机读取/任务余栈仍须首刷前核实。完整Bridge/认证/文件App OTA/持久安全启动未实现，P0/P2–P5实机未执行；[恢复评估](project-workflow/ap01/failure-recovery-20261013.md)确认底座失败不开壳恢复未知，Type-C/双槽不能作保障。用户报告仍0031，0041仅对照；研究/源码随本次提交归档，ROM/凭据/私有证据仍在Git忽略目录。待办C9。

**B6/C8**：Fake后续范围已收缩为[七项生产逻辑覆盖](project-workflow/bridge-macos/fake-rom-logic-coverage.md)，清单已文档化、待实施。Wi-Fi任务取消/到期及身份异常修复已部署，旧ACK回退重推已验；WFS4直接读取、真实变更才拆分，旧缺身份BRS记录不授权。真实最大容量、掉电及其余实机矩阵仍未完成。[Wi-Fi验收](project-workflow/bridge-macos/wifi-read-queue-investigation-20261013.md)。

10月14日已按用户授权直接OTA并恢复Bridge；原BLE隔离根因和再次触发证据见上方部署记录，B6/C8保持开放。

待办唯一来源：[backlog](docs/roadmap/backlog.md)；操作：[细则](docs/agent-operations.md)。
