# 10:09–10:10 网络启动与日志留存调查

关联 B6/C8。用户要求调查两个问题：为何该时段设备日志没能留存，以及退出 zzz 后为何没能立即启动联网。

## 计划与任务

1. 对齐旧 Bridge 10:09–10:11 日志、设备 wake_seq=632 与本机诊断归档；不使用后续联网成功倒推旧事件。
2. 核查日志采集筛选、RTC 容量、冻结触发、HTTP 归档与本机保留规则。
3. 用生产代码确定性复现未确认记录覆盖，以及 Plan 延长 RF 后旧 INFO 截止阻止 network_start。
4. 写明已证事实、历史未知、现有修复覆盖和后续缺口；不改设备配置/ROM，不重发历史事务。

## 待检验假设

- H1：日志在联网恢复前被设备环形缓冲覆盖。预测：首份恢复归档显式含 overwritten gap，生产 syncAppend 能在 ackedSeq=0 时淘汰旧记录。
- H2：HTTP 不可用阻断诊断冻结与上传。预测：Bridge transfer 在认证 status 失败时不会发 sync begin；设备没有独立离线冻结该次网络启动日志的路径。
- H3：某些网络状态文本未被采集。预测：生产日志筛选器拒绝正常连接/地址报告；结构化事件覆盖须单独核查。
- H4：旧 INFO RF 截止阻止启动命令。预测：Plan 设备侧延长 RF 后，未 refresh_info 的生产 DeviceConnection 在 network_start 写入前失败；刷新后同命令被接受。
- H5：等待和冷却放大延迟。预测：HTTP 尾预算约 30s、完成后 55s 冷却与旧日志再次联系时间吻合。

## 状态

已查到首份 2026-10-12 归档 batch553 的 overwritten gap：404–1802，共1399条。剩余首条1803仍属 wake632，uptime=1131658ms；这是启动相对时间，不是距10:09事件的时间。不能据此证明10:10已经覆盖，更不能证明未生成的关联/DHCP节点也在这个gap中。Bridge发送日志仍在。本机没有 retention_floor.json，未见本机归档裁剪造成这段缺口的证据。

确定性复现与源代码审查完成。此次只新增调查回归与记录；未改生产行为、部署 Bridge、刷写 ROM 或操作真实网络配置。

## 结论一：日志为何没能留存

- batch553 的 ROM 字段为 `0.18.83-note4`；文件本机落盘时间10:28:46。gap 属于 generation `8f64e9f9ba8f43cfa9cfab57caa2e0f6`，1399条是该代际整个未交付缺口，不能全部算作10:09–10:10两分钟的日志。10月5日 batch150 是另一代际/旧ROM，不能跨代际推算覆盖时长。
- `src/v2_sync.cpp::syncAppend` 的 4096B 环满时删除最旧整条，不根据 ackedSeq 阻止覆盖。用生产源文件离线写200条未确认文本：仅保留132–200，131条覆盖，used4071/4096；CRC逐条读取及RTC recover均通过。这是容量策略，非环损坏或本机归档删除。
- `bridge/crates/app/src/sync_diagnostics.rs::transfer_with_budget` 先认证 HTTP status，成功后才 begin。`src/v2_sync_protocol.cpp::freeze` 只从 sync_begin 创建不可变Flash批次；未到begin便网络失败时，普通记录只留RTC。已冻结文件可续传，但冻结前的有限环不能保证离线无损留存；这是当前设计对“未确认保留”的实际边界。
- 文本只采集 syncDiagnosticText 白名单。GOT_IP handler 只清除断线原因，正常关联/取得IP没有独立文本事件。`rendezvousWifiTick` 成功分支也未补 TRACE_STAGE_WIFI_IP，因此会合成功的阶段时间存在观测缺口；普通 pollWifi/startNormalMode 的标记不能覆盖所有会合路径。
- 实际仍找回一条 wake632 的48B摘要：diag seq1807，wake generation2534412038，awake1137141ms，result=answered/error=0，transport=ble+wifi，阶段包含BLE和sleep，**未包含 wifi_ip/http 时间**。两层CRC/FNV验证通过。它证明不是整醒次全丢，却不能还原用户观察的约一分钟关联/DHCP过程；阶段缺失也不能当作从未联网。
- Bridge侧保存遵循先fsync页、再ACK和完成；本机75份文件未达128批阈值且没有retention_floor标记。实际设备明确报告 overwritten，是本次直接的丢失证据。Bridge当时发送日志仍可查询。

