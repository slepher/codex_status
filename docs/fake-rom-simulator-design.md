# Fake ROM：全部未完成部分的方案（Astra 最终修订）

日期：2026-09-26。本版替代此前模拟器设计的实施建议，实施范围为 D 剩余、E、F；G 整套实机回归已取消。用户需求、现行 v2 合同与后续明确决策优先于设计。D/E/F 软件验收已完成，结论见 `PROGRESS.md` 最新节；执行与评审归档在 `docs/history/workflow/fake-rom-simulator-def/`，先前 A–D13d 证据在 `docs/history/workflow/fake-rom-simulator/`。

## 1. 需求与完成定义

Fake ROM 用来暴露协议、状态、时序和恢复设计漏洞，覆盖一台 Bridge 管理多台设备、日常深睡、下一次认证联系交付任务、错过窗口与未知提交。模板/Data/Plan/Bundle/Activate/claim 的软件决定运行真实 ROM 的同源 C++；设备进程不执行 ESP32 指令或上传的 ROM。

必须保留的需求：

- 一进程一设备，独立虚构 MAC、端口、存储、token、状态；一个隔离 Bridge 管多台 fake，各 fake 的设备侧和 Bridge 侧可选择独立时间视图。不得靠当前 UI 选择代替任务 MAC。
- Profile 为 1–8 个有序模板，全部参与按键循环；完整 A/B Bundle、单一 active context；保存不发布。Bridge 负责 push/pull 与正式 PowerPlan，设备不重写这两套策略。
- 身份、endpoint token、设备操作 token、owner/claim、seq/context/nonce 校验与生产协议一致；读取与 claim 不隐式延长电源期限。
- 暂时不可达保留最后成功快照及原采样时间，任务等待联系；提交未知先对账。假设备必须能失联、拒绝、掉 ACK、重启及恢复。
- 默认 1x；支持逐步、加速、最终协作 max；设备与 Bridge 对该 MAC 的倍率、偏移、漂移可分别配置。真实 I/O 仍有真实时间上限。
- 每实例 2–3 个预置版本足够。默认收到有效 OTA 上传才安排升级；测试可显式切换预置版本；两种来源可辨。不运行镜像。
- 无线电、Flash 电气、真实 RTC、面板波形/残影、功耗等硬件结论需要实机。模拟覆盖提高发现软件问题的概率，不承诺穷尽所有协议漏洞。

**近期最小可用阶段（D3）**：同源 HTTP 完整配置/数据/显示/恢复，可通过电源编排进入不可达并恢复；有限版本 OTA；隔离环境中真实 Bridge 应用路径在 1x 下完成按 MAC 延后发布和 OTA 对账。fake BLE、Bridge 加速尚未完成时，只能验证已有 HTTP 可达机会，不能声称验证 BLE 唤起 HTTP 的全链。

**完整软件 Fake ROM（F）**：当前产品支持的 v2 HTTP/BLE 业务、持久恢复、显示决定、按键/电源生命周期和代表性故障类别均有同源或明确边界模型证据，逐 MAC 时钟与 step/max 可重放。未来候选协议（例如 bridge_first）、已删除 legacy、真实硬件细节不在此门槛内。

## 2. 2026-09-26 工作树基线与差距

本轮只读核查源码，不重新运行历史测试，不把工作树当已部署版本。现场以 `PROGRESS.md` 顶部为准：两目标为 0.18.24 系列的顺序发布现场，不能继续引用旧设计中的 0.16.x。生产设备/队列与本次文档修订无关。

