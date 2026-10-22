# Codex Status 当前交接

更新时间：2026-10-22 香港时间

## AP01共享宿主离线重构（2026-10-20）

- 两仓库重构及第二应用/窗口热重建离线验收完成：八模板+独立两页共存，跨应用发布保持焦点锁，两次原厂九页删除/重建（含动画x=-279），复用Runtime/原三条payload线程/UI定时器，旧帧/revision拒绝，维护/发现不断。修正完整对象删除绑定、原厂NULL定时器和旧窗口订阅；最终原生run10/普通run03、核心ASan/UBSan及六场景像素通过。
- 普通候选03（未部署）：`artifacts/ap01-screen-lifecycle-production-20261020/ap01-screen-host-runtime-reuse-03.bin`，7,003,496 B，SHA256 `8b56c53bc86f255196d2a39cd43faa7b373e82c6b86ae43449bf2ac917e4cde4`；ELF `015de1182c2a99b9a0dd40df276934aff13a693fbae066e67318f31153615ad1`。Runtime2240B，不含探针。屏幕宿主重构未部署，后续诊断版以34精确源码另行构建并部署槽1，Bridge未部署；C9精确候选OTA/重启、物理LCD/SPI/高水位等仍开放。[专项](project-workflow/ap01-screen-host/status.md)。

## 部署与工作区

- **2026-10-22 AP01源码边界已调整并纳入本轮提交，未部署**：OTA/切槽/维护鉴权与HTTP策略、系统菜单及切网执行归固件仓库；本仓库保留Bridge通信App/模板和平台回调连接；配套固件提交`41541a2`。宿主App/维护及无App独立OTA/回执/切槽、切网核心ASan、RV32审计/链接、原生App/维护烟测通过；Wi-Fi菜单后续已完成离线验收（见下），未新增实机验收。用户已确认保留切网暂停App、App绑定初始化失败仍启维护、维护启动失败阻止App三项行为；四场景像素一致，本次仅更新核查记录、不刷机。[交接/资源/证据](project-workflow/ap01-firmware-boundary/status.md)。

- **应用OTA/切槽八位码06已部署槽1并实机通过**：build`5757fdd677800d1412d8795c1cb2dbe0`，经原0031救援0单次上传应用1；菜单码/原Bridge密钥维护兼容，缺失/错码401、原业务C2正常、Wi-Fi列表未变。八位码实际切槽/重启→救援0→独立入口回06通过，无二次上传；设备码跨重启保持，用户确认界面正常。救援0未写，完整非活动6822344B（含PEC）SHA与保留档一致。零绑定完整OTA为原生模拟证据，物理保留既有绑定；掉电/栈高水位未验。[实机证据](artifacts/ap01-menu-ota06-rollout-20261022/physical-validation.json)。实现及验证脚本已纳入本轮提交；配套固件仓库`57f18aa`。
- 设备码菜单/手动鉴权05已由06替换；原物理菜单、生成后旧码失效及字体/选择行为验收保留。[05归档](artifacts/ap01-device-code-auth-20261021/physical-validation.json)。
- AP01 `34F01557210B`/DID2232184842：**配网列表后端验收归档**，原build`f52b12`已由上述05替换，后端保留。该次救援0前后6822344B（含PEC）SHA均匹配原版；列表保存/uniq、软重启revision1/密文保持通过，清理至revision2；11份原网络/身份文件不变，用户确认原厂界面/旋钮正常。**该次物理验收仅列表后端；后续原生菜单/切网已完成离线验证，实机切网仍未验。** [专项](project-workflow/ap01-wifi-profiles/status.md)。历史误刷及0031恢复记录保留。
- **导航问题已修复并实机通过**：原厂Pet7→菜单6及菜单→主界面被页号递减误判为环回、重复插入应用；现仅在进入Pet的边/反向边插入全部模板。退出动画key0x13→2、0x14→1。用户本轮明确回复“页序和动画均正常”。
- AP01保留单页v9布局/字体，验证后当前Bundle **`6a8d702a`/CRC4831c60d**，commit_seq13/source_crc3ce26768；Data23/displayed/current_template_drawn。原厂页发布不抢焦点及模板页发布自动恢复已实机通过，用户确认“自动返回，额度正常”；所有设备Profile不变。[部署与验收](project-workflow/ap01-template-integration/bundle-page-restore.md)。
- 发现默认Bridge未监听，已按指定计划任务脚本启动既有EXE；force=false恢复既有占用并投递快照。该次启动未重建Bridge或刷Note4。
- **AP01 34验收边界**：原生136页面恢复与137–139 OTA/同NOR/无稳定记录回切、12组像素零差异及匹配34宿主ASan/UBSan通过；单页实机与协议反馈已验，多页仍为原生/8页宿主证据。Bridge未部署，原生WK受WSL缺swiftc限制。[证据](artifacts/ap01-app-rollout-20261020/physical-verification-page-restore34.json)。

