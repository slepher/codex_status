# 状态

2026-10-26：内置页状态ROM已USB部署，通用Wi-Fi配置已通过USB JTAG转换并实机联网。认证运行镜像SHA已匹配。旧 quad v12 含 device.state；兼容 quad v15 已保存并补齐月度 resetsAt 绑定，保留 quad/mini 顺序及原数据投递设置。完整 Bundle 99dc6326 已按用户后续授权通过原事务续传提交，认证ready/displayed，Bridge对账succeeded。

首次直接 OTA `.37-bw` 返回 UPDATE OK，绑定加密 BLE info（同MAC）及串口CLI均确认 `.37-bw`；运行整镜像SHA尚未认证。COM4枚举VID303a/PID1001、serial=70:04:1D:D7:A3:40；原生CreateFile/ReadFile/SetCommTimeouts监听，不调用DCB/SetCommState/DTR/RTS、不清缓冲。status/batt/diag与间隔复读status有响应；BLE wifi_owner/claim/plan/network_start回执亦成功，证明主循环并未被错误页阻断。串口为Wi-Fi off/IP-，等待Bridge选择网络；不能从该日志判为panic或复位。网络boot_id前后不可读，明确未知；无ROM reset文本，不声称跨平台无复位保证。

Windows默认Bridge配置原暂停书桌屏，原选Note4；已备份配置，恢复诊断期间临时解除暂停并选书桌屏。该exe仍发生BLE原生连接超时隔离/自动恢复；诊断结束已还原原默认设备/暂停名单并重启，未改数据投递开关。只读Wi-Fi任务4ecf7a795102d268等待会合。未擦NVS/dump/分区、未USB刷机。

实体BOOT实测：用户按键后串口捕获`action=1 physical_ms=718502 handled_ms=718671`（169ms），随后`[ble] session on`；紧接只读CLI仍返回`.37-bw state=off ip=-`。BLE status另有applied ACK。错误页不显示旧版无线状态导致视觉无变化，不能据此判为按键失效/死机；Wi-Fi等待Bridge选择网络与Windows BLE隔离是独立阻塞。证据serial-after-button.private.log、ble-read-1.private.json、serial-commands.private.log；不把这些运行版本证据扩大为整镜像哈希验证。

Wi-Fi off根因更正（用户指出设备配置应通用）：离线使用官方NVS解析器检查USB部署前已保存的NVS，仅输出键/长度/CRC，未打印凭据。wifi命名空间有s0/p0/last，条目CRC有效；无wifi-bonds条目。新启动startFallback仅调用wifiSchemesCredentials；后者只接受wifi-bonds的active配置。radio::loadPreferredWifiCredentials仍有旧wifi槽读取实现，但全仓没有调用点。因此这台旧设备的通用凭据保留却未进入新启动路径；此前将Bridge暂停/no_mapping列为设备自身Wi-Fi off根因并不准确。应修复旧通用凭据读取兼容，不能要求每个Bridge重配来掩盖。尚未修改兼容策略，须区分“新版配置不存在”的旧设备与“新版格式损坏/显式清空”，避免无条件旧键回退。证据wifi-nvs-keys-summary.json与部署前私有NVS；密码有效性尚未通过真实关联验证。

USB Wi-Fi更新已完成：串口ROM握手反复失败后，使用已枚举同MAC的原生USB JTAG。该接口最初虽有WinUSB服务却缺DeviceInterfaceGUIDs；安装乐鑫官方签名有效的USB_JTAG_debug_unit.inf（仅VID303A/PID1001/MI02，oem88.inf）后可打开。包安装同时更新了该硬件ID的三个已登记接口；未改COM串口驱动。官方OpenOCD版本/ZIP SHA由本地ESP-IDF tools.json校验；驱动签名及来源记录在私有证据。

先读取当前16KB NVS并比对原凭据，再生成119B共享active WFS4（来源usb、network_id=usb-existing，不创建owner/回执），通过生产wifiDecode4/wifiSchemesBegin/wifiSchemesCredentials及坏CRC拒绝验证。仅在现有wifi-bonds命名空间增加store3 V2 blob（6个NVS entry），更新0xC000的一个4KB页；原页全部存活记录及另外三页逐字节保留。完整NVS写后读回等于预期（wifi-usb-patch.json、wifi-usb-write-verified.json）。原旧凭据键保留，新增配置不绑定某个Bridge。临时明文凭据输入已删除，敏感NVS/恢复页留私有artifacts。

