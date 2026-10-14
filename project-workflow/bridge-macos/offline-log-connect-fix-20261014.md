# 连接结束修复与失联日志 Flash 保留

## Plan

用户授权同时修复 pending connect 导致 Bridge 隔离，以及设备失联时 RTC 日志被覆盖。保持 .100 已部署事实；新候选与部署分开记录。

1. 原生断开明确结束 pending connect；外设缺失的 connect 明确拒绝，未知原生操作仍隔离。
2. 共用生产生命周期逻辑，在 Fake 独立 BLE 设备中触发服务发现前断开，验证下一次连接。
3. 在现有 LittleFS 中保存失联诊断记录，显式版本/长度/CRC/时间与序号；按小时最多保留24小时，容量上限384 KiB，空间不足淘汰旧日志；不写 OTA 槽或修改分区表。
4. 同一存储实现接入 Fake，验证日志环覆盖后的读取、重启、到期、空间不足及截断恢复。提供有界认证读取和可观察存储错误。
5. 构建 Note4 新 ROM，核对版本/大小/哈希及栈；构建部署 Bridge。ROM 刷入前通知用户 BOOT 开启 Wi-Fi。

## Task / status

- [x] Bridge 生命周期红→绿与 Fake 集成。
- [x] Flash 日志保存与分页读取、同源 Fake 回归。
- [x] 资源/栈及相关回归、Note4候选构建。
- [x] Bridge部署。
- [x] ROM窗口通知、用户BOOT就绪、直接OTA及认证实机核验。

## Review

Bridge修复14:44部署；.101于14:54部署并完成跨深睡Flash读回，.102补齐RTC回填和到期去重，15:48已部署，15:54末轮睡醒回填及恢复Light通过。失联保存采用既有脱敏诊断流；真正未知的连接操作不能因 cleanup 成功而盲清隔离。24小时是上限，容量限制可能提前淘汰；失去可信时钟时清理旧日志且明确time_unknown，不将记录重新计时。详细证据留在 artifacts，任务状态关联 backlog B6/C8。

## 实现与证据（14:46）

CoreBluetooth原生断开回调与Fake discovery proxy共用complete/connect和原future。服务发现前独立设备断开后请求明失败、清理后下一连接继续；临时删除共享complete回复，同一Fake测试失败。7项实际原生handler回放、App94/BLE26/Fake68共188通过/3条件忽略。假StaticJson三帧渲染对拍及真实未知继续隔离保留。证据ble-discovery-disconnect-fake-red.log、ble-connect-close-handler-test.log、offline-connect-regression.log。

日志使用原SyncRecord序列和CRC，通过syncAppend注册的记录sink只保存失联记录；认证接触有90秒有界新鲜期，旧Plan不能无限抑制保存。RTC环不变。LittleFS /offline 下24个小时槽，每槽最多16384B；OFL1显式32B头（版本、小时、轮换serial、长度界、CRC），每条28B时间/代际前缀＋原24–120B记录＋4BCRC，共56–152B。每次关闭后读回校验，失败后保留有效前缀，下一次追加/启动修复尾部；不复制整个环或文件。未知格式明拒。小时轮换最多23–24小时，超过窗口清理；睡眠期间不额外唤醒，到期在运行维护/读取时执行。空间不足先淘汰旧日志，保留现有49152B Bundle/sync余量，仍不足明报storage_full。文件有轮换serial围栏，分页遇变更409/过期410，不混入新文件。

GET /offline-log 使用既有已绑定endpoint Bearer token；无hour时列出文件/错误/容量，有hour、offset、serial时最多返回6条原记录base64，含UTC、代际、序号；不延长电源期限。认证status有offline_log摘要。恢复联系不会清除日志或冒充诊断ACK，TTL独立于sync事务。

存储字节：空0B；单条[net] witness 0为103B；典型100条约12KiB；上限384KiB文件逻辑大小，暂存恢复另最多16KiB，LittleFS自身元数据另计。常驻模块仅时钟/游标/错误/锁，整ROM静态RAM120012B（.100为119948B，增加64B）；逐记录缓冲≤152B，修复复制缓冲256B，JSON分页按需堆分配。匹配Xtensa ELF新本地调用链最大1504B，syncAppend含显式sink边1264B；平台/调用者/其他间接调用不计，实机仍待验。

Fake真实HTTP/共享存储证明4KiB环覆盖后最早文本仍可取、进程冷启动仍保留、在线追加只写RTC、秘密脱敏、24小时到期删除。生产存储测试覆盖空/单条/典型/上限、轮换游标、截断写/读回/下一次恢复、空间余量、不删除Bundle、未知时间/格式；offline-log-storage-final.log、offline-log-fake-final.log。

Note4候选.101：1,851,488B，SHA256 6a3a95ec6c8e607fecd39790ce769fb5c5fd1106449f7b0743abca7450008cfa；artifacts/bridge-macos/note4-0.18.101-offline-log.bin、匹配ELF/map，栈日志同前缀。分区不变，当前设备仍.100。

Bridge候选20,826,672B，SHA256 50a79f187388bea00c272e611492358e681d7528cfb2c70a0519253a69b9b5d5；14:44:30托管部署，supervisor49742，health204。旧App/data私有backups/ble-connect-fix-20261014-144428，152文件哈希一致。部署证据ble-connect-fix-deployed.json。源码未提交，AP01工作保持。

