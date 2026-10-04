# 需求与待办总清单（单一事实来源）

> 建立：2026-09-25。用途：把所有散落在 `PROGRESS.md`、`project-workflow/*/status.md`、
> `next.md`、`bugs.md` 里的需求与遗留收敛成一份**可执行**清单。历史过程不入本文件，
> 只入 `docs/history/`。
>
> 维护规则：**只有本文件是"待办"的唯一事实来源。** 完成一项就把结论与证据（ROM/SHA256、
> 测试结果、实机判据）搬到 `PROGRESS.md` 现场节，并在本文件删除该条。新增需求先入本文件
> 再开 `project-workflow/<initiative>/`。

## 0. 当前设备与产物快照（2026-09-29；未更新项保留原核查日期）

| 项 | 值 |
|---|---|
| 1.54" 设备 | MAC `70041DD7A340`，显示名"书桌屏"，IP `192.168.3.163`，Profile 同步启用、按键顺序 `quad,mini`（2026-09-28 运行 state 核对） |
| Note4 设备 | MAC `7C4FADB93408`，Mac登记IP `192.168.10.101`，`sync_enabled=true`；本地Profile空，设备已安装 `codex-status-a`（2026-10-03） |
| 两设备可达性 | **2026-09-25 15:2x 已按键唤醒、实测可达**（`/status.json`+ARP 双证，MAC 与登记一致）；此后注意深睡仍会不可达 |
| Note4 已装固件 | 2026-10-03标准OTA job `63be0d96`：`0.18.45-note4-b-wifi3`、target `zectrix-note4-400x300`，ota_1认证 `image_verified` |
| 1.54 已装固件 | 2026-09-29 COM4 USB 刷入 `0.18.34-bw-ota1`，串口自报新版本；运行分区前 1,631,488 字节回读与候选逐字节一致。Bridge BLE 数据/Plan 已 ACK；旧诊断 checkpoint 返回 409，见 A0 |
| Note4 ROM | `artifacts/bridge-macos/note4-0.18.45-wifi3.bin`，1,720,784 B，SHA256 `B4635DC064378427FACBB62C6A7780DE9649E819CD0DB834B0063B90F149468C`；运行前缀一致，.44回退保留 |
| 1.54 ROM | `artifacts/rollout-20260928/154g-0.18.34-ota1.bin`，1,631,488 B，SHA256 `DDEE1ECAEB94714165E639CCD98C0327AFA5B3A46328153028C013B666249E1D`；USB 写入与整段哈希读回校验通过。`0.18.25-bw` 回退 ROM 保留；故障 `0.18.32-bw-sync1` 禁止再次上机 |
| 桥（Windows既有现场） | `bridge/target/debug/bridge-app.exe`，SHA256 `BA066990D60CCF2E8AAA86BC23D8B1D888C66433249893109DD052012C60F163`；2026-09-30 计划任务 Running、8765/8766 监听，运行 data 保留；含 Profile 字体资产发布与设备页自动刷新 |
| 桥（macOS 2026-10-03） | `artifacts/bridge-macos/Codex Status Bridge.app`，75,192,688 B / SHA256 `B827FA658CCC1A2E3E86B44592A909550773251683A837C78E67155B7D80CF1A`；分tab UI已部署，原按需LaunchAgent、data/安装身份保留 |
| 1.54 队列/模板 | **已解决（2026-09-25）**：`bundle_jobs` 为空；job `2bfc710c` = `succeeded`，设备 `committed_job_id=2bfc710c`、`v2_templates=2`(`mini,quad`)、`active=quad` 并已渲染。残留：数据帧被拒 `incomplete` → **见 A7** |
| Note4 正常 | ExtraLight 200 Bundle `5d798bd9` 保留；.45最新自动HTTP ACK seq16 applied/displayed，源/最后写屏缓冲89一致。实体字形/局刷故障与USB常驻BLE问题不因本项结案 |

## 1. 产品需求 → 实现状态

| 需求 | 状态 | 缺口 |
|---|---|---|
| ESP32-S3 墨水屏显示 Codex 余量，换样式只换模板 JSON | 已交付（固件 0.18.x + 桥） | 无 |
| v2 通用平台：Profile 1–8、CompiledTemplate、完整 A/B Bundle、单一 active context、Bridge 生成 PowerPlan | 已交付并实机跑通 | Bundle v3 压缩（见 C1） |
| 显式 claim/lease 占用、MAC 为身份、显示名可改 | 已交付 | 无 |
| 端口鉴权 token（`/update`、`/doUpdate`、OTA、`POST /claim`） | 已交付 | 无 |
| Note4 400×300 模板 `codex-status-a` | v2 已保存并发布、实机数字布局正常；宿主 `compiled` 7/7 通过 | Wi-Fi 连接时亮/空白图标有设备局刷计数，尚待目视确认；ROM `0.18.38` 已部署休眠前 Bridge ON/OFF 延续逻辑，两种起点仍待实机目视确认。Bridge OFF 独立 glyph 已做同源预览 |
| Note4 时钟每分钟局刷（不做整屏全刷） | 部分交付 | v2 会合路径的 ghost 预算**已补齐**；Note4 深睡断电导致基线丢失仍会回退全刷，需实机验收 |
| 两台设备离线深睡时钟准度 | Note4 0.18.59-note4已部署/真实RTC读写及周期采样通过，1.54未部署（2026-10-04） | Note4 PCF8563、1.54 PCF85063A已接入；两驱动ASan/UBSan与生产main helper通过，.58修复Wire最小32B及Note4 GPIO42 I²C电源前提，匹配ELF门禁/OTA/独立运行hash通过，实机ready/valid及采样推进已验，.59自然深睡唤醒后仍有效（project-workflow/external-rtc/）。仍需两板离线多轮深睡/重启、24h漂移、供电保持、整机峰值RAM/功耗；1.54首次可信校时/真实读取及ROM构建部署仍未完成。内置RTC定时唤醒保留，日期时间从独立RTC恢复；RTC INT调度另需产品决定，不将精准显示等同精准RF会合。 |
| 深睡功耗与电池续航 | 部分交付 | panel-power 双模式已实现但未刷入；bench 电流/残影/重复唤醒**从未测量**（见 D1） |
| 1.54 设备时钟局刷与全刷交替 | 源码已修 | 未刷机、未实机验收（见 B4） |
| 字体引擎化（CSFN 容器 + Profile 内版本选择） | 完整 A/B Bundle 路径已实机安装 ExtraLight 200 | manifest 增量发布仍待定稿，设备侧无增量端点（见 C5） |
| Bridge 多设备界面 + 按 MAC 推送 | 已交付 | 单设备全局身份已换成按 MAC 运行记录；UI 与 MCP 的设备类操作要求显式 MAC，多设备缺 MAC 一律要求选择（2026-09-25 两台实机自检，见 `docs/history/progress-archive-2026-10-03.md` 相关历史节）；剩余见 C7 的 `bridge-multi-device-ui` 行 |
| Bridge 界面视觉与信息层级 | 第一版已重建并运行 | 四个 Tab 已统一视觉；设备/数据/MCP 内容已重排。待打开窗口肉眼验收，并按第一版反馈细化（见 `project-workflow/bridge-ui-redesign/`） |
| Bridge 休眠设备的状态与延后操作 | S1–S4 已实现；S5 实机主路径通过、故障矩阵待验 | 两台 ROM 已按登记 MAC 顺序自然会合 OTA，各 1 次上传 ACK 且后续认证见新版本；两台完整 Bundle 发布 `succeeded`，1.54 的 waiting 发布跨 Bridge 重启后自然提交。实机发现并修正 OTA 待精确确认时阻塞后续发布、显式 sleep ACK 未记账；Bridge 已重建。剩余：错 IP/401/409 端到端注入、长期 PowerPlan 截止期、Note4 间歇 `display_state=failed` 诊断。当前 ROM 不提供可对照整文件哈希的运行镜像身份，OTA 保留 `awaiting_confirmation` / `version_seen_unproven`，精确 `image_verified` 待后续能力。证据见 `project-workflow/sleep-aware-bridge/`、`docs/history/progress-archive-2026-10-03.md` 相关历史节。 |
| Bridge 多实例 | 已交付（tag `bridge-multi-instance-2026-09-24`） | 无 |
| Fake ROM 设备模拟器（同源 C++ + 可控实验时钟） | D/E/F 软件验收完成；G 按用户决定取消 | 软件证据见 `docs/history/progress-archive-2026-10-03.md` 相关历史节；硬件特有错误仅留 case 文档，不在本项执行 |
| 设备页、周期 Wi-Fi 与统一诊断流 | Note4 与新版 Bridge 已部署；1.54 的新版 ROM 因 panic 已回退 | 先完成 A0，再做设备页真实窗口视觉、完整 Fake ROM 与实机故障矩阵；见 C8 |
| 唤醒会合诊断（一次唤醒一条记录） | 仅计划 | 固件与桥两侧都未实现（见 C3） |
| Bundle v3（manifest + 原始 dense 对象，省 ~71%） | 仅设计 | 未实现（见 C1） |
| `device_first` 低功耗会合 | 已运行；`bridge_first` 因时序问题已废弃（2026-10-03 用户确认） | 现行策略的长期稳定性与整周期电量验收见 C4；不再安排双策略切换 |
| 功耗基线 A/B（DFS 40/80、BT modem sleep、会合节奏） | Note4 BLE 三臂 PM 驻留已实测 | A/B/C 分别 34/32/34 周期；板级电流、整周期能量及长期稳定性仍待测（见 C4） |
| Bridge 独立于 Codex 生命周期 | 已改为按需 Windows 计划任务启动，待跨会话验收 | 退出 Codex 后确认 `CodexStatusBridge` 仍为 Running、Bridge 8765/8766 仍监听；本会话进程父级已核实为任务计划程序的 `svchost.exe`，见 `docs/history/progress-archive-2026-10-03.md` 2026-09-27 节 |

