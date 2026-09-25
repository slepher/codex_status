# 需求与待办总清单（单一事实来源）

> 建立：2026-09-25。用途：把所有散落在 `PROGRESS.md`、`project-workflow/*/status.md`、
> `next.md`、`bugs.md` 里的需求与遗留收敛成一份**可执行**清单。历史过程不入本文件，
> 只入 `docs/history/`。
>
> 维护规则：**只有本文件是"待办"的唯一事实来源。** 完成一项就把结论与证据（ROM/SHA256、
> 测试结果、实机判据）搬到 `PROGRESS.md` 现场节，并在本文件删除该条。新增需求先入本文件
> 再开 `project-workflow/<initiative>/`。

## 0. 当前设备与产物快照（2026-09-25 核查）

| 项 | 值 |
|---|---|
| 1.54" 设备 | MAC `70041DD7A340`，显示名"书桌屏"，IP `192.168.3.163`，`sync_enabled=true`，Profile `mini,quad` |
| Note4 设备 | MAC `7C4FADB93408`，显示名"Note4"，IP `192.168.3.177`，`sync_enabled=true`，Profile `codex-status-a` |
| 两设备可达性 | **均 ping 不通**（最后 ACK 约 2026-09-25 05:2x，距核查 5.9 小时）→ 任何实机任务的前提是先唤醒设备 |
| Note4 已装固件 | `0.18.23-note4-b`（如 09-25 05:08 OTA 记录仍成立；PROGRESS 09-24 节里的 panel-power / 回退修正**未**刷入） |
| 1.54 已装固件 | 最后记录 `0.16.7-bw`；master 上的时钟窗口保留修正（`clock-window-retention`）**未**刷入 |
| Note4 ROM | `.pio/build/zectrix-note4-b/firmware.bin` 1,758,032 B，SHA256 `42AAF00B…BCF72E`（= 当前 `main.cpp` 的 `0.18.23-note4-b`） |
| 1.54 ROM | `.pio/build/esp32-s3-epaper-154g/firmware.bin` 1,744,496 B（08-25 00:55 构建，非当前源码；**当前源码只构建 Note4**） |
| 桥 | `bridge/target/debug/bridge-app.exe` 2026-09-25 04:43 构建（含唤醒窗口下界修复 + 族同步保留修正）。**核查时进程未运行**（PID 文件 `artifacts/bridge-app-run.pid` 陈旧） |
| 1.54 卡住的作业 | Bundle job `83f4324c` 状态 `sending`，另有多个 `waiting` 作业积压（含 `91b51cdb`） |
| Note4 正常 | 最新 job `1b4500ad` = `succeeded`，`data_seq=applied_seq=57` |

## 1. 产品需求 → 实现状态

