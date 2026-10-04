# Bridge macOS 规划状态

2026-10-04 当前补齐：Wi-Fi整条覆盖、执行历史折叠、Bridge已保存当前SSID驱动与单条BLE连接反馈已实现并配套部署（Note4 .52）。运行镜像及Bridge健康/缓存核验通过，UI24场景及两端回归通过；系统两权限未授予，真实离线/改网/故障矩阵和变长资源仓库仍开放。当前产物/哈希以PROGRESS顶节为准；下面“未部署”均为历史阶段。

2026-10-04 最新：会合协议模块及Astra A1–A6、关联A7/A8源码候选已实施，310项Rust回归及C++专测通过，Note4 .46候选构建成功；未部署/烧录，真实仍.45、生产Bridge停机。逐项证据与现场缺口见 [rendezvous-protocol-review.md](rendezvous-protocol-review.md)，执行计划见 [rendezvous-protocol-plan.md](rendezvous-protocol-plan.md)，待办归backlog B6。下文保留此前里程碑状态。

2026-10-03 W7已按用户追加授权完成：Bridge+Note4 ROM一并部署，运行.45/wifi_management=2的镜像SHA256已核对。A8同时修复remaining默认分类与新context旧确认指纹，自动HTTP seq16 applied/displayed、源/最后写屏缓冲89一致。产物/备份/回归及USB plugged=false验证边界见PROGRESS顶节、usb-sync-diagnosis.md与wifi-rendezvous-execution.md §12；B5/B6真实配置/RF/TCC/功耗与USB仍开放。下文旧未部署记录保留为此前历史。


2026-10-03 W0–W6本批软件更新（未部署）：新版完整Wi-Fi投影/事务、三份非秘密状态与持久任务、实际机会调度、字段冲突/待确认、生产Wi-Fi/history Fake HAL和完整双侧UI已实施。五crate239 passed/1 ignored、Fake48 passed（合计287/1 ignored），host3入口与设备页JS通过；最后app51/1 ignored及Fake Wi-Fi3项复测通过。逐阶段/逐A编号证据及边界见 [wifi-rendezvous-execution.md第11节](wifi-rendezvous-execution.md#11-w0w6-软件交付与验收对账2026-10-03未部署)。下文旧“尚未实施”记录保留为当时历史。

自审收紧：预览local_revision原子校验；部分字段不扩大，新项全元数据批准；凭据操作单独选择/内存输入；unknown不能证明不可读密码；明确rejected结束；稳定ID丢ACK先query+readback；大型设备存储临时副本移至有界heap。可选授权后置Data/Plan且250ms上限，配置借合法Wi-Fi8s预算，设备锁及OTA/发布优先。v1迁移/损坏/账本容量/授权期限有宿主断言；旧宿主夹具临时字符串改为拥有副本，消除悬空ID造成的帧断言假失败。

W7未执行，实际仍.44/原运行.app；没有新ROM/App产物、运行应用重启、刷机、串口、真实网络修改或提交。账本8项永久保留无自动清理，满后明拒；整板链接/实板RAM/RF、原屏幕跳变、持续重连/INFO超时、TCC/功耗、Windows/其他架构/1.54及完整app事件/抢锁压力仍未验。一般诊断采集持久化只保留既有入口，不宣称全部改造。B5/B6仍开放，配额延迟调查/backlog A8另列。

2026-10-03 用户追加刷机已完成：Note4 `0.18.44-note4-b-ble1` / MAC7C4FADB93408，USB整段读回与认证运行前缀SHA256均为 `56367D6BDBC056B597DE354B05B47320404A2712CBC4F631532A512D1A81D0C8`；配套Bridge已升级，运行数据/旧应用备份保留。真实已绑定链路status/plan/sync_open ACK成功一次，但后续connect/INFO read仍超时，完整会合问题未结案。P3持久恢复及P2/P4余项不因刷机而结项；当前现场与证据以PROGRESS顶节为准。

2026-10-03 BLE 真实性首批软件交付：P0/P1 核心与 P2 原始属性/真实客户端链已实现，合同与边界见 [fake-rom-ble-fidelity-plan.md](fake-rom-ble-fidelity-plan.md) 第11节及 [wifi-protocol.md](wifi-protocol.md)。六 crate 回归合计278 passed / 1 ignored；实际 Wi-Fi HAL 最长最终关联帧418 B。P3仅新增稳定ID入口，持久读/同步作业恢复尚未实现；P2 Wi-Fi/history HAL与跨 bond/session/分页矩阵、P4完整放行仍待补。本轮未整板构建或部署、未操作设备/运行.app，不能将源码候选当成在机能力；B5/B6保持未结项。

最新设计入口（2026-10-03）：用户要求重新设计 Wi-Fi 全量方案管理与同步，见 [wifi-redesign.md](wifi-redesign.md)。当前 protocol=1 实现与下方旧规划不等于新设计已完成；新稿覆盖单方案管理/试连即保存启用的旧交互。Note4 试连跳屏异常待诊断，最新产物/未部署改动见 PROGRESS 顶节。


2026-10-03 实施更新：R0 合同已定稿于 [wifi-protocol.md](wifi-protocol.md)，R1–R4 与 R5 ARP/R6 按需监督已实现并通过本机软件回归；Note4 `0.18.42-note4-b-wifi1` 运行镜像已认证核对。以下规划阶段的旧状态仅保留背景，当前现场以 PROGRESS 顶节为准。尚缺真实离线/密码试连/换网、自然会合与常驻生命周期矩阵，B5/B6 不结项。


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
