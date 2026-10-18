# Bridge 重启后首次会合用量缺失调查

日期：2026-10-18。先完成源码调查、修复与回归；随后用户明确授权重建部署，香港时间11:43已重启Windows默认Bridge并验首轮自然会合。设备固件/模板/配置未刷写，原Bridge数据/凭据保留；运行状态由正常会合更新。下文调查引用的行号对应修复前源码。

## 结论与证据边界

用户报告 Bridge 重启后第一次会合显示 `--`，第二次才显示 `100`。源码确认存在可导致此现象的启动竞态：投递层尚未取得数据源首份快照时，仍允许生成并提交所有远端字段均为 `null/missing` 的完整 Data。缺失是合法业务值，因此设备接受并覆盖旧显示数据。应区分“数据源尚未就绪”和“成功读取后字段确实不存在”。

这是 Bridge 投递准入/初始化语义缺口，不需要据此更改 HTTP/BLE 编码或把合法 missing 改成拒绝。尚无该次现场的时间、载体、首包字段与显示配对，不能认定用户这次已完成实机根因闭环；`100` 的具体模板分支同样未核验。

## 源码调用链

1. `bridge/crates/core/src/platform/service.rs:345` / `:350`：open 恢复设备、context 和 data_contract，但 snapshots 为空、codex_envelope 为 None；`coordinator.rs:237` 起创建的 coordinator.sources 也为空。
2. `service.rs:360` 调用 refresh_all_contracts；`coordinator.rs:276` 的 set_contract 对启用同步的设备置 push_dirty。启动时并不要求数据源先取得快照。
3. `bridge/crates/core/src/runtime.rs:109` 把读取成功的 envelope 写入 app 共享状态；只有 `bridge/crates/app/src/main.rs:3440–3451` 的循环在先等待 10 秒后，才调用 platform::note_envelope 转入投递层。该循环还串行等待设备 cycle，10 秒不是严格传播上界；Codex 冷启动失败会进一步延后首份快照。
4. BLE 会合独立运行，`app/src/rendezvous.rs:125` 可先调用 next_delivery。`service.rs:2597` 的共用 HTTP/BLE 投递入口只检查设备合同是否已知和 requirements 非空，没有数据源就绪检查。
5. `coordinator.rs:348` 从空 sources 生成完整字段表，`datasource.rs:464` 将没有读取到的字段编码为 Value::Null / Quality::Missing；`coordinator.rs:985` 的 wire_fields 保留每个字段条目。因此它不是空 fields，也不会触发设备“不完整快照”拒绝。
6. `src/v2_runtime.cpp:129` 对 null 不建立相应 usage 节点；`src/firmware/platform.cpp:212–213` 缓存并替换 lastUsage。设备依模板渲染缺省分支。下一次投递取得真实快照后恢复数据。

## 调查阶段的证据与未覆盖项

- 现有 `remaining_changes_push_without_local_profile_after_ack_and_restart` 测试在服务重开后先 note_codex_envelope 再请求投递，避开了本次关键窗口（`service.rs:3575` 附近）。它不能证明初始化期间不会下发 missing。
- 本地 2026-10-17 日志的启动为香港时间 11:32:25，usage refreshed 为 11:32:29，首次成功联合 Data ACK 为 11:33:14 / seq 683；该样本不能证明首次包内容为空，也不是本次现场复现。2026-10-18 日志目前为零字节，未取得本次报文。
- 初次调查未运行新回归、未构建、未部署；当时以上竞态为源码确认的可达路径，未宣称已实机复现。后续宿主复现/修复见实施记录。

## 调查提出的修复与验收边界（待办归 A9）

应在共用投递入口区分依赖源尚未初始化与业务 missing：尚未就绪时保留设备已有数据，返回明确等待状态，不固定/提交新的缺失快照，也不因此制造正式 Light 意图。真实读取成功后缺桶/缺账号仍按模板规则投递。避免简单按字段 null 或全局 Codex 开关阻挡合法 missing、纯静态源或已发送待确认的原事务。

