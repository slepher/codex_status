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
| Note4 与 1.54 蓝牙 Wi-Fi 初始化 | 原始流程已设计；Note4 后续 Wi-Fi 管理/BOOT 恢复已实施部署，完整验收未完成 | 双 ROM 内置配网页/status bar、新配对屏幕码、Windows 首次接入与 1.54 实机验收；见 B6 |
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

### A9. PowerPlan 与 Data 同轮联合下发（2026-10-05）

- **2026-10-20 Plan激活已修并配套实机验收**：原60→59 Data872/Plan5880获Light却下一周期才联网，是network_start拆分导致；新能力由Plan原事务冻结目标摘要/回报路由并在BLE结束建连。Note4 `.106` 原OTA job f7186b2f succeeded/image_verified，Mac12:09已部署；401回归/5条件忽略，最终协议8、栈88项通过，周期同值Sleep对照通过。用户允许蓝牙/定位后14:20原Plan5904 ACK network_activation=plan，3.304秒内同Plan ready，随后同MAC认证HTTP Wi-Fi连接/BLE关闭/原期限递减，本轮complete；14:21 Data876 applied/displayed，用户目视确认Wi-Fi图标已显示。自然新余量全链路时延未验，不结A9。[实施/部署/边界](../../project-workflow/bridge-macos/plan-activation-20261020.md)。

- **2026-10-20 Wi-Fi余量快路径已修并部署Mac**：通知即时响应+源60秒兜底；新工作首次投递，只有待确认按设备每6轮重试，源消费/跨MAC独立，Data先于诊断。Core181/App122+集成26通过，最终定向3项通过；同MAC Data871 HTTP ACK applied/displayed及认证读回通过，源刷新实测61.49秒含RPC。仍缺真实新余量变化的通知/屏幕时延、Windows部署与长期RF；上游通知未到达原因未定。见[实施与部署](../../project-workflow/bridge-macos/wifi-usage-fast-path-20261020.md)、[原调查](../../project-workflow/bridge-macos/wifi-usage-latency-20261020.md)。

- **2026-10-18 Bridge重启首轮用量修复已部署Windows**：HTTP/BLE新Data区分源未就绪与真实missing，首次成功前明确等待、不制造正式Light，原in_flight保留。285回归通过/3条件忽略；补字段摘要日志后core159通过，11:43重启首轮Data738获ACK、认证displayed，源快照重建CRC一致，周剩余98%非missing，仅5h真实缺失。原data/凭据保留；本次首联系时源已ready，冷源等待仅回归覆盖，未目视验屏/未部署Mac。[部署、首包与验收边界](../../project-workflow/bridge-macos/first-contact-usage-20261018.md)。

- **2026-10-11 22:09配套部署完成与恢复缺口**：最新.83哈希/v5/最终Bridge均认证核验，见PROGRESS与[部署记录](../../project-workflow/bridge-macos/device-fields-deploy-20261011.md)。新Bridge拒绝旧设备合同，需先交接有效模板；本次原Bridge完成交接后恢复新版。BLE网络准入提前把Bundle标Sending、重启变Unknown后挡住OTA且不自动查询原BEGIN；本次按原ID/冻结字节查询并续传成功。后续补发送前与未知结果恢复，不以本次人工闭环代替自动恢复验收。

- **2026-10-11 当前设备字段收敛（候选未部署）**：删除模板 `device.state` 兼容，使用独立 Wi-Fi/BLE/mode 字段；已持久化 CT 在加载时校验未知绑定、编号一致性和字符串边界，失效进入内置恢复界面，保留文件直到显式发布替换。[实施与回归](../../project-workflow/bridge-macos/device-fields-only-20261011.md)。实际 v4 模板升级前须配套准备有效 v5 Bundle，不以宿主测试代替实机验收。

- **2026-10-11 BOOT网络/图标修正候选（未部署，关联B6/C8）**：Bridge失败不拆除已有Wi-Fi关联，BOOT/正式/临时期限不变；presence反向验证待定不冒充不可达；Bridge明确错误立即OFF；真实布局v5模板使Wi-Fi与BLE独立；boot_id与会话nonce分离，补正常事件诊断。[实现、Luna回归与主agent审查](../../project-workflow/bridge-macos/boot-network-fix-20261011.md)。未刷机/未发布模板，实体RF/EPD/重连矩阵仍待验，不结项。当前实际部署为.83+配套Bridge，以PROGRESS为准，下方“未部署/.82”仅为当时记录。

- **2026-10-11 周期Data临时Light缺口修正（未部署）**：周期全量及其in_flight/重试不再形成正式Light意图；大Data以reason=data/data_seq准入独立90s临时HTTP窗口，正式Light到期不截断、同ID不续期，读取/Data/诊断完成后收尾。协议及权威电源文档已补充；[实施与候选证据](../../project-workflow/bridge-macos/sync-window-fix-20261011.md)。现场仍.82/原Bridge，实际无USB自然睡醒、超时/丢ACK/RF/功耗矩阵待验，关联B6/C8不结项。

- **2026-10-11 行为缺口候选实现（关联 B6/C8）**：最终用户合同为 BOOT 开/关/再开窗口、健康 HTTP 禁自动 BLE、断网/Bridge HTTP 失败有界 BLE 汇报再回普通周期。[实施与验收](../../project-workflow/bridge-macos/boot-network-20261011.md)记录 .77 候选、311 测试通过/3 忽略及构建/栈门禁；未部署。后续[短时入睡调查](../../project-workflow/bridge-macos/boot-five-second-investigation-20261011.md)证实PWR唤醒6.977秒后深睡，收尾遗漏provisional；初始化BOOT漏按路径存在，具体物理时序未记录。后续 .78 已补两处缺口并通过原OTA单次上传认证镜像核验，配套Bridge已更新；实体按键重叠、RF/功耗/屏幕实机矩阵未验，不结项。

- **2026-10-11 协议闭环实施**：诊断尾部沿用HTTP准入、ready报告ACK后验证持续HTTP事实已修复；扩展审核修复新写入401/403仍未知、409修订冲突误分类、wifi_state合同文本类型。316项通过/3条件忽略、设备页及52项栈门禁通过；新版Bridge已部署，.76 OTA原job86054e26单次上传并image_verified，独立认证运行hash一致；13:10自然恢复会合/Data414通过。真实10轮自然会合、物理故障矩阵及显示未验，不结A9/B6/C8；详见[实施记录](../../project-workflow/bridge-macos/protocol-implementation-20261011.md)。

- **2026-10-11 Luna提交历史审核与协议闭环计划**：检查09-23以来85个提交、重点10项；主代理复核诊断尾部绕过HTTP准入、瞬时ready阻塞合法HTTP两项P1仍存在。依序补生产ble_cycle组合回归并修复、跨通道身份/事务换接、错误与未知结果全链审核、故障矩阵及Note4自然会合验收；关联B6/C8。[证据报告](../../project-workflow/bridge-macos/protocol-history-review-20261011.md)、[执行与结项闸门](../../project-workflow/bridge-macos/protocol-followup-plan-20261011.md)。本轮仅审核/规划，未修改生产代码；.75已部署，不把待实机验收误写为源码未修。

- **2026-10-11 Wi-Fi模板元数据候选完成**：用户明确图形交给模板，新增本地wifi_state/wifi_blink及阶段/相位重绘，等待不冒充OFF；quad v13/Note4模板v3，.75候选构建/宿主像素/52项栈检查通过。未部署/发布，RF闪烁与会合刷新耗时仍待验；诊断HTTP准入和ready竞态另两项未改，见[实施记录](../../project-workflow/bridge-macos/wifi-template-metadata-20261011-plan.md)。

- **2026-10-11 协议审核，待修复**：Note4 BLE seq398成功但本轮无需HTTP时仍执行约49s诊断HTTP尾部；会合ready瞬态与持续HTTP可用性混用，已认证verified仍超时。与B6共同验收WIFI OFF只作终态、冻结准入和迟到HTTP验证，见[审核与现场证据](../../project-workflow/bridge-macos/wifi-terminal-review-20261011.md)。本轮只读，未部署。

- **17:33 自动数据投递修复已部署Bridge**：30秒过期状态不能阻断自身刷新；登记IP每分钟有界恢复探测、认证/owner投递闸门保留。实机源used24→余76，自动seq377 applied/displayed且直接认证双序号377，确认指纹匹配最新源。Fake真实HTTP连续两次源变化/过期缓存恢复通过；ROM保持.74，实体多周期矩阵仍待验。

- **2026-10-09 16:30 重新打开手动激活/显示验收**：.73 实机仍报告 Sleep 无 Bridge；16:17 激活的设备最终回执为 `window_closed`。代码发现深睡无新 Data 仅刷新时钟而跳过联系恢复图标、EPD失败仍更新睡眠/失联标记，以及物理 BLE hold 与激活完成轮询互相等待的路径（现场 hold 条件待核对）。已有 Fake 按状态重绘全帧未覆盖真实会合调度；需补上述调度和显示失败回归，不能以 .73 部署宣称本场景完成。现场及下一步见[专项](../../project-workflow/bridge-macos/wifi-idle-activate-20261008-plan.md)。
- **16:39 USB/恢复补证**：COM5/MAC匹配，实机串口 .73确实WIFI OFF；busy_fails23是RTC累计，局刷回退且trusted1，不据此断言全屏损坏。Bridge note_match遗漏合法window_closed错误并return None，导致失败后不更新Connection/不恢复，16:39仍保留16:17 connecting；多轮network_start而无新activate已见日志。需追加错误投影及有界恢复回归，ca最初超时触发条件仍待验证。
- **17:08 .74修复已部署**：BLE hold仅限制新关联、不阻塞激活完成；健康同配置激活不先断网，Bridge有界错误投影/恢复、Sleep无Data联系标记重画和确认帧失败处理已实施。Fake补深睡RTC联系保留/新session BLE恢复/失败像素重试，最终62/62、app/shared render通过；Note4认证.74/ota_0与完整ROM hash一致，原home配置已真实恢复，OTA原job1172f92d单次上传image_verified。初始ca物理hold触发条件、实体Sleep目视及长期RF/功耗/EPD矩阵仍不因此结案。证据与资源见专项最新收尾及PROGRESS。
- **2026-10-09 .73追加部署**：自动短读/闲置联系复用仍有效的已ACK Light原ID，保留绝对期限；显式hold按剩余时间而非每轮600秒，显式Sleep仍可提前结束。Bridge图标按跨BLE/HTTP有效联系独立计算和重画；会合Light→深睡走enterDeep持久模式/睡眠全帧，不绕过Zzz。Note4认证运行 .73/ota_1、完整镜像127345E2…DA9450；OTA原事务单次image_verified，排队后约142秒确认，超两分钟继续核验，未重传。共享回归与Fake场景/剩余实体边界见专项及PROGRESS；书桌屏不升级。以下待修描述为定位时点，相关三个实现问题已由本段关闭。

