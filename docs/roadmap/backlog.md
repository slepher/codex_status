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
| 两设备可达性 | **2026-09-25 15:2x 已按键唤醒、实测可达**（`/status.json`+ARP 双证，MAC 与登记一致）；此后注意深睡仍会不可达 |
| Note4 已装固件 | `0.18.23-note4-b`（09-25 实机核实：`0.18.23-note4-b`，slot `ota_1`，`wake=ext1`；PROGRESS 09-24 节里的 panel-power / 回退修正**未**刷入） |
| 1.54 已装固件 | **2026-09-25 OTA 到 `0.18.23-bw`**（slot `ota_1`，1,740,576 B，`91937B18…`；此前为 `0.17.10-bw`）|
| Note4 ROM | `.pio/build/zectrix-note4-b/firmware.bin` 1,758,032 B，SHA256 `42AAF00B…BCF72E`（= 当前 `main.cpp` 的 `0.18.23-note4-b`） |
| 1.54 ROM | `.pio/build/esp32-s3-epaper-154g/firmware.bin` **1,740,576 B，SHA256 `91937B18…85CF`（2026-09-25 15:58 用当前源码重建，`0.18.23-bw`）——当日 16:10 已 OTA 进设备**；旧的合并期产物 1,744,496 B / `E9046F6B…` 已被覆盖 |
| 桥 | `bridge/target/debug/bridge-app.exe` 2026-09-25 04:43 构建（含唤醒窗口下界修复 + 族同步保留修正）。**2026-09-25 15:13:45 起运行中：PID 21848 + watchdog 41784**（8765/8766/8767 在听）；起桥须用非受限命令（受限沙箱会假失败并回收子进程，见 A1/A2 结论） |
| 1.54 队列/模板 | **已解决（2026-09-25）**：`bundle_jobs` 为空；job `2bfc710c` = `succeeded`，设备 `committed_job_id=2bfc710c`、`v2_templates=2`(`mini,quad`)、`active=quad` 并已渲染。残留：数据帧被拒 `incomplete` → **见 A7** |
| Note4 正常 | 最新 job `1b4500ad` = `succeeded`；2026-09-25 实机 `data_seq=applied_seq=63`，`committed_job_id=1b4500ad`/`commit_seq=267` |

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
- **结论（2026-09-25 实机复核，详见 `PROGRESS.md` §A1/A2）**：
  - 队列已清干净：`platform_publish_cancel {"mac":"70041DD7A340"}` 实测对 `sending` 有效（工具描述 "queued (unstarted)" 不准确），`bundle_jobs` 已为空；`jobs[]` 里的 11 条 `waiting` 是**孤儿历史**（`refresh_bundle_history` 只在终态转换时改写，`service.rs:318-326`），不是活状态。
  - 但**数据仍不通**：一次干净重发（`e60830a4`，55,312 B）在 BEGIN + 14 个 CHUNK 全 200 后，被设备在 **COMMIT** 拒绝，ACK 为 `{"result":"rejected","error":"oom"}`。flash 余量检查（`bundle_store.cpp:433-436`，不足会报 `"space"`）**没有**触发 → **不是"8 MB flash 装不下"**，而是 commit 期逐模板重建编译缓存时的 `malloc` 失败（`bundle_store.cpp:467-468`；1.54 空闲堆 ≈100 KB）。
  - 原始卡死同型：09-24 04:48–05:19 对 `83f4324c` 也是"chunks 全 200 → COMMIT 被拒"，之后作业再无状态变化。**桥侧把"非 applied 的 COMMIT ACK"当成"保持 sending"**（`coordinator.rs:624-636` 只在 `committed==true` 改状态；`service.rs:922` 只在 committed 时刷新历史）→ 该设备被永久顶住。这是独立缺陷，建议先修（落 `Failed`+`last_error` 或加重试上限/退避）。
  - 设备另有第二个故障：`[v2] active load failed: ct_abi` → `[v2] cannot rotate unknown-retention context; data disabled`，即已装 Bundle 的编译缓存 ABI 与固件不符，固件自关 data（`/v2/data` 返回 `rejected`）。建议核实 `bsLoadCompiled` 的"用槽内 source 重建缓存"兜底为何不生效（`bundle_store.cpp:568-591`）。
  - **第二次尝试（用户授权，1.54 冷启动后 19 s 内发，堆最新鲜）给出了第二种拒绝**：新作业 `37217f05`，BEGIN + 14 分片全 200，COMMIT 返回 **`error:"owner"`**（不是 oom）。该错误全仓只有一处产出：`v2_bundle_command.cpp:144-158` 比较「COMMIT 请求体的 `bridge_id`」与「flash 暂存载荷里的 `bridge_id`」，且它位于 CRC(`:138`)与 session 匹配(`:110`)之后 ⇒ 请求体解析正常，是**设备侧重读 55 KB 暂存载荷时没拿到 `bridge_id`**（退路 `server.arg("bridge_id")` 桥从不发）。桥侧同值：`deliver()` 先 `bind_pending_bundle_owner`(`platform.rs:685`→`service.rs:844-871`) 强制 `payload.bridge_id == link.bridge_id`，不一致就不发包。
  - **瓶颈判定为设备固件**：1.54 跑 `0.17.10-bw`，早于 `ca4022c`（0.18.23「stream bundle install from file」＝`oom` 那段的重写）；`owner` 检查本身（`b702aa3`，09-24）在 0.18.23 中未改。另注意**当前桥 exe（04:43 构建）成功提交数 = 0**（Note4 最后一次成功 `1b4500ad` 是 09-24 23:20，早于该构建）。
  - 设备另有第二个故障：`[v2] active load failed: ct_abi` → `[v2] cannot rotate unknown-retention context; data disabled`，即已装 Bundle 的编译缓存 ABI 与固件不符，固件自关 data（`/v2/data` 返回 `rejected`）。建议核实 `bsLoadCompiled` 的"用槽内 source 重建缓存"兜底为何不生效（`bundle_store.cpp:568-591`）。