## 结论二：为何没能立即启动网络

- 旧Bridge日志：10:09:10.414认证wake632；11.690 Plan3657 applied/remaining415s；12.592 wifi_status ACK失败；12.609 network_start仅 characteristic found，无完整written/ACK；41.470会合以http_or_transport失败。10:10:42.200同wake再次认证，重复该模式。
- 生产 DeviceConnection 用初始 INFO 的 rf_remaining_ms 固定本地截止；设备 applyV2Plan(need_http) 将等待窗口扩至最多30s。旧调用方未刷新，因而随后仍被初始短截止限制。
- 新差分回归 `network_start_requires_refresh_after_plan_extends_rf_phase` 直接调用生产DeviceConnection：Fake初始700ms、Plan后30000ms；建立status会话并ACK Plan，等待800ms后 network_start 返回 `RF window expired before write`，设备没有收到该操作。同MAC/wake refresh_info后，同连接 network_start accepted，设备仅收到一次启动命令。与已有生产ble_cycle的正常/ACK丢失/写入未知/RF未延长四边界一起通过。
- `command_with_id` 在写入前RF到期、write timeout外层 `?` 以及循环顶ACK到期分支没有细分失败日志。network_start调用方又把任意Err归为unknown，连明确“发送前失败”也等待HTTP证明。旧记录缺错误阶段，故**历史只能确定没有整帧发送成功/接受证据，不能从日志排除分片写入超时**；源码可复现的旧RF路径是最有证据的解释，不能冒称保存了10:09的具体异常文本。
- HTTP尾预算30s，BLE主循环在本轮返回后开始55s同设备冷却。10:09:41失败→最早10:10:36重试→10:10:42认证，与实测约92s联系间隔吻合。该时间是Bridge重试间隔，**不能直接解释用户报告约一分钟的Wi-Fi关联时间**。
- 固件明确等待独立 network_start；Plan/退出zzz并不代表联网命令已接收。物理恢复窗口仍有效时，未收到启动的会合结束后允许一次既有配置fallback，这提供“稍后能连上”的另一合法路径；当时是否走了fallback没有留下关键事件，不能定死。

## 后续缺口（统一归 B6/C8）

1. RF刷新修复已在本轮此前部署并通过真实BLE读取/HTTP核验；本次未再次部署。
2. 仍需补齐发送前失败/写入未知/ACK未知的日志与调用方分类，明确未发失败应立即终止HTTP等待。
3. 仍需定义离线失败摘要的有界保护策略，并记录启动接受、关联、IP、HTTP就绪及结束原因；网络不可用时不能只依赖之后HTTP归档。容量/Flash寿命和资源成本应先核算，不以无限日志或无限重试解决。

## 验证和证据

- `artifacts/bridge-macos/wake-network-20261012-1009-safe.log`：旧Bridge脱敏时间线。
- `wake-network-20261012-archive-safe.json`：首份恢复归档的gap与记录边界。
- `wake-network-20261012-wake632-safe.json`：剩余醒次摘要，仅阶段/时间/身份，无凭据。
- `wake-network-log-20261012-repro.cpp` / `-repro.log`：生产RTC环确定性覆盖及恢复检查。
- `wake-network-start-20261012-regression.log`：2项测试通过（其中既有ble_cycle包含4种边界），87项未在本次窄回归运行。首轮新用例漏status建立会话而失败，补齐生产前置步骤后通过；不是生产回归失败。

本次所有检查离线/read-only，未用合成结果替代10:09丢失的历史事件。

## 用户追问：两小时目标、4KiB是否固定、能否用空余Flash