缩短 envelope 传播延迟有益，但单独移除 10 秒等待不能消除 Codex 首次读取较慢的竞态。需覆盖 HTTP/BLE：重开服务后、首份源数据到达前先会合；随后有效值到达；成功读取但确实缺字段；静态源；已有 in_flight/未知提交的原 seq 恢复。最终实机验收需首包字段、ACK 和屏幕前后时间配对。

## 实施 plan / task（2026-10-18 用户授权）

1. 先补服务级回归：真实 ACK 后重开 PlatformService，先请求 HTTP/BLE 投递，再输入新用量；检查等待期间不 pin Data、不推进 seq/确认指纹/全量期限。
2. 在共用投递入口检查所需源的首份成功快照；失败 probe 不等于就绪，成功快照中的真实缺失仍可投递，静态源不依赖 Codex。原 in_flight 优先恢复，Bundle/Activate 不受数据源等待阻挡。
3. 同步 PowerPlan 的新 Data 工作准入，避免尚未就绪的数据源制造正式 Light。保留显式 Light、现有授权及其他真实工作。
4. 运行回归红→绿及 core 相关测试，更新 status/review 和交接。原阶段范围为源码修复与测试。
5. 用户追加授权后，核对实例/端口/任务，停watchdog再停主桥，完整备份data及旧exe；构建、哈希核验、标准启动脚本重启，采集首轮自然会合和认证回读。

## 实施 status

源码修复、回归及Windows部署完成；首轮自然会合已取得Data/ACK/认证状态及源快照CRC配对。Mac实例未操作。

- `PlatformService::delivery_for_transport` 对新的 Data 检查源的首份成功快照，未就绪明确返回 `waiting_for_data_source` / `read_data_source`；不 pin payload，也不推进 seq、确认指纹和 full-sync deadline。
- `data_sources_ready` 使用当前设备远端 requirements，优先当前模板绑定、其次通用绑定；无绑定时识别已提供字段的成功源快照，再按字段匹配/默认 Codex 查首份成功结果。失败 probe 的 `last_success_at=0` 不算就绪。真实成功但缺字段的源仍允许 missing；静态字段不受全局 Codex 未就绪阻挡。
- PowerPlan 的新 push 工作准入复用同一判定；显式 Light、物理窗口、Bundle/Activate、其他网络工作和原 in_flight 恢复逻辑保留。应用层保留等待结果的阶段及原因。
- 新增 4 项服务级回归，各覆盖 HTTP/BLE：重启首轮（含失败 probe、真实 `100`/真实缺失、无 pin/确认副作用/无正式Light）、静态与部分源就绪、原未确认 seq/CRC/内容重试、模板专属/通用绑定优先级及静态源真实缺失。

修复前 `cargo test -p bridge-core --lib first_contact --target-dir target-note4-deploy --offline` 得到预期 red：服务重开后返回 `light_data`，两项字段均 `quality=missing,value=null`，而非等待。

最终命令（`bridge/` 下、提权执行，复用隔离target，允许构建及夹具创建目录）：`cargo test -p bridge-core -p bridge-app --target-dir target-note4-deploy --offline`。结果 **285通过、0失败、3条件忽略**：core单元159 + HTTP客户端7 + envelope7 + template8，app104通过/3既有模拟器与私有测试接口条件忽略；4项新回归全部通过。完整输出：[测试证据](../../artifacts/first-contact-usage-tests-20261018.log)。`git diff --check` 通过。没有触碰运行桥target/debug/data。

## 实施 review

- 已审阅全部新 Data pin 调用入口及 PowerPlan 的 push 工作分支，两种载体共用就绪判定；Bundle/Activate 在 Data 分支外，不被源等待拦截。
- wire、固件、持久格式、设备 RAM/栈/NVS 均无变更；只复用宿主已有 SourceSnapshot.last_success_at，不增加持久记录或源缓存副本。辅助检查顺序遍历有界 requirements，临时 canonical 字符串逐字段释放。
- 不把原用户现象的具体报文归因写成已证实；本轮部署后的新现场证据与边界见下。

## Windows部署与首轮验收（香港时间2026-10-18）

