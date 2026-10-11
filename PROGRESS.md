# Codex Status 当前交接

更新时间：2026-10-11 23:12（香港时间）

## 部署与工作区

- 最近现场证据：Note4 `7C4FADB93408`（登记 IP .177）运行 **0.18.83-note4 / ota_0**，codex-status-a **v5** 已发布，合同 CRC `eb628a93`。已部署 ROM `artifacts/note4-0.18.83-device-fields-candidate.bin`，SHA256 `32486C0763DB240777758F549BCE386F8884A09547AD3BC89F4423E499664517`。详见[部署记录](project-workflow/bridge-macos/device-fields-deploy-20261011.md)。
- 本轮只改源码、文档并做本地构建；未提交、未推 tag、未启动 CI、未访问/刷写设备、未重启 Bridge。原 Bridge/Fake 现场未重新测量，1.54 未部署。

## 本轮里程碑与候选（C5/C6）

- 中文 CSFN v2 字体随完整 Bundle 发布，模板支持 UTF-8；两 ROM 均不链接旧中文字表。Base64 复用已有库。发布工作流覆盖 Windows x64、macOS arm64/x64、Note4/1.54：v* 标签生成 draft Release，手动运行只产物。[构建安装指南](docs/build-install.md)提供 clone 后安装 Bridge、USB 分区/刷机/配网的 Agent 提示词。
- **未部署 Note4 0.18.84-note4**：`artifacts/release-fonts-20261011/codex-status-0.18.84-note4.bin`，1,838,880 B，SHA256 `7a2e009a512111660a1e51168bcf880075c554be664f7b5682d7c12037847b94`。
- **未部署 1.54 0.18.36-bw**：`artifacts/release-fonts-20261011/codex-status-0.18.36-bw.bin`，1,820,816 B，SHA256 `285fc9051e855de9ff9b05ab48473cc015ef140879ca84909e28c76f5d3b31b6`。
- 两份候选同名 json/USB zip 保存匹配 ELF、分区和哈希。完整回归 309 passed / 3 ignored；最终字体专项 19 项通过；两 ROM 各 64 项 Wi-Fi 栈门禁通过；中文 JSON/CT/序列化恢复像素差 0。最终隔离目标 Note4。资源、ELF 哈希及主 agent 精读结论见[本轮状态](project-workflow/bridge-macos/release-fonts-status.md)与[审查](project-workflow/bridge-macos/release-fonts-review.md)。

## 风险与下一步

- C5/C6：中文渲染尚无实机证据，安装/典型/上限动态资源峰值未知；1.54 字体准入仍为 0，最新候选的 USB/OTA/配网闭环未验收。GitHub 矩阵首次实跑、干净 Windows 安装、macOS 两架构安装及签名/公证保留缺口。
- A9/B6/C8：Sending/Unknown 一般自动恢复、跨正式到期同步、自然休眠、真实 RF/EPD/长期功耗矩阵仍未完成；历史 BOOT blink 根因未证。[同步记录](project-workflow/bridge-macos/sync-window-fix-20261011.md)。软件构建与宿主像素测试不代替实机验收。

唯一待办：[backlog](docs/roadmap/backlog.md)；[当日归档](docs/history/progress-archive-2026-10-11.md)；[操作细则](docs/agent-operations.md)。
