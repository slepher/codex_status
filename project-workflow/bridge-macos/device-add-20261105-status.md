# 添加设备交互修复（2026-11-05）

已构建部署Mac默认Bridge，exe 81431200 B，SHA256 `67964ae8cb9b4f6ca3b84e411b9da63807f18c8dc124fa1767c156fc144a5789`。同LaunchAgent启动supervisor5327；原exe及完整data保存在 artifacts/bridge-macos/bridge-app-before-device-add-20261105、data-before-device-add-20261105.tar.gz；启动时核对9个非日志文件均与备份一致。工作区原有AP01 App OTA边界撤出改动保留并随本次构建包含，未提交。

- AP01搜索新增独立关闭按钮；停止仅在搜索中出现；关闭立即隐藏并拦截搜索/配对迟到响应，不取消已经发送的设备配对事务。
- 搜索结果投影持久pending事务存在性；已登记且无pending设备不重复提供配对；无事务查询/取消按钮禁用。
- 蓝牙添加进度与错误放在添加按钮附近，重复点击受控，登记结果必须registered=true才报成功。扫描锁等待限10秒，保留实际扫描/连接错误，避免全部伪装成未广播。
- 全局BLE恢复放在响应顶层及独立全局提示，不覆盖每设备最近认证状态、不重复生成设备告警。

验证：Node设备页回归通过（关闭竞态/事务按钮/错误归属/登记未知结果）；原生WKWebView82场景通过，560/1000截图已审阅；Rust global_ble_failure_does_not_override_authenticated_device通过；AP01边界脚本与git diff --check通过。真实运行UI通过CUA点击添加AP01，看到停止/关闭；点击关闭后面板立即消失，AP01卡片不再显示蓝牙错误。合成IPC不证明Note4绑定完成。

未解决：Note4 .107首次BLE服务发现此前两次在link_connected后4秒超时并触发全局隔离；本次未刷机。用户要求继续绑定并监控存活，14:34附近USB枚举已不见Note4，/dev/cu.usbmodem101消失；打开监听失败ENOENT，未发送pair，等待用户恢复USB/开机。不能将USB不见直接判为设备固件崩溃。

证据：artifacts/bridge-macos/device-add-{wk,rust,build}-20261105.log、wifi-ui/{560,1000}-device-add.png；此前串口原文note4-usb-20261105-*.log。关联B6/C8，不结实机绑定验收。

## 首次绑定追加

USB恢复后14:36串口与Bridge共同确认Note4服务发现/7特征/INFO成功，未绑定peer可在人工窗口连接，设备未重启。UI随后返回已取消添加，改用应用内MAC确认框替代window.confirm；Node与WK82复验通过。进一步查明wifi.add误用仅允许已有bond的DeviceConnection，已在显式add路径复用fetch_device_token完成MAC校验/系统配对再登记，保持自动路径禁止首次配对。pairing定向测试见device-add-pair-tests-20261105.log。

最终exe 81432032 B，SHA256 `befe4a7bd51f31b0e5f3c29b26da6aac8fe2f4850952edf9cafa7a8fd24733da`，已重启LaunchAgent。14:41:43再开120秒窗口并持续监听，USB同MAC仍在线。系统未将无窗口的托盘进程提供给CUA，反复getApp超时；已请用户点菜单栏打开面板。尚未执行最终确认/登记，不能宣称绑定成功。原始监听artifacts/bridge-macos/note4-binding-20261105.log。

## 实机登记与后续重连（14:45–14:52）

14:45:10同MAC重新开启120s人工窗口，持续串口100s，无DTR/RTS操作。真实应用内确认MAC，点击绑定并登记。14:46:14固件authenticated bonded peer、主机token保存；14:46:19–21 wifi_capabilities ACK，state.json持久新增Note4及能力；14:46:27 claim applied，14:46:28 sync_config applied。同wake_generation121932656/wake_seq288。真实UI显示已登记。未发布Bundle或写设备Wi-Fi。

**不能将本轮登记视为窗口外重连已验收**：14:49串口发现多次connected bonded=0后立即unbonded peer outside pairing window断开；本地代码onConnect及blePoll依据当前连接sec_state.bonded拒绝，NimBLEConnInfo::isBonded读取当前连接状态，而NimBLEDevice::isBonded另查持久bond列表。高度怀疑在加密恢复前误判；未读取实机bond仓库、.107精确源码/ELF未匹配，不能排除持久bond缺失，不据此宣称唯一根因或修复完成。

当前fw.107/ota_1，uptime1084.491s、heap64316、epd_writes216/busy_fails0/trusted1；state=idle/ip=-。日志network_not_found→phase off→BLE会合phase idle→render codex-status-a。现有设计idle隐藏Wi-Fi图标，与用户图标消失一致；未读取设备活动Bundle，故仅作符合设计的解释，不冒充逐像素验证。

