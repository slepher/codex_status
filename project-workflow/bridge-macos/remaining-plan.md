# Bridge macOS 与设备页 BLE 配网：剩余工作接续计划

最新配网/异步会合执行入口（2026-10-03）：[wifi-rendezvous-execution.md](wifi-rendezvous-execution.md)。以最新 PROGRESS 的 .44/Bridge 与 BLE 首批已完成为基线，从 W0–W6 实施剩余能力；第6节同时落实此前 Bridge 双侧方案、完整分组、差异预览和离线任务界面。下方旧基线/旧交互仅作历史，不重复已完成项。

最新 BLE 缺口专项入口（2026-10-03，仅规划）：[fake-rom-ble-fidelity-plan.md](fake-rom-ble-fidelity-plan.md)。先按 P0–P4 补齐 ROM/Fake 共用字节合同、512 字节属性与真实 ACK 等待/待确认回归，再沿 Wi-Fi 重设计推进；本轮不执行下方历史实施授权中的代码、构建或设备操作。

最新设计入口（2026-10-03）：用户要求重新设计 Wi-Fi 全量方案管理与同步，见 [wifi-redesign.md](wifi-redesign.md)。当前 protocol=1 实现与下方旧规划不等于新设计已完成；新稿覆盖单方案管理/试连即保存启用的旧交互。Note4 试连跳屏异常待诊断，最新产物/未部署改动见 PROGRESS 顶节。


日期：2026-10-03。本文件供新窗口接续尚未完成的实施，不重新执行已完成项。用户已授权“按照计划开始执行”；计划编写后用户已要求继续实施；2026-10-03 已推进 R0–R4 和 R5/R6 软件实现，最新部署见 PROGRESS 顶节与 wifi-protocol.md；不提交代码。待办事实仍以 `docs/roadmap/backlog.md` B5/B6 为唯一来源，现场以 `PROGRESS.md` 最新节为准。若文件描述冲突，按最新用户指令、现场记录与权威设计核对后更新。

## 新窗口先做什么

1. 依次读 `AGENTS.md`、`PROGRESS.md` 最新现场、backlog B5/B6、本文件。按任务需要读 `wifi-setup-plan.md`、`docs/power-state.md` 和 `docs/generic-display-platform-design.md`，不要通读历史。
2. 查看 `git status --short` 和涉及文件的 diff：当前工作树包含已实现、已部署但尚未提交的 macOS/设备字段合同改动，也有未跟踪文件。保留并接续，不 reset、stash、清理或从干净 worktree 构建旧版覆盖现场。
3. 只读核对实际 Bridge 路径、8765/8766、运行数据和设备身份。下面 IP、串口、版本和哈希是交接时快照，重用前重新核对；不要为读日志打开串口导致设备复位。
4. 从 R0 开始，将明确的配网协议/权限/持久化规则写入实施记录，然后推进 R1–R4；不因 Windows/另一架构现场缺席而暂停独立的代码与软件验证。

## 用户已确定的需求

- macOS 首版须达到 Windows 已运行的 BLE 与低功耗会合能力。只沿用 `device_first`；`bridge_first` 已废弃，不恢复双策略、Publisher 或设备定时扫描。
- **蓝牙配 Wi-Fi 与本机绑定的独立 Wi-Fi 方案，均放在 Bridge 设备页。** 已登记设备离线仍可进入；未登记设备从设备页的蓝牙添加入口接入。Windows/macOS 共用 application service 和 UI。
- 每个 BLE bond 在设备上有独立网络方案，支持多组“Bridge 主机 SSID → 设备 2.4GHz SSID”映射。主机可以连接 5GHz SSID，设备可以使用不同名称的 2.4GHz 网络。
- 网络名称、映射和密码保存在设备；Bridge 读取本绑定的无密码摘要，按当前主机 Wi-Fi 通知设备使用网络 ID。配置时暂存输入密码用于加密传输，不能落 Bridge state、日志、审计、夹具或仓库，不能回读设备密码。
- 正常运行、断网、无凭据/AP、USB/电池、深睡物理唤醒均须有可用的 BOOT 蓝牙恢复入口。现有长按切模板意图保留。
- 当前只有 Note4，1.54 不在现场；只构建 `zectrix-note4-b`，不操作缺席设备。现有两目标共享代码仍做适当软件回归。
- 本机 Rust 应升级并使用全局工具链，不能建立隔离 Rust。独立构建 target 可保留运行数据。
- 设备运行字段以设备报告为准，与 Bridge 本地模板/Profile 或是否曾发布无关；保存许可/网络方案不自动发布 Bundle。

