# Bridge macOS 菜单栏支持规划

日期：2026-10-03。状态：用户已授权开始执行，当前进入 P0。待办唯一入口：[backlog B5](../../docs/roadmap/backlog.md#b5-bridge-macos-菜单栏支持2026-10-03-新开)。

## 目标与边界

用户要求首版具备 Windows 已运行的 BLE 与低功耗会合能力。复用现有 Tauri v2、Rust application service、管理 UI、MCP、同源 C++ 渲染和设备协议，在 macOS 菜单栏常驻；不新建一套桥。

**用户已确认 `bridge_first` 因时序问题废弃。** 本项只支持现行 `device_first`：设备按计划唤醒并广播，Bridge 作为 BLE central 扫描、连接、认证和交换命令。PC 广播、设备定时扫描、双策略切换及其时序恢复均不纳入计划，也不留成后续阶段。

首版范围包括余量图标、现有菜单与管理窗口、Codex 数据源、Wi-Fi HTTP、MCP、双设备管理、BLE 首次绑定/取 token/endpoint 交接、自然 timer 会合、正式 PowerPlan、状态/数据/诊断同步、延后 Bundle 与 OTA。保存与显式发布、claim/lease、MAC 身份、ACK 确认规则保持现行合同。普通会合不增加全量模板传输。

用户后续补充：设备换到新环境，旧 Wi-Fi 不可用，因此首版接入增加[离线 BLE 配 Wi-Fi 与 BOOT 恢复入口](wifi-setup-plan.md)（backlog B6），需要固件与 Windows/macOS 共用 Bridge 服务共同设计；这是托盘平台适配之外的独立前置。当前环境只携带 Note4，1.54 不在现场：本轮硬件验证范围为 Note4，双设备路由仍可做软件回归；1.54 的真实 BLE/功耗另标未验，不要求本轮取回该设备。

最初仅获规划授权；用户后续已明确“按照计划开始执行”，并提供当前环境网络配置。首先执行 P0 的本机工具链升级、最小 macOS 编译与原生 `.app`/BLE 验证；Wi-Fi 配网按 W0–W4 推进。当前现场只有 Note4，不构建或操作缺席的 1.54；不创建 PR 或提交。用户明确要求升级本机 Rust，工具链使用现有系统路径，构建产物仍使用独立 target 以保护运行 data。

## 已核实的代码边界

| 位置 | 现状 | 规划改动 |
|---|---|---|
| `app/Cargo.toml`、`app/src/autostart.rs` | Tauri 已启用 tray；autostart 无条件导入 Windows 专用 API，阻塞 macOS 编译 | 对 Windows 依赖/实现做条件编译；补 macOS 启动实现 |
| `app/src/main.rs`、`icon.rs` | 窗口默认隐藏、关闭时隐藏；32px 彩色数字图标；路径/URL 打开调用 explorer/rundll32；非 Windows 时间为 `--:--` | 复用窗口和菜单，补菜单栏尺寸、系统外观、`open`、本地时间 |
| `app/src/instance.rs`、`watchdog.rs` | 单实例仅 Windows；macOS watchdog 不工作；命名实例强制写 `<exe>/instances` | 按实例加进程锁；macOS 生命周期交给用户会话 LaunchAgent；路径独立适配 |
| `core/src/paths.rs` | 默认 `<exe>/data`、`<exe>/seed` | 为 `.app` 设计外部可写 data 与只读 Resources seed；保留显式路径覆盖 |
| `core/src/codex.rs` | CLI 候选主要是 Windows 安装路径；末尾也尝试 `codex` | 增加 macOS 绝对路径候选，保持版本验证和失效重发现 |
| `app/src/main.rs` 的 `host_label` | 只读 COMPUTERNAME/HOSTNAME，缺失时统一退到 `bridge` | macOS 持久稳定安装身份，避免 Finder/LaunchAgent 环境导致 owner ID 冲突 |
| `app/src/discovery.rs` | 非 Windows ARP 分支直接返回 None | 增加 macOS 邻居表与有界探测；保留 HTTP→UDP→ARP→手动 BLE，无 mDNS |
| `ble/src/lib.rs` | 已按广告名称筛选、GATT 完整 Wi-Fi MAC 授权；但广告去重使用 `peripheral.address()` | 去重改 opaque peripheral ID；Wi-Fi MAC 仍是业务主键 |
| 同上 `read_info`、`request_device_token` | 未 bonded 在读取 info 后直接拒绝，并提示 Windows 设置；取 token 前要求 encrypted+bonded | 独立首次配对入口；原业务安全闸门保持，不把未绑定发现等同授权 |
| 同上 `Pusher::adapter` | 每轮创建 adapter 的理由来自 Windows watcher 行为 | 实测 macOS adapter 生命周期与事件恢复，再决定是否仅在 macOS 复用 |
| `ble/examples`、`ble/Cargo.toml` | Windows 广播实验程序和 dev dependency | 限定 Windows 示例编译，不为废弃策略移植 Publisher |
| `render/build.rs` | Bridge 需要 ArduinoJson 头文件；依赖同源 C++ | 独立提供固定版本头文件，不为构建 Bridge 编译固件 |
| `.github/workflows/release.yml` | 现有 release 与固件任务绑定，且含旧目标/旧 profiles seed 路径 | 新增独立 Bridge 检查/打包路径，不触发现有固件 release |

源码锁定 Tauri `2.11.5`、btleplug `0.11.8`；不把升级依赖作为默认前提。当前规划会话进程为 macOS `14.8.9` / `x86_64`；尚无 Apple Silicon 或 macOS 15+ 实测证据，也未做编译检查。

2026-10-03 本机只读硬件核查：型号 `MacBookPro15,2`，内建蓝牙控制器 `BCM_4364B0`，UART，固件 `v122 c4439`，蓝牙已开启，系统支持服务包含 GATT。该型号对应的 2018/2019 四 Thunderbolt 端口 13 英寸机型均由 Apple 标注 **Bluetooth 5.0**（[2018 规格](https://support.apple.com/en-us/111925)、[2019 规格](https://support.apple.com/en-us/111997)）。标准版本依据官方型号规格；本机 system_profiler/控制器 IORegistry 本次未显示 HCI/LMP 版本或逐项 LE feature。没有扫描、配对、连接外设，因此不能据此证明真实会合时序或可用吞吐。

## 建议的最小设计

### 1. 菜单栏与窗口

使用 Tauri `ActivationPolicy::Accessory`，常驻时不显示 Dock 图标；点击菜单栏打开操作菜单，通过“打开面板”聚焦原窗口。关闭窗口继续服务，退出菜单结束服务。沿用余量数字及 Ok/Stale/Error/NoData 语义。

首版先保留彩色状态图标，针对菜单栏生成合适尺寸并验证 Retina、浅色/深色、三位数。不能直接把彩色实底图标标为 template image：系统着色会丢失颜色语义。菜单同时以文字表达状态与暂停状态。采用 Tauri 已有 tray/menu API。[Tauri 托盘文档](https://v2.tauri.app/learn/system-tray/)、[ActivationPolicy](https://docs.rs/tauri-runtime/latest/x86_64-apple-darwin/tauri_runtime/enum.ActivationPolicy.html)。

### 2. 应用包、路径与身份

采用 `.app`，通过 `tauri.macos.conf.json` 覆盖 macOS 打包配置；不改变 Windows 默认包。seed 放 `Contents/Resources/seed/templates`，运行数据建议放 `~/Library/Application Support/dev.codexstatus.bridge/data`；命名实例放同一根下 `instances/<name>/data`。显式 `CODEX_STATUS_DATA` 仍优先，不向 `.app` 内写数据。

这是**拟新增的 macOS 路径约定**，与项目现有 `<exe>/data` 默认不同；实施时须同步 AGENTS.md、路径说明与恢复文档。本次不改变现行约定。开发时也须明确 data 路径，避免误落既有 Windows `target/debug/data`。首次 seed 只补缺失文件，更新 `.app` 不覆盖模板、Profile、tokens、jobs 和 contexts。

macOS 首次运行持久化安装身份，并按命名实例派生 `bridge_id`；进程重启、应用升级和睡眠恢复不改变 owner ID。跨机器复制数据的恢复规则须区分业务配置和本机 Bridge 身份；不直接沿用 Windows 的 owner 身份。显示名单独取系统主机名或用户配置。

Finder 和 LaunchAgent 不保证继承 shell PATH。CLI 顺序建议为明确配置/环境覆盖、PATH 中的 `codex`、`~/.local/bin/codex`、`/opt/homebrew/bin/codex`、`/usr/local/bin/codex`，每项保持 `--version` 校验。其他安装位置由显式配置解决；不读取用户 shell 配置或随意扫描 `.app`。资源定位和 PATH 问题见 [Tauri macOS 应用包说明](https://v2.tauri.app/distribute/macos-application-bundle/)。

### 3. BLE 权限、首次绑定与身份

`.app` 必须声明 `NSBluetoothAlwaysUsageDescription`；macOS 15+ 的局域网路径声明 `NSLocalNetworkUsageDescription`，实测许可、拒绝、后续恢复及 launchd 启动时的权限归属。终端被授权不代表应用包已授权。[btleplug macOS 权限说明](https://github.com/deviceplug/btleplug)、[Apple 蓝牙用途声明](https://developer.apple.com/documentation/bundleresources/information-property-list/nsbluetoothalwaysusagedescription)、[Apple 局域网隐私说明](https://developer.apple.com/documentation/technotes/tn3179-understanding-local-network-privacy)。

用 `peripheral.id()` 去重和关联扫描对象；广告名称只是候选，读 GATT info 的完整 Wi-Fi MAC 才核对目标。上游 CoreBluetooth 实现的 `address()` 返回默认地址，现有地址去重会有合并候选的风险；P0 需核对锁定版本和两设备现场。[btleplug CoreBluetooth 实现](https://github.com/deviceplug/btleplug/blob/master/src/corebluetooth/peripheral.rs)。固件当前可能只在 scan response 给名称、不带 service UUID，不能直接加 service-only ScanFilter。

首次绑定是独立交互流程：设备物理打开 120s pairing window → 扫描连接 → 读取候选身份并核对 MAC → 经现有需要加密的特征触发系统配对 → 重新核对 encrypted/bonded 和身份 → 请求 token。具体触发方式由 P0 验证，不能预先承诺 macOS 设置页能手工配对该 BLE 外设。为发现拆开的 info 读取不得解除业务认证要求；固件仍须在绑定且加密后才响应敏感命令。配对关闭/取消、未绑定、MAC 不符均拒绝 token 和业务操作，token 不进入日志。

### 4. `device_first` 与低功耗

复用 `platform::ble_cycle`、现行 coordinator、每 MAC 调度/冷却、分片写入、ACK、Wi-Fi open 与同步路径。macOS 后端的 adapter 建立、scan restart、重复广播、连接重试和任务取消需实测；不复制 Windows 专属优化理由，也不以延长固件窗口解决移植问题。

源码通常是未连接 3s、启用 B46 时 4s，连接命令期限 6s，成功 ACK 后 200ms 收尾；**测试基线以登记设备实际运行 ROM 为准**，日志旧注释或实验 ROM 参数不能替代现场。记录设备广播起点、主机首包/连接/发现/认证/ACK、设备 radio-off，以及完整失败轮次；跨端时间以 wake identity 和可关联事件为准，不能直接相减未校准的单调时钟。

只有正式 PowerPlan 更新 light deadline；发现、读状态、传输、claim 和 renew 不延长。push/pull 与成功 ACK 指纹提交保持现行语义。测试 BOOT provisional、timer wake、暂停/恢复、Wi-Fi 失联、双设备轮换、队列重启恢复及强制 sleep。BLE permission denied/蓝牙关闭采用有界退避，不能 250ms 打错误日志。

Mac 系统睡眠期间不提供实时会合，也不为 Bridge 阻止系统睡眠；设备继续按已有计划执行。唤醒后重建必要的 BLE 状态、刷新 LAN 地址/endpoint、恢复发现与队列，过期窗口和旧 ACK 不算新成功。App Nap/窗口隐藏下的会合稳定性列为真实验证项。设备耗电收益需要整周期电量测量，不用连接时间或编译成功代替。

### 5. 启动、单实例与恢复

建议用用户 GUI 会话 LaunchAgent 管理 macOS 常驻生命周期，不移植 Windows watchdog API，也不拆一个无 UI daemon。默认按需启动、登录自启需用户主动打开。[Apple LaunchAgent 说明](https://developer.apple.com/library/archive/documentation/MacOSX/Conceptual/BPSystemStartup/Chapters/CreatingLaunchdJobs.html)。

默认按需配置保存在 Application Support，显式 bootstrap/kickstart 到用户 GUI 会话；开启登录自启才注册 `~/Library/LaunchAgents`。Finder 启动入口、启动脚本与受 launchd 管理的入口必须有明确区分，避免递归拉起与双实例。按实例加进程持有的文件锁，覆盖直接启动与 managed 启动；命名实例保留不同端口、不监听固定 UDP 8767。

正常“退出”不被自动拉回；崩溃重启应有限速/熔断；实施前明确 launchd 配置与正常退出语义，不能使用无条件 KeepAlive。关闭 Codex/终端后继续运行是独立验收项。升级前只停目标路径/实例，备份 data，不终止别的 Bridge；升级/回滚只替换应用包。

### 6. 发现、构建与发行

ARP 建议先复用系统 `/usr/sbin/arp` 邻居表，冷表用有界并发连接设备 HTTP 端口触发邻居解析，仍按真实子网限制扫描；HTTP 再核对设备 MAC。VPN/多网卡、换网、同 IP 不同设备须覆盖，不使用原始 socket 或 root 权限，也不恢复 mDNS。

规划分别提供 Intel `x86_64-apple-darwin` 与 Apple Silicon `aarch64-apple-darwin` 的 `.app`；先以当前 Intel 环境验证，另一架构必须另有构建与 BLE 实机证据后才宣称支持。最低系统版本在 P0 按锁定依赖及实际测试确定；至少覆盖当前 macOS 14 与有局域网隐私机制的 15+。先产出本地测试包，再处理公开分发的签名、公证、DMG；正式发行取决于证书可用性，不影响本轮规划。[Tauri 签名与公证](https://v2.tauri.app/distribute/sign/macos/)。

Bridge CI 单独准备固定版本 ArduinoJson、运行 Rust/现有 JS 检查并打包，使用隔离 target；不调用固件 `pio run`，不夹带旧 profiles.json/seed。现有固件 release 的旧配置不在本项顺带重写。

## 实施顺序与交付门槛

| 阶段 | 交付物 | 进入下一阶段的条件 |
|---|---|---|
| P0：编译与 BLE 可行性 | 最小 `.app`、真实权限/首次绑定/双设备 GATT 证据；固定运行 ROM 基线 | 不绕过安全门槛、至少单次自然 timer 会合完成；配对或短窗口失败先定位，不能以 Wi-Fi-only 宣称首版完成 |
| P1：菜单栏与本机运行 | 原 UI、图标/菜单、CLI、data/seed/identity、单实例 | Finder 无 shell 环境可运行；重启/升级模拟保留身份与运行状态 |
| P2：设备业务对等 | BLE 与 HTTP/MCP 共用原服务；发现、Plan、同步、队列；关联 B6 的离线接入 | Note4 实机与双 MAC 软件路由验证，不误 claim、不隐式发布、不延长电源截止期；1.54 实机未验 |
| P3：常驻与恢复 | 按需 LaunchAgent、可选登录自启、正常退出/崩溃/睡眠恢复 | 关闭 Codex/终端仍运行；退出来回行为正确；睡眠后恢复自然会合 |
| P4：回归与发行准备 | 两架构构建结果、Windows 回归、实机矩阵、应用包和恢复文档 | 满足 task.md 验收，证据不足则保留未验状态；公开签名/公证另标发行条件 |

阶段可以内部逐步实现，但 **P0–P4 的必需能力与验收全部满足后才叫“首版支持 macOS”**。拟先安排 1–2 个工程日完成 P0；通过后再估 P1–P4。配对、CoreBluetooth 重连与权限归属尚未实测，不承诺总工期。任务拆分见 [task.md](task.md)，规划审阅见 [review.md](review.md)。
