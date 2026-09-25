# prompt — A3：v2 设备（Note4）深睡时钟残影：给会合时钟补上 ghost 预算

> 对应 `docs/roadmap/backlog.md` 的 **A3**。这是**唯一影响日常观感的显示缺陷**，需要改固件 + 实机验收。
> 用法：整份交给一个新窗口执行（需要用户按按键、帮忙拍照）。

## 你是一个新窗口。先读这些，不要通读仓库

1. `AGENTS.md` — 操作约定。**固件唯一目标 `zectrix-note4-b`，不要构建其他 env，不要交替构建。**
2. `PROGRESS.md` — 最新现场。
3. `docs/roadmap/backlog.md` §0 与 §2（A3）。
4. `docs/power-state.md` §13.2（显示刷新策略）与 `src/refresh_policy.h` 的文件头注释。
5. 本文件 §"代码事实"足够你开工；**不要**为了理解而通读 `main.cpp`（6088 行）。

**不提交 git**（除非用户明确要求）。不改模板、不发布、不 OTA 到 1.54。

## 缺陷（已用代码核实，不是猜测）

深睡期间只写时钟窗口，残影累积；`SYNC`/用量属整帧元素所以停在最后一次整帧。
根因是**在带已提交 v2 Bundle 的设备上，时钟窗口的 ghost 预算根本没有被评估**：

- 预算判定只在 `deepNetworkCycle()` 里：`src/main.cpp:5166`（渲染门）与 `5177`（`forceCleanRefresh = true`）。
- 但 `deepNetworkCycle()` 只在 `src/main.cpp:5694` 的 `else if (deepWakePath)` 分支被调用，
  而进入该分支的条件是 **`!(deepWakePath && v2BundleReady)`**（见 `src/main.cpp:5678`）。
- Note4 有已提交的 v2 Bundle（`v2BundleReady == true`）→ 走 `5678-5693` 分支 → **`deepNetworkCycle()` 从不被调用**
  → `rtcClkPartials >= CLK_GHOST_LIMIT` **在这台设备上永不成立**，计数器只增不减。
- v2 会合实际走的深睡时钟写入是 `v2RendezvousClockRender()`（`src/main.cpp:4617`），
  它只检查 `clkR.valid` / `activeTplHasNow` / `timeKnown()`（4618）与 `clkPixelsValid`（4627），
  **没有任何预算检查**；`clockTickWake()` 成功时也就无条件 `rtcClkPartials++`（1816）。

> 这个结论**推翻了** `PROGRESS.md` 曾一度写下的「v2 会合路径已覆盖 ghost 预算」（那是我写错的），
> 与更早的记录 `PROGRESS.md:37` 一致。落笔前自己再复核一遍，别信二手结论。

## 代码事实（写工单时已核实，可直接用）

**两个 `RGN_CLOCK` 不是同一件事**，改的时候别搞混：

| 位置 | 函数 | 含义 |
|---|---|---|
| `src/refresh_policy.cpp:97` | `classRank()` | `return 5` 是**区域合并优先级**，与 ghost 无关 |
| `src/refresh_policy.cpp:108` | `classDefaultBudget()` | `case RGN_CLOCK: return 90;` 是**每区域局刷预算**；注释自己写了 "matches CLK_GHOST_LIMIT" |

**两个计数器**：

| 计数器 | 存储 | 预算 | 在哪里被真正消费 |
|---|---|---|---|
| `rtcClkPartials` | `RTC_DATA_ATTR`（`main.cpp:197`），跨深睡保留 | `CLK_GHOST_LIMIT = 90`（`main.cpp:118`） | **只有 `deepNetworkCycle()`（5166/5177）——在 v2 设备上到不了** |
| `epdPartialCount` | 普通 RAM（`main.cpp:1080`），深睡清零 | 30（`main.cpp:1378` 的 `++epdPartialCount > 30`、`6075` 的 `>= 30`） | light 模式的分钟 tick（`loop():6075`） |

- 一次成功的分钟时钟窗口写入会**同时** `rtcClkPartials++`（1816）和 `epdPartialCount++`（1818）。
- `epdPartialCount` 复位点：`epdBegin()`（1288）与 `epdFlush()` 的全刷路径（1425）。
  **注意**：深睡薄唤醒在 `main.cpp:5528` 的 `deepThinWake()` 路径里，早于 `5531` 的 `epdBegin()` 就 `sleepToNextEvent()` 了，
  所以深睡期间 `epdPartialCount` 恒为 0→1，**永远到不了 30**。→ 深睡路径上**两个预算都不生效**。
- `CLK_GHOST_LIMIT` 与 `refresh_policy.cpp:108` 的 90 是**两处手抄的同一个数**（不同编译单元，
  宏定义在 `main.cpp:118`，`refresh_policy.cpp` 看不到它）——天然会漂移。
- `src/refresh_policy.h` 已被 `main.cpp:51`、`refresh_policy.cpp:1`、`bridge/crates/render/src/ffi.cpp:3` 共同包含，
  **是放单一来源宏的合适位置**。

## 任务 1：把预算补到会合时钟上（核心修复）

1. **单一来源**：在 `src/refresh_policy.h` 加
   `#define RGN_CLK_PARTIAL_BUDGET 30`（注释说明这是时钟窗口的 ghost 预算）。
   然后 `src/main.cpp:118` 改为 `#define CLK_GHOST_LIMIT RGN_CLK_PARTIAL_BUDGET`，
   `src/refresh_policy.cpp:108` 改为 `case RGN_CLOCK: return RGN_CLK_PARTIAL_BUDGET;`。
   顺手把 `main.cpp:1319-1320` 那条过时注释（"after 30 partials"）改准。
