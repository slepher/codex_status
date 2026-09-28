# 执行任务与可执行验收断言

状态：2026-09-28设计定稿；下列测试尚未为sync-v1实现或运行。待办总入口docs/roadmap/backlog.md C8；本文件是该条目的执行分解，协议字段/参数以design.md为准。

## 模块实施顺序

- [x] P0 定稿Bridge→设备HTTP、端点与状态合同、Fake ROM职责。
- [ ] P1 同源diag v1/4096B环/CRC/gap/脱敏；LittleFS冻结文件与双元数据；接FFI和Fake ROM。
- [ ] P2 认证能力/sync_config；BLE轻量投影；完整状态来源和持久缓存。
- [ ] P3 begin/page/ack/complete；Bridge连续持久cursor与serial；故障切点、runner同步调度。
- [ ] P4 sync_open与15轮计数；light入/离场；异常/低电/退避及跨MAC。
- [ ] P5 OTA arm/ticket/运行分区hash；Bundle确认；三次有限确认重试。
- [ ] P6 UI、兼容、S01–S14及可重放24h软件出口；相关总设计/PROGRESS更新。
- [ ] P7 仅后续授权后执行H01–H04所需最小硬件检查。

新增纯协议逻辑集中src/v2_sync.*，沿现有render FFI接宿主。不要在device-sim Rust复制counter/ring/幂等决定；硬件读写适配的行为与异常单独标注。现有snapshot/owner/Plan/Bundle逻辑继续复用，不为本专项全面重构main。

## 测试公共设施

运行前隔离目录提权预建；固定虚构LAA MAC A=0200000000A1（154g）、B=0200000000B2（Note4）、独立端口/token/data。控制面凭据与业务token分开，日志不输出secret。隔离Bridge使用已有协作模式，真实目标时钟不被推进。新测试位置优先现有device-sim/tests/bootstrap.rs、core/app模块测试及tools/fake-rom-runner.mjs场景；慢集成案例可拆有名场景文件，不另引入框架。

所有场景记录seed、设备/Bridge单调和wall时钟、boot、diag_generation、wake/record/batch/client_serial、ACK offset、deadline、业务副作用数。模拟控制面可作断言oracle但Bridge禁止读取它。页面/文件断言基于真实字节和fsync后的文件，不只看内存JSON。

每个S类别至少一个正例和一个可区分错误实现的负例。不是对所有错误做笛卡尔积。需要故意等待/丢包的场景须声明expected failure；runner不许将其整体skip，须断言随后恢复状态。

## S01 逐轮计数和BLE开网

从一次真正complete后的rounds=0开始，逐事件执行14次deep timer会合；每次i断言rounds=i、周期Wi-Fi建连数=0、日志/image请求数=0。第15次认证后Bridge发sync_open一次；正式plan deadline不变，完整成功后rounds=0，实际HTTP服务关闭返回deep。

第15次无BLE认证：rounds=15/due=true、Wi-Fi数仍0；下次有效会合才开。thin唤醒与同窗重复连接不增加；重复open_id不会重复建连。以wall推进15分钟但零会合计数不动，反向证明不是15min定时器。

## S02 light入口、离场与触发合并

先完成14轮，发真实light Plan，入场批次reasons含light_enter；complete后0。期间重复Plan不产生新入场批次、不续deadline。到期离场产生light_exit，完成后deep且0；下一timer为1。

第15轮与light Plan同窗，仅一次Wi-Fi连接、一个含periodic+light_enter的批次。已有旧批次时先完成旧serial，再冻结离场新快照；不能用旧batch清掉后来light_exit义务。

## S03 有进展慢批次不设短总截止

冻结已知H，limit=64形成>16页；每10s完成一页durable ack，总时长>原light剩余和120s。每一步断言原正式deadline完全不变、phase=WIFI_SYNC_ONCE且effective_radio_reason=sync_drain时仍能页传输，完整前deep次数=0。冻结后生成20条日志，所有seq>H不出现在该文件，文件sha/bytes不变。complete后才退出；后续批次包含新增记录或显式gap。

重复同页/status90s而无acked_offset增加：必须进入sync_incomplete，不被假进展保活。这是异常测试，不把持续推进的正例套总时限。

## S04 ACK丢失、幂等与cursor

在part已fsync但ack未到设备、ack已到但响应丢失、complete收据落盘但最终ACK丢失三个切点注入。逐一恢复同batch；确认文件字节无重复、durable_offset不跳过未存页、最终同IDalready_complete。

完成后跑2轮，再重放旧complete，rounds仍2，防二次归零。同ID错误hash/bytes拒绝且收据不变；同record键不同字节报protocol_conflict。serial回退stale_serial，不能新建伪批次。

## S05 断链、失败退避和无进展

关联失败三次（15s/次、1s/3s间隔）后本醒次不再开网，pending和due保留。连续HTTP失败达到3次中止；成功请求但无持久页进展90s同样中止。

第1/2/3次失败分别跳过1/2/4个完整自然窗口；消耗skip至0的本窗口仍不重试，下窗才可开。成功后failure/skip归零。新按键绕退避只一次，仍遵守失败上限。断链恢复从durable_offset继续，不自动创建新batch或重刷业务。

## S06 Bridge存储和进程退出

注入part写失败、fsync失败、checkpoint原子替换失败、最终文件写成功但任务/完成意图未落盘。任一未满足完整持久条件不得发送complete；cursor不得超前。

至少一次在页持久后直接kill隔离Bridge（不执行正常清理），重启从同data恢复同serial和文件前缀；另一次在完整文件持久后、complete前退出，恢复仅补完成确认。已声明持久文件后来人为损坏必须archive_lost，不要求设备重造。

