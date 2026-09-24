# next.md — 合并 codex/fake + 多 env 重构 + 包目录隔离

> **本文件是自包含交接文档。在新窗口只读这一份即可开工，不需要之前的对话。**
> 建立：2026-09-25。上一轮只做了**只读调查**与**一个提交**，合并与重构**均未开始**。

---

## 0. 开工前必读：三件事

1. **`git status` 会显示 13 个已修改文件 + 8 个未跟踪项**。这些是**别的进行中活**，**尚未验证**，不要顺手提交、不要因此以为工作树脏是异常。
2. **不要执行 `git merge --ff-only`** —— 它一定会失败（原因见 §2.3）。
3. **合并会产生两处冲突（`src/main.cpp`、`PROGRESS.md`），且 `src/main.cpp` 的冲突解决有一个陷阱**（见 §5.4）。先读懂 §4、§5 再动手。

---

## 1. 当前现场（实测值，可直接对账）

```
git log --oneline -3
ca4022c Firmware 0.18.23: stream bundle install from file and trace wake cycles; log 2026-09-24 field notes
6d131ac Connect device selection and family profile publishing in Bridge UI
3cfc8b8 Support isolated Bridge instances with distinct tray icons

git worktree list
D:/Documents/PlatformIO/Projects/codex_status       ca4022c [master]          ← 主工作树
C:/Users/cogic/.codex/worktrees/653c/codex_status  afaf5ab [codex/fake]      ← 待合并
```

| 项 | 值 |
|---|---|
| 分支 | `master` = `ca4022c`；`codex/fake` = `afaf5ab`（**未动**） |
| 合并基 | `6d131ac` |
| 拓扑 | 两边各有独有提交 → **真正的三方合并**（`codex/fake..master`=1，`master..codex/fake`=14） |
| 待合并 worktree | `C:/Users/cogic/.codex/worktrees/653c/codex_status`（自身干净，无未提交改动） |
| 工作树 | 13 个 `M` + 8 个 `??`（见 §4.3），**未提交、未验证** |
| 关键哈希 | 154g=`22ab75315012ed65`，note4-b=`9244f2068d3cf08d` |
| 平台/包 | espressif32 **55.03.311**；arduino **3.3.11**；libs **5.5.5+sha.b774170ff46**；IDF **5.5.5** |
| PIO home | `C:\Users\cogic\.platformio`（全机唯一，无 `PLATFORMIO_*` 环境变量） |
| 磁盘 | D: 空闲 **545 GB** |

**目录分层（回答"多 env 是否多 .platformio"）：**

```
① 用户级（全机一份，两目标共享）  C:\Users\cogic\.platformio\{platforms,packages,penv,tools,.cache}
② 项目级
   ├─ 全项目一份（两目标争用！）   sdkconfig.defaults 首行哈希、managed_components/、.pio/build/project.checksum
   └─ 按 env 独立                  .pio/build/<env>/、.pio/libdeps/<env>/
```

**多 env ≠ 多 `.platformio`。** 本机只有一个 `.platformio`；`platformio.ini` 里没有 `[platformio]` 段，无
`core_dir`/`packages_dir`/`build_dir` 覆盖，全走默认值。`core_dir` 是**全项目唯一**（`Multiple: No`），
所以**无法在同一份 ini 里给两个 env 指定不同的 core_dir**——要按目标隔离只能从调用侧注入。
（官方文档：`core_dir` / `packages_dir` / `workspace_dir` / `build_dir` 各页，`PLATFORMIO_*` 环境变量等价。）

---

## 2. 为什么现在是"三方合并"，以及一个必须知道的副作用

### 2.1 上一轮那个提交是**解锁合并的唯一手段**

提交前，本地有 24 项未提交改动，其中 **2 项与合并重叠**（`src/main.cpp`、`PROGRESS.md`）。
fast-forward 合并会拒绝执行（本地改动将被覆盖）。所以必须先提交这些重叠文件。

### 2.2 提交带来的必然副作用

一旦 `master` 有了独有提交，`codex/fake` 就不再是它的祖先，**快进变为三方合并**，冲突面 = 两边都改过的文件
= `src/main.cpp` + `PROGRESS.md`。**这是仓库当前状态的固有结果，不是操作失误。**

### 2.3 因此 `--ff-only` 已失效

```powershell
git merge --ff-only codex/fake   # ❌ 会失败：fatal: Not possible to fast-forward
git merge codex/fake             # ✅ 正确
```

### 2.4 提交级冲突预演（已实测，针对当前 HEAD）

```
git merge-tree --write-tree --name-only master codex/fake
→ PROGRESS.md  CONFLICT (content)
→ src/main.cpp CONFLICT (content)
（其余 38 个文件自动合并）
```
`merge-tree` 是**干跑**：只把结果写进对象库，不移动指针、不写索引、不动工作树。可反复运行。

---

## 3. `codex/fake` 改了什么（分支侧，14 个提交）

