# 状态与交接

## 2026-09-29：后台隔离回放证据（部分出口）

不占用桌面的隔离测试已完成：双 Fake ROM + 真实 Bridge 客户端虚拟 24h 共 2,785 事件，两台各连续完成 serial 1–97、都达到第 15 轮，归档 checkpoint 与 SHA 核对通过；另有正式 light 入/离场、Bridge 进程重启、Fake ROM 进程重启的独立烟测。同 seed 两次独立 1h trace 原始 SHA 一致、10 份归档按实例 Bridge ID 归一化后内容/记录字节一致。命令为提权执行 `node tools/run-sync-headless.mjs 86400000`、`1500000 light`、`1200000 restart`、`1200000 device-restart` 和两次 `3600000 periodic`；隔离证据路径和限定范围见 `PROGRESS.md` 最新节。候选 EXE 含一次性 Wi-Fi 完成后的续传修正，默认 EXE 未更新。

未通过的完整出口仍包括 F3 异步等待/延迟页、同一磁盘快照回放、S01–S14 全矩阵及页/完成意图中途强退、U01–U20 全矩阵、最终 Tauri 窗口视觉和必要硬件烟测。不能把上述烟测写成这些出口通过。后续待办只在 backlog C8 维护。

## 2026-09-29：恢复与设备页收束计划（执行中）

以 `PROGRESS.md` 最新节及实际运行数据为准。先对 A0 旧归档 checkpoint 做 MAC、归档字节 SHA、设备 pending/last_completed、client serial 核验；只在确证设备已丢旧批次且归档完整时记带原因的退役记录、原子推进 checkpoint，再由生产 Bridge 开新批次。不伪造设备 ACK，不改旧归档。随后按用户澄清删除设备 Tab 的模板/Profile/字体编辑 UI，另留设备级数据投递许可并只改 `sync_enabled`；修四组详情、null reason、年龄、同步摘要和任务分区。最后完成能运行的 S/U 代表用例、双 target 软件长场景和真实窗口/EXE 检查，逐项记证据与边界。底层服务及显式发布不改；模板 Tab 与数据 Tab 布局不动。此节是计划，不是通过记录。

## 2026-09-28：前阶段提交与设备页基础整理（未部署）

前阶段 sync-v1/协议统一源码已按用户要求提交：`c1ab267 Implement Note4 sync diagnostics and unify device protocol`。随后按 `device-tab.md` 开始设备 Tab 实施，已删除设备页 Codex 余量与全局/无效按钮，改为每 MAC 认证快照及分组采样年龄、同步与电源摘要；PM 只在显式采样时请求设备，服务端先核对公开身份 MAC，失败保留同 MAC 最近成功样本。选择代次阻断 A→B→A 旧响应，切换即清屏，Profile 草稿不被刷新覆盖，发布/恢复等冻结目标 MAC。模板/Profile/字体入口仍暂存设备页，归属迁移布局尚未实施。

`node tools/test-device-page.mjs` 的设备页针对性场景、`cargo test -p bridge-app --bin bridge-app` 42/42（另 1 ignored）、`git diff --check` 已过。浏览器安全策略拒绝本地 `file:` 预览且禁止绕过，因此窗口视觉未验；生产 Bridge EXE 及两台设备均未切换。完整 U01–U20、Fake ROM S01–S14、1.54 ROM 与实机证据仍见唯一待办 backlog C8，ROM 精确值见 `PROGRESS.md` 最新节。

## 2026-09-28：生产协议入口收敛 + Note4 Fake ROM 最小集成（未发布）

生产 `main.cpp` 与宿主 `sim_sync.cpp` 现在都调用 `src/v2_sync_protocol.*` 决定冻结文件格式、begin/page/ack/complete、批次字段及错误；RTC 环与 Flash A/B 存储继续分别共用 `v2_sync.*`、`v2_sync_store.*`。宿主初始化显式 `LittleFS.useDirectory(dataDir)`，不依赖 Bundle FFI 的调用顺序。环境适配仅提供状态采样、虚拟 RTC 持久、随机后缀和 HTTP/BLE 外壳。