- 部署前默认监听主PID45504、同exe watchdog15520，计划任务CodexStatusBridge目标及8765/8766均核对一致。按顺序停watchdog/任务/残留核对，停止后完整压缩data并保留旧exe；没有删除target或数据。
- 旧exe SHA256 `11F2530AC30E91BB804C3CD5510CDD102C66A8D90849F0E61FA5C61BBCB8EF6E`。私有data备份为`artifacts/bridge-data-before-first-contact-deploy-20261018.zip`（16,982,269 B，SHA256 `15419A5A960D435023047D3BAED0D692738FDF85FE64E500E0EC260C4A4C1840`）；含凭据，不提交/展示原文。
- 为首包核验补充共用投递日志：`data_source_wait`，及新包`data_snapshot_prepared`的seq/CRC/字段数/缺失字段名；不记录字段值、账号标签或凭据。未改wire/持久格式。补充后core159项通过，`cargo build -p bridge-app --target-dir target-note4-deploy --offline`成功（复用隔离target，提权建构建/夹具目录）。[测试](../../artifacts/first-contact-deploy-core-tests-20261018.log)、[构建](../../artifacts/first-contact-deploy-build-20261018.log)。
- 候选复制到默认`bridge/target/debug/bridge-app.exe`并核对一致：38,095,360 B，SHA256 **`2CDC1AD87048C65CF7BBB2FAD98AC774CE321AC2E39B0F30B73B1A465DBC7CA5`**。用提权`pwsh -NoProfile -File <repo>/tools/start-bridge.ps1`启动计划任务；11:43:11启动主PID32532/watchdog36928，8765/8766属于新主PID，任务Running。[部署清单](../../artifacts/first-contact-deploy-manifest-20261018.json)、[运行核验](../../artifacts/first-contact-deploy-runtime-20261018.json)。

首轮时间线（以下为香港时间）：

| 时间 | 事实 |
|---|---|
| 11:43:15.204 | 只读MCP源`codex_envelope_available=false`，sources无首份结果、in_flight为空；原context/设备登记保留。 |
| 11:43:15.438 | app-server首份usage refreshed；11:43:22投递层源快照初始化。 |
| 11:43:41.866 | 首次BLE身份核验MAC `7C4FADB93408`，wake_generation3742714749/wake_seq5767。 |
| 11:43:42.264 | 第一份新Data **seq738 / CRC cf770c57**，共7远程字段；missing仅`buckets[codex].5h.remaining`、`.5h.resetsAt`，周用量不缺失。 |
| 11:43:43.113 | 同seq738联合Plan/Data ACK `applied`；没有更早的新Data pin/ACK。 |
| 11:43:49.987–57.205 | 认证HTTP ready并完成原会合。 |
| 11:44:39 | 独立调用只读`platform_status_refresh`，认证MAC匹配，固件`0.18.104-note4`，context `045e89d2a93a12c2`，data/applied_seq738，display_state=`displayed`。 |
| 11:45:11 | 在内存按认证字段表与仍未变化的首份源重建完整wire CRC，得到`cf770c57`，与设备ACK确认CRC相同；周剩余**98 / good**（源usedPercent=2）。没有记录字符串字段值。 |

[首轮时间线](../../artifacts/first-contact-deploy-timeline-20261018.log)、[被动MCP采样](../../artifacts/first-contact-deploy-observation-20261018.jsonl)、[首包CRC证据](../../artifacts/first-contact-deploy-payload-proof-20261018.json)、[认证回读/凭据保留](../../artifacts/first-contact-deploy-verification-20261018.json)。两份device-token文件哈希未变，两台登记及sync flags（Note4开/书桌屏关）保留。

验收结论：本次重启后的首次自然会合没有把未就绪源编码为整份missing覆盖；首包周用量98有真实源及CRC/ACK/认证displayed配对。5h桶在成功源中确实不存在，其missing为正常模板合同。本轮首次会合发生在源已初始化之后，因此没有现场触发`data_source_wait`；该冷源/首会合交错由红→绿回归覆盖，不能冒充已做实机故意延迟源的故障注入。没有目视/摄像验屏，不把固件displayed报告扩大为实体像素检查。
