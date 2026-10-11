# 2026-10-11 协议部署前交接卡归档

以下为13:12精简前原文，候选/待确认描述保留为当时状态；当前部署以仓库根PROGRESS.md为准。

# Codex Status 当前交接

更新时间：2026-10-11 12:26（香港时间；Note4 .75 OTA已认证核验）

## 当前现场

- 工作区：代码修复已提交 `cf1cfa6`；交接文档已精简，AGENTS 按原始规则重写，详细操作迁至 `docs/agent-operations.md`，按任务读取对应节。Wi-Fi元数据修正尚未提交；固件现场于10月11日核验，Bridge信息沿用10月9日记录。
- Note4（`7C4FADB93408`，`192.168.3.177`）运行 `0.18.75-note4`，槽位 `ota_1`；12:26:29直接认证运行版本、MAC及完整ROM长度哈希一致，wifi_state=connected。ROM：`artifacts/note4-0.18.75-wifi-metadata.bin`，1,903,824 B，SHA256 `C0C5122FD5185B32665CA375964793FB6FCDD319576CA5AC216BA7FB1BB7E79D`。OTA原job `62b50171` 单次上传，succeeded/image_verified；证据`artifacts/ota75-completed.json`、`ota75-running-image.json`。
- 默认 Windows Bridge 已运行，HTTP/MCP 端口 `8765/8766`，PID54288/watchdog2428；exe SHA256 `76337653390D4EC6F9E162E14BD02B2BF58FD9AF47852A5746F76C0B4DB1432C`。书桌屏 `70041DD7A340` 暂停后台同步，登记/Profile/凭据保留，ROM 未动。
- `.72` 已修复大 Status 有界分页与整体 HTTP 期限、Bridge/Wi-Fi 状态隔离及 Wi-Fi 图标独立重绘；实机认证状态、Bridge 刷新与运行 ROM 核验通过。

## 后续关注

- **协议实施/部署验收中（A9/B6/C8）**：两项会合P1及认证拒绝/409分类/元数据合同类型共五项修复，316通过/3条件忽略，设备页/52栈门禁通过。新版Bridge PID56448，SHA256 `D74AF0FCD262D4123195CB91FE51163B16282C96490CECDC23E46816963624E6`；旧exe/data已备份。.76候选 `artifacts/note4-0.18.76-protocol.bin` /1,903,824 B/ SHA256 `49558ECDA0E81302180AEC36C9E00C3070F8C55E0BD9DAF9D4FCB6E56C2200B0`，原OTA86054e26仅一次upload_ack，运行hash尚待确认；重启后Wi-Fi报connect_timeout，未重传。真实多周期未验。[实施证据](../../project-workflow/bridge-macos/protocol-implementation-20261011.md)。

- **协议历史审核/规划已完成（A9/B6/C8，10月11日）**：Luna检查85个近期提交、重点10项；主代理复核两项P1仍在当前Bridge源码。先补BLE-only+sync enabled+Light零HTTP回归并修尾部准入，再修ready/HTTP换接，随后做ACK/未知结果/错误透传及跨阶段矩阵。仅审核和计划，未新增部署；[报告](../../project-workflow/bridge-macos/protocol-history-review-20261011.md)、[计划与结项闸门](../../project-workflow/bridge-macos/protocol-followup-plan-20261011.md)。

- **Wi-Fi模板元数据固件已部署（A9/B6）**：增加本地wifi_state与仅连接期存在的wifi_blink；模板决定等待隐藏/连接闪烁/常亮/终态OFF，阶段和相位均触发重绘。quad v13、Note4模板v3；render57/core15、生产C++/Python及52项ELF栈门禁通过。已部署`artifacts/note4-0.18.75-wifi-metadata.bin`，1,903,824 B，SHA256 `C0C5122FD5185B32665CA375964793FB6FCDD319576CA5AC216BA7FB1BB7E79D`；RAM117,964 B。运行ROM已.75，Bridge/运行库未动、Bundle未发布，当前活动模板仍走兼容路径；实机闪烁/多轮耗时未验。[实施与资源](../../project-workflow/bridge-macos/wifi-template-metadata-20261011-plan.md)。下述另两项协议问题仍待修。