## 2. 紧急（A 级：先做这些）

### A0-dump. Note4 dump 分区恢复 — 用户最高优先级（2026-10-04）

- 用户指定顺序：本项先于状态同步阻塞调查，其后完成原交汇设计；分区工作指定单独agent接手。计划/交接见 [note4-coredump-plan.md](../../project-workflow/bridge-macos/note4-coredump-plan.md)。
- .45已再次自报panic；USB启动日志和完整flash读回确认旧表无coredump。私有全flash+匹配ELF已备份，实际活动镜像核对。CSV已规划1MiB dump在0xB00000，缩短ota_1尾部并保留NVS/otadata/文件系统和app起始；生成/离线迁移校验通过。
- **2026-10-04 01:54 已写设备并完成写后验收**：USB（MAC 7C:4F:AD:B9:34:08）`write-flash 0x8000` + `erase-region 0xB00000 0x100000` 成功；读回 0x8000 逐字节等于候选表（3072B SHA256 `6A11BC01…0A39`），dump 区 1MiB 非 0xFF 字节为 0；与私有备份比对——bootloader/otadata/phy/ota_0/ota_1/storage 一致、活动 .45 仍 `B4635DC0…468C`，仅 NVS 有 658B 运行期键写差异。启动日志确认 `v0.18.45-note4-b-wifi3`/`ota_1`/bundle `5d798bd9`/ctx `dea4fb9663b879f5`/commit 421 保持，HTTP `/status.json` 200。证据 [note4-repartition-verify](../../artifacts/bridge-macos/note4-repartition-verify/)。
- **现场工具诱发复位说明（2026-10-04早期记录，20:38已细化）**：esptool进入/退出ROM的复位与普通串口监听分开；早期写表/擦除/read-flash/hard-reset及普通pyserial抓取/轮询流程曾使设备重启，不能泛化为所有port open/read必复位。"设备一直重启"经 Bridge 连续 `/api/status` 200（02:11:00–02:12:55，83–250ms）与 `uptime_s` 无中断证伪为崩溃；无 panic/`reset_reason=4`。非必要不再开串口轮询。
- **2026-10-04 20:38真实转储验收已完成，新增BLE panic定位**：[调查与串口对照](../../project-workflow/bridge-macos/note4-panic-20261004.md)。Note4 .59再次panic（用户未操作USB/按键）；只读实际表/1MiB dump/运行槽镜像，23972B有效转储CRC通过、镜像与匹配ELF哈希通过、官方解码完成。nimble_host InstFetchProhibited/PC0，寄存器事件地址精确匹配ble_hs_timer.ev；loopTask同时在会合关BLE/NimBLE deinit/host stop中。已定位直接空timer回调，源码的queued事件与callout memset竞态高度吻合；取证结束时仍须生产调度定向复现与修复（本轮修复见下项）；不因转储可读结案。原始凭据敏感证据私有保存，设备dump未擦除。
- **2026-10-04 BLE timer竞态已修复并部署.60**：真实NimBLE生产函数queued/dequeued交错旧版稳定PC0，STOPPING只stop、最终deinit负责销毁的修复通过ASan/UBSan2000轮；构建补丁严格校验/幂等。Note4唯一目标.60 ROM已OTA、独立认证运行hash与最新release确认（详见PROGRESS最新节）。USB被动串口下十次实际诊断init/scan/deinit同boot无panic；120s恢复窗实际连接/断开、到期controller released与正常深睡/自然timer reset_reason8已验。完整USB/电池连续业务会合、长期RF与功耗仍开放，不以短样本结案；1.54未构建部署。
- **串口复位规则修正**：同机实测raw无modem ioctl与跳过DTR/RTS更新的pyserial不复位；普通false/false open及单独DTR=false触发USB_UART_CHIP_RESET，随后RTS=false不再触发（只证明本次初态）。esptool主动ROM进入/退出复位分别记账，AGENTS.md已更新保现场recipe；不能将串口读取概括为必复位或以此跳过用户授权诊断。
- **仍待实施**：保首份dump策略（.60仍未启用`FLASH_NO_OVERWRITE`）、修复后的长期实机USB/电池/RF验收。BLE timer生命周期修复/回归/Note4部署已完成，真实panic+CRC+匹配ELF已验；普通应用OTA仍不更新分区表。


### A0. 恢复书桌屏旧诊断 checkpoint 的同步确认

