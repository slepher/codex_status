# Status

2026-10-27（工作开始于10-26）：共享 Wi-Fi 族源码与离线验收完成，Windows默认Bridge已部署并重启。完整备份旧exe/data后替换，设备网络/ROM未改；恢复原有自动服务，未创建Wi-Fi族或更改订阅。

## 已实现

- Bridge 共享族库（schema1、revision、稳定ID、默认5G SSID族名、2.4G/5G及同名双频），每设备独立选择多个族；库与设备目标原子落盘，错误回滚。明确订阅可接管完全相同的旧映射，其余旧记录保留/冲突明拒。
- Note/BW 复用 host_ssid 字段表达族名，设备 JSON/WFS4/WTX1 不变。主机任一族内SSID解析同族，无Wi-Fi时使用指定族；权限未知/歧义/未知SSID不猜测。设备只有订阅族才参与自动选择；空族库保留旧行为。
- 共享族修改投影到各订阅设备的 Bridge 目标，不直接写设备；继续读取/比较/显式同步。已有族的设备快照导入更新共享库及其他订阅目标；未知条目可明确选择目标族导入。不捏造AP01密码。
- AP01沿用18080菜单八位码GET/PUT网络列表，原厂旋钮手选不变。保留未选择列表条目、MAC/revision/容量检查，代码不保存设备码。先持久未知围栏再PUT，确定未发送立即结束；可能发送后只允许读回。人工接受读回列表可解除阻挡，但原保存及密码仍标未知，不冒充成功，不重发原保存。
- UI提供共享族编辑/密码显隐、每设备订阅、有线族、明确设备导入；MCP共用服务且递归拒绝/剔除密码和设备码，省略凭据的MCP编辑保留现有密码。

## 验证与边界

- Rust core/app/mcp全量337通过/5条件忽略；最终两处旧记录隔离/接管调整后，Wi-Fi定向56通过/2条件忽略。证据：`artifacts/wifi-families/rust-tests-final.log`、`rust-tests-scope-final.log`。
- 隔离Windows候选：`D:\Documents\PlatformIO\Projects\codex_status\bridge\target-note4-sync\debug\bridge-app.exe`，39009280 B，SHA256 `9077bb8cb338034291612fff5280c95ad6c5be456a2f7f0b4507c12280311cf1`。`cargo build -p bridge-app --target-dir target-note4-sync`通过；`build.log`及`candidate.json`登记，后续已部署到默认Windows实例。构建仅有既有btleplug生命周期警告。
- Node生产页面回归通过：`artifacts/wifi-families/node-tests.log`。
- Edge生产HTML合成IPC：560/1000宽度 × 原Bridge/设备/比较及族编辑/有线/AP01共12场景通过，容器边界/无横向溢出断言通过；族编辑、AP01手选两尺寸截图已人工审阅。见`artifacts/wifi-families/edge-results.json`及PNG。
- 必需的`bash tools/test-wifi-ui.sh`已尝试，Windows/WSL报`swiftc: command not found`。已把族场景加入既有Swift脚本；macOS WKWebView原生验收未完成。Edge不是Rust IPC、RF、设备实测或macOS验收。
- 当前三个已知固件目标的兼容网络选2.4G；5G-only族不可订阅给它们；未知固件目标拒绝推断。AP01最多8族/网络，Note/BW最多12；共享库最多64族。
- 固件/嵌入式资源、协议与存储新增成本0；全部新数据在Bridge，动态按实际条目分配。实机切网/长期RF与macOS原生验收及部署统一归B6。


## Windows 部署（2026-10-27）

用户要求重启后，核对默认exe/8765及8766/计划任务，先停同exe watchdog，再停CodexStatusBridge及核实残留主进程。完整备份286个data文件并逐文件SHA256验证、保留旧exe；新exe39009280 B及SHA256与隔离候选一致。随后由提权`tools/start-bridge.ps1`按需任务启动，主PID14720。未改设备ROM/网络、Profile或业务许可。

HTTP health204、MCP44工具、library_select可见；生产MCP读取Wi-Fi族schema1/空库成功，未配置族时沿用旧网络行为。重启前后3设备登记/Profile/许可一致，bridge-app.json字节一致。后续运行状态按正常服务更新，不声称全部动态state冻结。证据`artifacts/wifi-families/deployment.json`、`health.json`；私有备份路径记录在deployment.json。回滚旧exe时须配套恢复停机备份data，避免旧程序拒绝新库字段。
