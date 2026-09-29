# 状态与交接

## 2026-09-29：设备页时间投影与 MCP 对等（已部署，完整矩阵待验）

一次认证 Wi-Fi 回报共用公共时间；BLE 只更新实际携带的精简字段及联系时间；PM 整块共用显式成功采样时间，失败尝试另记；公开 owner 与任务/发布/归档保留各自事件来源。页面普通读数已去重，未来时间显示异常而非“刚刚”。新归档 complete 记录真实持久提交时间，旧记录继续显示时间未记录。默认 Bridge EXE SHA256 `DB6A8F95A699B763E738B06ADE6042EBCC18B8C4A405F563FEB2D093E7C8A5AA`；两台 MAC 主视图与各七个只读详情运行核验通过，已保存的 Note4 模板只读验证为 ABI 2 且有效。当前测试结果与旧渲染失败记录的区别见 `PROGRESS.md` 最新节。剩余出口以 backlog C8 为准：U01–U20、Tauri 最终窗口、PM 跨 boot 身份和真实 Plan 发送时刻。

## 2026-09-29：设备页主视图与 MCP 读取对等（已部署，完整矩阵待验）

设备页与 MCP 共用 `bridge/crates/app/src/platform.rs` 的按 MAC 主视图和折叠详情投影；默认 `platform_device_view` 只返回主区，`platform_device_detail` 按单个栏目只读本地缓存。旧 OTA 任务和 Bundle 任务保留在独立历史，最新版本只由显式发布目录判断。默认 Bridge 已安装 SHA256 `E4DE926C…`，现行双 ROM 已按 target 登记为本地最新可用版；两 MAC 运行 MCP 主视图和七栏目通过。测试与未验边界见 `PROGRESS.md` 最新节；U01–U20 完整矩阵和 Tauri 窗口视觉仍待验，不能记作完成。

## 2026-09-29：旧 OTA 任务生命周期收束（执行中）

**产品/数据合同。** 设备页主视图只显示该 MAC 最近一次认证设备快照的当前版本及采样时间、按 firmware target 显式登记且冻结字节的最新可用版本、两者比较结论。未登记发布版或未采样当前设备时明确未知；有两侧精确镜像哈希时优先比较运行分区前发布镜像字节数的 SHA256，同版本不同字节不得显示“最新版”。不同显示升级提示；相同显示最新版；不同且最近针对该设备/目标版的真实推送失败，补最近失败原因。发布目录不是最近 OTA job、工作区 bin 或版本字符串排序；只由显式校验 target/marker/大小/SHA 后登记替换。设备页独立历史栏保留原 job、上传 ACK、精确镜像观察、旧错误与关闭原因，主视图不出现 arm/upload_ack/version_seen_unproven/awaiting_confirmation。

现场两个历史任务仍占每 MAC 的唯一 `ota_jobs` 槽：书桌屏 `ff8a3dc9` 为 `awaiting_confirmation`、无上传 ACK、已请求取消、旧 sync arm 409；Note4 `bde8e156` 有上传 ACK 但仅 `version_seen_unproven`，目前设备已运行另一镜像。隔离实现目标：① 已请求取消的无 ACK 任务退出活跃槽，原记录/错误/冻结身份与关闭原因进入有界持久历史；② 有 ACK 的任务只通过同 MAC/target 的认证运行分区前 N 字节 SHA 判定指定镜像现在是否运行，精确不同的后续镜像使旧目标退出活跃槽，旧安装结果不伪造成功/失败；③ 同版本未证任务在有界认证机会内查询，不自动重刷；④ 新任务替换旧槽前保存唯一历史，幂等 request ID 不重新排队。用户目标是最终运行镜像是否匹配，不要求证明由本次上传造成切换。测试覆盖重启、取消、旧任务不重传/不阻塞、精确哈希同/异、发布目录 target 隔离及 UI。

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