## 已完成的基线，不要重做

| 内容 | 已有结果与边界 |
|---|---|
| 全局 Rust | Rust/Cargo 1.99.0，Rustup 1.29.1；不用隔离 Rust |
| macOS 基础适配 | Intel/macOS 14 本地 `.app` 可运行；Windows 条件依赖、应用/seed/CLI 路径、稳定安装身份、Unix 单实例锁、菜单栏基础图标、peripheral ID 去重已实现 |
| BLE 绑定 | 当前 Mac 与 Note4 已绑定；临时诊断客户端完成首次绑定/加密/重连，实际 Bridge 验证已有绑定及 USB 配网重启后恢复。不要为正常重连解绑或重新配对；完整产品接入/拒权恢复仍待验 |
| 当前环境联网 | 已调用旧 ROM 的 USB Wi-Fi 命令恢复网络，旧槽位保留。**这不是 BLE 配网或按 bond 方案**；交接不携带密码 |
| 设备字段合同 | 认证 HTTP 从当前 CompiledTemplate 导出完整字段表；Bridge 缓存并在同一 context/active 下复用，未知/切换通过有界 Wi-Fi 读取，BLE GATT 状态表未扩展 |
| 字段实际投递 | Mac 无对应本地 Profile，设备报告 14 个字段，7 个需 Bridge 提供；原有设备许可 true，本轮 Agent 未改变。真实 HTTP seq/data_seq 3 获 applied/displayed ACK，未发布 Bundle |
| 软件回归 | 五包 225 passed / 1 ignored；后续 app/core 序号接续回归通过；Fake ROM bootstrap 44/44；设备页 JS 与 diff 检查通过。不能代替真实 RF、系统权限、窗口目视或功耗验收 |

设备字段表随现有认证状态返回并缓存，不新增独立长轮询。未知合同不发送空字段包，本地草稿不覆盖运行需求；只有成功 ACK 更新确认指纹与 full_sync_deadline。

## 现场、产物与恢复

| 项目 | 2026-10-03 交接快照 |
|---|---|
| 主机 | macOS 14.8.9 / x86_64，MacBookPro15,2，BCM_4364B0，型号规格 Bluetooth 5.0 |
| Note4 | MAC `7C4FADB93408`，IP `192.168.10.101`，target `zectrix-note4-400x300` |
| 运行固件 | `0.18.41-note4-b-fields1`，ota_1，active `codex-status-a`，最近认证 commit_seq 364 |
| ROM | `artifacts/bridge-macos/note4-0.18.41-fields1.bin`，1,665,024 B，SHA256 `9B87601138B68047F0AE71A46486944E2050E8A40651453D613C1E953D8B4947` |
| ROM 证明 | Bridge OTA job `3f2e8154` succeeded/image_verified；认证运行镜像前 1,665,024 B SHA256 与文件相同 |
| 本地应用 | `artifacts/bridge-macos/Codex Status Bridge.app`，二进制 `Contents/MacOS/bridge-app`，72,672,496 B，SHA256 `E16732894CEDA38BEE162996A2807A9FC7551496BBA15E2558BAF75EBA2D892E` |
| 运行端点 | HTTP 8765，MCP `http://127.0.0.1:8766/mcp`；通过 `open -n` 脱离工具会话启动 |
| 默认运行数据 | `~/Library/Application Support/dev.codexstatus.bridge/data`；`installation-id` 在同根，命名实例在 `instances/<name>/data` |
| 数据备份 | ignored `artifacts/bridge-macos/data-before-fields1/`，包含私人运行数据，不输出其 token 等内容 |
| USB | 曾为 `/dev/cu.usbmodem14101`，交接时监视已关闭；重新枚举后才使用 |
| 构建缓存 | `.pio/build/zectrix-note4-b`、`.pio-core`、`.pio-pkgs/note4`；保留完整 `.pio-core/sdkconfig.defaults.note4.snapshot`，不要 clean 或伪造指纹 |