原始用户目标已找到：`docs/history/workflow/sleep-modes/prompt.md` 任务4及同目录status.md §8.3明确120条、每条约12B、约每分钟1条→约2小时，使用RTC避免Flash写入。该计算针对旧精简切换历史，不适用于后来统一的文字/摘要诊断流；本次不能把“仅按字节容量覆盖”当作已兑现两小时目标。

当前wake_summary为48B载荷+24B封装=72B，4096B最多56条；即使每分钟只有一条也仅约56分钟。真实batch553含58条/4045B，平均69.7B；文字占3541B（87.5%），其中BLE session20条、UI network14条、Wi-Fi link6条。当前采集组成已超出旧12B历史的计算。

batch556为同wake640的连续重试样本，33条/2393B，首末uptime差567.1s（9分27秒）；相同写入率下4096B覆盖约970.7s（16.2分钟），两小时需约30382B原始记录。此为该样本速率外推，不能保证所有场景、不能把跨wake累计uptime跨度当墙钟。完整脱敏统计 `artifacts/bridge-macos/log-capacity-20261012-safe.json`；未查看或输出凭据。

4KiB是当前软件定额：SyncRtc.bytes[4096]、v2_sync.cpp的CAP、协议冻结/Base64上限以及RTC资源预算共同固定；不是Flash可用容量，也不是硬件规定只能存4KiB。增大RTC环需要同源修改和匹配ROM map/资源检查，不能随意分配无限RTC空间。当前固件不会按空闲Flash自动扩容。

可以用设备LittleFS空余Flash延长离线保留。当前已留40KiB给最多16KiB冻结文件、替换副本及元数据，却只在HTTP sync_begin之后持久保存，不连续保存普通日志；这是旧低功耗/写入次数取舍，没有本次Flash满导致丢日志的证据。将日志按批追加到有上限的Flash段、保留短RTC缓存并处理读回校验/掉电恢复，可解除“没网络就无法保留故障现场”的耦合；应统一现有流/游标而非再建另一套history。可用空间属于LittleFS分区，不能任意挪用OTA槽/其他分区。容量须按实际记录速率和异常余量计算，重验两小时目标、RAM/栈/Flash写入成本；本次仅调查，没有实现该策略。

## 审查边界

用户“约一分钟后已连 Wi-Fi/light-sleep”为用户现场观察。Bridge未取得HTTP就绪证明不能反证设备未关联路由器。旧 network_start 的提前退出没有细分错误日志，因此不得把源码可复现路径写成已保存的历史错误；正常日志筛选与实际覆盖分别记账。

## 用户追问：为何10:10刚发生时就查询不到（更正）

先前回复把“当时不可读取”和“后来的设备覆盖”合成一个原因，证据不足，应更正：**没有证据证明10:10已发生即时覆盖**，也没有记录能测得这次1399条gap的起始墙钟时间。后一个异常样本估算16分钟不能套为10:10的固定超时时间；环按新增字节淘汰，不按事件年龄删除。

时间线与入口：

- 09:57:59已认证同wake632，10:09/10:10仍是同wake。首条剩余记录的19分钟uptime不能当作“10:09后过了19分钟”；前轮推断这一点不成立。
- 10:09:41、10:11:14会合HTTP失败。当时Bridge日志仍有完整身份/Plan/ACK/HTTP超时记录；没有“本机所有日志突然消失”的证据。
- MCP设备分栏的 `sync_diagnostics` 返回缓存sync读数与 `sync_diagnostics::summary`；后者直接读本机checkpoint和已下载JSON，不发设备实时环导出请求。查询未出现这次诊断只说明本机尚未收到，不证明设备内没有。
- 实际采集函数transfer_with_budget必须先认证HTTP；HTTP失败便不能begin冻结/分页归档。BLE身份可联系不等于HTTP诊断可读取。已有history摘要路径在sync启用时不作常规备用；本次没有当时的设备环导出或串口捕获可判断即时内容。
- 新归档batch553在10:28:46落盘，10:28:49 sync-v1 transfer complete。这时的gap证明恢复取到该批之前存在覆盖，但不能倒推10:10查询瞬间已丢失哪些记录。
- 此外，发送前RF到期/write timeout及会合IP成功缺日志/阶段标记；这些事件即使及时取环也不一定有详情，是“没有写入”，应与“已有记录后来覆盖”分开。

