# 2026-10-15 自动恢复修复前交接快照

# Codex Status 当前交接

更新时间：2026-10-15 10:10（中国标准时间；部署不变，移回办公室自动恢复失败调查）

## 部署与工作区

- Note4 **7C4FADB93408**：15:48用户BOOT就绪后原生OTA一次刷入 **0.18.102-note4 / ota_0**，认证运行SHA256匹配。BIN `artifacts/bridge-macos/note4-0.18.102-offline-log.bin`，1,853,344 B，SHA256 `7e9c1fa6df60cfc59ffcdd316263bf17eb377cbcf0b99354f94ecc5d0755ed9d`；ELF/map同前缀。`.101`保留ota_1及本机归档，`.100/.99`本机归档；分区/NVS不变。[实施/部署](project-workflow/bridge-macos/offline-log-connect-fix-20261014.md)。
- Bridge14:44已部署连接等待修复：原生断开结束pending connect、缺失外设connect明拒、重复失败回调安全。20,826,672 B，SHA256 `50a79f187388bea00c272e611492358e681d7528cfb2c70a0519253a69b9b5d5`，supervisor82215；旧App/data私有存档，152文件哈希核验，health204，扫描恢复。原生7项/Fake旧分支红→绿及完整188通过/3忽略；[联合实施](project-workflow/bridge-macos/offline-log-connect-fix-20261014.md)。部署后多轮自然BLE会合/关闭0ms及HTTP恢复通过。
- 失联诊断逐记录LittleFS保存，24小时内/384KiB上限、认证分页/轮换围栏；.102补齐薄唤醒RTC回填与过期去重。生产存储边界/Fake68/匹配ELF栈通过（本地链1664B；调用者/平台不计，静态RAM120028 B）。实机旧27条完整保持，.102读32条/2848B CRC通过、无凭据401；.101正式Sleep后的4轮timer会合/关闭0ms及跨深睡读回已验，.102自然醒次3恢复Light3948/认证HTTP通过，40条3504B CRC正常，薄唤醒2摘要已回填、原27条完整保持。

## 活跃事项与下一步

**B6/C8 移回办公室失联（10月15日）**：09:45自动BLE已认证status/claim成功，但Wi-Fi准备阶段的700ms外层timeout取消未完成写入，plan被previous_write_unknown拒发，关闭虽1ms成功仍进入蓝牙隔离，之后无扫描。页面“16小时前”来自昨17:58旧HTTP快照，未反映今早BLE联系。未按BOOT/未重启/未改部署；底层写延迟原因未知。详见[调查与修复边界](project-workflow/bridge-macos/return-office-investigation-20261015.md)。

**B6/C8 新断连**：12:20服务发现断开漏回pending connect导致永久隔离，14:28 BOOT为同一隔离延续。Bridge根因修复/.102日志均部署，HTTP正常；.102自然睡醒回填与恢复Light/认证HTTP通过，实施部署完成；本次修复随当前提交归档。首次物理链路超时详情因旧记录覆盖仍未知；真实24h保留及长期RF未验，不据此结项。[调查](project-workflow/bridge-macos/ble-connect-disconnect-20261014.md)。

**C9 / AP01**：最新用户决策为Wi-Fi双屏4位固定码＋旋钮确认，首版仅防误绑定，不要求严格加密/SMP；多对多持久绑定与单owner分离，复用300秒租约/60秒续约，有效owner时不弹新码。0031、无fallback、OTA与常规通信/绘图双进程不变。[具体设计](project-workflow/ap01/screen-binding-design-20261014.md)完成；旋钮/UI/常驻HTTP、绑定存储和Bridge适配/实机待实施。原厂ATT固定拒绝的10场景+6144组合与扫描证据保留为[历史调查](project-workflow/ap01/ble-binding-20261014.md)，不再是当前SMP实施闸门；未构建或刷入AP01新ROM。

历史0031最小XIP候选2未刷：6,804,520 B，SHA256 `61af60e00475f24f4460e60734d7b93a67fb05bb5b756077ccf9d81d4ba7f849`；Windows产物路径 `artifacts/ap01-research/app0031-20261013/candidate2/ap01-1.0.2_0031-app-xip.bin`，本Mac未恢复该候选。它是同步ABI诊断，不能承载长期BLE回调。完整Bridge/App OTA及P0/P2–P5实机未验；[验收](project-workflow/ap01/app-implementation-20261013.md)。

