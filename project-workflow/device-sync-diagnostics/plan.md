# 实施计划：Fake ROM为主要回归门槛

日期：2026-09-28。状态：设计定稿，P0文档完成，P1–P7未执行。协议唯一实施合同为design.md；待办入口为docs/roadmap/backlog.md C8。本文不是源码/部署完成记录。

## 授权与工作规则

文档定稿本身不是源码或部署完成证据。实施、构建、Bridge生命周期和实机操作遵循用户当前授权与AGENTS；目录创建提权、运行data保护、目标串行构建等约束始终有效。不撤销已有RF1及其它工作；每里程碑更新PROGRESS由当时负责者执行。

交付顺序是生产 sync-v1 → Fake ROM 完整接入与 S01–S14 → 设备 Tab（见 `device-tab.md` 的 U01–U20）。生产阶段内部按 P1→P5 实施；**共用协议入口和 Fake ROM 存储目录绑定提前到生产批次端点成型时处理**，避免先写两套 begin/page/ack/complete 再回头收敛。允许为该入口做小范围宿主调用检查，完整设备模拟与故障矩阵仍在第二阶段。新增的当前协议统一按 `protocol-unification.md` 执行：先固定一次性 wire/MCP/文档及双设备切换合同，在生产两端与 Fake ROM 同轮接线时消除 v2 字面，再继续 S01–S14；不在设备 Tab 阶段才回头改协议。P7只是硬件最小烟测，软件出口不等待完整实机故障矩阵。按需下一轮开网和bridge_first反向HTTP不在首版。

## P0 合同与基线（文档完成）

已确定Bridge→设备HTTP、BLE开网授权、第15轮due、4096B统一RTC流+有界Flash冻结副本、端点及持久ACK、light截止后的sync drain、失败/低电、运行分区前N字节证明、兼容和测试矩阵。

开工时读取AGENTS/最新PROGRESS，记录工作树、现有测试和D/E/F证据。核查引用源码是否变化；不因行号变动重新发明协议。协议不确定项已收敛，资源/IDF检查是技术门槛，失败需明确结果。

出口：task.md每条S断言对应实际执行路径；没有重复状态机或将硬件假设写成证据。

## P1 诊断格式、存储与同源核心

### Fake ROM接入顺序（2026-09-28只读接口复核补充）

以下是本专项必须实现的测试接线，不是现有能力。F1 中的共用协议入口及目录绑定随生产 P3 提前完成；其余 Fake ROM 接入在生产路径完成后按 F1→F6 执行，具体控制面请求和命令见 task.md“Fake ROM操作规程”。不改变 design.md 中的生产协议。

