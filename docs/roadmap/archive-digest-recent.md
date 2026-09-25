# 近期专项归档摘要（2026-09-23 ~ 2026-09-25）

> 用途：`project-workflow/<name>/` 归档后的检索入口。每节保留：做了什么、结论/关键证据、还欠什么。
> 归档目录：`docs/history/workflow/<name>/`。
> 摘要编制时的仓库事实（用于判断「未完成」是否仍然有效）：`master` 固件版本为 `0.18.23-note4-b`（`src/main.cpp`），现场 Note4 `7C4FADB93408` 已由 `0.18.21-note4-b` OTA 升至 `0.18.23-note4-b`（PROGRESS.md 最新节）。摘要未修改任何既有文件。

## note4-bridge-publish
- **目标**：Bridge 侧支持 Note4 资产/字体发布——设备能力校验、Profile 绑定 render target 与显式字体、CSFN 导入、冻结 manifest/对象与差量规划、只读预检、显式发布，以及版本化 asset-publish 协议。
- **状态**：部分完成。Bridge 宿主侧规划/UI/MCP 已完成并有测试；版本化资产传输（`protocol.md`）仍是**待双方确认的草案**；实机侧只完成了内建字体的完整 Bundle 首次显示。
- **关键结论与证据**：
  - 实机成功：`codex-status-a` 以 ABI1 完整 Bundle 安装并显示。修掉冻结 Bundle 顶层缺 `bridge_id` 后收到 COMMIT ACK `applied/displayed`、`active_context_id=1168aabe4a1d3ab3`，随后状态 `configured=true`、`commit_seq=2`、`committed_job_id=b462a509`、`active_template_id=codex-status-a`；照片 `artifacts/note4-first-bundle-display-20260923.jpg`。该 `commit_seq=2` 与「一个 job」不符，设备侧正在补持久 job id + payload CRC 幂等；Bridge 不再重发该 job。
  - 同一冻结作业 `b462a509`（27086 B，CRC `e60188f2`）在重启后从持久非终态 `bundle_jobs` 恢复。历史 CRC 字段仍是加 `bridge_id` 之前的旧载荷哈希，**不是**已发送 Bundle 的权威哈希；后续 job 才记录重新封装后的 CRC。
  - 故障链的证据：Windows 10054 落在第 4 个 4096 B `/v2/bundle/chunk`（offset 12288），降 1024 B 后推进到 offset 14336 超时（10060），根因是设备未启用 OPI PSRAM（`psram_free=0`、`heap_max_alloc=15348`）。`0.18.18-note4-b` 启用 PSRAM 后 `psram_free=8,351,272`，接收 27086 B 全程成功。
  - 宿主验证：隔离 `CARGO_TARGET_DIR` 下 `cargo test -q -p bridge-core -p bridge-render -p bridge-mcp -p bridge-app --no-fail-fast` 最终 126 项通过（`v2_client.rs` 仅余既有 dead-field 警告）；400×300 模板 18 区域、200×200 `quad` 13 区域的 JSON/编译渲染逐像素差 0。
  - 协议未实现：`protocol.md` 明确标注 pending，所提 `/v2/asset-publish/{begin,object,commit,cancel}` 路径是**提案**；设备当前 `CT_ABI=1`、48 KiB/8 字体上限、无 manifest/object 端点、无 CTP1 资产字体引用。
  - 显式 light PowerPlan：MCP/UI light 走显式请求路径，离线返回 `queued`、在线 HTTP ACK。实机在线验证 plan 52（`applied`，`accepted_remaining_s=600`，状态回读 583 s 剩余）；电池深睡下 plan 54 经 BLE 会合 `applied`，`reset=deep-sleep`、`wake=timer`。
- **未完成 / 遗留**：
  - 版本化资产发布协议：端点命名、token/session 框架、`max_chunk_bytes`、规范 JSON 兼容、ACK 存储格式——**仍有效**，待联合确认。
  - 设备侧工作：显式能力字段与鉴权 committed-manifest 摘要、CTP1 资产字体引用与设备字体查找/时钟/区域、ABI 升级、去掉临时 48 KiB/8 字体上限、manifest/object BEGIN·offset chunk·COMMIT、持久 ACK、错误码、GC 与断电恢复——**仍有效**，全部未实现。
  - Bridge 侧版本化 HTTP 客户端：**仍有效**且有意的未实现（阻塞于上一条）。
  - 新增字体发布与 Note4 实机资产发布验收：**仍有效**，未就绪；A/B ROM OTA 不能替代。
- **归档判定**：`可归档（只读历史）`。宿主工作已验收、协议草案与未完成清单完整记入本摘要；`protocol.md` 是仍需检索的合同，归档后按本摘要索引取用。

## note4-buttons
- **目标**：确认 Note4 深睡唤醒键行为，并把 PGUP/GPIO39 定为「仅清醒时切换模板」的按键。
- **状态**：仅设计/调查完成。**源码未实现 PGUP 路径**（`src/main.cpp` 中 `nextTemplate()` 只挂在 GPIO0/BOOT 长按路径上，全仓无 GPIO39/PGUP 代码）。
- **关键结论与证据**：
  - 现场设备仍是 `0.18.20-note4-b`（`ota_0`），`0.18.21` 面板电源候选**从未刷入**。
  - 14:33–14:34 +08 直连无代理 GET `192.168.3.177` 的 `/status.json`、`/history`、`/log` 均 200；当次启动仅 `wake=power-on`、`last_wake_code=0`，此前 ENTER 按下无法从该启动还原（打开串口本身会复位板子）。
  - `/log` 反复出现 `[clk] window write failed; full refresh required`，状态 `epd_busy_fails=8`：能证明存在显示更新失败，**不能**证明按键唤醒失效。
  - Note4 侧 ENTER 映射 GPIO0，`armWakeSources()` 对 GPIO0/GPIO18 配 active-low EXT1；**未发现源码级唤醒配置缺陷**。
  - 用户决定：GPIO39(PGUP) 不作深睡唤醒源，仅清醒时切换模板；ENTER 是必需的深睡唤醒键；GPIO18(PGDN/power) 维持原行为。