- **14:06数据后Light期限被提前覆盖（已修复，.73部署）**：Data369+Plan2963 ACK600 s，本应至约14:16；后续2964/2965改120 s，2966在14:10:58发Sleep。coordinator普通note_plan_ack只更新accepted记录，仅pending_explicit_light确认会hold_light；note_ack成功清push_dirty后service重新按工作选择120/Sleep，未保护自动数据授予的原期限。修正须区分数据驻留、临时读网及显式提前睡眠，冻结/保留实际确认期限；短任务结束/重试/下一次空闲会合不缩短也不无限续租，重启/读回恢复同一窗口。配置/caps当前600 s；14:10用户所见数值变化的精确时点仍以日志为准，本轮已确证14:05源变化→14:06投递及提前覆盖，未改生产计划。
- **2026-10-09新实机缺陷已修复并仅Note4部署 .72**：完整Status4048 B以上冻结有界分页，不裁剪合同；业务错误识别、整体HTTP期限（含session锁）及独立Bridge错误状态已落地。会合成功清除连接动画，Wi-Fi变化独立重绘并有界全刷重试。Fake大Status→同源C++→生产客户端闭环及总预算拒绝、最终bootstrap58/58、app75/core132/HTTP7及显式3项联测通过；实机完整Status、Wi-Fi connected/HTTP ready、运行完整镜像SHA与发布ROM一致，原b0事务正常completed。剩余：多周期实体RF/电池/掉电/动态堆峰值和显示故障矩阵；可信驱动基线不替代屏幕目视。macOS原生UI需在有Swift/WKWebView的主机验收。本次现场/资源/哈希见PROGRESS顶节，不因此结A9/B6。
- 2026-10-09用户明确Wi-Fi/Bridge状态独立：驱动关联/IP/实际关网决定Wi-Fi事实；Bridge认证、协议、Status超帧及结果未知只改变Bridge层，不能反推Wi-Fi断开或触发无根据的重配/重关联。修正需保留独立电源窗口关网语义，并验证Wi-Fi已连接但Bridge认证/回读失败时Wi-Fi状态与图标不变；当前Bridge连接流程state混入bridge_unreachable需分层处理，固件offlineMins的Bridge失联标记不能当Wi-Fi图标。
- **14:06–14:12照片/日志复查仍有固件显示语义bug**：BLE联合Data seq369 ACK applied，但`renderActiveUsage`仍以`wifiUp ? -1 : 0`生成Bridge失联字段，模板`exists=true`显示断开及OFF 0M；.72只修网络重绘和Bridge应用模型，未解耦此固件分支。后续应依据跨HTTP/BLE的有效Bridge联系计算失联，不因实际Wi-Fi关网直接设失联，覆盖最近BLE更新+Wi-Fi关闭、真实联系过期、连接中/深睡继承及恢复重画。Wi-Fi图标依据驱动另验，不为消除图标强制常驻Wi-Fi。照片无Zzz不证明light；Sleep Plan2966/2967 ACK且Light剩余0不等于已进入深睡。本轮直连超时、旧认证缓存stale_boot，实时RF/物理睡眠未知，未新增刷机或提交。
- 图标独立刷新追加缺陷：会合Wi-Fi ready先写wifiUp=true却不重画，后续pollWifi丢失上升沿；无新Data的light会合/深睡无sleep glyph标志路径可能保留WIFI OFF旧帧。须补同源“联网成功+Bridge读回失败/无Data”图标更新回归并修正，不依赖业务同步触发Wi-Fi状态绘制；EPD局刷回退另验。

- 用户已确认：真实push变化时，Bridge发送前冻结电源意图；小数据与PowerPlan同请求，大数据先Plan再经合法HTTP窗口投递。Data ACK成功不立即撤销活跃窗口，丢ACK不阻断Plan下放；重试不续租、不换原事务ID。
- 现场weekly余量65（usedPercent35），14:18:27检测usage变化；14:20/14:22/14:24 BLE data ACK超时，最后确认仍Plan1274/sleep/0s；设备后报applied_seq97，Bridge原seq97仍待确认。已证明Bridge先Data后Plan的编排缺口；ACK丢失的RF/设备原因未确定。
- 执行计划：[精简协议实施计划](../../project-workflow/bridge-macos/bridge-device-compact-protocol-implementation-plan.md)取代旧A9中的能力协商与兼容建议；Plan-first可选Data、分项结果已实现并通过合成部分拒绝测试，2026-10-08仅Note4部署新协议，实体剩余矩阵待验。旧[执行计划](../../project-workflow/bridge-macos/powerplan-data-execution-plan.md)保留A9原业务背景，不作为最终wire方案。
- 完成判据：小数据联合下放及大数据交接通过；ACK丢失仍能确认原Plan/Data；拒绝/冲突/过期/未知有明确有界反馈；原期限不被重试延长；授权后的Note4自然会合、BLE OFF/实际PM及到期deep实机验收分别留证。沿默认Note4构建约束，1.54与其他实例不自动纳入发布。

- 2026-10-08 P0–P5已进入生产代码并按用户授权仅部署Note4新协议 .71；同一Fake独立report→生产Bridge接收器→认证HTTP回读闭环已通过，保留非loopback源IP/MAC/session/owner/期限校验。新协议空闲陈旧连接状态仍授Light的实现bug、报告owner字段错误及op19二进制POST丢URL参数均已复现修正；四轮后续联系无新activate/RF begin仍1。app75/core129/HTTP6/Fake57及3项显式同源联测通过，匹配ELF栈门禁通过；实机.71运行前缀SHA256 DE7897DD…E6D31与ROM一致，原OTA单次上传image_verified。书桌屏暂不接入新Bridge，登记与配置保留。最终运行现场/观察见PROGRESS顶节及[周期activate计划](../../project-workflow/bridge-macos/wifi-idle-activate-20261008-plan.md)。Rust Fake报告adapter不等同Arduino HTTPClient/RF；丢失/迟到/重启与双向不可达组合、长期RF/功耗/动态RAM/掉电及macOS实体矩阵仍待验收，不结A9；B6配置事务、C4功耗和C8诊断继续独立保留。

- 用户追加约束：Bridge/设备分别更新软件版本，不提供协议版本历史或旧协议兼容；实施计划已取消能力降级、双协议和回退重发，最终一次性替换。信封采用固定marker，不维护协议版本号。

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

### A5. ~~降低 `PROGRESS.md` 的交接读取成本~~ → **已完成（2026-10-09）**
- 主文件现在是约 2 KB 的当前交接卡；详细进度按日期归档，待办仍只维护在本 backlog。维护规则见 `AGENTS.md`「交接与工作流」。

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

- **2026-10-12 统一授权入口已部署**：按用户新要求合并定位/蓝牙按钮，首次启动仅对未决定权限自动请求，原生delegate在定位选择后续请求蓝牙；拒绝不自动重弹，统一入口可开隐私设置。Node/App85（3条件忽略）/WK52与窄宽审阅通过；部署后MCP双allowed，未主动重置权限/钥匙串，首次未决真实弹窗次序仍未人为重现。详见[专项](../../project-workflow/bridge-macos/permissions-unified-20261012.md)，不以此结整个B5。

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

- **2026-10-19协议收敛R1–R5源码完成（关联C8，Windows/Note4已部署）**：已删除重复Wi-Fi执行/全局缓存、私有JSON会合状态、同步HTTP worker及旧JSON会话/op路径；C2显式35操作覆盖Bridge、Note4、1.54、Fake及AP01应用。491回归通过/5条件忽略、双ESP构建/匹配ELF栈、AP01 RV32/宿主ASan通过。新schema部署须冻结exe/data，不兼容旧wire；AP01完整候选仍flashable=false，不关闭C9。源码提交9e748eb后，Windows Bridge已重编译部署、Note4受鉴权直接OTA至.105/ota_1且完整运行镜像SHA匹配；15分钟观察完成：OTA后7次自然交汇、Data828 displayed、16条历史Wi-Fi Job/Profile不变，Light到期后按Sleep周期联系，见[rollout](../../project-workflow/bridge-macos/protocol-convergence-rollout.md)。切网fix分支合并与现场证据另列，长期RF/Flash/原生BLE矩阵仍开放。[删除清单、候选哈希与验收边界](../../project-workflow/bridge-macos/protocol-convergence-status.md)。

- **2026-10-19 Windows切网恢复已验收**：独立基线4868c8c排除未部署协议收敛，复用724f3a3修复旧Windows exe异步激活后漏network_start；332回归通过/5条件忽略。冻结264文件备份后部署，自然会合切至wd21-la、认证HTTP ready，原ID017a completed/revision342、14条历史Job不变，用户确认OFF图标恢复；无刷机。长期RF及本项其他验收仍开放。[证据](../../project-workflow/wifi-switch-fix/status.md)。

- **2026-10-19 事务终点逻辑修复已部署，关联C8**：终态未知历史释放准入，缺失/变化身份与三次原ID确认预算结束执行，保留未知结果；异步激活继续network_start，已发控制事务纳入恢复查询。Rust334项通过（含显式保护用例）/剩余4条件忽略，Node/WK68及窄宽截图通过。最终Bridge10:35部署后自然会合自动切回办公室Xiaomi_1EBB，003f按原IDcompleted、revision341/active office/v235、认证HTTP200；三条原未知历史逐字段及SHA不变。未刷机/BOOT/手动切网，图标未目视；长期RF及B6其他验收不因此结项。[修复与证据](../../project-workflow/bridge-macos/wifi-transaction-finality-20261019.md)。

- **2026-10-14 连接结束修复与离线日志已部署**：服务发现期间断开漏回pending connect导致12:20永久隔离；14:28 BOOT为同一隔离延续。Bridge根因修复14:44部署，真实handler7项/共享Fake红→绿及App94/BLE26/Fake68通过（3条件忽略）。.101完成跨深睡读回；最终Note4 .102于15:48单次原生OTA、认证运行hash匹配，LittleFS保留24小时内/384KiB日志，补薄唤醒RTC回填和过期去重，不改分区/NVS。正式Sleep→自然醒次3 BLE ACK/关闭0ms→Light3948/认证HTTP恢复，Flash40条3504B CRC正常、薄唤醒2摘要回填、原27条保持，无认证401。生产存储到期/轮换/截断/资源及最终Fake68/匹配ELF栈通过。初始物理链路超时因旧记录覆盖仍未知，长期RF/真实24h矩阵未验，不结项。[实施与部署](../../project-workflow/bridge-macos/offline-log-connect-fix-20261014.md)、[调查](../../project-workflow/bridge-macos/ble-connect-disconnect-20261014.md)。

- **2026-10-14 Fake ROM逻辑覆盖范围收缩，待实施**：以[七项生产逻辑覆盖清单](../../project-workflow/bridge-macos/fake-rom-logic-coverage.md)为后续入口，依次核对会合重试、期限、连接代际、权限、OTA完成判定、显示确认及按键解释。只补Fake绕过的生产分支和最小组合用例，不预建外设/现实故障模型；配置事务、Bundle和渲染先复用已有测试。清单文档化不代表实施或验收通过。

- **2026-10-14 CoreBluetooth断开竞态修复已部署，连续会合验证通过**：系统日志确认设备已主动断开，Bridge随后重复disconnect；锁定btleplug缺失外设时漏回future造成2s超时/永久隔离。补齐原生线程上的幂等close/state回复，保留真实未知隔离并补WARN。实际handler/future回放2红→4绿，BLE26/App94通过（3条件忽略），10:54部署新版hash f3576f02…；正式Sleep3897后定时唤醒，连续两次BLE会合关闭0/1ms、Data590及HTTP恢复通过；非长期RF验收。后续已补独立Fake BLE/ROM proxy，共享实际close/state/future和隔离路径；假StaticJson连续3帧对拍通过，恢复旧分支可复现第二次连接隔离。App94/BLE26/Fake67通过（3条件忽略），新测试结构未部署，见[Fake生命周期](../../project-workflow/bridge-macos/fake-ble-lifecycle-20261014.md)。[证据/实施](../../project-workflow/bridge-macos/ble-close-fix-20261014.md)。

- **2026-10-14 .100已部署**：用户BOOT就绪后直接原生OTA一次，认证ota_0/运行hash匹配，稳定MAC回执使BLE status/claim/plan通过；HTTP Data578 applied/displayed。转Wi-Fi再次触发Bridge disconnect_timeout，10:40托管重启清隔离，原App二进制未改。当前恢复不等于Bridge恢复逻辑修好；本次button醒，timer长期验收仍待完成。[部署](../../project-workflow/bridge-macos/deployment-20261014.md)。

- **2026-10-14 再次断连调查（未恢复现场）**：10月13日18:26后无认证HTTP成功，19:36 BLE disconnect超时2001ms使NativeLease永久隔离；此后同进程无扫描，次日权限allowed/health204仍正常。App仅debug记录restart_required，反馈与恢复不闭环；既有故障测试通过。单独重启只解进程阻断，仍须配套部署.99稳定MAC修复并验自然timer醒次；不能无证据清隔离。详见[本轮证据与边界](../../project-workflow/bridge-macos/disconnect-20261014.md)。