| 步骤 | 实际接点与交付 | 可以进入的验收 |
|---|---|---|
| F1 同源对象与保留域 | 先让生产 HTTP handler 与宿主适配调用同一个 C++ 冻结格式和 begin/page/ack/complete 协议入口；真实认证/硬件采样与模拟环境输入留在各自外壳。render/build.rs加入生产sync源；render/src/ffi.cpp和Rust封装导出初始化、输入事件、status、命令及RTC原字节保存/恢复。device-sim每进程一个实例，沿既有串行调用；不反序列化C++指针。同步存储开始前显式将 LittleFS 绑定该进程的 dataDir，验证两个独立设备进程不串目录；在此一进程一设备约束下不必重写全局store。deep/soft/cold明确传入生产恢复入口。 | 生产/宿主同入口定向检查；S07/S08编解码与恢复 |
| F2 设备面与BLE接线 | device-sim的app路由加入design列出的/api/sync/*；sim_ble_command当前只支持status/plan/data，须增加sync_config/sync_open并生成与固件相同的非200 BLE ACK。device_gate由单一power.light改为生产phase可达判定；reconcile_power不再直接用旧5s窗口代替生产同步转换。 | S01/S02/S12/S13 |
| F3 诊断、状态与镜像 | sim/state只读暴露sync/diag保留域、下一事件和计数；sim控制面注入文字/CRC坏/传感值，不直接设置rounds=15。现有RomVersion只有id/fw/target/size/sha256，须保存经校验的实际镜像字节及active槽选择，sync/image实时读取该字节域；不得返回catalog.sha256充当测量。 | S08/S10/S14 |
| F4 有效故障切点 | 扩展设备/sim/fault与存储接点，覆盖sync请求之前/持久之后/回包之前；Bridge隔离控制面也须能在part/fsync/checkpoint/complete意图处注入失败或硬退出。既有/sim/storage只接Bundle，不自动覆盖sync；每切点断言故障确实消费一次。 | S04–S07/S09 |
| F5 协作等待 | 扩展已有/sim/run控制路由的start/poll模式与已登记虚拟等待；runner能继续处理未来事件，而不是等一个未完成HTTP调用超过当前20s后退出。普通未知在途I/O仍禁止跳时。见task的明确控制合同。 | S03/S05/加速长跑 |
| F6 可复现启动与场景 | 复用bootstrap.rs现有Simulator/request/stop_preserving_data；补隔离Bridge进程句柄和sync_v1_s01…s14测试入口，不新增通用测试框架。保存实际执行场景配置、清理指定测试PID、恢复同data/时钟并运行1h重放/24h长场景。 | S01–S14最终出口 |

F1–F6按P1–P6所需逐项完成，不能“先用Rust伪造返回通过测试，未来再接生产”。若无法用同源入口执行某决定，该case标未覆盖并补生产接点。每阶段至少跑对应具名测试；只有局部HTTP客户端测试不能替代真实Bridge持久游标/任务路径。

已有D/E/F可复用，但以下不是现成事实：sync BLE命令、统一diag恢复、sync存储故障、运行字节证明、runner异步虚拟等待。同期实现如已提供等价接点，复用其名称并同步task操作规程，不保留两个同义控制接口。

负责区域：src/dev_log.*、src/main.cpp的history采集；新增一个有明确职责的src/v2_sync.*保存纯决定/二进制环/批次状态；必要的文件适配留在现有存储层。bridge/crates/render/build.rs、src/ffi.cpp及Rust FFI接点；device-sim/src/main.rs；取证工具。

步骤：

1. 定义diag v1字段、CRC、容量static_assert、generation、gap及当前wake工作结构；先做宿主字节夹具。
2. 将旧text/history生产者迁入同流，普通成功轮用summary替代重复文字。去掉AP密码等敏感采集点。
3. 实现RTC恢复与Freeze文件/双元数据存储，40KiB预留参与可用空间计算。
4. Fake ROM采用同样4096B限制、同源编解码和存储决定，注入短写、CRC坏、sync边界硬退出与deep/冷断电。
5. /log、/history成为只读同源视图；更新wake-contact-trace/estimate-power读取格式。
6. 记录预估RTC map；后续授权构建核对两目标最终map及512B余量。

主要出口：S07/S08/S14脱敏子集；正常15轮<=3600B；冻结文件<=16384B；无第二日志序列；未冻结断电丢失与冻结恢复区别明确。空间/RTC不满足不能静默缩水。

## P2 设备状态、能力与缓存

区域：src/v2_status_snapshot.*、main采样；core/src/platform/model.rs、service.rs；app/src/device_runtime.rs、main.rs；device-sim共享status接线。

实现BLE轻量sync投影与Wi-Fi完整分组快照；认证capability、sync_config；字段未提供与unavailable区别；分组采样元数据、持久last_success及last_attempt。旧字段默认迁移不删data。

主要出口：S12/S13状态能力子集、S14；BLE不新鲜化未采样字段，旧快照跨Bridge进程恢复，0/false/空数组有效，未认证候选不覆盖认证状态。设备配置按owner绑定并在NVS成功后ACK。

## P3 HTTP批次、游标与真实Bridge故障恢复

区域：main的v2路由与v2_sync决定；app/src/platform.rs、wake_history.rs；core/src/platform/model/service/store；device-sim；tools/fake-rom-runner.mjs。

按design §4–§6实现begin/page/ack/complete及原字节hash、Bridge part→final和checkpoint顺序、client_serial/收据幂等。生产切换后只使用当前设备路径与同步流；旧设备在升级窗口内不由新Bridge处理业务。不先修改生产全局下载器来绕开单请求错误。

生产端点成型时即收敛当前 `src/main.cpp` 与 `bridge/crates/render/src/sim_sync.cpp` 重复的冻结 JSON、分页、ACK 和完成判定，使用同一生产 C++ 协议入口；两端只注入各自快照、随机源和 I/O。不要因这项提前工作把完整 Fake ROM 验收提前宣称完成。保留模拟器虚拟电源/时钟与 runner 的环境调度；sync 的 rounds/due/retry/complete 由共享决定给出。

Fake ROM增加可达phase和页/最终ACK故障点；runner不再仅light调用HTTP，支持sync phase、预期失败、未来IO完成、持久游标观察。Bridge只经真实应用/设备客户端获取结果，不能读sim全知接口。

主要出口：S03–S08；所有落盘/ACK切点至少一条代表case；慢传输不因旧2s/16页或light截止截断；新日志不扩大冻结集合；Bridge和设备均有进程级恢复证据。持久页ACK是进展，重复读/status不是保活。

## P4 轮数、开网与light收尾

区域：main的setup/rendezvous/light/deep、v2_sync/v2_state；Bridge coordinator/PowerPlan接线；device-sim power模型与runner。

实现rounds/due/retry_skip、BLE sync_open、旧批次优先和原因合并、light入/离场、90s无进展及三次尝试。保留正式PowerPlan唯一改deadline、BOOT下界、token/claim及低电保护。第15轮无认证只保留due，不自主开Wi-Fi。当前B46物理参数不顺带优化。

主要出口：S01/S02/S03/S05/S11/S12；逐中间事件推进，1–15和第14轮转light的确定断言。模拟器不得只把计数设成15来替代主要路径；边界注入可辅助。

## P5 OTA/Bundle确认

区域：main OTA收尾/启动、v2_bundle_command接线、status；app/core任务与MCP映射；device-sim版本目录及共享响应；必要镜像解析/哈希夹具。

实现sync/arm、ticket与pending_reboot顺序、sync/image读取当前运行分区前N字节；保留原OTA token/target/owner。确认最多三次，只读重试，不重刷。Bundle原job对账后冻结观察，安装与显示分开。

先核对当前IDF接口和加密读取字节域；能力无法证明就不宣告支持，旧ROM继续待确认。Fake ROM按当前激活目录字节模拟，但Bridge不得读取目录真相。

主要出口：S09/S10/S12；同版本不同字节、错slot/长度、无能力、丢ACK与双进程恢复有明确结果。此阶段软件不能证明真实bootloader。

## P6 Fake ROM 软件出口，随后整理设备 Tab

先完成 Fake ROM 能力夹具、runner 与真实 Bridge 集成；S01–S14 全部通过后，才进入设备 Tab。后续实施者同步总设计§7/§11和PROGRESS/backlog，不能留下相反合同。

执行S01–S14代表case及新协议双target虚拟24h长场景，记录每个同步serial、实际批次/页面和计数变化。相同磁盘快照/seed重放至少1h，归一化trace/批次内容hash/帧CRC相同；batch随机后缀与nonce按首次出现映射，保留映射原件，每次传输自身原字节hash仍必须通过（具体规则见task F3）。运行当前Bridge/ROM、旧字面安全拒绝、双MAC隔离与一次性切换模型；旧Bridge版本夹具允许固定协议模拟，但不得宣称跑了不存在的旧二进制。

Fake ROM 出口：所有 S01–S14 有可重放断言；至少一个Bridge和一个设备进程kill/restart；相关Rust/C++检查和git diff --check通过。只扩展测试解决本次真实风险，不无条件跑无关全矩阵。

随后按 `device-tab.md` 的 M01–M17 与 U01–U20 整理 `app/ui/index.html`、共享状态服务/MCP 结果并做界面验收。设备页移除 Codex 余量和屏幕内容、模板/Profile 编排、字体管理编辑界面；模板 Tab 暂不增加对应界面，也不设计迁移布局。设备页保留只读安装/active/任务及设备级数据投递许可，修状态来源、年龄、缺失、作用域和多设备隔离；数据页排版不动。底层保存不发布、1–8完整顺序和目标MAC冻结合同保持。设备页出口单列，不把 Fake ROM 协议通过等同页面完成。

## P7 后续授权的最小硬件烟测

不恢复Fake ROM专项已取消的完整G实机矩阵。软件出口先通过，再按授权串行Note4/1.54标准B/W构建与测试；默认未获1.54本次授权不构建该目标。目标脚本、ROM marker/版本/大小/SHA、MAC/target核对按AGENTS。

- H01：每target一次自然达到第15轮，经实际BLE指令开Wi-Fi、完整批次结束实际deep；观察相邻普通轮无整段日志/镜像证明。一次物理按键入light/离场。仅证明接线通，不报告长期RF成功率。
- H02：两目标RTC最终map、少量deep保留；如获得OTA授权，一次正常镜像启动/当前分区证明与冻结文件恢复检查。故意掉电/bootloader破坏测试只在具体风险和另行授权下做。
- H03：实际按键、显示和电源状态小样本；软件已覆盖8项循环和渲染决定，不在板上重跑全协议故障。
- H04：仅需要功耗结论时用电池端积分；没有仪器就明确功耗未测，不能把模拟清醒时长当节电量。

实机打开串口会复位，不为看日志破坏现场。生产Bridge仅按路径/端口/计划任务核对后操作，保留data。报告严格区分软件已验、硬件烟测已验与未验。

## 验收职责矩阵

| 要验证的行为 | 主要环境 | 实机只补什么 |
|---|---|---|
| 15轮、light边界、优先级、失败退避 | Fake ROM虚拟时间+同源状态机 | BLE→Wi-Fi→deep物理接线 |
| 冻结、分页、持久ACK、gap、幂等 | Fake ROM+实际Bridge文件存储 | Flash/RTC实际保持边界 |
| Bridge/设备退出与恢复 | 隔离进程kill/restart | bootloader/分区真实启动 |
| 401/409、nonce、跨MAC、能力组合 | Fake ROM真实客户端 | 必要时配对/GATT缓存 |
| OTA证明、Bundle安装/显示分层 | 模拟目录+同源业务+哈希夹具 | 当前运行分区读取与面板 |
| 缓存时间/来源、缺失与UI文案 | Fake ROM+UI/service | 实际采样点的小核查 |
| 能耗/RF成功率/残影 | 不能由模拟器证明 | 仅有具体需求时测量 |

## 收尾与范围外事项

每阶段记录源revision/未提交范围、命令、输入seed、双方时域、MAC/job/batch/seq关联、测试证据和限制。运行产物进ignored artifacts或隔离data。源码实施完成不等于固件发布；没有发布授权时交付软件出口和精确硬件未验清单即可。

不实现按需无due开网、bridge_first反向HTTP、无限无损日志、常态逐事件Flash写、通用DeviceKernel或新数据库；若未来需求改变，另立合同。