## S07 设备恢复域与真正硬退出

deep保留generation、rounds、ring；RTC校验无效建立新generation/unknown due，不能把随机内存当日志。冻结文件跨设备进程kill/restart仍可续原generation，新环新事件留下一批。

在blob临时写、blob校验后、元数据引用前、complete收据写前/后各选代表切点硬退出。只恢复最高有效元数据引用的完整blob；无完成收据不得假完成。收据已落盘时重复complete幂等，冷启动轮数unknown不能据旧收据假造精确计数。

## S08 容量、溢出、CRC与单一流

用相同4096B环写15轮最大正常事件夹具，字节<=3600且无gap。异常text超过96B仅一条truncated且UTF-8完整；持续异常覆盖时seq仍单调、gap可见。

begin后填满环，冻结文件bytes/hash不变，后续gap明确；已冻结副本回收不算丢失。CRC坏、header坏、>8 gap合并都给对应原因/代际。旧/log和/history均来自同流，不能出现未经过统一seq的文字。

静态断言RTC控制<=768B；blob<=16384；元数据各<=2048；40KiB预留的空间不足只能明确失败。双目标map门槛留授权构建检查，宿主malloc成功不算RTC通过。

## S09 OTA/Bundle正常确认及重启

真实Bridge先arm、带ticket上传有限目录镜像；pending_reboot落盘后丢UPDATE OK，设备重启开一次授权Wi-Fi，Bridge重新认证nonce/MAC，原job只上传一次。

确认endpoint超时首次+2次后保持pending，后续联系查询，绝不再上传。Bundle在commit后丢ACK、Bridge退出，恢复查询原job/commit_seq/context；只安装一次。display_state=failed时保留安装成功与显示失败，不能reported displayed。

## S10 精确镜像证据等级

目录含版本字符串相同但字节不同的X/Y。冻结X，运行Y，image返回当前Y前N字节hash，必须不能image_verified。运行X且MAC/target/N/hash全部匹配才image_verified。

请求错误N超分区拒绝；不同slot的暂存X不能代运行Y；缺能力维持version_seen_unproven/awaiting_confirmation。override到预期版本不给upload_ack；Bridge不读目录内部SHA。离线夹具证明算法覆盖N字节全文件，包括尾部。真实ESP分区读取和Flash加密语义只在H02证明。

## S11 双MAC与任务隔离

A有pending frozen+OTA，B联系100次并多次sync，切UI选择、复用旧IP候选；A的serial/轮数/游标/job/确认时间均不被B更新，A业务写数0直到A自身认证。返回错MAC页或image立即拒绝。

两MAC不同device/Bridge rate/offset，推进A不推进B；每MACdeadline、skip独立。Profile8项完整仍可循环，保存模板/Profile业务publish计数为0。

## S12 token、owner、session及低电

新端点无/错endpoint token为401；OTA/claim仍需device token。错MAC/session为400，owner空409 claim_required、他人409 occupied；全部业务副作用0。sync_open不创建owner，显式claim不延light。

传输期间lease过期先暂停/显式claim恢复；他人接管旧批次owner_changed，不向新owner交付旧blob。未插电battery=4触发原保护，pending不清、rounds不归零；battery=5不凭本专项新增门槛关机。模拟只证明软件判断。

## S13 能力组合及回退

新Bridge+旧ROM：缺sync_v1保留旧history，无任何sync端点写，UI明确未支持。
旧Bridge+新ROM默认disabled：原业务正常，无自动sync等待。
新+新：config落盘ACK才启用，普通BLE停止history。
同owner回退旧Bridge且原enabled：未完成遇90s无进展退出，不永久在线，后续新Bridge可续。
新owner必须重配；disabled不删pending。混合双MAC不得全局切能力。没有实际旧EXE时固定协议夹具标记为兼容模型，不宣称旧EXE实跑。

## S14 状态、UI、时间与脱敏

先Wi-Fi采样heap=0、BLE=false、templates=[]，再只BLE联系；原值保持，Wi-Fi采样时间不变，最近BLE时间推进。失败HTTP/Bridge重启不清旧值；省略字段不清，null+read_error明确不可用。

radio/runtime/power>120s标stale；新boot令旧runtime/radio立即stale；firmware值仍保留原时间。wall倒退显示clock_anomaly不移动单调截止。public status错MAC不能覆盖。UI不因刷新页面产生sync_open/Plan。

设备页无Codex配额；屏幕内容/模板/字体仅归属声明、数据页布局未改。哨兵Wi-Fi/AP密码、endpoint/device token注入全部日志生产者，RTC、串口镜像、HTTP兼容视图、batch文件、Bridge归档均搜索不到secret。

## 软件最终出口与硬件边界

- [ ] S01–S14代表case均有执行命令、确定输入和结果；无“整体skip后通过”。
- [ ] 双target Fake ROM 24h协作运行，实际经过第15轮/入口/离场和恢复；未消费I/O时不跳时。
- [ ] 同快照/seed至少1h双回放归一化trace、批次文件hash、帧CRC一致。
- [ ] Bridge与设备各至少一次进程kill/restart；确认生产Bridge/实机未触及。
- [ ] 相关测试与git diff --check通过；新能力未获实现证据前仍标待实现。
- [ ] H01 RF/GATT物理切换、H02 RTC/Flash/分区/bootloader、H03按键/面板按授权最小烟测；H04仅电池端测量可得功耗结论。
- [ ] 最终交接分别列设计、源码/软件验收、构建、实机部署及证据，不混用完成等级。
