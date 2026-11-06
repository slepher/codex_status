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

- A9/B6/C8：11-06补P0会话恢复错误归属与activation总预算修正，源码未提交/未部署；[修正与软件验收](project-workflow/bridge-device-protocol-remediation/status.md)。组合本地网络准入、Wi-Fi v2匹配、ACK/在途Data恢复与远端P0/绑定重连/首次联网修复；合并源码不代表组合已部署。Mac原生、RF、多周期功耗验收仍开放；state回退须配套exe/data备份。[本地修复](project-workflow/wifi-families/connection-fix-status.md)、[P0](project-workflow/bridge-device-protocol-remediation/status.md)。
- C6：Windows交叉构建/双S3/AP01 CI入口与证据保留；历史候选不代表当前源码产物。[状态](project-workflow/release-ci/status.md)
- C9：App OTA归兄弟firmware仓库，本仓保留业务Bundle/C2/配对及ABI适配；救援不依赖业务协议。C4长期功耗、C5中文实机等剩余只见[backlog](docs/roadmap/backlog.md)。
- Note4电池图标v8完整Bundle已发布且用户确认；源码共享版本0.18.108（-note4/-bw），本次未生成ROM、未重启Bridge、未访问设备。[图标验收](project-workflow/bridge-macos/note4-battery-icon-20261105.md)

## 历史证据

[本地10/31完整卡](docs/history/progress-archive-2026-10-31-local-before-merge.md)、[远端11/05完整卡](docs/history/progress-archive-2026-11-05-remote-before-merge.md)。远端卡内旧Windows/书桌记录由较新本地证据接续；私有日志和备份保留artifacts。