- **2026-10-13 长断连根因与源码修复（未部署）**：17:02:51–18:10:36 共 29 次 BLE INFO 成功后会合失败。endpoint ACK 错用依赖 STA netif 的 `WiFi.macAddress()`，冷醒输出全零 MAC，被 Bridge 身份匹配过滤，5秒超时阻止后续网络命令；改用稳定 `radio::macText()`。生产处理函数/codec 8项红→绿、BLE-first检查通过；实机第69次timer无命令，第70次button恢复Wi-Fi。当前仍.99，未抓历史ACK字节，新ROM自然定时醒次闭环待验。[调查与边界](../../project-workflow/bridge-macos/disconnect-20261013.md)。
- **2026-10-13队列状态修复已部署**：取消/到期停止自动查询，身份缺失/变更转needs_attention，晚到普通失败不复活/覆盖，已在途真实回执仍可确认；显式新请求按自身job_id立即尝试。Core153+22集成/App94（3条件忽略）/UI66、窄宽截图通过。两条旧同步恢复已取消，activate停止自动重试；保留历史unknown。设备HTTP当前不可达，在线直达由模拟服务验证。部署hash与数据备份见PROGRESS和下方调查。
- **2026-10-13在线读取排队调查**：新read立即执行入口使用通用FIFO机会，旧身份未知事务仍可入选；每轮只处理一个，后台机会又位于claim/Plan之后。实机16:53读取约11秒完成，快照两页约500ms；生产准入方法隔离探针确认旧事务可抢机会。当次缺任务选择/锁日志，无法完全归因；修复已部署，见[调查与边界](../../project-workflow/bridge-macos/wifi-read-queue-investigation-20261013.md)。
- **2026-10-13最简OTA兼容修复**：`.99`候选保留逐键旧绑定和缺身份BRS2，不完整记录隔离授权/路由，完整绑定可继续使用；物理修复保留已有namespace，不猜旧身份。取消USB导出BRS刷机前置。生产存储ASAN/分配故障、BLE/Fake endpoint与原生OTA回归、匹配ELF64项门禁通过；用户就绪后`.99`已直接OTA并认证哈希核验；最终Bridge 237cd5a0…已部署，补修同context数据归零的旧ACK抑制，Core152+集成22/App93通过，实机569 applied/displayed、Codex good。证据见[部署记录](../../project-workflow/bridge-macos/deployment-20261013.md)，不结B6/C8。

- **2026-10-13用户修订**：配置保持WFS4编码，与WTX1事务恢复信息独立存储，实时状态留RAM；现有WFS4读取不改字节，32 B原子提交记录保持配置/回执一致。替代WFS4→WFS5部署迁移安排；源码与宿主验收见[拆分专项](../../project-workflow/bridge-macos/wifi-config-state-separation.md)，实机与BRS身份门禁仍未关闭。

- **2026-10-13继续Bridge/Fake ROM，未部署**：endpoint注册改为能力门禁、只读先核对、完整身份关联ACK；持久拒绝/未知、取消和丢ACK跨连接/重启核对、App/UI错误链已补宿主验证。旧固件缺能力发送前拒绝，匹配登记零NVS写。详细证据/限制见[执行状态](../../project-workflow/bridge-macos/protocol-redesign-status.md)末节；BRS/Bridge联合迁移、P2/P3/P5/P6与实机验收仍开放，不结B6/C8。

- **2026-10-13协议重设计阶段提交，未部署**：按用户要求停止推进并保存P0–P6阶段成果，详见[执行状态](../../project-workflow/bridge-macos/protocol-redesign-status.md)收尾节。Wi-Fi运行时收束WFS5、BRS拒绝旧/缺身份库；离线转换与宿主回归通过，实际联合迁移未完成。继续入口包括BLE endpoint注册的持久ACK与结构化错误链（当前Bridge只等写入）、BRS离线身份恢复及原计划其余；不结B6/C8。

- **2026-10-12重设计P1推进，未部署**：[执行状态](../../project-workflow/bridge-macos/protocol-redesign-status.md)记录类型化结果/取消与TTL后原ID确认、设备三元回执隔离、WFS5执行代际/原子中断终态、HTTP实际发送证据及worker保留设备许可。core168、app87/3忽略、Fake64、C++管理与ASAN存储通过；Note40.18.86候选及64栈项通过。P2扫描/现代建链截止、取消隔离、配对与控制写保护BLE26项已通过；P3旧Pusher驱动删除及CLI现行入口通过app87/3忽略、Fake64；全层身份围栏、P0发布转换、其余P2/P3及P4–P6仍开放；未启停默认Bridge、未访问设备。

- **2026-10-12分层重设计细化，实施进度见上条**：[设计](../../project-workflow/bridge-macos/protocol-redesign-design.md)、[P0–P6计划与故障矩阵](../../project-workflow/bridge-macos/protocol-redesign-plan.md)明确业务/执行器/HTTP-BLE职责和状态机，保持既有电源及双向HTTP原则。用户明确最终不留旧协议/降级，删除旧permit/management2/旧恢复分支；独立OTA保留简单token鉴权原生上传，不加Bridge/owner/session/frame。先业务结果与建链生命周期，再统一入口/删除旧栈；OTA独立验收。旧持久未知回执不得因升级清空；设计提交时尚未实施；本轮增量实施/测试见上条，部署仍未执行。

- **2026-10-12总体架构复核完成，未实施**：[源码审查与替代设计](../../project-workflow/bridge-macos/architecture-review-redesign-20261012.md)确认五类建链失败无界清理及Fake接缝缺口；新增trial/activate结果查询失败误清提交未知标记、事务namespace键不一致、取消/过期停止确认义务。先统一生命周期与事务结果，再收敛状态投影；验收必须证明失败后下一轮扫描/HTTP继续、未知原ID不重发及跨namespace隔离。用户确认保持深睡BLE会合及周期HTTP同步、浅睡HTTP直达、设备HTTP回报与Bridge反向请求；上层载体无关，下层选择合法可用通道。报告已按此修订，取消HTTP拉取替代方案评估，不改变电源策略。

- **2026-10-12 用户要求下一轮总体分析（关联C8）**：重点审查“防御代码多但关键逻辑漏处理”，按生产生命周期与资源所有权解释局部有界而失败清理无界的矛盾，评估重复防御的删除/合并并保持有效不变量，提出最小修复与失败后继续会合的验收。[当前问题报告/新窗口入口](../../project-workflow/bridge-macos/overall-analysis-handoff-20261012.md)已区分现场事实、推断、已部署与候选；本次仅记录，未做全仓审计或实施。

- **2026-10-12 13:19后BLE扫描停滞，15:55清理修复已部署**：建链失败无界disconnect及其余裸清理统一原2s预算，记录开始/完成/明确失败/耗尽，不增大连接或业务超时。生产控制流注入两次connect失败+cleanup pending，red→green验证原错误返回/锁释放/下一连接；Rust269/3条件忽略、Node/WK60通过。实机15:56首轮Plan写入未知后持续扫描，15:58同MAC/wake908完整会合、15:59认证HTTP通过。总体P2迟到回调/worker失效等矩阵仍未完成，原进程具体卡点无栈实证。[修复与部署](../../project-workflow/bridge-macos/ble-connect-cleanup-fix-20261012.md)、[原取证](../../project-workflow/bridge-macos/disconnect-investigation-20261012.md)。

- **2026-10-12 macOS读取RF截止修复已部署**：Plan ACK后及SyncOpen后读取同MAC/wake真实INFO窗口，完整发送丢ACK允许只读恢复，未知分片不复用。生产Fake四种边界及App85/BLE17通过、3条件忽略；用户恢复权限后最终Bridge实机BLE完整读取r237成功，后续认证HTTP正常且一次完整读回r238（active office/Xiaomi_1EBB）；本次读取故障验收通过，长期矩阵不结B6/C8。当前任务/部署边界见[专项](../../project-workflow/bridge-macos/wifi-read-rf-20261012-plan.md)。

- **2026-10-11 BOOT/GATT、计划恢复与OTA统一实现已部署**：Note4 .81/配套Bridge/实际手动BLE模板已部署，基线f822531后的统一实现随.82电量平滑按用户授权收尾提交。用户六次按键符合预期；计划3490经USB工具复位恢复同ID/原剩余期限并直接HTTP。367项软件测试通过、3条件忽略，匹配ELF66项栈检查通过；同名模板合同失效/发布后认证回读已修正。USB工具复位后认证HTTP延迟/超时及旧.78停滞根因仍未知；电池Sleep重启、RF/掉电/功耗完整矩阵未验，不结B6/C8。电量抖动研究后已按用户授权实施.82：5秒采样、显示专用90/10 EMA、原始保护/诊断独立，12定向回归及实机/用户按键验收通过，见[电量记录](../../project-workflow/bridge-macos/note4-battery-smoothing-20261011.md)。详见[执行与证据](../../project-workflow/bridge-macos/note4-boot-gatt-followup-20261011.md)及PROGRESS。

- 2026-10-08 新协议空闲重复activate实现bug已在同源Fake复现并修正，Note4/默认Bridge已协调升级，.71运行hash与原OTA单次image_verified核对。软件闭环与剩余实体边界见PROGRESS顶节、A9及[周期activate计划](../../project-workflow/bridge-macos/wifi-idle-activate-20261008-plan.md)；书桌屏暂停后台连接、未刷ROM，不以此结B6。
- **2026-10-08容量与误扣预算修复已部署、实机恢复通过。** Fake已复现32条已确认历史永久占队列及未发激活扣完预算；现已安全回收确认/已ACK终态、保护未知提交/待ACK与序号围栏，容量错误明确返回、首次实际发送才扣预算，33项相关测试通过。Bridge SHA38B7CE11…已部署，正常应用服务重试home/v8，w3_4a完成/回执退役；Note4 .69认证HTTP已连wd21-la/.177/attempts0，无需刷ROM。诊断/修复见`project-workflow/bridge-macos/wifi-activate-fake-20261008-plan.md`与PROGRESS。剩余：长期多轮RF/电池与Mac部署验收；Fake通用状态硬编码wifi_connected/http.ready，须补RF/HTTP及完整main生命周期适配；无实际HTTP工作仍重复启用路径待独立复现。

- **2026-10-07 后续实体验收**：Bridge连续失败预算生命周期/耗尽反馈/显式重试已修复并部署，后台及主动认证HTTP实机归零、原事务保留，移交PROGRESS；.68 INFO重复释放修复与.69运行SHA亦已验证。长期电池/RF多轮、Mac原生UI/部署仍待验。Windows跳过pyserial_update_*不足以保现场，serialwin32仍SetCommState，补不写DCB的原生监听及boot对照；不以USB reset11当panic。

- **2026-10-06 统一Wi-Fi权限已实现并Bridge OTA部署.68**：有效session+claim/lease owner统一管理全部WFS4来源记录，来源/密码、principal/namespace事务回执及高水位保留；总12条，旧同ID明确拒绝。Windows修复新OTA被旧重试上限阻挡，以及等待镜像确认反过来阻挡Wi-Fi前置恢复；原eaadc30f仅上传1次并image_verified，.68运行SHA与ROM一致。实机完整snapshot r13三条（含两条Mac来源）全部writable/password_cached，软件跨桥读写/启用/删除/拒绝/缓存回归通过。Mac app/WK、真实跨桥改删及长期RF矩阵待验；旧同ID显式逐记录修复入口待补，不静默迁移。见shared-wifi-20261006-plan.md及PROGRESS。