退出ROM等待经JTAG/官方USB复位处理，均作为主动调试复位记账。运行态JTAG确认network_count=1、format=4、configuration_bytes=verified_bytes=119、storeReady=1。HTTP同MAC确认`.38-bw`/槽0、connected、192.168.3.163、RSSI -24；认证/api/ota/image返回完整1837008B SHA与候选一致（wifi-usb-authenticated-image.json）。Windows Bridge原暂停名单/默认选择未再变动，旧Bundle错误页保留。期间出现RTC WDT reset16，且HTTP采样间一次wake_generation改变，原因未定，不能简单归因于调试或宣称稳定；最后同wake_generation 1940753067两次uptime160→175均connected（wifi-usb-observation.json），长期/重启风险归B6。

已部署：artifacts/desk-ota-20261026/desk-0.18.38-bw.bin，1837008B，SHA256 2b002a21e2673a96a4c519fb746219737911a82f577a2b5597cb2a89fba6580e。只为ESP32墨水屏内置页增加两行Wi-Fi阶段/实际BLE射频；模板不叠加状态栏，AP01无改动。sleep最终帧明确WIFI sleep/BLE off，不把手动授权当射频开关。沿用三次有界刷新；不修改电源期限。

USB部署：Wi-Fi原生OTA复探不可达后，用户要求先上传查看，使用已连接COM4。esptool核对同MAC、实际分区表和otadata：槽0 seq1/槽1 seq2均valid，原活动槽1。将冻结应用写入非活动0x10000，写后摘要校验通过；额外整份读回在376832B时传输中断，停止且未切槽。重连再次verify-flash整1837008B摘要匹配才切槽。官方ESP-IDF otatool.switch_ota_partition离线生成seq3，保留第二条记录，仅写0xD000一个4KB扇区，随后工具主动RTS复位。无bootloader/分区表/NVS/storage/dump写入。串口无DCB监听确认`v0.18.38-bw ... reset=usb slot=ota_0`及CLI`.38`，network glyph confirmed=1；USB工具复位单列，不判panic。原槽1`.37`保留，旧Bundle job776b2f66/quad/mini仍在。启动后整镜像SHA接口未读到，不将片上摘要核验冒称认证API核验。日志usb38-{write,readback,verify,select,running.private}.log；同源错误页预览error-page-preview.png不是实机截图。

验证：tools/test-builtin-radio-status.py生产绘制函数提取运行，覆盖200/400边界、长错误文字、四Wi-Fi阶段、自动BLE、睡眠/会合；test-wifi-metadata验证射频变化触发重绘与三次上限。154g构建成功，匹配ELF栈检查通过（Windows NamedTemporaryFile关闭句柄适配，ELF不改）。私有证据在artifacts/desk-ota-20261026/。剩余验收唯一来源为backlog B6。

2026-10-26模板发布验收：先认证同MAC与原committed_job_id=776b2f66，使用原冻结55469B/CRC4cbbc978及同一bundle-99dc6326身份执行有界BEGIN/CHUNK/COMMIT，不创建替代任务。首次BEGIN因MAC格式identity明确拒绝；改用认证返回的冒号MAC后BEGIN成功，首chunk误用传输session被业务nonce校验明确拒绝；按认证status的业务nonce续传原任务成功。COMMIT applied/displayed/flash，认证新context ffaf4e43e43639c1、commit_seq4097、active quad、content_state ready、content_error空、data_contract有效，运行仍0.18.38-bw且同boot/uptime递增。Bridge platform_status_refresh对账原job succeeded/无last_error，pending已清除。临时解除暂停已恢复，原数据投递false保留，不以无配额数据判模板失败。证据template-resume-{before,after,ack}.private.json、template-final-refresh.private.json与resume-template.py；面板视觉仍待用户确认，WDT根因与旧键兼容风险不关闭。

2026-10-26后续：用户报告默认页与按键切换无变化。认证旧Bundle已替换且两页有效、data_seq0；源码renderCurrent在lastUsage为空时直接screenStatus，导致已安装模板等待首份数据，COMMIT displayed仅代表面板刷新而非模板来源确认。日志有Click和Template action（长按>=2秒），后续认证context改变，不能称按键完全无响应；一次远程mini激活明确context冲突，未伪报成功。用户随后确认模板已显示并改为要求开启同步，本轮未修改/重编固件。经platform_data_sync_save开启设备许可，持久解除Windows Bridge该MAC暂停并按规定重启，原其他设置保留；platform_push_now返回HTTP data seq288 applied/displayed，最终认证applied_seq288、quad、content ready/error空，落盘sync_enabled=true。首份数据前渲染入口问题归B6；WDT风险仍保留。证据template-user-report-refresh.private.json、template-button-device-log.private.txt、sync-enable-verified.private.json。
