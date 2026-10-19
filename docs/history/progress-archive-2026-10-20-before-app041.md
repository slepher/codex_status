# Codex Status 当前交接

更新时间：2026-10-20 00:14 香港时间

## 部署与工作区

- Windows/Note4已部署B6/C8协议收敛R1–R5，源码`9e748eb`，切网修复已合并master`93bb386`；Bridge exe SHA256 `33595924e6d9791e4c9fef1405710b3ce58b9e154855dc400d5f37b807e291ed`，冻结备份`artifacts/protocol-convergence-backup-20261019-c2`。15分钟/31采样、7次自然BLE交汇/Data828 displayed通过，16条历史Wi-Fi Job/Profile不变。[部署及验收](project-workflow/bridge-macos/protocol-convergence-rollout.md)。本轮未启停Bridge/操作Note4/发布Bundle，未提交。
- AP01 `34F01557210B`/DID2232184842已部署**0031基础ROM/ap01-base0031/2**，认证build`53574b55382d55a514ef5b7767db66d9`，BootROM映射0x11000/槽0，物理表age12/active0。原厂0031槽1保留，头/尾实读匹配；用户确认显示/GIF与旋钮正常。仅OTA/维护/诊断，无应用/C2。Bridge未启停，未提交。

## 活跃事项、风险与下一步

- **基础ROM实机安装完成**：唯一miIO.ota获ok、一次完整下载后自动切槽，无恢复/重派；按用户纠正不预设固定稳定槽，保留当前可用原厂0031槽1/写非活动槽0。新维护构建/实际槽位、物理表及用户显示/旋钮核验通过，5次STATUS47–94ms、LOGS和无凭据元数据FLASH读取通过。临时HTTPS已关闭。证据artifacts/ap01-base0031-20261019/install-final-verification.json。
- **C9：0031基础ROM两项修复已完成离线验收**，仅OTA/诊断/恢复，无应用/模板/C2。编码常量消除第三恢复标记，构建强制两处且第二处为真尾；OTA与诊断共享非阻塞Flash围栏，忙时STATUS明确RAM状态、FLASH拒绝、LOGS可用。52请求/15组诊断、64次STATUS宿主回归及原生TCP13次并发诊断通过。
- 正式ROM原生TCP完整上传、CRC读回/非活动槽提交及保护区不变通过；0031原厂恢复函数拒绝旧三标记包、接受新包；OTA/恢复各自同NOR槽1启动通过。GIF/启动头/PEC/传统OTA入口保留，正式BIN/ELF重编一致，额外RAM8 B、持久增量0。原厂显示/旋钮已有用户实机确认，独立OTA占用期间诊断/掉电/高水位仍仅模拟或未验；没有为此再刷写保留槽。[结果、复现和边界](project-workflow/ap01-base0031/status.md)。
- 应用ROM工作另列；既有候选27未部署且不符合用户后续救援基线。自用OTA/手动切槽不依赖稳定确认、先实机验证后路再退役旧通道的要求已写AGENTS.md。当前基础ROM不加入应用能力。
- B6/C8长期RF/原生BLE/Flash矩阵按[backlog](docs/roadmap/backlog.md)；部署背景见[前卡](docs/history/progress-archive-2026-10-19-before-ap01-resume.md)。

## 最终镜像

- Note4已运行ota_1 `0.18.105-note4`：`.pio/build/zectrix-note4-b/firmware.bin`，1850016 B，SHA256 `5433769a050a7fe717f52d3d1b5c0c1f76e66a2ce12b5e4cce1880eee7059608`。
- AP01保留槽1原厂0031：`../cuktech-ap01-firmware/roms/0031/ap01-1.0.2_0031.bin`，6804520 B，SHA256 `8a721fc8ef25458d415b2460e4a251e0503a82f7743fdff85b12612190e5c1cb`。
- AP01候选27（未部署）：`artifacts/ap01-platform-integration/device/ap01-0041-codex-status-c2-slot-read-20261019.bin`，6994200 B，SHA256 `49433919ea479e126c72faa15fc6c8212c8f6a82faf90b1592eca2660c878945`；flashable=true/hardware_verified=false。匹配ELF/manifest/放行证据同目录。

- AP01基础ROM（已部署槽0）：`artifacts/ap01-base0031-20261019/ap01-base0031-v2-rom.bin`，6822344 B，SHA256 `d843aae4e097c31ce06a315c62e1a66f4fb3bf9299db289b6f31562d0ef2231b`；匹配ELF `base0031-v2.elf` SHA256 `1de09d9c2b94df5097e2baf31bb85510aee80596ec40da697f4e41f0f28ff43c`，acceptance.json同目录。
