# 文档总索引

> 2026-09-25 建立。本文件回答一个问题：**这轮该读哪一个？**
> 会话启动时不要通读 `project-workflow/`，也不要通读 `docs/history/`。

## 读文件的顺序（按角色）

| 你要做什么 | 依次读 |
|---|---|
| 接手项目、想知道现状 | `AGENTS.md`（操作约定）→ `PROGRESS.md`（最新现场与归档入口）→ `docs/roadmap/backlog.md`（唯一待办来源） |
| 决定下一步做什么 | `docs/roadmap/backlog.md` §2 紧急 / §3 暂缓 |
| 查某个已结项专项做了什么 | `docs/roadmap/archive-digest-legacy.md` / `archive-digest-recent.md` → 需要细节再进 `docs/history/workflow/<name>/` |
| 查历史里程碑（0.13–0.18 各轮实机修复） | `docs/history/progress-archive-2026-09-23.md`（≤09-22）、`docs/history/progress-archive-2026-09-25.md`（09-24 及合并过程）、[2026-10-03 归档](history/progress-archive-2026-10-03.md)（09-25 至 10-03 较早记录） |
| 改协议/模板/渲染 | `docs/generic-display-platform-design.md`（权威架构）+ `AGENTS.md` §关键不变量 |
| 改电源/唤醒/占用 | `docs/power-state.md` + `docs/generic-display-platform-design.md` §7 |
| 改休眠设备的状态页、模板/OTA 延后投递 | `docs/generic-display-platform-design.md` + `project-workflow/sleep-aware-bridge/design.md` / `task.md` |
| 实施 PowerPlan 与 Data 同轮联合下发 | `project-workflow/bridge-macos/powerplan-data-execution-plan.md`（P1–P6；仅计划，待办 A9） |
| 改 BLE 会合功耗 | `docs/ble-rendezvous-power-design.md` |
| 改双 ROM 首次蓝牙配网页与新配对屏幕码 | `project-workflow/ble-wifi-provisioning/design.md` + `plan.md`（2026-09-30 原始设计；协议/存储/窗口以后续统一协议为准，剩余归 backlog B6） |
| 接续 Bridge macOS 菜单栏与 BLE 支持 | `project-workflow/bridge-macos/remaining-plan.md`（新窗口剩余实施入口）→ 按需读原 `plan.md` / `task.md`；已授权实施，`bridge_first` 已废弃，待办见 backlog B5 |
| 设计/实施 USB、BOOT、Bridge 激活的统一在线状态与 HTTP/BLE 管理 | `project-workflow/bridge-macos/wifi-unified-protocol-design.md`（设计依据；配套实施/部署见同目录 `wifi-unified-protocol-execution.md`，剩余验收见 B6）→ `wifi-redesign.md`；现场先读 PROGRESS，待办归 B6 |
| 实施下一步配网、Bridge 双侧界面与异步会合 | `project-workflow/bridge-macos/wifi-rendezvous-execution.md`（W0–W6、第6节界面与Fake验收）→ `wifi-redesign.md` / `wifi-protocol.md` / BLE fidelity §11–§12；最新现场先读 PROGRESS |
| 验证 Wi-Fi 分 tab / 真实 macOS 布局 | `project-workflow/bridge-macos/wifi-tabs-plan.md`；`node tools/test-device-page.mjs` + `bash tools/test-wifi-ui.sh`，WKWebView 合成 IPC / 12 场景 / 截图审阅；设备通信另验 |
| 实施设备页离线 BLE 配网、绑定独立方案与 BOOT 恢复 | `project-workflow/bridge-macos/wifi-redesign.md`（最新全量管理/UI 重设计）→ `remaining-plan.md` R0–R4 / `wifi-protocol.md`；统一协议已配套部署，完整 RF/跨平台/双板矩阵仍待验，仅 Note4 在场，见 backlog B6 |
| 改模拟器/实验时钟 | `docs/fake-rom-simulator-design.md` + `docs/history/workflow/fake-rom-simulator-def/evidence.md`（D/E/F 已结项） |
| 改字体资产 | `docs/font-asset-format.md` + `project-workflow/note4-bridge-publish/protocol.md`（**草案，未定稿**） |
| 查缺陷结案 | `docs/history/bugs-2026-09-22.md`（原 `bugs.md`，已结案） |

## 活跃文档（`docs/` 根目录，仍在演进）

| 文件 | 状态 | 说明 |
|---|---|---|
| `generic-display-platform-design.md` | **权威** | 通用显示平台现行架构。与它冲突的旧描述一律以它为准 |
| `power-state.md` | **权威** | 电源状态、身份/发现、占用 claim/lease 语义（§9.1/§9.2/§13.3） |
| `ble-rendezvous-power-design.md` | 设计已定稿，实现分 6 阶段 | 会合功耗设计；阶段 1/2 已上线，其余见 backlog C4 |
| `fake-rom-simulator-design.md` | D/E/F 软件验收完成 | 逐 MAC 时钟、fake BLE、协作调度与代表性故障矩阵；G 已取消，证据归档在 `docs/history/workflow/fake-rom-simulator-def/` |
| `font-asset-format.md` | 已实现 | CSFN 容器与字体资产格式 |
| `device-setup-experience.md` | 已过时 | 早期配网体验记录，仅作产品意图参考 |

## 归档文档（只读历史）