- **2026-10-06 owner/BLE-first/完整密码缓存已修复部署；实体剩余矩阵待验**：最终Note4 .67/ROM ADC9CF4F…F69BBC1运行镜像认证匹配；Windows Bridge4A8D2FB8…FAC58D7部署。owner RTC原截止、显式BLE claim、无自主旧Wi-Fi、真实host映射单行保存启用闭环通过；设备已在wd21-la/192.168.3.177/认证HTTP正常。有效session+owner完整读其他绑定凭据、源记录仍只读，HTTP每页nonce及失owner分页拒绝；最后revision10三条密码全部缓存。生产store/失败合同、core配网/HTTPpager、app68/1ignored、生产Fake权限分页及匹配ELF栈通过。仍需真实电池反复深睡累计租约/无Bridge睡眠/BOOT/RF连续联系、掉电/EPD与macOS本地部署验收；不结B6。最终现场/证据见PROGRESS最新节及owner-expiry-20261006-plan.md。

- **2026-10-06 owner原缺陷记录（已由上条修复）**：ownerGet只用millis秒差检查租约，同boot失联可过期；ownerBegin恢复NVS旧uptime并把超当前uptime的lastSeen置为当前，普通重启/深睡唤醒可重新获得近300s，不证明旧桥存活，违反用户要求的真实过期语义。不得把该恢复宽限当合理永久占用：应保留原截止并计入睡眠，无法证明仍有效的恢复状态不可重新签发完整租约；到期只清空，后续显式claim，不自动转移。生产回归已通过，剩余真实深睡/跨平台矩阵见上条；.63为修复前版本。

- **2026-10-06 BLE-first修复已部署，实体矩阵仍待验**：普通启动与USB断网不再自主连接旧网络/扫描，统一有界BLE会合；setup关闭框架autoReconnect，保留显式认证配网save/activate/回滚。Bridge使用实际host SSID并冻结复核，缺映射/凭据/变化/超时返回明确错误。Note4 .63最终ROM 130E0CF6…57559C1与Windows默认Bridge 8E4ED842…FB75D010已构建并应用槽USB刷写校验；app67/Fake6/生产C++期限/调度guard/真实ELF栈通过。.62启动验收暴露提前UDP初始化panic，已移除并重建.63；.63原始串口38s/三次status正常、WIFI OFF、绑定BLE连接及会合到期关闭实证通过。本机Plan被现有owner拒绝，未强制接管。Windows已有home映射wd21-wo→wd21-la/revision1，当前MCP=needs_sync（不是no_mapping，不证明设备缺配置）；Mac部署与B网络认证HTTP闭环、电池无Bridge睡眠/BOOT/RF矩阵尚待验；不结B6。证据以PROGRESS顶节和ble-first-contact-20261006-plan.md为准。

- **2026-10-06 Wi-Fi/BLE调度确认待修**：普通启动存在loop前30s active等待+15s legacy扫描+30s legacy连接；断网30s后主动关BLE，USB常驻每60s重试同wifiSsid且不开放BLE会合，WFS备用项未被legacy fallback扫描。用户纠正后的合同：普通自动联系只开有界BLE会合，无Bridge就睡眠；退役绕过会合的普通启动connectBest及自主Wi-Fi重试，不新增设备独立选网策略。Wi-Fi只能由合法Bridge工作/PowerPlan准入触发，保留namespace/claim/固定窗口/PowerPlan期限，物理维护及USB常驻策略单独处理。需生产调度失败回归与USB/电池A→B实机证据；当前只源码复核，见诊断文档与PROGRESS顶节。

- **2026-10-06 A→B现场待核对**：用户报告双网络设备离开A后在B联系不上Bridge/见Wi-Fi ON。Windows新版Bridge已重建临时启动，但本机targets为空/no_mapping，HTTP旧登记超时、多轮BLE无Note4；不能将Mac已保存方案当成本机已保存。补实机BOOT窗口下SSID/IP/供电/图标闪烁与失败阶段证据，验证host_ssid映射→合法会合启用→认证HTTP闭环，并退役违背BLE-only普通联系合同的自主Wi-Fi旧路径（不新增设备独立多网络策略）。源码与宿主兼容回归、备份/运行hash见PROGRESS最新节及project-workflow/bridge-macos/note4-wifi-diagnosis-20261006.md，RF验收未完成。

- **双 ROM 首次配网设计合并（2026-10-06）**：保留 `project-workflow/ble-wifi-provisioning/design.md` / `plan.md` / `task.md` 的 2026-09-30 原始设计；B5 已用于 macOS 支持，本需求统一归 B6。剩余包括首次无网络的 ROM 内置配网页（复用双尺寸现行 status bar）、新绑定屏幕码交互与 Windows 系统配对入口；已有绑定直接复用。Note4 后续设备页 Wi-Fi 管理、BOOT 恢复与统一协议已经实施部署，不再标记全部未实现。旧稿的三槽 NVS、独立 wifi_setup 命令、只允许物理窗口写入及保存/试连顺序，不覆盖本节现行变长仓库、HTTP/BLE 共用事务、claim/lease、USB continuous 与后续用户修订。Fake/双尺寸画面对照、新配对/错误密码/掉电恢复及跨平台/1.54 实机矩阵仍待验；构建遵守 AGENTS.md 当前目标授权，不因旧计划自动扩大 154g 授权。

- **2026-10-05任务完成后排队提示残留已修复部署**：真实read39a7a49ab08b980c已完成，底部初始accepted未参与刷新；生产HTML red→green，同job_id提示跟随进度/成功/失败，静态后续反馈清除旧关联，迟到ack也经view校正。阶段中文化，Node/WK52通过、窄宽成功/失败截图已审阅；默认Bridge664B86AF…27F06F5/2756/health204，原任务/身份/data保留，未改ROM或另建实机任务。WK合成IPC不代替RF验收，B6仍开放；证据/备份见PROGRESS顶节及wifi-tabs-plan.md。

- **2026-10-05新事务旧缓存冲突反馈已修复部署**：用户取消a7efde70624b7097后，认证read取得revision3、新sync83cecdbef6a0e7c6以3提交4/durable_receipt，真实WFS4迁移成功。App发送前revision冲突明确conflict/revision_conflict、停止自动调度，不apply、不自动覆盖缓存；UI提示重新读取并保留Bridge编辑。Fake补真实生产wifi_storage状态/修订序号，新增App预检、设备拒绝和UI完整失败链路red→green；262 passed/1ignored、Node及WK48通过，窄宽截图已审阅。默认Bridge B3AC01D2…F6C7D9/38549/health204，身份/数据/成功任务保留；AGENTS.md写入任何请求包含失败必须明确返回。ROM仍.61未重刷，无另建实机任务；旧IDretired不证明旧请求成功，完整RF/电池/BOOT/命名实例/掉电容量仍开放。最新证据及备份见PROGRESS顶节/执行文档。

- **2026-10-05 USB常驻发送预算及HTTP读回信封修复已部署**：continuous/sent-query不再引用旧sync0截止；明确零发送与部分发送超时区分，避免新任务误标远端可能生效；HTTP setup只读关联ID补齐，写事务ID不变。三个生产seam red→green，受影响260 passed/1ignored。默认Bridge D6DC32B2…13FA2AD/35567，实机USB/BLE OFF现场与重启后原ID认证HTTP查询200已验，错误从window_budget_exhausted转为receipt_unknown。当前a7efde70624b7097/w3_0000000000000010仍awaiting_confirmation，slot0目标未匹配；用户未选择取消重建，保留原任务。绑定read/write有效，.61 ROM未重刷；软件修复不代表当前同步已完成或B6全部实体验收闭环。证据见执行文档/PROGRESS最新节。


- **2026-10-04 Astra medium一次审核已完成，设计已修订/待实现**：[审核](../../project-workflow/bridge-macos/wifi-unified-protocol-review.md)。补peer+principal token双证明、旧namespace不共享授权；激活pending/active/完成回执原子边界，RAM trial重启无终态如实未知；去掉BLE连接断开取消普通任务，恢复旧网同期限；区分8身份关联、3 namespace/12配置、8回执。用户已选择BOOT严格权限：只提供合法绑定/身份修复，保存/试连/启用先claim、他人owner拒绝。设计第3/4/7/8节已收口，代码与实体矩阵未实现，不结B6。

- **2026-10-05统一协议已配套部署，原任务实体闭环待系统权限**：U0 WFS4高水位/确认退役、U1–U5共享身份/命令、HTTP优先、BLE适配与在线通知/UI已实施；最终275 Rust/Fake、C++ sanitizer、JS、WK46场景和真实Note4 ELF栈门禁通过。最终默认Bridge B65A64FE…F24D60及Note4 .61已部署，OTA a9811cd3 succeeded/image_verified、独立运行hash匹配578EEDE7…703E3F4。原f287ac24cd546159保留ID/sent=false，合法绑定修复实际受bluetooth_permission_required阻挡，已请求用户对最终app授权；原revision1→设备3只做完整冻结意图认证核对，不强制覆盖。详见[wifi-unified-protocol-execution.md](../../project-workflow/bridge-macos/wifi-unified-protocol-execution.md)。USB+BLE OFF/light0+Bridge重启原任务闭环及完整RF/电池/命名实例/掉电容量矩阵仍待实机验收，B6不结项。
- **2026-10-04统一协议设计依据**：[设计稿](../../project-workflow/bridge-macos/wifi-unified-protocol-design.md)。USB插入/BOOT/Bridge主动激活共用DeviceStatus、在线事件和倒计时；认证/api/status区分continuous/window及通道ready，USB正式light剩余0不误挡。HTTP优先直接管理，BLE回退；长期principal关联、owner/即时可用性替代BLE临时permit前置，跨通道事务/回执恢复。定向通知覆盖命名实例，丢通知/Bridge重启需补查；深睡无接收机会不伪造远程在线。现行HTTP OTA已实机证明保留。按U1–U6实施并验USB/BOOT/主动激活/Bridge重启/丢ACK/电池期限/身份迁移/资源矩阵；本轮仅文档，无软件或ROM部署，不标记B6完成。

- **2026-10-04 USB常驻写许可缺口已定位，待修复**：plugged=true/deep_usb0、BLE交接后关闭，HTTP正式Plan仍更新；Bridge短时permit仅BLE签发/缓存，重启或过期后HTTP机会无法补取。任务f287ac24cd546159持续window_required/sent=false（22:02:54第14次），等待自然会合无法保证USB常驻下前进。需补USB常驻获取有效绑定写许可与重启/过期/同窗回归，不能用拔USB/改deep_usb替代正常流程，不能绕过bond/owner/窗口期限；现场与源码链见PROGRESS最新节和wifi-task-time-usb-status.json。

- **2026-10-04任务可观察性补充已部署**：当前Wi-Fi任务和执行记录显示完整创建时间，JS/WK42/窄宽截图通过。真实同步f287ac24cd546159创建21:48:32，仍window_required且sent=false；可达HTTP没有证明有效写许可，许可取得/实际同步完成仍需现场确认。本次只读核查与UI展示未取消/重发任务或改网，证据见PROGRESS最新节。

- **2026-10-04同步来源独立修复已部署**：修正来源writable禁用回传checkbox及同步后同ID来源混入比较/再次导入的设计偏差；导入Bridge配置独立编辑/保存，目标本机版本用于后续比较。新增设备→Bridge→编辑保存→设备完整点击与Rust服务往返red→green，覆盖本机/其他来源和确认后再次导入；core120/app66（1ignored）/JS/WK42与生产C++管理内存检查通过，窄宽截图已审阅。部署数据rev13/两目标/快照/基线保留，无需重导；WK合成IPC与Rust stub不替代真实设备端到端/RF验收。证据与备份见PROGRESS最新节，B6仍开放。

- **2026-10-04导入编辑/一步保存修复已部署**：用户澄清导入后即为Bridge可编辑配置，保留原标识；源设备bond/writable仅管设备操作，不延续为本地只读。编辑表单一次手动正式保存，取消加入草稿和外部按钮，输入不自动保存；按钮旁显示保存进度/结果，失败保修改。旧origin在显式保存时规范为local。生产HTML VM/WK取得red→green，补设备只读导入真实Rust import/save/restart与缺permit拒绝回归；checkbox原生尺寸/文字对齐另有WK几何red→green。部署状态见PROGRESS最新节；不代表真实改网/RF矩阵通过。


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