## Note4 / Mac Bridge（2026-10-20）

- Note4已OTA **`.106-note4`/C2**，原job `f7186b2f` succeeded/image_verified，同MAC运行镜像SHA与候选一致；`.105`原槽保留。Mac **12:09已配套部署Plan激活**，exe SHA256 `7ad7ee3049e26cc0ed209f8240cc8eed945a4bc68c4d3b0666dceeedf70d0b40`，79,089,552 B，160个data文件及installation-id核验备份、health204。Core/App/Render共401通过/5条件忽略，最终协议8通过，88项栈检查通过；[实施/实机验收](project-workflow/bridge-macos/plan-activation-20261020.md)。10:33 B6/C8修复保留，[证据](project-workflow/bridge-macos/review-fixes-20261020.md)。
- **11:23 Mac已部署A9余量快路径**：源通知即时响应+60秒兜底；新工作首次即投递，仅待确认按设备每6轮重试。源消费/各MAC独立，Data先于诊断。Core181/App122及集成26通过，最终定向3项通过。Data871同MAC HTTP ACK applied/displayed并认证读回；源两次刷新间隔61.49秒（含RPC）。exe SHA256 `eb4d56a4f436d0b90bfea8e3a5d90ec07457dfd9ee4efc29cb386b70884d3ec9`，旧app/data167文件已核验备份，health204。[实现/部署/边界](project-workflow/bridge-macos/wifi-usage-fast-path-20261020.md)。

## 下一步与验收边界

- **AP01 Wi-Fi菜单/切网离线完成（C9，未部署）**：REA合约直接落实固件模块；修复原厂布局枚举推断和退出设置定时器地址，统一SSID字体/长名裁剪。原生04导航/返回/重建、合成RF切网成功/重复点击/失败回退，原生07同NOR重启保持列表/当前勾号/设备码/身份；生产适配层ASan故障/掉电恢复、App/维护回归及精确ROM重建通过。候选03为7043512B，SHA256`da2143188172abce693f6ca5b6718e97c0628000772071ddddf65ccd8e472731`；ELF`29c53fb041f9a3ce07e64a14d95496a9ae1e2b1bb6bfffce2d53435300dcf3d6`。文件`artifacts/ap01-wifi-completion-20261022/ap01-wifi-complete-candidate03.bin`；[证据/资源/物理边界](project-workflow/ap01-wifi-profiles/status.md)。未访问设备或Bridge，应用06/救援0不变；RF/LCD/物理掉电与调用链高水位仍开放。

- A9软件/Mac部署已修；本轮Data是启动重确认，真实新余量变化通知/屏幕时延、Windows部署未验。B6/C8合盖苏醒、长期RF/目视仍未验，BOOT旧网回退未改。上游跨会话通知未到达原因仍未知，现有60秒采集兜底。[原调查](project-workflow/bridge-macos/wifi-usage-latency-20261020.md)。
- **A9 Plan同轮激活/图标实机已验**：用户已允许定位/蓝牙，14:20:08原Plan5904 BLE ACK明确network_activation=plan；3.304秒后同Plan ready报告，随后同MAC认证HTTP确认Wi-Fi连接、BLE关闭、Plan5904/剩余量递减，14:20:32本轮complete。14:21 Data876 ACK applied/displayed；用户确认Wi-Fi图标已显示。周期同值Sleep软件对照通过，自然余量变化完整时延未验。旧ROM维护兼容保留；[实机证据](artifacts/bridge-macos/plan-activation-physical-proof.json)。

- **AP01字体核查已确认恢复遗漏**：原厂ROM有完整ASCII **Montserrat14**（`0xa0262f60`，含Y）；67套fmt_txt字库中原清单漏12套。全部字形/映射/字距与LVGL标准资源一致。页面子集确有裁剪；不再把“55套缺Y”推广为ROM缺Y。[核查及像素证据](project-workflow/ap01-template-integration/font-rom-audit.md)。核查仅读归档，未访问设备。
- AP01 v9：完整Montserrat14/CJK20、额度64/60px；完整像素/九场景通过，沿用I4渲染。后续34修复焦点恢复，用户确认自动返回/额度正常；多页物理与长期边界保留，其他设备Profile不变。[布局证据](project-workflow/ap01-template-integration/bundle-refresh.md)。
- 本次导航项可结案；C9整体仍开放，物理掉电/Boot2前回退、真实调用链高水位等既有边界不变。反向完整导航为原生模拟证据，用户本轮确认来自顺时针实测请求。
- 130/133/134原生完整OTA、同NOR升级/回切/原事务回执恢复通过；12组原厂活动LVGL像素与共享引擎差异0，匹配33宿主ASan/UBSan通过。128旧32对照失败；126/127/129及131/132的测试路径/离线引导门控/漏传回执参数记录保留，未放宽放行。
- [部署与边界](project-workflow/ap01-template-integration/release-0041-20261020.md)；实机证据 `artifacts/ap01-app-rollout-20261020/physical-verification-navigation33.json`；[待办C9](docs/roadmap/backlog.md)。