| 需求 | 状态 | 缺口 |
|---|---|---|
| ESP32-S3 墨水屏显示 Codex 余量，换样式只换模板 JSON | 已交付（固件 0.18.x + 桥） | 无 |
| v2 通用平台：Profile 1–8、CompiledTemplate、完整 A/B Bundle、单一 active context、Bridge 生成 PowerPlan | 已交付并实机跑通 | Bundle v3 压缩（见 C1） |
| 显式 claim/lease 占用、MAC 为身份、显示名可改 | 已交付 | 无 |
| 端口鉴权 token（`/update`、`/doUpdate`、OTA、`POST /claim`） | 已交付 | 无 |
| Note4 400×300 模板 `codex-status-a` | 已交付并实机显示 | 图标校正版（`note4-icon-correction`）**未保存到桥、未发布**（见 A2） |
| Note4 时钟每分钟局刷（不做整屏全刷） | 部分交付 | v2 会合路径的 ghost 预算**已补齐**；Note4 深睡断电导致基线丢失仍会回退全刷，需实机验收 |
| 深睡功耗与电池续航 | 部分交付 | panel-power 双模式已实现但未刷入；bench 电流/残影/重复唤醒**从未测量**（见 D1） |
| 1.54 设备时钟局刷与全刷交替 | 源码已修 | 未刷机、未实机验收（见 B4） |
| 字体引擎化（CSFN 容器 + 设备字体库 + manifest 增量发布） | 引擎已实现，协议未定稿 | `note4-bridge-publish/protocol.md` 仍是**待双方确认草案**，设备侧无增量端点（见 D3） |
| Bridge 多设备界面 + 按 MAC 推送 | 已交付 | 族发布保留设备同步策略的源码修正**未构建进运行桥**（见 B2） |
| Bridge 多实例 | 已交付（tag `bridge-multi-instance-2026-09-24`） | 无 |
| Fake ROM 设备模拟器（同源 C++ + 可控实验时钟） | 进行中，无可用产物 | Stage D 未完成、E/F/G 未开始（见 C2） |
| 唤醒会合诊断（一次唤醒一条记录） | 仅计划 | 固件与桥两侧都未实现（见 C3） |
| Bundle v3（manifest + 原始 dense 对象，省 ~71%） | 仅设计 | 未实现（见 C1） |
| `device_first` / `bridge_first` 双策略会合 | 仅设计 + spike | 未实现（见 C4） |
| 功耗基线 A/B（DFS 40/80、BT modem sleep、会合节奏） | 未做 | 30–60 分钟基线从未采集（见 C4） |

## 2. 紧急（A 级：先做这些）

### A1. 让桥恢复运行并恢复两台设备的可达性 — 阻塞几乎所有实机任务
- **为什么紧急**：桥进程当前不在运行；两台设备 ping 不通。不解决则 A2/A3/B1/B3/B4 全部无法验证。
- **具体动作**：
  1. `pwsh tools/start-bridge.ps1` 起桥（立即返回，日志进 `artifacts/`），确认 `127.0.0.1:8766/mcp` 监听、PID 文件刷新。启动前核对 `artifacts/bridge-app-run.pid` 内的陈旧 PID 是否被复用。
  2. 唤醒两台设备（Note4 按键 / 1.54 按键），核对 `GET /status.json` 的 `fw`、`slot`、`wake`、`ip`。
  3. **1.54 的 IP 疑点**：state.json 记 `192.168.3.163`，但 `AGENTS.md` 硬件现场写 `192.168.1.50`。要求以设备 `/status.json` 与 ARP 实际结果为准，并把正确值同时写回 state 与 `AGENTS.md`（见 E1）。
- **完成判据**：桥 HTTP/MCP 监听；两台设备各自 `status.json` 可读且 MAC 与登记一致。

### A2. 清掉 1.54 卡住的 Bundle 作业队列
- **为什么紧急**：`bundle_jobs["70041DD7A340"]` 停在 `state=sending`（`83f4324c`，55,312 B，`saved_at` 1790053568），另有 6+ 个 `waiting` 作业积压。Coordinator **先处理未完成 Bundle 再处理 Data**，所以这台设备的数据投递会被永久顶住。
- **具体动作**：设备可达后先只读核对设备侧 `committed_job_id`/`commit_seq`；确认哪个作业真的没提交，然后显式取消/重发，**不要**批量重发全部 `waiting` 作业。
- **完成判据**：该 MAC 下不再有非终态作业；1.54 的 `data_seq/applied_seq` 开始推进，或明确判定为"设备侧 8 MB flash 装不下"（转 C1 处理）。

### A3. 修掉 Note4 深睡时钟残影（唯一影响日常观感的显示缺陷）
- **现象**：deep 期间只写时钟窗口，残影累积；`SKIP`/用量属整帧元素所以停在最后一次整帧（已被误读为 `04:64`）。
- **现状核查**：`src/main.cpp` 的 `rtcClkPartials++`（1816）与 `rtcClkPartials >= CLK_GHOST_LIMIT` 判定（5166/5177）现在**同时覆盖** light tick 与深睡会合时钟路径；`0.18.23-note4-b` 已含此逻辑。所以"v2 会合路径没有 ghost 预算"这条 PROGRESS 记录**已过时**。
- **仍需做**：实机确认 90 次预算是否合适（小面积高对比区建议调小到 ~30，与 `epdPartialCount` 的 30 对齐）；确认调小后整屏全刷频率可接受、无可见闪烁。改 `CLK_GHOST_LIMIT` 与 `refresh_policy.cpp:108` 的 `RGN_CLOCK` 预算必须同步。
- **完成判据**：连续 ≥90 分钟深睡后时钟无可辨残影，且全刷次数有日志计数。