### C9. 取消项目自管A/B、动态容量与内置更新界面

- [ ] **AP01 正式 ROM Wi-Fi 选择菜单（2026-10-22，用户要求转待办）**：在设置的“屏幕保护”后列出已保存 SSID，当前网络沿用原厂勾号，焦点与勾号独立，首尾循环。现有代码仅为阶段性实现，未发布、未刷机；设备仍为应用06／救援0。继续时先解决原生模拟12后段导航断言，完成真实截图/长SSID显示、原厂菜单返回/重建回归、原生切网与持久化故障/重启恢复、资源预算及可复现构建验收；再按授权安排实机RF/回退/身份保持验证。原厂更新标记行为保持不变。当前宿主ASan/UBSan核心测试通过，不能替代原生及实机验收。[交接与证据](../../project-workflow/ap01-wifi-profiles/status.md)。

- **2026-10-20 AP01共享宿主共存/热重建离线通过，未部署**：八模板+独立两页，跨应用焦点/发布及两次完整九页重建（含动画中途）、服务连续性、旧帧/revision拒绝通过；修正删除绑定、NULL定时器保护及旧窗口订阅。普通候选03原生回归/六场景像素/核心ASan通过；34设备未动，不关闭C9物理及精确OTA/资源边界。[专项](../../project-workflow/ap01-screen-host/status.md)。

- **2026-10-20 AP01页面恢复34已部署并单页实机通过**：先启动/认证0031救援槽0，再单次OTA写应用槽1，build cc18eeb5…/6995768 B运行一致，救援未写。原厂页不抢焦点并反馈waiting_for_template_page；模板页保持旋钮不动发布后自动恢复，用户确认“自动返回，额度正常”。同内容v9最终Bundle6a8d702a/commit13、Data23/displayed；所有Profile保持。原生136页面恢复、137–139 OTA/同NOR/无稳定记录回切、12组像素零差异及匹配宿主ASan/UBSan通过；多页仍为模拟证据，Bridge未部署/WK未验，不关闭C9。[实现与证据](../../project-workflow/ap01-template-integration/bundle-page-restore.md)。

- **2026-10-20 AP01布局v9 Bundle已部署**：首栏订阅/账号/时间、底部SYNC/RC合栏，完整Montserrat14、CJK20及额度64/60px；修复字体行高大于region造成上沿裁剪。九场景/完整像素/规范化CRC通过，Bundle3f4850d5/commit10、Data19 applied、最终显示回执pending；ROM与boot_id未变，其他设备Profile不变，不关闭C9实屏/长期边界。[部署与证据](../../project-workflow/ap01-template-integration/bundle-refresh.md)。

- **2026-10-20 AP01字体离线核查**：原厂0041中找到完整ASCII Montserrat14（含Y），67个原生fmt_txt描述符中旧55项清单漏12项；整套字形/映射/字距比对一致。页面子集裁剪与恢复遗漏同时存在；未改设备、ROM、Bundle或已有预览，详见[核查证据](../../project-workflow/ap01-template-integration/font-rom-audit.md)。

- **2026-10-20 AP01导航33已部署并实机通过**：Pet边识别替代数字递减判环回，修正Pet→菜单/菜单→主界面重复插入应用和退出方向；用户确认“页序和动画均正常”。槽1/build967049b5…，031救援槽0保留且本轮实机回切已验；原v5 Bundle保留，Data seq12已应用、末次状态pending。130/133/134原生OTA/同NOR恢复/无稳定记录回切/原回执及12组像素、匹配宿主ASan/UBSan通过。导航项结案，C9其他验收边界仍开放。[证据与交接](../../project-workflow/ap01-template-integration/release-0041-20261020.md)。

- **2026-10-20 0031基础ROM已实机部署**：单次原厂正常OTA写非活动槽0，保留当前正常原厂0031槽1；认证build53574b55…/BootROM0x11000/物理表age12一致，用户确认GIF/显示/旋钮正常，5次STATUS47–94ms及FLASH元数据/LOGS可读。临时服务关闭。独立OTA并发/掉电/高水位仍按各自证据边界，不追加刷写保留槽。[部署证据](../../project-workflow/ap01-base0031/status.md)。

- **2026-10-20 0031基础维护ROM离线修复验收完成**：消除第三恢复标记并加最终BIN门禁，非阻塞Flash围栏保证OTA占用时STATUS/FLASH忙反馈及LOGS可用；正式ROM原生TCP升级/读回/切槽、0031原厂恢复正反向、两条同NOR新槽启动通过，保留原厂GIF/功能、无应用。额外RAM8 B/持久0，未刷实机；物理并发时延/LCD/掉电/高水位未验。[产物与证据](../../project-workflow/ap01-base0031/status.md)。

- **2026-10-19 23:37 AP01原版0031已救回**：原生83验证原厂写入/拒签/关闭/恢复及槽0全区不变；实机唯一下载事务后主动恢复一次，相同MAC认证0031/在线，用户重新配网且界面/旋钮正常。旧0041槽0未作为写入目标，未物理dump。以后带应用ROM以移除OTA签名门禁的0041为基线，保留CRC/边界/鉴权及不依赖稳定确认的紧急更新通道；当前0031不再刷写。[证据](../../project-workflow/ap01-template-integration/flash-0041-20261019.md)。

- **2026-10-19 AP01恢复部署遇实机槽巡检拒绝**：旧候选25经认证确认为0041/build c8ff8f…运行槽0，用户确认界面/切页正常并7059旋钮绑定。Bridge C2无法读取旧C1；稳定确认明确409 slot_image_invalid，APS1与新ROM均未写。候选27将巡检改用原厂只读Flash叶，保留CRC/身份；C2 Data测试补请求ID。宿主ASan/UBSan及原生启动/8模板/完整OTA/健康锁回/中断恢复通过，最终回切恢复与严格放行结果见[实机续作](../../project-workflow/ap01-template-integration/flash-0041-20261019.md)。旧服务的升级前稳定门禁与巡检互锁；原厂0041仍保留未签名拒绝，不能当作已验证的替代通道。新候选未刷写、不伪造稳定记录；首启/掉电/高水位及独立应用更新仍开放。

- **2026-10-19 AP01应用代码独立更新缺口**：用户要求核对并记录。应用源码在`ports/ap01/`，可修改；`tools/link-ap01-native.py`生成XIP模块，`tools/build-ap01-rom.py`将其打包进完整0041 ROM，当前代码更新与整ROM发布耦合。完整Bundle更新模板/资源，不等于更新应用可执行代码。后续评估并定义独立应用更新边界/ABI、原子提交与恢复、认证/完整性、资源预算及宿主/原生验收；未实现前继续明确使用完整ROM更新，不记为“源码无法修改”，也不把原厂逆向仓库缺失当作本轮协议重构阻塞。不得以新增应用双槽违反单份应用/Bundle及保留原生固件双槽的既定约束。

- **2026-10-19 13:13 AP01自动启动安全窗口完成模拟交付**：独立OTA预置稳定槽、9003ms健康锁回、30秒超时/unknown/显式救援围栏；宿主ASan/UBSan、双PT模型防护及原生59–63通过。同升级NOR预置后中断→稳定0恢复绑定/Bundle/激活/业务与原回执，16组画面零像素差异。候选24完整待刷ROM6995352 B，SHA256 `a7d1962b5b7d3ef6f590857632f5d27ffbef5f8481039bf96b98cccdbe6b8194`，匹配ELF/包与严格放行41份证据见[交付](../../project-workflow/ap01-template-integration/release-0041-boot-safety.md)。未刷机/部署Bridge；保护从成功预置开始，物理Boot2/掉电/首启/LCD/free/高水位仍未验，C9不关闭。

- **2026-10-19 01:36 独立OTA稳定槽完成模拟**：维护18080加入slots/stable/switch/持久查询及响应后重启，与业务18081及owner/context/Bundle隔离。候选22/原生49、50、51验证有焦点模板时完整OTA、直接稳定槽回切、原槽重启回执；ff13重放只提交一次，两槽及保护区字节不变、三轮四场景零像素差异。修复实际LVGL绘制事件误入OTA输入分支的断言。新ROM6993160 B，SHA256 `63eb98e51c9cb1e0b2783d63b51c50da0dadb3db95b1ad107fcbb0fbb970df94`，未刷机/部署Bridge；物理与传统OTA旁路边界保留，C9不关闭。见[交付](../../project-workflow/ap01-template-integration/release-0041-ota-slots.md)。

- **2026-10-19 AP01完整0041 ROM模拟收尾**：原生维护/业务HTTP、VFS/Bundle/持久激活、发现/配对和原厂UI接线完成；候选16原生31/32/33验证八模板/四场景、完整ROM写非活动槽与保护区、同NOR新槽回执恢复。正式待刷ROM 6981976 B，SHA256 `6adb31dcaa0a35c77a38c729d298764f0c72ae7e729b6c77a21dd0b7beeb56bc`，配套Bridge隔离构建未部署。保留五GIF、AP01无PowerPlan；ASan/UBSan、core162/render15/v2_state21、存储8组合及Note4兼容/栈检查通过。**不关闭C9**：未刷机，物理首启/LCD-SPI/真实free和高水位、0031旧接收器跨升及macOS WK仍未验；QEMU显式模型限制见[完整交付](../../project-workflow/ap01-template-integration/release-0041.md)。下方早期“无ROM/接线待完成”保留历史语义。

- **2026-10-18 0041原生渲染续作**：画布/字体绑定/尺寸与裁剪状态迁为调用方持有对象；绑定及完整渲染核心11个RV32对象无可写静态段/隐式初始化段，原生String端口宿主4组画面像素差异0，33项共享回归通过。取得0041窗口/图片/事件小函数依据，不能代替原ROM UI运行。原生HTTP/VFS/Bundle/UI与完整ROM仍未完成、不刷机；旧Bridge候选未重建。[原生里程碑](../../project-workflow/ap01-template-integration/native-0041-progress.md)。

- **2026-10-18 独立HTTP与Bundle存储调查**：用户确认维护/应用管理18080与Codex业务18081分离，模拟业务停止后维护健康/配对仍可读，11检查/8截图；Bridge双端点发现/配对与隔离候选已配套，未部署。0041本身确认/resource总1.25MiB，8模板含三字体未压缩安装384575 B、同规格更新保守准入827039 B（8KiB额外占用模型），1/2/4/8×raw/CSZ1共8组合及满盘旧内容保护通过。实机free和LittleFS块开销仍未读；五份萌宠GIF为ROM内2153808 B，删除不直接释放/resource，当前不触发条件性整页删除。原生HTTP/VFS/statfs/隔离及ROM接线继续开放，先模拟不刷机。[调查/证据](../../project-workflow/ap01-template-integration/storage-http.md)。

- **用户最新纠正：新业务ROM基于0041制作。** `cuktech-ap01-0041`为当前接入目标；0031现场与既有审计仅保留历史含义，不得复用其地址或将其当新ROM基线。共享Bridge/模板/发现/配对逻辑继续，先模拟验证、暂不刷机。

- **2026-10-18 AP01 Bridge/实时SIM首轮已实现，原生ROM未完成**：常在线目标拒绝PowerPlan，UI/MCP共用主动多接口UDP/通告接收/邻居补查与认证端点更新；复用双屏四位码/旋钮绑定，先持久保存候选事务，未知结果查原ID。实际CompactFrame客户端→共享C++绑定/Bundle/激活/渲染→隔离NuttX UI，10检查/8截图、6组像素差异0，双模板切页/新context和重启恢复通过。Bridge候选已构建但未部署；原0031 HTTP/OTA/VFS/引擎RAM context/UI接线、真实存储和镜像资源审计仍未完成，**无新待刷ROM**。便携绑定核心RV32无可写静态段、绑定提交核心峰6816 B，不代表整机预算；多网卡/DHCP/权限及实机矩阵仍开放。见[实施与证据](../../project-workflow/ap01-template-integration/status.md)、[原生审计](../../project-workflow/ap01-template-integration/native-integration-review.md)，不刷机。

