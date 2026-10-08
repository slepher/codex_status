# Bridge—设备协议历史对账：过期、遗漏与尚未交付

日期：2026-10-08。定向核对协议相关历史与阶段记录，不通读全部归档。本轮只改文档，无软件测试/设备通信/部署。当前导航：[业务与 wire](bridge-device-protocol.md)、[执行场景](bridge-device-communication-scenarios.md)、[实现对照](bridge-device-communication-implementation.md)。

## 1. 如何判断历史条款

逐条判断，不用整份文件的日期或标题决定有效性。累计记录可能同时包含“尚未实施”的旧标题、后续部署修订、仍有效的不变量和未完成实机验收。

| 类别 | 判据 | 使用方式 |
|---|---|---|
| 过期实现/限制 | 后续用户决定或现行源码明确替代 | 只作历史依据，不恢复旧路径 |
| 过期现场 | 当时版本、PID、IP、候选 hash、停机/未部署状态 | 今日现场读 PROGRESS；不据旧记录操作设备/进程 |
| 遗漏的有效合同 | 后续未撤回且当前整理未覆盖 | 补场景/协议，单独说明实现证据 |
| 待实现方案 | 明示草案且仍在 backlog | 保留为未来方案，不列为现行协议，也不误标作废 |
| 仍待验收 | 有源码或部分证据，缺完整实体矩阵 | 不降格为旧方案，也不宣称已完成 |

## 2. 已过期的实现与阶段结论

| 历史来源 / 旧条款 | 当前判定及替代 |
|---|---|
| [旧 BLE 功耗设计](ble-rendezvous-power-design.md) §3：`WAKE_LIGHT → ACK → BLE_OFF → WIFI_CONNECT` | **换接顺序过期**。成功交汇需要保留 BLE 到认证 HTTP proof/转交确认后释放；HTTP 直达不走交汇。该文件还有电量/刷新等独立目标，不能整份作废 |
| 同文 §2：deep 每分钟 Wi-Fi GET usage、activity/idle 等时间自动控制 light | **旧运行模型**。当前 device_first 联系与正式 PowerPlan 合同取代；普通读写/claim 不续 light。旧采样及粗估不能当现版功耗实测 |
| [Wi-Fi 协议累计记录](../project-workflow/bridge-macos/wifi-protocol.md) 早期 R2–R4：关闭 BLE 后才试连、断开即取消候选 | **旧普通事务实现被取代**。统一事务允许在 BLE 保留时开网，生命周期不归属于该连接；源码旧兼容分支仍可能存在，见 §4 |
| 同文：BOOT-only 写配置、普通短 permit 决定全部管理许可 | **旧普通管理限制**。有效持久身份、session、owner 与合法操作窗口决定当前普通管理权限；首次绑定、物理恢复及旧槽迁移仍各有独立物理条件 |
| 同文：Bridge 不保存密码、禁止回读，其他绑定始终只读 | **已被用户修订及 .68 部署取代**。Bridge 本机 data 可缓存完整密码；有效 session+owner 共享配置权限，bond 是来源，回执仍隔离；不进入仓库/日志 |
| 同文 W0：所有 8 项回执永久保留、没有退役 | **新事务已取代**。WFS4 序号围栏与 result_ack 可退役已确认终态；旧任意 ID 回执仍保留，不能全删 |
| [会合评审](../project-workflow/bridge-macos/rendezvous-protocol-review.md) A5：HTTP 20s、Wi-Fi 工作 8s | **阶段预算已更新**。现行 Bridge 相关常量 30s，设备 handoff 30s，候选关联/DHCP 25s且受原截止约束；不回填为所有业务统一 30s TTL |
| [会合计划](../project-workflow/bridge-macos/rendezvous-protocol-plan.md) /评审：.46 候选未部署、Bridge 停机、现场 .45 | **当时现场，不是当前未实施结论**。当前最新 Note4 .69 与 Bridge 见 PROGRESS；原记录的长期 RF/功耗边界仍有效 |
| [旧平台设计状态](history/workflow/generic-display-platform-design/status.md)：implementation not started | **阶段状态过期**。现行 Profile 1–8、CompiledTemplate、完整 A/B Bundle、单 active context 已交付。v2 文件名现为导航 stub，不是另一套现行协议 |
| [早期模板交付](history/workflow/live-template-delivery/plan.md)：HTTP GET 条件 hash/304、逐模板 BLE 传送、旧 IP/ROM | **旧业务通道与现场**。当前完整 Bundle 显式发布；保留 hash/结构校验、分片/ACK 原则，不恢复旧 ≤3 槽与 Usage/Template Data 写入口 |
| 同文早期 missing `--` / infinity | **显示语义已被后续模板规则替代**。5h 缺失依 target 模板分支，不能从历史早期约束提炼全局规则 |
| 现行平台设计仍保留的 bridge_first Directive/StatusBeacon 候选 | **候选已由用户废弃**。当前 device_first；它不是遗漏的现行广播业务协议 |
| 泛化设计通用 `protocol` 与 Data 请求 `data_seq` | **wire 描述过期**。当前 Data/Plan 通用校验拒绝非空 protocol/rv，Data 请求为 seq、ACK 为 data_seq；Wi-Fi HTTP 仍有其兼容包装 |

