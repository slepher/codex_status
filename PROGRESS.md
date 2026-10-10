# Codex Status 当前交接

更新时间：2026-10-09 17:33（香港时间）

## 当前现场

- 工作区：代码修复已提交 `cf1cfa6`；交接文档已精简，AGENTS 按原始规则重写，详细操作迁至 `docs/agent-operations.md`，按任务读取对应节。以下现场为 2026-10-09 核验记录。
- Note4（`7C4FADB93408`，`192.168.3.177`）运行 `0.18.74-note4`，槽位 `ota_0`；17:06:52 直接认证运行镜像与发布 ROM 完整哈希一致。ROM：`artifacts/note4-0.18.74-activation-recovery.bin`，1,902,416 B，SHA256 `C83E75DBCAB4F758CA259CD08C06EAC6553683795B86C65266B49F8ACBA93813`。
- 默认 Windows Bridge 已运行，HTTP/MCP 端口 `8765/8766`，PID54288/watchdog2428；exe SHA256 `76337653390D4EC6F9E162E14BD02B2BF58FD9AF47852A5746F76C0B4DB1432C`。书桌屏 `70041DD7A340` 暂停后台同步，登记/Profile/凭据保留，ROM 未动。
- `.72` 已修复大 Status 有界分页与整体 HTTP 期限、Bridge/Wi-Fi 状态隔离及 Wi-Fi 图标独立重绘；实机认证状态、Bridge 刷新与运行 ROM 核验通过。

## 后续关注

- 数据自动投递修复（17:31部署Bridge）：旧调度以30秒状态新鲜度排除刷新目标，慢周期后永久停摆，OTA后实机seq0。现过期在线先认证刷新，每分钟有界探测登记IP，发送仍过新鲜MAC/token/owner闸门。源used24→余76，17:32:34自动seq377 ACK applied/displayed，直接认证data_seq/applied_seq377，最新确认指纹一致。完整app76通过/3条件忽略，真实Fake连续两次源变化自动应用回归通过。未手动推送冒充自动恢复，ROM仍.74、USB未操作；备份artifacts/bridge-before-data-chain-20261009.*。

- A9/B6：.74 已修复 BLE hold 阻塞激活完成、同配置健康链显式激活先断开、Bridge 丢弃合法 window_closed 错误导致不恢复、Sleep无Data只刷时钟遗漏Bridge标记。显示RTC标记只在确认帧后更新；Fake补深睡RTC联系保留及失败重画，62/62通过。已有 .73 Light绝对期限保护继续保留。实机原home配置已恢复，17:07:31 Wi-Fi connected/HTTP ready、Light2997剩519s、最近有效联系正常，非凭据修改。详见[专项](project-workflow/bridge-macos/wifi-idle-activate-20261008-plan.md)。
- OTA原job1172f92d：17:01:29排队，超过两分钟未上传继续调查；17:04:55单次上传ACK，17:06:17自动succeeded/image_verified，17:06:52直接运行镜像复核。没有换ID重传。备份exe/data在artifacts/bridge-before-fix74-20261009.*；匹配ELF栈门禁通过，静态RAM117,948 B，Flash1,851,231 B。
- USB事故独立：16:52–54右侧扩展坞VID1A86/PID80A0多次意外移除整棵设备树，最多27设备；XHC0，与左侧Note4的XHC3不同。COM5原生只读监听16:38结束，无DCB/DTR/RTS/下载/烧录/控制器重置，后续仅网络OTA。不能据此判断扩展坞是供电/线缆/驱动问题；事件证据artifacts/usb-dock-drop-20261009.json。
- 实机多周期 RF/电池/掉电、动态堆峰值和显示故障矩阵仍未完成；可信显示基线不等于屏幕目视验收。对应路线图见 `docs/roadmap/backlog.md` 的 A9/B6。
- macOS 原生 UI 验收需在有 Swift/WKWebView 的主机完成。不要据合成 IPC 测试宣称真实设备通信验收。

## 深入记录

- 本轮及此前详细进度原文：[2026-10-09 归档](docs/history/progress-archive-2026-10-09.md)
- 待办唯一来源：[backlog](docs/roadmap/backlog.md)
- 文档导航：[docs/README.md](docs/README.md)