- **未完成 / 遗留**：
  - PGUP/GPIO39 的 active-low 输入 + 去抖 + 单次 `nextTemplate()`：**仍有效**，计划已写、代码未写。
  - 「ENTER 是否唤醒 MCU」：**仍有效**，未取证。后续做法是观察到真实深睡下的 ENTER 按下后立即读无代理 `/status.json` 与 `/history`（不打开 COM5），`wake=ext1` 即可把按键识别与刷新失败分离。
- **归档判定**：`可归档（只读历史）`。未实现的 PGUP 需求已记入本摘要，属可选增强而非阻塞项。

## note4-icon-correction
- **目标**：只改 400×300 模板图稿与状态选择——Bridge/Wi-Fi 用互斥整图、BLE OFF 留空、深睡用原 `sleep-20` 图稿、缩短日期时间间隙、电量右对齐。
- **状态**：已完成（模板本地改完，未发布）。未改 ROM/Bridge、未重启服务、未操作设备、未提交。
- **关键结论与证据**：
  - Bridge/Wi-Fi 改为按状态绘制**完整位图**而非在 on 图上叠加 off 图；Bridge 判定用 `device.offline_mins` 是否存在，Wi-Fi 判定用 `device.state`。
  - 日期/时间可见间隙由约 12 px 减到 6 px（`device.now` x=75 → 69）；BLE OFF 单元留空；深睡在该单元显示恢复后的原 `sleep-20` 位图（与原图字节一致）。
  - 根因（推翻「电池文字被猜着上移」的旧判断）：`ntthin18` 行高 26 px，而文本区域只有 20 px，渲染器把整行居中导致字形墨迹上移 3 px。区域改为 `[344,5,44,26]`，元素自身 y 仍为 5。
  - 受控电量实验：旧区域 `--%` 墨迹 y=9..21（中心 15）；26 px 区域 y=12..24（中心 18），电池外框 y=12..23（中心 17.5），对齐成立。
  - 五种状态（BLE ON/OFF、WIFI OFF、Bridge 离线、深睡）JSON/编译/序列化渲染逐像素差 0，含空 BLE OFF 单元逐格比对一致；模板 49 操作、14 位图资源在 ABI2 上限 64/16 内。
- **未完成 / 遗留**：
  - 修正后的模板**从未保存到 Bridge 模板库、未发布、屏上从未出现**：**仍有效**（实机视觉验收未做）。
  - `make-template.py` 早于当前 96 px 版式，不得用它重新生成本规范 JSON：**仍有效**（操作约束）。
- **归档判定**：`可归档（只读历史）`。改动只在本仓库模板 JSON 与图稿源码，未发布状态已记录。

## note4-live-sync
- **目标**：让 Note4 用真实 Codex 数据同步，并把 `deliver(ctx, mac)` 修正为按目标 MAC 取该设备自己的 IP；深睡分钟时钟在无保留窗口时以本地渲染兜底。
- **状态**：已完成（有实机证据）。
- **关键结论与证据**：
  - Note4 Profile 原为 `sync_enabled=false` 且四个配额绑定指向测试源 `static1`；经正常服务改为 `codex` 并开启同步后，最终模板发布后设备数据 ACK 达 **16/16**（`data_seq`/applied 均 16，`display_state=displayed`，新 Bundle job `4905c76c`）。
  - 多设备路由缺陷已修：`platform::deliver(ctx, mac)` 原先用全局选中设备拼 HTTP 链路，会把 Note4 的决策发到 1.54 地址；现按请求的规范化 MAC 取该设备记录的 IP，未知 MAC/缺 IP 拒绝。1.54（`70041DD7A340`）仍独立登记。
  - 升级：鉴权单次 OTA 安装 `0.18.20-note4-b` 到 `ota_0`，保留已装 Bundle 与作业；ROM `artifacts/codex-status-0.18.20-note4-b-abi2.bin`，SHA256 `DDA7F214127814FC93B03B06D4C50818D77847CCCDF18938701E12C69C3F094B`，证据 `artifacts/note4-ota-01820-b-abi2.json`；设备报 ABI2、`partial=true`。
  - 驱动参照 ZECTRIX NOTE4 B/W 参考实现的 SSD2683 OTP 局刷序列，**要求先有有效整帧基线**；Note4 能力已宣告 `partial=true`。
- **未完成 / 遗留**：
  - 用户观察到时钟更新仍在做**整屏刷新**：本任务按要求**只记录不修**，**仍有效**、未定位到具体唤醒路径（深睡断面板电导致局刷基线失效是候选原因之一）。后续面板电源专项的帧缓存就是针对它，但未实机验证。
  - 1.54 在末次检查时其 Bridge coordinator 处于 pending/full-sync-due，未被用作 Note4 投递证据：**仍有效**，该状态从未对账。
  - 驱动层实机验收（干净整帧基线、light 模式分钟局刷、深唤醒兜底、周期性干净全刷）：**仍有效**，未做。