### 3.1 主题：把 v2 决策逻辑从固件里**抽成固件与宿主共用的 C++**

14 个提交（`master..codex/fake`）：

```
afaf5ab Serve simulator PowerPlan decisions from shared C++
e591951 Persist simulator ownership and serve shared claim decisions
7aa6d38 Add controllable device simulator clock
a8578a6 Bootstrap loopback device simulator with shared v2 status
80c3671 Plan device simulator bootstrap after Stage C validation
e4fb250 Share v2 status snapshot across firmware and host
c154945 Share v2 command envelope and ACK serialization
f030dac Share claim decisions across firmware and host
dc445ff Share v2 Activate decision across firmware and host
b702aa3 Share v2 Bundle commit validation across firmware and host
fa50902 Share v2 Bundle chunk decisions across firmware and host
bd916c7 Share v2 Bundle begin decision across firmware and host
e997a72 Share v2 PowerPlan decision across firmware and host
02dd043 Share v2 Data decision across firmware and host
```

规模：**40 文件，+5436 / −273**。**`platformio.ini` 未被改动**（所以 ini 重构可在合并后独立做）。

### 3.2 新增的固件侧共享模块（8 组，`src/`）

| 文件 | 公开 API（节选） |
|---|---|
| `src/v2_data_command.{h,cpp}` | `V2DataDecision v2DecideData(...)` |
| `src/v2_plan_command.{h,cpp}` | `V2PlanDecision v2DecidePlan(...)` |
| `src/v2_bundle_command.{h,cpp}` | `v2DecideBundleBegin` / `v2DecideBundleChunkStart` / `v2DecideBundleCommit`、`V2BundleFingerprint` |
| `src/v2_activate_command.{h,cpp}` | `V2ActivateDecision v2DecideActivate(...)` |
| `src/v2_claim_command.{h,cpp}` | `v2ClaimText` / `v2PrepareClaim` / `v2DecideClaim` |
| `src/v2_command_envelope.{h,cpp}` | `v2ParseCommand` / `v2CheckCommandSession` / `v2BuildAck` |
| `src/v2_status_snapshot.{h,cpp}` | `v2BuildStatusSnapshot(const V2StatusSnapshot &)` |

### 3.3 新增的宿主侧（`bridge/`）

- `bridge/crates/device-sim/`（新 crate，1120 行 `main.rs` + 1054 行 `tests/bootstrap.rs`）——回环设备模拟器
- `bridge/crates/render/src/ffi.cpp`（458 行）+ `lib.rs`（216 行）+ `tests/v2_state.rs`（1281 行）——把固件同一份 C++ 引擎编进宿主
- `bridge/Cargo.toml` / `Cargo.lock` 相应更新

### 3.4 ★ 分支对 `src/main.cpp` 做了什么（冲突关键）

`src/main.cpp`：**+155 / −267**（净删 112 行）。被删掉的函数定义里可见：

```
-static String claimText(const String &in, size_t maxChars)      → 移到 v2_claim_command
-static String v2BodyBridgeId(const String &body)                → v2_claim_command.h 里声明了 v2BodyBridgeId
-static bool   v2BundleReplay(JsonDocument &doc)                 → 移到 v2_bundle_command
（其余决策函数体同样外移，hunk 大多为 "净删除 + 改调用"）
```

> **★ 实测关键差异：**
> - 分支版 `main.cpp`：引用 `v2Decide*` / `v2ParseCommand` / `v2BuildAck` / `v2PrepareClaim` / `v2BuildStatusSnapshot` 共 **13 处**
> - 本地版（`ca4022c`）`main.cpp`：**0 处**
>
> 即：**本地版本从未见过新的共享 API**，它仍持有旧的内联决策实现与旧调用。

---

## 4. 本地改了什么

### 4.1 已提交（`ca4022c`，4 文件 +757/−118）

| 文件 | 规模 | 内容 |
|---|---|---|
| `src/main.cpp` | +684/−83 | **唤醒周期追踪子系统** + `bsInstall` 调用改为流式 |
| `src/bundle_store.h` | +4 | 新增重载声明 `bsInstall(File&, uint32_t bundleLength, uint32_t bundleCrc, ...)` + `#include <LittleFS.h>` |
| `src/bundle_store.cpp` | +153 | 该重载的实现 |
| `PROGRESS.md` | +34 | 2026-09-24 现场记录 5 节 |

`src/main.cpp` 的具体内容：

- **唤醒追踪**：`TRACE_MAGIC 0x57545232u`（"WTR2"）、`TRACE_FORMAT 2`、`TRACE_STAGE_COUNT 9`、`TRACE_VALID_START/END`、
  `struct WakeTraceRec`（48 字节，`static_assert`）、`traceCrc` / `traceValid` / `traceCurrent` / `tracePrepare` / `traceCommit`、
  `traceStageName` / `traceWakeTypeName` / `traceResultName` / `traceTransportName` / `traceWakeType` / `traceExt1Summary`。