### A4. 关闭 `bugs.md` 的两个 v2 数据缺陷（已修，需归档结案）
- **核查结论**：BUG-1 与 BUG-2 **都已在 v2 实现中修复**——
  - BUG-1（ACK CRC 永不匹配）：`platform.rs` 的 `note_ack` 传的是 `body["crc"]`，而 `coordinator.rs:478` 起 `content_crc = data_fields_crc(wire_fields(...))`，与发送给设备的 `crc`（`coordinator.rs:844`）同源；`service.rs:2397` 有断言 `decision["content_crc"] == body["crc"]`。
  - BUG-2（唤醒重放冻结旧快照）：`main.cpp:5637-5649` 在 deep 唤醒时 `v2DataCheckpoint.restore(...)`，无已应用基线则 `v2SwitchActive(v2Profile.initial)` 轮换 context，旧 seq 不再是新包。现场 `data_seq=applied_seq=57`（Note4）与 `0 0`（1.54，无可达数据）都不再出现 BUG-1 的"永不推进"。
- **具体动作**：把 `bugs.md` 改成结案记录（写明修复点与证据），或整体归档到 `docs/history/bugs-2026-09-22.md`；不要留着一个看起来"均未修复"的文件误导下一轮。
- **完成判据**：`bugs.md` 不再声称未修复；修复点有文件行号指向。

### A5. 把 `PROGRESS.md` 从 89 KB 降到可读规模（本项已完成，见 §5）
- 这项已由本轮动手执行：`PROGRESS.md` 只剩最新现场 + 归档索引，其余进
  `docs/history/progress-archive-*.md`。以后每次里程碑只往 `PROGRESS.md` 追加**一节**并
  在超过 ~15 KB 时把旧节下沉到归档。

### A6. 别让族发布把 1.54 的 `sync_enabled=true` 覆盖回 false（活回归风险）
- **风险**：`bridge-family-sync-preservation` 的源码修正（`confirmFamilyPublish()` 从目标设备 Profile 复制 `sync_enabled`/`full_sync_s`）**已写但未构建进运行桥**；同时**1.54 族草稿仍是 `sync_enabled=false`**。只要有人从当前运行的模板页发布一次 1.54 族，隐藏的草稿值就会覆盖设备上刚被显式开启的 `sync_enabled=true`，静默关掉数据同步。
- **具体动作**（择一，写清选了哪个）：① 重建并重启桥，确认运行二进制包含该修正；② 或先把 1.54 族草稿也改成 `sync_enabled=true`。
- **完成判据**：能说清"这次发布之后设备侧 `sync_enabled` 是否可能被改回 false"，并在 `PROGRESS.md` 记录结论。

## 3. 暂缓（B 级：有价值但不阻塞，排期在 A 之后）

### B1. Note4 panel-power 双模式 + 帧缓存自愈修正上机验收
- 源码已实现：NVS `pm/panel_pwr`（`keep`/`off_cache`）、深睡保存整帧缓存 + RTC 哈希、`note4RestoreFrameBaseline()` 自愈修正。
- **没有**进入任何已构建 ROM：候选 `artifacts/codex-status-0.18.21-note4-b-panel-modes.bin`（SHA256 `917FAFAD…C9FD6A`）是 **0.18.21 时代的旧候选**；当前 `main.cpp` 已是 `0.18.23-note4-b` 且已 OTA，需确认这些改动是否已在其中，否则重编。
- 待做：`keep` vs `off_cache` 的电流/波形/残影/重复按键唤醒测量；`/diag?panel_power=` 与 `/status.json.panel_power_mode` 实机回读。