- **归档判定**：`可归档（只读历史）`。唯一未决项（时钟整屏刷新）在后续专项与本摘要中持续可查。

## note4-panel-power
- **目标**：为 Note4 增加面板电源模式（`keep` / `off_cache`），在深睡跨过面板断电时保住已验证的旧帧基线以支撑局刷，并保持 1.54 不回归。
- **状态**：部分完成。源码与两种构建完成；`0.18.21-note4-b` 面板模式候选 ROM **从未刷机**，实机功耗/局刷从未测量。
- **关键结论与证据**：
  - `pm/panel_pwr` 持久化在 NVS，默认且当前选择 `keep`；`off_cache` 保留断电实验。写路径为 token 门控 `POST /diag?panel_power=keep|off_cache` 与串口 `panelpower …`，读路径为 `/status.json.panel_power_mode`。两种模式都在刷新后关闭 SSD2683 内部高压；深睡 GPIO6 保持/释放跟随所选电源轨。
  - 进入深睡时把当前 15 KB 显示帧写入 LittleFS 一次，帧哈希镜像到 RTC；深唤醒校验缓存并叠加 RTC 时钟窗口像素后再灌入驱动影子。薄分钟唤醒不写闪存；缓存缺失/损坏则维持原来的全刷兜底。
  - 构建证据：`0.18.21-note4-b` 终版；候选 ROM `artifacts/codex-status-0.18.21-note4-b-panel-modes.bin`，1,742,144 B，SHA256 `917FAFADA6886A797FC9C2390990286A90E8DB173BF1DF1A50E3778879C9FD6A`。1.54 回归构建通过（首次失败源于中断构建遗留的单个零字节生成对象）。
  - 后续源码自愈修复（条件性缺陷，不是在跑 0.18.20 的诊断）：缓存恢复失败时 `rtcNote4FrameHash` 可能残留，导致下次深睡跳过重写同一帧。`note4RestoreFrameBaseline()` 改为先清标记，全部缓存与时钟窗口校验通过后才恢复。隔离 worktree `a695` 顺序构建 Note4 与 1.54 均退出 0，`git diff --check` 通过；Note4 B `firmware.bin` SHA256 `A1A4F7E43FF04DB77F7F52F9CFB7DFD78AEB0E2E721DB5343577E4723FF3FE51`（1,742,144 B），1.54 回归 `3DD4AFE7F064219B3635398130C03F4CF45F0685C7FD8DB19D44063EE5897FA3`（1,728,368 B）。该次**未复制发布 ROM、未升版本号、未刷机**。
- **未完成 / 遗留**：
  - 自愈修复**既未发布也未刷机**，且从未证明现场 0.18.20 会命中该失败模式：**仍有效**（源码级修复，无实机证据）。
  - 台架测量全部缺失：电流消耗、分钟局刷波形、鬼影、重复按键/定时唤醒：**仍有效**。
  - 面板电源候选 `0.18.21-note4-b-panel-modes.bin` 未刷入：**已作废（被取代）**——现场已按 PROGRESS.md 由 `0.18.21` OTA 到 `0.18.23-note4-b`；该候选只作历史证据。
  - 缓存自愈的构建产物只留在隔离 worktree 的 `.pio/build/`，未释放 ROM：**仍有效**（如需实机验证须重新构建并记录哈希）。
- **归档判定**：`仍需保留在工作区（原因：面板电源两种模式与缓存自愈均无实机测量，且注意 `0.18.21` 候选已被 `0.18.23` 取代）`。可在 `PROGRESS.md` 未决清单保留一行指针后归档。

## note4-template-96
- **目标**：400×300 `codex-status-a` 全部用量数字改用 96 px 内置 `ntreg96`，weekly-only 居中并去掉可见 `RESET` 字样，连接状态图标随设备状态变化。
- **状态**：已完成（已实机显示）。
- **关键结论与证据**：
  - 新增 Note4 专用内置字面 `ntreg96`；200×200 目标校验**拒绝**该字面，现有字体索引保持不变。所有用量（双桶与 weekly-only）均使用它。
  - 用户批准把编译模板资源上限由 8 提到 **16**（含 Bluetooth-off、Wi-Fi-off、Bridge-off、`zzz` 深睡四对半图），操作上限由 48 提到 **64**，同属 ABI2；该模板用 57 操作、16 资源。ABI1 仍可用于 Bridge 发现/取数，发布要求 ABI 精确匹配。
  - 六组 400×300 宿主预览的 JSON/编译/序列化渲染逐像素差全为 0；Bridge 模板库登记源 CRC `11455c65`、编译 CRC `442007ea`；MCP 保存在显式发布前报 `published=false`。后续完整 Bundle 发布被 Note4 回 ACK `displayed`。
  - Bridge UI 在 MCP v2 保存后收到 `templates-changed`，并用已缓存 usage 信封做预览，修掉了保存后预览陈旧/配额为空的问题。
- **未完成 / 遗留**：
  - 用户观察到的时钟更新整屏刷新：**已作废（本专项范围外，由 note4-live-sync 记录、保留原样）**；根因追踪归入面板电源/时钟窗口专项。
- **归档判定**：`可归档（只读历史）`。

