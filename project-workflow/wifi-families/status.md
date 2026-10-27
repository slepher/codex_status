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

### 2026-10-27 01:41 用户授权提交并部署

- 实现提交`53a1fdd`（Unify Wi-Fi management around shared families and subscriptions）。用户明确授权后部署默认Windows实例；前述审批阻挡已解除。
- 首次脚本停止旧实例后读到已退出PID的过期监听记录，身份安全检查中止，未备份或替换程序；先经标准启动脚本恢复旧实例，再增加最多6秒的退出等待后重新执行。仅确认同exe残留进程可停止，未知身份仍拒绝。
- 第二次部署01:41完成：旧exe和294文件完整data备份位于`artifacts/wifi-families/workspace-deployment-20261027-014122/`，逐文件SHA256一致；最终exe39005184 B、SHA256`621386772286dd20d581c0000bd76182c02772b31a992c75f46b62f879a085ab`，与候选一致。
- 标准计划任务启动主PID20624/watchdog31928；任务Running，8765/8766归同主PID，health204、MCP44。共享库3族、3设备identity/capabilities/data_contract/profile/sync_enabled、sources/templates/family_profiles/发布目录以及4份设置/凭据文件均与备份一致；运行observed/认证联系时间/读取尝试按正常调度变化，不要求运行state整文件不变。不打印敏感值。证据`verification.json`和`data-preservation.json`位于上述备份目录。
- Node末轮复跑和git diff --check通过；Edge80及截图沿用最终构建前验证。未推送远端，未刷ROM、未手动切网，macOS与B6实体矩阵仍开放。

### 01:44 有线回退保存提示更正

用户指出有线回退只属Bridge行为，且无需额外写“无需同步设备”。`library_wired`成功提示改为“有线回退已保存，在 Bridge 未连接 Wi-Fi 时生效。”，其他保存提示保留。Node与隔离构建通过，默认Windows已备份/重启部署：39005184 B，SHA256`14c519e07efaa9e13f5ce5d08a9985c0ae627993578c38abf43608735a6575d4`，health204/MCP44。旧exe与295文件data备份逐一校验一致，证据`artifacts/wifi-families/workspace-deployment-20261027-014434/verification.json`。仅调整提示，无手动切网或ROM修改。

## 名称匹配与BLE窗口修复（进行中）

调查见note4-connection-20261027.md；按用户要求修复，尚未构建新ROM/部署本轮修复。

## 2026-10-27 名称匹配/窗口修复已部署

- 连接匹配v2（wc=2）不含label/族显示名，只对network_id、SSID/open及密码摘要核验；v1仍按旧摘要，未知显式match_v拒绝。共享族改名保留已确认credential/version；冻结事务确认期间只改名称也可消费原回执，真实SSID/密码变更保持新意图。
- BLE control_only使用共享C++编码单帧控制摘要；包含boot/fw/读写权限/principal/namespace/高水位/修订。原完整状态分页保留，旧ROM分页预估不够则前置拒绝并预留后续命令预算；删去会合重复读取。apply/activate/trial预算不足750ms不发送，已发未知仍只查原ID。
- 最终core/ble/render/app共442通过/5条件忽略；额外显式执行真实device-sim的原未知激活预算回归1通过（原忽略项）。最大字段完整控制帧481 B（含48 B头）；短窗口生产fixture只1次wifi_status，旧大摘要在第一页面后发送前明拒，保留剩余预算。Python/Rust摘要与C++实际匹配一致，改名同摘要、SSID/open/密码变化失配。证据`connection-fix-rust-verified.log`、`connection-fix-native-sim.log`、`connection-fix-frame-limit.log`、`connection-fix-cross-language.log`、`connection-fix-control-regression.log`。
- Note4构建env `zectrix-note4-b`，`.107-note4`冻结bin/ELF位于`artifacts/wifi-families/note4-0.18.107-connection-fix.{bin,elf}`。ROM1858240 B，SHA256 `973c4d6c9377747dc4fafb7e1bd72bf54fd84482a16c5dea26d9d359f8df81d7`；ELF SHA256 `27f4c7d9b1acabdd6c95f3e83c6b5406914a17c2985bde51a0587f7bdf2cb998`。marker target `zectrix-note4-400x300`已核对。真实栈90检查通过，最大本地帧992 B/受检链2784 B；新控制摘要链384 B、打包帧336 B。Windows原检查临时文件锁会导致objcopy失败，使用先关闭句柄的私有包装运行同一检查逻辑；当前已退役入口用对应两个no-legacy选项明验，未放宽栈阈值。
- 资源：空/1/典型3/上限12条网络的NVS增量均0 B，WFS4/WTX1/原子提交及回执格式不变。未新增常驻仓库/凭据副本；连接摘要仅34 B有界SSID+NUL+flag缓冲（另32 B digest/65 B hex，生命周期为调用）。控制打包JSON临时对象按11字段分配，完整帧上限481 B；新增动态JSON实际峰值未做实机分配器隔离测量，不把帧长当峰值RAM。实机OTA后heap_free118308 B/heap_min99072 B为当时采样，非精确增量保证。