| 证据 | 已实现 | 未完成 / 本版处理 |
|---|---|---|
| `src/v2_{data,plan,bundle,activate,claim}_command.*`、`v2_command_envelope.*`、`v2_status_snapshot.*`；`render/build.rs` | C 阶段命令决定、信封、状态构造由固件/宿主共享 | 决策函数不含所有应用副作用；接通现成函数及必要编排，不重做 Stage C |
| `src/main.cpp::applyV2Data`、Bundle handlers、`serviceV2Ble` 等 | 认证、应用、存储、显示、BLE 服务、电源编排仍在真实入口 | 未共享的可观察决定按纵向路径抽取，避免 Rust 复制第二套状态机 |
| `device-sim/src/main.rs::CAPABILITIES`、`status`、`endpoint_write` | HTTP 启动、LAA MAC/三 token、共享 status/claim/Plan；倍率/暂停/step/偏移 | configured 固定 false；Data/Bundle/Activate 为 501；无公共注册引导、BLE、显示与电源生命周期 |
| `OwnerStore`、`bootstrap.rs::owner_is_restored_clamped_and_renewable_after_restart` | owner 文件持久化、身份 marker 与重启钳制 | 不能笼统说完全无持久化；Bundle/fields/RTC/clock/显示恢复尚未接通；owner 副作用仍需同源核对 |
| `SimClock`、`ClockSnapshot::json` | 设备局部时钟与原始加步 | uptime 与 monotonic 当前同值；无跨重启实验时钟、事件调度或多参与者屏障；step 不等于生命周期模拟 |
| `render/build.rs`、`src/ffi.cpp`、`shim/` | 模板、bundle_store、字体、refresh_policy、GUI_Paint 同源编译 | shim 是全局测试设施；文件/显示副作用须连通；FFI 状态字段须跟随 fw 等新增字段 |
| `app/src/device_runtime.rs`、`app/src/platform.rs`、`core/src/platform/service.rs` | 多 MAC 运行记录、认证快照、持久发布/OTA、Unknown/awaiting_confirmation、版本证据 | 新 OTA 时间戳、overview/full-sync/快照节流仍直接 `now_secs`；E 不能沿用旧时钟审计结论 |
| `core/tests/v2_client.rs` | 手写 HTTP 响应的客户端合同测试 | 保留快速错误路径测试；不等于同源 Fake ROM，不替代本计划 |

历史 A/B/B2/C、D13a–d 的完成记录保留；它们不证明后续新增代码满足实验时钟与全部端到端要求。

## 3. 最小结构与同源边界

沿用 `device-sim`（Rust 进程/HTTP/控制面/调度）→ `bridge-render` FFI → 已有 C++ 命令、存储、渲染。一进程一个 C++ 全局上下文，全部 FFI 由单一执行线程串行调用；HTTP 解包线程不能并发重入画布和存储。无需先创建统一 DeviceKernel、新 sys crate、通用 HAL 或事件总线。

每条迁移切片包含“校验 → 业务决定 → 应用副作用 → 生成响应”。Bundle 不仅调 `v2DecideBundleCommit`，还运行真实 `bsInstall`、context/active 更新与恢复；Data 不仅返回 applied，还更新 fields/checkpoint、安排显示及正确 ACK。同一决定函数是最低要求，不是完整同源证明。

必要宿主接点仅为：时间/最近截止、存储字节与 KV/RTC、显示提交/完成、radio/链路事件、按键/电量/USB/reset 输入。名称与文件随切片确定，不为每项新建接口类。保留已能宿主编译的 String/ArduinoJson 与全局模块。

Rust 处理字节、实例配置、事件与宿主效果；不另写 owner 到期/幂等/Bundle 提交/刷新策略。现有 Rust owner 编排在 D2 与 `owner_store` 对照，复用决定与恢复，硬件存储适配保持薄层并配对验证。发现 ROM 现状不满足目标时记录 characterization 与需求断言，修改共享路径后验证两端，禁止仅在 sim 偷改语义。

HTTP 保持原 method/path/query、token 域、错误先后与 ACK 结构；`/v2/bundle/begin|chunk|commit` 不合并成新端点。无配置状态仍保留；D1 配置/状态字段可信后才增加 `/status.json` 等公共注册响应。模拟能力不冒充未来真实设备能力。

