# 设备 Tab 整理与验收清单

日期：2026-09-28。状态：实施清单定稿，设备页基础整理已入源码并通过针对性脚本；完整 U01–U20、实际窗口和实机尚未验收，见 `status.md` 最新节。本文是本专项 P2/P6 的设备页分解，不是另一套项目方案。协议以同目录 `design.md` 的 sync-v1 为准，实施顺序见 `plan.md`，软件断言见 `task.md`，待办唯一入口为 `docs/roadmap/backlog.md` C8。

## 1. 空白 context 阅读顺序与范围

先读仓库 `AGENTS.md`、`PROGRESS.md` 最新节、本目录 `design.md` §4/§9/§10/§11、`plan.md` P2/P5/P6、`task.md` S09–S14，再读本文与对应源码。本文路径均相对仓库根；行号是定稿时定位提示，实施时以符号为准。

用户已确定：

- 设备页直接移除 Codex 余量。数据页已经足够详细，本轮不往那里补圆环，也不调整其排版。
- 屏幕内容、模板、Profile 编排及字体管理归模板页；本文只声明归属和跨页约束，不设计或执行具体迁移布局。保留设备页只读的实际 active/安装结果及按设备任务观察。
- 普通 BLE 保持精简，不为了填满设备详情而增加完整遥测、日志或镜像摘要传输。
- 设备页解决状态来源、采样时间、缓存与缺失原因；深睡暂不可达不能变成设备故障，最新 BLE 联系不能使旧 Wi-Fi 数据变新。
- 不在本文件另定 OTA 协议、确认终态、重试策略或同步开网机制。特别是旧 ROM 缺精确证明时仍按 design §9 保持待确认，不能自动结成成功或新增 `unverified` 终态。

本文定稿时仅创建清单；后续实施状态见 `status.md` 与 `PROGRESS.md`。多名协作者共用仓库，不撤销已有 RF1 或其它改动。新目录需按 AGENTS 提权预建；不清理 `bridge/target/debug/data`。未经要求不提交。

## 2. 已核实基线与运行版边界

### 2.1 源码基线

| 入口 | 已存在的行为 | 不能据此声称的能力 |
|---|---|---|
| `app/ui/index.html:240` 的 `tab-device` | 设备选择、概览、Codex 圆环、连接占用、详情、平台状态、Profile/字体/发布、恢复、功耗、操作区 | 没有完整 OTA UI；“升级固件…”是禁用按钮 |
| `app/src/main.rs:get_status` | 返回全局 usage、weekly_remaining、paused、last_sync、last_error，另附所选 MAC 身份 | 这些全局时间/错误/暂停不是所选设备的数据投递结果 |
| `get_device_status` / `refreshDevice` | 读取持久 `last_authenticated`、最近认证联系、最近尝试；公开遥测经 `public_online` 30s 门控 | 读取此命令不是请求设备重新采样；一次 HTTP timeout 不代表设备损坏 |
| `platform_devices` / `refreshPlatformDevice` | 列出登记、capabilities、Profile、coordinator observed、Bundle/OTA job | `observed` 可由 BLE 更新，不保证与完整 HTTP 快照同次采样 |
| `platform_status_refresh` | 显式认证 HTTP `/api/status`、核对 MAC、更新缓存与 OTA 版本证据 | 不能唤醒 deep 设备；当前响应没有完整 sync-v1 分组字段 |
| `platform.rs:ble_cycle` | 精简 status→协调状态→最近 BLE 联系→Data/Plan | 不刷新完整 Wi-Fi 快照；不提供当前运行镜像证明 |
| `get_pmstats` | 指定 MAC 的公开 HTTP `/pmstats`，10s 缓存；失败缓存目前会替换成功结果 | 不包含完整 deep 能量统计，不是认证任务证据；UI 还存在全局采样节流与旧 DOM 保留 |
| `core/src/platform/service.rs:ota_note_authenticated_version` | 记录 `version_observed` / `version_seen_unproven`，保留 awaiting_confirmation | 版本等于预期不代表精确镜像验证成功 |

表中 app/core 前缀分别为 `bridge/crates/app`、`bridge/crates/core`。完整源码索引见 §10。

### 2.2 运行版与文档的证据等级

