# Codex Status 当前交接

更新时间：2026-11-06，Asia/Shanghai。

## 工作区与部署

- Pull合并：本地6f61814（2个独有提交）、远端9a55586（26个独有提交）及本地未提交修复已合并，随本次合并提交登记；未推送、未部署。[验证与备份](project-workflow/pull-merge-20261105/status.md)。以下为最后已记录证据，当前运行未重新核验。
- Windows Bridge：`bridge/target/debug/bridge-app.exe`，39285248 B，SHA256 `f94cc00011708a925028d830ba53cd4be1280130bb21a6d624623381064d1e62`。10/31 03:28部署重复claim/同Plan HTTP抑制修复，PID48712，health204；347文件data备份校验、3设备配置及4凭据文件保留。[修复/部署](project-workflow/power-plan-c/communication-fix-status.md)。
- 书桌屏 `70041DD7A340`：20:14认证`.107-bw`/ota_1，原`.38-bw`/ota_0保留；原非活动.37槽1已替换。ROM `artifacts/wifi-families/desk-0.18.107-connection-fix.bin`，1840176 B，SHA256 `72225732b045651efd991bd4a29393c5df55e871b436c565066fb1ff1f5d6705`。原OTA succeeded/image_verified，20:15同boot HTTP/网络/内容正常，Profile/Bundle/网络目标保留；context按正常重启轮换并已对账。[验收](project-workflow/wifi-families/status.md)
- Note4 `7C4FADB93408`：11-05按用户明确授权USB部署基于.106的`.108-note4`到`ota_0`，串口同MAC/版本/槽已确认；原`.107`保留`ota_1`，NVS/Bundle未写。ROM `artifacts/bridge-macos/note4-0.18.108-bond-reconnect.bin`，1873040B，SHA256 `13019165228c888a25f40e8c9f8566673419a5a57d25fcf4b591a44b06b467fa`，全镜像读回逐字节一致。自然窗口外原绑定重连/认证HTTP通过；Xiaomi_1EBB@192.168.31.90，Data seq5 applied/displayed。[记录](project-workflow/bridge-macos/device-add-20261105-status.md)
- AP01 `34F01557210B`：11-05当前槽1，CUKTECH ROM `0.1.0-cuktech`（基于0041），build `677bc30246d7ae8f7cd73698fe4086d6`；独立App `0.18.108-ap01`已安装，集合2 succeeded/healthy、清理完成。版本诊断与运行列表一致。App版本由本仓 `ports/ap01/build-0041.json` 声明，基座独立发版。[产物哈希与验收](project-workflow/ap01-app-set-v3/status.md)。
- AP01槽0现为0.1.0-cuktech无载入器救援基座，已取代旧0031记录。Mac Bridge最后11/05：81482064 B，SHA256 `ddff5b57a7470de3088de0b35f6e2359db6112e7ca99f4fdfdf58d5d9af90a43`；配对/无IP联网/TUN地址修复已部署。[状态](project-workflow/bridge-macos/device-add-20261105-status.md)

## 活跃事项与边界

- 11-06 17:32 AP01启动优化已部署：槽1 `0.1.2-cuktech`，build `1e311c12e2881d7ea0195ac00743233d`；App `.109-ap01`/集合3不变。稳定集合首次健康即恢复，不重复30秒试运行；claim预检对账新context，首次Data245直接applied/displayed，重启请求至应用16.77秒；自动续租通过，用户确认ota_1不卡顿。ROM 6935928B SHA256 `0c840a788b73d72e2e0e2a8a69c0b1d4c19a6fd6907232420e5fa508903aeacb`，兄弟 `build/claim-ui-stale-20261106/ap01-0.1.2-cuktech-app.bin`；Bridge 81809808B SHA256 `922f03681d994d04877e2b3087484ef64c7bd439cb73c22a919b905175724406`。救援槽0启动验证并保留；用户报告上传时旋钮卡顿，疑写闪存临时阻塞，空闲对照未验。前次UI健康自动恢复修复保留。[验证/证据](project-workflow/bridge-device-protocol-remediation/ap01-sync-investigation-20261106.md)。A9/B6/C8，未提交。