> 2026-10-18：AP01 逆向、ROM 与模拟证据已迁至同父目录 `cuktech-ap01-firmware` 仓库（报告在 `reverse/reports/ap01/`）。本仓库 C9 只保留 AP01 **对接**实现与验收；下列历史条目中的原 ap01 链接已改为对该仓库的引用。

- **2026-10-18 AP01彩色通用模板/页面功能接入已验（未部署，视觉精修已SIM复验）**：用户选择保留彩色圆环；新增显式320×240 palette4目标、共享C++ ring、`codex-status-a`变体。缺5h整组隐藏并居中，RC/账号按when，RC独立于5h（单窗口示例夹具漏项已修正）；原有条件切页插入Profile全部1–8项，后台不抢屏，弹窗/屏保/长按保留。共享core/render253通过、app串行105通过/3忽略（首次并行网络夹具断言失败，单独/串行重跑通过），隔离0041 NuttX真实X11 21检查/9截图，4次与同源引擎像素差异0。CTP1仍11333 B，单帧38400 B＋LVGL调色板64 B，缓存4096 B按需分配。现已补齐CSFN v3 alpha4、16色覆盖率圆环及排版，原尺寸前后截图已目视复核；帧缓冲不增，用户视觉确认与实机观感仍未验。真实AP01binding/HTTP/Bundle接收/持久激活、原ROM UI hook/实机资源与安装仍未验，C9不结项；无刷机/Bridge重启。接续[专项状态](../../project-workflow/ap01-template-integration/status.md)和[验收边界](../../project-workflow/ap01-template-integration/review.md)。

- **2026-10-17 AP01 0031 fallback 基座离线闭环完成（用户目标：ota_0 为永久固定 0031 fallback，ota_1 只跑 0041 开发）**：0031 原件 6,804,520 B/8a721fc8…；按 0031 反编译完成 20 个原厂绑定与独立认证 HTTP OTA 端口，候选 A `cca6a243…`、B `c909ae30…`（均 flashable=false）。147 健康 200/401/500→149 A→B 整包 6,804,520 B+原厂 ff13=0+payload 复位→150 独立核对 active1/age2/NOR `4b36b82a…`→151 同 NOR 复启 B 健康→152 B→A→153 核对 active0/age3/NOR `6b257d0d…`→154 复启 A 健康。边界：仅宿主 loopback；0031 scan 请求显式抑制（scan API 未派生）；cloud OTA 未改；不证明首刷/Boot2/掉电/物理 Wi-Fi。专项（ap01 仓库 reverse/reports/ap01/qemu-ota-0031-20261017.md）

- **2026-10-17 AP01 0031 fallback 安全回退最小闭环完成（QEMU）**：安全角色镜像（无 HTTP，ota-safety-dev3-0031.bin，sha b9cc6ed9…）经 fallback HTTP 装入 ota_1；开机 ff13 预切 E000→active0，崩溃窗 169 同 NOR 复启 fallback 200/401/500 通过（170）；9 s 后直接重写 E000 表为 active1/age+1 锁回 ota_1，锁定窗 168 同 NOR 复启 dev3 正常（171）；开发镜像运行期 socket 调用为 0。实现复用 0031 RAM 原生表写/CRC 叶（0x62fc8cd8、0x62fc168e、0x62fc1c5e/0x62fc159e）。旋钮强制回退无 Boot2 证据未验证；仅 QEMU/loopback，不刷实机；RAM 叶依赖固件版本。专项（ap01 仓库 reverse/reports/ap01/safety-return-20261017.md）

- **AP01下一步（2026-10-17用户要求看到真实界面）**：从QEMU显示交接（ap01 仓库 reverse/reports/ap01/qemu-display-handoff-20261017.md）接续，先确认framebuffer是否更新，再查fbdev/LCD任务/SPI/DMA/TE链；实际LCD仍Logo，根因尚未定位。保留127–132已验证OTA模拟往返，不刷实机、不用宿主SIM替代。不重复扩展米家协议作为前置。

- **2026-10-17 AP01 QEMU独立OTA模拟闭环完成**：原件125持续观察50宿主秒/8628虚拟ms，无guest异常，原生MiIO/LVGL回调执行（LCD仍Logo）。基于授权unsigned候选注入6132 B模块，127 A→B实际HTTP整包/原厂CRC/ff13提交/payload复位、128独立槽/镜像/保护区核对、129 B从同NOR虚拟复启后OTA、130 B→A回刷、131核对、132 A复启后OTA均通过。最终引擎7246b77a…18b9d；HTTP42、DMA6/PWM3/NOR三profile、扫描13/验证器20通过。只证明显式模型：网络为payload专属loopback，Boot2为虚拟选择；不证明031首刷/物理Wi-Fi/救砖或任意后续Patch稳定。全部候选flashable=false，未刷实机/改Bridge，C9整体仍开放。验收/指纹/复现（ap01 仓库 reverse/reports/ap01/qemu-ota-final-20261017.md）。

- **2026-10-17 AP01 RTC/显示Mock续作**：0041已在QEMU显示原厂CUKTECH开机Logo（`lcd-icount-paced-24.json`/`.json.png`、76800 pixels），主UI入口未到。RTC/CLIC/WFI/P/UART/SPI/DMA/TE、WDG/双TIMER、显式PEC与合成NOR native检查通过；保留icount shift2/sleepON作为明确计时假设，sleepOFF复位对照单列。现停板级RF访问20001220（`aon-cap-25.json`），继续必要Mock，C9开放。031现有ROM无备份途径，不刷设备/改Bridge。证据/复现/边界（ap01 仓库 reverse/reports/ap01/qemu-rtc-display-20261017.md）。旧实例仅历史快照（ap01 仓库 reverse/reports/ap01/qemu-mock-runtime-20261017.md）。

- **2026-10-17 AP01 Patch/QEMU路线**：用户选择保留原厂后台＋注入App，完整MiIO/MiBLE恢复不再是接入前置。真实QEMU10.1.0最小machine/INSB/有限启动模型已运行0041至RAM中的OTP触发写，18项回归通过；入口12 B范围Patch＋44 B追加代码的非零寄存器/返回对照通过。整ROM/NuttX/UI未启动，候选仅QEMU用不可安装。下一步OTP只读事务及显式fixture，再按轨迹补计时/中断/Flash等。设备无已知救砖入口，继续不刷写；C9开放。复现/证据/边界（ap01 仓库 reverse/reports/ap01/qemu-0041-runtime-20261017.md）。

- **2026-10-17 AP01 0041完整代码逆向进行中**：REA＋GPT-6.1 Sol使用已修复私有模型，387检查/286执行通过；MiIO全部162已知根完成局部实现审阅，90命令描述符关联合同，新增6份路由及9份MiBLE传输/认证合同。当前同profile315函数、2529直接边，537个未采集直接callee/161处未定向调用、原0041浮点ABI及运行闭包仍开放，不宣称完整源码。官方SDK固定41bf338a，确认E907/MHINT；未刷实机/改Bridge。接手读0041交接（ap01 仓库 reverse/reports/ap01/0041-handoff.md）、本轮上层恢复/验收（ap01 仓库 reverse/reports/ap01/upper-layer-0041-20261017.md）；模型复现见私有模型（ap01 仓库 reverse/reports/ap01/private-profile-0041-20261017.md）。

- **2026-10-17 0041应用SIM子任务完成**：GPT-6.1 Sol逐项逆向并补齐33构造器、7组额外业务、25服务契约；原厂布局/55字体/指南/连接态/旋钮/屏保/绑定/双目标OTA与恢复、持久事务和错误恢复均用本地模拟数据闭环。最终502步/99截图、WSLg30步/15截图、服务147项ASan/UBSan及81772天农历校验通过，同一ELF/源码/配置/报告哈希归档。数值空洞仍标未观察；不声称真实云端、整ROM或硬件验收。未刷实机/改Bridge；C9维护集成仍开放。验收、矩阵与产物（ap01 仓库 reverse/reports/ap01/application-rehost-complete-0041-20261017.md）。

- **2026-10-16 0041第四批端口与详情**：GPT-6.1 Sol恢复C1/C2离线四卡、Power六项/模式/端口子菜单、日期周行跨月年、天气五档长页、米家QR/引导入口，32套ROM字体、219步真实X11及字体校验通过，WSLg已起。农历/连接态专属UI/实操/真实服务/在线跳页仍缺，接口动作明确不可用；无刷机/OTA候选变更。实现/指纹/验收边界（ap01 仓库 reverse/reports/ap01/application-rehost-0041-round4-20261016.md），C9开放。

- **2026-10-16 0041第三批照片校准**：时间仪表/空图、天气离线状态、日历原厂小字体和时间设置勾选已接入，118步真实X11/数字逐像素误差0/字体校验通过，WSLg已验证运行；F可切换照片对照场景。改用GPT-6.1 Sol完成复杂逆向/实现/审阅，不再使用Luna。仍缺C1/C2及多页详情/实操/真实服务，未刷实机。第三批实现、指纹及边界（ap01 仓库 reverse/reports/ap01/application-rehost-0041-round3-20261016.md），C9开放。

- **2026-10-16 0041应用重建第二批**：原厂10套字体、设置顺序/详情、日历天气布局、时间制式、5萌宠GIF、指南前两页已接入SIM，最终108步真实X11及字体独立校验通过，WSLg已起。首批设置错误映射已更正；C1/C2及功率/日期天气详情、实操指南和在线服务仍缺，未刷实机。指纹/证据/缺口见第二批验收（ap01 仓库 reverse/reports/ap01/application-rehost-0041-round2-20261016.md），C9开放。

- **2026-10-16 0041应用重建首批已运行/OTA新增门槛离线修改**：按用户切换0041基线，原厂数字/天气图标/萌宠GIF、八页导航/七项设置/详情返回/SIM亮度，82步真实X11与截图通过，WSLg已启动。设置端点部分为跨版本映射，全部原厂布局与服务仍未移植。新增unsigned拒绝仅改2字节分支+4字节恢复CRC，离线候选与8项回归（含实际RV32分支执行）通过；未验证首次安装或完整原厂ROM启动，未刷写。实现/产物/复现/剩余缺口（ap01 仓库 reverse/reports/ap01/application-rehost-0041-20261016.md）。C9整体开放。

- **2026-10-16 路线调整及硬件模型阶段结果**：按用户“系统用开源NuttX、只逆向应用”调整后续主线为应用重建（ap01 仓库 reverse/reports/ap01/application-rehost-plan-20261016.md）。已实现部分CSR/扩展指令/寄存器及RAM别名模型，11测试通过；原厂回放180961条尝试、两块RAM搬运逐字节匹配，当前停eFuse影子访问0x20056000，未完整启动。模型/证据/限制（ap01 仓库 reverse/reports/ap01/full-rom-runtime-20261016.md）。当时原厂应用重建未实现；后续0041首批结果见上条，仍不能标为完整ROM运行。

- **2026-10-16 原厂整ROM启动仍未完成**：用户要求完整运行原厂0031；复用Unicorn2.1.4原样执行入口，观察7条指令尝试，最后0xa0000018读CSR0x7c5后异常2，SP0x62fdb890，未进入NuttX/UI，无补丁/stub/设备访问。需AP01 CPU扩展及SoC/外设模型，SIM不能直接承载原厂BIN。实际证据、复现及缺口（ap01 仓库 reverse/reports/ap01/full-rom-runtime-20261016.md）。