### 实际部署

- Windows Bridge：`bridge/target/debug/bridge-app.exe`，39035392 B，SHA256 `d9778eb76f6f45334a359da6a6d684ad2444061b15ef1df2356a8947c530bb8d`。19:56按标准任务停启，旧exe与301文件data逐文件校验备份于`artifacts/wifi-families/workspace-deployment-20261027-195607/`；health204/MCP44。
- Note4 OTA唯一原请求`note4-connection-fix-20261027-107`/job`af38c6b3`，1次上传；排队瞬间设备正常休眠，先前同boot认证版本/槽位仅作排队依据，正式worker重新认证MAC后才上传。未通过临时预检失败重发写入。
- 20:01原OTA `succeeded/image_verified`，认证同MAC `7C4FADB93408`运行`.107-note4`/ota_1，running-prefix-v1完整1858240 B与冻结bin SHA256完全一致；此前运行ota_0未作为写入目标（未整槽导出读回）。Wi-Fi控制v2、WFS4/WTX1 ready/uncertain=false、4条网络、内容ready/error空。随后20:07独立认证HTTP仍`.107`、HTTP ready/bridge_reachable=true/network.error=null，同boot。证据`connection-fix-device-verification.json`、`connection-fix-final-status.json`、原事务请求/响应在私有artifacts。
- 未修改真实SSID/密码/订阅做测试；BLE实际最大帧/改名RF全过程仍不能由合成fixture代替，长时/跨平台余项保留B6。无需重新配对，GATT表未改。

## 统一版本系统（用户澄清：不要求同时打包）

`src/main.cpp`唯一`FW_RELEASE_VERSION=0.18.107`，Note4/BW派生`-note4`/`-bw`，今后改共同基号一次。各目标可独立构建/打包/部署，使用所处源码快照的共同序列，不继续独立编号。此次不执行拟议的双板.108打包或升级；AP01沿用兄弟仓库版本。

| 层次 | Note4 | 书桌屏 |
|---|---|---|
| 当前源码基号 | 0.18.107 | 0.18.107 |
| 下一次以此源码打包的完整版本 | 0.18.107-note4 | 0.18.107-bw |
| 已部署/最后验收 | 0.18.107-note4，20:01镜像证明 | 0.18.38-bw，10-26部署证据；本次未升级 |

打包工具增加release_version与source_commit/source_dirty；核对ROM基号等于当前源码，拒绝旧缓存冒充新版本。`tools/test-release-rom.py`通过；对真实冻结Note4 .107通过，真实书桌屏 .38缓存按预期拒绝。后续同版本重编仍用SHA区分具体镜像，部署台账不由源码宏推断。版本声明整理发生在.107 ROM部署之后，未重新打包/刷机，因此已部署ROM仍是上述973c4d6c哈希。

`tools/test-firmware-modules.py`在未改的display旧字符串断言处失败；相关display源码与该脚本均与HEAD一致，entry/version之前断言已通过，不将该存量失败计为此次全套成功。

更新：2026-10-27 20:08，Asia/Hong_Kong。本轮修复/版本来源改动尚未提交。

## 书桌屏.107-bw部署（进行中）

用户明确授权部署书桌屏，切换154g标准BW目标，尚未构建/写入本轮书桌镜像。