本机MCP只读Note4视图：manual_selection=false、automatic_state=no_mapping、matched_network_id=null；设备尚未订阅Wi-Fi族。AP01设备码仅用于AP01，Note4不应显示/提交。此前添加后选中MAC带冒号与规范MAC不匹配已修正，避免无选中状态的旧AP01区域残留；当前Node及WK82通过。蓝牙恢复状态已真实转为recovered。新固件未构建/部署，现有绑定/网络/模板保留。

最终Bridge exe 81432032 B，SHA256 `dd8df31bc3599953cb9512b2c984f8bf0d2cd12ada18fd619c543211f63efa41`。LaunchAgent已重启。证据新增note4-after-binding-20261105.log、device-add-final-{node,wk,build}-20261105.log。

## 首次联网与固件候选（15:15进行中）

新增证据：首次登记无IP时，Plan activation在report_device_address_missing前置退出。已改无IP时使用本机地址；本机TUN默认路由172.19.0.1，CoreWLAN实际en0/192.168.31.66，故登记/claim/Plan回报统一优先实际Wi-Fi接口。显式use_current在物理窗口也应允许既有认证事务恢复，不把用户显式恢复降为仅选择。新增no_ip服务测试先发送Plan再模拟认证地址发现，通过；普通BOOT/失败报告准入回归通过。AP01切换后设备码隐藏/清空Node通过。运行现场已见Plan9173 applied，但当前.107报告plan_network_target_mismatch，尚未联网，不算恢复完成。

固件候选0.18.108-note4已构建，尚未部署：artifacts/bridge-macos/note4-0.18.108-bond-reconnect.bin，1873040B，SHA256 13019165228c888a25f40e8c9f8566673419a5a57d25fcf4b591a44b06b467fa；匹配ELF SHA256 9863bcce8ebac32d7f6f580bfc03ad11fc8e2dafd2d693453e13c63f3533cc99。marker正确、栈检查通过、回调宿主已验证持久bond重连/未知peer拒绝/加密认证失败拒绝；静态RAM97856B不变，flash代码较原构建增加272B，无新增持久格式。

## 用户授权USB部署.108（11-05 15:18–15:25）

用户明确要求直接刷入基于.106构建的.108，不等待Windows .107源码。刷前串口同MAC、.107/ota_1/off/noIP。esptool5.4核实USB303A:1001/MAC7C4FADB93408，真实分区ota_0=0x20000/0x5F0000、ota_1=0x610000/0x4F0000、NVS0x9000/0x4000、otadata0xD000/0x2000。16MB和2MB连续读取均发生USB读取中断，未形成完整Flash备份；256KB分块成功保存将覆盖的ota_0前2MB及0..0x10000引导/NVS备份，私有artifacts留存。

仅写0x20000候选1873040B，esptool校验成功，再分块读回全镜像与候选逐字节一致（SHA25613019165228c888a25f40e8c9f8566673419a5a57d25fcf4b591a44b06b467fa）。otadata从seq39/40、state2改第一副本seq41/正确CRC，第二副本40保持，选择ota_0；不写bootloader/分区表/NVS/storage/ota_1。备份前2MB SHA2566dbc51cf383aa07e78de762602e80d5789713b9c04ec23f70927d16f85c77b75；引导/NVS备份SHA25690908bef0b0c2fff44de90ca7edac448e18bddc9c2fd8e710c22f79604f8d5cf。

RTS hard-reset后仍在stub，no-reset连接确认；官方run默认不重启，最终esptool watchdog-reset退出。以上为工具主动复位，不记作固件自发WDT。随后串口明确v0.18.108-note4/mac7C4FADB93408/ota_0，原Bundle加载、token从NVS加载；初次时钟局刷失败后完整重画，epd_writes4/busy_fails1/trusted1。Wi-Fi仍在连接过程，无IP；待窗口外BLE及HTTP验收。证据note4-108-{write,readback,switch,tool-reset,running,reconnect}.log。

15:26:31未打开人工窗口的自然会合：connected bonded=1 → authenticated bonded peer，claim/sync_config/wifi_match/Plan9176均ACK。15:26:49 recovery_save原事务完成、version346；15:26:56 activate完成。15:27取得Xiaomi_1EBB /192.168.31.90，ready_report先ACK后同MAC认证HTTP核验。首次会合尾部http_proof_budget_exhausted，但随后report独立认证verified，不以超时误判最终失败。15:27:25最新完整Data seq5 applied/displayed，15:27:32 sync-v1 complete；15:28恢复视图同MAC/.108/ota_0、wifi connected、applied_seq5/displayed、人工窗口0。原绑定/NVS/Bundle沿用，未重新发布模板。HTTP续租仍使用lan_ip()而把owner host覆盖为TUN，正改为同一真实Wi-Fi接口解析函数。

15:30最终Bridge81482064B/SHA256ddff5b57a7470de3088de0b35f6e2359db6112e7ca99f4fdfdf58d5d9af90a43已重建重启（LaunchAgent17201）。claim定向9测试通过；真实HTTP续租后owner host从172.19.0.1变为192.168.31.66，设备仍.108/ota_0/connected/applied_seq5/displayed。本轮明确刷入与联网恢复完成，未扩大为B6/C8全矩阵验收；未提交。
