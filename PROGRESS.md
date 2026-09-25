# Codex Status 项目进度（交接文档）

## 显示修复：时钟局刷窗口宽度算错（Note4 / 比例字体）：2026-09-25 — 已实现、已 OTA、判据命中

- **根因（几何，与残影无关）**：`src/template_engine.cpp` 的 `tplFontClockBox()` 比例字体分支按 **2 个数字 + 冒号** 预留窗口：`long adv = 2L * digitAdv + glyphs[':'].adv;`。而 `device.now` 的字符串是 **"HH:MM" = 4 个数字 + 冒号**。ntthin18 实测（`src/font_noto_ntthin18.h:124-134`）：`'0'..'9'` adv=160（10 px）、`':'` adv=64（4 px）→ 真实宽度 **44 px**，代码只算出 **26 px**。
  - 后果：窗口 `x=69..94`（byte 窗口 4 B = 32 px），blit 上限 `bufW=32`、文本 `xOff=5` → **只能画到第 27 px**；字形边界为 `0:69-78 / 4:79-88 / ::89-92 / 分钟十位:93-102 / 分钟个位:103-112` → 窗口局刷只更新前 ~2.5 个字，**其余像素保持上一次整帧渲染的旧笔画**，视觉上是"新前缀 + 旧后缀"的怪字形（现场被读成 `04:64`）。
  - 整帧渲染不受影响（按画布裁剪，不按 box），所以"全刷正常、只有局刷坏"；这也解释了为什么加 ghost 预算/强制全刷都治不了根。
  - **位置来源**在设备上是编译产物（`v2ActiveLoad()` → `clkComputeRectCt()` 读 `v2Ct.ops[]`），**尺寸**却是运行时重算（`tplFontClockBox`）→ 两处不同源正是缺陷土壤；若模板给时钟加 `rect`+`align:center/right`，同一个错误 textW 还会让**位置**也偏 18 px（当前 `codex-status-a` 无 rect，故只有尺寸错）。
- **改动**：
  - `src/template_engine.cpp`：`2L * digitAdv` → `4L * digitAdv`（窗口 26→46 px，byte 4→7，104→182 B；Note4 `CLK_MAX_BYTES=512` 足够）；新增 `tplFontPropWidth()` 与"上次 blit 被裁掉多少墨水像素"的 `tplFontClockClipped()`。
  - `src/template_engine.h`：上述两个声明的公开声明。
  - `src/main.cpp`：`clockTickWake()` 增加**窗口容纳校验**（`need > avail` 或 `clipped > 0` → 不发这次局刷，改走全刷并记日志），这类几何错误以后会自己暴露；新增 `clkBaselineAfterPanelWrite()`，在**每次成功渲染后**（全刷总是、区域局刷仅当覆盖时钟窗口）重新 `clkCaptureFromFramebuffer()`——此前 light 下只要发生过一次帧渲染，`clkPixels` 就会与驱动 shadow 失配、窗口局刷被拒并**永久退化**为每分钟整帧渲染。
- **构建产物**：`.pio/build/zectrix-note4-b/firmware.bin`，1,758,032 B，SHA256 `42AAF00B257908434E86CEA45838DC60FD65FE8FD426D0632E570BD6BDBCF72E`；`FW_VERSION=0.18.23-note4-b`；镜像头 `byte[3]=0x40`（16 MB/40 MHz）；`sdkconfig.defaults` 首行 `# TASMOTA__9244f2068d3cf08d` ✓。按用户决定**未复制进 `artifacts/`**（`artifacts/` 写入被沙箱拒绝、提权被用户否决），ROM 保留在 `.pio/build/`。
- **烧录**：经桥内建 MCP 先 `power_plan {mode:"light", mac:7C4FADB93408}`（plan 659 排队）让设备在会合窗口升 light，再 `firmware_ota {rom:…, device_ip:192.168.3.177, device_mac:7C4FADB93408}` → **`0.18.21-note4-b → 0.18.23-note4-b`**（1758032 B），槽位 `ota_1`、`wake=power-on`。
- **判据命中（实机）**：`[clk] reserved x=69..114 y=5..30 box=46x26 win=7x182B`（修复前 `x=69..94 box=26x26 win=4x104B`）；整分钟 `[clk] tick build=866us wake=54652us write=709175us total=764790us`，**无** "too narrow"/裁剪告警，`epd_busy_fails=0`、`clk_partials` 递增。
- **未做/后续**：(1) v2 会合路径仍无 ghost 预算（`CLK_GHOST_LIMIT` 只在 legacy pull 与 light tick 的 per-boot 计数上生效）；(2) 消除"位置来自编译产物、尺寸运行时重算"的分叉——把测量宽度写进 `CtOp` 需要同步改 Rust 编译器与 Python 测试桥的哈希，属独立任务。