`PROGRESS.md` 顶节明确 sync-v1 仍仅文档；不能把字段、端点、第 15 轮同步或新 UI 写成已部署。已记录的生产 Bridge SHA256 为 `492D618EAB0309F26643DEACADE6738729A0F1BEBEEE8D70FD7B693AA6752A56`，此值是既有记录，不是本文重新测量。当前工作区另有后续未提交源码，文件 mtime 不能证明代码进入此 EXE。

`bridge-ui-redesign` 曾只做四页视觉整理，要求保留所有 DOM id；本次删除圆环/占位等要求优先，可以成对删除 DOM 和 JS 引用，但不得误删 tray 等仍使用的 `weekly_remaining()` 后端功能。此前有从不含新版 UI 的干净 worktree 构建并覆盖运行 EXE 的事故；本次实施后需在包含最终 UI 的工作区构建并核对运行 EXE。源码验收、构建成功、实际窗口验收、设备部署分别记录。

## 3. 最终设备页职责与模块清单

页面顺序为：设备选择 → 设备概览及联系 → 固件/任务 → 同步与功耗 → 折叠诊断/维护。这是设备页内部的信息层级，不规定模板页布局。每个值和动作都必须能回答“哪台设备、哪次观察、哪个作用域”。

| 编号/现有模块 | 当前来源/作用域 | 处理及可执行要求 |
|---|---|---|
| M01 当前屏幕/登记列表 | `platform_devices`，Bridge 实例登记集合；每行 MAC 为键 | 保留。名称可重名；选择以 MAC 为准。显示名称、MAC 尾部/完整详情与独立任务摘要；IP 仅连接线索。多设备无选择时要求选择，不猜第一台 |
| M02 状态条/电量/Wi-Fi/最近同步 | `get_device_status` 的认证快照与公开短缓存；“最近同步”却取全局 `get_status.last_sync` | 与连接模块合并概览。移除全局“最近同步”指标，改为最近认证联系、上次完整 Wi-Fi 同步（能力未实现前称完整认证快照）。电量保留值和年龄；Wi-Fi 是采样时关联状态，不能等同当前可达 |
| M03 Codex 余量/周圆环 | `get_status.weekly_remaining`，全局 Codex envelope | 删除设备页 HTML、专用 CSS（仅确认无其它引用才删）和 `refresh()` 的 `ring-week` 更新。不搬到数据页，不影响源采集、托盘额度、模板绑定 |
| M04 连接与占用 | `device_facts_json` 的名称、MAC、IP、discover、owner、yielded；设备级 | 合并 M02，保留改名/发现/claim/release。明确 owner 的观察时间和本地 yielded；“owner 未读取”不能显示为空闲/已归本桥。剩余 lease 是观察值或有依据的估计，不能显示过期静态值为实时 |
| M05 设备详细信息 | fw/模板/电池来自认证快照；slot/reset 当前认证快照不含；BLE/EPD/heap/RSSI 仅 public_recent 时填值 | 重组为固件、连接、运行、显示分组，逐字段执行 §4。解决采样缺口依赖 P2 的 Wi-Fi 完整状态，不通过常态扩充 BLE 解决 |
| M06 平台运行状态 | 登记 capabilities、coordinator session、Bundle job；OTA 字符串附在同一行 | 拆出实际 active/显示摘要和独立任务区。target/render_target/ABI 留详情；context/commit_seq/job ID 留诊断。不得把当前 Bundle job 状态直接冠名“已提交”；显示设备确实观察到的 committed_job_id |
| M07 屏幕内容与发布 | 每设备 Profile 草稿、库模板、target、font_ids；字体导入是实例资源库操作 | 本轮从设备 Tab 移除屏幕内容、模板/Profile 编排、字体管理编辑界面；模板 Tab 暂不增加对应界面，也不设计迁移布局。保留服务/API 与显式发布合同，设备 Tab 只读实际 active、已安装和任务观察 |
| M08 同步许可 | `pt-sync` 当前作为 `platform_profile_save` 的一部分；`full_sync_s` 已存在但当前没有编辑控件 | 设备级数据投递许可保留为独立的设备设置；保存时只更改所选设备的 `sync_enabled`，不改 Profile 模板/字体/顺序，不发布。它不关闭 sync-v1 状态/诊断同步。完整同步阈值只读展示现有值 |
| M09 恢复与维护 | `recoverPlatform()` 先认证读取，再 `platform_recovery`；目标由摘要 MAC 确定 | 保留高级入口。传入前比对请求目标 MAC 与返回摘要 MAC，不拿错误/空摘要恢复；导入的 Profile 默认关同步，不自动占用、唤醒或发布 |
| M10 正式 PowerPlan | `platform_power` 的 last_sent/last_accepted/session.power；设备级 | 保留 light/sleep 显式动作，分开“桥已发送”和“设备已接受”。ACK 时剩余值带时间；无当前推算依据时不做实时倒计时。provisional、正式期限、sync 执行阶段分开 |
| M11 电池下限 | 当前 `pt-batt` 渲染 session.power.battery_percent，实为电量 | 改为“采样电量”，与概览共用来源规则；不得伪造下限。若未来显示保护阈值，必须另有真实配置字段，本轮不加 |
| M12 PM 统计/CPU 档/锁/原始输出 | `/pmstats` 文本解析，设备级但 UI 可能跨设备残留 | 留折叠诊断。标“本次启动的 light/PM 统计”，保留采样范围/时间；CPU 档是驻留占比，锁不能简单统称 Active>0 都阻止任何睡眠。无日志/计数依据不推算整机耗电、deep占比或续航 |
| M13 暂停推送 | `set_paused`，实例全局；不是 selected MAC | 从设备操作移除；现有托盘全局入口保留，不新增另一页布局。若展示暂停状态，明确“本 Bridge 全局已暂停”，不能看成设备已释放或同步许可被关闭 |
| M14 立即同步 | `force_sync()` 无 MAC，触发全局活动/所选后端发现和推送通知 | 移除设备页此按钮，不把它改名为完整 Wi-Fi 同步。已有按 MAC `platform_push_now` 服务属于数据投递，不等于 sync-v1 批次；本轮不新增替代按钮或按需无 due 开网 |
| M15 升级固件/打开设备页/开机自启 | 三个 disabled 占位，无有效处理器 | 删除占位，不把 OTA 上传入口当既有功能。保留真实 OTA 任务观察；新增上传表单另行定义，不在本文实现。开机自启是 Bridge 级功能，不属于设备 |
| M16 最近错误 | 全局 last_error 加 device_note | 移除全局错误对所选设备状态灯的直接判定；展示此 MAC 的 last_attempt/job/同步原因。必要的全局 Bridge 错误保留明确全局标签，不复用设备故障色 |
| M17 sync-v1 同步/诊断摘要（待实现） | P2/P3 的设备组、批次、checkpoint、complete 收据；设备级 | 在能力启用后展示 rounds/due、phase/radio_reason、上次完整成功、当前批次进度和失败原因。读取本地缓存/归档；普通刷新不能发 sync_open。无能力显示未支持，不显示虚构0/15 |

