# Bridge macOS 规划状态

新窗口实施入口：[remaining-plan.md](remaining-plan.md)，只接续 backlog B5/B6 的未完成工作；包含当前 ROM/应用证明、运行数据与已完成项、R0–R7 执行顺序和可复制接续提示。

2026-10-03 最新：用户已授权开始执行，P0/T0 正在推进。已升级系统 Rust/Cargo 至 1.99.0、修复 Rustup 入口；取消并清理此前启动的隔离工具链，只隔离构建产物。原规划仍要求首版 BLE/低功耗对等，`bridge_first` 保持废弃。

最新需求补充：蓝牙配 Wi-Fi 与本机绑定独立 Wi-Fi 方案都在 Bridge 设备页展示和操作，离线仍可进入，包含主机 SSID→设备 2.4GHz SSID 映射及设备报告的配网结果。方案与凭据保存在设备；设备页功能尚未实现，待办为 backlog B6/W3。

同日设备字段合同修复已部署：Note4 `0.18.41-note4-b-fields1` 的运行镜像哈希验证通过；当前 Mac 没有对应本地 Profile，仍可采用设备报告的 14 个字段（7 个远程字段），已有真实 HTTP applied/displayed ACK。字段表随既有认证状态返回并缓存，不新增独立长轮询。软件回归五包 225 passed / 1 ignored、Fake ROM 44/44，现场证据与最终应用哈希见 `PROGRESS.md` 最新节。这项修复不代表 BLE 配网或 macOS 首版全部验收完成。

最小 Intel/macOS 14 `.app` 已构建启动，MCP 8766 / HTTP 8765 可达；正式应用已确认 Note4 完整 MAC、绑定与加密，配网重启后仍有效。基础编译、菜单栏模板图标、CLI/数据/资源路径、安装身份、单实例锁、可选登录 plist 与扫描 ID 已适配。测试首轮 221 passed / 1 ignored，新增锁测试后 app 47 passed / 1 ignored；密码未落盘。当前经 USB legacy 配网恢复到 `192.168.10.101`，旧网络保留；按 bond Wi-Fi/BOOT 恢复、真正自然 timer 完整 ACK、按需 LaunchAgent 监督与其他平台/长时矩阵仍未完成，不标记首版验收。运行包和现场证据以 `PROGRESS.md` 最新节为准。

以下为前期规划与配对历史，保留其当时的证据边界：

已只读核对平台入口、托盘、路径、身份、发现、BLE 认证与扫描、构建和发行配置；查阅 Tauri、btleplug 与 Apple 官方资料。产出 plan/task/review 文档，未修改实现、构建、启动服务、安装后台配置或操作设备。

用户补充新环境旧 Wi-Fi 不可用与 BOOT 无法开蓝牙，新增 `wifi-setup-plan.md` / backlog B6。源码确认 WIFI OFF 单击不会开 BLE，现有 BLE 未实现写 Wi-Fi 凭据；只读 USB 枚举见 Espressif `303A:1001` / `/dev/cu.usbmodem14101`，未打开串口。用户确认仅携带 Note4，当前硬件规划范围据此收束，1.54 实机未验；设备 MAC、运行版本、按键现场未读回。

网络模型进一步确认：每个 BLE bond 有设备持久化的独立方案，允许“Bridge 5GHz SSID → 设备不同 SSID 的 2.4GHz”映射；Bridge 按当前 Wi-Fi 动态选择并发送设备网络 ID，密码不回读/落盘。规划已增加按 bond 隔离、SSID 权限、无映射、竞态、deep 延后及 owner 协调，仍未实现。

运行 Bridge、设备版本及 ROM SHA256 沿用 `PROGRESS.md` 既有记录，本轮没有新产物或新设备测量。实施状态与待办唯一来源：[backlog B5](../../docs/roadmap/backlog.md#b5-bridge-macos-菜单栏支持2026-10-03-新开)。

同日后续：用户另行授权 USB 开蓝牙并开始配对，已通过串口 `status` 读到 Note4 `0.18.40-note4-b-display1` / WIFI OFF / ota_0，执行 `pair` 获得 120 秒配对窗口确认。单次临时 macOS 诊断客户端在系统蓝牙 TCC 授权环节等待 105 秒后超时退出，尚未证明完整 MAC、bond/encryption 或重连；USB 监视已关闭，完成系统授权后需重开窗口并重试客户端。此操作仅使用已有固件命令，未实现 Bridge 或 BLE 配网功能，未构建或部署 ROM；新现场以 `PROGRESS.md` 顶节为准。

随后按用户“重试”成功：当前 Mac 已获得临时诊断应用的蓝牙权限，GATT INFO 核对 Note4 完整 MAC `7C4FADB93408`，加密 AUTH 探测后 peerBonded/peerEncrypted 均 true，主动断开重连后仍均 true，USB 也确认认证绑定。仅完成 BLE 绑定现场验证，未接通新 Wi-Fi，未验证设备重启持久性、生产 `.app` 权限身份或 device_first 短窗口；不据此标记 macOS Bridge 首版验收完成。此次仍未取 token、写业务配置或修改实现。
