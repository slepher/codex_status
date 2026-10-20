# Codex Status 当前交接

更新时间：2026-10-20 14:21 香港时间

## 部署与工作区

- AP01 `34F01557210B`/DID2232184842：候选32运行**槽1**，build `6b60914b9f64a7117a53c45b6d93501e`。两次经0031基础ROM槽0启动/核对头尾后单次OTA更新应用槽，**救援槽0保留且实机回切已验**。本轮事务 `artifacts/ap01-app-rollout-20261020/navigation-rollout-task.json`。
- 模板v5/完整Bundle `b097bacb`（CRC4b72d7d7）保留；真实Data seq9/displayed，commit_seq4。用户确认**当前时间已正确**，重置UTC+8转换沿用已确认版本。**切页仍未通过：左旋钮偶发反向动画，页序不可靠**。
- 用户要求先快速结束并提交，停止后续实现/刷写。AP01部署轮沿用既有Bridge；后续本机Mac更新见下，Note4 ROM本轮未刷写。

## Note4 / Mac Bridge（本轮）

- Note4已OTA **`.106-note4`/C2**，原job `f7186b2f` succeeded/image_verified，同MAC运行镜像SHA与候选一致；`.105`原槽保留。Mac **12:09已配套部署Plan激活**，exe SHA256 `7ad7ee3049e26cc0ed209f8240cc8eed945a4bc68c4d3b0666dceeedf70d0b40`，79,089,552 B，160个data文件及installation-id核验备份、health204。Core/App/Render共401通过/5条件忽略，最终协议8通过，88项栈检查通过；[实施/实机验收](project-workflow/bridge-macos/plan-activation-20261020.md)。10:33 B6/C8修复保留，[证据](project-workflow/bridge-macos/review-fixes-20261020.md)。
- **11:23 Mac已部署A9余量快路径**：源通知即时响应+60秒兜底；新工作首次即投递，仅待确认按设备每6轮重试。源消费/各MAC独立，Data先于诊断。Core181/App122及集成26通过，最终定向3项通过。Data871同MAC HTTP ACK applied/displayed并认证读回；源两次刷新间隔61.49秒（含RPC）。exe SHA256 `eb4d56a4f436d0b90bfea8e3a5d90ec07457dfd9ee4efc29cb386b70884d3ec9`，旧app/data167文件已核验备份，health204。[实现/部署/边界](project-workflow/bridge-macos/wifi-usage-fast-path-20261020.md)。

## 待续与边界

- A9软件/Mac部署已修；本轮Data是启动重确认，真实新余量变化通知/屏幕时延、Windows部署未验。B6/C8合盖苏醒、长期RF/目视仍未验，BOOT旧网回退未改。上游跨会话通知未到达原因仍未知，现有60秒采集兜底。[原调查](project-workflow/bridge-macos/wifi-usage-latency-20261020.md)。
- **A9 Plan同轮激活/图标实机已验**：用户已允许定位/蓝牙，14:20:08原Plan5904 BLE ACK明确network_activation=plan；3.304秒后同Plan ready报告，随后同MAC认证HTTP确认Wi-Fi连接、BLE关闭、Plan5904/剩余量递减，14:20:32本轮complete。14:21 Data876 ACK applied/displayed；用户确认Wi-Fi图标已显示。周期同值Sleep软件对照通过，自然余量变化完整时延未验。旧ROM维护兼容保留；[实机证据](artifacts/bridge-macos/plan-activation-physical-proof.json)。

- 用户补充实际顺时针序列：宠物 → 应用（左入）→ 菜单（右入）→ 应用（左入）→ 日期/瓦数/温度主界面（右入）。这是异常观察，非目标顺序；仅记录，详见部署文档末节。

- C9剩余导航问题：最新原厂反编译确认key0x13的动画参数为2、key0x14为1，当前应用退出用相反参数；页面图也非简单连续索引。尚未修改/验证这项发现，不能称导航修复完成。对照原厂事件图和方向映射后再模拟/实机验收。
- 32宿主ASan/UBSan及原生120完整OTA/RTC归一化、121回切、122同NOR恢复通过；123旧版同页退出复现。125原始固定下一页=1断言失败（实际到原厂7且未回应用），独立后置条件复核通过；该模拟未覆盖实机方向问题，不冒充完整硬件验收。物理掉电/Boot2前回退/高水位未保证。
- [部署与详细边界](project-workflow/ap01-template-integration/release-0041-20261020.md)；物理反馈 `artifacts/ap01-app-rollout-20261020/physical-verification-navigation.json`；[待办 C9](docs/roadmap/backlog.md)。

## 精确ROM

- Note4已部署：`artifacts/bridge-macos/note4-0.18.106-plan-activation.bin`，1,865,680 B，SHA256 `36ca4c3aacdc7b27c9c7cebe4cb18df0bee4048461e189f51050b6332b24ceb4`；匹配ELF SHA256 `40cbc05fcaccdd9f144132f2ffb40db61d956119e636df69be7591073e8fe21a`。

- 已部署：`artifacts/ap01-platform-integration/device/ap01-0041-app-navigation-20261020.bin`，6994856 B，SHA256 `7f3789d37ad0ca8b8c1bbd7733729ec69262f7e3248501b40e05e750cf37a33c`；匹配ELF SHA256 `1ad5735dd7a443078106859ee2a0ad07be9666952eae6de6653195c86cff4bc0`。
- 救援槽0：`artifacts/ap01-base0031-20261019/ap01-base0031-v2-rom.bin`，6822344 B，SHA256 `d843aae4e097c31ce06a315c62e1a66f4fb3bf9299db289b6f31562d0ef2231b`；未写槽0，本轮回切巡检CRC/运行/头尾通过，未声称全片实读哈希。
