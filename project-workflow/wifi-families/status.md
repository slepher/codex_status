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


## 已保存网络合并（2026-10-27）

用户要求合并既有网络并自动分配频段。读取当前Bridge目标与完整设备缓存，去重为3族；2族有不同2.4G/5G SSID，1族只有已证实2.4G记录，未推断其5G。一个主机SSID通过本机可见BSS确认5GHz/channel44，另一个5G归类依据原映射和明确命名；不能冒称全部RF频段已验证。独立5G密码无缓存，保持未知；原有2.4G密码逐条一致且保留。

在私有副本先执行现有PlatformService校验与原子保存，再按规定停watchdog/任务、备份并校验286文件data，在停机状态应用同一合并，最后脚本启动默认Bridge（PID8916）。原程序/设备ROM未改。库revision1，Note4=3族/BW=原1族，AP01无旧Bridge网络快照，未新增订阅或修改手选列表。MCP分别读取三设备均可见3族，主机当前族正确解析；HTTP204、3设备身份/Profile/许可、Bridge设置及原事务冻结范围保留。设备端投递/真实切网不作为此次合并已验结果。

详细网络名、私有快照和备份位置仅存`artifacts/wifi-families/merge-result.json`、`merge-verification.json`及对应merge-backup目录；一次性助手及构建日志也仅在私有artifacts。未把密码/SSID原文加入版本库。

2026-10-27后续用户确认：第三族实际为同名双频，已通过原有应用服务更新ssid_5g与同SSID凭据，库revision2。三个设备均只支持2.4G，5G仅作为Bridge族识别信息；核验所有设备目标仍等于族内2.4G SSID，目标local_revision、订阅及Bridge设置未变，AP01继续手选。更新前完整备份/逐文件校验，按规定重启（PID17200），HTTP204/MCP实际族读取通过。私有证据dual-band-result.json、dual-band-verification.json位于artifacts/wifi-families/，未改设备ROM或切网。

## 页面重整（2026-10-27）

已按用户粘贴方案完成：Bridge页只有共享族订阅列表与唯一族编辑器；设备配置导入归比较页，未关联记录必须明确选择族；概览工具直接可见，在线胶囊与AP01设备码横幅收拢。旧本地目标编辑/复制/删除入口及草稿状态移除；恢复工具选择已保存订阅网络，未知密码明拒。

- Node回归通过，Edge复用原生脚本的40场景×560/1000两种宽度共80通过；包含共享族编辑/删除保护/失败保留、当前族快捷入口、缓存导入及当前/其他绑定的同步回环。截图已审阅窄宽编辑器、概览、比较、AP01横幅。证据 `artifacts/wifi-families/edge-results.json`、`{560,1000}-*.png`、`workspace-node-tests.log`。
- `bash tools/test-wifi-ui.sh`已通过Git Bash实际运行，因缺少`swiftc`失败；macOS原生WKWebView及真实Rust/RF矩阵仍归B6，不以Edge合成IPC代替。
- 最终隔离构建`cargo build -p bridge-app --target-dir target-note4-sync`通过，只有既有btleplug生命周期警告。**未部署候选** `bridge/target-note4-sync/debug/bridge-app.exe`，39005184 B，SHA256 `621386772286dd20d581c0000bd76182c02772b31a992c75f46b62f879a085ab`。
- 已核对默认exe路径、任务及8765/8766同PID2728/watchdog32096。自动审批拒绝执行`artifacts/wifi-families/deploy-workspace.ps1`：停止共享运行实例、替换exe和重启需用户明确授权。脚本未运行，未停服务/备份/替换exe，真实data、设备网络及ROM未修改。当前部署仍为本页前述9077bb8c版本。
- 部署脚本已准备完整data逐文件哈希校验、旧exe备份、watchdog→任务停止顺序、启动/health/MCP验证；待授权后执行。未提交。