## clock-window-retention
- **目标**：修掉 1.54 分钟刷新在局刷与全刷之间交替的问题——v2 会合时不再无条件丢弃 RTC 时钟窗口像素。
- **状态**：部分完成。源码已改、1.54 构建通过；**未刷机、未重测**，用户观察未被接受为期望行为。
- **关键结论与证据**：
  - 根因：每次会合启动的 `v2ActiveLoad()` 会重算同一时钟区域并**无条件清空** `clkPixelsValid`，于是 `v2RendezvousClockRender()` 走无基线全刷兜底；而薄时钟唤醒发生在模板重载之前，仍能用保留像素。v2 会合周期 60 s、时钟对齐分钟边界，两者因此交替。
  - 修复：`src/main.cpp` 仅在 v2 context 与时钟区域完全一致时保留 RTC 时钟像素，并在模板成功渲染后抓取实际显示窗口。冷启动、新 context、区域变化或显示失败仍作废基线并保留全刷兜底。
- **未完成 / 遗留**：
  - 1.54 修复**未刷机**，现场该设备运行时行为未知：**仍有效**。
  - 用户所见的交替现象**未被重测确认已消失**：**仍有效**。下次硬件检查需把每分钟的 `/history` 唤醒类型、时钟 tick 数与实际刷新类型对应起来，并验证 context/模板变化后的全刷兜底。
  - 本次中断的 Note4 构建：共享 PlatformIO 包已重装、未完成隔离目录已清除：**已作废（环境处理完成）**。
- **归档判定**：`仍需保留在工作区（原因：未被刷入 1.54 的源码修复 + 具名未复测的用户现象）`。归档时须在 `PROGRESS.md` 留一行「1.54 时钟窗口保留修复未刷机、未复测」。

## bridge-multi-device-ui
- **目标**：以 `render_target` 为族键的多设备 Bridge——新增「族 ▾」下拉、族 Profile 草稿、把 v2 Profile 编辑迁到模板页、按 MAC 路由并新增「推送到设备」子菜单（只读预检 + 单台显式发布）。
- **状态**：部分完成。族下拉与统一族 Profile UI 已落地并运行；**按 MAC 的发布路由与统一族发布菜单仍是待办**。
- **关键结论与证据**：
  - `FamilyProfile` 模型、持久化与 Tauri/MCP API 已落地。用户多次纠正后最终统一：模板页两族共用族 Profile、旧版行视觉与同一套 CRUD/拖拽/预览/启用逻辑；最多 8 项、无初始项 UI、启用子集显式保存、零启用项显示「暂不同步」；独立模板库卡片已删除但添加模板弹窗仍显示按 `render_target` 精确渲染的缩略图。**设备 Tab 的 v2 编辑器未移动**（`design.md` §4 的迁移安排已被用户后来要求的窄范围修正取代——以后者为准）。
  - 无损迁入已获批准并执行：旧 `profiles.json` 的 3 份配置按原顺序/启用标志导入 1.54 族；Note4 已有 `default` 草稿保留未覆盖；按族完成标记防止删除后重启复活；迁入前备份在 `artifacts/family-profile-migration-20260923-224900/`。
  - 新 Bridge 曾构建并后台运行（主进程 PID 46796、watchdog PID 15716，MCP 端点监听）；内联 JS 语法、Rust 构建、`git diff --check` 通过；未对设备发布或刷机。
- **未完成 / 遗留**：
  - 按 MAC 的多设备发布路由与两个族的统一推送项（当前暂时禁用）：**仍有效**，本专项核心需求未完成。
  - 「推送到设备」子菜单的 0/1/多设备、同族多台、跨族、离线、被占用、旧任务进行中的逐项验收：**仍有效**，未做。
  - 族草稿 → 目标设备 Profile 的冻结/预检/确认发布事务（`family_publish_preview` / `family_publish_checked`）：**仍有效**，未接通。
  - 两台实机核对族联动、独立发布与恢复，并更新 `PROGRESS.md`：**仍有效**，未做。
  - v2 编辑器是否迁到模板页：`design.md` §4 要求迁、用户后续要求保持原布局，**存在文档冲突，以后者（不移动）为准**；若归档后要恢复迁移须重新决策。
- **归档判定**：`仍需保留在工作区（原因：多项核心需求（MAC 发布路由、推送子菜单、发布事务、实机验收）未实现）`。

## bridge-multi-instance
- **目标**：允许并存多个命名 Bridge 实例（独立锁/数据目录/端口/owner ID/托盘形状），并在 UI 中列出两台设备、按所选 MAC 显式发布。
- **状态**：部分完成。启动器与实例隔离已完成并验证（提交 `3cfc8b8`，本地附注 tag `bridge-multi-instance-2026-09-24`）；UI 侧多设备选择与族推送已接线；**实机发布未做**。
- **关键结论与证据**：
  - 主 checkout 拥有该改动：`tools/start-bridge.ps1` 新增 `-Instance`、`-Port`、`-McpPort`、`-IconShape square|circle|diamond`、可选精确 `-DeviceMac`/`-DeviceIp`；命名实例必须同时给两个端口，脚本校验监听、按实例记录 PID 并确认所启进程确实占用其 HTTP 端口；默认调用仍是 8765/8766 与原 PID/日志名。
  - Windows 进程锁按实例名；命名实例数据目录 `<exe>/instances/<name>/data`，owner ID 为 `<原 4-hex host ID>-<instance>`（在设备 32 字符 claim 上限内），有形状可选的托盘底色；命名实例**不绑**固件固定 UDP 通告端口 8767，只走精确 MAC 的 HTTP/BLE；其自启动菜单项被禁用，不能改默认注册表项；watchdog 重启继承实例环境。无需改 ROM 或协议。
  - 验证：`cargo check --offline -p bridge-app` 通过；`cargo test --offline -p bridge-app --bin bridge-app` 23 项通过（含 claim-ID 长度与托盘形状检查）；隔离 `cargo build --offline -p bridge-app` 通过。两个假 MAC/loopback 探针实例同跑：`probe` 8875/8876（PID 46548）、`probe2` 8877/8878（PID 9160），默认实例仍占 8765/8766（PID 27552）；重复调用启动器只返回原 PID，同名直启被进程锁拒绝；两个探针 watchdog 与应用事后均已停止。
  - 默认 Bridge 已用新可执行文件重建并重启（最终 PID 11644）；UI 现列出两台已登记设备并标记当前所选 MAC，平台状态卡跟随该 MAC，族 Profile 菜单的推送动作会匹配同一已登记的 v2 设备、保存为其 Profile 并对所选 MAC 显式发布。Node 语法与模拟发布流程通过。