这些判断由后续用户决定、当前源码及最新交接共同支持；无需改写只读历史原件。

## 3. 历史中仍有效、已补入现行整理的内容

| 遗漏内容 | 合同 / 层次 | 本轮补入位置 |
|---|---|---|
| 网络环境不是两端 SSID 必须相同 | 下层选择按主机真实 SSID→设备 2.4GHz 网络唯一映射；未知权限/有线/重复映射不猜测 | 场景 S15；实现对照补充 |
| 正常 HTTP 成功路径不另做成功 BLE 报告或完整配置重读 | 下层诊断按需：HTTP 失败才取最小诊断；明确配置意图仍需版本核对 | 场景 S16，独立于必要 HTTP 认证/换接证明 |
| 已保存、一致、已激活、实际连上、HTTP 可达是不同事实 | 配置不同先保存；已保存未活动只启用；旧网络 HTTP 可达不能证明新激活完成 | 场景 S17 |
| 连接失败类别、有限重试与成功归零 | 认证/安全失败停同内容重试；其他失败有限退避；HTTP 认证成功恢复连续失败机会 | 场景 S18；实现当前 3 次/≥60s，并非场景硬编码 |
| 临时窗口内冻结准入任务集及先读后诊断 complete | 下层不让诊断 complete 提前关闭尚需读取的临时窗口；新任务不借已有准入扩张 | 场景 S19 |
| 普通 BLE/CPU awake 与业务 light 不同 | 成功交接释放普通 BLE/PM 锁后才允许自动 CPU light-sleep；物理窗口与其他锁独立 | 场景 §2、实现 PM 证据 |
| 任务取消/过期不等于设备撤销或未提交 | 高层保留原事务及 may-have-committed；下层不因本地过期重发 | 场景 S20；协议事务补充 |
| 页快照过期、修订变化或中途失 owner | 不暴露部分数据为完整快照，保留旧缓存及 Bridge 编辑 | 协议事务补充；源码 snapshot 30s冻结 TTL |
| 回执退役围栏与旧 ID 保留 | w3 序号、principal/namespace 高水位、终态持久确认后 result_ack；retired 不等于成功 | 协议事务补充 |
| 显式迁移旧槽不等于自动读取或启用网络 | 只在新存储验证成功后退役旧键；retirement_pending 不等于回滚 | 协议兼容补充 |

来源主要为 [Wi-Fi 协议阶段记录](../project-workflow/bridge-macos/wifi-protocol.md)、[会合计划/评审](../project-workflow/bridge-macos/rendezvous-protocol-plan.md)、[统一控制设计 §10](../project-workflow/bridge-macos/wifi-unified-protocol-design.md)。当前实现核对点见实现对照；保留历史条款不代表本轮完成运行验收。

## 4. 不能当作“过期”或“已到位”的内容

- **Bundle v3 是待实现方案**：[原始设计](history/workflow/bundle-v3/design.md) 的 CSB3/dense 容器仍对应 backlog C1；其 `/v2/*` 路径是旧命名，不能据此认为格式已实现或整项作废。当前完整 JSON Bundle 的 begin/chunk/commit 不等于支持 CSB3。
- **字体增量发布是待定草案**：[历史草案](history/workflow/note4-bridge-publish/protocol.md) 与活跃专项存在关联，待办 C5。`asset_publish_protocol`、对象 PUT、manifest status、target_id 去重不能冒充现行端点。草案“asset font 在 full Bundle 路径拒绝”已被现行 full Bundle 字体交付覆盖，不能恢复该旧限制。
- **统一 Wi-Fi 的兼容分支仍存在**：`wifiSetupPoll` 对非 unified 的旧候选仍有按 peer/generation/encryption 取消逻辑，`wifiSetupRadioClosed` 也保留兼容条件；统一事务绕开这些取消条件。旧规则对普通新协议过期，不等于兼容代码已彻底删除。兼容退役验收仍归 B6。
- **普通 BLE 写静默失败仍是实现缺口证据**：未授权/畸形组包/队列满的入口未必产生结构化 ACK；历史成功样本不覆盖失败合同，不因分层整理而消失。
- **HTTP 直达全入口、断链/丢确认组合、真实 PM 与 RF/电池矩阵仍需各自证据**。已有软件或特定 Note4 实机记录不扩大为全部固件、两设备及 macOS 通过。
- **backlog 本身含旧阶段描述**：B6 后半部仍有“密码不回读/创建者只读/未部署”等历史里程碑文字，其最新条款与 PROGRESS 已覆盖它们。本轮不批量删待办或把实机边界标完成；阅读 B6 应区分最新合同与“此前里程碑”。

## 5. 核查范围与结果用途

已读：旧 BLE 功耗设计相关状态机；会合 plan/review；Wi-Fi protocol 的累计修订、wire/W0/迁移条款；统一控制设计回执退役；早期 live-template plan；归档平台状态；Bundle v3 与字体增量 protocol；对应现行源码与 backlog 相关部分。

未做：通读所有历史、查阅可能含凭据的 artifacts、重新运行历史测试、修改兼容代码或部署。该对账只承担本次协议范围的历史去歧义，不声称全仓库文档审计完成。待办唯一入口仍为 [backlog](roadmap/backlog.md)。