- **10月11日 WIFI OFF 协议审核（A9/B6，待修）**：11:48 BLE seq398/Plan3413成功，本轮needs_http=false却继续诊断HTTP至11:48:56失败；11:50开网报告认证verified后会合仍超时。源码确认OFF缺终态判据、诊断尾部绕过HTTP准入、ready瞬态与HTTP可用性混用三项问题。11:53–54直接认证Note4仍.74、398 applied/displayed、Wi-Fi已恢复；未改ROM/Bridge/USB。证据、修复与验收边界见[审核](../../project-workflow/bridge-macos/wifi-terminal-review-20261011.md)，不得据当前ON宣布已修复。

- 数据自动投递修复（17:31部署Bridge）：旧调度以30秒状态新鲜度排除刷新目标，慢周期后永久停摆，OTA后实机seq0。现过期在线先认证刷新，每分钟有界探测登记IP，发送仍过新鲜MAC/token/owner闸门。源used24→余76，17:32:34自动seq377 ACK applied/displayed，直接认证data_seq/applied_seq377，最新确认指纹一致。完整app76通过/3条件忽略，真实Fake连续两次源变化自动应用回归通过。未手动推送冒充自动恢复，ROM仍.74、USB未操作；备份artifacts/bridge-before-data-chain-20261009.*。

- A9/B6：.74 已修复 BLE hold 阻塞激活完成、同配置健康链显式激活先断开、Bridge 丢弃合法 window_closed 错误导致不恢复、Sleep无Data只刷时钟遗漏Bridge标记。显示RTC标记只在确认帧后更新；Fake补深睡RTC联系保留及失败重画，62/62通过。已有 .73 Light绝对期限保护继续保留。实机原home配置已恢复，17:07:31 Wi-Fi connected/HTTP ready、Light2997剩519s、最近有效联系正常，非凭据修改。详见[专项](../../project-workflow/bridge-macos/wifi-idle-activate-20261008-plan.md)。
- OTA原job1172f92d：17:01:29排队，超过两分钟未上传继续调查；17:04:55单次上传ACK，17:06:17自动succeeded/image_verified，17:06:52直接运行镜像复核。没有换ID重传。备份exe/data在artifacts/bridge-before-fix74-20261009.*；匹配ELF栈门禁通过，静态RAM117,948 B，Flash1,851,231 B。
- USB事故独立：16:52–54右侧扩展坞VID1A86/PID80A0多次意外移除整棵设备树，最多27设备；XHC0，与左侧Note4的XHC3不同。COM5原生只读监听16:38结束，无DCB/DTR/RTS/下载/烧录/控制器重置，后续仅网络OTA。不能据此判断扩展坞是供电/线缆/驱动问题；事件证据artifacts/usb-dock-drop-20261009.json。
- 实机多周期 RF/电池/掉电、动态堆峰值和显示故障矩阵仍未完成；可信显示基线不等于屏幕目视验收。对应路线图见 `docs/roadmap/backlog.md` 的 A9/B6。
- macOS 原生 UI 验收需在有 Swift/WKWebView 的主机完成。不要据合成 IPC 测试宣称真实设备通信验收。

## 深入记录

- 本轮及此前详细进度原文：[2026-10-09 归档](../history/progress-archive-2026-10-09.md)
- 待办唯一来源：[backlog](../roadmap/backlog.md)
- 文档导航：[docs/README.md](../README.md)


## 17:42 更新前交接卡（.80基线）

# Codex Status 当前交接

更新时间：2026-10-11 16:40（香港时间）

## 部署与工作区

- Note4 `7C4FADB93408` 已认证运行 **0.18.80-note4 / ota_0**，登记IP `.177`，16:27:53完整镜像匹配。ROM `artifacts/note4-0.18.80-zlib-blocks.bin`，1,920,688 B，SHA256 `4CCD39B2C751C44DD6C89527742592656B3495F9700A4121D98BFDEB6C0A7B83`；同名ELF。OTA `0972b435` 单次上传，succeeded/image_verified。USB continuous/light，压缩能力1；旧模板迁移后Data441 displayed。
- Windows默认Bridge已配套更新，PID63064，计划任务启动；exe SHA256 `BD641F1E5ADC0870063B329F77F6734C22F4E22F8AB418185C081850CBE7C2FB`。旧exe/data备份 `artifacts/bridge-before-c9-20261011.exe`、`bridge-before-c9-20261011-data.zip`（ZIP CRC通过）；书桌屏未升级。
- 本轮协议、Wi-Fi元数据与BOOT/PWR修复一并提交；新模板Bundle未发布，当前屏仍用旧模板兼容路径。

## 活跃事项（A9/B6/C8）

