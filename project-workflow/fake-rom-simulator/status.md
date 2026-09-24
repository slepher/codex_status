# Fake ROM 与多设备实验时钟状态

## 2026-09-24：计划落地，Task 1 完成

主代理定下一个模拟器进程一台设备、Bridge 按 MAC 维护独立目标时钟、设备侧和 Bridge 侧可分别配置倍率/偏移/漂移的合同；同步与失步都列入验收。阶段、路由、实机隔离与时间迁移约束见 `plan.md`，原提案 `docs/fake-rom-simulator-design.md` 已同步修正。当前仍没有可运行的 Fake ROM。

6-luna high 完成首个编码任务 `task-1-coordinator-time.md`：Coordinator 的四个隐藏系统时间入口改为显式 `now`，生产调用传原真实时间；新增跨 MAC 时间测试。隔离测试 104 项通过、app check 通过、`git diff --check` 通过。没有部署、设备操作或提交。

下一步：主代理先给出 Task 2 的精确按 MAC 运行目标模型和测试合同，再交 6-luna high 实现。现有 `PlatformService`、app、BLE 仍有隐藏时间读取；不能把 Task 1 当成全链 fake 时钟已接通。

## 2026-09-24：Task 1 已提交，Task 2 写入目标核验通过

Task 1 与计划已提交为 `cc6a6c1`。Task 2 `task-2-target-mac-guard.md` 由 6-luna high 编码并通过宿主验证：v2 HTTP 写入在 POST 前核对认证状态的目标 MAC；app 按目标记录取 IP；错 MAC 的 Data/Plan/Activate/Bundle 测试只见四次 GET、没有 POST。隔离 `bridge-core` 105 项与 `bridge-app` check 通过，未提交、未部署、未接设备。另一代理同期修改 ROM，本任务没有编辑其文件。

下一块先隔离设备操作 token 与 BLE 目标身份，再把缓存、claim 与周期调度逐台接通。`src/main.cpp` 和 `project-workflow/clock-window-retention/` 属另一代理，后续提交需明确排除。

## 2026-09-24：Task 3 按 MAC token 隔离通过

`task-3-device-token-mac.md` 已由 6-luna high 执行。新 token 缓存文件含目标 MAC；旧无绑定缓存保留但不使用。BLE 在取 token 命令前核对绑定 info 的 Wi-Fi MAC；claim/OTA 均按目标 MAC 查找或获取 token。隔离 `bridge-mcp` 1 项、`bridge-ble` 4 项测试与 `bridge-app` check 全通过；没有操作实机或运行服务。本轮未提交。下一步先让 Bridge 状态读取支持 fake 进程的 `127.0.0.1:port`，再实现逐 MAC 的 app 状态缓存和投递循环。

## 2026-09-24：Task 4 显式端口状态读取通过

`task-4-device-endpoint-port.md` 已由 6-luna high 实现。`bridge-core::device` 保留裸 IPv4 默认端口 80，支持显式 `IPv4:port`；本地 TCP 测试确认从动态端口取得 `/status.json` 及 MAC。隔离 `bridge-core` 测试全通过，`git diff --check` 通过。未接硬件、未部署、未提交。下一步处理按目标 MAC 的 `/claim` 端点和状态隔离，再接多设备周期调度。

## 2026-09-24：Task 5 claim 目标端点核验通过

`task-5-claim-target.md` 已由 6-luna high 实现。claim 先按目标 MAC 解析设备端点，在 POST token 前取状态并核对 MAC。本地 HTTP 测试证明错 MAC 仅 GET 无 POST、匹配才 POST；隔离 `bridge-app` check 与 2 项定向测试通过，全仓 `git diff --check` 通过。本轮未部署、未访问设备、未提交。逐 MAC owner/在线缓存和周期投递仍待实现。

## 2026-09-24：Task 6 逐 MAC 只读状态轮询通过

`task-6-v2-status-poll.md` 已由 6-luna high 完成。认证 `/v2/status` 的成功、离线与错 MAC 按目标 MAC 写独立缓存，10 秒轮询遍历已登记的非 legacy 设备。隔离 `bridge-app` check、缓存测试 1 项、claim 回归测试 2 项及 `git diff --check` 全通过。在线与时间字段尚无调度消费者；逐 MAC claim、投递和独立 fake 时钟仍待实现。本轮未提交、未访问实机或运行服务。

## 2026-09-24：改为串行开发，Task 7 逐 MAC 占用完成

用户取消并行开发方案。主代理已把 A→B→C→D→E→F→G 的串行验收与“同一时间仅一名 Luna 编码代理”写入 `plan.md`，不建立并行 release worktree。Task 7 逐 MAC owner、yielded、last_claim_at 和在线门限已通过宿主检查；评审修正了 409 不得刷新成功 claim 时间的问题。隔离 `bridge-app` check、v2 状态测试 2 项、claim 回归测试 2 项通过。v2 gate 尚未接周期循环；后续先完成阶段 B，才启动同源 Fake ROM 抽取。未提交、未接设备。

## 2026-09-24：Task 8 逐 MAC HTTP 调度通过

`task-8-v2-cycle.md` 已完成。用户纠正当前没有 legacy 设备，任务和测试已去除相关虚构现场；历史 `legacy` 标记仅作为防止误入 v2 协议的兼容字段。Bridge HTTP 周期循环现在按已登记 MAC 选择认证在线目标，逐台经占用 gate 做计划与投递；目标刷新失败不写该台。隔离 `bridge-app` check、3 项状态/目标测试和 `git diff --check` 通过。BLE 机会仍只针对当前设备，Fake ROM/时钟未开始；本轮未提交或接实机。