2. **在 `v2RendezvousClockRender()` 顶部（`main.cpp:4617` 之后、`clkPixelsValid` 判断之前或之后，由你判断哪个语义更对）
   加入 v2 路径自己的预算检查**，语义对齐 legacy 的 `5177-5188`：
   预算耗尽 → `forceCleanRefresh = true; renderCurrent(); clkCaptureFromFramebuffer(); rtcClkPartials = 0;` 然后返回。
   **关键顺序要求**：先做整帧刷新与 `clkCaptureFromFramebuffer()`，**再**清 `rtcClkPartials`；
   顺序反了会让下一次仍走旧基线。
3. **同样给 `deepThinWake()`（`main.cpp:5283` 的 `clockTickWake()`）加检查**吗？
   先判断它是否可能独立累积：它走的是同一条 `rtcClkPartials`。**要求你在报告里给出结论**，
   不要默认"顺手也加"——重复加会产生一次多余的全刷。若加，理由和实测一并写清。
4. **不要**动 `epdPartialCount` 的 30（它是"本次启动内任意局刷"的另一个作用域，
   语义与时钟窗口预算不同）。若你认为该统一，**先提出方案再改**，不要直接改。
5. **注意 `forceCleanRefresh` 的生效条件**（`main.cpp:1343`/`1357` 消费并清除）：
   - 它只在 `rgnPolicyOn == true` 的策略路径里被用于决策；`?policy=off` 的 legacy 分支
     （`main.cpp:1356-1369`）**只清除不使用** → 在 policy 关掉时，预算清影会静默失效。
   - 它只有在 `epdFlush()` 真的被走到时才生效，所以必须经 `renderCurrent()` / `renderActiveUsage()`。
6. **不要忘记失败路径**：`epdFlush()` 的 `else` 分支（约 `1452-1456`）在整帧失败时**不重置** `rtcClkPartials`，
   导致预算保持耗尽。判断这是否会让下一次进入 `!clkPixelsValid` / `!epdBaselineTrusted` 路径，
   并在报告里说明你的改法是否覆盖了这个情况。

## 任务 2：主机构建与回归

1. `src/refresh_policy.cpp` 改动会影响**宿主渲染库**（`bridge/crates/render` 编译同一份 C++）。
   必须跑：隔离 `CARGO_TARGET_DIR` 下 `cargo test -p bridge-render`（全套，现基线 35 项）。
2. 固件构建**只**用：`pio run -e zectrix-note4-b`（不要带 `-v`，GBK 控制台会挂；
   不要构建 `esp32-s3-epaper-154g`）。
3. 构建后核对并按 `AGENTS.md` 要求记录：环境名、`FW_VERSION`（在 `src/main.cpp`，
   需要 bump 吗？由你判断并说明）、产物大小、SHA256、镜像头 `byte[3]` 应为 `0x40`。
4. `git diff --check` 必须通过。

## 任务 3：实机验收（核心判据）

烧录（USB `-t upload`，或按既有受鉴权 OTA 路径）后：

1. **让设备跑一段深睡**（≥90 分钟，覆盖多个会合周期与整点分钟），期间**不要**发布模板/改 Profile。
2. 从 `/log` 与 `/status.json` 取证每项：
   - `clk_partials` 是否在到达新预算后**归零**（而不是继续爬到上限）；
   - 是否出现预期的整帧清影（`/log` 里对应的渲染条目 + `epd_streak`/`epd_busy_fails` 的变化）；
   - **`epd_busy_fails` 不得增长**（增长说明窗口几何或时序被这个改动破坏了）；
   - 记录一次完整周期的耗时（`[clk] tick build=/wake=/write=/total=`），确认整帧清影的频率可接受。
3. **残影对照**：固定机位拍照（或用户协助），对比修改前后同一时长下的时钟区域。
   照片放 `artifacts/` 并在 `PROGRESS.md` 写文件名。**不要用"看起来好些"当结论**，要写清对比条件。
4. **回归项**：确认整点分钟仍能正常局刷（不是每次唤醒都全刷）、时钟不跳字、不闪屏；
   确认轻负载下（桥不可达）也不会因为预算而每轮全刷。
5. 若预算 30 导致全刷过于频繁（观感或耗时不可接受），**回来改数字**并记录两个候选值的实测差异，
   不要只凭喜好定 30。

## 明确不要做

- **不要构建 `esp32-s3-epaper-154g`**：交替构建会触发 framework 重装（实测单次约 15 分钟量级）。
- **不要**改模板、不要发布、不要动设备 token、不要改 `bridge/crates/core` 的编译产物格式。
- **不要**把测量宽度写进 `CtOp`（那是 backlog D6 的独立任务，需要同步改 Rust 编译器与 Python 测试桥哈希）。
- **不要**顺手统一 `epdPartialCount` 与时钟预算（见任务 1.4）。
- **不要**用 `git stash`（本机会丢文件——见 `PROGRESS.md` 的事故记录）。
- **不要**提交 git；不要把 token 写进任何文件或报告。

## 交付物

1. `src/refresh_policy.h`、`src/main.cpp`、`src/refresh_policy.cpp` 的改动（保持三方模板哈希不变量不受影响）。
2. `PROGRESS.md` 追加一节：改动点（含 `file:line`）、构建产物与 SHA256、实机判据读数、照片文件名、待办。
3. 在 `docs/roadmap/backlog.md` 的 A3 里更新状态；若 D7（两处预算漂移）因本次改动而消除，一并关闭 D7。
4. 报告里明确区分证据等级：源码已改 / 已构建 / 已刷机 / 已实机验证。

## 需要用户配合

- 按 ENTER 唤醒、帮忙拍固定机位照片、确认观感可接受（全刷频率是主观判据）。