## .101 实机验收与 .102 补齐（15:45）

.101 原生OTA单次UPDATE OK，认证运行镜像哈希与冻结BIN一致，ota_1。无认证读取401；认证读出6条/664B，原记录CRC通过。正式Sleep3925之后醒次3/5/7/9均status/claim/plan ACK，关闭0ms，无新隔离；正式Light3929恢复后再次读取11条/1136B，原6条的代际/序号/原始字节全部保持。证据note4-0.18.101-upload.json、note4-0.18.101-auth-verified.raw.json、note4-0.18.101-offline-log-readback.json、note4-0.18.101-offline-log-after-sleep.json。Bridge重启后的supervisor54720，旧文中的49742为初次部署进程。

读回暴露薄唤醒记录未回填：.101初始化仅选当前醒次，薄唤醒不挂载Flash。 .102初始化逐记录回填整个保留RTC环，再注册实时sink；不复制整环，不改原wake/uptime/CRC。分页UTC明确time_basis=saved，表示保存时间，不能冒称恢复出的事件墙钟时间。

到期/轮换/空间淘汰前，原子保存44B OFC1去重游标（版本/长度、代际、序号、保存UTC、CRC），读回验证后改名；只含位置，不含日志内容，不作为业务ACK。防止旧记录尚在RTC时重新落盘获得一天期限。非每记录额外写Flash；空仓库0B，首次淘汰之后游标44B，提交暂存另44B。日志仍384KiB上限，恢复暂存最多16KiB、文件系统元数据另计。生产被动RTC测试直接推进一天而不产生事件，断言到期清空、同环再回填/重挂载仍为空；Fake推进一天会产生新的时钟唤醒，允许新记录，但旧小时与旧文本必须消失。这区分了新日志和旧日志复活。

.102整ROM静态RAM120028B（.100增加80B；.101增加16B）。.102候选：1,853,344B，SHA256 7e9c1fa6df60cfc59ffcdd316263bf17eb377cbcf0b99354f94ecc5d0755ed9d；artifacts/bridge-macos/note4-0.18.102-offline-log.bin及匹配ELF/map。唯一env zectrix-note4-b，marker target zectrix-note4-400x300。生产存储边界通过（offline-log-boundary-final.log）；Fake针对回归通过（offline-log-capture-final.log）；匹配ELF新增本地链最大1664B、capture帧432B（note4-0.18.102-offline-log-stack.log）。平台/调用者/未纳入间接调用不计。候选未部署，15:45通知用户再次BOOT，当前.101。

## .102 已部署（15:48）

用户再次BOOT就绪。上传前认证核对.101/ota_1/原SHA256，归档27条/2317B；直接原生OTA单次HTTP200 UPDATE OK，finally恢复托管Bridge。 .102运行ota_0，认证镜像SHA256等于冻结BIN；heap_free105764/min93772/loop_stack_free7636，软件OTA复位reset_reason3单独记账。.101保留ota_1，.100本机归档。部署Bridge哈希仍50a79f…5d5、supervisor82215、health204。

.102无认证读取401，认证读32条/2848B、CRC全部通过、error=null；原27条的代际/序号/原始字节完全保持。旧OFL1文件兼容读取，未改分区/NVS配置。证据note4-0.18.102-{before-auth,auth-verified}.raw.json、note4-0.18.102-upload.json、note4-0.18.102-{before,after}-archive-{pages.raw,summary}.json。最终Fake完整68项通过（offline-log-fake-suite-final.log）。源码随本次提交归档。

- [x] .102原生OTA与认证镜像/旧Flash记录保持。
- [x] .102自然睡醒RTC回填、恢复Light及末次认证核验。

## .102 自然睡醒最终验收（15:54）

正式Sleep3946认证ACK applied/remaining0，实际HTTP窗口结束；正式Light3948排入下次BLE会合。自然醒次3完成认证status/owner/claim/plan，ACK Light600，cleanup0ms、无隔离；随后ready_report与认证HTTP readback通过。只读最终认证仍.102/ota_0/同SHA256，reset_reason8为正常深睡，network.bridge_reachable=true、error=null；Light3948剩487s，heap_free99676/min76720/loop_stack_free5620。

Flash读回40条/3504B、CRC全部通过、error=null；旧27条全部保持，新增含SYNC_EVENT/SYNC_RESULT，以及醒次1、薄唤醒2的SYNC_WAKE_SUMMARY（time_basis=saved）。这实证.101缺失的薄唤醒摘要在.102下一次完整启动中回填。初次Light ACK后HTTP未就绪的读取有界超时，后续07:53:17UTC认证ready再读成功，不将计划ACK冒称HTTP就绪。证据note4-0.18.102-after-sleep-{pages.raw,summary}.json、note4-0.18.102-final-auth.raw.json、note4-0.18.102-acceptance.json、note4-0.18.102-test-bridge.raw.log（私有）。

本次实现与部署完成。24h到期/游标/重挂载由生产存储与Fake测试覆盖，实际24h持续观察、长期RF和第一次物理链路超时原因仍未验证；不据此关闭B6/C8。本次修复随当前提交归档，AP01变更保留。