- 双设备栈修复 ROM 已上机；Note4 的旧 3725 B 诊断批次在新 ROM 上完成，证明分页/ACK/complete 主路径可运行，详情见 `docs/history/progress-archive-2026-10-03.md` 相关历史节。书桌屏 USB 恢复后，Bridge 本地 `diagnostics/70041DD7A340/checkpoint.json` 仍在等待旧批次 `1-cc96959b5a75e68e9559837e5eada4fd` 的设备 ACK，但设备已清空旧批次；重试 `sync_complete` 返回 409 `batch_conflict`。旧归档 JSON 已保存，不能把设备未确认伪写成已确认。
- 设计并验证安全的 checkpoint 退役/重建路径：保留旧归档和失败原因，严格核对 MAC、归档 SHA、设备当前/最近批次及 client serial，再使书桌屏开始新批次；恢复后验证 `sync_begin/page/ack/complete`、后续会合和设备页。Note4 .45于2026-10-04再次自报panic；转储分区恢复已提升为上方A0-dump，历史短时未复现不代表长期稳定。历史故障 ROM `0.18.32-bw-sync1` 不得重传。

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
- **结论（2026-09-25 实机复核，详见 `docs/history/progress-archive-2026-10-03.md` §A1/A2）**：
  - 队列已清干净：`platform_publish_cancel {"mac":"70041DD7A340"}` 实测对 `sending` 有效（工具描述 "queued (unstarted)" 不准确），`bundle_jobs` 已为空；`jobs[]` 里的 11 条 `waiting` 是**孤儿历史**（`refresh_bundle_history` 只在终态转换时改写，`service.rs:318-326`），不是活状态。
  - 但**数据仍不通**：一次干净重发（`e60830a4`，55,312 B）在 BEGIN + 14 个 CHUNK 全 200 后，被设备在 **COMMIT** 拒绝，ACK 为 `{"result":"rejected","error":"oom"}`。flash 余量检查（`bundle_store.cpp:433-436`，不足会报 `"space"`）**没有**触发 → **不是"8 MB flash 装不下"**，而是 commit 期逐模板重建编译缓存时的 `malloc` 失败（`bundle_store.cpp:467-468`；1.54 空闲堆 ≈100 KB）。
  - 原始卡死同型：09-24 04:48–05:19 对 `83f4324c` 也是"chunks 全 200 → COMMIT 被拒"，之后作业再无状态变化。**桥侧把"非 applied 的 COMMIT ACK"当成"保持 sending"**（`coordinator.rs:624-636` 只在 `committed==true` 改状态；`service.rs:922` 只在 committed 时刷新历史）→ 该设备被永久顶住。这是独立缺陷，建议先修（落 `Failed`+`last_error` 或加重试上限/退避）。
  - 设备另有第二个故障：`[v2] active load failed: ct_abi` → `[v2] cannot rotate unknown-retention context; data disabled`，即已装 Bundle 的编译缓存 ABI 与固件不符，固件自关 data（`/v2/data` 返回 `rejected`）。建议核实 `bsLoadCompiled` 的"用槽内 source 重建缓存"兜底为何不生效（`bundle_store.cpp:568-591`）。
  - **第二次尝试（用户授权，1.54 冷启动后 19 s 内发，堆最新鲜）给出了第二种拒绝**：新作业 `37217f05`，BEGIN + 14 分片全 200，COMMIT 返回 **`error:"owner"`**（不是 oom）。该错误全仓只有一处产出：`v2_bundle_command.cpp:144-158` 比较「COMMIT 请求体的 `bridge_id`」与「flash 暂存载荷里的 `bridge_id`」，且它位于 CRC(`:138`)与 session 匹配(`:110`)之后 ⇒ 请求体解析正常，是**设备侧重读 55 KB 暂存载荷时没拿到 `bridge_id`**（退路 `server.arg("bridge_id")` 桥从不发）。桥侧同值：`deliver()` 先 `bind_pending_bundle_owner`(`platform.rs:685`→`service.rs:844-871`) 强制 `payload.bridge_id == link.bridge_id`，不一致就不发包。
  - **瓶颈判定为设备固件**：1.54 跑 `0.17.10-bw`，早于 `ca4022c`（0.18.23「stream bundle install from file」＝`oom` 那段的重写）；`owner` 检查本身（`b702aa3`，09-24）在 0.18.23 中未改。另注意**当前桥 exe（04:43 构建）成功提交数 = 0**（Note4 最后一次成功 `1b4500ad` 是 09-24 23:20，早于该构建）。
  - 设备另有第二个故障：`[v2] active load failed: ct_abi` → `[v2] cannot rotate unknown-retention context; data disabled`，即已装 Bundle 的编译缓存 ABI 与固件不符，固件自关 data（`/v2/data` 返回 `rejected`）。建议核实 `bsLoadCompiled` 的"用槽内 source 重建缓存"兜底为何不生效（`bundle_store.cpp:568-591`）。
- **✅ 已解决（2026-09-25 18:1x，详见 `docs/history/progress-archive-2026-10-03.md` 相关历史节）**：把 1.54 OTA 到 `0.18.23-bw`（`firmware_ota`，54.1 s，ROM `91937B18…`）后，**一次 `platform_publish` 即 applied**：job `2bfc710c` = `succeeded`，设备 `committed_job_id=2bfc710c`、`v2_templates=2`(`mini,quad`)、`active=quad` 并渲染（`[clk] reserved …`），`[bundle] install total=78150 free=1810432`。**`oom`、`owner`、`ct_abi`/`data disabled` 三者同时消失** → 证实瓶颈是 0.17.10-bw 的 commit 路径（0.18.23 的流式安装 + 一次成功安装写入的新编译缓存/新 context）。
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

### A8. 设备合同 remaining 延迟推送及重启上下文确认（2026-10-03，已修复/部署验证）

- 原无本地Profile/default Codex源误将remaining分pull，93→92只等3600s；共享默认分类现沿resolve_field归一化，显式源覆盖保留。另补同合同新context不得沿用旧ACK指纹，自动重排快照、序号递增、旧ACK隔离。
- 生产service14组合红→绿（HTTP/BLE、ACK后变化、重启、push/pull覆盖、pull-only不改Plan及新context），最终相关crate/Fake/UI检查通过。新版Bridge与Note4 .45按用户授权一并部署，ROM job63be0d96=image_verified；产物/私有备份/准确测试边界见PROGRESS顶节。
- 真实源剩余90→89，22:36:11自动HTTP ACK seq16 applied/displayed；认证data/applied均16，确认push指纹一致，最后写屏缓冲89，无push_now/源伪造/模板发布。原92/93差异在部署前小时同步已消失，故不再以旧数值作当下验收。
- 本项闭环；USB常驻BLE绘制、重连/局刷和新Wi-Fi真实配置矩阵仍见B5/B6/A3，不把A8通过当全部USB/硬件问题已解决。[证据与诊断](../../project-workflow/bridge-macos/usb-sync-diagnosis.md)。

### A3. 修掉 Note4 深睡时钟残影（唯一影响日常观感的显示缺陷）
- **2026-09-30 进展**：ExtraLight 200 Bundle job `5d798bd9` 已成功；`0.18.40-note4-b-display1` OTA job `10ec4b37` 已认证 `image_verified`，设备新回报 `display_state=displayed`、`epd_busy_fails=0`。源码以成功写入计数优先于局刷失败计数，宿主覆盖“局刷失败后整刷成功”及真正未写入失败。仍需将局刷失败原因分型（影子帧、分配、BUSY/阶段）并确认有无硬件侧间歇 BUSY 故障；旧 `failed` 不能当作模板提交失败。
- **现象**：deep 期间只写时钟窗口，残影累积；`SYNC`/用量属整帧元素所以停在最后一次整帧（现场曾被读成 `04:64`）。
- **根因（已用代码核实，2026-09-25）**：**v2 设备上时钟窗口的 ghost 预算根本没有被评估**。预算判定只在 `deepNetworkCycle()`（`main.cpp:5166` 渲染门 / `5177` 强制清影），而它唯一的调用点在 `main.cpp:5697`，进入条件是 `!(deepWakePath && v2BundleReady)`（`main.cpp:5678`）。Note4 有已提交 v2 Bundle → 走 `5678-5693` → `deepNetworkCycle()` 从不被调用 → `rtcClkPartials >= CLK_GHOST_LIMIT` **永不成立**，计数器只增不减。v2 会合真正走的 `v2RendezvousClockRender()`（`main.cpp:4617`）只检查 `clkR.valid`/`activeTplHasNow`/`timeKnown()`/`clkPixelsValid`，**无预算检查**。
- ⚠️ **曾经写错、此处纠正**：上一版这里写过「已过时，rtcClkPartials 现已覆盖两条路径」——那是错的，归档 `docs/history/progress-archive-2026-10-03.md`「当前现场与待办（2026-09-25 核查）」保留了正确结论。落笔前请自己复核 `main.cpp:5678` 的分支条件。
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
- **2026-09-25：构建侧与刷机侧都已完成** —— 154g 隔离目录 + 当前源码 ROM 就绪（`91937B18…`，`0.18.23-bw`，含 `ca4022c` 与时钟窗口修复），并已 **OTA 进设备**（`0.17.10-bw → 0.18.23-bw`，54.1 s）；同一次会话里 Bundle 也装成功（A2 关闭）。见 `docs/history/progress-archive-2026-10-03.md` 相关历史节。
- 待做：按 `/history` 的每分钟唤醒类型 + `clk_partials` + 可见刷新类型三项对照（**实机验收**）；现在设备跑的是 `0.18.23-bw`，验收条件已具备。

### B5. Bridge macOS 菜单栏支持（2026-10-03 新开）

- **2026-10-04 权限引导已部署**：缺权限时打开界面一次性转至引导，未决可请求/拒绝可开系统设置，定位请求等待实际前台激活、BLE后台不反复创建未授权central；App64通过/1既有需模拟器ignored，JS与WKWebView16场景通过。生产回读两权限尚未授权；待用户系统确认后真实SSID/撤权恢复及同镜像重启验证。用户明确暂不更改钥匙串，持久签名未实施，跨重建授权保持不能据此结项。证据/状态见[permissions-plan.md](../../project-workflow/bridge-macos/permissions-plan.md)。

