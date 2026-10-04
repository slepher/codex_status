# W4/W6 会合窗口收口计划（只读审计 + 可执行清单）

日期：2026-10-04。范围：`project-workflow/bridge-macos/` 原 W4「读取/同步与会合闭环」、W6「软件验收与对账」。
本文件是**只读审计**：未改 bridge 生产代码、未操作设备、未重建/重启已部署 Bridge.app。
依据：工作树未提交源码（`bridge/crates/**`、`src/main.cpp` 为 M）、`rendezvous-audit.md` F1–F3、`wifi-rendezvous-execution.md` §5/§8/§14、`PROGRESS.md`「会合协议审查」节、`backlog.md` B6。

证据目录：`artifacts/bridge-macos/w4-w6/`
- `code-references.md`：全部 `文件:行号` 引用
- `existing-signals.md`：现有通过/失败信号（区分 PASS-现有行为 / PASS-局部 / RED）
- `w6-gap-matrix.csv`：W6 缺口矩阵（机器可读）

**术语**：`PASS-现有行为` = 测试通过但刻画的是当前坏行为，**不是修复证据**；`PASS-局部` = 通过但调用链绕过 app 协调；`RED` = 复现目标缺陷的失败。

---

## 0. 结论摘要

1. **W4 未闭环**：Bridge 侧不存在「本窗口已准入任务集合」这一概念；三套待办来源（plan/coordinator、`WifiState.jobs`、设备侧 `sync.due/pending`）互不可见。诊断 drain 与 Wi-Fi 读的先后由一行硬编码决定（`platform.rs:2223`），且发生在设备端窗口已自行决定关闭之后。
2. **W6 组合证据不足**：现有 48 Fake + 51 app 测试中**没有一条**经过 `ble_cycle → sync_http_inner → opportunity_locked` 顺序（`existing-signals.md` B/C）；慢入网、BLE 断链、Wi-Fi/BLE 并存、close 悬挂**无任何 app 级测试**。
3. 两个会合审查测试是 **PASS-现有行为**，`rendezvous-audit-tests.log` 首轮的 1 failed 是夹具 bug（`sync begin HTTP 400: range`），**不是**目标缺陷 red。
4. `ble_cycle` 持锁无界 close（`platform.rs:2222` + `ble/src/lib.rs:1428`）一行留占位，等 `ble-lock-investigator`（task-1）结论，不替其下结论。

---

## 1. W4 窗口生命周期：目标定义 vs 现状

### 1.1 窗口的三种来源

| 来源 | 建立点 | 期限语义 |
|---|---|---|
| 正式 PowerPlan light | `service.rs:2330 plan_for_rendezvous` → `platform.rs:2144` plan ACK；设备 `v2Plan.lightActive`（`src/main.cpp:4809`、`src/v2_state.h:97`） | `MAX_LIGHT_S=600s` / `MIN_LIGHT_S=30s`（`coordinator.rs:208-209`），BOOT provisional 300s（`coordinator.rs:211`） |
| 仅 sync 的临时窗口 | `platform.rs:2183-2197` `sync_open`（reason=retry/periodic） | 设备 `syncOpenThisWake`；无正式 light 时由诊断完成决定关窗（见 §1.4） |
| BOOT 维护窗口 | `manual_boot` 分支，`service.rs` `boot_plan`；入口 `platform.rs:1505 post_ota_window` / `hold_light` | `t_boot + 300s`，不因普通读续租 |

### 1.2 现状：谁允许准入（Bridge 侧）

`work_want_light`（`service.rs:2366-2380`）当前准入集合：
- `contract_needed`（`2362`）：缺 data_contract 且有 active context/template；
- `light_hold_until`（`2370`）：`hold_light`/BOOT/显式申请；
- `push_dirty`（`2371`，设置点 `2122`/`2186`）；
- `in_flight`（`2372`）；
- `pending_activate`（`2373`，设置点 `coordinator.rs:641`）；
- 发布 `job` 非终态（`2374-2378`）；
- `ota_jobs == queued`（`2379`）。