- **B6追加交接**：诊断进程已停止设备访问；.80部署身份/结果见上方，BOOT/GATT图标与异常启动仍待[统一执行/验收](project-workflow/bridge-macos/note4-boot-gatt-followup-20261011.md)。手动窗口与自动BLE必须独立；16:08曾确认窗口有效而用户未见图标，异常启动根因仍未知。

- BOOT/联网合同及短时入睡修复已部署：[实施](project-workflow/bridge-macos/boot-network-20261011.md)、[根因/修复](project-workflow/bridge-macos/boot-five-second-investigation-20261011.md)。PWR/BOOT统一300s下界，初始化GPIO0锁存去重，时钟/交汇/底层睡眠入口保窗。新增生产窗口/按键回归、固件模块、52项ELF栈门禁通过；配套Rust回归311通过/3忽略。认证HTTP持续正常、BLE关闭、automatic_ble=false；证据 `artifacts/boot-fix78-running.json`、`note4-fix78-verify-20261011.json`。真实PWR→BOOT重叠按键、timer竞态与功耗矩阵仍需实机验收。

- [历史审核](project-workflow/bridge-macos/protocol-history-review-20261011.md)后完成五项修复：HTTP诊断尾部遵守准入；会合ready与HTTP能力分开；新写入401/403明确失败而历史未知仍查询原ID；保留409修订冲突；wifi_state合同报文本。316测试通过/3条件忽略，末次403补丁真实client7项通过；设备页、52项匹配ELF栈门禁通过。
- 13:10自然恢复会合：ready_report verified→rendezvous complete，Data414 applied/displayed；认证HTTP可用且rv.ready=false，不再错误等待。OTA后此前有connect_timeout及三轮会合失败，原激活结果经查询后自动恢复，不能抹去或归因为新修复已覆盖全部RF故障。证据`artifacts/protocol-ota76-status.json`、`protocol-device-latest.private.json`。
- **下一步/验收边界**：真实10轮自然会合、物理丢ACK/复位矩阵、屏幕闪烁与事件对齐未完成；长期RF/功耗/掉电/动态堆峰值未知。macOS原生通信与1.54未验，不结A9/B6/C8。详见[实施/矩阵](project-workflow/bridge-macos/protocol-implementation-20261011.md)、[计划](project-workflow/bridge-macos/protocol-followup-plan-20261011.md)。

## 入口

- **单份内容/统一压缩（C9）已部署**：Note4复用ROM miniz，CSZ1独立16KiB块、压缩驻留、Bridge冻结字节；下载后停内容覆盖，保留原生OTA。359项通过/3条件忽略，匹配ELF栈检查通过。[交接/限制](project-workflow/single-copy-updates/status.md)。未发布新模板或实测压缩安装峰值；旧故障根因未明。AP01实机/加载器待调查。
- **BOOT/GATT与启动计划统一续项（B6/C8）**：正式计划跨重启、Light/USB走HTTP、Sleep一次BLE、OTA确认已按用户要求补入[统一执行文档](project-workflow/bridge-macos/note4-boot-gatt-followup-20261011.md)。当前仅规划；本轮未完成源码草稿已备份并撤回，未构建/部署.81。待统一实施，.78诊断仍用保存的.78 ELF。

- 待办唯一来源：[backlog](docs/roadmap/backlog.md)
- 前次现场/USB独立事故与详细里程碑：[10月11日归档](docs/history/progress-archive-2026-10-11.md)、[10月9日归档](docs/history/progress-archive-2026-10-09.md)
- 导航与操作：[docs/README.md](docs/README.md)、[agent-operations](docs/agent-operations.md)


## 17:55 更新前交接卡（.81与.82候选）

# Codex Status 当前交接

更新时间：2026-10-11 17:42（香港时间）

## 部署与工作区

- **电量平滑候选已构建，尚未刷入**：`artifacts/note4-0.18.82-battery-smoothing.bin`，1,926,256 B，SHA256 `B2C9E1C1FA8E8A01FE2CB66985875155E61B6F6ADF08F046E8B98C69D6E286E7`，同名ELF；12项定向测试/66项栈检查通过，下一步原Bridge OTA认证验证。[执行记录](project-workflow/bridge-macos/note4-battery-smoothing-20261011.md)。下方.81仍为当前部署。