- **2026-10-16 SIM与限定应用逆向**：用户新增授权非等价开源系统模拟及subagent应用逆向。NuttX12.10.0/LVGL9.2.1在WSL2编译运行，320×240与方向键/Enter/Space/Esc真实X11验证通过，WSLg窗口已启动；只演示导航/确认，不含原厂业务或实机移植。环境/使用/证据（ap01 仓库 reverse/reports/ap01/nuttx-sim-20261016.md）、5条REA应用证据（ap01 仓库 reverse/reports/ap01/ui-reverse-20261016.md）。此授权覆盖本次结构/入口/编码器/页面的限定分析，不修改原厂ROM或现场，C9整体仍开放。

- **2026-10-16 REA RISC-V adapter已实现**：同父目录`rea`的`codex/riscv-raw-adapter-6.5.0`分支工作区，RV32/RV64裸BIN、base/entry分离与真实CLI/MCP验收通过；旧ARM/x86真实回归通过。AP01完整镜像映射verified，但已知间接调用目标未自动恢复；Windows原生RV32/RV64真实CLI/MCP及110项回归亦通过（无WSL），见Windows验收（ap01 仓库 reverse/reports/ap01/rea-windows-riscv-20261016.md）；原始ABI/厂商指令质量仍不承诺。未提交/推送/全局安装，不改现场。实施与验收边界（ap01 仓库 reverse/reports/ap01/rea-riscv-implementation-20261016.md）。

- **2026-10-15 AP01维护核心已构建**：认证/流式事务/单份安装仲裁/未知结果查询/恢复记录核心通过宿主与RV32同组故障回归，代码3330 B、核心栈链256 B；产物和限制（ap01 仓库 reverse/reports/ap01/maintenance-build-20261015.md）。现有破解材料无原厂完整源码，缺独立任务/HTTP、持久认证、原厂共用排他、安全停机/安装/UI适配；无额外逆向、无新ROM/刷机，整项维护服务保持未完成，不以核心ELF代替完整ROM。

- **2026-10-15 AP01后续约束**：以同父目录已存在的0031破解包/源码/清单/SOP推进，不额外逆向。已核对看板opt包指纹及米家OTA安装器，未找到现成直接切槽入口；公开包须个人化后才能用于部署。包核对（ap01 仓库 reverse/reports/ap01/independent-maintenance-20261015.md#最新约束已有破解包核对）优先于此前逆向接续安排。

- **2026-10-15 AP01独立维护**：用户确认新增独立维护界面及更新服务、业务停机可修复、原厂米家OTA窗口逻辑保留。设计/定位（ap01 仓库 reverse/reports/ap01/independent-maintenance-20261015.md）及精确0031工具自检完成；1秒原厂回调是点号动画，不是OTA接收窗口。常驻任务/网络、认证、原厂排他及文件App安装仍待实施，无新ROM/刷机；不承诺解决底座启动失败或内存越界。

- **2026-10-14 AP01最新决策**：0031、无fallback、OTA与常规通信/绘图双进程；改用Wi-Fi双屏4位固定码＋旋钮确认，仅防误绑定，首版不要求严格加密/SMP。持久多对多、单owner、300秒租约/60秒续约，有效owner不弹新码。具体方案（ap01 仓库 reverse/reports/ap01/screen-binding-design-20261014.md）已完成；旋钮/UI/常驻HTTP、绑定存储及Bridge适配/实机待实施。原厂ATT固定拒绝的历史调查（ap01 仓库 reverse/reports/ap01/ble-binding-20261014.md）保留，不能标已绑定/可刷。

- **2026-10-11 源码实现与候选验证**：共享固件 Bundle 改为单份内容与短小回执，Bridge 使用动态容量；原生固件 OTA 双槽保留，覆盖全部共享固件目标。用户进一步明确允许临时下载文件，下载期间继续运行，完成后才停用内容并覆盖。
- 更新覆盖失败进入内置恢复，身份/配对/owner/认证重传保留。动态容量含下载、缓存与预留，旧设备仍使用其能力上限。资产发布模型移除 rollback manifest；AP01 独立应用尚无可运行实现，保持同一设计；逆向成果归本仓库 AP01 专项（ap01 仓库 reverse/reports/ap01/README.md）。
- **2026-10-13 AP01实现**：0031最小同步XIP App加载器/ABI1候选2已构建，宿主ASAN/UBSAN、RV32模拟12/12和独立终审通过，未刷机；实现/指纹（ap01 仓库 reverse/reports/ap01/app-implementation-20261013.md）。首刷前核实同机日志读取/UI任务余栈；接续完整Bridge/长期宿主、认证、文件App-only OTA与安装/恢复。0041仅对照；P0/P2–P5实机未执行，完整Bridge ROM未有。App/Bundle单份、原生双槽保留。恢复评估（ap01 仓库 reverse/reports/ap01/failure-recovery-20261013.md）：底座启动失败不开壳恢复未知，双槽/Type-C不构成保障。
- Note4 **0.18.80** 已完成统一CSZ1分块zlib并于16:27配套部署Bridge：复用ROM miniz、压缩正文按块读取、Bridge冻结传输字节，动态物理容量与原文处理上限分开。359项回归通过/3条件忽略，匹配ELF栈检查通过；OTA单次上传、运行镜像SHA256认证一致，旧模板迁移和Data显示正常，压缩安装实机峰值/掉电矩阵未验。证据、ROM哈希和实机待验见[实施交接](../../project-workflow/single-copy-updates/status.md)。AP01仅确定兼容格式，解压入口/实机仍待调查；C1 dense编码未实现，不以压缩代替其验收。

### C1. Bundle v3（manifest + 原始 dense 对象）
- 设计已定稿（`docs/history/workflow/bundle-v3/design.md`）：`CSB3` 容器 + `ct-dense-v1` 对象，`mini+quad` 从 55.3 KB 降到约 16.2 KB（省 ~71%）。
- **为什么重要**：1.54 是 8 MB flash，55 KB Bundle 的十六进制编码（45 KB 十六进制字符）是它最容易 OOM/超时的环节，也是 A2 的根因候选。
- 需改：固件（新格式解析 + 按C9单份安装/内置恢复 + 拒绝路径）、桥编译器（canonical dense 编码器）、Python 测试桥哈希三方同步；设备能力加 `bundle_format: 3`；v3 ROM 把 pre-v3 Bundle 视为不存在。C9共享实现已改为单份；现场部署仍以交接卡为准。

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

- 2026-10-19 R1–R5源码/软件验收完成，C2四端改动及删除清单见B6本日收敛条目；Windows/Note4随后已部署C2，1.54/AP01未部署；不将单次上线或短时观察记作长期RF验收。