- **✅ 已解决（2026-09-25 18:1x，详见 `PROGRESS.md` 顶部两节）**：把 1.54 OTA 到 `0.18.23-bw`（`firmware_ota`，54.1 s，ROM `91937B18…`）后，**一次 `platform_publish` 即 applied**：job `2bfc710c` = `succeeded`，设备 `committed_job_id=2bfc710c`、`v2_templates=2`(`mini,quad`)、`active=quad` 并渲染（`[clk] reserved …`），`[bundle] install total=78150 free=1810432`。**`oom`、`owner`、`ct_abi`/`data disabled` 三者同时消失** → 证实瓶颈是 0.17.10-bw 的 commit 路径（0.18.23 的流式安装 + 一次成功安装写入的新编译缓存/新 context）。
- **残留（新开条目，见 A7）**：数据帧仍被设备拒 `incomplete`（字段条目数 ≠ 已装模板的远端 requirement 数），根因在桥侧字段契约 + Profile 的 `static1` 绑定。
- **完成判据**：该 MAC 下不再有非终态作业（**已达成 2026-09-25**）；1.54 的 `data_seq/applied_seq` 开始推进，或给出确切失败阶段与根因并转 C1。**实机结论是后者**，且须更正原判据里的候选："设备侧 8 MB flash 装不下" **不成立**——flash 余量检查未触发，失败发生在 commit 期的堆分配（`oom`）。

### A7. 1.54 数据面 `incomplete`：桥字段契约落后于设备的本地模板切换（2026-09-25 新开 → **已修并实机验收**）
- **现象/证据**：OTA 到 `0.18.23-bw` 并成功安装 Bundle（job `2bfc710c`）后，第一帧数据 `seq=82` **applied**（当时设备在 `mini`），设备按键本地切到 `quad` 之后每帧都被拒；`platform_push_now` 的原始 ACK 给出确切原因 `error:"incomplete"`。
- **机制**：`incomplete` 出自固件 `v2_runtime.cpp:114-118`（`fields.size() != remoteCount`，remoteCount = 已装模板里 `kind<=9` 的 requirement 数；其前的 CRC 检查 `:110` 已通过）→ **是条目数不符，不是内容错**。桥侧 `wire_fields`(`coordinator.rs:949-966`) 对每条 requirement 各发一条、缺值发 `v:null`（固件 `:128-133` 允许 null）——所以前提是"契约里的 requirement 列表 == 已装模板的"。
- **根因（已确认）**：桥只在 **context 变化**时按设备上报的模板重建契约（`service.rs:1298-1321`，在 `note_device_status` 的 reconcile 分支内），设备"本地按键切模板"被桥提前采纳 context 后契约不再刷新 → 桥继续按上一个模板（`mini`，2 远端字段）发，设备按 `quad`（8 远端字段）校验。
- **修复（2026-09-25）**：`note_device_status` 增加"设备上报的 `active_template_id` 与契约不同且该模板在 Profile 内 → 重建契约"（`set_contract`）；新增单测 `device_side_template_switch_refreshes_the_data_contract`；`cargo test -p bridge-core` **85 项全绿**，桥已重建并重启。
- **叠加的配置缺陷已一并修掉**：1.54/Note4 的 Profile 绑定原本有 5/… 条指向测试源 `static1`（只提供 `weekly.usedPercent`/`weekly.resetsAt`）→ 现在两个 Profile 的绑定**全部指向真实源 `codex`**（1.54 8 条、Note4 7 条）。
- **实机验收**：`platform_publish` job `48b47968` = applied/displayed；`platform_push_now` → **`{"op":"data","result":"applied","display_state":"displayed","data_seq":1}`**，设备 `data_seq=1 applied_seq=1 display=displayed renders=9`。
- **仍待做**：按一次 1.54 按键本地切到 `quad`，现场确认契约会跟着刷新（单测已覆盖逻辑）。