**缺口（F1）**：`service.rs:2330-2408` 区间内无任何 `wifi` 引用（grep 无命中）。排队的 Wi-Fi 读/同步任务**不进入工作集合**，因此永远只能"借用"别人开的窗口。证据测试 `bootstrap.rs:3003`（PASS-现有行为）。

### 1.3 现状：谁登记待办（三个互不可见的登记处）

| 登记处 | 写入 API | 存储 | 是否被窗口计划看到 |
|---|---|---|---|
| plan/coordinator | `data.push_dirty`、`pending_activate`、发布 job、`ota_jobs`、`hold_light` | `coordinator` + `platform/state.json` | 是（`2366`） |
| Wi-Fi 作业 | `wifi.rs:55` → `service.rs:471 wifi_queue_fields_checked` | `WifiState.jobs`（`platform/wifi.rs:109/132/135`） | **否** |
| 设备侧诊断 | 设备上报 `sync.due/pending`（`main.cpp:4042`），Bridge 在 `platform.rs:2184-2185` 读它发 `sync_open` | 设备 RTC + 诊断 checkpoint | 设备自己知道 |

**缺口**：没有任何一方持有「本窗口已准入任务集合」。因此「读任务是否该在本窗口完成」无法回答。

### 1.4 现状：窗口何时关闭（固件侧，已部署 ROM .45）

BLE 窗口：`max(windowStart+3000/4000ms, connectedAt+6000ms, answeredAt+200ms, historyUntil)`（`main.cpp:4778/4790/4801-4804`），到点 `bleOff`（`4810`）。

Wi-Fi 存活与关闭：
1. `light = v2Plan.lightActive()`（`4809`）；`light` 为真 → 返回后 `startNormalMode` 维持 HTTP；
2. 仅 `syncOpenThisWake` 为真（无正式 light）→ 仍返回真，HTTP 起来给诊断用；
3. `sync_complete` 受理且无后续 due（`!(syncRtc.flags & 1)`，DUE 位）→ `syncExitAtMs = millis()+200`（`4119-4120`）；
4. 主循环到期：清 `syncOpenThisWake`；**若 `!v2Plan.lightActive` 则 `enterDeep("sync complete")`**（`5784-5787`）；
5. 兜底：90s 无 durable progress → `enterDeep("sync incomplete")`（`5789-5793`）。

**缺口（F2）**：固件的关闭条件只覆盖**设备侧诊断义务**，不知道 Bridge 还有排队 Wi-Fi 作业。设备端临时窗口会在诊断完成 +200ms 关掉。证据测试 `bootstrap.rs:3022`（PASS-现有行为）。注意 F2 是**窗口归属/顺序竞态**，不等于实板每次必败（快请求可能赶上 200ms），也不等于 23:44 那次 HTTP 不可达的根因。

### 1.5 现状：Bridge 的执行顺序与位置

```
platform.rs:2222  link.close().await;              // 仍持 全局锁(2042) + 每MAC锁(2055)
platform.rs:2223  if sync_opened { sync_http_inner(...).await; opportunity_locked(...).await; }
```
- 先 drain 诊断（`platform.rs:376-407`，`sync_diagnostics::transfer` ×≤3，批间 1s 探测）；
- 再读 Wi-Fi（`wifi.rs:305-343`，先 2s status，`opportunity_window` 门禁，再 8s 预算）。

**缺口（W4-5）**：drain/read 都在 `link.close()` 之后执行。若 close 悬挂（§3.5 占位），两者**永不执行**，且两把锁不释放。

### 1.6 判定规则（收口目标语义）

> 判定「诊断 drain 与 Wi-Fi 读的先后」必须按窗口类型分派：

| 窗口类型 | 规则 |
|---|---|
| 正式 light（`accepted_remaining_s > 0` / `power.mode == "light"`） | 顺序自由，但**所有**准入任务共用一个剩余预算 `R = min(plan deadline, ...)`；超出 `R` 的任务不得在本窗口宣称执行 |
| 仅 sync 临时窗口（无正式 light） | `sync_complete` 是该窗口的**关窗动作**。任何被准入的 HTTP 读必须**先于最后一批 `sync_complete`** 发出；否则它**未被准入**，应保持 `waiting_rendezvous` 并由 UI 明确显示「等待 Wi-Fi 窗口 / 需要 BOOT」 |
| BOOT 维护窗口 | 只按 `t_boot+300s`；普通读不得延长；写入按原截止 |