- **`HIST_CAP 64`** 历史环形缓冲扩容。
- **`FW_VERSION`** 新增分支：`CODEX_NOTE4_ROM_B` → `"0.18.23-note4-b"`；否则 → `"0.18.23-bw"`
  （另有 note4-a = `"0.18.19-note4-a"`，生效值见 `src/main.cpp:64-81`）。
- **`bsInstall` 由"整份 JSON 进内存"改为"从 `File` + 长度 + CRC 流式安装"**：
  调用点从 `bsInstall(body, FW_TARGET_ID, RENDER_TARGET_ID, ctx, err)` 改为
  `bsInstall(f, v2Rx.length, crc, FW_TARGET_ID, RENDER_TARGET_ID, ctx, err)`。

> ⚠️ **这三份固件文件必须共进退**：`main.cpp` 调用新重载，声明在 `bundle_store.h`、实现在 `bundle_store.cpp`。
> 只提交 `main.cpp` 会留下**编译不过的提交**。已在 `ca4022c` 中一并提交。

### 4.2 与 `codex/fake` 的关系

- `bundle_store.{h,cpp}`、`AGENTS.md` 及桥侧文件**不在**分支的 40 个文件里 → **不会冲突**。
- 但 `bundle_store.cpp` 的改动**与分支的 `v2_bundle_command` 抽离在语义域上重叠**（都动 bundle 安装/提交路径）→
  合并后必须复查二者是否一致（见 §5.4 复核点 3）。

### 4.3 工作树剩余未提交内容（13 `M` + 8 `??`，属**其他进行中活**，未验证）

```
 M AGENTS.md                                             |  10 +     ← 含本轮调查新增的双目标"已知限制"段落
 M bridge/crates/app/src/platform.rs                      | 226 +    ← 含 WakeHistory 集成，#[path="wake_history.rs"]
 M bridge/crates/app/ui/index.html                        |   5 +
 M bridge/crates/ble/src/lib.rs                           | 306 +    ← 设备 token 缓存 / 请求
 M bridge/crates/mcp/src/lib.rs                           |  60 +    ← device_token_path_at / valid_device_token
 M .../concepts-400x300/codex-status-a-400x300.json       |  98 +
 M .../concepts-400x300/status-icons-bits.json            |   4 +
 M .../concepts-400x300/status_icons.py                   |   7 +
 M .../font-plan-400x300/template-lvgl-crop.json          | 244 +
 M .../generic-display-platform-implementation/plan.md    | Bin     ← ⚠️ git 视为二进制
 M project-workflow/note4-panel-power/plan.md             |   4 +
 M project-workflow/note4-panel-power/status.md           |   6 +
 M tools/estimate-power.mjs                               |  95 +

?? bridge/crates/app/src/wake_history.rs      ← platform.rs 已 #[path] 引用，**未跟踪**
?? next.md                                     ← 本文件
?? project-workflow/bridge-family-sync-preservation/
?? project-workflow/bundle-v3/
?? project-workflow/note4-buttons/
?? project-workflow/wake-contact-trace/
?? tools/wake-contact-trace.mjs
```

两批可辨识的活：
- **唤醒诊断**（`platform.rs` + `wake_history.rs` + `tools/wake-contact-trace.mjs` + `note4-buttons/` 等）
- **桥侧设备 token 缓存**（`mcp/src/lib.rs` + `ble/src/lib.rs`）
- 另有 400×300 概念/字体与 `estimate-power.mjs` 的调整

---

## 5. 合并步骤（新窗口从这里开始）

### 5.1 前置检查

```powershell
Set-Location 'D:\Documents\PlatformIO\Projects\codex_status'
git log --oneline -1                  # 期望 ca4022c
git worktree list                     # 期望两个工作树
git merge-tree --write-tree --name-only master codex/fake   # 期望仍是 PROGRESS.md + src/main.cpp 两处冲突
```

### 5.2 保护现场（合并会动工作树）

- [ ] 备份 13 个已修改文件 + 7 个未跟踪项（**必须排除 `next.md`**）：

```powershell
git stash push -u -m "wip-before-codex-fake-merge" -- `
  AGENTS.md `
  bridge tools `
  project-workflow/generic-display-platform-implementation `
  project-workflow/note4-panel-power `
  ':!next.md'
```

> ⚠️ **不要图省事写裸 `git stash push -u`**：它会把 `next.md` 自己（未跟踪）也暂存掉，
> 于是你读完文档一执行，文档就从工作树里消失了（还在 stash 里，但当下没法再读）。
>
> 采用选择性 stash 的理由：三方合并成功时 git 会保留未冲突的本地改动，但出意外时难以回退；
> `-u` 必须带上，否则 `wake_history.rs` 不被暂存，而 `platform.rs` 已 `#[path]` 引用它 → Rust 侧编译失败。
> 执行完请核对 `git status --short` 中只剩 `UU src/main.cpp`、`UU PROGRESS.md` 与 `?? next.md`。
- [ ] 或最小备份：`Copy-Item .pio .pio.bak -Recurse`（约 230 MB）以防后面重建时想回退