### B2. 1.54 族发布路径上的同步策略（已升为 A6；此处仅留背景）
- 背景：`confirmFamilyPublish()` 的复制逻辑与族草稿 `sync_enabled=false` 的共存问题，见 A6。

### B3. Note4 模板发布图标校正版
- `note4-icon-correction` 已完成源码与逐像素对拍（sleep-20 恢复为 zzz、On/Off 位图互斥、电量区改 `[344,5,44,26]`）。
- 未做：保存到桥模板库、显式发布、实机验收。这是纯用户显式动作，无技术风险。

### B4. 1.54 时钟窗口保留修正 + 会合交替验收
- `clock-window-retention` 已改源码（同一 context + 相同时钟区域才保留 RTC 时钟像素），**只做过 1.54 `pio run` 构建，未刷机**。
- 由于现在只构建 Note4，1.54 需要单独决定：要么用 `.pio-pkgs/note4` 之外的隔离包目录重建 1.54（见 D4），要么承认 1.54 冻结在当前固件。
- 待做：刷入后按 `/history` 的每分钟唤醒类型 + `clk_partials` + 可见刷新类型三项对照。

## 4. 战略/长线（C 级：需要独立 initiative，不在近期窗口）

### C1. Bundle v3（manifest + 原始 dense 对象）
- 设计已定稿（`docs/history/workflow/bundle-v3/design.md`）：`CSB3` 容器 + `ct-dense-v1` 对象，`mini+quad` 从 55.3 KB 降到约 16.2 KB（省 ~71%）。
- **为什么重要**：1.54 是 8 MB flash，55 KB Bundle 的十六进制编码（45 KB 十六进制字符）是它最容易 OOM/超时的环节，也是 A2 的根因候选。
- 需改：固件（新格式解析 + A/B 原子安装 + 拒绝路径）、桥编译器（canonical dense 编码器）、Python 测试桥哈希三方同步；设备能力加 `bundle_format: 3`；v3 ROM 把 pre-v3 Bundle 视为不存在。

### C2. Fake ROM 设备模拟器（Stage D 收尾 → E/F/G）
- 已完成：A（显式时间）、B（多设备运行层）、B2（协议时间线日志）、C（Data/Plan/Bundle/Activate/claim 同源 C++ 决策）、D 的 13a/13b/13c/13d。
- 未完成：**D 剩余**（Data/Bundle/Activate 走共享命令路径、显示效果、持久状态、断电重启端到端）→ **E**（Bridge 按 MAC 目标时钟接线）→ **F**（同步/失步/丢 ACK/重启/multi-device 联合场景）→ **G**（实机回归）。
- 当前 `device-sim` 仍是"未配置设备"：无 Bundle 所以不走 sleep/无线会合，Data/Bundle/Activate 返回 501。不能登记为完整 Fake ROM。

### C3. 唤醒会合诊断记录（wake-contact-trace）
- **代码已实现并已合入 master**（先前记录"仅计划"是错的）：`bridge/crates/app/src/wake_history.rs`、
  `bridge/crates/app/src/platform.rs`、`bridge/crates/ble/src/lib.rs`、`src/main.cpp`、
  `src/v2_status_snapshot.cpp`、`tools/wake-contact-trace.mjs` 都在；`estimate-power.mjs` 已能读
  `format:2` 的 `wake_type` / `awake_ms`。原计划里的 `wake_generation` + `wake_seq` 关联键、
  Bridge 按 MAC 落 JSONL 的持久化都到位。
- **未做**：实机验收（两台设备各 ≥30 次 timer 会合 + 一次按键唤醒 + 一次故意关桥超时，要求能从
  设备记录与桥日志重建完整时间线）；新版记录上限（64 × 48 B）下两种 target 的 RTC map 容量复核。