**不变量**（沿用既有策略，不得放宽）：
- 读取/传输/claim/owner renew **不产生也不延长** light deadline（`PROGRESS.md`、执行文档 §5）；
- 普通 BLE ACK（status/plan 成功）**不等于** HTTP ready（F3）；
- 不得用 periodic/诊断条件冒充读任务的开窗理由。

### 1.7 W4 未闭环点精确清单

| 编号 | 未闭环点 | 证据（文件:行号） | 关联 |
|---|---|---|---|
| W4-1 | 排队的 Wi-Fi 读/同步任务不进入 `work_want_light`，无独立开窗保证 | `service.rs:2366-2380`；grep `service.rs:2330-2408` 无 `wifi` | F1；`bootstrap.rs:3003` ；**[task-6 实测 2026-10-04，已闭环]** 用户授权的「读任务自动申请有界正式 light」已实施：`plan_for_rendezvous`（`service.rs:2370-2420`）按规范化 MAC 把可重试的非终态 Wi-Fi 读/同步作业计入窗口工作集合，仅此类工作单独存在时申请新增有界常量 `WIFI_READ_LIGHT_S=120s`（`coordinator.rs:221`，< `MAX_LIGHT_S=600s`），其它工作仍用 `MAX_LIGHT_S`，BOOT/手动窗口不延长；准入是计划决策，读/传输不写任何 hold（red 断言准入后 `attempt_count==0`、读完成后回到 `sleep`）。red→green：`artifacts/bridge-macos/w4-w6/admission-red-core.log`（修复前 `left: Sleep, right: Light`，exit 101）→ `admission-green-core.log`；旧 `bootstrap.rs:3003` 断言坏行为，已按 §3.2「不能继续要求坏行为存在」反转为 `rendezvous_audit_queued_wifi_read_grants_bounded_light_window`（`admission-green-device-sim.log`）。`cargo test -p bridge-core` 106 passed / 0 failed；`cargo test -p bridge-app` 57 passed / 0 failed / 1 ignored。详见 `artifacts/bridge-macos/w4-w6/ADMISSION.md` 与 `admission-focused.diff`。未验：实机轮次/功耗、慢入网（C01）、诊断与读同窗顺序（C13/W4-2）。 |
| W4-2 | 诊断 complete 的关窗条件不知道 Bridge 侧 Wi-Fi 作业；Bridge 先 drain 后读，读落在窗口外 | `main.cpp:4119-4120`、`5784-5787`；`platform.rs:2223` | F2；`bootstrap.rs:3022` |
| W4-3 | 无「窗口准入任务集合」；登记分散在 3 处互不可见（§1.3） | `service.rs:2366` vs `service.rs:471` vs `main.cpp:4042` | 归属 |
| W4-4 | `opportunity_locked` 只用设备 status 判机会（`mode=="light"` 或 `phase=="WIFI_SYNC_ONCE"`），无任务集合、无共享剩余预算；2s status + 独立 8s 预算 | `wifi.rs:314/326/327` | F3 |
| W4-5 | drain/read 都在 `link.close()` 之后，且持两把锁；close 无界则两者都不执行 | `platform.rs:2222-2223`；`ble/src/lib.rs:1428` | task-1 |
| W4-6 | 读失败即退 `waiting_rendezvous`（`http_status_unavailable`），无「下一机会必执行」保证；UI 未区分「等窗口/需 BOOT」 | `wifi.rs:315-319`、`wifi.rs:236-245` | F1 产品面 |
| W4-7 | `opportunity_window` 要求 `power.mode=="light"`，但固件投影为 `"light"/"sleep"`（`main.cpp:2713`）、Fake 为 `"light"/"deep"`（`device-sim/src/main.rs:197`）；语义仅按 `"light"` 匹配，跨 Fake/实板不一致风险未测 | `wifi.rs:241-245` | 组合风险 |