- 11-06 Note4 Bridge off/on误报已修复并部署 `.110-note4`：认证联系独立刷新显示时间，不修改PowerPlan期限。OTA6e7be972 succeeded/image_verified；ROM `artifacts/ap01-protocol-deploy-20261106/note4-0.18.110.bin`，1865664B，SHA256 `60a2430dc4092de6c52f8a681843ac4b3773972de25f71abef1ec80818fdc474`；owner隔离/30分钟联系/真实失联/显示回归及栈检查通过。AP01/Bridge未重启，未提交。[调查与修复](project-workflow/bridge-device-protocol-remediation/ap01-deployment.md)。

- 11-06 14:28配套部署完成：AP01独立App `.109-ap01`/集合3 succeeded、healthy；基础0041 ROM build及槽1不变，未写救援槽。Note4 `.109-note4` OTA succeeded/image_verified，ROM `artifacts/ap01-protocol-deploy-20261106/note4-0.18.109.bin`（1865360B），SHA256 `82bd57deb431fb383a492decd152fc8958b28c25e0e90771df2b4776b1131673`。Mac Bridge新版81787296B、SHA256 `582c711141e12b49d78ed201970f92faf819c124389604f3aee432bf29528e5c` 已托管重启，health204；两设备Data applied/状态displayed。原data和应用已备份，未提交。[部署证据与验收边界](project-workflow/bridge-device-protocol-remediation/ap01-deployment.md)。

- A9/B6/C8：P0提交3315c05、P1提交3cf9061；整改全文非实机验收完成。[完成审计](project-workflow/bridge-device-protocol-remediation/completion-audit.md)。P2数字Wi-Fi opcode、CHUNK重复nonce、history统一sync-v1、统一network_start、boot身份/传输会话分离与CRC关联ID移除已实现未提交；最新完整workspace542通过/0失败/5条件忽略，含模拟器71。最大转义分页缺陷已修，独立management/setup及ASan/UBSan通过；本批最终全套通过。Note4仅候选`artifacts/protocol-p2/note4-p2-sequence-final-0.18.108.bin`，1865360B，SHA256 `8b6c0473316cb2cb7ad8d9ebd24c3a14d7c68a7b135cd9ee97b8732d568a7431`，匹配ELF栈92项通过。[详情](project-workflow/bridge-device-protocol-remediation/p2-status.md)。显式request_seq/WTX2及旧事务迁移已实现，宿主故障/ASan通过，最新全工作区通过；Bundle完成钩子、实际AP01 App宿主ASan/UBSan与3项显式App/Fake集成通过；非实机整改目标已完成；后续用户授权的配套部署见上条，待提交P2，长期RF/功耗仍未验收。
- C6：Windows交叉构建/双S3/AP01 CI入口与证据保留；历史候选不代表当前源码产物。[状态](project-workflow/release-ci/status.md)
- C9：App OTA归兄弟firmware仓库，本仓保留业务Bundle/C2/配对及ABI适配；救援不依赖业务协议。C4长期功耗、C5中文实机等剩余只见[backlog](docs/roadmap/backlog.md)。
- Note4电池图标v8完整Bundle已发布且用户确认；当时源码共享版本0.18.108；现已随11-06部署升至0.18.109。[图标验收](project-workflow/bridge-macos/note4-battery-icon-20261105.md)

## 历史证据

[本地10/31完整卡](docs/history/progress-archive-2026-10-31-local-before-merge.md)、[远端11/05完整卡](docs/history/progress-archive-2026-11-05-remote-before-merge.md)。远端卡内旧Windows/书桌记录由较新本地证据接续；私有日志和备份保留artifacts。