- Note4 `7C4FADB93408` 已认证运行 **0.18.81-note4 / ota_1**，登记IP `.177`。ROM `artifacts/note4-0.18.81-boot-plan.bin`，1,925,792 B；SHA256 `BC2ED81473FADC1EBC04DA96CE299550AFC527311FE28ED236FD392ADC109391`，同名匹配ELF。OTA `35c89bae` 单次上传、succeeded/image_verified。
- 默认Bridge通过计划任务部署，exe SHA256 `3F53CFEB953A883C1637280E7332357CC54CC44C35E989DA7551B0160E3C759D`。exe/data备份见 `artifacts/bridge-before-contract-fix-20261011*`。书桌屏未刷机。
- 基线已提交 `f822531`；后续统一实现仍在工作区。实际codex-status-a v4已发布，Bundle `53cab015` succeeded，认证applied_seq453/displayed。不要用本地夹具覆盖用户实际布局。

## 活跃事项与验收边界（B6/C8，关联A9）

- **BOOT/GATT、跨重启计划、OTA统一实现已部署**：[执行/测试/资源/剩余矩阵](project-workflow/bridge-macos/note4-boot-gatt-followup-20261011.md)。用户六次实体BOOT开关符合预期；手动图标独立于自动BLE。PPL1正式计划3490经一次明确USB工具复位仍同ID/restored_light，剩417/600秒，直接HTTP且自动BLE关闭。没有一次性OTA marker，报告不删除正式计划。
- core/app等241测试、render/MCP/device-sim126测试通过，3条件忽略；固件模块及匹配ELF66项栈门禁通过。数据合同变更后旧索引失效、发布ACK后认证回读已修正并配套部署。首次模板短暂内置界面的拒绝码未保留，不将源码缺口当唯一已证根因。
- **未结问题**：USB工具复位后启动/认证HTTP延迟、间歇超时，根因未知；被动串口对照未证明CDC因果，boot保持、模板已恢复。证据 `artifacts/note4-81-cdc-auth-meta.json`、`note4-81-final-auth.private.json`。不声称历史.78停滞已根治。
- 无USB Sleep重启BLE、电池深睡/Light到期竞态、Flash掉电、长期RF/功耗、动态堆峰值及macOS/1.54未验；B6/C8不结项。电量95/96/97抖动已查本地Slate和GitHub CrossPoint：前者单次平均/充电显示策略，后者整数EMA；尚未改电量算法，证据与建议见同执行文档末节。

## 入口

- C9单份内容/CSZ1压缩随.80继承至.81：[交接](project-workflow/single-copy-updates/status.md)。原生双OTA分区保留；AP01实机/加载器后续调查。
- 唯一待办：[backlog](docs/roadmap/backlog.md)；[当日历史](docs/history/progress-archive-2026-10-11.md)；[操作细则](docs/agent-operations.md)。


## 21:07交接卡归档（BOOT网络修复前）

# Codex Status 当前交接

更新时间：2026-10-11 21:07（香港时间；.83已部署，Fake回归完成，网络问题先调查不改）

## 部署与工作区

- Note4 `7C4FADB93408` 已认证运行 **0.18.83-note4 / ota_1**，登记IP `.177`。ROM `artifacts/note4-0.18.83-temporary-sync.bin`，1,927,024 B；SHA256 `6343FCDA4676509BEEB5025F79E991C6BDCFA399F88A4C8336902489AAD1207B`，同名匹配ELF。OTA `e5c08f9a` 单次上传、succeeded/image_verified，认证运行镜像哈希已核对；证据 `artifacts/note4-83-verify.private.json`。
- 默认Bridge配套部署、通过计划任务运行，exe SHA256 `5A205444A78893ADBBA6629195EA447A14EB5B0C9A8101DA4B668CDBAC6D18EB`。exe/data备份见 `artifacts/bridge-before-83-20261011*`。20:31 PID75952、watchdog44608，8765/8766归前者。书桌屏未刷机。
- 统一实现与电量平滑已提交 `9c38e45`（基线 `f822531`）。实际codex-status-a v4已发布，Bundle `53cab015` succeeded，认证applied_seq453/displayed。不要用本地夹具覆盖用户实际布局。

## 活跃事项与验收边界（B6/C8，关联A9）