## 4. 配置、控制面与隔离

沿用当前显式指定的 loopback 地址、LAA MAC、data-dir 与三个 token；不新设默认生产数据目录。target 选择限产品实际两族测试夹具，配合现有编译能力；版本目录与 target 一致，版本切换不能改变板型。未实现能力在 `/sim/state` 列为 unsupported，业务接口不假成功。

保留同一 HTTP listener 上 `/sim/*` 控制面，control token 与 endpoint/device token 分离；运行清单只输出地址、MAC、target、能力、seed 等，不输出 secret。控制面能读实验真相，Bridge 只能走设备面，不得读取来源/提交事实等全知证据。

设备面随电源关闭时中止连接或制造有界黑洞，不用正常 JSON/503 冒充无线消失；控制面继续可达。同 listener 不能模拟 TCP connect-refused；该特定分类用现有网络夹具/受控进程终止验证，不为此扩大常驻架构。响应丢失必须可发生在真实提交之后。

body/队列/日志/事件次数有界。OTA 单独流式上传，不复用当前 64 KiB JSON body 限额，按目录文件大小限定，不将整份 ROM 塞入 JSON。慢 header/body 与外部 socket 用真实 watchdog，不占 C++ 执行线程。

## 5. 状态、显示、持久恢复与电源

| 保留域 | 内容与规则 |
|---|---|
| 易失运行态 | nonce、收包/在途显示/radio、本次启动时钟；重启按真实路径重置，不反序列化整个 C++ 对象 |
| RTC | 仅真实 ROM 保留的 checkpoint、唤醒/时钟/帧信任等；deep 与普通重启/冷断电分别建模；seq checkpoint 不等于完整 fields |
| 文件/KV | Bundle A/B、双 metadata、活动项、owner 等走真实写入/恢复；宿主提供容量、短写、损坏、切断位置 |
| 实验记录 | 跨 boot 实验时间、seed、输入、版本目录和来源；隔离于 ROM 数据，不能让生产协议读取其全知状态 |

短写返回错误与硬断电分开：后者在写边界立即停止业务，不运行收尾，再启动同一恢复路径。A/B 槽与 metadata 每个关键边界覆盖，不能一次保存成理想原子 JSON。损坏时明确拒绝/恢复，不悄悄清空。至少一次进程级 kill/restart，不能全靠调用 reset。

显示保留 candidate、最后成功 displayed、display_state 和 CRC；使用真实引擎/差分/refresh_policy。完成成功才更新帧/预算，失败记 uncertainty，不能证明真实面板仍是旧图。BLE 应用与会合后合并绘制保持原顺序；HTTP 同步显示保持同步 ACK，不为方便另改协议。

D2 将 `setup/loop/enterDeep/sleepToNextEvent/v2Rendezvous` 中影响协议的时间/radio 决定小步共享：未配置行为、物理/按键唤醒 provisional、timer wake、正式 Plan、会合/连接截止与 ACK grace、窗口关闭、低电/USB、锁释放。通过输入驱动，不只设置 deep 标签。未配置设备不能因接受 Plan 就假称已有正常睡眠日程。

当前 owner 恢复钳制本次 uptime，过期比较符也须按真实实现确认，不擅改成跨 deep 绝对 lease。v2BootMs 采样及绘制/通知越过名义 deadline 等由边界测试显露，需求与现状有差异时按共享代码缺陷处理。

## 6. 时间模型与加速顺序

每个 fake MAC 有 Device 与 Bridge 两个视图，分别设置倍率、wall 偏移、漂移；真实目标取真实时钟。显式实验登记决定时域，不能仅见 loopback 就启用测试行为。

