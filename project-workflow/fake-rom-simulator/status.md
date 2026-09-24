# Fake ROM 与多设备实验时钟状态

## 2026-09-24：Stage C / Task 12c Bundle BEGIN 决策切片通过宿主验证

在 Data `02dd043`、Plan `e997a72` 后，串行抽取 Bundle BEGIN 的已提交请求重放、CRC/长度、live 会话忙碌/恢复、新会话候选状态为 `src/v2_bundle_command.{h,cpp}`，固件与宿主 render FFI 使用同一函数。设备端仍先做 token/owner/session 预检，只有空接收文件成功创建并关闭后才写入新 `v2Rx`；重复 BEGIN 返回现有 offset，绝不延长 deadline。宿主测试覆盖重放/冲突、busy、过期、CRC/大小错误和拒绝时状态不变；`cargo test -p bridge-render --test v2_state` 13/13、`git diff --check` 通过。未接主 Bridge/实机，未构建或烧录 ROM。下一步 CHUNK/COMMIT，仍无可运行 Fake ROM。

## 2026-09-24：Stage C / Task 12b Plan 决策切片通过宿主验证

在已提交的 Data 切片 `02dd043` 后，串行抽取 `src/main.cpp::applyV2Plan` 预检后的 PowerPlan 形状校验、授予上限选择和 ACK 分类为 `src/v2_plan_command.{h,cpp}`；宿主 render FFI 编译同一 C++ 实现。设备端认证和正式计划接受后的 deadline、模式持久化、日志副作用保留原顺序。同 ID 完全重放保留原截止；冲突、旧 ID 和非法形状不改计划。宿主测试覆盖 provisional 300s、常规 600s、最低 30s、重放、冲突、旧 ID 和 sleep；`cargo test -p bridge-render --test v2_state` 12/12 通过，`git diff --check` 通过。未接实机或主 Bridge，未构建/烧录 ROM。下一步继续 Bundle、Activate、claim 命令路径；仍无可运行 Fake ROM。

## 2026-09-24：Stage C / Task 12a Data 决策切片通过宿主验证

在独立 `codex/fake` worktree（HEAD 与 `bridge-multi-instance-2026-09-24` tag 均为 `6d131ac`）开始 Stage C。ROM 文件已交接；只有一名 6-luna high 代理顺序编码。`src/main.cpp::applyV2Data` 的预检后决策和 ACK 分类已抽入 `src/v2_data_command.{h,cpp}`，与宿主 render FFI 编译同一 C++ 实现；设备端原 token/owner/session 预检、首次接受后的 fields/checkpoint/cache/显示顺序保留。拒绝与重放不触发副作用，未配置仍返回原 ACK。初审发现 ACK 的 result/error 混用后已修正。

宿主 `cargo test -p bridge-render --test v2_state` 11/11 通过，`git diff --check` 通过。测试通过临时 `CODEX_STATUS_ARDUINOJSON` 指向主工作树已有的只读 ArduinoJson 头文件；没有修改主工作树、运行中的 Bridge 或设备。本切片未构建/烧录 ROM、未提交。它仍不是可运行 Fake ROM：认证、owner、存储、显示副作用、Plan/Bundle/Activate/claim 和 HTTP/BLE 入口尚未在宿主共享。下一步由主代理设计 Stage C 后续命令路径，继续串行实现。

## 2026-09-24：Task 9/10a/10b 已提交，Task 11a/10c/11b 完成宿主验证

显式 v2 端点登记与多目标 BLE 一次扫描/逐 MAC 会合已提交 `c82487c`。随后按串行顺序完成 BLE 扫描/GATT 时间线、UDP 已登记目标的认证改址、UDP/HTTP/PowerPlan/Data/ACK 结构化日志。真实 ROM 会发 UDP；v2 广播仅为地址线索，不再改全局选择/IP/推送/BLE 标志，也不构成 BLE 会合前提。无 legacy 设备参与本轮验收。

Task 11a 隔离 `bridge-ble` 11 项测试及 app check 通过；Task 10c app UDP 2 项、身份 1 项测试及 app check 通过；Task 11b 隔离 `bridge-core` 全套与 app UDP 4 项定向测试通过，`git diff --check` 通过。最终审查发现并修复 HTTP 非成功错误误把请求 JSON 拼入错误文本的问题，POST/status/Bundle chunk 现只保留状态码和必要 offset。未启动生产 Bridge、操作实机或触碰 ROM；多设备和日志的现场验证、Fake ROM/实验时钟仍待完成。

Task 11a/10c/11b 已提交 `d65091f`。Stage C 首条同源 Data 决策切片的主代理设计见 `task-12a-data-command-slice.md`；现只读核查已提交 ROM 源码，等待另一代理的 `src/` 工作交接后才开始编码。

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

## 2026-09-24：Task 9 显式端点登记通过

前一批 Task 2–8 已提交 `d3ba464`，未包含并行 ROM 改动。`task-9-register-endpoint.md` 新增 `platform_device_register_v2`：显式输入 MAC/IPv4(:port)，状态页与认证状态均匹配目标且具 v2 能力后才登记，不自动 claim 或推送。隔离 app check、app 3 项登记测试、MCP 1 项测试及 `git diff --check` 通过。未部署、未访问设备；本 Task 尚未提交。下一项是 BLE 多设备机会，需一次扫描从登记集合选目标，不能对每个 MAC 串行扫描 3 秒而漏掉短会合窗口。

## 2026-09-24：Task 10a 一次 BLE 扫描入口通过

`task-10a-ble-any-target.md` 完成宿主验证：BLE `connect_any` 一次扫描所有目标广播候选，连接后用完整 info MAC 授权；单目标 `connect` 复用新实现。主代理评审消除了重复扫描循环。隔离 `bridge-ble` 7 项测试、`bridge-app` check 和 `git diff --check` 通过。尚未接 app 多设备机会、未接硬件或提交。

## 2026-09-24：Task 10b 逐 MAC BLE 会合通过

`task-10b-ble-cycle.md` 已接入 app：从登记记录生成 v2 BLE 候选，单次扫描可匹配任一目标；连接后再次核对登记与完整 MAC，按 MAC 记录 55 秒尝试节流。评审去除了不存在的 legacy 实机测试，并确保有已登记 v2 目标时 UDP 通知不转入旧长扫描。隔离 app check、2 项定向测试及 `git diff --check` 通过。未连接设备、未部署或提交。下一步核对多设备发现和 Bridge 阶段 B 的剩余入口。