### A3. 修掉 Note4 深睡时钟残影（唯一影响日常观感的显示缺陷）
- **现象**：deep 期间只写时钟窗口，残影累积；`SYNC`/用量属整帧元素所以停在最后一次整帧（现场曾被读成 `04:64`）。
- **根因（已用代码核实，2026-09-25）**：**v2 设备上时钟窗口的 ghost 预算根本没有被评估**。预算判定只在 `deepNetworkCycle()`（`main.cpp:5166` 渲染门 / `5177` 强制清影），而它唯一的调用点在 `main.cpp:5697`，进入条件是 `!(deepWakePath && v2BundleReady)`（`main.cpp:5678`）。Note4 有已提交 v2 Bundle → 走 `5678-5693` → `deepNetworkCycle()` 从不被调用 → `rtcClkPartials >= CLK_GHOST_LIMIT` **永不成立**，计数器只增不减。v2 会合真正走的 `v2RendezvousClockRender()`（`main.cpp:4617`）只检查 `clkR.valid`/`activeTplHasNow`/`timeKnown()`/`clkPixelsValid`，**无预算检查**。
- ⚠️ **曾经写错、此处纠正**：上一版这里写过「已过时，rtcClkPartials 现已覆盖两条路径」——那是错的，更早的 `PROGRESS.md:37` 才对。落笔前请自己复核 `main.cpp:5678` 的分支条件。
- **深睡路径上两个预算都不生效**：`epdPartialCount` 是普通 RAM（深睡清零），而薄唤醒 `deepThinWake()`（`5275/5283`）在 `main.cpp:5528` 早于 `5531` 的 `epdBegin()` 就返回。
- **两个 `RGN_CLOCK` 不是同一件事**：`refresh_policy.cpp:97` 的 `5` 是 `classRank()` 合并优先级；`:108` 的 `90` 才是 `classDefaultBudget()` 预算，且只被走 `epdFlush()` 的局刷消费，直接写时钟窗口的路径不经过它。
- **改法**：在 `src/refresh_policy.h` 落单一来源宏（该头已被固件与宿主共同包含），`main.cpp:118` 与 `refresh_policy.cpp:108` 都改为引用它；再给 `v2RendezvousClockRender()` 补上 budget 检查（`forceCleanRefresh` + `renderCurrent()` + `clkCaptureFromFramebuffer()` **之后**才清 `rtcClkPartials`）。预算先从 90 调到 30 并实机验收。
- **工单**：`docs/roadmap/prompts/A3-note4-clock-ghost-budget.md`。**完成判据**：连续 ≥90 分钟深睡后时钟无可辨残影，`clk_partials` 到预算后归零、`epd_busy_fails` 不增长，且全刷频率可接受。

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

### A6. ~~别让族发布把 1.54 的 `sync_enabled=true` 覆盖回 false~~ → **已关闭（2026-09-25 核实）**
- **原判断（错）**："源码已修但未构建进运行桥"。用产物时间戳复核后证伪：
  - 修正本体在 `bridge/crates/app/ui/index.html:541`（`sync_enabled: device.profile?.sync_enabled ?? false`，
    即从**目标设备 Profile** 复制、不再取族草稿），引入于提交 `fc3c464`（**2026-09-25 04:04:10**）；
  - 此后 `index.html` **再无改动**（`git log fc3c464..HEAD -- bridge/crates/app/ui/index.html` 为空）；
  - 运行桥 `bridge/target/debug/bridge-app.exe` 构建于 **04:43:36**，比该提交晚 39 分钟。
  - → 该 exe 的 UI 源码含此修正，**当前桥的族发布已经会保留设备侧的 `sync_enabled`**。
