# Bridge macOS 规划审阅

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
