# Wi-Fi 会合协议实施计划（2026-10-04）

状态：P1–P6 软件实施、验收及 Bridge/Note4 配套部署完成；真实 RF 与资源验收仍开放。最新规则覆盖 wifi-protocol / wifi-redesign / wifi-rendezvous-execution 的历史 BOOT-only、密码不回读及重复读配置条款。待办唯一来源仍为 backlog B6。

## 冻结的行为

1. 已绑定且认证、owner 允许的普通配置写入借用本次正式 light / sync / 物理维护窗口；授权截止取既有窗口，不隐式续租。BOOT 仅首次绑定、救援和旧槽迁移。
2. 页面选中的整条变更可经 BLE 原子提交（最多四条），复用 HTTP 的 NVS 事务与持久回执。缺 Wi-Fi 不阻止保存。MAC、bond、nonce、版本和 owner 校验保留。
3. 显式完整读取一次后缓存；导入与同步不用前后重复读配置。同步冻结设备修订，冲突暂停；提交回执更新选中缓存与基线。未知 ACK 只查询原事务，不盲重写。
4. 首次会合先尝试认证 HTTP；不可用时才 BLE 查询最小网络诊断。已入网但 HTTP 不通展示 Bridge 访问失败；401/身份错误独立。认证失败关联尝试的网络/版本，不能依赖断网后 WiFi.SSID()。
5. 当前环境没有唯一完整 Bridge 映射先提示本地配置；设备缺项/不同则保存并启用；已有但未活动只启用；同版本认证失败提示修正凭据并停止重复写入。其他失败有限退避，真实 HTTP 恢复清除旧错误。
6. **仅页面手动 Bridge→设备同步**校验合并后仍有当前环境网络；允许删旧项，不自动补选、切网或试连。预览冻结主机网络、本地版本与快照；服务端再次验证，自动配网/程序化MCP同步/本地保存/缓存导入不受此 guard 影响。
7. BLE 存配置、激活与 HTTP 交接共享绝对有界预算；HTTP 就绪且交接确认后关 BLE，再允许 light-sleep。失败结束本次窗口，保留可恢复作业；close 有界且先释放投递锁。

## 工作步骤与验收

- P1：设备合法窗口写授权、多条 BLE 事务、回执归属；宿主正负权限/原子变更/重放测试。
- P2：Bridge 缓存冻结修订/纯回执确认、通用 BLE 写作业；无二次 snapshot、丢 ACK 查询、重启和新版编辑保护测试。
- P3：按需网络诊断、错误投影与统一预算；真实 Fake-GATT 会合次序、正常路径无额外查询、错误分类回归。
- P4：页面当前网络合并校验与失败文案；生产 HTML JS、原生 WKWebView 窄/宽场景与截图审阅。
- P5：core/app/device-sim 与 C++ 放行、唯一 Note4 构建/栈门禁、Bridge 候选；先 Bridge 后 Note4 OTA，备份数据/安装身份并核对运行镜像。
- P6：PROGRESS/backlog 对账。真实错误密码、断网、多次睡醒 RF 验收如受系统权限/现场影响须明确留项，不以模拟验收代替。

不改钥匙串/系统信任、不重置设备存储、不改分区/模板协议；变长 Wi-Fi 存储与 AP/USB 写入口收归仍单列 B6。

## 放行记录（2026-10-04 已部署）

P1–P6 已完成本轮软件与部署交付。生产 C++ 管理/setup/rendezvous 正负授权、多条事务/回执、空SSID认证失败及交接边界通过；最终setup回复418B。core118通过；app全量65通过/1既有ignored，随后页面/MCP范围回归1通过。后者确认主机网络未知时页面提交拒绝，而程序化同步可离线入队，且不取投递锁。device-sim受影响5项通过，生产调用顺序snapshot→apply→result，丢ACK/重启不增加完整读取。生产HTML JS及原生WKWebView32个560/1000场景通过，窄宽同步守卫和不可达状态截图已审阅；WK测试使用合成IPC。

交接最终补充已纳入候选并复验：取得DHCP且候选持久启用完成才HTTP ready，旧网络HTTP不能完成新激活；显式当前网络启用意图保持至完成；未知Plan仅查询已发事务，无新BLE写入。30s绝对预算共用，无逐阶段重开。保留此前Astra审核的持久Plan ACK/高水位、会话/唤醒/owner交接证明、准入读取集合和先释放锁再有界close。

先备份运行数据和安装身份，部署Bridge，再OTA Note4，最后部署页面/MCP范围修正；未重置配置。Bridge默认supervisor63095/health204，exe76,063,040B，SHA256 `23DFF946FA555849D40E494B0D74B5293A6ACE9F151BD403FECEE7FCB3DD4D87`，运行app `artifacts/bridge-macos/Codex Status Bridge.app`。旧app `Codex Status Bridge-before-wifi-rv2.app`；私有数据备份 `~/Library/Application Support/dev.codexstatus.bridge/deployment-backups/wifi-rv2-20261004-172942`；installation-id未变。

Note4 `7C4FADB93408` / `192.168.10.101` 已运行 `0.18.54-note4-b-wifi-rv2`，target `zectrix-note4-400x300`，ota_1；ROM1,737,776B，SHA256 `1923E9B0C9138F5DAF4CF1BFC4A508904990EEA6C2364CE1EA9D63015F122604`。产物 `artifacts/bridge-macos/note4-0.18.54-wifi-rv2.bin` / 同名ELF。仅构建zectrix-note4-b，真实启动栈64/80B≤1024B；OTA21192869成功，独立认证运行镜像身份、目标、版本、长度与哈希匹配。未开串口、清NVS或改分区。

证据均在 `artifacts/bridge-macos/`：wifi-rv2-deployment.json、wifi-rv2-running-image.json、wifi-rv2-rom-manifest.json、wifi-rv2-passive-view.json、wifi-protocol-tests-final.log、wifi-app-tests-final.log、wifi-page-scope-test.log、wifi-rv2-sim-tests.log、wifi-rv2-pio.log、wifi-rv2-app-build.log，以及wifi-ui/560-sync-guard.png、1000-sync-guard.png、1000-current-unreachable.png。运行快照仅被动查看缓存元数据，未为测试新读或改写设备配置。

## 保留验收缺口

- 系统location/bluetooth仍not_determined，主机permission_required；权限界面已有引导，未改钥匙串。真实当前SSID、无配置/错误密码/旧网络/同网HTTP不可达、AP/DHCP及多次睡醒RF闭环尚未实机验证。
- 初次新版显式完整读取中密码缓存的实机证据仍待用户主动读取；现有2条快照继续使用，不自动重读。
- 变长仓库/按需RAM、AP/USB写入口收归及兼容代码退役不在本轮已实现范围；跨平台、其他硬件/架构、功耗与掉电矩阵继续按backlog B5/B6推进。B6不结项；本轮源码与文档随本次提交归档。