### 书桌屏.107-bw部署完成（2026-10-27 20:17，Asia/Hong_Kong）

- 用户随后明确授权部署书桌屏；按`tools/pio-target.ps1 -Target 154g`构建`esp32-s3-epaper-154g`，55.83s成功，只此一个pio进程，无clean/灰度变体。共同基号`0.18.107`/完整版本`0.18.107-bw`；Note4未重编/刷入。
- ROM `artifacts/wifi-families/desk-0.18.107-connection-fix.bin`，1840176 B，SHA256 `72225732b045651efd991bd4a29393c5df55e871b436c565066fb1ff1f5d6705`；匹配ELF `desk-0.18.107-connection-fix.elf`，SHA256 `4751b244f00320e234346df12f07b57ee21684aafc79205012828d9fd1395ccf`。marker target`codex-status-154g`、源码基号及原生双槽各3145728 B容量核对通过，未混用Note4镜像或旧.38缓存。
- 真实ELF栈90检查通过，最大本地帧992 B/受检链2784 B，控制摘要链384 B/打包帧336 B。RAM静态81240/327680 B，Flash1787020/3145728 B；沿用已验442+1共享协议回归，Wi-Fi存储格式和NVS布局不变。私有manifest登记source_commit=`3b631f5...`/dirty=true，保存构建时source.patch及其SHA，不伪称候选来自干净提交。配置隔离快照已保存154g；以后Note4构建仍经target脚本切换。
- 原OTA request`desk-connection-fix-20261027-107`/job`c23c5155`，1次上传，最终`succeeded/image_verified`。认证MAC`70041DD7A340`运行`.107-bw`/ota_1，running-prefix-v1完整1840176 B/SHA与冻结bin完全一致；原运行`.38-bw`/ota_0未作为写入目标。旧非活动槽1的.37已由本次正常OTA替换，不再把它列为当前回退ROM。
- 20:14及20:15独立认证HTTP，同boot`4d032d61f77426b73eafcb20412d03d3`、HTTP ready/bridge_reachable=true/network.error=null；wifi_connection=2，WFS4/WTX1 ready/uncertain=false、2条网络/revision3，内容ready/error空。原Bundle`99dc6326`、quad、Profile/sync设置/共享族/设备目标均保留，未重新发布。
- 软件重启按`application.cpp`未知保留状态分支轮换active context，以防旧在途数据重放：旧`39e33b5cb20d57b5`→新`0b7b426c5239dd35`。Bridge reason=reconciled_from_device且ID一致；这是正常启动处理，不能要求context字节不变或将其误报为Bundle丢失。未做实体屏幕目视/长期RF/掉电验收。
- 证据在`artifacts/wifi-families/desk-0.18.107-{build,stack}.log`、`desk-0.18.107-{candidate,ota-request,ota-queued,verification,preservation}.json`、冻结bin/ELF及source.patch。未停桥、未打开串口、未擦NVS/分区或改SSID/密码，未提交。

当前实机版本现为Note4`0.18.107-note4`、书桌屏`0.18.107-bw`，同属共同基号0.18.107；先前.38表格仅为本次部署前历史。

### 提交前最终检查（2026-10-27 20:24，Asia/Hong_Kong）

旧分页及后续Wi-Fi请求的预算进一步取调用预算、设备RF截止和共享命令deadline的最小值，避免只有Bridge预算时仍越过真实通信窗口。新增截止时间/已过期预算测试，连同单帧控制摘要、未知ACK共享deadline、完成回执退休共4项定向回归通过；构建通过。日志：`artifacts/wifi-families/commit-rf-budget-{tests,build}.log`。

最终Windows Bridge为39039488 B，SHA256 `0b8b843414d97904fb21dac3f341bfc269edcc147d42e759e6e4039fd4899a47`，20:24部署，旧exe及308文件data逐文件备份核对通过，health204/MCP44；证据`artifacts/wifi-families/workspace-deployment-20261027-202355/verification.json`。此补充仅宿主Rust，双板.107 ROM及其hash保持上述实机证明，无重新OTA。本次提交纳入修复、统一版本规则和双板部署记录，不纳入私有日志/data/ROM/ELF；前述“未提交”是对应时点的历史状态。