M07 的本轮删除只针对设备页编辑 UI；底层服务和已存配置不删除。模板页布局迁移未实施。

## 4. 详情大量 `--` 的根因与字段合同

### 4.1 当前具体缺口

`refreshDevice()` 当前把 fw、slot、reset 先取 `last_success.body`，再在 `public_online` 时取公开字段；但 `note_authenticated_status()` 的保存白名单没有 slot/reset。RSSI、BLE connected、EPD writes、Free heap 也仅在公开样本不足 30s 时显示，深睡后必然变 `--`。这不是四个硬件能力同时失效。

另有两个缺口：`template_ids?.join(', ') || '--'` 会把有效空数组显示为未知；最近 BLE 只更新轻量协调状态和联系时间，不提供 heap、电量等完整详情。应修各字段的观察合同，不能把公开状态有效期改成无限来遮住问题。

### 4.2 字段实施映射

| 字段/组 | 当前实际来源 | sync-v1 后的指定来源与显示 |
|---|---|---|
| 名称/MAC/target/render_target/ABI | Bridge 登记 identity/capabilities；名称是本地标签 | identity/firmware组加 Bridge 名称配置；显示来源。未经验证的硬件兼容标签不等于 OTA 未验证 |
| fw/slot/reset_reason | 认证快照只稳定存 fw；slot/reset 依赖近期公开状态 | firmware组认证 Wi-Fi；保留采样时间，不因120s清空。新boot的 reset 原因未采样时不得把旧值当本次 |
| battery | 完整认证 HTTP snapshot.power.battery；power区另取session缓存 | power组；保留历史值和年龄，两处统一口径。没有采样显示“尚未读取电量”，不能用0代缺失 |
| Wi-Fi关联/RSSI/网络/BLE连接 | 公开/status.json短缓存 | radio组认证 Wi-Fi；写明“采样时”。默认仅“已连接网络”，不持久SSID原文；显式本地诊断规则按design §10 |
| heap_free/heap_min/uptime | 当前只公开 Free heap；其余 UI 尚无可靠字段 | runtime组认证 Wi-Fi；字节/时间单位明确。0是有效值；本次boot未知时不拼接不同boot的计数 |
| EPD写入/失败计数、display_state | 写入数公开；display_state完整认证快照；失败数当前页未接 | display组；已应用与已显示分别呈现。显示失败不改写安装成功，不凭最新BLE Plan ACK变成displayed |
| 已安装template_ids/active/context/commit_seq | 完整HTTP或coordinator轻量观察，来源不同 | 保留每次实际携带的字段及其来源时间；空数组显示“未安装模板”。只读安装观察不是Profile草稿 |
| 正式plan/provisional/remaining | coordinator plan及session.power | power组及真实Plan ACK，分别列期望/已接受/采样剩余；缺失显示未获得，不能用0暗示期限已过 |
| rounds/due/phase/radio_reason | 当前生产协议未提供完整sync-v1语义 | P2/P4完成后按能力显示。15是实际deep BLE窗口数，不是15分钟；due而无认证显示“已到期，等待认证开网” |
| OTA/Bundle证据 | 本地持久任务、设备观察；OTA当前仅上传/版本证据 | jobs组+任务原身份；镜像证明仅按待确认请求经Wi-Fi取得，不在普通BLE或每次周期同步中计算 |