## 桥侧修复：物理唤醒窗口取大 `max(plan, t_boot+300s)`：2026-09-25 — 已实现、已重启桥、待实机复核

- **现象（实机 0.18.21-note4-b）**：04:03:19 与 04:30:33 两次按钮唤醒都只维持约 55 s 就 `enter-deep`；而设备当时明确上报 `power.provisional_remaining_s=285`，桥的 plan 记录为 `{mode:"sleep", light_duration_s:0, reason:"rendezvous"}`。历史环证据：`04:03:19 boot(aux=3=EXT1) → 04:04:12 enter-deep(aux=60)`、`WAKE(light) dur=55292ms`。
- **根因（两处）**：
  1. `plan_for_rendezvous` 的 `want_light` 只含 `light_hold_until`/`push_dirty`/`in_flight`/`pending_activate`/未终态 job，**不含“本次是手动/BOOT 唤醒”**；`boot_plan(..., want_light=false, ...)` 直接短路 `plan_for(false, 0, "boot")` → sleep(0)。
  2. 设备在首个正式 plan 被接受后**不再上报** `provisional_remaining_s`（`src/main.cpp:2930` 的 `v2Provisional && !v2Plan.accepted()`；`src/v2_state.h:97-107` 的 `lightActive()` 也不含 provisional），所以只修“第一次应答”不够——后继会合仍会 sleep。
- **改动**：
  - `bridge/crates/core/src/coordinator.rs`：`PlanState.manual_until`（`#[serde(default)]`，随 `state.json` 持久化）+ `note_manual_window(t_boot)` / `manual_remaining(now)`；`boot_plan` 取 `duration.max(floor)`（floor = 剩余窗口，上限 `max_light_s`）；新增 `MIN_LIGHT_S` 取代散落的 `30`。
  - `bridge/crates/core/src/platform/service.rs`：`wake_reason=="manual"` 时按 `BOOT_PROVISIONAL_S - provisional_remaining_s` 反推 `t_boot` 并由桥记住窗口；`want_light = 待办 || 剩余窗口 ≥ MIN_LIGHT_S`；**无待办时按剩余窗口下发 light**（若仍发 `MAX_LIGHT_S`，每 60 s 一次会合会把窗口无限延长）；有待办仍用 `MAX_LIGHT_S`。显式入口（`explicit_plan`/`queue_explicit_light`）保持可提前结束。
  - 文档同步：`docs/generic-display-platform-design-v2.md` §7 新增“物理唤醒窗口是下界（取大）”段；`docs/power-state.md` §13.3 手动唤醒条目补同一语义。