- **未完成 / 遗留**：
  - **实际模板发布与 OTA 未执行**，因为两台设备的 HTTP 端点均超时：**仍有效**（多设备发布路径只经过模拟验证）。
  - 现场两台设备 HTTP 不可达这一事实本身：**仍有效**，后续任何发布验收前须先恢复可达性。
  - Note4 `0.18.21-note4-b` 候选未刷：**已作废（被取代）**——现场已升到 `0.18.23-note4-b`。
  - 1.54 时钟修正仅有构建结果、没有单独命名的发布 ROM：**仍有效**（见 clock-window-retention）。
  - 临时 Git worktree 注册已移除，但其空目录仍被当时任务进程占用：**仍需单独确认**是否残留。
- **归档判定**：`可归档（只读历史）`。启动器能力与验证证据已完整记录；剩余项（实机发布、设备不可达、1.54 ROM）在本摘要其他节同样有记录。

## bridge-family-sync-preservation
- **目标**：族发布不得覆盖目标设备的 `sync_enabled` / `full_sync_s`——这两个值只能来自该设备 Profile 或平台默认。
- **状态**：部分完成。源码已修，**未构建进运行中的 Bridge**；运行态仍存在会覆盖用户设置的风险。
- **关键结论与证据**：
  - 只读状态检查：1.54 族草稿在 2026-09-23 22:50:11 +08 迁入时为 `sync_enabled:false`；模板页 2026-09-24 14:42:49 +08 的发布把该隐藏值复制进 1.54 设备 Profile。持久记录**无法证明**用户此前是否刻意选择过设备原值，即该流程会静默覆盖。
  - `confirmFamilyPublish()` 改为从所选目标设备 Profile 复制 `sync_enabled` 与 `full_sync_s`，仅在没有设备 Profile 时回退默认 `false` / `3600`；仍按原顺序保存模板 ID/绑定再显式发布 Bundle。浏览器脚本解析、定向的 sync-on/sync-off/无 Profile 三种模拟与 `git diff --check` 通过。
  - 用户随后显式要求 1.54 `sync_enabled=true`：运行中的 Bridge MCP `profile_save_v2` 只保存该设备 Profile 并返回 `published:false`，回读确认 true，且保留 `mini,quad`、绑定与 `full_sync_s=3600`；族草稿仍为 false。**没有任何设备 ACK**（HTTP 不可达、BLE 会合失败、旧 Bundle 作业仍在活动）。
- **未完成 / 遗留**：
  - 源码修复**未构建、未加载进运行中的主 Bridge**：**仍有效**，且当前运行态是「下一次从模板页发布就会把 false 覆盖回设备」的危险组合——用户显式设置的 `true` 可能被撤销。这是本专项最需保留的一条。
  - 上一次族发布造成的设备 Profile 覆盖是否与用户原意一致：**无法判定**（持久记录不足）。
  - 设备 ACK 与同步实际生效验证：**仍有效**，未取得（设备不可达）。
- **归档判定**：`仍需保留在工作区（原因：源码修复未部署，运行中的 Bridge 会在下次族发布时覆盖用户的 sync 设置）`。至少在 `PROGRESS.md`/`next.md` 留高优先级一行。

## bundle-v3
- **目标**：以 `bundle_format: 3` 的二进制容器（`CSB3` 头 + 规范 JSON manifest + 连续原始编译对象）替换 v2 中十六进制编码的 `compiled.binary`，把 1.54 的 `mini`+`quad` Bundle 从 55,347 B 降到约 16 KB。
- **状态**：仅设计（2026-09-24 设计决策）。本目录只有 `design.md`，没有实现，也没有 status/plan。
- **关键结论与证据**：
  - 实测问题：冻结的 1.54 `mini`+`quad` Bundle 为 55,347 B，其中两个 `compiled.binary` 各 22,666 个十六进制字符（合计 45,332 字符），而每个 fixed-layout render plan 只有 11,333 原始字节；`mini` 的 98.6%、`quad` 的 90.5% 字节为零，模板源合计约 5.6 KB。
  - 容器设计：`CSB3` 定长头（格式、manifest 长度、对象长度、整体 CRC32）+ 规范 UTF-8 JSON manifest + 连续原始对象；**不需要** hex/base64/multipart 或第二条通道，因为现有 `/v2/bundle/chunk` 已能把任意字节流写入 LittleFS。`compiler_abi: 2` 在渲染计划语义与打包元素布局不变时继续有效；`bundle_format: 3` 必须由鉴权能力字段宣告，Bridge 只对 v3 设备发 v3。
  - 对象编码 `ct-dense-v1`：`CTD3` magic、ABI、op/req/resource 计数、源 CRC32、模板 ID(17)、恰好使用的 `CtOp`(118B)/`CtReq`(84B)/`CtResource`(66B)、对象 CRC32。预期 `mini` 约 875 B、`quad` 约 5,197 B。
  - 升级规则（用户决定）：v3 ROM 视 v2 前 Bundle 为不存在，**不迁移、不激活**，等新发布的 v3 Bundle；Bridge 必须把冻结的 v2 作业标为 stale 而不是对 v3 设备重试，并让当前 1.54 设备在 v3 ROM 更新后走这条路。
