# 实施计划：Fake ROM为主要回归门槛

日期：2026-09-28。状态：设计定稿，P0文档完成，P1–P7未执行。协议唯一实施合同为design.md；待办入口为docs/roadmap/backlog.md C8。本文不是源码/部署完成记录。

## 授权与工作规则

本轮只修改本目录四份文档。后续源码、测试进程、构建、Bridge重启、OTA和实机须在实际授权范围内进行；不得把文档定稿当部署授权。目录创建提权、运行data保护、默认实例生命周期、目标串行构建遵循AGENTS。不撤销已有RF1及其它工作；每里程碑更新PROGRESS由当时负责者执行。

实现顺序P1→P2→P3→P4→P5→P6；每阶段同时接Fake ROM并验证，不能最后另写一个“看似通过”的模拟状态机。P7只是硬件最小烟测，软件出口不等待完整实机故障矩阵。按需下一轮开网和bridge_first反向HTTP不在首版。

## P0 合同与基线（文档完成）

已确定Bridge→设备HTTP、BLE开网授权、第15轮due、4096B统一RTC流+有界Flash冻结副本、端点及持久ACK、light截止后的sync drain、失败/低电、运行分区前N字节证明、兼容和测试矩阵。

开工时读取AGENTS/最新PROGRESS，记录工作树、现有测试和D/E/F证据。核查引用源码是否变化；不因行号变动重新发明协议。协议不确定项已收敛，资源/IDF检查是技术门槛，失败需明确结果。

出口：task.md每条S断言对应实际执行路径；没有重复状态机或将硬件假设写成证据。

## P1 诊断格式、存储与同源核心

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

按design §4–§6实现begin/page/ack/complete及原字节hash、Bridge part→final和checkpoint顺序、client_serial/收据幂等。新能力停止BLE history；旧能力保留旧路径。不先修改生产全局下载器来绕开单请求错误。

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

## P6 UI、兼容和软件最终出口

区域：app/ui/index.html、共享状态服务/MCP结果、Fake ROM能力夹具与runner；后续实施者同步总设计§7/§11和PROGRESS/backlog，不能留下相反合同。

只删除设备页Codex余量并修状态来源/年龄/缺失呈现。屏幕内容/模板/字体归模板页仅记录迁移边界，本专项不动其布局；数据页排版不动。保持保存不发布、1–8完整顺序与目标MAC冻结。

执行S01–S14代表case及新协议双target虚拟24h长场景，记录每个同步serial、实际批次/页面和计数变化。相同磁盘快照/seed重放至少1h，归一化trace/文件hash/帧CRC相同。运行new/new、new/old、old/new和双MAC混合能力；旧Bridge版本夹具允许固定协议模拟，但不得宣称跑了不存在的旧二进制。

主要出口：所有软件要求有可重放断言；至少一个Bridge和一个设备进程kill/restart；相关Rust/C++/UI检查和git diff --check通过。只扩展测试解决本次真实风险，不无条件跑无关全矩阵。

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