- 实验单调时间跨双方重启连续，runner 持锚点；恢复实验须恢复锚点或明确新实验，不能从零续用旧截止。
- boot uptime 从设备启动起算，deep wake 按实际重启归零；保留 ROM 32 位 millis 视图以测回绕。
- wall epoch 用于观测/屏幕/server_time，可校时跳变；不得移动运行期单调截止。
- 宿主 monotonic 用于真实 I/O/线程进展 watchdog 与性能统计；失败和协议超时分开。

同一实验坐标 t 下，视图增量为 `rate × (t - anchor)`；改倍率先结算锚点，单调值不倒退。独立推进 A 不推进 B；联合 max 将各视图最近截止逆映射到实验坐标取最早值。暂停视图不自动到期；全部暂停且无事件时返回等待。

| 模式 | 行为与证据强度 |
|---|---|
| 1x | 默认真实秒；D3 用真实 Bridge 应用路径做最小端到端验证 |
| Nx | 逐个处理跨过事件；无协作 Bridge 时只是设备加速/压力观察，不算确定性联测 |
| step | 冻结后当前输入处理到静止；advance 执行全部中间截止并返回实际停点/原因，不仅给计数加数 |
| max | 有界直到时间/事件数/断言终点，所有参与者就绪才推进；依赖 E2 屏障 |

**E1 接线**：复用显式 now 参数，在按 MAC runtime 选择时钟，覆盖 coordinator/service 的 ACK/full-sync/job/Plan、app 机会/重试/claim 60s/快照年龄、OTA 排队/尝试/确认、BLE stamp_clock 与逻辑等待。数据采集仍是 Bridge 级，Static 夹具提供输入；每设备过期/质量判断用其视图，A 加速不能拉快真实 Codex 采集。ID 用计数/既有熵，不依赖 wall 时间唯一。

持久 epoch 字段保留原格式/含义，通过对应 wall 视图输入，绝不把旧数字解释为 uptime。运行期新计时用单调；重启先对账再恢复可写状态。若 epoch 截止在校时中违反需求，单独修生产合同/存储版本/恢复并测旧数据；不以 sim 名义全量改持久时钟，也不掩盖失败。设备本地截止权威，Bridge 镜像只是估计。

**E2 屏障**：runner 驱动复用的 Bridge 单轮应用工作与设备循环，参与者报告 idle(next_deadline) / in_flight(id)。真实请求前登记，响应被消费/错误应用后释放；未登记 I/O 禁止跳时。模拟延迟/丢 ACK 是已登记未来完成/逻辑超时，可推进到它，不能因“有在途”死锁。每步 event limit，外部 I/O wall watchdog，错误独立分类。不能靠“50ms 没包”猜空闲。

同一时间事件顺序从 ROM 实际调度提取并由边界测试固定，再共享一致规则；不能预先强写 deadline 永远优先而改真实行为。trace 带时域、局部时间、实验坐标、boot id、事件序号。

## 7. OTA 的有限版本模型

每实例目录含 2–3 项：版本 ID、fw 字符串、target、上传夹具 SHA256/大小，初始 active 显式指定。同版本不同字节可占两个目录项以测版本证明不足。可用真实 ROM 字节或满足 Bridge 校验的测试夹具，均不执行。

默认路径：有效设备 token → 按真实 OTA/owner/电源约束接受 → 完整收流并核对 target/摘要/大小 → pending → 可发 UPDATE OK → 延迟模拟重启 → 切换 active。中断、错 token/target/摘要、目录外镜像不切换。接受前后、重启前后掉电逐点定义并测试 pending 保留规则。OTA 是宿主边界模型；安全检查尚未与固件共享时明确证据范围，不宣称跑了真实 bootloader。

测试控制可在目录项间显式切换并模拟所需重启，记录 `cause=test_override`、前后 ID、时间/boot id，不生成上传 ACK。默认升级记录 `cause=ota_upload` 与接收关联。深睡、advance、Bridge 重启、发现预期版本均不自主升级；同版本重刷也留来源事件。