- **测试**：隔离 `CARGO_TARGET_DIR` 下 `cargo test -p bridge-core` **84 项全绿**；新增 `coordinator::tests::physical_wake_window_is_a_floor_and_never_cut_short`、`platform::service::tests::physical_wake_window_survives_follow_up_rendezvous`。
- **部署**：新 exe 已重建并重启桥（PID 11664 + watchdog 10340，HTTP 8765 / MCP 8766）。重启后桥的取数源**自愈**：`usage changed (rev 1)`/`usage refreshed`（此前自本地 14:02 起该心跳完全消失，`want_light` 因此恒假，是本次缺陷的放大器）。
- **待实机复核**：按一次 ENTER → 桥应应答 light(≈285 s) 并在后续会合保持到 `t_boot+300s`（设备应维持约 5 min，而非 55 s）。
- **环境（沙箱写入边界；2026-09-25 查清并已修）**：
  - 机制：DSH 的 Windows 沙箱用受限令牌 + 随机**能力 SID**（`--write-sid S-1-4-…`，本机 `S-1-4-1018769461-493222538`）授权，shell 的写入**只认该 ACE**（宽泛组在受限令牌下不生效）→ “能不能写”是**逐对象 DACL**，不是路径策略。`write`/`edit`（fs 后端）只做路径包含检查、由宿主进程写入，所以**文本在工作区内始终可写**（`artifacts/` 里也能写文本），二进制只能靠 shell。
  - 症状与根因：`artifacts/`、`tools/` 的属主曾是 `喵的问都死\CodexSandboxOffline`（2026-09-09 14:47 由早期沙箱账号创建），DACL 是旧快照（10 条继承 vs 其他目录 14 条），**缺**那条能力 ACE；而该 ACE 是后加到仓库根的，可继承 ACE **不会回溯传播**到已存在的子对象，于是这两个目录一直没拿到 → 表现为“同目录里带 ACE 的文件可写，新建/修改这两个目录里的文件被拒”。
  - 修复（一次性操作，**未在仓库留脚本**）：在**提权**的 PowerShell 里对 `artifacts/`、`tools/` 各做两步 —— ① 用 `Acl.SetOwner(<当前用户 SID>)` 把属主改回 `喵的问都死\cogic`；② 用 .NET `SecurityIdentifier` + `Set-Acl` 追加规则 `S-1-4-1018769461-493222538:(OI)(CI)(W,D,DC)`。要点：**必须走 .NET**——`icacls /grant "*S-1-4-…"` 会因无法映射账户名报 `No mapping between account names and security IDs`；且只给**目录**加 ACE 就足以新建文件，**不要递归**（`artifacts/` 有 7.1 万文件，递归会被拖死/中断；递归只在需要“修改已存在文件”时才必要）。**已实测**：两目录属主已回到 `cogic`、沙箱内 `Set-Content`/`Remove-Item` 均成功。
  - 仍需一次性提权的场景只剩：shell 写**工作区外**、或写未带 ACE 的**已存在**文件（如 `bridge/target/debug/.fingerprint/…/lib-bridge_render` 会让 `cargo` 停在那里，提权重建一次即过）。新建目录请按 `AGENTS.md` 工作流约定**提权创建**，避免属主再次落到 `CodexSandboxOffline`。
- **显示侧缺陷（另案；2026-09-25 复读照片后修正结论）**：04:23 照片里的 `SYNC 04:04` **不是面板冻结**——`SYNC`/用量属于整帧元素，deep 期间只写时钟窗口，所以它停在最后一次整帧渲染（04:04:12 那次 light 会话结束帧）；而时钟窗口本身一直在按分钟写（light 下 `[clk] tick … write=711ms`、deep 下 `HIST_THIN aux=1`、`clock_ticks` 递增、`epd_busy_fails` 仅 4、无 BUSY 超时）。照片第 4 个字形带斜笔画，更符合 **'2' 叠旧 '0' 的残影**（当时约 04:2x，被读成 "04:64"），即问题在**局刷残影**而非“写不进面板”。
  - **根因（代码级）**：时钟窗口的 ghost 预算只在两处生效——`src/main.cpp:5124`（legacy deep `/usage` 拉取路径）与 `src/main.cpp:6033`（light tick 用 **per-boot** `epdPartialCount>=30`）。带 v2 Bundle 时走会合路径（`v2RendezvousClockRender`，`src/main.cpp:4575+`），**没有任何 ghost 检查**；`rtcClkPartials`（RTC，上限 `CLK_GHOST_LIMIT=90`，`src/main.cpp:118`）只累加不被消费，而 `epdPartialCount` 每次 deep 唤醒清零。于是 deep 下时钟窗口可无限局刷、残影累积；light 下也要等 30 次才清一次。
  - **建议最小修复（未实施）**：`v2RendezvousClockRender()` 增加 `rtcClkPartials >= CLK_GHOST_LIMIT` → `forceCleanRefresh` + `renderCurrent()` + `clkCaptureFromFramebuffer()` + `rtcClkPartials = 0`（对齐 legacy 路径），并把时钟窗口预算调小（如 30，小面积高对比区更易积影）；light tick 同时查 RTC 计数。

