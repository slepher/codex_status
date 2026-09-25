# prompt — A1/A2：桥与设备恢复，清掉 1.54 卡住的 Bundle 队列

> 对应 `docs/roadmap/backlog.md` 的 **A1**（紧急，阻塞所有实机任务）与 **A2**。
> 用法：把这个文件整份交给一个新窗口（或一个新 agent）执行。它是自包含的工作单。

## 你是一个新窗口。先按顺序读这些，不要通读仓库

1. `AGENTS.md` — 操作约定（固件只构建 `zectrix-note4-b`；后台进程必须分离启动；不提交密钥）。**必须遵守。**
2. `PROGRESS.md` — 最新现场（只有 ~17 KB）。
3. `docs/roadmap/backlog.md` §0 与 §2 — 待办与当前设备/产物快照。
4. 需要协议上下文时再读 `docs/generic-display-platform-design-v2.md` 的相关小节；**不要通读**。
5. `docs/history/workflow/` 下的旧专项只在被本文件点名时读。

**默认不提交 git**（用户明确要求才提交）。不要改任何代码——本任务是现场恢复，不是开发。

## 现在的状态（2026-09-25 核查，动手前先自己复核一遍）

| 项 | 值 |
|---|---|
| 桥进程 | **没有在运行**。`bridge/target/debug/bridge-app.exe` 是 2026-09-25 04:43 构建的。`artifacts/bridge-app-run.pid` 是**陈旧**的 |
| 1.54" 设备 | MAC `70041DD7A340`，显示名"书桌屏"，桥登记 IP `192.168.3.163`，`sync_enabled=true`，Profile `mini,quad` |
| Note4 设备 | MAC `7C4FADB93408`，显示名"Note4"，IP `192.168.3.177`，`sync_enabled=true`，Profile `codex-status-a` |
| 两台可达性 | 核查时**都 ping 不通**（最后 ACK ≈ 05:2x，距今约 6 h）。设备可能只是深睡，按键即可唤醒 |
| Note4 已装固件 | `0.18.23-note4-b`；最新 job `1b4500ad` = `succeeded`，`data_seq = applied_seq = 57` |
| 1.54 已装固件 | 最后记录 `0.16.7-bw` |
| 1.54 卡住的作业 | `bundle_jobs["70041DD7A340"]` = `{job_id: 83f4324c, state: "sending", created_at: 1790243724}`，冻结 Bundle 55,312 B；另有多个 `waiting` 作业（`7e7777e5`、`91b51cdb` 等） |
| 1.54 的绑定隐患 | 它的 Profile 里 4 个配额字段仍绑到 `static1`（测试源），只有 `account.plan`/`resetCredits`/`bridge.label` 绑 `codex` |
| 族草稿隐患 | 1.54 族草稿仍是 `sync_enabled=false`（见 backlog A6，**本任务不要碰**，但要避免触发族发布） |

## 背景：为什么这两个任务绑在一起

1.4 的 Coordinator **先处理未完成的 Bundle 作业，再处理 Data 投递**（这一点在你的步骤 3 里要用代码复核）。
所以只要 `83f4324c` 停在 `sending`，这台设备的数据投递就被永久顶住——即使 `sync_enabled=true` 也没用。
反过来，只要桥不在运行，谁也没法观察或清理这个作业。所以必须先把桥和设备拉起来。

## 任务 1（A1）：让桥恢复运行，并让两台设备可达

1. **先确认没有残留进程**：检查是否已有 `bridge-app.exe` 在跑（不要盲目再起一个，AGENTS.md 说重复调用只会打印 PID，但陈旧的 PID 文件会误导你）。确认目标进程确实是 `bridge/target/debug` 下的构建产物。
2. **起桥**：`pwsh tools/start-bridge.ps1`。它必须**立即返回**（分离启动、日志重定向到 `artifacts/`）。核对：
   - `artifacts/bridge-app-run.pid` 已刷新为新 PID，且该 PID 确实存在；
   - `http://127.0.0.1:8766/mcp` 已监听（对 GET 返回 405 即说明端点活着）;
   - `artifacts/bridge-app-run.out` / `.err` 有启动日志、无 panic。
3. **唤醒设备**（用户在场，让用户按键）。两台都要确认：
   - `GET http://<ip>/status.json` 可读，返回 `fw`、`slot`、`wake`、`ip`、`data_seq`、`applied_seq`；
   - **MAC 与桥登记一致**（不匹配就是另一台设备占了这个 IP，必须查清，不要改桥数据去迁就）。