Bridge 只看生产设备能提供的认证状态（包括 fw），不看控制面来源或目录 SHA。按 sleep-aware-bridge 已定 Q3 区分 upload_ack/version_observed/version_seen_unproven/image_verified。没有精确运行镜像能力就保留 awaiting_confirmation，不自动重刷；fake 知道目录项也不能代 Bridge 证明镜像。未来精确身份须真实协议先定义算法与覆盖字节。

已有上传 ACK 加上传后同 MAC 预期版本认证观察，可按既定产品决定解除后续独立模板/数据阻塞；OTA 自身仍非 image_verified。版本来源不明、override、上传前版本相同/未知全部列入回归。

## 8. fake BLE 与自然联系

生产 timer 会合并不保证 HTTP 常开，完整范围必须含 fake BLE。E2 在真实 V2Connection 的连接/命令/关闭边界接测试 transport，优先小 enum/现有接点，不复制应用调度。命令封装、nonce/request 匹配、时钟戳共享；链路用 loopback socket 或有界 channel，不驱动 OS 蓝牙。

扫描候选、身份核对、连接/断开、分片重组、队列上限、认证会话、响应读取最终必测。若初版只传完整 JSON，明确分片 unsupported；F 前补逻辑分片/乱序/截断/超限。真实配对/加密/MTU 的硬件特有错误仅补 case 文档，不实现或执行。BLE 不支持的操作按真实 http_required 等结果处理，不替它开放 OTA/Bundle。

闭环为自然 timer/按键机会 → 认证 → 未知提交对账 → 必要正式 Plan → HTTP 可达 → 显式 claim/owner → 已授权任务 → 结果/深睡。D3 HTTP 起点只覆盖后半段，E2/F 才证明全链。

## 9. 延后可达性与协议故障矩阵

每行记录输入/切点、设备真相、Bridge 观察、最终任务/快照和副作用次数；相关截止均测前/当点/后。D3 做 HTTP 适用子集，F 完成全表；文档清单不算测试证据。

| ID | 场景 | 必须成立的断言 |
|---|---|---|
| M01 | 已登记无 runtime；深睡读状态；失败后重启桥 | 旧快照/原时间保留，尝试单列，不伪造在线 |
| M02 | 保存与显式发布；离线入队；源文件后改 | 保存 0 发布；冻结 MAC/内容，重启保留，充分认证机会才交付 |
| M03 | A 离线、B 联系 100 次、切 UI；同 IP 错 MAC | A 0 误传，B 不串状态，地址不转移任务身份 |
| M04 | 401/错 nonce/旧 session/他人 owner/失效 lease/yielded | 0 业务副作用；原因区别睡眠，claim 不等于发布 |
| M05 | 预算不足、重放 Plan、读取/claim/重试、校时 | 无新正式 Plan 时 light deadline 增量 0；timer wake 无新 BOOT 300s |
| M06 | Bundle 中断/重复/乱序/超时/错 context、ABI、CRC/空间不足 | 拒绝或同事务恢复，不部分激活；1–8 项不裁剪；未知 type/font/bind 整包拒绝 |
| M07 | COMMIT 后丢 ACK，Bridge Sending 时退出 | 恢复 Unknown，先认证对账 job/context，不盲重传或换 job ID |
| M08 | Data 重复 seq/CRC 冲突、换 context、Activate 丢 ACK | 应用与显示分离，重放不重复绘制，成功 ACK 才更新指纹/full-sync |
| M09 | push 值/缺失/质量，pull-only，源过期/失步 | 合并最新完整快照，pull-only 不推送/改 Plan，A 时间不影响 B |
| M10 | BUSY 失败、零差分、强刷、时钟、按键第 4–8 项 | 成功才更新帧/预算；8 项循环；thin wake 不假成网络会合 |
| M11 | A/B/metadata 各写切点断电、RTC 损坏/冷启动 | 只恢复完整合法包，上下文一致，无幽灵 ACK/旧数据复用 |
| M12 | OTA 错 token/target/摘要、截断、目录外 | 不升级，不伪造 UPDATE OK，错误分类明确 |
| M13 | OTA 接受后丢 ACK、延迟/失败重启、意外版本、重启桥 | 等待确认/尝试次数正确，未知结果不自动重刷，休眠不改版本 |
| M14 | 同版本、前版本未知、override 到预期版本 | 来源可追溯但 Bridge 无全知信息；有限版本证据不能 image_verified |
| M15 | OTA/发布并排、同类重复、取消竞态 | 创建时间排序、同秒 OTA 先；只取消确定未开始；后项重验认证/Plan/owner |
| M16 | BLE 会合、错过窗口、Plan 丢 ACK、fragment/队列超限 | 关闭窗口/释放锁，下次可恢复，HTTP 未出现不假判完成/失败 |
| M17 | 双 MAC 两侧独立 rate/offset/drift、倍率切换、wall 跳变/millis 回绕 | 不续租，错过窗口可观察，不串截止，可重放 |
| M18 | 桥/设备独立或同时退出、磁盘失败/坏文件 | 已持久任务/包恢复，未持久不能回 queued，错误明确，不触生产数据 |