### 5.3 执行合并

```powershell
git merge codex/fake            # 期望：38 文件自动合并，PROGRESS.md 与 src/main.cpp 报冲突
git status --short              # 确认只有那两个文件处于 UU
```

### 5.4 解决 `src/main.cpp` 冲突（**有陷阱，务必按此顺序**）

**策略：以分支版为骨架，只把本地新增贴回去。**

1. 先看分支版结构：`git show codex/fake:src/main.cpp`（决策逻辑已外移，13 处 `v2Decide*` 调用）
2. 取分支版为底：`git checkout --theirs src/main.cpp`（merge 状态下 `--theirs` = `codex/fake` 侧）
3. 再把**本地独有的两部分**贴回：
   - 唤醒追踪子系统（`WakeTraceRec` 及其全部 helper，见 §4.1）
   - `bsInstall(f, v2Rx.length, crc, ...)` 流式调用（替换旧的 `bsInstall(body, ...)`）
4. **★ 复核点（防止静默回退）**：贴回后必须确认
   - `main.cpp` 中 `v2Decide*` / `v2ParseCommand` / `v2BuildAck` / `v2PrepareClaim` / `v2BuildStatusSnapshot`
     的引用数 **≥ 13**（与分支版一致）。若变成 0 或明显减少，说明把旧的**内联决策实现**又带回来了——
     那会绕过共享模块，导致固件与模拟器行为分叉。
   - 旧的 `claimText` / `v2BundleReplay` 等函数**没有**被重新引入 `main.cpp`（它们应只存在于 `v2_*_command.cpp`）。
5. **复核点 2**：`bsInstall` 的两个重载都还在（`bundle_store.h` 里声明 2 个、`bundle_store.cpp` 里 2 个实现）。
6. **复核点 3**：`bundle_store.cpp`（本地流式重载）与 `v2_bundle_command.*`（分支抽离的 bundle 决策）
   在 bundle 校验顺序/CRC 语义上是否一致；如不一致，**记录下来交给用户决策**，不要自行改变语义。

### 5.5 解决 `PROGRESS.md` 冲突

- [ ] 按时间顺序合并两段追加内容（本地 9/24 五节 + 分支 56 行），纯文档，无逻辑风险
- [ ] 保留双方全部小节，不删任何一节

### 5.6 收尾

```powershell
git add src/main.cpp PROGRESS.md
git commit                     # 完成合并提交（信息建议：Merge codex/fake: shared v2 command decisions + device simulator）
git log --oneline --graph -4   # 确认出现合并提交（两个父提交）
git worktree remove C:/Users/cogic/.codex/worktrees/653c/codex_status   # 仅在合并成功后
git branch                     # codex/fake 分支保留，不删
git stash pop                  # 恢复 §5.2 的进行中改动
```

- [ ] 记录：合并提交 sha、冲突解决方式、复核点 1/2/3 的实测结果

### 5.7 合并后必须重编（预期之内）

8 组 `src/v2_*.cpp` 会加入 154g 与 Note4 两个目标的编译，**两目标首次构建必须重编应用层**。
这不是框架重装，但同样耗时。诊断建议：

- [ ] 合并后先跑 **154g 一个目标**，确认编译通过（不要立刻两个都跑）
- [ ] 用 `-DCODEX_TARGET_*` 相关条件确认 `FW_VERSION` 正确
- [ ] 两目标 ROM 都出来后按 §7 验证

---

## 6. 合并之后：多 env + extends 重构（步骤 3）

**目标**：把 `platformio.ini` 重排为清晰分层，**env 名一律不变**
（改名会让 `.pio/libdeps/<env>`、`.pio/build/<env>` 失配，招致额外全量重编）。

```
[env:esp32-s3-base]               ← 新增：共享 platform/board/framework/闪存/PM sdkconfig
[env:epaper-154g-base]            ← 新增：154g 硬件（8 MB / qio_opi / partitions.csv / 40 MHz）
[env:zectrix-note4-base]          ← 新增：Note4 硬件（16 MB / dio_opi / partitions_note4.csv）
[env:esp32-s3-epaper-154g]        ← 发行目标 A（名字不变）
[env:esp32-s3-epaper-154g-gray4]  ← 实验（extends 154g）
[env:esp32-s3-epaper-154g-btpm]   ← 实验（extends 154g，追加一条 sdkconfig）
[env:zectrix-note4-a]             ← 实验/基线（extends note4-base）
[env:zectrix-note4-b]             ← 发行目标 B（extends note4-a，追加 CODEX_NOTE4_ROM_B）
```

**等价性验证（必做，任何差异都是回归）：**

