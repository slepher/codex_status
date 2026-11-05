# Codex Status 当前交接

更新时间：2026-11-04，Asia/Shanghai。

## 当前部署

- Windows默认Bridge已部署单一共享族网络工作台（含有线回退提示更正）并重启：`bridge/target/debug/bridge-app.exe`，39005184 B，SHA256 `14c519e07efaa9e13f5ce5d08a9985c0ae627993578c38abf43608735a6575d4`。health204/MCP44、主进程/watchdog/端口核验通过；旧exe及295文件data完整备份校验，3族/3设备配置、模板/Profile/设置及凭据保留。[验收](project-workflow/wifi-families/status.md)

- 当前Mac（arm64）Bridge已首次构建/启动：`artifacts/bridge-macos/Codex Status Bridge.app`，exe 81394752 B，SHA256 `de3e039fddefcfb8b45e37ddca86359f0f7af7f781035317eddcf767130578df`。按需LaunchAgent supervisor51790/child51794，8765健康204、8766 MCP初始化通过，Codex app-server已启动。该机原无应用/运行数据，当前data已导入恢复的3组Wi-Fi，AP01已重新配对并认证登记；Note4因不在身边待配对，未验设备投递；此前Mac数据/设备ACK记录不能视为本机现场。Rust1.99.1、ArduinoJson7.0.4；托盘左键打开面板、右键菜单：Mac取消自动菜单挂载，改为右键显式弹出，用户两键实测正常；Windows源码保持同逻辑、未部署；已修复未选设备时初始化异常及主机网络按钮漏授权，Node/WK80和窄宽截图通过，用户已确认真实权限弹窗允许；菜单栏刷新已改原子保留系统模板模式，透明底居中数字，已按用户要求移除全部额外状态标记；定向2测试通过，原应用/data已备份。仅数字版本重建/重启，health204/MCP ping通过。旧数据迁移包未含Bridge目录；从历史对话找回2台Mac设备登记与3组Wi-Fi/历史密码，私有恢复清单在`artifacts/bridge-macos/recovered-device-wifi-20261102.json`；3组2.4G密码已导入并逐项读回一致，已按用户要求将3组5G密码设为对应2.4G密码并读回一致（库revision2），未推送设备网络。仓库5模板已校验并导入平台库，建立3个屏幕族的“仓库默认”本地配置；未绑定/发布设备。[记录](project-workflow/bridge-macos/tray-contrast-20261102.md)。
- AP01 `34F01557210B`：首尾动画修复ROM应用槽1，版本`1.0.2_0041`、build`ec0a58f71f65c2830c08c9bfd7c155db`、Bundle`6a8d702a`。ROM `artifacts/ap01-wifi-animation-20261022/ap01-wifi-animation-candidate01.bin`，7045480 B，SHA256 `5a49f78b550fabbafcc3cde75aa1f6e9986fb9dd88320cd029cd6655d61e0cee`。10-22认证槽1，用户确认原厂界面/旋钮/App及双向快转首尾正常；10-23再次认证同build，未刷机、未复核槽位。
- 救援0031/槽0保留：10-22先认证build`53574b55382d55a514ef5b7767db66d9`及槽0，再单次写应用1；按用户要求未整槽读回/导出，停止标记保留。
- Note4 `.106-note4`认证通过；ROM `artifacts/bridge-macos/note4-0.18.106-plan-activation.bin`，1865680 B，SHA256 `36ca4c3aacdc7b27c9c7cebe4cb18df0bee4048461e189f51050b6332b24ceb4`。10-23未刷机。

## 活跃事项与下一步

- C9 独立 App：固件 D3/D4 已有集合日志、重启选择/清理、HTTP 与 Python CLI；首装/单更/批更/整集回退通过 C++ 服务及回环 TCP 交叉模型。Bridge core 已有 v3 维护传输与 AMF3/APX2 校验，12 项客户端回归、真实双包/能力匹配及回执恢复校验通过；core 单步协调及丢 commit 回复恢复测试通过；Rust/C++ 回环四链及丢回复恢复交叉通过；共享 Tauri/MCP 单步应用入口已接；离线回执查询、终态归档及接续更新已验；显式地址恢复/未确认事务取消已验；后台 worker/持久取消及 C++ 四链通过；UI 已接且 Node/WK88 通过；UI→Rust→C++ 四链通过（测试 IPC）；暂存 128 MiB/24h 清理及活动回执保留已验；UI/Rust→真实 RV32 五事务闭环已通过（测试 IPC）；业务可写恢复已接 ABI 3.1，真实 worker Busy 后恢复双基址通过；完整业务写集、原生 IPC、网络变化、设备孤儿回收与完整原厂启动仍待完成。未启停/部署 Bridge，现场不变；模型不替代硬件准入。[Bridge 接入](project-workflow/ap01-app-set-v3/status.md) · [固件证据](../cuktech-ap01-firmware/project-workflow/ap01-app-update/status.md)

- 协议整改 P0 源码/软件验收完成，已提交、未部署：HTTP 会话复用与重放、BLE 错误/四槽队列/Notify 退避/可恢复冷却；workspace522通过/5条件忽略，最终core+ble245通过/2忽略，Note4构建及匹配ELF栈88项通过。**未部署同版本候选** `artifacts/protocol-p0/note4-p0-0.18.106.bin`，1872736 B，SHA256 `1b93bf34f4139c21d322fb0a8a3c93d9333503fb056663c4771f8a07bfaeb864`；现场ROM不变。新工具链、资源预算及实机边界见[状态](project-workflow/bridge-device-protocol-remediation/status.md)。A9/B6/C8实机未结，P1/P2未实施。