当前 ROM 若与需求断言冲突，报告真实失败并修共享代码；不能整体 skip 后宣布 F 通过。

## 10. 阶段验收与软件边界

2026-09-26 用户调整当前范围为 D/E/F，取消 G 的整套实机回归。M01–M18 各类别选能区分正确/错误结果的代表性 case，不对所有错误排列做穷举。软件无法覆盖的硬件特有错误仅补少量针对性 case 文档，不实现、不执行实机测试。

阶段细项见归档 `task.md`，实际完成证据见 `PROGRESS.md` 最新节与归档 `evidence.md`。已有 render/core 合同单测保留，慢端到端不替代它们。

- D1：真实 v2_client 发布/数据/激活，状态/帧 CRC/ACK 来自同一共享状态；未配置拒绝仍有效；两个实例隔离。
- D2：配置后电源生命周期、按键/显示完成、跨进程恢复可运行；短写/硬断电区别；HTTP 不可达符合电源；相关固件路径转用共享决定。
- D3：M01–M15/M18 的 HTTP 子集有真实应用链证据，默认 OTA/override 可观察；形成近期交付，不等待全部加速完成。
- E：逐 MAC 时钟清单含新 OTA/快照；不触真实蓝牙；step/max 不越过未消费响应；未协作 EXE 的 Nx 不算通过。
- F：每个矩阵类别有代表性证据或明确限制；同 seed/输入的归一化 trace/帧 CRC 一致；有界 max 至少 24 h 虚拟时间及 1000 wake（可分两场景），报告真实耗时，不承诺固定倍速；至少两 fake 交错。真实时钟目标隔离合同测试确认不被加速，无需接实机。

实际无线、配对/GATT 缓存、DHCP/ARP、Flash/RTC/看门狗、USB/ADC/按键、面板 BUSY/LUT/残影/功耗无法仅凭 Fake ROM 证明；若它们暴露了 D/E/F 无法覆盖的具体错误，只增加该错误的针对性 case 文档，不实现或执行这些 case。

每份证据含 revision/工作树摘要、共享源清单、target/能力、seed/输入、时域、MAC/boot/job/request/seq/plan 关联、状态变化/结果。日志有界，不含 token、Wi-Fi 密码、完整用户数据。产物进 artifacts 或隔离测试目录，新目录依 AGENTS.md 提权预建。构建不清理 target/debug/data；两族模拟测试不自动授权真实刷写。

本版撤销旧版预设通用 DeviceKernel、重复 S0–S6 与 A–G 两套里程碑、legacy ≤3 验收等建议。保留现有切片，按可观察证据完成全部剩余需求。