- **实际风险等级下调**：1.54 族草稿仍是 `sync_enabled=false`，但它现在**不会**被复制到设备。
  设备 Profile 自身是 `true`，从当前桥再发布也仍然是 `true`。只有当用户**回退到旧 exe** 才需要担心。
- **残余的一行记录**：若将来重建桥，重建后请顺手确认 `index.html:541` 仍是这个语义（别被"族配置优先"的改动带回旧行为）。
- （旧的"未构建"结论来自 `bridge-family-sync-preservation/status.md`，该文件写于构建之前；归档摘要沿用了它。已在本行修正。）

## 3. 暂缓（B 级：有价值但不阻塞，排期在 A 之后）

### B1. Note4 panel-power 双模式 + 帧缓存自愈修正上机验收
- 源码已实现：NVS `pm/panel_pwr`（`keep`/`off_cache`）、深睡保存整帧缓存 + RTC 哈希、`note4RestoreFrameBaseline()` 自愈修正。
- **已核实无需重编**：`panel_pwr` 与 `note4RestoreFrameBaseline()` 都引入于 `e226d8e`，而该提交**早于** `0.18.23` 的源码提交 `af2b607`（`git show af2b607:src/main.cpp` 里两处都在），且 `af2b607..HEAD` 未再改动 `src/`。**所以已 OTA 的 `0.18.23-note4-b` 就含这两项**；`artifacts/codex-status-0.18.21-note4-b-panel-modes.bin` 只是过期的中间候选，可忽略。
- B1 因此**只剩实机测量**，不含编码：`keep` vs `off_cache` 的电流/波形/残影/重复按键唤醒对照；`/diag?panel_power=keep|off_cache` 与 `/status.json.panel_power_mode` 回读；深睡缓存命中与 `note4RestoreFrameBaseline()` 自愈是否真的被走到（看 `/log`）。

### B2. ~~1.54 族发布路径上的同步策略~~ → 随 A6 一并关闭
- 结论：当前桥的族发布取自目标设备 Profile（`ui/index.html:541`），族草稿的 `sync_enabled=false` 不会被复制。详见 A6。

### B3. Note4 模板发布图标校正版
- `note4-icon-correction` 已完成源码与逐像素对拍（sleep-20 恢复为 zzz、On/Off 位图互斥、电量区改 `[344,5,44,26]`）。
- 未做：保存到桥模板库、显式发布、实机验收。这是纯用户显式动作，无技术风险。

