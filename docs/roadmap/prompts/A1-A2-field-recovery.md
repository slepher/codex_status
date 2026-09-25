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

前提：任务 1 已完成，且你**已经用代码复核过**作业状态机与取消路径（不要只看文档）。

1. **先复核优先级**：在 `bridge/crates/core/src/coordinator.rs` 与 `bridge/crates/core/src/platform/service.rs` 里找到决定"先 Bundle 后 Data"的那段判定，引用 `file:line` 写进报告。如果事实与上面背景不同，**以代码为准**并说明。
2. **只读核对设备侧真相**：1.54 当前的 `committed_job_id` 与 `commit_seq` 是多少？`83f4324c` 到底有没有被提交过？
   - 若**已提交**：桥的状态只是记账错误，问题降级为"清掉桥侧的陈旧非终态"。
   - 若**未提交**：确认它是否还值得重试（55 KB 的 Bundle，1.54 是 8 MB flash）。
3. **用受支持的路径清理**：优先用 MCP 的 `platform_publish_cancel`（先 `platform_overview` 看清作业 id 与作用域）。**明确记录**它是否覆盖 `sending` 状态、以及它是否同时改动了设备 Profile / 冻结 Bundle / 设备 `committed_job_id` / active context。
4. **不要批量重发**那 7 个 `waiting` 作业。它们是历史积压，重发只会制造新的冲突。正确做法是：先判定哪些是**同一份内容的重复发布**，保留一个、取消其余，或者全部取消后由用户显式发布一次。
5. **如果没有任何受支持的取消路径能覆盖 `sending`**：**停下来报告**，不要直接编辑 `state.json`。在报告里说明改成一致状态需要动哪些字段（哪些对象引用了该 `job_id`），以及"桥与设备状态失同步"的具体风险。让用户决定。
6. **验证**：清理后确认该 MAC 下不再有非终态作业，且设备的 `data_seq`/`applied_seq` 开始推进（或明确判定为"8 MB flash 装不下 55 KB Bundle"，转 `docs/roadmap/backlog.md` C1）。

**完成判据**：1.54 该 MAC 下无非终态作业；能明确回答"数据投递是否已解除阻塞"；若无法解除，给出根因与下一步（C1 Bundle v3 或降低模板体积）。

## 明确不要做

- **不要**从模板页发布 1.54 族：族草稿仍是 `sync_enabled=false`，发布会把设备上刚开启的 `sync_enabled` 覆盖回 false（backlog A6）。要发布先把 A6 处理掉。
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