### 4.3 值、年龄、原因的统一规则

沿用 design §10 的组级 metadata：`received_at`、`sampled_boot_id`、`sampled_uptime_ms`、`sampled_wall`（可空）、`transport`、`quality`。不另建一套全局 UI 缓存。

| 状态 | 显示规则 |
|---|---|
| 有值且observed | 显示值、采样/收到时间和通道；不笼统写实时 |
| 历史值stale | 保留值，标“上次采样 · …前”；radio/runtime/power超过120s为stale；已知新boot使旧runtime/radio立即stale |
| 本响应省略字段 | 未采样；保留已有值及原时间，BLE联系不刷新其年龄 |
| null + not_sampled | “尚未读取”；有历史值可另列“上次值”，不能显示成当前有效值 |
| null + unsupported | “固件未提供/不支持此项”；不要求用户反复刷新，不虚报可修复硬件错误 |
| null + not_applicable | “不适用”，例如对应能力不存在；与未读取区分 |
| null + read_error | “本次读取失败”；附有界原因和时间，历史值保留原时间 |
| deep暂不可达 | 页级“预计休眠/等待下次联系”；现有字段年龄继续推进，无删除缓存或自动升light |
| 无完整快照但有BLE联系 | “最近BLE已认证 · 尚无完整Wi-Fi快照”；只呈现BLE实际携带字段 |
| 连首次认证都没有 | “尚无认证观察”；登记属性可显示为登记值，不把空owner说成空闲 |
| wall倒退/无可信wall | clock_anomaly提示；显示收到于/本次启动采样，不给负年龄。deadline使用单调时间 |

不使用 `value || '--'` 判缺失：0、false、空数组都是有效数据。加载中可用临时占位，加载完成后的每个空字段必须有明确原因；不能用一行“离线”解释所有缺失。公开status/ARP仅作发现候选或明确标注的临时诊断，不覆盖认证缓存，不形成任务证据。

## 5. 操作与刷新合同