- **BOOT blink调查/Fake回归完成**：[测试计划与覆盖](project-workflow/bridge-macos/boot-blink-fake-test-20261011.md)、[源码6项缺口](project-workflow/bridge-macos/boot-blink-code-review-20261011.md)。Luna有效回归64/64通过；Fake PID34328、127.0.0.1:18783保持运行，默认Bridge/watchdog与端口已复核。实体BOOT/自动BLE收尾/EPD/LED未被Fake覆盖。20:43关闭BLE只读对照未复现blink；历史逐按根因仍未知。Wi-Fi图标依赖BLE、Bridge失败主动断Wi-Fi、presence缓存误判、会话nonce兼作boot_id均列入调查，暂不修改网络行为。
- **绿灯数据活动候选已实现、未部署**：[实现/测试/资源](project-workflow/bridge-macos/data-led-20261011.md)。宿主策略/模块测试、Note4构建及匹配ELF64项栈检查通过。ROM `artifacts/note4-0.18.83-data-led-candidate.bin` 1,836,832 B，SHA256 `31D734AE255A260ACDE30D696DE2FFDC2DEC84D877B31BBE34D07E483366632D`，同名ELF；版本沿用.83仅本地候选，勿与上方部署镜像混淆。GPIO/动态堆/功耗实测待验。

- **周期全量同步临时Light修正已配套部署，完整实机矩阵未通过**：[实施/测试/资源](project-workflow/bridge-macos/sync-window-fix-20261011.md)、[部署与图标现场调查](project-workflow/bridge-macos/note4-radio-phenomena-20261011.md)。实机reason=data/seq463进入WIFI_SYNC_ONCE、同open重放ACK；跨到期测试因停Bridge移除ready接收方而无效，另受BOOT 300s下限干扰，不能报通过/失败。恢复Bridge后Data464 ACK、诊断457 complete。已确认bridge_http_unreachable主动断Wi-Fi、BOOT保持期限未阻止断网；Bridge图标只按>6min新鲜度而非明确不可达。用户要求Wi-Fi继续ON/Bridge OFF，暂不修。蓝牙消失正常、其后blink异常的具体逐按因果未知；不把连接中blink代码当作该现象的验收依据。

- **BOOT/GATT、跨重启计划、OTA统一实现已部署**：[执行/测试/资源/剩余矩阵](project-workflow/bridge-macos/note4-boot-gatt-followup-20261011.md)。用户六次实体BOOT开关符合预期；手动图标独立于自动BLE。PPL1正式计划3490经一次明确USB工具复位仍同ID/restored_light，剩417/600秒，直接HTTP且自动BLE关闭。没有一次性OTA marker，报告不删除正式计划。
- core/app等241测试、render/MCP/device-sim126测试通过，3条件忽略；固件模块及匹配ELF66项栈门禁通过。数据合同变更后旧索引失效、发布ACK后认证回读已修正并配套部署。首次模板短暂内置界面的拒绝码未保留，不将源码缺口当唯一已证根因。
- **未结问题**：USB工具复位后启动/认证HTTP延迟、间歇超时，根因未知；被动串口对照未证明CDC因果，boot保持、模板已恢复。证据 `artifacts/note4-81-cdc-auth-meta.json`、`note4-81-final-auth.private.json`。不声称历史.78停滞已根治。
- 无USB Sleep重启BLE、电池深睡/Light到期竞态、Flash掉电、长期RF/功耗、动态堆峰值及macOS/1.54未验；B6/C8不结项。电量95/96/97抖动已查本地Slate和GitHub CrossPoint：前者单次平均/充电显示策略，后者整数EMA；电量EMA已随.82落地，见下条。

- **电量平滑已部署/用户验收通过**：[执行记录](project-workflow/bridge-macos/note4-battery-smoothing-20261011.md)。5秒采样、90/10整数EMA仅用于显示，原始电压/低电保护独立；RAM+16 B、BIN+464 B，无NVS/新库/新任务。12项定向回归和66项栈检查通过；20次实机观测原始95/96/97、采样约5秒，用户按键确认没有跳变。新boot `eb3cc78d6243e38e5feb871b29efcb77`，模板ready/displayed；证据 `artifacts/note4-82-ota-status.json`、`note4-82-battery-observation.json`。

- **协议文档已按9c38e45同步**：[协议总览](docs/bridge-device-protocol.md)、[48 B编码](docs/bridge-device-compact-envelope-design.md)、[场景/实现对照](docs/bridge-device-communication-implementation.md)。更新PPL1/BOOT/OTA、单份CSZ1 Bundle、合同失效和实际POST/frame；仅文档，不改变部署或扩大验收结论。

## 入口

- C9单份内容/CSZ1压缩随.80继承至.81：[交接](project-workflow/single-copy-updates/status.md)。原生双OTA分区保留；AP01实机/加载器后续调查。
- 唯一待办：[backlog](docs/roadmap/backlog.md)；[当日历史](docs/history/progress-archive-2026-10-11.md)；[操作细则](docs/agent-operations.md)。