```powershell
# 脚本已存在（只读，逐项解析 extends + ${env:...} 插值）
python artifacts/hash-forensics/dump_effective.py > "$env:TEMP\before.json"   # 改之前
python artifacts/hash-forensics/dump_effective.py > "$env:TEMP\after.json"    # 改之后
code "$env:TEMP\before.json" "$env:TEMP\after.json"                          # 逐项比对
# 两个发行目标哈希必须不变：
python artifacts/hash-forensics/compute_fingerprint.py
#   期望 154g=22ab75315012ed65   note4-b=9244f2068d3cf08d
```

> ⚠️ **`artifacts/` 目录对 shell 写入被 ACL 拒绝**（实测 `UnauthorizedAccessException`；`write` 工具可写）。
> 所以快照输出请落到 `$env:TEMP`，不要写进 `artifacts/`。

**⚠️ 已知副作用：改 `platformio.ini` 会让 PlatformIO 自己删掉 `.pio/build`。**
官方 `build_dir` 文档原文：*"If you modify platformio.ini, then PlatformIO will remove this folder automatically."*
判据是 `.pio/build/project.checksum`（当前 `66b5223279e89d6904fd4e0a2f232947580450b9`）。
→ **ini 重构必然触发两目标各一次全量重编**（实测 942 s / 1003 s）。这与 `AGENTS.md`「不要例行清理」不冲突
（工具行为，非人为），但要事先知道。

---

## 7. 包目录隔离（步骤 4：真正消除重装的手段）

**根因回顾**（完整调查见 `artifacts/hash-forensics/`）：pioarduino `arduino.py` 把
`MD5(custom_sdkconfig + mcu + board_memory_fingerprint)` 与**项目根 `sdkconfig.defaults` 首行**的
`# TASMOTA__<hash>` 比对；不匹配就打印 `*** Reinstall Arduino framework ***`、
删掉**所有** `sdkconfig.<env>`、**整个删除并重新下载** `framework-arduinoespressif32{,-libs}`，再全量重编 IDF 库。
两个发行目标共享同一份 `custom_sdkconfig`，但指纹不同（154g `|False|qio_opi|` vs note4 `|False|dio_opi|opi`），
且**项目根 `sdkconfig.defaults` 是唯一的**（被两个目标争写）→ 交替切换必然失配、必然重装（已双向实测）。

**实现**（已落地 `tools/pio-target.ps1`；**不改 `platformio.ini`**）：

**① 实测推翻了一个前提**：设置 `packages_dir` 后 PlatformIO **不会**回退到 `core_dir/packages`。实测把 `PLATFORMIO_PACKAGES_DIR` 指向空目录后，`pio pkg list` 立即开始安装 `tool-esp_install`。所以「只放 framework 与 `-libs`、其余靠回退」不成立——每个目标目录必须自足。

**② 用「真实拷贝 + junction」代替重新下载**：只把真正冲突的两个包（`framework-arduinoespressif32` 69 MB + `framework-arduinoespressif32-libs` ≈2.0 GB）**真实拷贝**给各目标独占，可自由重装而不影响对方；其余 15 个包（`toolchain-xtensa-esp-elf` 1.36 GB、`toolchain-riscv32-esp` 908 MB、`framework-espidf`、`tool-cmake`、`tool-scons`、esptool、gdb 等）用**目录 junction** 指向共享 `core_dir\packages`（Windows 目录 junction 无需管理员权限）。

**③ 全部落在仓库内，不需要越权访问**：packages 根默认 `<repo>\.pio-pkgs\<target>`，并把 `PLATFORMIO_CORE_DIR` 也指向 `<repo>\.pio-core`（内含指向共享 `platforms\espressif32` 的 junction）。必要性：PlatformIO **每次运行都会写 `<core_dir>\platforms.lock`**，若沿用机器级 core dir，则每次构建都要求仓库外写权限。两者均已加入 `.gitignore`。**必须从调用侧注入**，因为 `[platformio]` 段全项目唯一、`core_dir` 为 `Multiple: No`。

```powershell
pwsh tools/pio-target.ps1 -Target note4 -Setup   # 只准备目录，不构建
pwsh tools/pio-target.ps1 -Target note4          # 构建（-Target 默认 note4）
pwsh tools/pio-target.ps1 -Target note4 -t upload
```

**核心验证判据（逐条记录日志）：**

- [x] a. note4 首用新 packages 目录 → **未出现 banner**（种子拷贝已带 note4 指纹），93 s
- [x] b. 同目录再跑 note4 → **无** banner / `Compile Arduino IDF libs`，26–45 s
- [ ] c. 154g 首用其 packages 目录 → **允许**一次（**未做**：154g 目录创建被用户拒绝，按「只构建 Note4」约定暂缓）
- [ ] d. **切回 note4 → 不得出现 banner**（关键断言，**未做**）
- [ ] e. 交替 note4 → 154g → note4（均为第二次以上）→ **三次都无 banner**（**未做**）