| 当前动作/处理器 | 真实行为与作用域 | 最终动作合同 |
|---|---|---|
| `pickDevice/applyDeviceSelection` | UI记住MAC，重载所选设备；会丢未保存Profile草稿 | 保留当前丢草稿提示，执行§6；选择本身零设备写，不触发完整同步 |
| `refresh()` 定时轮询 | get_status，周期性触发本地refreshDevice | 只读缓存；删除圆环/全局last_sync绑定，不额外增加设备HTTP轮询 |
| 概览“刷新”/`refreshDevice` | get_device_status读本地记录 | 命名“刷新缓存”或清楚注明本地；完成提示不写“设备已更新” |
| 平台“刷新”/`refreshPlatformDevice` | platform_devices读Bridge状态和库 | 与本地刷新合并入口，保留分组错误；不能默默覆盖正在编辑的Profile草稿 |
| “从设备读取”/`refreshPlatformStatus` | 认证HTTP/api/status，指定MAC，可能timeout/401/409 | 保留显式读取，说明需已有Wi-Fi机会；deep时返回等待联系，不发sync_open或Plan。成功后本地各关联区一起更新；失败保留旧值 |
| “重新发现”/`discoverDevice('auto')` | 显式发现，HTTP/ARP等现有链；指定MAC或登记前发现 | 保留。IP变更须核对MAC；发现成功不等于认证、owner获得、数据已应用 |
| “BLE发现” | 显式BLE发现/连接路径 | 留高级连接入口，不作为定时完整遥测；超时呈现发现未命中，不判硬件坏 |
| 点击名称/`rename_device` | 所选MAC本地标签 | 保留；弹窗打开时冻结MAC，保存前仍显示其名称/MAC；名称不是身份 |
| “占用/恢复”/`claimDevice(false)` | POST claim相关设备用例 | 保留，名称改为清楚的占用含义，避免与恢复Profile混淆；离线/冲突报告原因，不先显示已占用 |
| “释放”/`releaseDevice` | 释放并记录本地yielded | 保留；显示本地让步状态。不能把释放解释为暂停全局桥或删除Profile |
| “强制接管”/`claimDevice(true)` | 显式force claim | 保留高级危险操作；冻结并展示目标MAC和已知owner；确认只授权该目标，不扩大为全局接管 |
| Profile/字体编辑、保存、发布预检/发布 | 本地草稿/实例库，发布需显式冻结目标 | 本轮移除设备页入口；服务/API和已有配置保留，保存零publish、发布需显式动作。模板页暂不增加替代入口 |
| 从设备摘要导入/`recoverPlatform` | 新认证读取后恢复本地Profile | 保留高级入口；无成功摘要不调用恢复，结果按MAC展示；默认关数据同步 |
| 展开功耗/`refreshPmStats(false)` | 当前会自动发/pmstats；`refreshPlatformPower`只读本地 | 改为展开只显示该MAC缓存。设备HTTP采样只由显式“采样”触发；文案说明可能扰动light统计、不能唤醒deep |
| “采样”/`refreshPmStats(true)` | 当前true只绕前端节流，服务仍有10s缓存 | 显示实际fetched_at并说明短缓存；不承诺强制新样本。成功缓存与最近失败尝试分离；失败后保留同MAC上次样本 |
| PowerPlan“刷新” | platform_power本地汇总 | 并入本地刷新或清楚标注，不能暗发Plan |
| 显式light/sleep/`platformPlan` | 按MAC正式计划请求/ACK | 保留，在结果中区分排队/已发送/已接受/被拒。sync_drain可在正式deadline后继续执行完整批次，不能显示为偷偷续了light计划 |
| PM原始输出复制 | 复制目前显示的本地样本 | 保留，内容标题带MAC/采样时间；不复制token等秘密 |
| 暂停推送/立即同步/三个disabled占位 | 全局或未实现 | 按M13–M15移除，不为凑操作区新增动作 |

每个异步动作必须有处理中、成功/等待、失败三类结果；捕获Promise rejection，不留无反馈按钮。401/403、409、MAC不符单列认证/占用/身份错误；网络timeout只表示这次未取得回应。原始JSON置诊断，不以整块协议JSON作为日常成功提示。

## 6. 多设备切换与缓存隔离（必须先修）