## 精确ROM

- **边界迁移候选03（未部署）**：`artifacts/ap01-firmware-boundary-20261022/ap01-boundary-candidate03.bin`，7043304B，build`39a3d969e2e7f2e49956054feebec8fe`，SHA256`a0053614fba6c9d3e2443a71a61d843f4aec85ea0c934b25fa04503bdbde1fbf`；ELF`76a291bb2e8c7bfe0dc48a197f08dddc5a81783f7049d3e231171601fd888587`。原生执行02，与03 ELF/可执行payload逐字节相同；[验收边界](artifacts/ap01-firmware-boundary-20261022/acceptance.json)。

- **已部署应用菜单码OTA06／槽1**：`artifacts/ap01-device-code-ota-20261021/ap01-0041-menu-ota-candidate06.bin`，7019304B，build`5757fdd677800d1412d8795c1cb2dbe0`，SHA256`82d33c493f7625a2b22588210d6492897af93b4f7eba05d3559abd9f15197e18`；ELF`76cbab6897edb5652a72a6553d749f0be799d7c9aa698fad1e7678749003be86`。[离线验收](artifacts/ap01-device-code-ota-20261021/offline-acceptance.json)。

- **设备码/手动鉴权05归档（已由06替换）**：`artifacts/ap01-device-code-auth-20261021/ap01-0041-device-code-auth-candidate05.bin`，7019080B，SHA256`ccfa3c7dee00f81b4c2a9b60add89944eef82857bddff710f73443d0f4a26e7d`；ELF`386adefc11615650eb16b4a35a93d060fbedfdfa24762099f537dfec0615e8fd`。单次OTA成功及同MAC/build/槽位已认证，实体菜单/鉴权通过；未声称新活动槽全Flash哈希读回。

- **配网列表后端0041归档（槽1已由05替换）**：`../cuktech-ap01-firmware/evidence/wifi-profiles-20261021/ap01-0041-wifi-profiles.bin`，7006536 B，SHA256 `1ce657386b8deff11887428a30340bdaf91701981c2f82d9bd1d56aeb9df33bf`。基于诊断34隔离基线；该次非活动槽1全镜像实读匹配（含PEC）。[部署/边界](../cuktech-ap01-firmware/reverse/reports/ap01/wifi-profiles-0041-20261021.md)。

- **独立诊断0041归档（槽1已由列表版替换）**：`../cuktech-ap01-firmware/evidence/diagnostic-0041-20261021/ap01-0041-independent-diagnostic.bin`，7000056 B，SHA256 `29352943f2301cc7bcb5fac60a504803d3b3488afe11d56c5dc27c23bc6f176c`。

- **34归档（槽1已由独立诊断版替换）**：`artifacts/ap01-platform-integration/device/ap01-0041-page-restore-20261020.bin`，6995768 B，SHA256 `6ef5f3fd831e2f4faca5d50e2ea92cd620844080d33afb1873dc25a0fcfa42aa`；匹配ELF SHA256 `671adea921e1259dd87848fa901dd830489d19e6038b95512466ec794bf90426`。匹配发布包/上传完成及认证运行已验，未声称全Flash实读SHA。

- Note4已部署：`artifacts/bridge-macos/note4-0.18.106-plan-activation.bin`，1,865,680 B，SHA256 `36ca4c3aacdc7b27c9c7cebe4cb18df0bee4048461e189f51050b6332b24ceb4`；匹配ELF SHA256 `40cbc05fcaccdd9f144132f2ffb40db61d956119e636df69be7591073e8fe21a`。

- 上版33归档（槽1已由34替换）：`artifacts/ap01-platform-integration/device/ap01-0041-pet-navigation-20261020.bin`，6994872 B，SHA256 `a4a34b872815711094c68f4a5c6c1ded8c2316475520bff95e1d8ec25011c535`；匹配ELF SHA256 `5696ba3e8193d9b2458936edbc2394cf2372930ff1b3dd41d52bc4d4a149ecbd`。
- 救援槽0：`artifacts/ap01-base0031-20261019/ap01-base0031-v2-rom.bin`，6822344 B，SHA256 `d843aae4e097c31ce06a315c62e1a66f4fb3bf9299db289b6f31562d0ef2231b`。2026-10-21误刷后已恢复此原版，认证启动槽0；非活动槽全镜像6822344 B实读SHA256与此归档完全匹配（含PEC）。