---

## 2. 就绪/预算模型

### 2.1 三层就绪必须分离

| 层 | 含义 | 现有判据 | 问题 |
|---|---|---|---|
| L1 链路可联系 | BLE/GATT 已连、身份/加密正确 | `ble_cycle` 连接 + status ACK（`platform.rs:2073`） | 与 L3 常被混为一谈 |
| L2 请求已接受 | plan/sync_open 得到 `result=="applied"` | `platform.rs:2148`、`2195` | ACK 只说明"收到并受理"，不说明 HTTP 可用 |
| L3 HTTP ready | 设备已关联/DHCP/HTTP 监听，MAC 与合同匹配 | `wifi.rs:314` 2s status；`sync_diagnostics.rs:422` 15s status | 慢入网时 L2 成功、L3 未就绪 |

**结论**：普通 BLE ACK ≠ HTTP ready；不得因 plan ACK 成功就把 HTTP 步骤当已就绪。

### 2.2 现有预算表（含缺口）

| 阶段 | 现有值 | 位置 | 缺口 |
|---|---|---|---|
| 设备 BLE 窗口 | 3000/4000ms，connected+6000ms，ACK+200ms | `main.cpp:150-155/4778-4804` | Bridge 未读取该 deadline |
| Bridge 正常交换 | 10s | `platform.rs:2201` | — |
| 可选授权 | 250ms | `platform.rs:2208` | 与正式窗口无关 |
| 受理许可 | ≤120s | `wifi.rs:293-294` | 未绑定真实 deadline |
| 诊断 status | 15s ×3 + 1s/3s | `sync_diagnostics.rs:422-429` | 单独预算，未与读共享 |
| 诊断批间探测 | 1s | `platform.rs:388` | — |
| Wi-Fi 机会 status | 2s | `wifi.rs:314` | **小于**实板 `connectBest`（扫描 15s + 连接 30s，`main.cpp:2310/2393`）→ 慢入网会误判 |
| Wi-Fi 机会预算 | 8s | `wifi.rs:327` | 未含设备剩余窗口期限 |
| HTTP 连接/读/写 | 各自 = timeout | `device_client.rs:37-39` | 无整体 request deadline |
| 设备诊断无进度 | 90s | `main.cpp:5789` | 与 Bridge 无关 |

### 2.3 目标预算规则

1. 窗口开启时，Bridge 从设备 status 取剩余窗口（`power.provisional_remaining_s` / plan ACK `accepted_remaining_s` / 本地 plan deadline），得到单一 `R`；
2. 所有准入任务（诊断 drain、Wi-Fi 读/写、data/plan）从同一 `R` 扣减；`R<=0` 即不再发起新步骤；
3. `sleep` 计划**不因读取被续租**；`data.push_dirty`/`in_flight` 等既有开窗理由保持不变；
4. 入网失败/预算不足必须返回具体等待原因（`http_status_unavailable` 之外区分 `association_timeout`/`dhcp_timeout`/`window_budget`），UI 如实显示。

---

## 3. W6 生产 app 组合回归矩阵

### 3.1 为什么现有测试不算

现有 48 Fake + 51 app 测试全部满足下列之一，**没有一条**经过 `platform.rs:2038 ble_cycle` 的完整顺序：
- 直接调 `svc.wifi_attempt` + HTTP transport（`bootstrap.rs:2857`）；
- 直接对 Fake HTTP 端点发请求；
- 只测 app 内纯函数（`wifi.rs:402-443`）。
`existing-signals.md` B/C 已逐条列出。

### 3.2 矩阵（完整版见 `w6-gap-matrix.csv`）