**成本**：note4 目录 ≈2.1 GB（仅真实拷贝部分，junction 不占空间）；两目录的包版本需手动保持同步。

**⚠️ 副作用：ROM 随 packages 路径变化，不可跨路径复现。** 绝对路径会被编进固件（`firmware.bin` 内含 `pio-pkgs` 字符串），故同一份源码在不同 packages 路径下 ROM 哈希不同（实测相差 736 B）。若要可复现哈希，需加 `-ffile-prefix-map`/`-fmacro-prefix-map` 归一化路径——**未做，留作决策**。

---

## 8. 产物正确性验证（每个 ROM 都要过）

- [ ] `sdkconfig.defaults` 首行 == 该目标预期哈希（154g `22ab75315012ed65` / note4-b `9244f2068d3cf08d`）
- [ ] 镜像头：`firmware.bin` 第 4 字节 `0x30`（8 MB/40 MHz）= 154g；`0x40`（16 MB/40 MHz）= Note4
- [ ] `partitions.bin` SHA256 两目标不同，且与各自 `.csv` 对应
- [ ] 框架侧 `framework-arduinoespressif32-libs/esp32s3/sdkconfig` 的 `CONFIG_ESPTOOLPY_FLASHSIZE` / `CONFIG_SPIRAM_MODE_*` 与本目标一致
      （**共用包时此项不充分**，见风险 R2）
- [ ] `FW_VERSION`（`src/main.cpp:64-81`）与 ROM 内 `esp_app_desc` 一致
- [ ] 两目标 ROM 路径 + SHA256 写入 `PROGRESS.md`

参考值（2026-09-24 实测，仅供对账）：

```
esp32-s3-epaper-154g firmware.bin  1736352  AC2C8985DC084F0F076CA321FA3974ADBA88742209E8F61AB4E581E7B059B188
zectrix-note4-b      firmware.bin  1750128  BA879C9F3AA5C0664E86DEF0AAA550054FA08E3CD66E53952E09C406F81C0E64
esp32-s3-epaper-154g bootloader.bin 19968   ABF25ECE9CAF8B736A3EE4DDFDA32627AEB0A5196E1057C2F5767687276F19FE
zectrix-note4-b      bootloader.bin 18720   80F92A58A2C05EC25DF91BD838D977081FAA4438FFB27384BA6DF91CB937F0FB
```

---

## 9. 决策点（**推迟到新窗口做**，不要现在自行决定）

| ID | 决策点 | 选项 | 备注 |
|---|---|---|---|
| **D1** | 合并产出的合并提交**是否同时**包含 §6 的 ini 重构 | (a) 合并单独一个提交，重构另起 (b) 合并即顺带重构 | 建议 (a)：合并已足够大（40 文件），混入 ini 重构会让回退粒度变粗 |
| **D2** | 实验 env（`-gray4` / `-btpm`）在新结构中定位 | (a) 与发行目标同层、注释区分 (b) 拆到独立 `extra_configs` 文件 | 建议 (a)，只有 2 个 |
| **D3** | `project.checksum` 导致的 ini 重构全量重编 | (a) 接受 (b) 尝试保留 `.pio` 规避 | 建议 (a)：规避依赖未公开实现细节 |
| **D4** | `bundle_store.cpp` 与 `v2_bundle_command.*` 语义是否一致（§5.4 复核点 3） | 需人工判断 | 若不一致必须交用户决策，不得自行改语义 |
| **D5** | 工作树 13+8 项进行中改动如何收尾（提交/暂存/丢弃） | — | 属他人进行中的活，**不要替用户决定** |
| **D6** | 清理 `C:\Users\cogic\.platformio\framework-arduinoespressif32-libs.broken-20260924`（9/24 手工恢复遗留坏包） | (a) 删除 (b) 移出 PIO home 存档 (c) 暂不动 | **建议 (b)**：留证但不干扰判定 |
| **D7** | 合并成功后是否 `git worktree remove` | (a) 移除 (b) 保留 | 建议 (a)；`codex/fake` 分支保留不删 |
| **D8** | `project-workflow/generic-display-platform-implementation/plan.md` 被 git 视为**二进制** | 需查是否误判（编码/CRLF） | 会影响 diff 可读性，建议排查 |

---

## 10. 风险与"不要做"

### 风险