| 位置 | 内容 |
|---|---|
| [docs/history/progress-archive-2026-10-03.md](history/progress-archive-2026-10-03.md) | 2026-09-25 至 2026-10-03 较早的 68 节；正文完整保留，当前部署记录留在 PROGRESS.md |
| `docs/history/progress-archive-2026-09-23.md` | 2026-09-22 及更早的进度节（48 节） |
| `docs/history/progress-archive-2026-09-25.md` | 2026-09-24 及更早的历史节 + 合并/包隔离过程细节（原 `PROGRESS.md` 111 行起，逐字节保留） |
| `docs/history/generic-display-platform-design-v1.md` | 早期架构，已被现行设计取代，仅留决策史 |
| `docs/history/request.md` | 项目最初需求原文 |
| `docs/history/discussion-summary.md` | 早期讨论摘要 |
| `docs/history/sleep-plan-v4.md` | 早期睡眠计划 |
| `docs/history/icons-task.md` | 早期图标任务 |
| `docs/history/next-2026-09-25.md` | 原 `next.md`：合并 + 多 env 重构 + 包目录隔离的**已完成**记录 |
| `docs/history/bugs-2026-09-22.md` | 原 `bugs.md`：BUG-1 / BUG-2 结案记录 |
| `docs/history/workflow/<name>/` | 已结项的 `project-workflow/*` 专项原件。2026-09-25 分两批迁入 **17 个**：`bridge-multi-instance`、`bundle-v3`、`codex-quota-display`、`deep-pull-test`、`device-discovery`、`fake-rom-simulator`、`note4-bridge-publish`、`note4-buttons`、`note4-icon-correction`、`note4-live-sync`、`note4-ota-bringup`、`note4-template-96`、`pmstats`、`sleep-modes`、`wake-contact-trace`（第一批 15 个）；`generic-display-platform-design`、`live-template-delivery`（第二批，等 ACL 修好后补上） |
| `docs/history/workflow/fake-rom-simulator-def/` | 2026-09-26 D/E/F 完成原件：计划、任务、时钟审计、M01–M18 证据、硬件边界 case、状态和评审 |

### 仍在 `project-workflow/` 的专项（未结项，别归档）

Fake ROM D/E/F 已结项并归档；下表只列仍需推进的专项。

| 专项 | 为什么还留着 |
|---|---|
| `generic-display-platform-implementation` | AGENTS.md 指定的当前实现工作区；字体资产 task-7 与 convergence 条目未闭环，且 `bridge` 的测试夹具曾引用它（见下） |
| `note4-panel-power` | 面板供电双模式与帧缓存自愈修正未刷机、bench 测量全缺 |
| `clock-window-retention` | 1.54 时钟窗口保留修正已构建但未刷机、未实机复测 |
| `bridge-multi-device-ui` | 按 MAC 推送与统一族发布菜单未实现 |
| `sleep-aware-bridge` | S1–S4 已实现，S5 的硬件故障矩阵仍待验证；方案与现场见该目录和 PROGRESS |
| `bridge-macos` | 仅规划；首版要求 Windows 已运行的 BLE/低功耗 `device_first` 对等，未实施 |
| `bridge-family-sync-preservation` | 源码已修但未进入运行桥；未部署就有覆盖设备 `sync_enabled` 的风险 |
| `ble-rendezvous-power` | stage 3–6 全部未实现 |
| `power-plan-c` / `power-state` / `sleep-battery` | 功耗基线、整机硬件验收、M3 验收未闭环 |

### 构建夹具已迁出 `project-workflow`

400×300 模板夹具 `codex-status-a-400x300.json` 原在
`project-workflow/generic-display-platform-implementation/concepts-400x300/`，被 3 处 Rust
代码用 `include_str!`/`read_to_string` 引用——这是"归档文档却被构建依赖"的隐患。
2026-09-25 已复制到 `bridge/crates/core/tests/fixtures/codex-status-a-400x300.json`（与源
文件 SHA256 同为 `A429D4E0…B48F5`），并把 3 处引用改为该路径；`cargo test -p bridge-core`
与 `-p bridge-render` 全绿。**`tests/fixtures/` 里的那份才是权威夹具。**
原文件在 `concepts-400x300/` 下仍留着（`generic-display-platform-implementation` 目录未归档），
但它已不再被任何代码引用，将来随该目录一起处理即可。

## 归档判据（下次归档照着判）

一个 `project-workflow/<initiative>/` 可以移到 `docs/history/workflow/`，当且仅当：

1. 它对应的需求已在 `docs/roadmap/backlog.md` 里消失（即已交付），**且**
2. 它没有"未刷机/未实机验收/未部署"的尾巴，**或**这些尾巴已经单独记进 backlog，**且**
3. 它引用的源码路径在仓库里仍然存在（归档不改代码）。

只要还有未完成的验收或未部署的修正，就留在 `project-workflow/` 并在 backlog 里有对应条目。

## 维护约定

- `PROGRESS.md` 只保留**最新现场**；每次里程碑追加一节，超过约 15 KB 就把最旧的节下沉到新的 `docs/history/progress-archive-<date>.md`。
- 待办只写在 `docs/roadmap/backlog.md`；不要在 `PROGRESS.md`、`AGENTS.md` 或 `status.md` 里另开一份待办清单。
- 专项完成后：把结论与证据搬进 `PROGRESS.md`，从 backlog 删除该条，专项目录按上面的判据归档。
