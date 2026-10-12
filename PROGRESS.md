# Codex Status 当前交接

更新时间：2026-10-12 15:59（中国标准时间；Bridge清理修复已部署重启，真实BLE后续会合及HTTP核验通过）

## 部署与工作区

- 最近现场证据：15:59认证 Note4 `7C4FADB93408`（IP192.168.31.90）运行 **0.18.83-note4 / ota_0**，wake908/reset_reason8，Plan3726/Light剩余531s；15:58自然BLE会合完成、HTTP可达。此前wake841实际light_sleep44s证据保留。codex-status-a **v5** 已发布，合同 CRC `eb628a93`。已部署 ROM `artifacts/note4-0.18.83-device-fields-candidate.bin`，SHA256 `32486C0763DB240777758F549BCE386F8884A09547AD3BC89F4423E499664517`。详见[部署记录](project-workflow/bridge-macos/device-fields-deploy-20261011.md)。
- 本机macOS Bridge **15:55已部署并重启**清理/分类修复（包含此前权限/RF改动，源码随本轮提交）：x86_64 release20,850,560 B，App `artifacts/bridge-macos/Codex Status Bridge.app`，exe SHA256 `fc792b2b2016181550b7629b71846c66eba0aa8f69acd0378bbc1f85ef62acf7`。supervisor15210/child15214，health204/MCP43工具、签名/原data及身份保留；Rust269/3条件忽略、Node/WK60及窄宽审阅通过。双权限allowed；15:56首轮Plan写入未知后扫描继续，15:58下一同MAC/wake908会合完整成功、15:59认证HTTP通过。[修复/备份/验收](project-workflow/bridge-macos/ble-connect-cleanup-fix-20261012.md)。固件未刷、未发布模板。


## 本轮里程碑与候选（C5/C6）

- **仅固件未部署 B6/C8分类修复候选**：Note4 **0.18.85-note4**，`artifacts/bridge-macos/timeout-fix-note4-0.18.85.bin`，1,851,104 B，SHA256 `8c4126cfec33c18073c11639c8d5c90ddd5c2071eb73b6119c997cbcba8fecad`；匹配ELF/map已保存。原Bridge候选 `artifacts/bridge-macos/Timeout Reporting Candidate.app`，20,817,136 B，SHA256 `58b4a78d6584f2dd5bfe0e47f56a989941f516502b2a498b371dd0c226cdb06b`，签名验证通过。Rust268/3条件忽略、生产回调/Node/WK60/64栈检查通过；未发/未知/AP未找到/预算独立报告，禁止未知分片复用，补关键日志与初始化顺序。未加长超时；Bridge部分已随15:55清理修复部署，固件仍未部署，源码随本轮提交。[实现/资源/旧宿主基线失败](project-workflow/bridge-macos/timeout-reporting-fix-20261012.md)。

- 中文 CSFN v2 字体随完整 Bundle 发布，模板支持 UTF-8；两 ROM 均不链接旧中文字表。Base64 复用已有库。发布工作流覆盖 Windows x64、macOS arm64/x64、Note4/1.54：v* 标签生成 draft Release，手动运行只产物。[构建安装指南](docs/build-install.md)提供 clone 后安装 Bridge、USB 分区/刷机/配网的 Agent 提示词。
- **未部署 Note4 0.18.84-note4**：`artifacts/release-fonts-20261011/codex-status-0.18.84-note4.bin`，1,838,880 B，SHA256 `7a2e009a512111660a1e51168bcf880075c554be664f7b5682d7c12037847b94`。
- **未部署 1.54 0.18.36-bw**：`artifacts/release-fonts-20261011/codex-status-0.18.36-bw.bin`，1,820,816 B，SHA256 `285fc9051e855de9ff9b05ab48473cc015ef140879ca84909e28c76f5d3b31b6`。
- 两份候选同名 json/USB zip 保存匹配 ELF、分区和哈希。完整回归 309 passed / 3 ignored；最终字体专项 19 项通过；两 ROM 各 64 项 Wi-Fi 栈门禁通过；中文 JSON/CT/序列化恢复像素差 0。最终隔离目标 Note4。资源、ELF 哈希及主 agent 精读结论见[本轮状态](project-workflow/bridge-macos/release-fonts-status.md)与[审查](project-workflow/bridge-macos/release-fonts-review.md)。

- C9/AP01：逆向报告、接入设计、知识记录及分析脚本迁入本仓库，[研究入口](project-workflow/ap01/README.md)。单份更新要求已同步；原始证据保留于 artifacts/ap01-research，第三方克隆仅作只读来源。迁移验证见[记录](project-workflow/ap01/migration.md)，无构建或设备操作。

## 风险与下一步

- B6：13:19建链失败无界disconnect缺口 **15:55已修复部署**，所有裸断开共用原2s预算。生产控制流red→green证明原错误/锁释放/下一次连接；实机失败后继续扫描并于15:58完成后续会合。原进程具体await卡点未抓栈；迟到原生回调/worker失效及总体P2仍未完成。[修复](project-workflow/bridge-macos/ble-connect-cleanup-fix-20261012.md)、[原局域网取证](project-workflow/bridge-macos/disconnect-investigation-20261012.md)。
- B6/C8：Bridge分类已部署；固件分类/关键日志候选待部署后实体故障矩阵，离线两小时留存仍缺。10:10即时查不到不能归为当时已覆盖；11:43手动wake675为6.6s IP/7.7s HTTP及真实light-sleep，旧632仅归档摘要、约一分钟路径未知。[证据与更正](project-workflow/bridge-macos/wake-network-log-investigation-20261012.md)。旧Wi-Fi管理宿主active_protected断言在HEAD对照亦失败，独立合同核对未结。
- C5/C6：中文渲染尚无实机证据，安装/典型/上限动态资源峰值未知；1.54 字体准入仍为 0，最新候选的 USB/OTA/配网闭环未验收。GitHub 矩阵首次实跑、干净 Windows 安装、macOS 两架构安装及签名/公证保留缺口。
- A9/B6/C8：Sending/Unknown 一般自动恢复、跨正式到期同步、自然休眠、真实 RF/EPD/长期功耗矩阵仍未完成；历史 BOOT blink 根因未证。[同步记录](project-workflow/bridge-macos/sync-window-fix-20261011.md)。软件构建与宿主像素测试不代替实机验收。

唯一待办：[backlog](docs/roadmap/backlog.md)；[前轮归档](docs/history/progress-archive-2026-10-11.md)；[本日归档](docs/history/progress-archive-2026-10-12.md)；[操作细则](docs/agent-operations.md)。