- [ ] 每次读取在发起时捕获规范化MAC及UI选择代次；响应只写回对应MAC数据，只有MAC和代次仍匹配当前选择才渲染。A→B→A的旧A响应也不得覆盖后来样本。
- [ ] 所有写动作在打开弹窗/预检时冻结目标MAC和相关job/preview身份；提交调用该冻结目标。选择改变则取消弹窗或明确继续原目标，不在确认时偷偷读取新selectedDeviceMac。
- [ ] 切换时立即重置所有分组、错误、PM统计/原文/锁、任务及provisional占位，再载入B缓存。不能等请求成功才清A字段。
- [ ] `lastDeviceFetch`、`lastPmFetch` 等节流按MAC维护或随选择重置；展开PM不能因A刚采样而跳过B缓存装载。B离线时仅保留B历史样本。
- [ ] `renderDeviceIdentity(null)` 不再静默留下上一台名称/owner。零设备/多设备未选时，清空设备视图并禁用需要目标的写动作。
- [ ] 设备页不再有 Profile 草稿；数据投递许可保存冻结 MAC，自动刷新不改变用户正在操作的开关。
- [ ] UI中的selected MAC、后端默认selected MAC和当前任务device_mac分别核对；每个设备动作显式传MAC。全局能力不伪装成设备动作。
- [ ] 本地缓存/组metadata、同步serial/游标、job、错误、能力按MAC隔离；B的100次联系不得刷新A的年龄或完成A的任务。错误IP返回B时拒绝采纳并保留A旧值。

只需现有服务记录与小型UI选择代次，不引入通用前端状态框架、第二数据库或新的设备选择主键。

## 7. 固件、OTA、Bundle与sync-v1的显示语义

固件区显示设备采样fw、slot、target和时间；本地ROM路径/预期版本/冻结SHA/大小属于OTA任务，不能拿它填“设备固件”。认证版本观察、精确镜像证明、硬件兼容验证、屏幕显示成功分别标注。

| 已有/规划状态 | 用户可见含义 | 不允许的转换 |
|---|---|---|
| OTA queued | 已排队，等待该MAC机会 | 因另一设备在线就发送 |
| transferring | 正在上传，显示attempt/job关联 | 刷新页面重启上传 |
| upload_ack | 设备接受上传，等待重启后确认 | 直接显示升级完成 |
| version_observed | 已认证看到预期版本变化；精确镜像仍待证 | 用版本字符串替代SHA |
| version_seen_unproven | 看到预期版本，但上传前相同/未知，不能证明变化 | 把“再次看到”当重刷成功 |
| awaiting_confirmation/confirmation_pending | 等待下一次允许的Wi-Fi观察；展示原因和最近尝试 | 因普通BLE成功或sync_complete而结成成功；每次会合重复上传/开网 |
| image_verified（待P5实现） | 同MAC新认证session的运行分区前N字节与冻结文件匹配 | 使用非运行槽、公开快照、旧session、另一job或另一MAC证据 |
| 无identity能力/查询失败 | 明示能力不支持/本次查询失败，保留原证据等级与待确认 | 新增自动超时失败/成功/unverified终态，或强制重刷旧ROM |
| Bundle提交已确认 | 设备committed_job_id/commit_seq/context匹配，安装结果已确认 | 等同displayed |
| display failed/pending | 安装可成功，但显示失败/待绘制 | 由Plan ACK、批次完成或页面刷新清掉错误 |
| 同步批次完成 | 冻结状态/诊断已完整持久确认 | 等同Data ACK、Bundle提交或OTA镜像核验成功 |

OTA镜像算法、arm/ticket、N和重试严格引用 design §9 的 `sha256-running-prefix-v1`；不在UI清单另换成ELF/native image digest算法。普通BLE不请求image或完整日志。首次加两次确认查询失败后按已有义务和退避等待，任务页只解释事实，不自己安排开网。

第15轮due、light_enter/light_exit、OTA/Bundle回报都显示为“同步原因”；正式PowerPlan的deadline与执行phase/radio_reason分开。`sync_drain`在截止后传完已冻结批次不是新light租约；持续正常持久进展不能被UI倒计时误报超时。数据同步关闭也不等于状态/诊断同步关闭。

OTA当前取消仅queued变cancelled，上传后`cancel_requested`不是撤回已写固件；若任务区接取消入口，必须准确显示受理结果，不能承诺停止刷写/回滚。本轮可只显示既有任务，不新增完整OTA上传/重试/取消控制器。Bundle取消同样以现有service真实返回为准；不改状态机或重发语义。

