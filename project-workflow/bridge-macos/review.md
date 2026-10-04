# Bridge macOS 规划审阅

2026-10-04 当前补齐：Wi-Fi整条覆盖、执行历史折叠、Bridge已保存当前SSID驱动与单条BLE连接反馈已实现并配套部署（Note4 .52）。运行镜像及Bridge健康/缓存核验通过，UI24场景及两端回归通过；系统两权限未授予，真实离线/改网/故障矩阵和变长资源仓库仍开放。当前产物/哈希以PROGRESS顶节为准；下面“未部署”均为历史阶段。

2026-10-03 W7已按用户追加授权完成：Bridge+Note4 ROM一并部署，运行.45/wifi_management=2的镜像SHA256已核对。A8同时修复remaining默认分类与新context旧确认指纹，自动HTTP seq16 applied/displayed、源/最后写屏缓冲89一致。产物/备份/回归及USB plugged=false验证边界见PROGRESS顶节、usb-sync-diagnosis.md与wifi-rendezvous-execution.md §12；B5/B6真实配置/RF/TCC/功耗与USB仍开放。下文旧未部署记录保留为此前历史。


2026-10-03 W0–W6本批软件更新（未部署）：新版完整Wi-Fi投影/事务、三份非秘密状态与持久任务、实际机会调度、字段冲突/待确认、生产Wi-Fi/history Fake HAL和完整双侧UI已实施。五crate239 passed/1 ignored、Fake48 passed（合计287/1 ignored），host3入口与设备页JS通过；最后app51/1 ignored及Fake Wi-Fi3项复测通过。逐阶段/逐A编号证据及边界见 [wifi-rendezvous-execution.md第11节](wifi-rendezvous-execution.md#11-w0w6-软件交付与验收对账2026-10-03未部署)。下文旧“尚未实施”记录保留为当时历史。

自审收紧：预览local_revision原子校验；部分字段不扩大，新项全元数据批准；凭据操作单独选择/内存输入；unknown不能证明不可读密码；明确rejected结束；稳定ID丢ACK先query+readback；大型设备存储临时副本移至有界heap。可选授权后置Data/Plan且250ms上限，配置借合法Wi-Fi8s预算，设备锁及OTA/发布优先。v1迁移/损坏/账本容量/授权期限有宿主断言；旧宿主夹具临时字符串改为拥有副本，消除悬空ID造成的帧断言假失败。

W7未执行，实际仍.44/原运行.app；没有新ROM/App产物、运行应用重启、刷机、串口、真实网络修改或提交。账本8项永久保留无自动清理，满后明拒；整板链接/实板RAM/RF、原屏幕跳变、持续重连/INFO超时、TCC/功耗、Windows/其他架构/1.54及完整app事件/抢锁压力仍未验。一般诊断采集持久化只保留既有入口，不宣称全部改造。B5/B6仍开放，配额延迟调查/backlog A8另列。

2026-10-03 用户追加刷机已完成：Note4 `0.18.44-note4-b-ble1` / MAC7C4FADB93408，USB整段读回与认证运行前缀SHA256均为 `56367D6BDBC056B597DE354B05B47320404A2712CBC4F631532A512D1A81D0C8`；配套Bridge已升级，运行数据/旧应用备份保留。真实已绑定链路status/plan/sync_open ACK成功一次，但后续connect/INFO read仍超时，完整会合问题未结案。P3持久恢复及P2/P4余项不因刷机而结项；当前现场与证据以PROGRESS顶节为准。

首批自审边界：生产输出只在共享C++构建器编码一次，Fake存原始字节；故障注入的635 B是同长度合成帧，不是原实机抓帧。最终信封参与容量计算，历史仅裁完整记录；执行后超限保留unknown语义。固定deadline在写入前建立，错wake不再仅警告后接受，重读不会续时。稳定ID尚无冻结作业/重启恢复保证，旧Wi-Fi UI首次失败结果流程未在本批改为可恢复作业。独立代理评审、完整SDK编译与硬件确认本轮未做。

2026-10-03 BLE 真实性首批软件交付：P0/P1 核心与 P2 原始属性/真实客户端链已实现，合同与边界见 [fake-rom-ble-fidelity-plan.md](fake-rom-ble-fidelity-plan.md) 第11节及 [wifi-protocol.md](wifi-protocol.md)。六 crate 回归合计278 passed / 1 ignored；实际 Wi-Fi HAL 最长最终关联帧418 B。P3仅新增稳定ID入口，持久读/同步作业恢复尚未实现；P2 Wi-Fi/history HAL与跨 bond/session/分页矩阵、P4完整放行仍待补。本轮未整板构建或部署、未操作设备/运行.app，不能将源码候选当成在机能力；B5/B6保持未结项。

最新设计入口（2026-10-03）：用户要求重新设计 Wi-Fi 全量方案管理与同步，见 [wifi-redesign.md](wifi-redesign.md)。当前 protocol=1 实现与下方旧规划不等于新设计已完成；新稿覆盖单方案管理/试连即保存启用的旧交互。Note4 试连跳屏异常待诊断，最新产物/未部署改动见 PROGRESS 顶节。


2026-10-03 实施更新：R0 合同已定稿于 [wifi-protocol.md](wifi-protocol.md)，R1–R4 与 R5 ARP/R6 按需监督已实现并通过本机软件回归；Note4 `0.18.42-note4-b-wifi1` 运行镜像已认证核对。以下规划阶段的旧状态仅保留背景，当前现场以 PROGRESS 顶节为准。尚缺真实离线/密码试连/换网、自然会合与常驻生命周期矩阵，B5/B6 不结项。


2026-10-03 后续实施自审：设备字段合同修复已通过无本地模板/Profile、同名草稿不同字段、坏合同拒绝、上下文切换缓存失效与重启许可/序号恢复测试，并已在 Note4 新 ROM 上取得 image_verified 和真实 HTTP applied/displayed ACK。运行字段只来自设备，数据源和 push/pull 仍归 Bridge；字段表随既有认证状态返回，不增加独立长轮询。未自动修改许可或发布 Bundle。完整 macOS BLE/低功耗对等、按 bond BLE 配网和 BOOT 离线恢复仍未验收，不能以本项结果标记首版完成。

以下为实施前的规划审阅：

2026-10-03，基于源码与官方文档的静态自审；未进行独立代理评审、编译或实机试验。

- 范围已服从用户纠正：本轮仅文档规划；首版完整 BLE/低功耗对等；`bridge_first` 废弃，不保留 Publisher 或双策略待办。
- 复用现有 Tauri 与 application service，无新业务桥、无跨平台框架重写；平台分支集中在已有路径/生命周期/发现/BLE 边界。
- 首次绑定是 P0 门槛：现有 read_info 先拒绝 unbonded，不能靠改错误提示解决；探索系统配对触发时仍保持 MAC、bond、encryption 和固件 token 安全检查。
- 多设备去重采用 peripheral ID，但业务身份仍是完整 Wi-Fi MAC；不能用 CoreBluetooth UUID、默认 BLE 地址或名称后缀代替。
- 路径、主机身份和后台启动必须一起收束：只加菜单栏会留下 `.app` 写入、PATH、owner 冲突、双实例和生命周期缺口。macOS 外部 data 是拟新增约定，尚未更改 AGENTS.md 的现行规则。
- 会合参数以实机 ROM 为准；不把 4s/6s、源码默认 3s 或旧 5s/9s 实验混为同一版本。完成 ACK、保持 radio 截止期和真实耗电分开举证。
- 真 `.app`、不同硬件目标、Windows 对照、不同 CPU 架构与 macOS 15+ 权限须分别记录；用户确认当前现场只有 Note4，1.54 单列未验，当前 Intel/macOS 14/Note4 现场不能证明所有支持矩阵。
- 新环境接入需求增加 B6：离线 BLE Wi-Fi 配网和不依赖 Wi-Fi 的物理 BOOT 蓝牙恢复入口。代码确认原 WIFI OFF 分支不开 BLE，startup/AP/断网收尾也会影响入口；静态分析不冒称实机复现。单独移除 BOOT guard 无法完成新行为。
- 最主要的不确定性是 CoreBluetooth 配对与短窗口重连、LaunchAgent 权限归属和主机后台调度。P0 先消除这些风险，之后再估总工期；不能退成 Wi-Fi-only 并称首版完成。