应用为本地 ad-hoc 签名包，无正式签名/公证；没有启用登录 LaunchAgent，按需监督/崩溃恢复尚未实现。当前版本固件仍没有按 bond BLE 配网/选网能力，BOOT 离线行为未修复。

重建 Bridge 前只停**当前应用路径**的进程，备份外部 data，重建包含当前 UI 改动的工作区，再通过原生应用入口脱离工具会话启动；不停止其他路径/实例。不要删除包含运行数据的构建目录。已有正确绑定/token 使用正常私有运行缓存，不能打印、轮换或绕过认证来“恢复”。

## 实施顺序与交付条件

每个阶段先补充具体任务/实施记录，再动代码；完成后记录实现、测试和现场证据并同步 PROGRESS/backlog。下表中的阶段是 B5/B6 的执行拆分，不建立第二份待办账本。

| 阶段 | 剩余工作与主要位置 | 交付条件 |
|---|---|---|
| R0 接续与协议定稿（B6/W0） | 检查上述现场；确认真实 BOOT 失败基线。围绕 `src/main.cpp`、`src/ble_bridge.*` 和现有 NVS 存储，确定认证 bond 身份、方案容量、请求/结果边界、解绑后方案处理及旧网络兼容规则 | 小型协议与状态表落文档；命令/错误码/容量/断电提交规则明确，无隐式覆盖、跨 bond 访问或 owner 旁路。串口仅在必要现场操作时打开 |
| R1 离线 BOOT 恢复（B6/W1） | 修正 setup/loop/handleBootClick/pollWifi/serviceBleSession 与电源文档；恢复窗口与正常短会合分开 | 正常联网、断网、AP/无网络/无 Bundle、USB/电池、深睡按键均能进入物理有界 BLE 恢复；建议固定 120s，操作不无限续期；timer 不获得该窗口，长按切模板无回归 |
| R2 设备配网与绑定方案（B6/W2） | 现有加密 GATT 路由、分片及 NVS：`wifi_setup/status/cancel`、`wifi_scheme_get`、`wifi_select`；如支持网络扫描则明确能力；异步试连与结果查询 | 本绑定方案隔离，凭据先在 RAM 试连，成功提交后才报持久化；错密码、AP/DHCP 失败、通知丢失、断电不破坏旧网络；跨绑定网络 ID 拒绝，容量满须显式替换。旧 USB/AP 网络保持明确 legacy 兼容 |
| R3 设备页完整配网（B6/W3，B5/T1/T4） | `bridge/crates/ble` client、`core` 模型/service、`app/src/platform.rs`、`app/ui/index.html` 共用服务；设备页离线添加/接入、蓝牙配网、本机绑定方案增改删及已有网络选择 | 真设备页可离线接入 Note4；已有绑定正常重连，首次绑定在物理窗口内完成；状态区分接收/试连/IP/持久化/HTTP 可达。页面读设备摘要和结果，不回读密码、不依赖本地 Profile，不用不可执行的静态按钮代替功能 |
| R4 主机换网自动选择（B6/W3/W4） | macOS CoreWLAN SSID 权限/变化检测及 Windows 平台实现；设备页显示主机网络、匹配映射、设备所选网络与结果 | 主机从 5GHz SSID 映射到指定设备 2.4GHz 网络；正常自动选择只发网络 ID，合并变化并保持幂等。无权限/有线/无映射明确展示；deep 等下一会合，旧事件不回切；有效他人 owner 不抢网 |
| R5 macOS 发现与自然短会合（B5/T2/T4） | `app/src/discovery.rs` macOS ARP 回退（目前非 Windows 为 None）；BLE adapter/扫描/取消/重试按实测调整；设备业务与延后队列回归 | 精确 MAC 的 HTTP→UDP→ARP→BLE 发现恢复，无 mDNS；401/409/错 MAC 不误操作。至少先完成单次自然 timer 的身份/认证/数据或 Plan ACK/radio-off，再做 30 次筛查；不延长设备窗口掩盖问题 |
| R6 常驻、睡眠与网络恢复（B5/T5/T6） | macOS 按需 LaunchAgent 入口、单实例、启动停止流程；BLE/LAN/endpoint 重建与过期任务处理 | 按需启动不默认登录自启；正常退出不拉回，异常重启限速/熔断；关 Codex/终端仍存活，命名实例互不停止；Mac 睡眠不被阻止，唤醒、蓝牙关开、换 IP/多网卡/VPN 后恢复，不补发过期 Plan/复用旧 ACK |
| R7 界面、回归与发行准备（B5/T3/T7） | 菜单栏 Retina/深浅色/0–100/无数据/错误、关窗隐藏与原面板；独立 Bridge CI/打包/恢复文档；平台矩阵 | Finder 无 shell 环境可定位 Codex；升级/回滚保留身份/data；Intel、Apple Silicon、Windows 与 macOS 15+ 分别记录构建/实机证据；本地签名与正式签名/公证/DMG 区分，证书缺席不阻塞本地功能 |