## 8. 分步实施出口

1. **UI基础整理（可独立于新ROM）**：删除M03/M13/M14/M15设备入口；合并缓存刷新和状态说明；落实§6；修复PowerPlan/PM文字；既有空字段显示实际已知缺失原因。不能在旧ROM上编造unsupported细分：缺能力证据时写“当前快照未提供”。
2. **P2接线**：沿design扩展有界认证Wi-Fi快照和分组metadata；填补slot/reset/heap/EPD等缺口，保存null原因；BLE仅更新实际投影。旧schema默认值兼容，不删除原data。UI读取共享application service结果，MCP同语义。
3. **P3–P5接线后的P6呈现**：展示batch/rounds/due/phase、诊断完整/缺口、OTA/Bundle证据。能力未部署时不露虚假进度；保持当前旧ROM待确认合同。
4. **软件和实际窗口验收**：执行下表及相关S断言；脚本语法/DOM引用检查和diff检查；在560×680及更窄窗口验证折行/按钮/折叠/无横向溢出。构建和运行版展示按后续授权，未做就明确“视觉/运行版未验”。

M07 本轮从设备 Tab 删除编辑界面，模板 Tab 暂不增加界面或迁移布局；底层发布服务保留。实施者不能把本文件扩展成四页重做，也不能遗漏状态/多设备修复。

## 9. 设备页验收矩阵

主要用Fake ROM真实Bridge客户端+服务/UI夹具，关联task S11/S13/S14；OTA/Bundle同时关联S09/S10，sync与电源关联S01–S05。下列是待执行断言，不是通过记录。

| ID | 输入/动作 | 必须观察到的结果 |
|---|---|---|
| U01 范围 | 打开设备和数据页 | 设备无Codex圆环/配额DOM及更新引用；数据页结构/排版未变；托盘和源采集仍工作 |
| U02 无/多设备 | 0台；2台无选择；名称相同两台 | 清楚空态/要求选择；不猜第一台，写按钮无目标不可用；按MAC区别同名 |
| U03 完整后BLE | Wi-Fi样本有battery/heap/EPD，再仅BLE联系 | 最近BLE推进；未采样组值和时间不变，radio/runtime/power>120s标旧，仍保留值 |
| U04 合法空值 | heap=0、BLE=false、templates=[]、remaining=0 | 分别显示0/未连接/未安装/0，不显示未知；缺字段不能被同样处理 |
| U05 四种缺失 | 缺字段、null+四种reason；分别有/无历史 | 未采样/不支持/不适用/读取失败区分；历史值带旧时间，不用新失败时间使其变新 |
| U06 深睡与首次联系 | HTTP timeout；有/无旧快照；仅BLE联系 | 预计休眠/等待联系；最后快照不消失；无完整样本明确说明，不能整页硬件故障 |
| U07 新boot/时间异常 | 新boot只BLE；wall倒退 | 旧runtime/radio立即stale；固件保留原采样时间；不出现负年龄/伪实时deadline |
| U08 A/B竞态 | A慢响应，切B，再切A；A刚采PM而B离线 | 不串身份/owner/PM/原文/锁/任务/错误；旧代次响应不覆盖新样本；B只呈B缓存 |
| U09 操作冻结 | A打开改名/claim/发布预检，再改变选择 | 取消或明确原A目标；绝不写B。所有动作参数可证MAC，未选设备零写 |
| U10 占用与发现 | owner未读、空闲、本桥、他桥、yielded、lease旧值；IP被B复用 | 未读不称空闲；yielded明确；MAC错拒绝；发现不等于claim成功；401/409原因分开 |
| U11 刷新副作用 | 本地刷新、展开诊断、显式HTTP读、显式PM采样 | 前两者设备请求/Plan/sync_open计数不增；显式读取不续租或开网；PM缓存命中标真实时间 |
| U12 计划 | last_sent新而accepted旧、无ACK、provisional、到期sync_drain | 发送/接受分开；0不是缺失；不把旧ACK剩余值当实时；正式期限不变，phase明确 |
| U13 数据/全局范围 | 全局paused、A数据同步关、B开、源错误 | 无设备级全局暂停按钮；A/B状态各自准确；Data许可不关闭诊断sync；源错误不判设备坏 |
| U14 OTA证据 | ACK丢失、版本相同不同字节、无能力、只有BLE成功、sync已完成 | 依§7保持正确证据；不自动重刷/结成功；新能力下只同MAC新session匹配证明完成 |
| U15 Bundle/显示 | committed匹配但display failed，随后Plan ACK | 安装成功和显示失败同时可见；Plan/批次成功不清显示错误；原job对账不创建新publish |
| U16 草稿/恢复 | 编辑未保存时本地刷新；保存8项；失败/错MAC摘要恢复 | 草稿不被轮询覆盖；保存零发布；8项不裁成3项；错误摘要不恢复，成功默认关同步 |
| U17 PM失败恢复 | A成功采样后失败，再Bridge重启/切B | 同MAC上次有效样本和最近失败分开（持久范围以P2实现声明为准）；不能显示B沿用A样本；计数范围和单位正确 |
| U18 能力组合 | 新桥旧ROM、新新、双MAC混合 | 旧ROM不显示已启用15轮/镜像证明；新能力按MAC显示；无需新GATT characteristic |
| U19 批次与诊断 | 第15轮无认证、批次慢进展、gap、确认请求失败但batch完成 | 到期等待开网、进度和gap可见；batch完成不等于OTA完成；UI不启动新HTTP/开网循环 |
| U20 渲染/构建边界 | 长MAC/job/error、560×680和窄窗口、快速切页 | 无横向溢出、错误可折行、键盘可操作、删除DOM无悬空引用；实际EXE证明包含最终UI，未验项如实记录 |