- **2026-10-16隔夜断连/自动恢复修复已部署，授权后自然恢复通过**：昨晚服务发现未完成与11:40系统加密失败719都经4s超时进入隔离，休眠不是必要触发。11:58部署supervisor退避重建、持久化UI/MCP错误提示及发现失败回执，保留未知隔离；App107/BLE28/回放3/Node/WK68和supervisor红→绿通过。两项权限allowed后14:03自然wake2104完成Plan4050/认证HTTP/sync，独立认证MAC/.104/Bridge可达；正式Sleep4051后14:05自然wake2106再次会合。9旧Wi-Fi作业哈希不变，无BOOT/刷机/手动重启。真实隔离后自动child重建尚未现场触发，最初原生失败深层原因与长期矩阵仍开放。[证据/部署/边界](../../project-workflow/bridge-macos/sleep-resume-disconnect-20261016.md)。
- **2026-10-15 .104已正式部署**：用户授权后OTA d1199896一次succeeded/image_verified，独立完整hash/MAC/版本/ota_0一致，.103保留ota_1。SPI初始化自锁宿主red→green与实际ELF门禁通过；电池deep网络醒的实机复测仍开放，不以部署代替。[部署证据](../../project-workflow/bridge-macos/clock-stall-20261015.md)。
- **2026-10-15停钟自锁已复现/修复，.104候选未部署**：DEV_Module_Init先GPIO_Config再查spiReady，二次初始化拆SCK→spiStopBus重取本任务已持有mutex；.103/.104匹配ELF与生产/HAL red→green、100重入ASan/UBSan、分钟/模块/栈门禁通过。与15:36现场高度吻合，缺原CPU栈，实机候选验收仍开放。[根因/候选/边界](../../project-workflow/bridge-macos/clock-stall-20261015.md)。
- **2026-10-15新增停钟/失联现场**：用户报告停15:36且在Mac附近/电池供电；最后BLE会合15:34:59成功，随后目标不可见，17:27直连Host is down。USB无控制线监听/只读status、log无新回应，BOOT短按无变化。用户授权读取dump：23620B/CRC有效但ELF指纹不属.103，不能归因本次；工具复位已单记，未擦写Flash，根因仍待查。[调查](../../project-workflow/bridge-macos/clock-stall-20261015.md)。
- **2026-10-15独立分钟时钟.103已部署**：分钟/会合解耦及绝对唤醒截止，Fake ROM :58跨:00先red后green、69全套及固件门禁通过。14:48正式OTA后完整运行hash/版本/ota_1认证一致；用户14:49确认“时钟已及时更新，不做进一步复测”，停止追加验收。本轮未取得实机跨界秒级配对，不扩大为全部场景定量保证。[实施/部署](../../project-workflow/bridge-macos/independent-clock-20261015.md)。
- **2026-10-15分钟显示延迟实测**：在线light两次时钟渲染774/761ms，整分后≤3.13/2.73s完成，认证钟差[-2.14,-0.14]s；未复现用户此前10s，用户14:22:17目视更新与wake1188的16.581会合结束吻合；确认deep网络醒会合后才刷钟，启动/RTC分段仍待测。另见RTC短读后恢复、局刷回退全刷1362ms，原因未归并，未改ROM。[测量与边界](../../project-workflow/bridge-macos/clock-display-delay-20261015.md)。
- **2026-10-15统一预算修复已部署，14:00自然恢复通过**：四条BLE路径取消/分页错误、HTTP proof/claim、RPC及恢复GET已收口；310通过/3既有忽略。Bridge/data备份及health204通过，9作业不变。用户完成新版macOS权限授权后，timer wake1186完成Plan4011→认证HTTP proof/同步；独立认证HTTP ready/Bridge reachable，manual/provisional0、9旧作业哈希不变。两条ACK未知如实保留，由HTTP确认恢复；长期RF/原生连接矩阵仍开放。[实施与下一步](../../project-workflow/bridge-macos/timeout-budget-followup-20261015.md)。
- **2026-10-15 11:38存量网络恢复已部署/实机通过**：认证matches/active/version且无密码替换时不再创建activate，直接走既有Plan→network_start；历史未知事务与真正冲突写保护保留。两次生产ble_cycle及6类反例、300测试通过/3既有忽略；自然timer wake1074认证HTTP恢复，9条旧作业哈希不变，用户确认无BOOT联系成功。卡片误报待处理/任务ID/空状态亦修正并回归；后续在线窗口改为本地逐秒倒计时并部署，通信频率不变。原生连接独立超时与长期矩阵仍开放。[实施验收](../../project-workflow/bridge-macos/existing-network-reconnect-20261015.md)。
- **2026-10-20 Mac旧协议导致Bridge OFF已配套更新**：USB确认设备.105/C2，旧Mac签名exe实际编码C1；当前源码Mac已编译签名/受管部署，10:09自然BLE status/claim/Plan成功、10:10同MAC认证HTTP ready、10:11诊断同步/会合complete。旧app/data已备份，未刷ROM。HTTP读取超时、BOOT旧网回退及睡醒隔离后未自动恢复仍开放。[现场与边界](../../project-workflow/bridge-macos/bridge-off-sleep-20261020.md)。 后续[审查修复](../../project-workflow/bridge-macos/review-fixes-20261020.md)10:33已部署Mac：独立任务/native监督、共享有界清理锁序、本次boot HTTP事实/减少重复写盘、清账耗尽unknown完整校验归档及原ID只读重试；Core180/App119+追加3/WK74通过，同MACHTTP/Plan/health已核验。合盖苏醒及长期实体矩阵未结项。
- **2026-10-15 11:20再失联现场（11:38已恢复，见上）**：10:35后本地3条已取消/过期但提交未知作业阻止新activate，Wi-Fi恢复跳过network_start；11:09 wifi_match写失败时预算尚余3118ms，后续进入原生隔离。与700ms取消分开记账；不得清旧回执或未知旗绕过。[现场与证据](../../project-workflow/bridge-macos/bridge-off-recurrence-20261015.md)。
- **2026-10-15其他超时审查快照（后续实施见上）**：Fake慢写注入确认token1s、history500ms、Wi-Fi分页与自动恢复四条外层取消会留下previous_write_unknown；HTTP proof完整分页3s另有就绪误判。claim worker/RPC写阶段/恢复GET总期限存在静态缺口，分级与证据见[专项审查](../../project-workflow/bridge-macos/timeout-audit-20261015.md)。探针已移除，未改运行部署。
- **2026-10-15移回办公室自动恢复修复已部署**：移除Wi-Fi准备700ms外层取消，10s会合总预算下沉carrier，与单命令/RF期限取最小值；准备失败终止本轮，未知写围栏保留。慢写及丢ACK/未知写入口回归红→绿，298通过/3既有忽略；Bridge已部署，Note4不按BOOT自然醒1048恢复认证HTTP，timer/无manual/provisional核验。SyncOpen/network_start本轮ACK未知与HTTP proof超时保留真实失败，后续独立HTTP确认恢复；长期矩阵仍开放。[实施/验收](../../project-workflow/bridge-macos/return-office-fix-20261015.md)。
- **2026-10-12架构复核（关联B6）**：[报告](../../project-workflow/bridge-macos/architecture-review-redesign-20261012.md)确认4KiB环满淘汰、40KiB仅冻结批次，不提供两小时连续离线保证；提出本地持久化与上传解耦、限流摘要及资源/掉电验收。新增sync Meta按原生struct持久化的格式风险，需独立迁移验收。状态投影需区分观察、联系结果、调度健康和电源预期。仅静态审查，未修改生产代码或新增测试通过结论。
- **10月12日分类/关键日志：Bridge15:55已部署，固件仍候选**：未发送立即结束；写入未知禁止复用链路；整帧丢ACK可只读核验，已发事务仅原ID查询；201/认证原因不被普通断开覆盖，补启动/关联/IP/监听/ready ACK及明确预算原因，清空INFO初始化伪错误。Rust268/3条件忽略、Node/WK60、生产回调专项及Note4.85构建/64栈门禁通过；[实现与候选](../../project-workflow/bridge-macos/timeout-reporting-fix-20261012.md)。Bridge部分随[清理修复](../../project-workflow/bridge-macos/ble-connect-cleanup-fix-20261012.md)部署；固件0.18.85未部署，固件关键日志与完整RF矩阵/离线保留仍不结项。旧Wi-Fi管理宿主active_protected断言在HEAD对照也失败，需独立核对合同与用例，不能记为本次全套通过。
- **10月12日超时分类补证**：10:09/10:10分页ACK正常61–152ms、末页仅19/16ms被旧RF预算截断，属已证Bridge窗口错误；10:22 SyncOpen等3844ms未ACK仍为写后未知，不能同归短窗口。wake638两次先出现201/network_not_found后约15s失败，90s同步日志又混合总预算/无进展，需分别报告而非统一延长超时。[分类与证据](../../project-workflow/bridge-macos/wake-network-log-investigation-20261012.md)。
- **11:43手动Wi-Fi现场取证**：认证.83/wake675成功，6.6s IP/7.7s HTTP、真实light-sleep累计通过；同诊断代际，当前环最早1962，history654–675，旧wake632只剩归档摘要。另定位BLE初始化先refreshInfo后设provider制造空MAC预检错误；现场11条占902B（4KiB约22%），应修初始化順序减少伪错误，不能当作旧network_start故障根因。pcf8563短读现场有两条，影响未知。[直接取证](../../project-workflow/bridge-macos/wake-network-log-investigation-20261012.md)。
- **即时查询语义续项**：10:10查不到不能归为已覆盖；MCP诊断分栏读本机checkpoint/已归档JSON，HTTP失败阻断实时采集，直到10:28归档才见gap。需区分未采集/读取失败/旧缓存/确认gap并显示采集时间，禁止把无本机记录当设备无日志；19分钟uptime不是距10:09事件时间。[证据更正](../../project-workflow/bridge-macos/wake-network-log-investigation-20261012.md)。
- **原两小时保留目标续验**：旧sleep-modes设计120×约12B/每分钟1条；当前统一流wake_summary已72B且混入文本，4KiB不能沿用旧估算。10月12日单醒次重试样本2393B/567.1s，相同速率仅约16分钟、两小时约30KiB原始记录。需按实际写入率重验两小时目标并定义有界Flash离线保存/写入成本；40KiB现有冻结保留不等于连续日志落盘。[计算与证据](../../project-workflow/bridge-macos/wake-network-log-investigation-20261012.md)。
- **2026-10-12 离线日志/启动调查续项**：旧RF截止阻止network_start路径已由生产客户端差分回归复现，此前RF刷新已部署。首份恢复batch553明确报告404–1802共1399条overwritten；冻结前4KiB RTC环在HTTP失败时不能无损保留。发送前失败/写超时缺细分日志、network_start把确定未发也当unknown等待，以及会合取得IP缺阶段时间仍待修；需有界失败摘要保护和关键启动/IP/HTTP事件，见[调查](../../project-workflow/bridge-macos/wake-network-log-investigation-20261012.md)。10:09具体提前退出异常/约一分钟关联时延无足够历史证据，不以源码复现代替实机事实。
- **2026-10-11续项**：BOOT/GATT与跨重启PowerPlan、USB/Light直接HTTP、OTA独立镜像确认已随.81部署；发布后旧data_contract索引失效并认证回读已部署。软件与部分实机通过，认证HTTP间歇延迟、完整电池/RF/掉电矩阵仍开放；见[统一执行记录](../../project-workflow/bridge-macos/note4-boot-gatt-followup-20261011.md)，不因USB单轮通过结项。
- `sync-v1` 实施合同见 `project-workflow/device-sync-diagnostics/design.md`。当前默认 Bridge 与双设备现场见 `docs/history/progress-archive-2026-10-03.md` 相关历史节：Note4 `0.18.38-note4-b-bridge2` 的运行分区前缀 SHA256 已由认证接口与候选 ROM 精确匹配；1.54 `0.18.34-bw-ota1` 已做 USB 写入后逐字节回读，运行分区的独立认证哈希仍待验。双设备 BLE status/config/plan/open、OTA 上传与重启换槽等先前实机证据保留。2026-09-29 后台隔离双 Fake ROM + 候选 Bridge 虚拟 24h 达到各 97 个连续批次和第 15 轮；独立 light 入/离场、进程重启及同 seed 1h 回放烟测通过，详见 `docs/history/progress-archive-2026-10-03.md`。一次性 Wi-Fi 续传修正及设备页/MCP 对等版已部署默认实例。Fake ROM S01–S14、设备页 U01–U20 完整矩阵、F3 延迟页/同磁盘快照回放、页中途强退、真实窗口视觉及其他实机故障矩阵仍未完成；Flash 断电恢复和 1.54 运行分区哈希证据仍须过门槛。
- 当前设备协议作为唯一业务协议的一次性统一见 `project-workflow/device-sync-diagnostics/protocol-unification.md`：业务 HTTP 已收敛到 `/api/*`，旧 `protocol=2`、BLE `rv=2`/`ack="v2"` 与 MCP `_v2` 入口已移除。两台设备现已升级，临时旧 Bridge 已停、新版默认实例已恢复。1.54 的增量缓存复核已完成，结论与四次构建日志见 `docs/history/progress-archive-2026-10-03.md` 相关历史节；不并行运行两个 Bridge，也不保留长期双协议兼容。Bridge 本地 `state.json` 无损保留，不凭 wire 更名删除运行数据。
- 实现每 15 次深睡 BLE 会合的一次性 Wi-Fi 同步，以及正式 light 入口和退出时的 Wi-Fi 同步；成功同步才重置计数。普通 BLE 保持精简，诊断日志按冻结批次经 Wi-Fi 增量同步，未确认内容保留。
- 合并普通 `/log` 与跨深睡 `/history` 为统一、跨深睡、可分页/确认/报告 gap 的诊断流；OTA/Bundle 执行后以 Wi-Fi 报告分级结果并有限重试。设备页移除 Codex 余量和屏幕内容、模板/Profile 编排、字体管理编辑界面；保留只读实际 active/已安装/任务及设备级数据投递许可。模板 Tab 暂不增加对应界面或设计迁移布局，数据页排版不动。详情时间与缺失原因已收束，完整 U 矩阵仍待验。
- 设备页内容与信息层级见 `project-workflow/device-sync-diagnostics/device-page-content-contract.md`，MCP 读取对等要求见同目录 `device-page-mcp-contract.md`。共享投影、默认主视图和七个按 MAC 折叠栏目已部署；版本结论依各设备缓存与显式发布目录判断。时间显示按同目录 `device-page-time-plan.md` 收束并完成两 MAC、七栏目运行 MCP 核验：完整 Wi-Fi 回报共用一个时间，较新的 BLE 联系单列，PM 整块共用一次成功采样时间，任务和占用使用事件/观察时间；UI 与 MCP 同源。剩余：真实 Plan 发送时刻尚未单独持久化（现显示生成与 ACK）、PM 样本跨 boot 身份、U01–U20 完整矩阵、最终 Tauri 窗口视觉及折叠诊断产品验收；详情见 `docs/history/progress-archive-2026-10-03.md` 相关历史节。
- 以 Fake ROM 验证绝大多数协议和故障场景（S01–S14）；实机只验证 RF/GATT、ESP32 Wi-Fi 入网与 HTTP、light/deep 切换、RTC/Flash/bootloader 等模拟器不能证明的部分（H01–H03）。详见 `project-workflow/device-sync-diagnostics/plan.md`。

### C5. 字体资产增量发布
- **2026-10-11 中文资产候选（未部署）**：用户要求保留中文支持但ROM不内置中文字库。CSFN v2有界Unicode子集、UTF-8静态/动态模板度量与渲染、既有完整Bundle安装恢复路径及Bridge能力预检已接入；旧ASCII/v1保留。旧CN绘制代码/表源码保留，ROM排除两份CN表。协议见[字体合同](../font-asset-format.md)，实施/验证见[记录](../../project-workflow/bridge-macos/release-fonts-status.md)。**缺口：中文引擎未实机测试；真实EPD字形/对齐/缺字、典型/上限资产安装峰值RAM/栈、掉电/重启恢复、长期刷新均未验。1.54仍未开放Bundle字体准入（报告0），不因ROM可构建就声称支持中文发布。**
- 单一字体注册表与显式Profile字体选择经完整Bundle发布；历史Note4安装18px ExtraLight200见job `5d798bd9`/commit355。当前单份Bundle保留源与CT ABI2显式字体引用，设备单资产48KiB/最多8项；本轮中文扩展没有引入增量端点。
- `docs/history/workflow/note4-bridge-publish/protocol.md`的manifest增量传输仍为草案。未来若实现，遵循C9单份内容策略，另验设备动态容量/流式解析，不恢复当前/回退双清单。

### C6. 多 env / packages 目录隔离收尾
- **2026-10-11 发布矩阵与安装文档**：保留Note4/1.54两个ROM构建（串行、明确env、marker/版本/大小/SHA256/双槽容量），Windows x64与macOS arm64/x64 Bridge原生构建。标签触发draft Release，手动触发仅产物，必需目标失败阻止完整发布。缺口：尚未运行GitHub矩阵；macOS签名公证/干净主机安装未验；1.54最新候选虽本机构建通过，认证运行、OTA/USB/配网及发布闭环未验，不能用旧版本记录替代。克隆后Agent提示词和步骤见[构建安装](../build-install.md)，证据见[实施状态](../../project-workflow/bridge-macos/release-fonts-status.md)。
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
