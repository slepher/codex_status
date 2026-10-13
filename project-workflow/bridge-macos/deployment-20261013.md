# 2026-10-13 配套 ROM / Bridge 部署

用户授权：存档当前 ROM 和 Bridge，然后先刷入新 ROM、再部署新 Bridge。目标仅 Note4 `7C4FADB93408`；不提交 Git。

## Plan / Task

- [x] 核对登记MAC，私有存档旧Bridge/data/安装身份及本机`.85`包；真实`.83`BIN本机缺失，运行哈希已验证，旧OTA槽保留。
- [x] 修复WFS4和旧绑定读取兼容，不要求USB导出或配置迁移；不清空设备配置。
- [x] 构建Note4与Bridge，核对marker/大小/SHA256/签名/匹配栈及回归。
- [x] 用户BOOT就绪后直接原生OTA `.99`，认证运行哈希；部署Bridge并完成数据恢复补修与实机ACK。
- [x] 更新交接与review，保留最大NVS容量/掉电等尚未验收边界，不结整个P0–P6。

## Status

已授权部署。起点为运行中的旧 Bridge 与 `.83-note4`；本机候选 `.95` 不是本轮源码产物。此前完整 P0–P6 尚未结束，此次设备部署不等于项目结项。

已完成本机存档和候选构建，未刷入/替换。旧服务停机后复制完整data、安装身份、plist及旧App，存档位于用户私有 `~/Library/Application Support/dev.codexstatus.bridge/backups/protocol-deploy-20261013/`，manifest逐文件SHA256校验通过。旧Bridge 20,850,560 B，SHA256 `fc792b2b2016181550b7629b71846c66eba0aa8f69acd0378bbc1f85ef62acf7`。

用户询问已有包是否可复用：可以；但记录的 `.83` 包未在此Mac找到。另存现存 `.85` 候选BIN/ELF/map，BIN 1,851,104 B，SHA256 `8c4126cfec33c18073c11639c8d5c90ddd5c2071eb73b6119c997cbcba8fecad`；ELF marker核实 `.85`。这不是当前设备 `.83` 的镜像备份。

新Note4 `.96` 构建55.44s、RAM119932 B、Flash1782959 B；ROM/ELF/map及哈希见 `artifacts/bridge-macos/protocol-deploy-candidate.json`。62项栈/旧入口门禁通过；新增endpoint实测静态帧：核对64 B、handler336 B、回执编码176 B（不代表完整嵌入式动态峰值）。新Bridge release构建3m58s、签名验证通过。独立只读程序使用生产PersistedState解码存档state.json通过，不改写运行数据。

部署前缺口：USB仍未枚举，无法读出设备WFS/BRS并转换核验；用户已获解释。`.83`对应源码写WFS4，候选只读WFS5。变更是新增整库及回执的executionGeneration，SSID/密码/网络记录编码未变；删除旧reader是单独的兼容策略，不能将未完成迁移转嫁为默认重配网。用户未要求清空配置，不执行擦除。候选保留，已启动恢复原路径旧Bridge。

## Review

本机存档校验、新ROM构建/栈和新Bridge签名完成；设备当前ROM精确存档、WFS/BRS迁移、烧录及新配套认证运行仍未完成。没有Git提交。证据均在 `artifacts/bridge-macos/protocol-deploy-*`，旧服务恢复后的健康核验记录同前缀。部署顺序继续为先ROM后Bridge。

## 用户后续修订

用户要求配置与现状分开，明确选择独立存储配置和事务信息。原WFS4→WFS5迁移安排被[拆分专项](wifi-config-state-separation.md)替代，`.96`候选不再用于部署；`.97`候选保留WFS4配置读取。此前USB/WFS迁移说明是修订前的判断，不继续作为重新配网或必须导出Wi-Fi的理由。BRS身份核验、真实旧ROM存档与现场部署证据仍需完成。

## 15:41 直接 OTA 顺序核验

用户明确刷机跳过 Bridge：候选准备好后暂停旧服务，直接调用设备原生 `/doUpdate`，核验新 ROM 运行身份后才替换/启动新 Bridge。不通过 MCP/Bridge OTA 队列。

直接 `/status.json` 核对 Note4 MAC、`.83-note4`、当前 `ota_0`/下一 `ota_1`。原生 `/update` 使用设备 token 返回200；生产客户端直接认证 status 与 ota_image 成功，运行1,837,936 B镜像SHA256 `32486c0763db240777758f549bce386f8884a09547ad3bc89f4423e499664517`，与历史部署记录一致。第一次组合探测镜像接口401，改用该接口所需设备token后通过；业务status使用endpoint token通过。证据：`artifacts/bridge-macos/protocol-direct-{before.json,ota-preflight.json}`。