- **未完成 / 遗留**：
  - 全部实现：**仍有效**，未写一行代码。`design.md` 未记录状态，其前言称「与当前 v2 Bundle OOM 修复分开」——该 OOM 修复已并入后续固件线（`src/bundle_store.*`、`0.18.23`），但**本设计本身未被实现**。
  - `roughly 16.2 KB` 是**估算**，必须先由规范编码器报出精确长度再发布：**仍有效**。
  - 验收全未做：双目标 dense 对象 goldens、畸形长度/偏移/重叠/截断/CRC/计数/ABI/target 拒绝、各暂存与提交点的复位仿真、1.54 与 Note4 实机发布及 wire 长度与峰值堆测量：**仍有效**。
- **归档判定**：`可归档（只读历史）`。纯设计文档、无代码与现场状态、无未完成构建产物；实现时须整份取回作为合同。

## wake-contact-trace
- **目标**：把两台设备的**一次物理唤醒记成一条**取证记录（BLE/Wi-Fi/命令/回复各阶段与耗时），让 Bridge 按 MAC + 唤醒序号 + 请求 ID 对照自己的扫描/连接/投递日志，回答「这次通信停在哪一步」。
- **状态**：部分完成。固件记录格式与 Bridge 历史持久化已实现并合并（现场固件已是 `0.18.23`，`bridge/crates/app/src/wake_history.rs` 存在，`tools/wake-contact-trace.mjs` 存在）；**实机三十次会合的验收从未执行**。
- **关键结论与证据**：
  - 记录格式：固定长二进制、目标 `sizeof <= 48 B`，不存字符串/token/完整 MAC/JSON；四组字段为身份（`wake_seq`、短请求校验值）、唤醒（cause、EXT1 位图摘要、计划/RTC 时刻、启动时间来源）、阶段（无线启动…首个 HTTP 结果，未经过用哨兵）、结果（`thin/answered/advertising_start_failed/no_connection/no_command/wifi_no_ip/http_failed/interrupted` 等互斥终态 + 原始错误码 + 总清醒时长）。
  - 存储：复用现有 RTC `/history` 环形缓冲**替换**逐事件记录，不并存第二份 RTC 日志；首版 64 条 × ≤48 B = 最多 3,072 B，旧环 120 × 16 B = 1,920 B，净增最多 1,152 B；当前 7,680 B RTC SLOW 链接区中 Note4 占 2,764 B、1.54 占 2,312 B，按上限替换后约占 3,916/3,464 B，**必须用两种构建的 map 重新核对**。
  - 关联键定为「已认证 MAC + `wake_generation` + `wake_seq`」（RTC 断电会重置 `wake_seq`，故新增 `wake_generation`，深睡保留）；`request_id` 完整值只留在 Bridge 日志、设备只存短校验值。`/history` 保持增量（`since=<seq>`）与数组外形，每行含 `format:2`、`wake_generation`、`seq`、`wake_type`、`awake_ms`、`result` 与阶段/耗时；`/status.json`、BLE INFO、v2 ACK 同步暴露当前 `wake_generation`/`wake_seq`；无代际字段的旧 ROM 只能做不完整关联。
  - 阶段时间只在状态真实发生处写入；设备只能证明「回复已交给本地 GATT 层」，不能自证 Bridge 读到 ACK，该结论必须用 Bridge 日志补齐。RTC 断电即清空，本轮不把阶段写 Flash/NVS（避免磨损与干扰会合时序）。
  - Bridge 侧持久化决策：只在认证 MAC 后读设备历史；每台设备首连立即同步一轮，完整成功后至少间隔 15 分钟才启动下一轮，上轮未取完则只续取未落盘序号；PowerPlan ACK 后用剩余 BLE 时间分页请求 `history(since, limit)`，同步错误只影响诊断游标；记录按 MAC + `wake_generation` + `seq` 写入 PC 的 `data/platform` JSONL，`complete:false` 快照可追加修订、`complete:true` 终版才推进完成游标；RTC 环覆盖造成的序号缺口单独记录。
- **未完成 / 遗留**：
  - 实机验收：两台设备各至少 30 次 timer 会合 + 一次按键唤醒 + 一次故意关闭 Bridge 的超时，抽查能用设备记录与 Bridge 阶段日志重建完整时间线：**仍有效**，未做（计划明言「不把构建通过写成实机通过」）。
  - 两种构建在新记录上限下的 RTC map 容量复核对账：**仍有效**（计划要求的核对项）。
  - 计划中写的「统一版本号 `0.18.22`、后缀 `-note4-b`/`-bw`」：**已作废（被取代）**——固件实际已到 `0.18.23-note4-b` / `0.18.23-bw`，勿按 0.18.22 找 ROM。
  - 在尚未连接 Bridge 前断电、或 RTC 环已覆盖而 Bridge 从未读到的记录不可恢复，只能报缺口：**仍有效**（已知边界，非待办）。
  - `tools/estimate-power.mjs` 的 `format:2`/`wake_type`/`awake_ms` 读取已落地：**已作废（已完成）**——本摘要核对源码时确认该脚本已支持新格式。