| id | 场景 | 现有测试 | 当前信号 |
|---|---|---|---|
| C01 | 慢入网（assoc/DHCP 延迟 >2s / >30s） | 无（sim 无延迟旋钮） | 无信号，需新 red 夹具 |
| C02 | BLE 自然断链（disconnect 不完成） | task-1 | 占位，等结论 |
| C03 | Bridge 重启后续办 | `bootstrap.rs:2857` | PASS-局部（绕过 app 顺序） |
| C04 | Wi-Fi 读与 BLE 并存（同 wake） | 无 | PASS-局部（service 直调） |
| C05 | `ble_cycle` close 悬挂 | task-1 | 占位 |
| C06 | 普通 sleep 会合 + 排队读 | `bootstrap.rs:3003` | **PASS-现有行为**（F1 缺陷刻画） |
| C07 | 周期窗口 + 诊断 drain + 分页快照 | `bootstrap.rs:3022` | **PASS-现有行为**（F2 缺陷刻画） |
| C08 | BOOT 窗口到期 | `tools/test-wifi-recovery.cpp` | PASS-局部（宿主） |
| C09 | 200ms 关窗竞态 | 无 | 无信号 |
| C10 | Wi-Fi 与 BLE 并存（自动选网） | `bootstrap.rs:2963` | PASS-局部（Fake） |
| C11 | OTA/发布优先与窗口互斥 | `wifi-w6-regression.log`、A16 | PASS-局部 |
| C12 | 断链/换 IP 后重读 | 无 | 无信号 |
| C13 | drain/read 顺序本身 | 无 | 无信号（必须新增） |

> **判定要求**：C06/C07 现有 PASS 是「证明坏行为存在」。修复后必须把断言改成目标语义（任务在合法窗口内完成 / 关窗遵守任务集合），**不能继续要求坏行为存在**。

### 3.2.1 C01/C13 实测回填（task-5，2026-10-04，只追加）

落点与命令：`bridge/crates/app/src/platform.rs` 测试模块新增 4 条 app 级组合用例；
`cd bridge && cargo test -p bridge-app combo_ -- --nocapture --test-threads=1`
与 `cargo test -p bridge-app`。全量 `57 passed / 0 failed / 1 ignored`（唯一 ignored 为既有
`sync_v1_bridge_archive_against_note4_fake_rom`）。证据：
`artifacts/bridge-macos/w4-w6/combo-regression.md`、`combo-tests.log`、`combo-app-suite.log`。

- **C13（本行回填，原「无信号」→ 现有顺序已被生产路径实测刻画 + task-4 回归保护）**：
  `combo_cycle_order_close_then_drain_then_wifi_opportunity`（platform.rs:3206）在**生产**
  `ble_cycle` 上观测到事件序 `ble:close < sync_http_inner 的 /api/status < opportunity_locked
  的 /api/status`，且排队 job 终态为 `failed/unsupported`（只能由 opportunity 产生）；
  `combo_cycle_drain_read_is_the_single_post_close_http_step_without_a_job`（3284）用「无 job
  时 close 后仅 1 次 status 读」钉住 drain 身份。
  `combo_cycle_hung_close_still_runs_drain_and_wifi_opportunity`（3345）在 `/sim/ble/close`
  悬挂下断言两步仍按序执行且 `ble_cycle` 约 2s 有界返回——**这是 task-4 修复的行为回归保护**；
  其红色基线沿用 task-4 已归档的 `recovery-red-rerun.log`（FAILED, exit 101），本轮未回退生产码
  造 red。判定为 **PASS-现有顺序 + 修复回归**，非新缺陷 red。
- **C01（本行回填，原「无信号/需新 red 夹具」→ 已有 app 级基线，缺陷仍在）**：
  `combo_slow_onboarding_reply_after_budget_keeps_read_job_waiting`（3425）注入设备迟 3s 才答
  `/api/status`（真实 `connectBest` 上限远比机会预算 `wifi.rs:314` 的 2s 长）。实测基线：
  job → `waiting_rendezvous/http_status_unavailable`、非终态、迟答复确实送达
  （`slow_reply_sent=true`）、`plan_commands==1`（不续租 sleep）、路径无 `/api/claim`
  （不占 owner）。判定为 **PASS-现有行为（缺陷刻画）**：固定 2s 预算仍会误判慢入网，`wifi.rs`
  的错误分类/有界等待未修，故 C01 状态由「无信号」改为「有基线证据、缺陷未修」。
  注：task 描述点名的 device-sim `delay/delay_ms`（`device-sim/src/main.rs:83-87/124-126`）是
  **BLE 写 ACK 延迟**，不是 Wi-Fi 关联延迟，且不在 app 测试路径上；本轮改用语义正确的 app 侧
  设备 HTTP 延迟注入（等价注入），理由与可选最小方案见 `combo-regression.md` §4/§6。