4. **解开 1.54 的 IP 疑点**：`AGENTS.md` 的硬件表与 `state.json` 现在都是 `192.168.3.163`，但更早的记录写过 `192.168.1.50`。以设备 `/status.json` + ARP 实测为准；如果实测值不同，**同时**回写 `state.json` 的登记与 `AGENTS.md` 的硬件表，并在 `PROGRESS.md` 记一句。
5. **只读采集现场**，写进 `PROGRESS.md` 的新一节（现场/证据/待办）：
   - 两台设备的 `fw` / `slot` / `wake` / `data_seq` / `applied_seq` / 电量 / `ip`；
   - 桥的 `/mcp` `platform_overview`（或等价的 MCP 调用）输出：登记设备、Profile、作业状态；
   - `state.json` 里 1.54 的 `bundle_jobs` 与 `jobs` 全量状态清单。
   - **不要**在这台设备上做任何发布/OTA/claim 变更。

**完成判据**：桥 HTTP/MCP 在监听；两台设备各自 `/status.json` 可读且 MAC 与登记一致；1.54 的 `bundle_jobs` 全量状态已记录。

## 任务 2（A2）：清掉 1.54 卡住的 Bundle 队列

前提：任务 1 已完成（桥在跑，MCP `:8766` 可访问）。以下事实已由代码调查核实（2026-09-25），可直接采信，但**关键结论仍要求你用 `file:line` 复核一次**：

### 已核实的事实

- **受支持的取消路径存在**：MCP 工具 **`platform_publish_cancel`**，参数 `{"mac":"70041DD7A340"}`（`mac` 是可选参数，省略则用当前选中设备）。
  派发链：`app/platform.rs:1868-1871` → `job_cancel`（`app/platform.rs:662-665`）→ `PlatformService::cancel_job`（`service.rs:895-907`）→ `Coordinator::cancel_job`（`coordinator.rs:613-621`）。
  **它对 `sending` 状态同样有效**（工具描述说 "queued (unstarted)" 是不准确的——代码只看 `!state.is_terminal()`）。
- **取消的副作用**（逐项已核实）：`state = Cancelled`、`updated_at = now`、**并把 `data.in_flight` 清空**；随后重写 `jobs[]` 里同一 `job_id` 的摘要并持久化；因为 `Cancelled` 是终态，下一次 persist 会把它从 `bundle_jobs` 里**整条删掉**。
- **取消不会动**：设备 Profile、磁盘上的 frozen bundle、设备的 `committed_job_id`（桥**没有**写它的路径，只读）、active context、设备端任何值（包括半途 staged 的分片；已提交的 bundle 也不会被回滚）。
- **优先级确认**：`Coordinator::next_delivery`（`coordinator.rs:365-448`）里 Bundle 分支（`368-386`，带 `return`）在 Activate（390）与所有 Data 路径（408/421/433）之前 → 未完成的 Bundle 确实顶住同一 MAC 的 Data 投递。
- **为什么这个作业自己永远好不了**：状态对账要求设备上报的 `committed_job_id == job.job_id`（`service.rs:1266`）。实测设备上报的是 **`9098ec1d`**（一个早已 succeeded 的老作业），而待处理的是 **`83f4324c`** → 对账永远匹配不上，所以它会一直 `sending`。
- **重启会把 `sending` 降级为 `waiting`**（`service.rs:164-174`）。所以看到磁盘上持久化的 `sending`，含义是"本进程起来后至少成功 tick 过一次且此后从未观察到提交"，不是"重启也无法恢复"。
- **`jobs[]` 只是滞后的历史缓存**：同一个 `job_id` 在 `jobs[]` 里是 `waiting`、在 `bundle_jobs` 里是 `sending`，因为 `refresh_bundle_history`（`service.rs:318-326`）只在终态转换时被调用。**以 `bundle_jobs` 为准。**
- **影响范围只有该 MAC**（`coordinator.rs:1-6` 明写 "different devices are independent"）：Note4 `7C4FADB93408` 不受影响。但被卡住的 MAC 上，新的 `platform_publish` 会被拒（`coordinator.rs:578-580`，报 "a publish is already in progress for this device"）。

### 要做的事