- A8与W7已修复/部署，成功数据证据不能替代USB常驻验收：实固件BLE data置pending仅由timer会合消费，普通USB loop的消费缺口/Fake立即绘制差异仍待独立复现与修复；完整USB/自然会合/重连矩阵保持开放。

- 新窗口接续入口：`project-workflow/bridge-macos/remaining-plan.md`，汇总已完成基线、当前产物/数据、R0–R7 剩余实施顺序与验收；本项继续只维护未完成事实，不将已部署基础适配重列为待实现。
- 2026-10-03 后续用户已授权开始执行；当前进入 P0/T0。本机 Rust 使用默认路径升级，不使用隔离工具链；仅构建产物隔离，保护原运行 data。当前主机网络与设备目标已核对，凭据不记入文档。
- 用户要求首版达到 Windows **已运行**的 BLE 与低功耗会合能力：菜单栏、现有管理 UI/Codex/Wi-Fi/MCP、绑定加密链路取 token、`device_first` 定时会合、正式 PowerPlan 与延后作业；`bridge_first` 已因时序问题废弃，不纳入本项或后续移植阶段（2026-10-03 用户确认）。
- P0 已开始：本机 Rust/Cargo 升级，最小 Intel/macOS 14 `.app` 已构建启动，MCP/HTTP 端口可达；正式应用已完成 Note4 GATT 精确 MAC、绑定与加密读取，并在设备配网重启后再验证同一绑定。编译、路径、基础单实例及扫描去重已适配；首次未绑定接入、自然 timer 完整 ACK、生命周期、Windows 对照及长时验收仍未完成。方案与分阶段任务见 `project-workflow/bridge-macos/plan.md` / `task.md`，不标记首版完成。
- R5 ARP 与 R6 按需 LaunchAgent/限速监督已实现并本机运行（证据见 PROGRESS 顶节）；当前缺口为真实发现回退/正常退出/异常恢复、主机睡眠与网络/BLE 状态恢复；首次未绑定接入及拒权/撤权恢复的完整产品流程、自然 timer 短窗口 ACK 与 Windows 对照/长时验收尚未完成。Windows 条件依赖、peripheral ID 去重、数据/seed/Codex 路径、稳定安装身份与 Unix 单实例锁已适配，不再列为未实现。
- 2026-10-03 用户补充新环境无法连接旧 Wi-Fi：首版接入增加 B6 的离线 BLE 配网与不依赖 Wi-Fi 的物理按键开蓝牙前置。该功能需要固件与跨平台 Bridge 一同设计，不能靠 macOS 托盘移植补齐；规划见同目录 `wifi-setup-plan.md`。
- 2026-10-03 用户确认本环境仅携带 Note4，1.54 不在现场：当前实机验收以 Note4 为范围，保留双设备软件身份回归；Windows 对照、Note4 每平台 ≥30 个自然会合及 macOS ≥24 h soak 按后续可用现场验证。1.54 实机与另一 CPU 架构另标未验，不为本轮要求用户取回设备，也不据 Note4 结果宣称全硬件已验。当前会话为 macOS 14.8.9 / x86_64，macOS 15+ 权限现场尚未验证。

### B6. 离线 BLE 配 Wi-Fi 与 BOOT 蓝牙恢复入口（2026-10-03 新开）

- **2026-10-04变长存储/AP与USB收归已提交并部署，真实WFS3迁移已验**：[设计与资源验收](../../project-workflow/bridge-macos/wifi-variable-storage.md)。WFS3显式头/字段记录长度/CRC、实际条目与字符串按需RAM、单一仓库及堆候选，活动同版引用/旧凭据最小快照；旧WFZ2/raw-v2/v1验证迁移后退役scheme，保留读取恢复代码。AP/USB不再写sN/pN，认证JSONapply/result共用permit/owner/修订/回执，不伪造bond或保存即激活。生产C++三套内存检查、core12/app8/device-sim5及旧恢复通过；唯一Note4候选.55、栈门禁与资源预算已具备，空/典型两条两回执/满容量格式33/387/5036B，存储峰值组件预算最高22436B。用户明确授权后源码f8426cb已提交，OTA bfd4e115 succeeded/image_verified；Note4运行.55/ota_0，独立镜像hash匹配3A552561…642FF。真实迁移WFS3 ready/249B/2条/0回执/旧scheme退役，后续深睡启动仍加载；Bridge健康与缓存不变。AP/USB真实写入与Flash掉电、RF/功耗/跨平台/兼容代码最终移除独立留项，不结B6。

- **2026-10-04页面手动同步校验已实现/部署**：仅比较页Bridge→设备同步允许删除原方案，但缓存合并结果须保留当前环境host_ssid→设备2.4GHz映射，否则阻止预览/提交。预览冻结主机网络、本地版本、快照与所选项，服务原子复核；不自动扩大范围/切网/试连。自动配网、程序化MCP同步、本地保存和缓存导入不受此守卫影响。最终范围隔离回归通过；详见[实施及验收记录](../../project-workflow/bridge-macos/wifi-rendezvous-implementation.md)。

- **最新协议审核已实施/配套部署**：首次交汇认证HTTP失败才BLE最小诊断；Bridge缺配置先提示补全，设备缺项/不同保存启用、已有未活动只启用，同活动版本认证/安全失败停止重复操作并提示修正，其他错误有限退避；已入网但HTTP不可达、认证/身份失败独立。普通空SSID不漏认证错误，正常成功无额外报告，实际HTTP恢复清旧提示。日常绑定认证写入复用已有合法窗口，无额外BOOT；通用最多4条BLE原子同步共享HTTP事务/持久回执，ACK丢失只查原事务，冻结缓存不重复读取完整配置。共享30s绝对交接期限，DHCP/候选持久启用完成、同session/wake/plan/owner HTTP交接后关BLE再light；未知Plan不发新写。保留Astra审核的持久ACK/高水位、准入读集合及先释放锁再2s有界close。Bridge和Note4 .54已部署且认证运行镜像匹配，core118/app65+最终范围1/device-sim5/C++/JS/WK32通过。

- **仍未验/未实现边界**：系统权限尚未授权，真实SSID/无配置/错误密码/旧网络/同网HTTP不可达/AP/DHCP/连续睡醒RF矩阵及首次新版密码读取证据未验；未为验证重复读配置或改写实际网络。变长存储/按需RAM与AP/USB入口收归已完成软件候选，真实部署迁移/入口写入和兼容代码最终退役、长期RF/功耗/掉电/跨平台矩阵继续开放，B6不结项。运行路径、最终hash与备份见PROGRESS顶节；下方为此前里程碑记录。

- **2026-10-04删除/重命名回归已修复部署**：生产HTML实际调用red→green；编辑保留原身份/版本/密码/删除标记，固定ID、显示名/SSID更新原条目；删除支持撤销，折叠时保存仍可用。JS/WK28/core整条提交确认及重启回归通过；只更新Bridge，.53沿用，不以合成IPC宣称实际删网。证据和部署见PROGRESS顶节。

- **2026-10-04 最新密码修订已实现/配套部署**：Bridge可保存/编辑密码，眼睛按钮显示/隐藏；首次认证完整读取含本机绑定密码，缓存导入/复制/重启保留，作业可直接复用，无独立补读密码/钥匙串步骤。旧缓存缺密码仍未知，不自动重新读取；其他绑定/legacy不越权回传。密码单独不同标冲突、wifi_match可核对缓存凭据。core116/app64（1既有ignored）/device-sim5/C++两入口/JS/WK24通过，Bridge及Note4 .53运行镜像核验通过。实际新版首次读取含密码待用户显式操作验证，权限/RF/资源矩阵仍开放，B6不结项；详见PROGRESS顶节与wifi-redesign.md。