- **注意**：计划里写的目标 ROM 版本 `0.18.22` 已被实际发布的 `0.18.23` 取代。
- 相关：`tools/wake-contact-trace.mjs` 已存在，可先做离线对拍再上机。

### C4. 功耗与会合策略（power-plan-c / next-execution-plan）
- 未做：30–60 分钟基线采集（≥30 个 deep 周期）、A2/A3（DFS 40/80 × BT modem sleep）、A5（整分钟对齐导致的 ~2 分钟间隔）、A4（`device_first` vs `bridge_first`）。
- `bridge_first` 只有 spike 通过，生产协议/策略切换未实现；`task-5-advert-rendezvous.md` 的认证载荷/密钥/窗口规则待冻结。
- 前置：C3 的记录格式（否则没有可信的对照口径）。

### C5. 字体资产增量发布
- 引擎侧已交付（单一字体注册表 + CSFN 容器 + 设备字体库），整包 Bundle 可发布。
- 未定稿：`docs/history/workflow/note4-bridge-publish/protocol.md`（manifest/CSFN 增量发布合同）仍是**待双方确认草案**；设备侧无增量端点、仍是 ABI 1 时代的 48 KiB/8 字体实现。
- 结论：在 C1（Bundle v3）定案前不要启动，否则两套编码器要一起改。

### C6. 多 env / packages 目录隔离收尾
- `tools/pio-target.ps1` 已落地并验证 note4 隔离（`.pio-pkgs/note4` + `.pio-core`，无 banner）。
- 未做：判据 c/d/e（需构建 154g，用户曾拒绝创建其 packages 目录）；`next.md §6` 的 `extends` 重构未做。
- **当前决定：不推进。** 既然唯一固件目标是 note4，争议点消失。如需恢复 1.54（见 B4），再回到本项。

### C7. 其他单项遗留（已归档专项带出来的、仍然有效的条目）

来源：`docs/roadmap/archive-digest-legacy.md` 与 `-recent.md` 的"未完成/遗留"。这些不构成新的 initiative，但**不能因为归档就当作已完成**；要动哪一块就在对应目录（现在在 `docs/history/workflow/`）里查上下文。