- Wi-Fi页面重整已提交`53a1fdd`并部署Windows默认实例：唯一共享族列表/编辑器、比较页明确导入、有线回退/概览/恢复工具归位。Node/Edge80/窄宽审阅及隔离构建通过；macOS缺swiftc，B6实机未验。01:41部署备份与数据核验见[状态](project-workflow/wifi-families/status.md)，未刷ROM或手动切网。

- Wi-Fi族Windows已部署，旧网络已合并3族（2异名双频+1同名双频；3设备均只连2.4G），密码保留；Note4选3族/BW保留原1族，AP01手选不变。共享族库/有线指定、Note/BW现有字段传族名。Rust337/定向56、Node/Edge12通过；macOS原生/部署及实机切网归B6。[状态](project-workflow/wifi-families/status.md)。

- 书桌屏 `70041DD7A340`：USB部署`.38-bw`/`ota_0`，ROM `artifacts/desk-ota-20261026/desk-0.18.38-bw.bin`，1837008 B，SHA256 `2b002a21e2673a96a4c519fb746219737911a82f577a2b5597cb2a89fba6580e`，认证运行镜像SHA已匹配。原`.37`槽1/模板保留；154g构建、200/400宿主布局、匹配ELF栈通过，Note4未构建/部署。用户授权后经USB JTAG将原通用凭据转换为119B共享active WFS4，仅写NVS的0xC000扇区，完整NVS读回一致且原记录保留；实机加载1条/format4，HTTP确认connected/IP192.168.3.163。
- 旧quad v12含退役device.state；兼容v15+mini完整Bundle `99dc6326` 已用原事务续传，COMMIT显示displayed，认证commit_seq4097/context `ffaf4e43e43639c1`、quad active、content ready/error空；Bridge对账succeeded。用户随后确认模板已显示；按最新要求已开启sync_enabled并持久解除书桌屏暂停，HTTP数据seq288 ACK displayed且认证applied_seq288/content_error空。此前实体BOOT169ms内响应。Windows JTAG接口已安装签名官方驱动oem88.inf，诊断临时Bridge配置仍保持还原。调试/恢复期间记录RTC WDT重启，根因未定；最后两次HTTP同wake/uptime160→175，不能扩大为长期稳定验收。B6证据：[状态](project-workflow/builtin-radio-status/status.md)、`artifacts/desk-ota-20261026/`。

- AP01 构建已统一到兄弟固件仓库，App 仅源码/配置及兼容入口；独立版/App版、隔离构建和旧命令验证通过，均未部署。[记录](project-workflow/ap01-unified-build/status.md)
- 未部署 platform ROM：`D:\Documents\PlatformIO\Projects\cuktech-ap01-firmware\build\unified-0041\platform.bin`，6875656 B，SHA256 `d77900bf5ce60ac9d1139a37d7a7314e75f2a6ac1f021eb7f04bfd9b440e2859`。
- 未部署 codex-status ROM：`D:\Documents\PlatformIO\Projects\cuktech-ap01-firmware\build\unified-0041\codex-status-commit.bin`，7044408 B，SHA256 `7cf24650ed8b24323e42b1b4a5400f5b05aa0d0f52c81127c2271c2c7d4e4183`。

- C9：AP01用量80多时画面/旋钮无响应，用户拔电后恢复；同部署build、认证displayed。用量重置关联未证，旧栈/死机前日志缺失，根因未知；未刷机。[调查](project-workflow/ap01-template-integration/freeze-20261028.md)
- 默认同步已提交，core/app/mcp 338通过/5忽略；应用及完整data已私有备份，重建期间data哈希一致。Bridge监督修复已随重建部署。
- C9：共享宿主有效帧、Wi-Fi菜单及首尾循环动画修复已部署；原生红绿/宿主消毒器/菜单回归及用户视觉验收通过，不扩大为全部GPIO或逐帧保证。10-22末轮未切网/改列表，临时UU清理未执行；此前失联后按键重连原因未定。[调查与实机](project-workflow/ap01-wifi-profiles/animation-20261022.md)
- 较早四日审查候选已由上述部署记录接续，不再代表当前ROM；历史候选/资源证据见[审查](project-workflow/review-fixes-20261022/status.md)。物理RF/掉电/完整栈高水位、A9/B6/C8新余量时延/跨会话通知/Windows及长期合盖恢复仍开放，60秒兜底保留。[唯一待办](docs/roadmap/backlog.md)

## 证据

[默认同步前卡](docs/history/progress-archive-2026-10-23-before-default-data-sync.md)、[完整历史](docs/history/progress-archive-2026-10-22-before-review-fixes.md)。旧产物释放27.69GiB，部分历史链接可能失效。实机证据在`artifacts/ap01-entry-rollout-20261022/`、`artifacts/ap01-wifi-animation-rollout-20261022/`；同步备份/证据在`artifacts/default-data-sync-20261023/`。日志/SSID/绑定原文仅留私有artifacts。