| ID | 风险 | 缓解 |
|---|---|---|
| **R1** | 合并重建与 ini 重构重建代价叠加（各约 30 min 量级） | 分先后；先合并→确认编译通过→再决定何时重构 |
| **R2** | 上游 [issue #532](https://github.com/pioarduino/platform-espressif32/issues/532)（**open**）：HybridCompile 把按内存类型的产物写进共享 `lib/`+`ld/`。**本机已实测该错位**（7 个文件；`dio_opi/` 仍是 7/20 原厂内容）。相关 [#533](https://github.com/pioarduino/platform-espressif32/issues/533) | 包目录隔离只能把影响限制在**单目标内**，不能消除。持续跟踪上游 |
| **R3** | `sdkconfig.defaults` 是项目级唯一生成物，被两目标争写 | 不要试图固定首行；靠包隔离让两侧各有期望值 |
| **R4** | 隔离后两个 packages 目录包版本可能漂移 | 每次出 ROM 记录 `PACKAGES:` 段并比对 |
| **R5** | `artifacts/` 对 shell 写入被 ACL 拒绝 | 快照写到 `$env:TEMP`，或用 `write` 工具 |
| **R6** | `src/main.cpp` 冲突若解错，会**静默绕过**共享决策模块（固件与模拟器行为分叉） | §5.4 复核点 1 的引用计数检查 |
| **R7** | `platformio.ini` 改动会让 PlatformIO 自动删 `.pio/build` | 事先接受；不要在不知情时以为是故障 |

### 不要做

- ❌ 不要 `git merge --ff-only`（必失败）
- ❌ 不要修改框架包内 `arduino.py` / `espidf.py` / `component_manager.py`
- ❌ 不要改 `custom_sdkconfig` 内容或 `board_build.arduino.memory_type` / `board_build.psram_type`
      （上游 PR [#511](https://github.com/pioarduino/platform-espressif32/pull/511) 记录其后果是静默启动崩溃）
- ❌ 不要固定/手工编辑 `sdkconfig.defaults` 首行
- ❌ 不要改 env 名
- ❌ 不要 `pio run -t clean` 或手工删 `.pio` 来"解决"问题
- ❌ 不要提交构建产物/生成物，不要提交任何密钥（Wi-Fi 密码、设备 token）
- ❌ **不要替用户决定 D5**（工作树里那 21 项进行中改动）
- ❌ 不要为了省时间跳过 §5.4 的复核点

---

## 11. 关键背景资料（本次调查产物，均在 `artifacts/hash-forensics/`）

| 文件 | 用途 |
|---|---|
| `compute_fingerprint.py` | 复算各 env 的 `TASMOTA__` 哈希；**用来证明 ini 重构后哈希不变** |
| `dump_effective.py` | 解析 extends + `${env:...}` 插值，导出每个 env 的有效配置；**用来证明 ini 重构语义等价** |
| `pin_down.py` | 演示哈希输入构成的等价性 |
| `reinstall-repro-154g.log` | （若存在）1.54g 方向的重装复现日志 |

外部依据（社区证据）：

- pioarduino PR [#511](https://github.com/pioarduino/platform-espressif32/pull/511)（2026-07-23 合并）——触发重装的指纹逻辑来源，作者也是 Meshtastic 维护者
- pioarduino issue [#532](https://github.com/pioarduino/platform-espressif32/issues/532)（open）/[#533](https://github.com/pioarduino/platform-espressif32/issues/533)——共享 `lib/`+`ld/` 污染
- Meshtastic PR [#11834](https://github.com/meshtastic/firmware/pull/11834)——"哈希写入过早导致陈旧复用"，其修法是比对包内 `<mcu>/sdkconfig` mtime
- PlatformIO 文档 `core_dir` / `packages_dir` / `workspace_dir` / `build_dir` 四页

---

## 执行记录

> 执行时在此追加：时间、命令、结果摘要、判据是否通过。

| 步骤 | 时间 | 结果 | 证据 |
|---|---|---|---|
| 前置提交（`ca4022c`） | 2026-09-25 | ✅ 完成 | 4 文件 +757/−118 |
| 1 保护现场 | 2026-09-25 00:28 | ⚠️ **§5.2 流程失效，已改用别法** | 见下「事故」节；WIP 21/21 逐文件哈希一致 |
| 2 合并 `codex/fake` | 2026-09-25 00:42 | ✅ 完成 | 合并提交 `ecc7419`（父 `ca4022c` + `afaf5ab`），41 文件 |
| 3 ini 重构 | — | 未开始 | 见 §9 D1（建议 (a)：另起提交） |
| 4 包目录隔离 | — | 未开始 | — |
| 5 产物验证 | 2026-09-25 01:15 | ✅ 完成 | 154g + note4-b 均编译成功并按 §8 逐项验证；宿主 `cargo test -p bridge-render` 35/35 |
| 6 文档同步 | 2026-09-25 | ✅ 部分 | 合并已写入 `PROGRESS.md`；本节 |

### 事故：§5.2 的 stash 流程在本机不成立（务必修订本节）

按 §5.2 执行选择性 `git stash push -u` **部分失败**：`tools/` 与 `artifacts/` 目录带 `CodexSandboxUsers` ACL，**拒绝 shell 写入**（§6 只记了 `artifacts/`，`tools/` 同样）。git stash 先 unlink 再写回，于是 **16 个文件被删除却无法重建**（`AGENTS.md`、`platform.rs`、`ui/index.html`、`ble/lib.rs`、`mcp/lib.rs`、`wake_history.rs`、`bundle-v3/design.md`、4 个 project-workflow 子目录文件等），索引却已暂存全部改动。

**已无损恢复**：stash 对象 `stash@{0}`（`ae42586`）完整保存 13 个已修改 + 8 个未跟踪文件；索引中亦有同一份 WIP 快照 → 用 `git checkout --` 从索引恢复 16 个被删文件，再 `git reset` 取消暂存。**核对 21/21 个 WIP 文件与 `stash@{0}` / `stash@{0}^3` 的 blob 哈希完全一致**。另存独立备份于 `%TEMP%\codex-wip-backup\{wip-tracked.patch,wip-untracked.tar}`。

**修订建议**：本机不要用 `git stash` 保护现场（`tools/`、`artifacts/` 只能由 `write`/`edit` 工具写入，git 无法回写）。替代做法：(1) 先 `git merge-tree --write-tree --name-only` 判定冲突面；(2) 若冲突文件与 WIP **零交集**，直接在脏工作树上合并（本次即如此，分支 40 文件 vs WIP 21 项无交集）；(3) 需要备份时用 `git diff`/`git archive` 导出到 `%TEMP%`，不要动工作树。

### 决策落实

- **D4（bundle 安装，用户选 B）**：已把 `src/v2_bundle_command.cpp` 改为**流式校验**并**移除 `bodyOut` 出参**。分块（256 B）从 flash 算 CRC，并用 ArduinoJson 过滤解析只读 `bridge_id`/`job_id`。校验顺序与错误码逐项保留。**复核点 3 结论：语义一致** —— `v2CrcOf`/`v2Crc32` 与本地 `v2FileCrc` 同为 CRC32/IEEE（poly `0xEDB88320`、初值 `0xFFFFFFFF`、末尾取反），唯一差异是分支多一次整份 `String` 读取，现已消除。
  - 配套：`src/v2_state.h` 拆出 `v2Crc32Start/Update/Finish`（`v2Crc32` 结果不变）；宿主 shim `LittleFS.h` 补 `File::readBytes`（ArduinoJson 非 `Stream` 通用 reader 需要）；`ffi.cpp` 改读回文件填 `body_out`，使 `v2_state.rs` 载荷断言无需放宽。
  - ⚠️ 副作用：`oom` 分支（`reserveString` 失败）随整份载荷一起消失，已删 `reserveString` 与 `<type_traits>`；该路径本就无测试覆盖。
- **D2（/v2/status 唤醒字段，用户要求两边都保留）**：`V2StatusSnapshot` 新增 `const V2WakeSnapshot *wake = nullptr`；固件绑定真实 trace，宿主/模拟器留空 → 宿主 JSON 逐字节不变，字段位置与本地版一致（紧跟 `device_mac`）。

### 复核点实测

| 复核点 | 判据 | 实测 |
|---|---|---|
| 1 防静默回退 | 共享 API 引用 ≥13，且旧内联决策未回归 | **14 处**（分支 13 + 1 注释）；`v2BundleReplay`/`v2FileCrc`/`v2BodyBridgeId` 均 **0** |
| 2 `bsInstall` 双载 | 声明 2 + 定义 2 | `bundle_store.h` **2**、`bundle_store.cpp` **2**；`main.cpp` 调流式重载 |
| 3 bundle 语义 | 与 `bundle_store.cpp` 流式路径一致 | **一致**；差异仅为内存策略，已按 D4 消除 |

### 待办 / 风险

- [ ] **并发写入警告**：执行期间观测到**本工作树被外部进程同时修改**——`bridge/crates/mcp/src/lib.rs`（00:37:51）、`bridge/crates/app/src/main.rs`（00:40:22，+177/−43，新增 `preflight_fallback` 路径）都不是本轮改动，且 `main.rs` 原本不在 §4.3 的 13 项清单内。D5 范围内的「别人的进行中活」正在变动，**合并提交已刻意排除这些文件**（`git diff --cached` 核对过）。后续操作前请重跑 `git status` 对账。
- [x] 154g 与 note4-b 均已编译成功并按 §8 验证，ROM 路径 / 大小 / SHA256 已写入 `PROGRESS.md`（见「ROM 产物」节）。**交替目标的代价实测确认**：154g 单目标 188 s；切到 note4-b 触发 framework 重装共 **926 s**。
- [ ] **用户已定：`zectrix-note4-b` 为唯一固件目标**，`AGENTS.md` 已改写（原「固件双目标构建顺序」→「固件目标：只构建 Note4」），`platformio.ini` **未改**。在 §7 包目录隔离落地前不要构建 154g。
- [ ] D1/D3/D6/D7/D8 尚未处理（D8：`generic-display-platform-implementation/plan.md` 被 git 视为二进制，仍未排查）。
- [ ] 合并后 `stash@{0}` 仍保留作备份；**不要 `git stash pop`**（WIP 已在工作树中，pop 会冲突且再次触发 ACL 删除）。