R1/R2 的授权例外仅适用于物理恢复会话与已绑定加密网络配置；普通业务继续使用原 token/claim/owner/nonce 合同。恢复/配网不能发布 Bundle、清模板字体、重建 active context，读写命令与 claim/renew 不能续正式 light deadline。动态选择已有网络与编辑凭据的物理授权要求分别实现。

## 设备页应交付的具体行为

- “通过蓝牙配置 Wi-Fi”：目标 MAC 与绑定/连接状态、BOOT 提示、设备 2.4GHz SSID 与遮蔽密码输入、对应主机 SSID、提交/取消、分阶段结果；密码完成/取消/失败后清理。
- “本机绑定的 Wi-Fi 方案”：设备报告的本绑定无密码摘要、映射增改删、显式替换/删除确认、已选网络和设备 IP/错误。同一主机不同 Bridge 实例不伪装成不同 bond，不能读取或编辑另一主机方案。
- 自动选择状态：当前主机 SSID、对应设备网络、等下一会合/无映射/缺权限/他人占用/选网失败；设备联网与 Bridge HTTP 可达分别展示。
- 离线设备始终可进入蓝牙路径；BLE 配网能力不支持的旧固件明确提示升级或现有 USB/AP 恢复。按后端真实能力展示，不假报已有新 ROM 支持。

## 验证与未验边界

优先使用现有宿主/Fake ROM 测试覆盖分片、幂等、bond 隔离、容量、取消、网络失败、持久化、上下文/序号、owner 和电源截止期；新增测试验证外部行为，不为低影响文案编写镜像实现测试。设备页检查沿用 `tools/test-device-page.mjs`。软件模拟不能证明实际 TCC、RF、NVS 断电行为或耗电。

Note4 实机重点：BOOT 断网恢复；已有绑定无需重配；从设备页离线 BLE 配网到认证 HTTP；错误密码不丢旧配置；动态映射及请求延后；自然 timer 完整 ACK 后无线收尾。第二个真实 bond、换两组网络、Windows 对照等依可用现场逐项记录，不能拿一个真实绑定或软件模拟标记全部通过。

首版筛查沿原计划：每平台 Note4 至少 30 次自然会合，建议至少 29/30 有效回答，记录失败/P95/radio 截止；macOS 至少 24h 常驻含窗口隐藏、暂停恢复、睡眠/换网/队列重启。长时实验使用脱离工具会话的采集，不在工具前台长轮询，也不要求用户持续汇报。实际平台/设备缺席或测量时长未到，标记未验并继续其他工作；不提前宣称 Windows 对等或首版完成。宣称等功耗须另有同条件电流测量。