**B6/C8**：Fake后续范围已收缩为[七项生产逻辑覆盖](project-workflow/bridge-macos/fake-rom-logic-coverage.md)，清单已文档化、待实施。Wi-Fi任务取消/到期及身份异常修复已部署，旧ACK回退重推已验；WFS4直接读取、真实变更才拆分，旧缺身份BRS记录不授权。真实最大容量、掉电及其余实机矩阵仍未完成。[Wi-Fi验收](project-workflow/bridge-macos/wifi-read-queue-investigation-20261013.md)。

10月14日已按用户授权直接OTA并恢复Bridge；原BLE隔离根因和再次触发证据见上方部署记录，B6/C8保持开放。

历史详细记录见[合并快照](docs/history/progress-archive-2026-10-14-conflict.md)。待办唯一来源：[backlog](docs/roadmap/backlog.md)；操作：[细则](docs/agent-operations.md)。


## 11:23恢复修复前快照

# Codex Status 当前交接

更新时间：2026-10-15 11:23（中国标准时间）

## 部署与工作区

- Note4 **7C4FADB93408**：仍为 **0.18.102-note4 / ota_0**，本轮未刷ROM。BIN `artifacts/bridge-macos/note4-0.18.102-offline-log.bin`，1,853,344 B，SHA256 `7e9c1fa6df60cfc59ffcdd316263bf17eb377cbcf0b99354f94ecc5d0755ed9d`；匹配ELF/map同前缀，.101保留ota_1。[部署](project-workflow/bridge-macos/offline-log-connect-fix-20261014.md)。
- Bridge **10:21已部署**会合预算修复，exe 79,432,512 B（debug未strip），SHA256 `4329f4be5fa68aa6e16ddd9ae3052ca5a3375eac7923402f31d5621a6e3bad64`；supervisor99172、health204。旧App/data备份 `~/Library/Application Support/dev.codexstatus.bridge/backups/wifi-budget-20261015-102032`，153文件哈希核验。源码未提交。

## 活跃事项与下一步

**B6/C8 再次失联，当前未恢复**：最后HTTP10:33:40，BLE11:09:03；本地3条取消/过期但提交未知作业阻止新activate，自动恢复跳过network_start。11:09又在剩3118ms预算时发生wifi_match原生写失败并隔离，之后无扫描；非700ms重现。未重启/改部署，详见[现场](project-workflow/bridge-macos/bridge-off-recurrence-20261015.md)。

**B6/C8 自动恢复修复已验**：700ms外层取消写入导致隔离；现将10s总预算下沉carrier，与5s命令/RF期限取最小值，准备失败结束本轮，保留未知写围栏。298测试通过/3既有忽略、最终入口2项通过。10:23设备电池自然醒1048，wifi_match/Plan3999/Data617成功；最终认证HTTP核对timer/reset8、.102、HTTP ready及Bridge可达，manual/provisional均0，无BOOT/新隔离。SyncOpen/network_start ACK未知及当轮HTTP proof超时仍如实记录，后续独立HTTP确认恢复，不宣称诊断全部成功。[实施与证据](project-workflow/bridge-macos/return-office-fix-20261015.md)。24h日志/长期RF、最大容量/掉电及其他矩阵仍未验，B6/C8不结项。

**B6/C8 超时审查待修**：另四条外层取消路径已Fake注入确认，HTTP分页/claim/RPC等预算缺口已分级。[审查与证据](project-workflow/bridge-macos/timeout-audit-20261015.md)。本轮未改部署。

**C9/AP01**：已选Wi-Fi双屏4位固定码＋旋钮确认，首版防误绑定，无严格SMP要求；持久绑定与单owner分离，300s租约/60s续约。0031、无fallback、OTA/常规双进程不变。[设计](project-workflow/ap01/screen-binding-design-20261014.md)完成，实施/实机待做；未刷AP01候选。历史ATT及XIP候选证据见[此前快照](docs/history/progress-archive-2026-10-15.md)。

待办唯一来源：[backlog](docs/roadmap/backlog.md)；[操作细则](docs/agent-operations.md)。其余旧验收与部署细节见此前快照。