结论：当时查不到的已证直接因素是读取/归档通道失败与关键事件未记录；后续覆盖使追溯更困难。不能把环容量当作即时缺失的充分解释。安全时间线 `artifacts/bridge-macos/log-query-timeline-20261012-safe.log`。仍需在UI/MCP明确区分未采集、缓存过期、读取失败与设备报告gap；目前归档摘要不能替代实时可用性证明。

## 用户手动开启Wi-Fi后的直接取证（11:40–11:43，中国标准时间）

用户要求趁已启动Wi-Fi调查。先通过MCP platform_status_refresh认证读取，再用登记IP的恢复视图核对MAC，立即读取/log、/history和/pmstats。GET /api/status 返回404，核对源码确认业务status实际使用POST/compact frame；认证已由生产MCP客户端完成，随后GET诊断目标用/status.json再次核对。/history是数组（不是BLE history分页对象）；首个脚本已把数组保存，打印摘要时类型判断错误，修正摘要后继续补采pmstats，现场数据没有因此丢失。

### 当前现场事实

- MAC7C4FADB93408、IP192.168.31.90、ROM0.18.83-note4/ota_0。认证status result=ok，Wi-Fi connected/RSSI=-28，Bridge reachable/error=null，wake generation2534412038、wake675、reset_reason=8。恢复视图明确reset=deep-sleep；不把boot_id变化误记为panic/冷复位。
- 诊断generation仍为8f64e9f9ba8f43cfa9cfab57caa2e0f6，与batch553相同。live earliest_seq1962/next_seq2024，last_completed=batch560 through1964，当前due=false/pending=null。因此旧1803等记录已不在当前环；这次手动联网没有将诊断流清空或换代际。
- /history保留22条，wake654–675，末条是当前active按钮醒次，complete=false。旧wake632摘要只能从batch553读；当前设备已不能重新导出被覆盖的404–1802。当前活跃摘要同样已单独保存，不等它进入deep后才取。
- wake675的第一阶段时刻：BLE advertising400ms、BLE off4400ms、Wi-Fi IP6600ms、HTTP结果7700ms；没有BLE connected阶段。首次command阶段38100ms。Bridge11:40:27开始通过HTTPACK Plan3679，后续剩余264→230→196→162→129→95→60s递减，不延长到期。手动联网实际先于Bridge正式Plan/GATT自动交接，证明当前已保存网络能在该路径快速取得IP；不说明10:10的关联也同速率。
- 11:43恢复视图uptime196s、light_sleep_ms137296/count8405，pm_light_sleep=true；属于实际累计light-sleep证据，而非只看mode字段。

### 新查到的日志容量浪费

直接/log共37条文本，其中11条完全相同的 `[ble-wire] chr=info bytes=0 max=512 result=precheck_failed`，22条BLE session开/关，2条pcf8563 short_read，2条UI网络状态。

源码原因：每次冷/深睡启动后infoProvider/infoExtra为初始值；radio::enterBleOn先bleBegin，bleBegin创建INFO后立即refreshInfo，调用方此时还未安装provider/updateInfoExtra，snapshot.mac为空；bleBuildInfo明拒空MAC返回空串，writeAttribute遂记precheck_failed。其后provider设置与刷新可恢复正常INFO，当前多次真实MAC认证也证明这类启动噪声不能等同整个BLE会合失败。应调整初始化顺序/避免在未就绪状态制造错误事件，而非关闭所有错误记录。

11条×(58B文本+24B封装)=902B，约占4KiB容量22%。batch553也含3条同样文本/246B。它是可证的额外占用，不足以单独解释整个1399条缺口，也不是10:09 network_start未确认的直接原因。pcf8563 short_read为另一个现场异常，当前网络正常，未有其导致10:10失败的证据，不扩展为已证根因。

### 结论和边界