### B4. 1.54 时钟窗口保留修正 + 会合交替验收
- `clock-window-retention` 已改源码（同一 context + 相同时钟区域才保留 RTC 时钟像素），**只做过 1.54 `pio run` 构建，未刷机**。
- **2026-09-25：构建侧与刷机侧都已完成** —— 154g 隔离目录 + 当前源码 ROM 就绪（`91937B18…`，`0.18.23-bw`，含 `ca4022c` 与时钟窗口修复），并已 **OTA 进设备**（`0.17.10-bw → 0.18.23-bw`，54.1 s）；同一次会话里 Bundle 也装成功（A2 关闭）。见 `PROGRESS.md` 顶部两节。
- 待做：按 `/history` 的每分钟唤醒类型 + `clk_partials` + 可见刷新类型三项对照（**实机验收**）；现在设备跑的是 `0.18.23-bw`，验收条件已具备。

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
- `tools/pio-target.ps1` 已落地，**两侧都已实测**（2026-09-25）：`.pio-pkgs/note4` 与 `.pio-pkgs/154g` 各 2 real（两个冲突 framework 包，69 MB + 2057 MB）+ 15 junction，各 ≈2126 MB。
- **154g 构建成功**：`pio run -d <repo> -e esp32-s3-epaper-154g` → SUCCESS 102.38 s，无 banner、无重装；产物 `1,740,576 B / 91937B18…`（`0.18.23-bw`），另见 `PROGRESS.md`「多 env 实测」节。
- **判据 c/d/e 已实测（结论与预期相反）**：**交替构建仍会触发 framework 重装**——`sdkconfig.defaults` 是仓库根唯一生成物，切到 154g 后首行变成 154g 指纹，切回 note4 即触发 `*** Reinstall Arduino framework ***`。隔离的价值是**重装只发生在目标自己的包目录里、不伤对方**（整轮两个目录完好，note4 ROM hash 未变 `42AAF00B…`），**不是**省掉重装代价。
- **重装在受限沙箱里跑不完**：uv 要写 `%LOCALAPPDATA%\uv\cache`（工作区外）→ `os error 5 拒绝访问` → `Failed to create a proper virtual environment`；`UV_CACHE_DIR` 指到工作区内可解这一条，但 `tool-esptoolpy` 的 editable 安装仍因写 junction 指向的共享包目录报 `Cannot update time stamp of directory 'esptool.egg-info'`（非致命警告）。要完整跑通需提权或把这两条路径纳入工作区。
- **脚本坑**：`tools/pio-target.ps1` 在**后台作业**里会假失败——它 `& pio run` 时 cwd 丢失成 `D:\Documents\project`；给 pio 加 `-d <repo>`（或在前台 shell 跑脚本）即可。
- `next.md §6` 的 `extends` 重构未做。「只构建 Note4」现仍成立，但已明确是**成本**约束。
- **当前决定：不主动推进**；需要 1.54 固件时（见 A2/B4）用上面已验证的路径。

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
| D11 | ~~已存在文件的 DACL 缺沙箱能力 ACE~~ → **已解决（2026-09-25）** | — | 用户修好 ACL 后，`generic-display-platform-design` 与 `live-template-delivery` 已成功 `git mv` 到 `docs/history/workflow/`（归档总数 17 个）。关键判据：**真正挡路的是 ACE 不是属主**——ACE 补齐后，即使属主仍是 `CodexSandboxOffline`，`git mv`/`Add-Content` 都能成功。已把该结论记入 `PROGRESS.md` 与 `docs/history/acl-repair-notes.md` |
| D12 | **构建夹具放在文档目录里**：`codex-status-a-400x300.json` 被 3 处 Rust 代码 `include_str!`/`read_to_string` 引用（`core/src/compile.rs`、`core/src/platform/service.rs`、`render/tests/compiled.rs`），却放在 `project-workflow/.../concepts-400x300/` | "归档文档"其实是构建依赖，删文档就会断构建 | **已解决**：夹具复制到 `bridge/crates/core/tests/fixtures/codex-status-a-400x300.json`（SHA256 与源文件一致 `A429D4E0…B48F5`），3 处引用已改指新路径，`bridge-core` 与 `bridge-render` 测试全绿。源文件留在 `generic-display-platform-implementation` 下但已无引用 |
| D13 | **仓库里仍有 64+ 个对象属主是 `CodexSandboxOffline`**（ACE 已补齐，所以能读写；属主本身不阻塞） | 只影响观感与后续审计；若哪天 ACE 再次缺失，这些对象又会变成不可写 | 想彻底清干净就在**管理员** PowerShell 里跑 `docs/history/acl-repair-notes.md` 的脚本（必须 `elevated? True`）。清单：`project-workflow/generic-display-platform-design/{plan,status,task-1}.md`、`generic-display-platform-implementation` 下 55 项、`ble-rendezvous-power/{plan,status,task-1}.md`、`src/EPD_SSD2683.cpp`、`src/font_noto_ntreg96.h`、`bridge/crates/core/src/platform/publish.rs`、`bridge/crates/render/tests/bundle_store.rs`、`bridge/assets/fonts/font_ntreg96.bin`、`partitions_note4.csv`、`docs/ble-rendezvous-power-design.md`、`docs/fake-rom-simulator-design.md`、`docs/history/generic-display-platform-design-v1.md`、`docs/history/workflow/` 下 5 个已归档专项 |

## 6. 归档映射

| 归档文件 | 内容 |
|---|---|
| `docs/history/progress-archive-<date>.md` | `PROGRESS.md` 下沉的历史里程碑节 |
| `docs/history/workflow/<initiative>/` | 已结项的 `project-workflow/*` 目录原件 |
| `docs/roadmap/archive-digest-legacy.md` | 旧专项（09-09 ~ 09-24）一页式摘要与"还欠什么" |
| `docs/roadmap/archive-digest-recent.md` | 近期专项（09-23 ~ 09-25）一页式摘要与"还欠什么" |
| `docs/README.md` | 文档总索引与"该读哪一个"的导航 |