验证只为上述实际风险补测试；不新增UI框架、通用历史数据库或全项目无关回归。构建后若要替换默认Bridge，严格按AGENTS的watchdog→计划任务→主进程顺序与路径核对，保留运行data；此步骤只有后续获得执行授权才做。

## 10. 代码定位与明确范围外事项

实施定位：

- `bridge/crates/app/ui/index.html`：设备HTML约240–408；`refresh`约955；`renderDeviceIdentity`约993；`refreshDevice`约1073；PM约1124；`forceSync/togglePause`约1215；`renderRegisteredDevices/pickDevice/applyDeviceSelection/refreshPlatformDevice`约1316；字体/Profile/发布/恢复约1441–1556；`refreshPlatformPower`约1558。
- `bridge/crates/app/src/main.rs`：`get_status`约927；设备动作约1999；`platform_power/platform_plan/platform_status_refresh`约2237；`get_device_status`约2271；`get_pmstats`约2349；`force_sync/request_sync/set_paused`约2509。
- `bridge/crates/app/src/platform.rs`：`refresh_status`约965；`ble_cycle`约1552；`enqueue_ota/deliver_ota`约1772；`tool`中的OTA/PowerPlan/recovery路由。
- `bridge/crates/core/src/platform/service.rs`：OtaJob/blocks_following_work约35；OTA队列/取消/版本证据约948–1100；`devices`约1320；`note_authenticated_status`约1420；`note_ble_contact`约1644。`platform/model.rs`保存DeviceRecord与快照类型。
- 固件来源核对：`src/v2_status_snapshot.{h,cpp}` 的HTTP完整状态与 `src/main.cpp` 的BLE `status` 分支当前并非同一份字段集合；按design P2接线，不在UI凭空补值。
- 既有视觉与运行记录：`project-workflow/bridge-ui-redesign/{plan,task,review,status}.md`；仅继承可用视觉和窗口限制，本轮范围覆盖旧的“余量放设备页/所有DOM必须保留”要求。

明确不在本文实施范围：数据页排版；模板页具体迁移布局；新的OTA上传表单/协议/终态；随时按需无due开Wi-Fi；bridge_first反向HTTP；扩充普通BLE完整遥测/日志/镜像证明；多设备原子发布；新增通用历史数据库；全局Bridge设置/开机自启设计；长期RF成功率、整机耗电/续航结论；未经授权的构建、部署、OTA和实机故障注入。

完成交接需列出：各M/U项结果、源码与运行EXE身份、实际设备能力、未验窗口/硬件项。sync-v1未实现/部署的字段必须继续标为计划，不用UI假数据掩盖缺口。