- **2026-10-04用户最新交互已实现/部署**：当前真实主机SSID查询Bridge targets，存在则在合法会合通过BLE单条核对/启用，不存在预填并提示配置；设备按网络ID/版本反馈真实连接和有限错误，有界尝试/冷却，认证失败要求修正。比较按整条方案覆盖，执行历史折叠，缓存设备→Bridge不新读；无需多步恢复向导。Bridge及Note4 .52配套部署、认证镜像/健康/缓存核验通过；Rust/生产C++/JS/WK24场景通过。两权限仍not_determined，真实SSID/错误配置/离线BLE配网闭环未验，B6不结项；详见[wifi-redesign.md](../../project-workflow/bridge-macos/wifi-redesign.md)与PROGRESS顶节。
- **2026-10-04最新现场**：两条旧Wi-Fi槽迁移成功，设备回ok/persisted/retired=true、revision1、active=wifi-slot-1。迁移后的.50启动栈溢出由USB/dump复现定位；.51原地构造修复已USB部署、完整读回与认证镜像核验，两次启动无panic；完整读5a55a29e9a41542a一次completed，两个正常槽/无legacy。实际HTTP交接后关BLE并light通过，长期矩阵仍开放。ROM/证据以PROGRESS顶节为准。
- **资源设计后续（用户明确约束）**：按AGENTS.md新增指导，将Wi-Fi仓库改为带版本/有效长度/数量/校验头的变长格式，只存实际条目/字段/所需回执，RAM按需分配；压缩不作为空槽与固定结构开销的解决方案。当前.52沿用.51栈修复，仍固定结构+压缩存储；变长实现、空/单条/典型/满容量字节及峰值RAM/栈预算、损坏/截断/超限/掉电/旧格式迁移门禁待做。AP/USB旧格式写入收归及后续ROM删除兼容代码仍开放；Windows系统zlib链接仍未验。
- **Bridge设备→本机**：缓存方案选择已实现/部署，保存选中整条快照方案不联系设备；snapshot_id和local_revision事务内冲突检查。app离线持锁入口/core8/JS/原生12场景通过。
- **会合协议模块源码与配套部署均完成**：Astra A1–A8及追加复审已落实，Rust310/C++门禁通过；不是源码候选未部署。剩余为连续RF/断链/丢ACK/慢Wi-Fi/电池USB/功耗/panic矩阵及close回归A/B因果，详见[计划](../../project-workflow/bridge-macos/rendezvous-protocol-plan.md)与[逐项审核](../../project-workflow/bridge-macos/rendezvous-protocol-review.md)。下列审计失败记录属历史输入，不代表本次修复尚未编码。