| 专项 | 仍然有效的遗留 |
|---|---|
| `ble-rendezvous-power` | **stage 3–6 全部未实现**（GATT v2 事务；时钟优先会合 + radio 硬切断 + `POST /power`；桥常驻监听 + WAKE/RENEW/SLEEP；分阶段启用与实测功耗）。另：AP/桥不可达时的失败路径未测；stage 2 固定机位照片验收与 90 次局刷 soak 未做；stage 1 的 BLE 回归需一次 BOOT 点击 + 用户提供基线照片；task-10 的 BOOT blink 检查与"无 AP"失败运行未做；task-8 的 5 项 OTA 改进未做 |
| `power-state` | task-5 **整机硬件验收整项未做**：按键/GP3 LED、插电拔电宽限、低电 5% 断电、WIFI OFF 深睡节奏、T10 电池斜率、T9 push 延迟、UDP 换 IP 生效、桥失联 `OFF N M`、OTA 回归、三端模板哈希一致 |
| `bridge-multi-device-ui` | 按 MAC 的发布路由与统一族推送菜单未实现（当前禁用）；`family_publish_preview` 预检事务未接线；0/1/多设备、跨族、离线、被占用、在途作业的验收全未做；没有双设备实机验证 |
| `pmstats` | T10 拔电电池斜率对照（5 ms vs 25 ms）未做；诊断面 `?timers=1` / `?diag=1` / `POST /diag` 去留未决（倾向保留） |
| `device-discovery` | ARP 邻居表冷路径（真实换网）未复现；自动 `ble=1` 通告触发路径未捕获到（UDP 广播偶发丢失，机制未变） |
| `deep-pull-test` | deep 窗口内不起 WebServer ⇒ **桥在设备 deep 期间无法 push/OTA**（这是后续 pending 排队设计存在的根因） |
| `sleep-modes` | ≥2 h 长测未闭环（电量斜率/时钟准度/残影/history 逐分钟 thin）；**BLE 打断唤醒仍未闭环**；提交前整理未做（调试开关默认关、`/history` 暴露面、`DEV_Module_Init` 幂等审阅） |
| `note4-buttons` | PGUP/GPIO39 的「仅清醒时切换模板」**不在源码里**（全仓无 GPIO39/PGUP 代码，`nextTemplate()` 只挂在 GPIO0/BOOT 路径）；「ENTER 是否真的唤醒 MCU」仍未取证 |
| `note4-panel-power` | 见 B1；另：候选 ROM 与生成脚本（`ota_verify.py`/`ota_upgrade.py`/`provision_endpoint.py`/`usb_recover_once.py`/`capture_serial.py`）归档后仍需可达 |
| `note4-ota-bringup` | 像素级文字验收未通过（现场照片偏软）；按键/唤醒与更宽温度范围未测；两条恢复路径（B→A、整片工厂备份）文档化但未执行 |
| `generic-display-platform-implementation`（**仍在工作区**） | 字体资产 task-7 未闭环：`File::name()` 修复后固件**尚未重编**；`CtOp.fontRef` + `CT_ABI` bump；`font_inventory`/BEGIN-CHUNK-COMMIT+ACK/`fontStorePrune` 无调用者；桥侧 publish preview / MCP 字体工具 / UI；最终字重字号决策；16 MB 板 assets 分区。**BLE 会合传输未实现**（设备仍走 legacy Wi-Fi deep pull）。固定机位残影照片与 90 次局刷 soak。4 个状态图标绑定未定义。`convergence.md` 的 6 条收敛工作仍全部有效 |
| `generic-display-platform-design` / `live-template-delivery` | 内容已判定为"只读历史"、无遗留，但**目录因 ACL 移不动**（见 D11） |

## 5. 技术债与"屎山"清单（D 级）

