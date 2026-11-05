# Codex Status 当前交接

更新时间：2026-10-31 03:30，Asia/Hong_Kong。

## 当前部署

- Windows Bridge：`bridge/target/debug/bridge-app.exe`，39285248 B，SHA256 `f94cc00011708a925028d830ba53cd4be1280130bb21a6d624623381064d1e62`。10/31 03:28部署重复claim/同Plan HTTP抑制修复，PID48712，health204；347文件data备份校验、3设备配置及4凭据文件保留。[修复/部署](project-workflow/power-plan-c/communication-fix-status.md)。
- Note4 `7C4FADB93408`：20:01认证`.107-note4`/ota_1，原ota_0未作为写入目标。ROM `artifacts/wifi-families/note4-0.18.107-connection-fix.bin`，1858240 B，SHA256 `973c4d6c9377747dc4fafb7e1bd72bf54fd84482a16c5dea26d9d359f8df81d7`；原OTA succeeded/image_verified，20:07同boot HTTP ready/bridge_reachable/error空。
- 书桌屏 `70041DD7A340`：20:14认证`.107-bw`/ota_1，原`.38-bw`/ota_0保留；原非活动.37槽1已替换。ROM `artifacts/wifi-families/desk-0.18.107-connection-fix.bin`，1840176 B，SHA256 `72225732b045651efd991bd4a29393c5df55e871b436c565066fb1ff1f5d6705`。原OTA succeeded/image_verified，20:15同boot HTTP/网络/内容正常，Profile/Bundle/网络目标保留；context按正常重启轮换并已对账。[验收](project-workflow/wifi-families/status.md)
- AP01 `34F01557210B`：上次验`1.0.2_0041`应用1/build`ec0a58f71f65c2830c08c9bfd7c155db`；救援0031/槽0保留。本次未操作。Mac Bridge及其他候选/ROM指纹见[前卡](docs/history/progress-archive-2026-10-27-before-wifi-control-fix.md)。

## 活跃事项与下一步

- 10/31 Note4：快区间Light/Sleep主导电量读数斜率约2.55:1（非实测电流比）。重复claim与同Plan HTTP修复Windows已部署；App132通过/3忽略，原Light期限不续、ROM不变。C4长期功耗仍待验。[调查](project-workflow/power-plan-c/note4-battery-diagnostic-20261031.md)、[修复](project-workflow/power-plan-c/communication-fix-status.md)。

- AP01新候选（未刷机）：`artifacts/release-ci/ap01-menu-ci/http-dist/ap01-base-0031.bin` 6827032 B/SHA256 `8ee193821256e2ff8f0f168ba7f16c65267dff8ca6d1c6c6c7d214bc62cd8fb9`；`artifacts/release-ci/ap01-menu-ci/0041/ap01-codex-status-0041.bin` 7044408 B/SHA256 `83109dd67f41a33abb5fe704cb778f11741572f513fe31a3ea821ab5d089f486`。0031无App、菜单设备码、http扩容保留猫GIF（旧gif-http已停用）；[构建与验收边界](project-workflow/release-ci/status.md)。

- 10/31 C6：act完整windows-cross/真实上传/接收包核验通过，交叉EXE Windows运行通过后恢复原版；双S3候选与AP01 0041双仓构建已验证并合并主CI；兄弟库0031菜单已实现，双AP01主CI接入与本地构建校验通过（69015e7），未刷机。[合集/证据](project-workflow/release-ci/status.md)。GitHub云端/Mac原生仍待验。
- 本轮S3候选目录 `artifacts/release-ci/release-dist/`：`codex-status-0.18.107-note4.bin` 1858240 B，SHA256 `060a269ff81cc1061adc9225b1e29a8e511f0c954156b8f7f25665c3b594b8a4`；`codex-status-0.18.107-bw.bin` 1840176 B，SHA256 `72225732b045651efd991bd4a29393c5df55e871b436c565066fb1ff1f5d6705`。匹配ELF栈/USB哈希通过；Note4重编hash已变，不代表设备已更新。

- 10/31 C8/A9事件Windows已部署：BLE超时保留原操作并有界收尾，真实未知关闭仍隔离；重启恢复ACK与冻结在途Data，同值不再误授600秒。Core/BLE/App347通过、3条件忽略；Windows一轮真实超时收尾/后续会合与同值Sleep0已验；长期多周期/Mac原生未验。新版state新增data_in_flight，部署/回退须配套exe/data备份。[状态/兼容边界](project-workflow/wifi-families/connection-fix-status.md)、[原现场](project-workflow/wifi-families/connection-incident-20261030.md)；剩余只看backlog A9/C8。

- BLE占用准入（B6，Windows已部署）：Bridge网络/映射/活动LAN/HTTP未就绪时不创建或续租owner；342回归通过/5条件忽略。Mac原生构建、部署和休眠恢复实测按用户要求留到Mac上。10/30 Note4因Mac旧租约暂拒Windows，21:25自动恢复；首次target mismatch及Mac实际版本待核对。[状态](project-workflow/wifi-families/ble-claim-network-status.md)

- 周期诊断激活（B6/A9）：已修并部署，HTTP需求不再授予120s正式Light，诊断沿用临时窗口。生产路径红绿/两种Plan路径及诊断完成通过，Rust341通过/5条件忽略；配置/许可/凭据保留。实机自然周期长测未验，真实Data仍按原规则激活600s。[验收](project-workflow/wifi-families/activation-fix-status.md)
- Wi-Fi：共享族工作台已部署；连接v2只匹配真实无线参数，显示名不触发重新配网；控制摘要单帧481 B，去除重复读取，旧分页按实际RF截止与共享deadline保留命令预算。4项最终定向回归、442回归+1真实Fake专项、Note4/BW构建及各90栈检查通过；同MAC运行hash与网络已验，未用实机改名/切网作测试。完整RF/macOS余项归B6。[验收](project-workflow/wifi-families/status.md)
- 版本：Note4/BW共用`src/main.cpp`的`FW_RELEASE_VERSION=0.18.107`，后缀分别-note4/-bw；用户明确不要求同时打包/升级。后续按源码快照共用序列，打包校验marker基号/target/双槽容量并登记commit/dirty/ROM及ELF hash；旧书桌.38缓存明确拒绝冒充.107。源码号、产物、实机分别追踪；用户随后单独授权书桌屏部署，双板现同0.18.107基号。[规则](docs/build-install.md)
- 修复、版本规则和双板部署证据随本次提交登记；`test-firmware-modules.py`有HEAD同样存在的display旧断言失败，记录在专项；不扩大为全套检查通过。A9/B6/C5/C8等剩余只看[唯一待办](docs/roadmap/backlog.md)。
