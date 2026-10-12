# 合并权限入口与首次启动请求

关联 B5。用户要求蓝牙/定位两个请求按钮合并为一个，并在首次启动时自动弹出系统授权。

## Plan / Task

- 共用一个 UI 入口；未决定权限由原生回调依次请求定位/蓝牙，慢于 UI 轮询仍继续第二项。
- 首次加载自动触发一次，仅请求系统未决定权限；已允许/拒绝/受限不自动重弹或打开设置。
- 拒绝/禁用时统一进入隐私设置，保留两项说明和真实状态；系统弹窗仍由 macOS 分别控制。
- Node 行为回归、原生 WK 窄/宽截图审阅；重建默认 Bridge 前核对实例、私有备份，部署保留数据/身份。

## Status

初始：UI 为两个按钮，startup/panel-opened 被动检查；原生定位请求等待实际应用激活。运行App为读取修复最终版 d1fd0fda…58b1。已有工作区 RF 修复和其他文档保留。复用公开权限API，不修改钥匙串、TCC、固件或设备方案。

## Review

仅未决定权限自动请求；系统拒绝不能伪报成功。首次自动请求按系统授权状态判定，新签名重置后也自动补申请；同一进程打开面板/后台轮询不重复请求。测试合成IPC不代替原生系统授权验收。

## 实施与软件验证

UI主动作改为“授权定位与蓝牙”，拒绝/系统定位关闭改为“打开权限设置”；保留两项状态与重新检查。startup通过wifiInitPermissions仅一次请求未决定权限，panel-opened仍被动检查。原生combinedRequestPending与定位delegate衔接蓝牙，超出12×750ms UI轮询后仍继续，重复请求由pending/manager状态抑制。permission_all/settings_permissions复用原platform_wifi入口，旧单项操作仍兼容。

Node生产HTML行为回归通过；App85 passed/3 ignored。WK初跑发现两处既有旧标签（来源分组、连接确认）断言，与当前生产语义不符，已更新；测试权限分发改用既有__wifiRoundtripInvoke，真实按钮/初始化函数合成IPC覆盖通过。WK52场景通过、窄/宽已审阅；合成快照复原的成功文字已清除，最终文案复跑52场景通过，窄/宽权限截图审阅无裁切/溢出。不能以WK证明真实系统弹窗已允许。

11:04:02 默认实例路径/旧hash d1fd0fda…58b1 核对后停止，所有同路径进程退出；私有data/installation-id备份读回哈希一致。release构建中，部署记录 artifacts/bridge-macos/permissions-unified-20261012-deployment.json；运行数据/回滚保留。

## 最终部署与审查

11:08:29 release部署成功（3m18s），x86_64/20,804,352 B，SHA256 `53a5fab4b26ea31cee2ac283f6cf4308d761b8d2ef6a066d1f30b36f2c0a9b63`，候选/默认App一致，严格签名通过。supervisor11717/child11724、health204/MCP43工具，安装身份一致，停桥期间state哈希未变。私有数据备份与回滚包位置见 artifacts/bridge-macos/permissions-unified-20261012-deployment.json，构建日志 permissions-unified-20261012-build.log。

11:08:42 MCP被动核验蓝牙/定位均allowed、主机Xiaomi_1EBB/connected，自动网络状态connected；无需再次要求用户点击两个旧按钮。未人工撤销/reset权限，因此本次未决定时真实两项系统弹窗次序未单独人为重现；软件回归覆盖自动调用/顺序衔接与受限/拒绝边界，原生代码编译通过。系统仍分别显示两项对话框；不将统一入口称为合并macOS系统弹窗。

Node、App85/3条件忽略、WK52最终通过，窄宽截图已审阅，git diff --check通过。当前运行已包含原RF读取修复及本次UI/原生权限变更；未提交、未刷ROM/发布Bundle/修改钥匙串或TCC。