手动路径已真实联网/HTTP/light-sleep；旧自动路径的最有证据解释仍是Bridge旧RF截止发送阻断，已生产差分复现且此前修复部署。此次没有复现旧失败，也没有新增设备写入、Flash冻结请求、PowerPlan、复位或ROM/Bridge部署；不能通过如今开启Wi-Fi恢复本已覆盖或从未记录的旧事件。即时缺失语义更正仍有效，旧用户约一分钟后联网的具体路径无法从现存字段唯一识别。

所有本次现场文件位于artifacts/bridge-macos：`manual-wifi-20261012-{status-safe.json,device-log-safe.log,history-safe.json,pmstats-safe.log,resources-safe.json,bridge-safe.log}`。密码/token/nonce/owner等未输出或写入这些证据。

## 用户追问：超时分类与窗口是否过短

结论不是统一加长所有超时。已证窗口缺陷在旧Bridge沿用过期RF截止；HTTP等待失败、AP未找到与同步预算收尾必须分别报告。

| 事件 | 证据 | 分类与判断 |
| --- | --- | --- |
| 3s扫描无候选 | 10:09:06、10:10:39扫描结束，之后很快发现并认证同设备 | 正常有界发现轮次结束，不是业务命令已发送后超时；这些样本不能证明3s不足。 |
| 400ms HTTP预探测 | rendezvous.rs先测试现成HTTP，失败后继续BLE | 正常通道选择；设备尚未联网时不应以此要求400ms覆盖关联/DHCP。 |
| 10:09/10:10 wifi_status最后一页ACK失败 | 前六页ACK分别61–91ms、61–152ms；第七页只等19ms、16ms。Plan已确认remaining415s、323s | 异常，剩余RF预算提前截断。与过期INFO截止源码及差分复现吻合，属于Bridge实现窗口错误；正式Light剩余时间不能替代RF阶段预算。 |
| 同两轮network_start及30s HTTP证明失败 | network_start仅find，无整帧written/accepted；随后等HTTP至10:09:41、10:11:14 | 异常下游等待；没有证明网络启动已成功，也没有证据30s本身不足。明确未发送应立即失败，部分发送/丢ACK才保留结果未知并核验。历史启动具体失败分支仍未知。 |
| 10:22:05 SyncOpen ACK超时 | 整帧written92ms，随后等待3844ms未确认 | 异常、写后结果未知。不同于16–19ms案例，不能只据本条证明ACK窗口太短；丢ACK/链路关闭与预算截断需要进一步区分。 |
| wake638两次约15s Wi-Fi尝试失败（10:22之后） | batch553：第一次启动/connecting11984/12074ms，reason201=16938ms，失败27868ms；第二次102070/102140ms，reason201=104988ms，失败117924ms。wifi_setup.cpp将201解释为network_not_found | 入网失败先于最终截止出现，不能简写成等待超时或断言15s太短。当前手动675取得IP6.6s/HTTP7.7s，638后续尝试也约6s成功，仅说明已有快路径；不排除其他环境需更长，不倒推10:10。 |
| wake638 sync 90s window exhausted | batch553 seq1846/uptime124783ms；application.cpp总截止90s或无进展90s共用该日志 | 有界失败收尾保护功耗，业务同步未成功。本样本前面已出现AP未找到，没有“持续有效传输被90s截断”的证据，不判主要窗口过短；日志仍须区分总预算与无进展。 |
| BLE session off (rendezvous deadline) | 当前多轮普通BLE醒次answered/error0、约3s、无HTTP任务 | 正常会合完成后的关无线；若仍有已准入未完成任务，同样文字应判异常，不能按文本一概归正常。 |

另55s冷却不是命令超时。从上轮约31s失败收尾开始再冷却55s、加扫描，实测同wake再次认证约92s；它会放大自动重试延迟，不能当作Wi-Fi关联用时或HTTP响应时间。尚无证据将用户10:09–10:10观察的约一分钟唯一归因于冷却或fallback。

Bridge具体页与SyncOpen时间保存在`artifacts/bridge-macos/timeout-classification-20261012-safe.log`。本轮追加证据分类与记录，未改生产超时、启动设备、部署或扩大测试；此前生产客户端差分回归已覆盖过期RF截止，不能代替其他未知原因的实机证明。