## 合并 codex/fake（共享 v2 决策 + 设备模拟器）：2026-09-25 — 合并完成、ROM 已重编并验证

`codex/fake`（`afaf5ab`，14 提交）已合入 `master`；`PROGRESS.md` 与 `src/main.cpp` 共 6 处冲突按原 `next.md` §5.4「以分支版为骨架，只贴回本地新增」解决，并按用户决策 D4/D2 处理语义分歧：C1 删除本地重复的流式辅助函数；C2 `/v2/status` 采用分支共享快照并把 4 个唤醒字段作为可选块接回；C3/C4 `/v2/plan` 采用分支 `v2DecidePlan` 并保留本地 `history_sync_ms` 语义；C5/C6 Bundle COMMIT 采用分支决策流 + 流式安装。

**过程细节、冲突逐项对照、复核点实测与包目录隔离实测已下沉** → `docs/history/progress-archive-2026-09-25.md`。

### ROM 产物（合并后重编，2026-09-25）

| 目标 | 产物 | 大小 | SHA256 |
|---|---|---|---|
| `esp32-s3-epaper-154g` | `.pio/build/esp32-s3-epaper-154g/firmware.bin` | 1744496 | `E9046F6BD1A510DBA224179D6F62D62FFB059C20D8F2844B5F184860F9B5B91E` |
| `zectrix-note4-b` | `.pio/build/zectrix-note4-b/firmware.bin` | 1758384 | `7997C3235C53755CCED60487A81AD01BB0B896AEF2693B950904D90586EC28F7` |

- 上表 note4-b 是**共享 packages 目录**下的产物。当前采用隔离包目录（`.pio-pkgs/note4` + `.pio-core`）后，同一源码产出 1757600 B / `71594E8E62783345BB2F73ABDE235A85C4C0B2324A469196B9D1ED1A5369C3DE`；再经 0.18.23 时钟窗口修复后为 **1,758,032 B / `42AAF00B257908434E86CEA45838DC60FD65FE8FD426D0632E570BD6BDBCF72E`**（= 当前 `main.cpp` 的 `0.18.23-note4-b`）。
- 同一源码在不同 packages 路径下 ROM 哈希不同（绝对路径被编进固件）。可复现构建的归一化**未做**，见 backlog 技术债 D8。
- 按 `AGENTS.md` 新约定 **`zectrix-note4-b` 为唯一固件目标**，此后不再构建 154g（交替构建代价已实测：154g 单目标 188 s，切回 note4 触发 framework 重装合计 926 s）。

### 包目录隔离（next.md §7）：note4 已落地并验证

`tools/pio-target.ps1` 按目标注入 `PLATFORMIO_PACKAGES_DIR`（`note4` → `<repo>\.pio-pkgs\note4`）与 `PLATFORMIO_CORE_DIR`（`<repo>\.pio-core`），**不改 `platformio.ini`**；两处均在仓库内且已 gitignore，因此构建不需要仓库外写权限。实测修正了 next.md 的前提：设置 `packages_dir` 后 PlatformIO **不会**回退到 `core_dir/packages`，目录必须自足——冲突的 `framework-arduinoespressif32`(+`-libs`) 真拷贝、其余 15 个包用目录 junction 指向共享 `core_dir\packages`。

判据：首用新 packages 目录 **无 banner**（93 s）、同目录再跑无 banner（26 s 增量），日志 `Reinstall|Compile Arduino IDF libs` 匹配数为 0。判据 c/d/e（需构建 154g）未做，按「只构建 Note4」约定暂缓。

### 现场事故：`git stash` 在本机不能用来保护现场

`tools/`、`artifacts/` 的 DACL 曾缺沙箱能力 ACE，shell 写入被拒；`git stash` 先 unlink 再写回，导致 16 个文件被删却无法重建（索引中仍有完整快照，已逐文件哈希核对 21/21 一致并无损恢复）。**结论：本机不要用 `git stash` 保护现场**；这两个目录只能由 `write`/`edit` 工具写入。`artifacts/`、`tools/` 的属主与能力 ACE 已于 2026-09-25 一次性修好（需提权、走 .NET、不递归）。

## 当前现场与待办（2026-09-25 核查）