| 编号 | 债务 | 影响 | 处理建议 |
|---|---|---|---|
| D1 | `project-workflow/` 原有 246 个受版本控制文件、约 1.07 MB，多数已结项 | 翻找成本高、误导后来者 | **已执行**：15 个结项专项目录移到 `docs/history/workflow/`（剩余 11 个进行中，见 `docs/README.md`）；摘要见 `docs/roadmap/archive-digest-*.md` |
| D2 | 生成物/证据资产被入库：`concepts-400x300/*.png`、`font-plan-400x300/*.png`、`note4-icon-correction/evidence/*.png`、`note4-ota-bringup/*.py` | 仓库膨胀、噪音 | 图片证据随专项目录一并进了 `docs/history/workflow/`；后续新证据统一放 `artifacts/`（已 gitignore） |
| D3 | `bugs.md` 声称两个缺陷"均未修复"，实际已修 | 直接误导 | **已执行**：结案为 `docs/history/bugs-2026-09-22.md`（含行号证据），见 A4 |
| D4 | `next.md`（33 KB）主体是已完成的合并/多 env/包隔离记录，头尾仍像"开工指引" | 新窗口会照着过期步骤做（其 §5.2 的 stash 流程已实测不成立） | **已执行**：归档为 `docs/history/next-2026-09-25.md`，§7 包隔离结论并入 `PROGRESS.md` 与 C6 |
| D5 | `AGENTS.md` 的"硬件现场与坑"含过期地址（`192.168.1.50`、`70:04:1D:AA:BB:CC`、"SSID home-wifi"）与已停用目标（`esp32-s3-epaper-154g` 的 40 MHz 闪存脚注） | 每轮会话都注入，直接占用上下文并给出错地址 | **已执行**：改为"以桥登记为准"的设备表 + 历史细节下沉；地址不一致已标注为待核实 |
| D6 | 显示宽度两处不同源：位置来自编译产物 `v2Ct.ops[]`，尺寸在运行时由 `tplFontClockBox()` 重算 | 已在 0.18.23 修掉一类（`2L*digitAdv` → `4L*digitAdv`），但只要模板给时钟加 `rect`+`align` 就会再犯 18 px 偏移 | 把测量宽度写进 `CtOp`，需同步改 Rust 编译器与 Python 测试桥哈希 → 独立任务 |
| D7 | ghost 预算两套计数：`rtcClkPartials`（上限 90）与 `epdPartialCount`（上限 30）并存，`refresh_policy.cpp:108` 又写死 90 | 行为不一致、改一处忘一处 | 统一为单一预算源，`refresh_policy.cpp` 从 `CLK_GHOST_LIMIT` 取值 |
| D8 | ROM 哈希随 packages 绝对路径变化（`pio-pkgs` 字符串被编进固件），同一源码不同路径产出不同 SHA256 | "可复现构建"不成立；跨机器核对 ROM 会误判 | 加 `-ffile-prefix-map` / `-fmacro-prefix-map` 归一化（未做，留作决策） |
| D9 | 构建沙箱归属：新目录若在沙箱内创建会落到 `CodexSandboxOffline` 属主，缺能力 ACE，导致目录内文件写入被拒 | 曾造成 stash 丢 16 个文件的现场事故 | 保持 `AGENTS.md` 的提权建目录约定；不要把 `git stash` 用作保护现场的手段 |
| D10 | `sdkconfig.defaults` 是全项目唯一生成物，被各 target 争写；交替构建必触发 framework 重装（实测 154g 188 s + note4 926 s） | 任何"多目标"操作都极其昂贵 | 已被"只构建 Note4"约定绕过；若恢复多目标必须先落地 C6 的包隔离 |
| D11 | **已存在文件的 DACL 缺沙箱能力 ACE**（`S-1-4-…`），shell 无法删除/改写；`project-workflow/` 下共 86 个文件受影响 | 2026-09-25 归档失败 2 个目录（其余 15 个成功）：`generic-display-platform-design`（3 文件，**所有者还是 `CodexSandboxOffline`**）与 `live-template-delivery`（6 文件，属主是 cogic、只是缺 ACE）。`generic-display-platform-implementation`(63)、`ble-rendezvous-power`(13) 同样缺 ACE，将来改写它们也会被拒 | 两种原因不是同一种：属主错 + DACL 错 vs 只有 DACL 错。修复步骤见 **`docs/history/acl-repair-notes.md`**（提权 PowerShell、走 .NET、不递归）；修完 `git mv` 这两个目录并更新本行 |
| D12 | **构建夹具放在文档目录里**：`codex-status-a-400x300.json` 被 3 处 Rust 代码 `include_str!`/`read_to_string` 引用（`core/src/compile.rs`、`core/src/platform/service.rs`、`render/tests/compiled.rs`），却放在 `project-workflow/.../concepts-400x300/` | "归档文档"其实是构建依赖，删文档就会断构建 | **已部分执行**：夹具复制到 `bridge/crates/core/tests/fixtures/codex-status-a-400x300.json`（SHA256 与源文件一致 `A429D4E0…B48F5`），3 处引用已改指新路径，`bridge-core` 与 `bridge-render` 测试全绿。原文件因 D11 的 ACL 删不掉，留在原处（不再被引用） |

## 6. 归档映射

| 归档文件 | 内容 |
|---|---|
| `docs/history/progress-archive-<date>.md` | `PROGRESS.md` 下沉的历史里程碑节 |
| `docs/history/workflow/<initiative>/` | 已结项的 `project-workflow/*` 目录原件 |
| `docs/roadmap/archive-digest-legacy.md` | 旧专项（09-09 ~ 09-24）一页式摘要与"还欠什么" |
| `docs/roadmap/archive-digest-recent.md` | 近期专项（09-23 ~ 09-25）一页式摘要与"还欠什么" |
| `docs/README.md` | 文档总索引与"该读哪一个"的导航 |