1. **复核**上面两条最关键结论并引用 `file:line`：① Bundle 先于 Data（`coordinator.rs:365-448`）；② `platform_publish_cancel` 对 `sending` 有效（`coordinator.rs:613-621` 的终态判断）。
2. **只读核对设备侧真相**：读 1.54 的 `/status.json`，记录 `committed_job_id` 与 `commit_seq`。若它其实**已经**提交过 `83f4324c`（ACK 丢了），那问题的性质是"桥侧记账陈旧"，取消是安全且正确的；**不要**试图把设备的 `committed_job_id` 改成别的值，桥没有这条写路径，也不该有。
3. **执行取消**：MCP `platform_publish_cancel {"mac":"70041DD7A340"}`。记录返回的 `{"cancelled":…, "job":…}`，并**回读 `state.json` 确认** `bundle_jobs` 里该 MAC 的条目已消失、`jobs[]` 里 `83f4324c` 已变为 `cancelled`。
4. **处理那 7 个 `waiting` 历史摘要**：它们是**被替换掉的旧作业留下的孤儿历史**（`enqueue_bundle` 会替换 `waiting` 作业，`coordinator.rs:581`），**不是**活状态，不需要也不应该逐个取消。只要 `bundle_jobs` 里干净即可；在报告里说明你确认了它们是孤儿（依据：同一 MAC 短期内多个 `waiting` 摘要 + `bundle_jobs` 只有一条活记录）。
5. **只做一次干净的重发，并当作诊断**：取消后由**用户显式**发布一次（或经 MCP `platform_publish`）。全程盯着两边日志：
   - 桥：`v2_client::install_bundle` 的分片进度（`v2_client.rs:323-407` 的 BEGIN/CHUNK/COMMIT 各阶段）；
   - 设备：`/log` 的接收与提交条目。
   - 目标：拿到 **`applied` 的 COMMIT ACK**（`app/platform.rs:709-722`）。
6. **如果这次仍然失败**：**停下来，不要反复重发**。记录失败点（`v2_client.rs` 的哪个阶段：`:336` 身份/nonce、`:349-350` BEGIN 被拒、`:378-393` 分片 HTTP 错误或超时、`:396-401` 分片 `result != applied`、`:403-406` `next_offset` 不匹配），以及设备的 `epd_busy_fails`/heap/`/log`。然后判定：
   - 若失败在分片传输/超时 → 指向 backlog **C1（Bundle v3）**：55 KB Bundle 的十六进制编码（45 KB 十六进制字符）是最大嫌疑，v3 把它压到约 16 KB。
   - 若设备侧报空间/长度错误 → 记录确切错误码，再谈。
   - **不要**自行降低模板数量或裁剪 Profile 来"绕过"——那是改变产品行为，需要用户决定。

**完成判据**：① `bundle_jobs` 里该 MAC 干净；② 要么数据投递已解除阻塞（`data_seq`/`applied_seq` 开始推进），要么给出确切的失败阶段与根因，并明确指向 C1 或需要用户决策。

## 明确不要做

- **不要**从模板页发布 1.54 族：族草稿仍是 `sync_enabled=false`。**（补充：`backlog` A6 已核实当前桥的族发布会从目标设备 Profile 复制 `sync_enabled`，所以这条风险其实已不成立；但本任务仍不需要族发布，别顺手做。）**
- **不要**手改 `state.json`：受支持的取消路径存在，用工具。只有在桥根本无法启动时才考虑，且必须**先停进程**（`PlatformService` 在内存里持有状态，下一次 persist 会整文件覆盖你的修改），并同时改 `bundle_jobs` 与 `jobs[]` 两处。
- **不要**复活已取消的 `job_id`：同一 `request_id`（`bundle-<job_id>`，`v2_client.rs:341`）会按 `next_offset` 续传（`v2_client.rs:352-356`），可能接上一个半途 staged 的载荷。
- **不要**改设备的 `committed_job_id`，也不要试图让桥"认领"设备已提交的作业。
- **不要**构建固件（本任务不涉及固件；固件只构建 `zectrix-note4-b`，且交替构建代价极大）。
- **不要** OTA、不要改设备 token、不要把 `usage`/`template` 当作可以转移 owner 的接口。
- **不要**用 `git stash` 保护现场（这台机器上它不成立，会丢文件——见 `PROGRESS.md` 的事故记录）。
- **不要**停掉不属于当前构建目录的桥/服务。
- **不要**提交 git，不要把 token/Wi-Fi 密码写进任何文件或日志。

## 交付物

1. `PROGRESS.md` 追加一节：现场（两台设备 + 桥）、证据（状态读取、作业状态清单、你引用的 `file:line`）、待办。
2. 若 1.54 的 IP 或 MAC 与登记不一致，同步修正 `AGENTS.md` 的硬件表与 `state.json`。
3. 若 A2 无法用受支持路径解决，在 `docs/roadmap/backlog.md` 的 A2 条目里补一句"已确认为受支持路径缺失"，并给出建议动作。

## 需要用户配合的点（提前说明，不要卡住）

- 唤醒设备需要用户按键（尤其 Note4 的 ENTER 是深睡唤醒键）。
- 若 1.54 的 Bundle 判定为"装不下"，后续方向选择（缩模板 / 做 Bundle v3 / 放弃这台设备的数据同步）需要用户决定。