| 项 | 值 |
|---|---|
| 1.54" 设备 | MAC `70041DD7A340`，名"书桌屏"，登记 IP `192.168.3.163`，`sync_enabled=true`，Profile `mini,quad` |
| Note4 设备 | MAC `7C4FADB93408`，名"Note4"，IP `192.168.3.177`，`sync_enabled=true`，Profile `codex-status-a` |
| 两设备可达性 | **核查时均 ping 不通**（最后 ACK ≈ 09-25 05:2x，距今约 5.9 h）→ 实机任务的前提是先唤醒设备 |
| 已装固件 | Note4 `0.18.23-note4-b`；1.54 最后记录 `0.16.7-bw`（master 上的时钟窗口保留修正未刷入） |
| 桥 | `bridge/target/debug/bridge-app.exe`（09-25 04:43 构建，含唤醒窗口下界修复）；**核查时进程未运行**，PID 文件陈旧 |
| 1.54 卡住作业 | Bundle job `83f4324c` 停在 `sending`（55,312 B），另有多个 `waiting` 积压 → 顶住该设备的数据投递 |
| Note4 正常 | 最新 job `1b4500ad` `succeeded`，`data_seq=applied_seq=57` |

**待办的唯一事实来源是 `docs/roadmap/backlog.md`**（紧急 A1–A5 / 暂缓 B1–B4 / 长线 C1–C6 / 技术债 D1–D10）。本轮核查已关闭两条过时结论：

- 「v2 会合路径没有 ghost 预算」**已过时**：`main.cpp` 的 `rtcClkPartials++`（1816）与 `rtcClkPartials >= CLK_GHOST_LIMIT` 判定（5166/5177）现已覆盖 light tick 与深睡会合两条时钟路径，且已随 `0.18.23-note4-b` OTA 上机；剩余工作只是调预算与实机验收。
- `bugs.md` 的 BUG-1 / BUG-2 **均已由 v2 实现修复**（ACK CRC 同源、deep 唤醒恢复数据检查点或轮换 context）；已结案并归档为 `docs/history/bugs-2026-09-22.md`（含文件行号证据）。
- `next.md` 的合并/多 env/包隔离工作已完成，已归档为 `docs/history/next-2026-09-25.md`；其 §5.2 的 `git stash` 流程在本机**不成立**（见本文下一节），不要再照做。

## 历史归档

本轮已把 `PROGRESS.md` 从 89 KB 压到约 16 KB，并归档文档（详见 `docs/README.md`）：

| 归档 | 内容 |
|---|---|
| `docs/history/progress-archive-2026-09-25.md` | 本次下沉：原 `PROGRESS.md` 111 行起的全部历史节（2026-09-24 及更早 + 合并/包隔离过程细节，约 71 KB，逐字节保留） |
| `docs/history/progress-archive-2026-09-23.md` | 2026-09-22 及更早的进度（通用平台 v2 实现、0.13–0.16 各轮实机修复） |
| `docs/history/workflow/<initiative>/` | 已结项专项原件：2026-09-25 迁入 15 个（`codex-quota-display`、`live-template-delivery` 等大部分 09-09~09-25 的专项） |
| `docs/history/next-2026-09-25.md` | 原 `next.md`（合并/多 env/包隔离，均已完成） |
| `docs/history/bugs-2026-09-22.md` | 原 `bugs.md`（BUG-1/BUG-2 结案） |
| `docs/roadmap/backlog.md` | **待办的唯一事实来源**（紧急 A / 暂缓 B / 长线 C / 技术债 D） |
| `docs/roadmap/archive-digest-legacy.md` / `-recent.md` | 各专项一页式摘要与"还欠什么" |
| `docs/README.md` | 文档总索引与"该读哪一个" |

两处归档未完成，需要决策（见 backlog D11）：`project-workflow/generic-display-platform-design/` 与
`project-workflow/live-template-delivery/` 的**文件自身 DACL 缺沙箱能力 ACE**，`git mv` /
`Remove-Item` / `[IO.File]::Delete` 全部 `Permission denied`，沙箱提权也被拒。修复方式是在**提权**
PowerShell 里对这两个目录重做 ACL（改属主 + 追加能力 ACE，走 .NET，不递归），然后
`git mv project-workflow/<name> docs/history/workflow/`。