设备报告2条endpoint；当前调用者principal/namespace存在，但远程status没有全部BRS原始记录，不能据此证明另一条符合候选完整身份约束。USB未枚举，BRS核验尚未完成；这与OTA是否经过Bridge无关。未上传ROM、未替换运行应用，也未改设备配置。

用户补充：刷机准备完成后先通知，由用户点击BOOT并启动light-sleep；收到就绪回复再开始直接OTA。等待不视作已就绪。

15:45 当前源码Bridge release重建通过（6m04s），候选20,739,488 B，SHA256 `a619e66bafe3ff566582ecacf7e576a95202c5d2b4bfbc18e16c3792a3321425`，签名核验通过；与此前候选二进制一致。未替换运行应用。manifest为 `artifacts/bridge-macos/wifi-split-bridge-candidate.json`。

## BRS部署前核验准备

复核`.83`历史读取器允许无principal的BRS2记录，`.98`生产读取器要求每条完整身份；任一旧记录缺身份会使整库拒绝，因此两条endpoint及当前调用者认证成功不足以放行。现有HTTP/BLE接口无完整BRS导出，当前仍无USB串口；已请求用户接数据线作只读核验，不是要求重复打包或升级Wi-Fi配置。

已构建 `artifacts/bridge-macos/brs-preflight`，直接包含当前生产 `bridge_store.cpp`，只读取提取的records2，报告接受数量/拒绝，不输出凭据，不写输入。生产 `test-bridge-store.cpp` 回归通过：完整绑定、双身份、旧/缺身份拒绝、字节保留、损坏/截断、容量。NVS解析复用本地ESP-IDF `nvs_partition_tool`，不自制解析器。等USB现场后先核对MAC/分区、私有导出并验证；不因候选可构建而直接刷机。

## 用户修订：刷机不以绑定库导出为前置

用户明确要求解决固件兼容问题，撤销必须USB导出BRS的前置要求。实现计划：
- [x] 保留边界/CRC有效的旧BRS2及逐键记录，读取不改写；缺身份记录只保存，不作为当前授权或投递端点。
- [x] 有效记录在混合库内继续认证/核对/更新，不能被另一条旧记录拖垮；损坏、未知格式、重复身份/凭据仍明确拒绝。
- [x] 旧条目只有匹配凭据/来源及物理绑定修复时才补入经过验证的身份；保留已存在namespace，缺失时签发新namespace，不从peer推断历史事务身份。未修复记录不删除。
- [x] 生产存储回归覆盖混合库、重启、只读零写、修复权限、失败保留；Fake/BLE回执链回归，构建匹配ROM及栈预算。
- [ ] 完成后通知用户BOOT/light-sleep，等就绪再原生OTA，确认后部署Bridge；无Git提交。

Review：实现与宿主测试通过。读取支持有界逐键旧记录和BRS2缺身份记录，完整记录认证/路由不受影响；storeGet与storeAuthenticate统一隔离不完整记录，避免诊断/通知/BLE旁路。物理修复要求已有token、匹配来源与无身份冲突；已有namespace保留，缺失才使用设备新签发namespace，不猜旧值。读取零写入，完整绑定只读核对零写；旧逐键记录仅在新的完整BRS2写入并读回验证成功后退役。未知格式、损坏、超长/截断、重复非空身份或token继续明确拒绝。USB导出不是正常升级条件。

生产存储ASAN/UBSAN通过，覆盖3类缺身份混合库、多个空身份旧条目、有效绑定更新时逐字节保留旧记录、物理恢复/跨principal/token/MAC拒绝、写失败和提交未知后只读核对、9处分配故障。Fake真实客户端endpoint关联ACK与BLE endpoint单测通过（2项），原生OTA生产handler鉴权/目标/繁忙/写失败/断连/超时/重试通过。日志 `brs-compatible-asan.log`、`brs-compatible-endpoint-tests.log`、`brs-independent-ota.log`，均在artifacts/bridge-macos。

BRS2有效载荷仍为空库0 B（显式编码空库16 B）、1条108 B、典型2条187 B、8条697 B、全字段上限8条2856 B；没有新格式或配置副本。仅在显式变更时重新写BRS2；ESP NVS写时旧/新blob临时共存的有效载荷上限5712 B，索引/GC另计。RAM按实际条目分配，最多旧/候选两组及编码/读回两块，单块最多4096 B，新增旧字符串读取缓冲129 B；宿主上限更新峰值增量14304 B含HAL，不作为ESP实际堆。分配失败返回有限错误或提交未知，未改写Wi-Fi仓库。