- **并发与归属更正（Lead 2026-10-04）**：本回填对应 `platform.rs` 快照 SHA256 `1869e3fc…2f7004`，
  `admission-fix`(task-6) 正在改 `platform.rs`/`service.rs` 生产码，其落地后需重跑上述两条命令复核。
  **更正**：02:34 替换并重启已部署 Bridge.app 的是 **Lead 本人**（部署 task-4 的 close 修复，新 exe
  SHA256 `59D7BA54…B0C6`，PID 74755/74757），**不是** `admission-fix`。已核实 `admission-fix` 未触碰
  任何 `.app`、未做重建/重启，其改动当时仅 `service.rs` 与 `platform.rs`。

### 3.3 占位行：`ble_cycle` 持锁 close 悬挂（等 task-1）

- 调用点：`platform.rs:2222`，持有全局锁（`2042`）与每 MAC 锁（`2055`）；
- 无界点：`ble/src/lib.rs:1428` `peripheral.disconnect().await`（`close` 定义 `1421-1431`）；
- 后果链：close 不返回 → `platform.rs:2223` 的 `sync_http_inner`/`opportunity_locked` 永不执行 → 两把锁不释放 → 后续 HTTP 投递与下一轮会合被阻塞；
- **状态：等待 `ble-lock-investigator`（task-1）的 red 复现与最小修复结论后回填。本文件不替其判定该 future 是否就是 23:46 现场的那次悬空。**

### 3.4 建议的新增/改写测试（落点）

1. `bootstrap.rs` 新增生产 app 协调组合：慢入网旋钮（先加 `device-sim` HAL 延迟 seam）→ 断言在读任务窗口归属/等待原因；
2. 改写 `rendezvous_audit_*` 为回归：读任务完成 / 关窗遵守任务集合（F1/F2 反转）；
3. app 级顺序断言：C13 覆盖 drain/read 先后判定表；
4. C02/C05 由 task-1 交付；
5. C09 用虚拟时钟做确定性断言（不接受"偶然赶上"）。

---

## 4. 实机多轮验收步骤与判定标准

前置：设备 **Note4 `7C4FADB93408`**，登记 IP 见 `platform/state.json`（最近 192.168.10.101），运行 **.45 / ota_1**（ROM SHA256 `B4635D…9468C`）；Bridge 为已部署 app（`B827FA…CF1A`）。**当前 HTTP 可达性由 task-2 验收中，本计划不操作设备。**

步骤（每轮记录原始时间戳与 `job_id`）：
1. 队列一个只读任务（UI「读取设备方案」），确认返回稳定 `job_id`、状态 `queued/waiting_rendezvous`；
2. 等待**自然**会合（不按 BOOT、不手动扫描；记录 `device rendezvous`、`sync-v1`、`wifi` 日志）；
3. 记录该轮窗口类型：设备 `/api/status` 的 `power.mode`、`sync.phase`、`power.provisional_remaining_s`；Bridge plan ACK `accepted_remaining_s`；
4. 记录任务终态与 `wifi_state.jobs` 阶段/错误码；
5. 电池（深睡/timer）与 USB 各连续多轮（建议每侧 ≥5 轮），期间不重启 Bridge、不改网络；
6. 每轮核对：`data_seq/applied_seq`、`push_fp/acked_push_fp` 未回退（确认读不干扰 Data 路径）。