- **原W4/W6未闭环（2026-10-04对账更正，非新增需求）**：原执行文档§5/W4/A02及§8要求实际app交接组合，先前局部通过不能代替；[归属与验收更正](../../project-workflow/bridge-macos/wifi-rendezvous-execution.md#14-原任务归属与验收更正2026-10-04)。BOOT只作现场调查/原维护授权入口，不当日常交接修复。会合审查未修复：[报告](../../project-workflow/bridge-macos/rendezvous-audit.md)。2个行为复现测试确认读任务不产生正式light窗口、诊断complete关临时窗口后方案快照不可达；Bridge先诊断后Wi-Fi作业，实板200ms宽限构成竞态。需统一合法窗口的任务准入/完成/关闭，接受ACK与HTTPready/剩余预算分离；按现行被动策略准确显示等待Wi-Fi/BOOT，不偷用periodic或读/传输续租。补生产app协调组合测试及Note4电池/USB连续多轮；现有service/Fake事务测试绕过app顺序，不能作为此项完成证据。截图那次HTTP不可达根因仍未由设备端日志确定。
- **2026-10-04 收口审计与 BLE 阻塞 red**：只读审计清单见 [w4-w6-closure-plan.md](../../project-workflow/bridge-macos/w4-w6-closure-plan.md)（证据 artifacts/bridge-macos/w4-w6/）。确认根本缺口是"不存在本窗口已准入任务集合"——三套待办（plan/coordinator、`WifiState.jobs` service.rs:471、设备侧 `sync.due` main.cpp:4042）互不可见；排队 Wi-Fi 读不入 `work_want_light`（service.rs:2366-2380 区间无 wifi）；诊断 complete 关窗（main.cpp:4119-4120、5784-5787）不知 Bridge 侧读作业。W6 现有 48 Fake + 51 app 测试**无一条**经过 `ble_cycle→sync_http_inner→opportunity_locked` 顺序。**② 同步阻塞已取得生产路径 red**：`platform.rs:2222` 无界 `link.close()` 在全局锁(2042)+每MAC锁(2055)内执行，10s 期限(2201-2204)只包 `work`；seam 注入 disconnect 永不完成后 `deliver()` 与下一轮 `ble_cycle()` 双双阻塞（501ms 预算耗尽、两锁 held），对照 0.06s 通过；证据 [ble-close-lock](../../artifacts/bridge-macos/ble-close-lock/README.md)，生产代码零行为改动。**修复已实现并通过恢复验证**（新增 `close_bounded(2s)` + `ble_cycle` 先放两把锁再有界 close、随后为 `sync_http_inner` 重取每 MAC 锁）：同一注入下该用例从 FAILED(509ms, 两锁 held) 转为 ok(2ms, 锁已释放)，`bridge-app` 53 passed/1 ignored、`bridge-ble` 17 passed（Lead 独立复跑），见 [RECOVERY.md](../../artifacts/bridge-macos/ble-close-lock/RECOVERY.md)。**⚠️ 已部署（2026-10-04 02:34，用户授权）**：重建 `Codex Status Bridge.app` 并重启 LaunchAgent `dev.codexstatus.bridge.managed`；新 exe SHA256 `59D7BA54…B0C6`（旧 `B827FA65…CF1A`），PID 74755/74757，`/health`=204，日志见 Note4 会合与 `/api/status` 200；回滚包 `Codex Status Bridge-before-closefix.app` 保留，运行数据未动。**尚未取得线上真实 close 悬挂事件**（`disconnect_timeout` 计数 0），真实自然断链路径仍待长期观察。`wifi.rs:80/91/198` 同类无界 close 未改。读任务申请有界正式 light 已获授权并在实施（task-6）。


- 分tab UI已落地并部署：真实 WKWebView 12 场景、14 张截图与设备页JS回归通过；[计划与证据](../../project-workflow/bridge-macos/wifi-tabs-plan.md)。使用合成IPC，不替代真实设备改网。23:44真实读任务 BLE 可联系但 HTTP 状态不可达，仍 waiting_rendezvous；BOOT窗口可用于现场调查；日常任务交接仍须完成原W4/W6验收，根因未确定，B6不结项。


- 配网与异步会合下一步执行：[wifi-rendezvous-execution.md](../../project-workflow/bridge-macos/wifi-rendezvous-execution.md)。W0–W6 接续剩余合同、本地目标/持久作业、设备事务、Fake Wi-Fi/HAL、真实会合闭环和第6节已设计 Bridge 界面；主要软件验收采用 Fake ROM。总状态仍归 B6；W0–W5源码候选已实施，W6本机软件回归287 passed/1 ignored、host3入口与设备页JS通过。已含生产Wi-Fi/history HAL、完整投影/事务、实际机会持久恢复及双侧UI；逐A编号证据/边界见执行文档第11节。W7已按用户追加授权构建并部署Note4 .45与新版Mac Bridge，运行镜像哈希核对；仍未写实际网络/试连/换网，不以部署代替实体矩阵；设备账本8项永久保留/满后明拒限制待长期管理评估。

- BLE 模拟缺口专项：[fake-rom-ble-fidelity-plan.md](../../project-workflow/bridge-macos/fake-rom-ble-fidelity-plan.md) 第11节，仍归 B6。已实现 P0/P1 核心（共享生产编码、512 B 属性保护）及 P2 属性读写分离/真实 DeviceConnection ACK 关联和超限/丢 ACK 回归；278 passed / 1 ignored，Wi-Fi 宿主最终帧最大418 B。后续W0–W6已补P3持久读/同步的机会恢复、待确认查询/回读、期限/重启与字段冲突，及P2生产Wi-Fi/history HAL和跨bond/session/分页软件矩阵（fidelity第13节）；完整app压力与实体P4/P5不因此结案。用户追加刷机后，Note4 .44已整板构建/USB烧录、整段读回及认证运行镜像哈希核对成功，配套Bridge已升级并保留data；真实已绑定链路status/plan/sync_open ACK通过一次。后续连接/INFO读取仍超时，须补连续重连与自然会合关联诊断；完整RF/TCC/功耗与原试连异常未验，不标记B6完成。

- 最新需求修订：Wi-Fi 管理必须有 Bridge 持久目标方案、显式读取的设备完整快照、比较和双向同步；不要求进入页直接读设备。支持选中差异 Bridge→设备 / 设备→Bridge，读取/本地保存/同步/试连/启用分开，保留冲突/删除/密码不可比较边界。最新版 `project-workflow/bridge-macos/wifi-redesign.md` 与 `artifacts/bridge-macos/wifi-plan-sync.html` 已更新设计；生产UI/服务已按双侧语义实施并于W7部署，示意HTML仍不是设备证据，真实网络操作矩阵未验。

- 上一版需求记录（以本节最新双侧同步修订为准）：当前单个方案界面不满意，要求重设计完整 Wi-Fi 同步/管理。新执行设计见 `project-workflow/bridge-macos/wifi-redesign.md`：全设备无密码集合（含其他绑定只读与 legacy）、完整表格/分组布局、保存/临时试连/启用分离、冻结全量快照和修订冲突。当时新版尚为设计；后续源码实施见本节首行，不以.42/.44本绑定摘要冒充部署后的全量管理；新版已于W7部署，真实试连/换网仍待验。
- 实机新缺陷：试连后 Note4 屏幕出现启动时画面，BLE 查询曾 ACK 后写失败，HTTP 不可达；异常后的 boot_id/reset_reason 未读回，复位与根因尚未确认。需先建立实机反馈环并排除跳屏/复位，再验收试连结果/旧链路恢复。记录在 PROGRESS 顶节，不用软件回归结案。

- 剩余实施优先按 `project-workflow/bridge-macos/remaining-plan.md` R0–R4 推进；配网细节见 `wifi-setup-plan.md`，新窗口不以现有 USB 联网或字段合同修复替代 B6 验收。
- 2026-10-03 后续用户已授权开始执行并提供当前环境网络配置；此前已通过现有 USB `pair` 完成 Note4 与 Mac 的 BLE 绑定/加密及重连验证。按 bond 网络方案、BOOT 恢复与动态映射已实现候选并部署 Note4，仍需真实场景验收，不能把现有配对成功标记为配网功能完成。
- 用户提出设备换环境后旧 Wi-Fi 不可用，需通过 Bridge 蓝牙配置新 Wi-Fi，并在正常运行/断网时用物理按键打开蓝牙。已授权实施；现已实现按 bond 协议、BOOT 恢复、设备页与动态映射候选，运行 Note4 0.18.42 的镜像已验证；主机 SSID 权限当前未授权，尚未完成真实 BLE 密码试连/换网和离线矩阵。方案、状态矩阵与验收见 `project-workflow/bridge-macos/wifi-setup-plan.md`，与 B5 共用 macOS 首次绑定入口，协议和固件能力应同时服务 Windows。
- 用户确认网络模型：每个 BLE bond 对应设备上的独立 Wi-Fi 方案，内含“Bridge 当前 SSID → 设备 2.4GHz 网络”的映射，允许 Bridge 连 5GHz、设备连不同 SSID 的 2.4GHz。网络名称、映射与密码均持久化在设备；Bridge 读无密码摘要，根据主机换网动态下发网络 ID，不保存或反复传密码。macOS 主机 SSID读取权限、无映射/有线/读不到 SSID、换网竞态、设备深睡延后与 owner 协调须纳入合同和验收。
- 用户确认产品入口：蓝牙配 Wi-Fi 与“本机绑定的 Wi-Fi 方案”均在 Bridge 设备页，包含离线蓝牙添加/接入、映射增改删、当前主机网络/所选设备网络及试连/持久化结果。已登记设备离线仍可进入；已有绑定可重连，无需重新配对；方案摘要从设备读取，密码仅输入与加密传输，不回读或落 Bridge 持久存储。详见 `wifi-setup-plan.md` 的设备页入口与展示。
- 旧版源码基线：`handleBootClick()` 在 `!wifiUp` 时返回，插 PC USB 只重试 Wi-Fi；电池启动连接失败时 `startNormalMode()` 可在进入按键 loop 前重新睡眠；AP 分支提前返回也跳过 BOOT 处理。旧 `docs/power-state.md` §3/§4 明确要求 WIFI OFF 不开 BLE，这是现有设计限制与新需求冲突，尚未实机复现。
- 蓝牙 Wi-Fi 凭据命令、固定恢复窗口、bond 隔离、异步试连、单 blob 提交和分页摘要已实现并通过宿主验证；实际失败/断电/多 bond 场景待实测。旧版现有 endpoint 是 Bridge 的 host/port/token，不是 SSID/密码。需设计独立物理恢复会话、绑定/加密授权、分片/请求 ID/结果查询、候选网络试连与失败保留旧配置；不隐式 claim、发布或重建 active context，不借 BLE 连接/传输续正式 light deadline。
- 2026-10-03 当前只有 Note4，USB `/dev/cu.usbmodem14101` 起初读回 `0.18.40-note4-b-display1` 并与 BLE/HTTP MAC `7C4FADB93408` 匹配；通过旧 USB `wifi` 命令把新环境凭据写入空闲槽 1，HTTP `wifi_slots=2`，旧网络保留，当前 IP `192.168.10.101`。随后已 OTA 到字段合同修正版 `0.18.41-note4-b-fields1`，当时的运行镜像验证见 `docs/history/progress-archive-2026-10-03.md`；当前连接仍沿用 legacy 网络，尚未提交真实 bond 网络方案，不证明 BOOT 离线矩阵已验收。最新 Wi-Fi 候选产物与运行证明见 PROGRESS 顶节。1.54 不在现场，仍单列未验。

## 4. 战略/长线（C 级：需要独立 initiative，不在近期窗口）

### C1. Bundle v3（manifest + 原始 dense 对象）
- 设计已定稿（`docs/history/workflow/bundle-v3/design.md`）：`CSB3` 容器 + `ct-dense-v1` 对象，`mini+quad` 从 55.3 KB 降到约 16.2 KB（省 ~71%）。
- **为什么重要**：1.54 是 8 MB flash，55 KB Bundle 的十六进制编码（45 KB 十六进制字符）是它最容易 OOM/超时的环节，也是 A2 的根因候选。
- 需改：固件（新格式解析 + A/B 原子安装 + 拒绝路径）、桥编译器（canonical dense 编码器）、Python 测试桥哈希三方同步；设备能力加 `bundle_format: 3`；v3 ROM 把 pre-v3 Bundle 视为不存在。

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
- 用户已选 B 方案用于 Note4 与 1.54 B/W：BT modem sleep + main XTAL、BLE 会合期 240/80 MHz DFS 且不启用自动 light sleep；未连接广播最多 4s，已连接等指令最多 6s，成功 ACK 后沿用 200ms 提前收尾。两份干净 ROM 已顺序构建并逐台 OTA，新版本及后续真实 timer BLE ACK 均已观察；1.54 设备日志确认 `esp_bt_sleep_enable()` 返回 `ESP_OK`。仍待两台新版 4s/6s 长期会合率、1.54 BLE 专属频率驻留与整机电池端电量测量。B 的 Note4 3s/6s 旧样本不能当作新版 4s/6s 或 1.54 的成功率。
- 已做：Note4 BLE 等待 A/B/C 三臂各 ≥30 个 deep 周期，分别为 BMS 关/80 MHz、BMS 开/80 MHz、BMS 开/40 MHz；有效回答 33/34、32/32、30/34。C 的 4 次未答是 3 次 3s 未连接、1 次连接后 6s 无命令。随后同 C 参数仅延长截止到 5s 广播/9s 等命令，实机 **34/34 回答**、平均 BLE 窗口 2.277s（旧 C 2.858s）；两轮在广播后 4.2/4.3s 才建链，说明 5s 上限有实际覆盖，9s 上限收益尚未单独验证。各组约 34 轮且未交错测试，不能把成功率变化归因于窗口或推出净耗电下降；详见 `project-workflow/power-plan-c/task-4-modem-sleep.md`。Bridge 为 15s 实验周期临时缩短同 MAC 冷却到 8s，测后已恢复生产 55s。下一步是电流仪下的 30–60 分钟整周期基线、5s/9s 长时对照与连接回归；BMS 关/40 MHz 第四臂、A5（整分钟对齐导致的 ~2 分钟间隔）仍待测。A4（`device_first` vs `bridge_first`）随 `bridge_first` 废弃而取消（2026-10-03 用户确认）。
- 已列出 Note4 每分钟互斥阶段电量和 BLE 电流敏感度，见 `project-workflow/power-plan-c/energy-budget-per-minute.md`；当前表只给芯片/CPU 参考情景，整机面板、PSRAM、稳压及偶发 Wi-Fi/light 的电池端增量仍待仪器实测，不能把表中合计当续航承诺。
- A2 Note4 80/40 MHz 旧短测、早启用 DFS 的 80/10 MHz 临时 A/B 已做（2026-09-27/28，见 `docs/history/progress-archive-2026-10-03.md`）：旧 BLE 等待 141 个 240 MHz **运行点**快照不能代表整窗驻留。早启用 PM 后，80 臂等待 1.723s 中 1.337s 在 80 MHz 档；10 臂等待 2.043s 中 1.589s 仍在 80 MHz 档，**10 MHz 档增量为 0**。随后三臂独立实机测试证明 BMS 开启时 80/40 MHz 最低档分别有 24.9%/38.2% 驻留；C 5s/9s 复测为 35.0% 的 40 MHz 驻留、34/34 回答。**A2/A3 尚未具备生产发布依据**：需整周期电流与长时连接回归；当前 BLE 等待无需继续追 10/20 MHz 下限。
- **策略收束（2026-10-03 用户确认）：`bridge_first` 已因时序问题废弃。** 删除其生产协议/双策略切换、预定 Publisher、设备定时扫描、认证重对齐/epoch 及该策略的反向 HTTP 实现待办；不移植到 macOS。后续会合只沿用现行 `device_first`，聚焦短窗口 GATT 稳定性、恢复与真实功耗。此前 RF P0、A/B/C 与窗口模型仅为历史实验，证据留在 `project-workflow/power-plan-c/task-5-advert-rendezvous.md` §9 和 PROGRESS 旧节，不作为仍有效的实施计划。
- 前置：C3 的记录格式（否则没有可信的对照口径）。

### C8. 设备同步与诊断协议（device-sync-diagnostics）
- `sync-v1` 实施合同见 `project-workflow/device-sync-diagnostics/design.md`。当前默认 Bridge 与双设备现场见 `docs/history/progress-archive-2026-10-03.md` 相关历史节：Note4 `0.18.38-note4-b-bridge2` 的运行分区前缀 SHA256 已由认证接口与候选 ROM 精确匹配；1.54 `0.18.34-bw-ota1` 已做 USB 写入后逐字节回读，运行分区的独立认证哈希仍待验。双设备 BLE status/config/plan/open、OTA 上传与重启换槽等先前实机证据保留。2026-09-29 后台隔离双 Fake ROM + 候选 Bridge 虚拟 24h 达到各 97 个连续批次和第 15 轮；独立 light 入/离场、进程重启及同 seed 1h 回放烟测通过，详见 `docs/history/progress-archive-2026-10-03.md`。一次性 Wi-Fi 续传修正及设备页/MCP 对等版已部署默认实例。Fake ROM S01–S14、设备页 U01–U20 完整矩阵、F3 延迟页/同磁盘快照回放、页中途强退、真实窗口视觉及其他实机故障矩阵仍未完成；Flash 断电恢复和 1.54 运行分区哈希证据仍须过门槛。
- 当前设备协议作为唯一业务协议的一次性统一见 `project-workflow/device-sync-diagnostics/protocol-unification.md`：业务 HTTP 已收敛到 `/api/*`，旧 `protocol=2`、BLE `rv=2`/`ack="v2"` 与 MCP `_v2` 入口已移除。两台设备现已升级，临时旧 Bridge 已停、新版默认实例已恢复。1.54 的增量缓存复核已完成，结论与四次构建日志见 `docs/history/progress-archive-2026-10-03.md` 相关历史节；不并行运行两个 Bridge，也不保留长期双协议兼容。Bridge 本地 `state.json` 无损保留，不凭 wire 更名删除运行数据。
- 实现每 15 次深睡 BLE 会合的一次性 Wi-Fi 同步，以及正式 light 入口和退出时的 Wi-Fi 同步；成功同步才重置计数。普通 BLE 保持精简，诊断日志按冻结批次经 Wi-Fi 增量同步，未确认内容保留。
- 合并普通 `/log` 与跨深睡 `/history` 为统一、跨深睡、可分页/确认/报告 gap 的诊断流；OTA/Bundle 执行后以 Wi-Fi 报告分级结果并有限重试。设备页移除 Codex 余量和屏幕内容、模板/Profile 编排、字体管理编辑界面；保留只读实际 active/已安装/任务及设备级数据投递许可。模板 Tab 暂不增加对应界面或设计迁移布局，数据页排版不动。详情时间与缺失原因已收束，完整 U 矩阵仍待验。
- 设备页内容与信息层级见 `project-workflow/device-sync-diagnostics/device-page-content-contract.md`，MCP 读取对等要求见同目录 `device-page-mcp-contract.md`。共享投影、默认主视图和七个按 MAC 折叠栏目已部署；版本结论依各设备缓存与显式发布目录判断。时间显示按同目录 `device-page-time-plan.md` 收束并完成两 MAC、七栏目运行 MCP 核验：完整 Wi-Fi 回报共用一个时间，较新的 BLE 联系单列，PM 整块共用一次成功采样时间，任务和占用使用事件/观察时间；UI 与 MCP 同源。剩余：真实 Plan 发送时刻尚未单独持久化（现显示生成与 ACK）、PM 样本跨 boot 身份、U01–U20 完整矩阵、最终 Tauri 窗口视觉及折叠诊断产品验收；详情见 `docs/history/progress-archive-2026-10-03.md` 相关历史节。
- 以 Fake ROM 验证绝大多数协议和故障场景（S01–S14）；实机只验证 RF/GATT、ESP32 Wi-Fi 入网与 HTTP、light/deep 切换、RTC/Flash/bootloader 等模拟器不能证明的部分（H01–H03）。详见 `project-workflow/device-sync-diagnostics/plan.md`。

### C5. 字体资产增量发布
- 单一字体注册表 + CSFN 容器已交付；Profile 版本选择经完整 A/B Bundle 发布与重启恢复，Note4 已安装 18 px ExtraLight 200（job `5d798bd9`、commit 355）。Bridge 模板页预览使用 Profile 资产；实体字形待目视核对。
- 未定稿：`docs/history/workflow/note4-bridge-publish/protocol.md`（manifest/CSFN 增量发布合同）仍是**待双方确认草案**；设备侧无增量端点、仍是 ABI 1 时代的 48 KiB/8 字体实现。
- 结论：在 C1（Bundle v3）定案前不要启动，否则两套编码器要一起改。

### C6. 多 env / packages 目录隔离收尾
- `tools/pio-target.ps1` 已落地，**两侧都已实测**（2026-09-25）：`.pio-pkgs/note4` 与 `.pio-pkgs/154g` 各 2 real（两个冲突 framework 包，69 MB + 2057 MB）+ 15 junction，各 ≈2126 MB。
- **154g 构建成功**：`pio run -d <repo> -e esp32-s3-epaper-154g` → SUCCESS 102.38 s，无 banner、无重装；产物 `1,740,576 B / 91937B18…`（`0.18.23-bw`），另见 `docs/history/progress-archive-2026-10-03.md`「多 env 实测」节。
- **判据 c/d/e 已实测（2026-09-25，两个阶段两种结论）**：
  - 只做包目录隔离**不足以**免重装：**交替构建仍会触发 framework 重装**（`sdkconfig.defaults` 是仓库根唯一生成物，切到 154g 后首行变成 154g 指纹，切回 note4 即触发 `*** Reinstall Arduino framework ***`）；隔离的价值是**重装只发生在目标自己的包目录里、不伤对方**（整轮两个目录完好，note4 ROM hash 未变 `42AAF00B…`）。
  - **随后补上"每目标整份 sdkconfig 快照"后，交替已不再重装**：`tools/pio-target.ps1` 把 `sdkconfig.defaults` 按目标快照到 `.pio-core/sdkconfig.defaults.<target>.snapshot`，构建前还原、构建后再快照（还原的是该目标自己生成过的完整文件，不是伪造首行——`next.md` §10 禁止的是后者；安全证明：重建的 154g ROM 与记录值逐字节相同 `91937B18…`）。实测：priming 45.7 s → **note4 39.2 s、154g 42.1 s，两侧均无 `*** Reinstall ***`/`Compile Arduino IDF libs`**，ROM 哈希不变。副作用：priming 仍需一次完整重装，且该路径需工作区外写权限（受限沙箱下需提权；uv 缓存 `%LOCALAPPDATA%\uv\cache` 否则 `os error 5`）。
  - 同一脚本还修了 cwd 依赖（`Set-Location $RepoRoot` + `pio run -d $RepoRoot`）：嵌套 pwsh/沙箱 broker 会把 `pio` 的 cwd 换成别的目录 → `NotPlatformIOProjectError`。
- **上次考察的出处**（下个窗口先读这些，不要重推）：`docs/history/next-2026-09-25.md` §7/§10/§11 + `artifacts/hash-forensics/{compute_fingerprint,dump_effective,pin_down,why_differ}.py`（本地 gitignore）+ 上游 pioarduino PR #511 / issue #532、#533 / Meshtastic PR #11834。
- **重装在受限沙箱里跑不完**：uv 要写 `%LOCALAPPDATA%\uv\cache`（工作区外）→ `os error 5 拒绝访问` → `Failed to create a proper virtual environment`；`UV_CACHE_DIR` 指到工作区内可解这一条，但 `tool-esptoolpy` 的 editable 安装仍因写 junction 指向的共享包目录报 `Cannot update time stamp of directory 'esptool.egg-info'`（非致命警告）。要完整跑通需提权或把这两条路径纳入工作区。
- **脚本坑**：`tools/pio-target.ps1` 在**后台作业**里会假失败——它 `& pio run` 时 cwd 丢失成 `D:\Documents\project`；给 pio 加 `-d <repo>`（或在前台 shell 跑脚本）即可。
- `next.md §6` 的 `extends` 重构未做。「只构建 Note4」现仍成立，但已明确是**成本**约束。
- **2026-09-28 回头查已完成**：在 Note4/Bridge OTA 闸门通过后才构建 1.54。此前 1.54 的 `.pio/build` 对象目录已不存在；PlatformIO 的全项目 `project.checksum` 纳入源码文件清单，结构变化使整个构建目录失效。重新建立缓存的首次 154g 构建 318 个对象、无 framework/IDF 库重编；其后 154g→Note4→154g 三次均零编译、ROM 哈希不变。详细证据见 `docs/history/progress-archive-2026-10-03.md` 相关历史节及 `project-workflow/target-build-cache/plan.md`。`extends` 重构仍未做，但不影响已验证的目标切换；没有必要为避免正当的结构失效而伪造 checksum。

### C7. 其他单项遗留（已归档专项带出来的、仍然有效的条目）

来源：`docs/roadmap/archive-digest-legacy.md` 与 `-recent.md` 的"未完成/遗留"。这些不构成新的 initiative，但**不能因为归档就当作已完成**；要动哪一块就在对应目录（现在在 `docs/history/workflow/`）里查上下文。

| 专项 | 仍然有效的遗留 |
|---|---|
| `ble-rendezvous-power` | **stage 3–6 全部未实现**（GATT v2 事务；时钟优先会合 + radio 硬切断 + `POST /power`；桥常驻监听 + WAKE/RENEW/SLEEP；分阶段启用与实测功耗）。另：AP/桥不可达时的失败路径未测；stage 2 固定机位照片验收与 90 次局刷 soak 未做；stage 1 的 BLE 回归需一次 BOOT 点击 + 用户提供基线照片；task-10 的 BOOT blink 检查与"无 AP"失败运行未做；task-8 的 5 项 OTA 改进未做 |
| `power-state` | task-5 **整机硬件验收整项未做**：按键/GP3 LED、插电拔电宽限、低电 5% 断电、WIFI OFF 深睡节奏、T10 电池斜率、T9 push 延迟、UDP 换 IP 生效、桥失联 `OFF N M`、OTA 回归、三端模板哈希一致 |
| `bridge-multi-device-ui` | **已做（2026-09-25，见 `docs/history/progress-archive-2026-10-03.md` 相关历史节）**：桥侧 legacy 通道与历史迁移删除（含静默写标记缺陷）；`AppCtx` 全局单设备字段与缓存改为按 MAC 运行记录（新 `device_runtime.rs`）；发现/claim/owner/note/pending 按 MAC 路由；UI 与 MCP 的设备类操作要求显式 MAC，多设备缺 MAC 直接拒绝（两台实机自检通过）。**仍缺**：文件模板库 `Library` 去留待决策（A5）；统一族推送菜单的 0/1/多设备、跨族、离线、被占用、在途作业验收；双设备**发布**（非只读）实机验证；`ui/index.html` 未被人眼验收 |
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
| D4 | `next.md`（33 KB）主体是已完成的合并/多 env/包隔离记录，头尾仍像"开工指引" | 新窗口会照着过期步骤做（其 §5.2 的 stash 流程已实测不成立） | **已执行**：归档为 `docs/history/next-2026-09-25.md`，§7 包隔离结论并入 `docs/history/progress-archive-2026-10-03.md` 与 C6 |
| D5 | `AGENTS.md` 的"硬件现场与坑"含过期地址（`192.168.1.50`、`70:04:1D:AA:BB:CC`、"SSID home-wifi"）与已停用目标（`esp32-s3-epaper-154g` 的 40 MHz 闪存脚注） | 每轮会话都注入，直接占用上下文并给出错地址 | **已执行**：改为"以桥登记为准"的设备表 + 历史细节下沉；地址不一致已标注为待核实 |
| D6 | 显示宽度两处不同源：位置来自编译产物 `v2Ct.ops[]`，尺寸在运行时由 `tplFontClockBox()` 重算 | 已在 0.18.23 修掉一类（`2L*digitAdv` → `4L*digitAdv`），但只要模板给时钟加 `rect`+`align` 就会再犯 18 px 偏移 | 把测量宽度写进 `CtOp`，需同步改 Rust 编译器与 Python 测试桥哈希 → 独立任务 |
| D7 | ghost 预算两套计数：`rtcClkPartials`（上限 90）与 `epdPartialCount`（上限 30）并存，`refresh_policy.cpp:108` 又写死 90 | 行为不一致、改一处忘一处 | 统一为单一预算源，`refresh_policy.cpp` 从 `CLK_GHOST_LIMIT` 取值 |
| D8 | ROM 哈希随 packages 绝对路径变化（`pio-pkgs` 字符串被编进固件），同一源码不同路径产出不同 SHA256 | "可复现构建"不成立；跨机器核对 ROM 会误判 | 加 `-ffile-prefix-map` / `-fmacro-prefix-map` 归一化（未做，留作决策） |
| D9 | 构建沙箱归属：新目录若在沙箱内创建会落到 `CodexSandboxOffline` 属主，缺能力 ACE，导致目录内文件写入被拒 | 曾造成 stash 丢 16 个文件的现场事故 | 保持 `AGENTS.md` 的提权建目录约定；不要把 `git stash` 用作保护现场的手段 |
| D10 | `sdkconfig.defaults` 是全项目唯一生成物，被各 target 争写；交替构建必触发 framework 重装（实测 154g 188 s + note4 926 s） | 任何"多目标"操作都极其昂贵 | 已被"只构建 Note4"约定绕过；若恢复多目标必须先落地 C6 的包隔离 |
| D11 | ~~已存在文件的 DACL 缺沙箱能力 ACE~~ → **已解决（2026-09-25）** | — | 用户修好 ACL 后，`generic-display-platform-design` 与 `live-template-delivery` 已成功 `git mv` 到 `docs/history/workflow/`（归档总数 17 个）。关键判据：**真正挡路的是 ACE 不是属主**——ACE 补齐后，即使属主仍是 `CodexSandboxOffline`，`git mv`/`Add-Content` 都能成功。已把该结论记入 `docs/history/progress-archive-2026-10-03.md` 与 `docs/history/acl-repair-notes.md` |
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