- **归档判定**：`可归档（只读历史）`。字段合同与存储/容量决策是唯一仍需检索的内容，已完整摘入本节；实机验收项须在 `PROGRESS.md` 留一行待办。

## fake-rom-simulator
- **目标**：以「一进程一台虚构设备」的同源 ROM 模拟器（`device-sim`）+ Bridge 逐 MAC 独立实验时钟，让 Bridge 的多设备、会合、占用、PowerPlan 与失步场景可在无实机条件下验收。
- **状态**：部分完成（执行中）。Stage A/B/B2 **已合并进 master**（提交 `cc6a6c1`、`d3ba464`、`c82487c`、`d65091f`，worktree 干净、`codex/fake` 已并入 HEAD）；Stage C 的同源命令切片与 Stage D 的 13a/13b/13c/13d 均已提交（`a8578a6`、`7aa6d38`、`e591951`、`afaf5ab`，并含合并提交 `94af78f`）。**仍没有可运行的 Fake ROM**：Bundle、Activate、显示、BLE 与 Bridge 逐目标时钟未接。
- **关键结论与证据**：
  - 架构决策：一个 `device-sim` 进程 = 一台设备（独立虚构 MAC、loopback 端口、数据目录）；一个生产 Bridge 管理多台真实/fake 设备，运行目标、认证材料、owner/claim、状态缓存、调度、发布任务与时钟上下文**全部按规范化 MAC 隔离**；fake 身份必须由测试配置显式登记，不能因为地址是 `127.0.0.1` 就自动启用虚拟时间。
  - 双时钟合同（修正了原设计「Bridge 与设备共用一个逻辑时钟」的表述）：设备侧与 Bridge 侧对该 MAC 各有独立视图，分别配置倍率/epoch 偏移/漂移，相同参数给同步场景、不同参数给失步场景；`Nx` 定义为锚点后虚拟增量 = 宿主 monotonic 增量 × 倍率，改倍率先结算旧锚点、单调时间不得倒退；`server_time` 可校 wall epoch 但不得改变任何单调截止；`step/max` 需显式按 MAC 场景驱动与在途 I/O 屏障，不能靠缩短 `sleep` 或扫描超时实现；真实 TCP/GATT 操作保留有限真实时间 watchdog，超时结果须标明是逻辑超时还是宿主 I/O 失败。阶段 A 只显式传参、保持旧持久格式，单调截止与旧 epoch 状态的迁移留到阶段 E。
  - 阶段 A（Task 1，`cc6a6c1`）：Coordinator 四个隐藏系统时间入口改为显式 `now` 参数，生产调用仍传真实时间；隔离测试 104 项通过。
  - 阶段 B（Task 2–8，`d3ba464`）：v2 HTTP 写在 POST 前核对认证状态的目标 MAC（错 MAC 只见 GET 无 POST）、token 缓存文件含目标 MAC（旧无绑定缓存保留但不使用）、`bridge-core::device` 支持显式 `IPv4:port`（裸 IPv4 仍默认 80）、`/claim` 先按目标 MAC 解析端点再取状态核对 MAC、认证 `/v2/status` 按 MAC 写独立缓存并 10 秒遍历非 legacy 设备、逐 MAC owner/yielded/last_claim_at/在线门限（评审修正了「409 不得刷新成功 claim 时间」）、HTTP 周期循环按已登记 MAC 经占用 gate 计划与投递、新增 `platform_device_register_v2` 显式登记（状态页与认证状态都匹配且具 v2 能力才登记，不自动 claim/推送）。Task 2–8 期间评审还纠正了「不存在的 legacy 现场」：当前现场均为 v2，`legacy` 仅作协议路径防护。
  - 阶段 B/B2 的 BLE 多设备入口（Task 10a/10b，`c82487c`）：`connect_any` 一次扫描匹配任一候选、连接后用完整 info MAC 授权，按 MAC 记录 55 秒尝试节流；有已登记 v2 目标时 UDP 通知不再转入旧长扫描。Task 9/10a/10b 与 Task 11a/10c/11b（BLE 扫描/GATT 时间线、UDP 认证改址、UDP/HTTP/PowerPlan/Data/ACK 结构化日志）已提交；日志合同要求 `scan_id`、候选 vs `verified_mac`、忽略原因计数、空窗口最多每 30 秒一条汇总，且不得写 token/Wi-Fi 密码/请求体/完整快照。
  - 阶段 C（Task 12a–12i）：把固件 `main.cpp` 的真实 v2 路径逐条纵向抽为**固件与宿主共编同一份 C++**，串行顺序为 Data（`02dd043`）→ Plan（`e997a72`）→ Bundle BEGIN（`bd916c7`）→ CHUNK（`fa50902`）→ COMMIT（`b702aa3`）→ Activate（`dc445ff`）→ claim（`f030dac`）→ 命令信封（`c154945`）→ `/v2/status` 快照（Stage C 收口）。最终 `cargo test -p bridge-render` 完整包 35/35、`v2_state` 19/19 通过。切片只抽「预检后的决策」，设备端 token/owner/session 预检、NVS、LittleFS、显示副作用与 HTTP/BLE 包装仍留在 `main.cpp`；未覆盖功能显式报 unsupported。
  - 阶段 D：Task 13a 启动层（`a8578a6`）——loopback 一进程一虚构 MAC、三 token 域互不相同、鉴权 `/v2/status` 走 Stage C 同源 C++ 输出未配置状态、`/sim/state` 只读、其余写端点 501，且**不提供 `/status.json`** 以免 Bridge 误登记。Task 13b 设备时钟（`7aa6d38`）——每进程 `SimClock` 以宿主 `Instant` 为锚，支持暂停/step/0..1000x 倍率/wall 偏移，改倍率先结算旧时间、单调不倒退，step 仅暂停时允许且单次 ≤86,400,000 ms，`max` 与跨进程持久时钟明确未实现。Task 13c owner/claim（`e591951`）——新增必填 `--data-dir`（拒绝指向 Bridge 自己的 data 目录），`simulator.json` 绑定虚构 MAC，owner 写入 `owner.json`（字段与固件 `OwnerRec` 对齐、按 32 位 uptime 秒租期恢复并钳制 `since`/`last_seen`），`/claim` 经设备 token 鉴权并调用共享 `v2PrepareClaim`/`v2DecideClaim`（401/400/409/200 语义与固件一致，只有成功 claim/renew 更新 `last_seen`），13 项集成测试通过。Task 13d 未配置设备的 Plan（`afaf5ab`）——`/v2/plan` 调用 ROM 同源解析/session/Plan 判定/ACK 构造，Plan ID 与截止按进程隔离，重放 ACK 保留原授予秒数而 status 剩余随逻辑时间递减、wall 校时不改截止；16 项集成测试通过。
  - Task 13d 明确未做假阳性规避：cold boot 保持 `provisional=false`、`boot_ms=0`（此进程没有真实按键唤醒），有效 Plan 不改变 `deep_sleep`，能力标 `power_lifecycle` unsupported，不宣称已模拟无线休眠/会合。