`.99-note4`构建55.90s，RAM119948 B、Flash1788039 B，匹配ELF64项栈/退役入口门禁通过；storeUpsert帧128 B/局部链752 B，storeAuthenticate64/688 B，legacyString192/304 B。真实NVS容量、堆和Flash掉电仍待硬件验收，不据宿主结果宣称通过。

当前候选ROM `artifacts/bridge-macos/note4-0.18.99-bindings-compatible-candidate.bin`，1,839,968 B，SHA256 `e8be15320a46efe2b930dccfa4c73fb489a04cd6126c00eeab4e261d40e477dd`，marker/ELF/map在`brs-compatible-candidate.json`。`.98`被替代。Bridge产物不涉及本次固件绑定库修改，沿用15:45核验的配套候选及签名。尚未刷机或替换Bridge。

## 16:32 原生OTA与第一版Bridge部署

用户确认BOOT/light-sleep就绪后，直接核对登记MAC及`.83/ota_0`认证镜像，暂停旧托管Bridge并补存data（132文件校验），一次原生`POST /doUpdate`上传`.99`，HTTP200/UPDATE OK。认证运行已为`.99/ota_1`，实际SHA256 `e8be15320a46efe2b930dccfa4c73fb489a04cd6126c00eeab4e261d40e477dd`完全匹配，reset_reason3（软件重启），endpoint仍2条。旧`.83`槽未写，未接USB、未擦NVS、未改Wi-Fi配置。

新Bridge a619e66b…安装原路径，旧应用另保留Codex Status Bridge.pre-99.app，托管PID87248，health204/MCP43，正式platform_status_refresh认证成功。Codex数据源good、18字段（含primary/weekly配额），owner匹配本Bridge，内容ready。Wi-Fi仍为原WFS4混合库634 B/4网络/2回执/2游标，revision265、ready且无uncertain；首次真实持久变更才拆分，未为验收制造配置写入。

验收另外发现：设备OTA后同context数据序号归零，Bridge仍保留last_applied_seq565及旧ACK指纹，push_now返回idle。ROM与Bridge部署已完成，但完整快照恢复尚未验收；正在补服务层回归和修复，不以采集good代替投递成功。证据`brs-compatible-direct-ota.json`、`brs-compatible-live-after.json`、`brs-compatible-deployment-verified.json`；完整接口响应位于原私有存档。

### 数据恢复补修计划

复现依据：新ROM认证data_seq/applied_seq=0，安装context未变；Bridge持久context仍last_applied_seq565、旧ACK指纹，完整同步期限未到，手动push返回idle。已排除采集失败（good/18字段）、投递关闭（sync_enabled=true）和合同未知（认证16字段）。最小服务测试模拟ACK→Bridge重启→设备同context序号归零，要求完整重推、序号不回退、未知in-flight保留、PowerPlan/确认期限不变。修复仅使设备已丢失的确认基线失效，不重发Bundle或更换Wi-Fi事务ID；通过后重新构建Bridge并替换。

数据恢复最小回归已先红（expected light_data, got idle）后绿；修复在认证状态同context且设备序号低于已确认序号时失效旧ACK指纹，保留in-flight/next_seq/PowerPlan/full_sync_deadline。测试模拟Bridge重启前后、重复读取不换未知快照、新ACK后回到idle。Core152、集成22通过（ota-data-recovery-core-final.log），App93通过/3条件忽略（ota-data-recovery-green.log）；首次测试冷启动可能先补一次缓存的前置已修正，生产逻辑未改变。release正在构建。

## 最终部署 Review

2026-10-13 16:46：`.99-note4/ota_1`继续正常运行，运行镜像哈希与候选完全一致。最终Bridge release 3m34s，20,739,488 B，SHA256 `237cd5a03f6474fdf06f26addee7997ae3a7cf0b65cfb4980218b4be2f5087ab`，签名通过，原路径托管PID88781，health204/MCP43。替换前再次私有备份133文件并校验；旧版本分别保留在原私有备份和本地.pre-99/.99-initial应用包。

最终Bridge启动自动补发后，认证data_seq/applied_seq从0到567；随后正式push_now收到569的设备ACK `applied/displayed`，Bridge确认同为applied，最终认证状态仍569/displayed/content ready。Codex good/18字段，error为空。Wi-Fi配置与事务仍按原WFS4读取且ready；未制造配置变更来强制拆分，没有重新发布Bundle、清NVS或再次刷ROM。最终证据 `brs-compatible-deployment-final.json`、`ota-data-recovery-bridge-deployed.json`，完整ACK/状态只放用户私有备份。最大容量/真实Flash掉电、其余P0–P6仍未完成，不能据此宣称全面发布验收。未Git提交。
