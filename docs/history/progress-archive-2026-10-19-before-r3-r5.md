# Codex Status 当前交接

更新时间：2026-10-19 19:05 香港时间

## 部署与工作区

- Windows 19:55已部署切网修复，exe38,739,968 B，SHA256 `66e8e7f2ea64f9569a24ecbe49b6c57c0064d1f6aadc7ff7bd7ccd0a06b937e6`。264文件冻结备份 `artifacts/wifi-switch-backup-20261019-195530`；19:57自然会合恢复wd21-la/认证HTTP，原ID017a completed/revision342，14条旧Job不变，用户确认OFF图标恢复；固件仍.104-note4。合并 `codex/fix-wifi-switch`，详情及回滚见[切网验收](project-workflow/wifi-switch-fix/status.md)。

- macOS托管Bridge 15:26已部署AP01族菜单；exe80,984,128 B，SHA256 `cc81bf028101309f00b4b472417b737c4b317378fd7d911f8adc766c99e535d6`，health204。停机冻结App/data备份 `~/Library/Application Support/dev.codexstatus.bridge/backups/cross-family-sync-20261019-152556`；159文件哈希一致、启动前data未变、签名及部署包一致。
- Note4 `7C4FADB93408` v7此前已发布：job `cac235bb` succeeded，认证source_crc `6d54e167`、context `9c3bd636edcd32e4`、commit/data802 displayed；固件`.104-note4/ota_0`。本轮保留其Profile/发布jobs与合同，不重复发布、不刷机。实物屏幕未目视。
- **本轮已同步Bridge**：1.54 `quad` v15/CRC `abd79ca9`；AP01 `codex-status-a` v3/CRC `9bc56e9a`。重启后源码与项目逐字段一致，族配置「1.54 Codex额度」「AP01 Codex额度」及原Note4配置均在；AP01/1.54未登记或发布实机，Windows实例未同步。

## 本轮结果与边界

- quad修复monthly余额/重置，保留四象限、缺5h静态100；PLAN/label/reset/RC固定行且缺RC/用户名仍隐藏。AP01恢复历史仅WEEK并叠加MONTH，5h双栏优先。新增AP01菜单；此前Note4旧空缓存问题已修复且用户确认窗口可见。
- core9/AP01 3/render12、Node预览/UI、三目标Python canonical通过；WK74窄宽截图已审（合成IPC，不冒充真实窗口）。AP01中文CSFN未导入，本轮只同步模板/配置，不宣称中文预览或实机验收。**本轮修改随本次提交保存**。[状态/证据](project-workflow/free-month-metering/status.md)。

## 其他活跃事项

- **B6/C8协议收敛R1/R2及R3a已改未部署**：自动发现/连接拆分，忙设备连接前跳过；历史事务原ID有限查询/ACK，HTTP/BLE共用收尾。主回归345通过/5条件忽略，Sol Fake专项4通过，扩展Fake分页夹具错误已修复，Note4关联15通过。本地R3a拆分正常接受/未知结果、保留原ID有限查询与围栏，临时凭据移入AppCtx；App115/Core171通过，3条件忽略，关键会合复验通过。R3其余投影与R4/R5未完成。wire须覆盖Note4/1.54/Fake/AP01应用，无旧版兼容；用户允许升级失配时受鉴权裸OTA。[计划/证据](project-workflow/bridge-macos/protocol-convergence-status.md)。AP01源码在ports/ap01可修改，代码随整ROM发布的独立更新缺口记C9。
- B6/C8事务终点修复已部署；历史未知事务保留，长期RF矩阵仍开放。[依据](project-workflow/bridge-macos/wifi-transaction-finality-20261019.md)。
- C9 AP01本地进度：0031过渡接收器 `ap01-http/tail3-minimal` 已认证运行槽2（MAC34F01557210B，build ef8319734dcaa8e63d5151bfd512a69a）。最后记录为单次安装完整0041候选25到槽1，当前结果未知，禁止重派；物理首启/断电/LCD/资源仍未验。候选24模拟已交付，不再沿用远端候选22/23状态。[安装记录](project-workflow/ap01-template-integration/flash-0041-20261019.md)、[模拟交付](project-workflow/ap01-template-integration/release-0041-boot-safety.md)。不关闭C9。
- [前卡全文/部署历史](docs/history/progress-archive-2026-10-19-before-cross-family-sync.md)；待办唯一来源[backlog](docs/roadmap/backlog.md)。

本地AP01未提交工作已与远端ea64ff8合并；原始本地文件保存在Git stash `codex-before-pull-20261019`。用户要求先提交本地再pull；冲突已解决，本地AP01工作随本次提交保存，本轮B6/C8 R3a修改留在工作区，未提交、未部署；后续为R3其余职责与R4/R5。