判定标准（全满足才算 W4 通过）：
- 只读任务在**有限轮次**内进入 `completed` 并有完整快照；不会无限 `waiting_rendezvous`；
- 若某轮无合法窗口，任务保留且 UI 显示具体等待原因（`等待 Wi-Fi 窗口 / 需要 BOOT`），**不伪报成功**；
- 读取/传输**不改变** PowerPlan 的 light deadline（对照设备侧 plan_id/remaining 与 Bridge 记录）；
- 诊断 drain 与读任务在同一窗口时，二者都有明确准入结果（完成或如实延后），无"诊断 complete 把读挤出窗口"的静默失败；
- Data/Plan/ACK 行为无回退（`PROGRESS` A8 基线）。

---

## 5. 生产代码改动位置与风险

| # | 位置 | 改动意图 | 风险 | 是否触碰已部署协议行为 | 必须先有 red？ |
|---|---|---|---|---|---|
| 1 | `bridge/crates/app/src/platform.rs:2223` | 统一窗口任务集合；仅 sync 窗口先读后 complete；正式 light 窗口共享 `R` | 改变 RF 使用时机；顺序错误会让诊断/数据退化 | 不改协议字节，但改变设备窗口使用顺序 | **是**（C13/C04/C07 回归） |
| 2 | `bridge/crates/core/src/platform/service.rs:2366-2380` | 让排队 Wi-Fi 读进入 `work_want_light`（自动申请 light） | **直接延长 RF/功耗**，与「读取不续租」策略冲突 | **是**（PowerPlan 语义，设备侧可见） | **是 + 产品确认** |
| 3 | `bridge/crates/app/src/wifi.rs:305-343` | 统一就绪/预算；`association/dhcp/window_budget` 错误分类 | 改变任务终态分布 | 否（Bridge 内部） | **是** |
| 4 | `bridge/crates/app/src/wifi.rs:314` | 2s status 改为按剩余预算的有界等待 | 慢入网误判/超时变长 | 否 | **是**（C01） |
| 5 | `bridge/crates/ble/src/lib.rs:1421-1431` | close 有界化 | BLE 断开语义变化，可能影响重连 | 否（链路层） | **是**（task-1） |
| 6 | `src/main.cpp:4119-4120`、`5784-5787` | 关窗条件纳入 Bridge 报告的准入集合/租约 | **固件改动**；影响已部署 ROM 的窗口/功耗语义 | **是**（设备协议/功耗） | **是**（Fake red + 明确重刷授权） |
| 7 | `bridge/crates/app/ui/index.html` + `wifi.rs:27-135` | 显示「等待 Wi-Fi 窗口 / 需要 BOOT」与具体原因 | 低 | 否 | 否（但需 `test-device-page.mjs`） |

**硬约束**：本轮只审计，**不实施**上表任何改动；#2/#6 属已部署协议行为，需用户明确授权与新的 red 证据才可动。

---

## 6. 未决项与依赖

1. **task-1（ble-lock-investigator）**：`ble_cycle` close 悬挂的 red 与最小修复；本文件 §3.3 占位，结论出来后回填并更新 C02/C05。
2. **task-2（device-acceptance）**：Note4 写分区后 HTTP 恢复与身份/ROM 核对；未确认前 §4 实机步骤不可执行。
3. **23:44 现场根因**仍未确定：当时无 `sync_open`，不能把 F2 当作那一次 HTTP 不可达的唯一根因；需设备端同 wake 的 Wi-Fi/DHCP/HTTP 日志。
4. **产品决策**：只读任务是否允许「自动申请有界正式 light」（改 #2）还是维持「仅借窗口 + UI 如实等待」（改 #1/#3/#7）。当前策略倾向前者禁止，本计划默认按「仅借窗口」设计，并把自动申请列为需授权的备选。
5. **Fake 缺口**：`device-sim` 无入网延迟/DHCP 故障旋钮（仅 `CODEX_STATUS_SIM_COOPERATIVE`、`BLE_ENDPOINTS` 等），C01 需先补 HAL seam。
6. **B6 状态**：维持开放；本文件不结项，W4/W6 的通过判据以 §4 判定标准与 §3 矩阵为准。