- **未完成 / 遗留**：
  - **没有可运行的 Fake ROM**：Data / Bundle / Activate 端点仍 501，`/status.json` 未提供，Bundle 未配置，因此模拟器从未执行实际 sleep 或无线会合：**仍有效**（计划阶段 D 明确的中间态）。
  - 显示效果、RTC、跨重启持久 monotonic（`clock_persistence: unsupported`）与 `max` 全部未实现：**仍有效**。
  - 存储故障/突然断电/撕裂写入恢复未验证——owner 只验证了正常进程重启，**不能冒充断电恢复证据**：**仍有效**。
  - `reserve` OOM 未在宿主触发验证（宿主 shim 不报告 reserve 失败）：**仍有效**（已知验证缺口）。
  - 阶段 E（Bridge 逐目标时钟接线）、阶段 F（1x/Nx/step、同步与失步、丢 ACK、双端重启、多设备交错）、阶段 G（两台实机回归）：**仍有效**，全部未开始。
  - fake BLE、多设备同时运行的端到端证据（断电重启保留规则、鉴权与 ACK）：**仍有效**，未做。
  - 「一进程一台设备」+ 严格串行开发（同一时间只委托一名 6-luna high 代理）的约束在用户取消并行后写入 `plan.md`：**仍有效**（若继续实施须遵守）。
- **归档判定**：`可归档（只读历史）`。全部 Stage C/D 源码切片已提交并合并进 master（`codex/fake` 是 master 的祖先，两个 worktree 都干净），未完成项是**尚未实现的功能**而非未提交的工作；归档后按本摘要与 `plan.md` 的阶段表继续。归档时建议同时删除已注册但空的 worktree 注册（`C:/Users/cogic/.codex/worktrees/653c/codex_status` 对应的注册项，`git worktree prune`），因其目录被旧任务进程占用。

## 跨专项未决事项（归档后最需要保留的检索线索）
- **版本化资产发布协议（note4-bridge-publish）**：`protocol.md` 仍是草案；设备缺能力字段、manifest/object 端点、CTP1 资产字体引用与 ABI 升级；Bridge 版本化 HTTP 客户端有意未实现。
- **运行中 Bridge 会覆盖用户 sync 设置（bridge-family-sync-preservation）**：源码修复未部署，下次族发布可能把 1.54 的 `sync_enabled=true` 覆盖回 false。
- **面板电源未实机验证（note4-panel-power）**：两种模式与缓存自愈均无台架数据；注意 `0.18.21` 候选已被现场 `0.18.23` 取代。
- **1.54 时钟窗口保留未刷机、未复测（clock-window-retention）**：用户所见的局刷/全刷交替未被确认修复。
- **时钟更新整屏刷新（note4-live-sync / note4-template-96）**：用户观察，按要求只记录未修，跨两个专项。
- **Bridge 多设备发布与推送菜单（bridge-multi-device-ui / bridge-multi-instance）**：以 MAC 为键的发布路由与统一推送子菜单未实现；两台设备 HTTP 当时不可达，实机发布/OTA 未做。
- **Note4 PGUP/GPIO39 清醒态切模板（note4-buttons）**：计划已定、源码未实现；ENTER 是否真的唤醒 MCU 仍未取证。
- **唤醒会合取证（wake-contact-trace）**：固件记录与 Bridge 持久化已实现，实机 30 次会合与故意超时验收未做；计划中的 `0.18.22` 版本号已被 `0.18.23` 取代。
- **Bundle v3（bundle-v3）**：纯设计，未实现；`roughly 16.2 KB` 需由编码器实测确认。
- **Fake ROM 模拟器（fake-rom-simulator）**：Stage D 未完成（Bundle/Activate/显示/BLE/持久时钟缺失），阶段 E/F/G 全部未开始。