每次发布 Note4 前核对 MAC、target、FW marker、大小/SHA256，发布后通过认证运行镜像验证；不要仅依据版本字符串。BOOT/断网/睡眠实验前记录恢复 ROM 和数据备份，避免丢现场。已有用户授权涵盖必要实施及 Note4 验证，不为每个常规步骤重复询问；公开发布、不可逆删除或新环境实际权限提示等确需用户参与的事项，说明具体原因后再处理。

## 可复用构建与检查入口

从仓库根执行 Bridge/JS 检查，从 `bridge/` 执行 Cargo。下面命令只是已知入口，不要求交接时无条件重跑。新目录及自动生成目录遵循 AGENTS.md：预建时提权，不能先在沙箱创建再修权限；构建需要创建未知子目录时使用获准提权。使用全局工具链和既有缓存。

```sh
# 仓库根：构建原生 Bridge，脚本不构建固件；替换前按精确应用路径停旧实例并备份 data
CODEX_STATUS_ARDUINOJSON="$PWD/artifacts/bridge-macos/deps/ArduinoJson-7.0.4/src" \
  bash tools/build-bridge-macos.sh
open -n 'artifacts/bridge-macos/Codex Status Bridge.app'
node tools/test-device-page.mjs
git diff --check

# bridge/：按改动选择相关包；环境路径须使用绝对路径，避免 cwd 改变
CODEX_STATUS_ARDUINOJSON=/Users/slepher/project/codex_status/artifacts/bridge-macos/deps/ArduinoJson-7.0.4/src \
  cargo test --locked -p bridge-app -p bridge-core -p bridge-ble -p bridge-mcp -p bridge-render \
  --target-dir ../artifacts/bridge-macos/target
CODEX_STATUS_ARDUINOJSON=/Users/slepher/project/codex_status/artifacts/bridge-macos/deps/ArduinoJson-7.0.4/src \
  NO_PROXY=127.0.0.1,localhost cargo test --locked -p device-sim --test bootstrap \
  --target-dir ../artifacts/bridge-macos/sim-target -- --test-threads=1

# 仓库根：仅在需要新固件时构建 Note4，不能与其他 pio 并行
PLATFORMIO_CORE_DIR="$PWD/.pio-core" PLATFORMIO_PACKAGES_DIR="$PWD/.pio-pkgs/note4" \
  artifacts/bridge-macos/pio-env/bin/pio run -e zectrix-note4-b
```

PlatformIO 为既有 Python venv，不是隔离 Rust；首次 Mac 包准备已完成，缓存保留。上次耗时约 21m35s 是首次包/sdk 准备，不应据此例行删缓存。SDK 配置/板卡切换可触发重装，目标约束与完整 sdkconfig 快照遵守 AGENTS.md。

记录每一阶段的源码结果、适当测试、原生构建哈希、最终 ROM 路径/大小/SHA256、实际 ACK/运行镜像证明及未验项。证据放 ignored `artifacts/bridge-macos/`，不保存密码。完成事项移出 backlog，未完成事项仍只维护 B5/B6。本轮没有创建提交、PR 或开启新窗口。

## 复制到新窗口的接续提示

```text
请继续 /Users/slepher/project/codex_status 的剩余实施。
先读 AGENTS.md、PROGRESS.md 最新现场、docs/roadmap/backlog.md B5/B6，
再读 project-workflow/bridge-macos/remaining-plan.md；它是本轮接续计划。
用户已授权按计划执行，从 R0 核对现场和配网合同开始，优先推进 R1–R4。
蓝牙配 Wi-Fi 与每个 BLE 绑定独立的 Wi-Fi 方案必须在 Bridge 设备页实现；
方案/映射/密码保存在设备，Bridge 按主机当前 SSID 选择设备网络 ID，允许 5GHz→2.4GHz 不同 SSID。
当前只有 Note4；只构建 zectrix-note4-b。bridge_first 已废弃。
保留当前未提交改动与运行数据，不重做已完成的字段合同、不自动发布模板、不因已有绑定正常重连要求重配。
使用全局 Rust；按里程碑更新记录，完成必要实现和可用现场验证。
平台缺席、长时观察尚未结束等记为未验，不冒称完成，也不阻塞独立工作。不要提交代码。
```