Note4 Fake ROM的真实HTTP最小往返和Bridge归档集成通过。具名用例覆盖部分S01/S02/S03/S04/S05/S07/S08/S12；S07修正设备ACK偏移只存RAM的问题，现随A/B元数据持久化。当前协议已切至 `/api/*`，BLE旧marker在副作用前拒绝；Bridge模块/应用层标识部分收敛。Fake ROM bootstrap 43/43，Bridge app 42/42、显式归档1/1、MCP 3/3。Bridge本地8765仅保留/health；固件旧usage处理器、深睡拉取和`/deep`通知已移除，未配置设备改走BLE会合后由正式Plan开网，Fake ROM首个Bundle安装用例已过，实机未验。S01–S14完整矩阵、双机24h、剩余故障切点、设备Tab、实机部署未验收。1.54构建按用户指示暂缓，以Note4为准；ROM路径和精确哈希见 `PROGRESS.md` 最新节；待办只由backlog C8管理。

## 2026-09-28：sync-v1实施合同定稿（仅文档）

Status：文档完成，功能待实现。四份文件为design.md（协议）、plan.md（P0–P7顺序）、task.md（S01–S14可执行断言）、本文件。待办唯一来源仍为docs/roadmap/backlog.md C8。

Result：已固定Bridge→设备HTTP；第15轮due通过BLE sync_open开网，未认证不自主开网；light入/离场完整同步成功清零；正常批次不设总时长截止，90s无进展/三次失败进入明确异常；统一4096B RTC流，begin时生成有界LittleFS冻结副本，Bridge持久页ACK和最终complete收据分开；OTA/Bundle确认分级，镜像采用当前运行分区前N字节SHA256明确合同；能力/owner/恢复/缓存语义已定。

Changes：替换原讨论稿中的“评审时再定”事项，给出精确端点/信封/错误、容量/存储顺序、序号/游标/幂等与异常；P1起即共用固件/宿主逻辑，Fake ROM是主要回归；模板页仅声明归属，不实施布局迁移。

后续范围补充：`device-tab.md` 已作为设备 Tab 的独立实施/验收合同；Fake ROM 接入步骤与故障切点已补入 plan/task。主代理核对当前生产 `main.cpp` 与宿主 `sim_sync.cpp` 后，要求冻结文件格式和 begin/page/ack/complete 的协议决定共用生产 C++ 入口，Fake ROM 仅保留环境适配；该收敛和 dataDir 显式绑定提前到生产端点成型时做。完整 Fake ROM S01–S14 随后执行，最后才实施设备 Tab U01–U20。本段是实施要求，非已通过的源码证据。

Evidence（只读核查，未重跑历史测试）：

- AGENTS.md、PROGRESS.md顶节及Fake ROM D/E/F节、backlog C8、总设计v2 §7/§11。
- src/main.cpp的认证/owner/v2Command、serviceV2Ble、rendezvous、OTA/Bundle收尾；src/v2_command_envelope.cpp、v2_plan_command.cpp、v2_state.h、v2_status_snapshot.*、dev_log.*。
- core平台model/service与app device_runtime/platform/wake_history，设备UI refreshDevice。
- device-sim/src/main.rs、tests/bootstrap.rs、ble/src/lib.rs、tools/fake-rom-runner.mjs；docs/fake-rom-simulator-design.md及归档D/E/F evidence/status/review。
- 当前本地Note4旧产物map：RTC SLOW使用0xF90/0x1E00；当前1.54 map路径不存在。此信息仅用于设计可行性线索，未构建、未证明新布局。
- 仓库git diff --check退出0，仅现有src/ble_bridge.cpp的CRLF提示。四文档当前未跟踪，另用git diff --no-index --check对空文件逐份核查，无空白错误输出（退出1表示存在新增内容）；该检查不是协议测试。

Caveats：

- Clarification required: none。已作合理工程定值，未留下需要用户才能选择的产品语义。
- 资源门槛仍需实现证据：两目标新map与512B余量、40KiB文件预留、实际IDF运行分区字节域、Flash写边界恢复；不通过不得缩容量/降确认或假成功。
- Fake ROM历史D/E/F不证明sync-v1已完成；当前HTTP门控、runner失败模型、诊断generation与新协议均需补接。
- RTC未冻结记录不能跨冷断电保全；gap必须可见。Flash冻结副本不是第二套日志流，正常事件不逐条写Flash。
- 运行分区prefix证明定义明确，不是硬件远程认证；加密读取字节域不可对应时禁止启用该算法。
- 总设计§7仍有旧“截止必关网”描述。实施P6须同步修订，当前仅编辑被授权四文件。
- 未改源码、未启动/停止Bridge、未运行测试二进制、未访问设备、未构建/OTA/提交/再委派。现有RF1与其它未提交工作保留。
