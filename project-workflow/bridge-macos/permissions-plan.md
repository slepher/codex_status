# macOS 蓝牙与主机 Wi-Fi 权限引导和请求时机修复

2026-10-04，用户要求解决主机SSID定位授权及每次启动蓝牙授权，不接受反复提示缺权限。

## 证据与假设

运行app的codesign designated requirement为二进制cdhash，打包脚本每次adhoc签名；本机0 valid signing identities。TCC日志确认responsible与accessing均为同app（supervisor/child），未证明同一镜像普通重启也丢授权，已询问用户。CoreLocation当前后台同步请求when-in-use后立即返回，delegate未处理授权变化。权限结果必须来自系统，不以本地标记伪报已授权。

## 实施

1. **按用户最新选择不更改钥匙串**。临时签名身份随重建变化已确认，但持久证书导入/用户级codeSign信任被auto-review拒绝（未明确授权持久安全变更），操作未执行。随后用户明确“只完成权限界面，暂不改钥匙串”；撤回签名工具与构建预检改动，保留现有ad-hoc打包。不能声称跨部署永久免授权。
2. 本地权限API只读检查CoreBluetooth/CLLocation状态，后台不创建Bluetooth manager请求未决/拒绝权限；授权按钮才触发请求。请求定位前激活应用并等待有限授权变化，禁用定位服务/拒绝/受限/未决分别反馈，拒绝后打开准确系统设置页，不重复request。读取SSID不采集或记录坐标。
3. **用户最终要求：打开app一次性引导**。首次打开界面缺权限时自动切到设备页权限区域；每进程仅引导一次，后台轮询不弹窗/不跳转。Wi-Fi页提供可直接完成的系统权限引导（授权、打开定位/蓝牙设置、重新检查），已授权后自动隐藏；启动不反复弹应用告警。只提供本功能所需设置，固定系统URL白名单；不能用界面把授权失败误报已完成。
4. 权限分支回归、JS/WKWebView窄宽截图、生产app重建/保留数据与回滚包；系统弹窗由用户选择，实机回读状态与两次同镜像重启核验。

## 边界

不自动勾选macOS隐私授权，不运行tccutil reset，不改TCC数据库；BLE设备配对与应用蓝牙权限是不同事项。此前Wi-Fi整条覆盖/折叠记录与当前网络驱动协议仅设计未实现，不夹带到权限补丁。

## 2026-10-04 验证与部署

- 已实现：公开API读取真实CoreLocation/CoreBluetooth权限；后台BLE预检，不创建未授权central；定位授权先等待NSApplication实际前台激活，delegate读取变化；未决/拒绝/受限/系统定位关闭各有行动入口。系统设置URL仅定位/蓝牙白名单，无坐标采集。
- UI启动被动检查与panel-opened检查；缺权限第一次切设备页并聚焦权限卡，本进程不会在3秒刷新或再次开窗时重复跳转。授权后最多12次×750ms只读跟进，超时仍报告未决，不能伪写授权。已授权行隐藏，两项均成功才隐藏整个权限卡。
- JS设备页回归通过；WKWebView 16场景、18截图（560×680/1000×800）通过，窄宽权限截图已审阅。合成IPC不代替实际Rust/系统弹窗交互。原生SDK编译先发现macOS无AuthorizedWhenInUse状态枚举，改为该平台AuthorizedAlways授权状态；requestWhenInUseAuthorization方法本身在本机SDK可用。
- App完整回归64 passed/0 failed/1 ignored（既有sync_v1_bridge_archive_against_note4_fake_rom需单独模拟器）；新授权门禁单测包含于64，不重复计数。最后原生请求观察器调整经过生产候选编译。
- 默认Bridge候选构建31.99s并沿用ad-hoc签名，保留旧app/私有data后部署，health204、MCP被动view正常、安装identity不变。当前exe75,817,232B，SHA25692B1BB244243D1C90D77D7BEE0E7ED131CD3A754004DEA0311F9689E8DC8CD76；supervisor39432。证据artifacts/bridge-macos/permissions-{app-tests,build}.log、permissions-{deployment,runtime}.json与wifi-ui。
- 真实回读两项均not_determined，主机permission_required，已迁移设备完整缓存保留。未触发真实授权请求、未勾选系统权限，用户从菜单栏打开界面点击两项授权后才能验证系统弹窗/SSID回读/同镜像重启；不宣称已经授权或跨部署永久免授权。
- 用户明确不更改钥匙串；相关持久签名代码已撤回，证书/私钥导入与信任命令未执行。不改变设备ROM、配网、Profile/Bundle或旧作业。

## 2026-10-12 用户修订：统一按钮与首次启动请求

用户明确要求两个权限请求按钮合并，并首次启动自动弹出系统请求，取代此前仅被动引导的产品要求。沿用原生激活等待和真实授权状态；自动只请求not_determined，已拒绝不重弹/不开设置，统一按钮进入隐私设置。定位delegate继续蓝牙请求，慢决策超过UI轮询仍可衔接。状态/软件与部署验收见[当日专项](permissions-unified-20261012.md)。不改钥匙串/签名政策/TCC数据库。
