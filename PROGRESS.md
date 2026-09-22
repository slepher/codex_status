# Codex Status 项目进度（交接文档）

## 进行中：2026-09-22 — 第二硬件 target（4.2" 400×300 SSD2683 / ZecTrix Note4）

用户已确认面板事实：**4.2 英寸黑白、400×300、SSD2683**；按键 = 侧边 PGUP/PGDN、
正面 ENTER（原理图 net：`KEY_PGUP` / `KEY_ESP32_EN` / `KEY_ENTER`）。
本轮已完成（编译/宿主验证，未 OTA）：
- **几何参数化**：`template_engine`（`tplSetCanvas`）、`refresh_policy`（`rgnSetPanel`）、
  `platform_target.h`（`TARGET_WIDTH/HEIGHT/ROW_BYTES/FB_BYTES`）——同一引擎可服务
  200×200 与 400×300；200×200 宿主逐像素/区域一致性测试仍全绿。
- **驱动选择层** `src/epd_target.h`：`EPD_TGT_*` 别名按 target 选 SSD1681/SSD2683；
  `main.cpp` 已改为别名（200×200 行为不变）。
- **SSD2683 驱动骨架** `src/EPD_SSD2683.{h,cpp}`：400×300/1bpp（50B/行、15000B 帧）、
  窗口/双 plane/BUSY 传播/局刷窗口接口；仅 `CODEX_TARGET_NOTE4` 编译。
- **第二 ROM 环境（未启用）**：`platformio.ini` 的 note4 env、waveform LUT 与引脚
  全部以 `#error` 显式列出（不猜），故不加入默认 `pio run`。
- **ROM**：`artifacts/codex-status-0.16.8-bw.bin`（1 684 928 B，SHA256
  `7AA6A96C1B9B07B6501B7EA6C10DE758DBF1B5A52D25D0D34EC6F22AA09297A3`，含几何参数化，
  **未 OTA**；设备现场仍为 0.16.7 的 `687A611B…`）。

**继续所需的硬件事实（缺一不可，勿猜）**
1. ESP32-S3 侧 EPD GPIO：`EPD_SCK/EPD_MOSI/EPD_CS/EPD_DC/EPD_RST/EPD_BUSY/EPD3V3_EN`
   对应 GPIO 号（原理图放大截图或文字对照）。
2. 三个按键 `KEY_PGUP/KEY_PGDN/KEY_ENTER` 的 GPIO 号。
3. SSD2683 面板的时序参数（gate 数、方向/数据入口、border、温度曲线）与
   **两套 waveform LUT**（厂商样例/规格书），用于 `ssd2683_luts.h`。
4. 该板 flash/PSRAM 型号与容量（独立 ROM 的分区/帧缓存规划；400×300 1bpp 单帧
   15000B，A/B 双帧 + 编译产物仍需容量审计）。

拿到 1–4 后：填 `src/platform_target.h`/`DEV_Config.h` 的 NOTE4 映射 → 启用 env →
`TARGET_PARTIAL` 仅在波形/BUSY 实测后打开 → 模板 variant（`render_target=
epd-ssd2683-400x300-1bpp`）与 OTA 双端防错已在协议/桥侧就绪 → 逐项实机清单见
`project-workflow/generic-display-platform-implementation/status.md`。
桥侧 400×300 target 注册/画布校验/预览与 variant 路径尚未接线（下一步）。

## 实机验证：2026-09-22 — v2 平台在 200×200 SSD1681 设备上跑通（固件 0.16.7-bw）

设备 `70041DD7A340` / 192.168.3.163，桥为本次实现构建（`bridge/target/debug`）。
过程固件：0.15.10 → 0.16.0 → 0.16.7（每轮都是实机暴露问题后的修复，全部 OTA 验证）。

**已验证（实机）**
- **legacy 回归**：装 v2 Bundle 前 `[v2] no committed bundle; legacy template store active`，
  quad 正常渲染、区域策略 `n=13`、Wi-Fi push 正常。
- **完整 Bundle 安装**：BEGIN/CHUNK/COMMIT 提交成功；`v2_bundle=true`、3 模板、
  `commit_seq=2`、设备生成 context；变更模板后再次发布走另一槽（A/B），
  `commit_seq` 递增、context 重新生成。
- **数据投递**：`data_seq` 单调（1→5），`display=displayed`，`epd_writes` 递增；
  字段 CRC 与桥逐字节一致。
- **局刷与清影**：黑块反白数字变化 `refresh=partial/ok dirty=37`（未整块重刷）；
  连续 89↔90 多次后按预算升级为 `full/clean`（实机观察到阈值行为）。
- **正式 PowerPlan**：plan_id 1/4/6/7；`remaining_s` 单调递减（跨多次状态读取与一次数据推送
  不续租）；旧 plan_id（0）被 `stale_plan` 拒绝。
- **BOOT provisional**：`wake=ext1` 后 `prov=True prov_rem=276`（从物理唤醒起算）；
  桥保持原窗口下发 `granted=267`（不是新的 300），随后用新 plan_id 延长到 600。
- **deep 与 timer wake**：上下文在正常 deep 唤醒后保持同一 `active_context_id`；
  deep 期间排队的 push 在唤醒后的首个会合窗口投递（约 60–70s）。
- **A→B→A**：远程显式激活产生三个互不相同的 context。
- **OTA target 防错**：错误 target 返回 401，设备日志
  `[ota] rejected: target codex-status-154g-gray4 != codex-status-154g`，固件未变。
- **安装中断 + 掉电**：写入半个 Bundle 后深睡/重启，已提交包与 job 完好。
- **PM**：`light_sleep_counts=2822`、SLEEP 占比 79%，无 OTA/USB 锁泄漏。

**实机暴露并修复的问题**（全部已回归）
1. `/v2/*` 认证应为 endpoint token（桥业务通道），非设备操作 token。
2. BEGIN/COMMIT/ACTIVATE 的 `bridge_id` 在 JSON body 中（此前误读 query）。
3. Bundle 槽尺寸少算 12B 序列化头 → 读回长度校验失败。
4. `bsInstall` 的 9KB `CtTemplate` 落在 8KB loop 栈 → 栈溢出（int-wdt）。
5. `LittleFS.begin` 用默认 label 覆盖挂载标签 → `totalBytes()=0`、空间检查误拒。
6. Bundle 必须能在没有 context 时投递（它是 context 的来源）。
7. 设备空 `active_context_id` 不得被当作文成 context 采纳。
8. activate 成功后未清 `pending_activate` → 周期性重复激活/新 context。
9. plan 内容相同但窗口过期后必须换新 plan_id（否则无法重新授予 light）。
10. 有 Bundle 但无正式计划时需要设备侧 max light lease 兜底。
11. v2 有待投递数据时 legacy pull 响应必须回 light（否则 timer wake 立刻回 deep）。

- **桥不可达时的 BOOT 300s 兜底**（实机，桥停机）：`wake=ext1` 后 provisional 从 288 单调
  递减（Wi-Fi 已连、`http -1` 重试），到 `prov_rem=3`（≈t_boot+293s）后设备关闭无线并回
  deep，此后 ~1 分钟无响应；全程未接受任何正式计划（`plan=0`）。
- **桥重启后的恢复**：设备 timer 唤醒后保持同一 context；桥用已持久化的计数继续
  （`data_seq=9` 跳号被接受），并下发新的正式计划（id 7，600s），`display=displayed`。

**ROM**：`artifacts/codex-status-0.16.7-bw.bin`（1 684 864 B，SHA256
`687A611B6DF655A62A3F9314328DFD8FFFDEA0C8F5E7D8D51CABCBA6ED8250CB`，与当前源码重建一致，
已 OTA 到设备 ota_0/ota_1 轮换）；中间构建保留 0.16.0–0.16.6（sha 见各自 artifacts）。
**交互验证产物**：`artifacts/panel-*.png`（电脑摄像头拍摄：清洁全刷参考 + 局刷后对比；
自动面板定位置信度不足）。用户要求残影定量照片“后续再拍”，当前以
`partial/ok dirty=37`、连续变化后自动 `full/clean`、跨 deep 基线与零刷新作为软件证据。

**剩余（非阻塞）**：固定机位残影照片定量判定；新硬件 target（面板/控制器资料未到，
`blocked_by_hardware_arrival`）。

## 实现：2026-09-22 — 通用多设备墨水屏平台 v2（M0–M6 实现，SSD1681 实机验证待设备在线）

- **工作区**：`project-workflow/generic-display-platform-implementation/`（plan/status +
  task-0…task-6）。架构权威：`docs/generic-display-platform-design-v2.md`。
- **固件 0.16.0-bw**（`src/main.cpp` FW_VERSION，target 见 `src/platform_target.h`）：
  - `v2_state.h`（纯状态机，固件与宿主共用）：PowerPlan 幂等/旧 ID 拒绝/设备上限缩短、
    BOOT provisional 300s（从物理唤醒单调计时）、data_seq 幂等/冲突/乱序、退出路径锁。
  - `template_engine.{h,cpp}`：`tplCompile/tplDrawCt/tplCtSerialize/tplCtDeserialize`。
    模板 JSON 只在保存/安装时解析一次；运行、按键切换、重绘、data 应用都不再解析模板 JSON。
    编译产物定长、无指针、ABI/索引/资源边界重校验；8 个模板不会同时展开 DOM。
  - `bundle_store.{h,cpp}`：完整 Bundle A/B 槽（littlefs `/bundle/a|b|littlefs meta 双副本`），
    先编译全部模板→写非当前槽→读回 CRC→写防撕裂提交记录→才切换；空间不足拒绝且不删有效包；
    上一完整包仅用于失败恢复；`active_context_id` 在提交/激活/恢复时重建。
  - `v2_runtime.{h,cpp}`：Data 按编译后的 requirement 索引校验并重建 usage 文档；
    字段 CRC 与桥 `data_fields_crc` 逐字节一致（宿主交叉测试）。
  - HTTP：`GET /v2/status`、`POST /v2/data|/v2/plan|/v2/activate`、`POST /v2/bundle` 与
    有界 `BEGIN/CHUNK(offset,超时)/COMMIT`；全部走 endpoint token + owner，均不隐式续租。
    `/status.json` 增加 fw/render target、context、active、job、data_seq、power 等字段；
    OTA 增加 `?target=` 双端防错。
  - `refresh_policy`：新增 `rgnBuildCt`（从编译产物推导安全区域，激活路径不解析 JSON），
    与 JSON 推导在宿主逐区域一致。
  - 第二 target：`esp32-s3-epaper-154g-gray4`（2bpp/4gray，独立 ROM，`partial=false`）；
    `CODEX_TARGET_UNVERIFIED` 在缺少面板/引脚事实时拒绝正常运行，属
    `blocked_by_hardware_arrival`。
- **Bridge（未提交）**：
  - `bridge-core`：`compile.rs`（CompiledTemplate）、`datasource.rs`（Codex/Static JSON、
    push/pull 真值表、SourceSnapshot）、`coordinator.rs`（每设备串行协调器：单 active context、
    push/full 指纹、ack 基线、full_sync_deadline、单调 data_seq、单 PublishJob、
    PowerPlan 幂等/BOOT remaining、merge/指纹）、`platform/service.rs`（共享应用服务）、
    `platform/store.rs`（原子写入）、`v2_client.rs`（HTTP Status/Data/Plan/Activate/Bundle）。
  - Profile 1–8 全部参与循环、无 enabled 子集、无 revision；保存≠发布；发布冻结；排队编辑不漂移；
    target 不匹配两端拒绝；ACK 丢失重试同 seq/同内容，旧 ACK 不确认新数据。
  - `crates/render`：新增 `compile/compiled_serialize/compiled_deserialize/render_compiled_bits/
    rgn_build_compiled` FFI；宿主测试证明编译路径与 JSON 引擎逐像素一致（quad/mini/full×
    正常/缺失态）、区域推导一致、CRC 与桥/Rust 交叉一致。
  - app：`platform.rs` 四页共用服务 + 19 个 Tauri 命令 + v2 交付循环；v2 设备自动停用 legacy
    Wi-Fi 推送（一台设备一个数据通道）。MCP 新增 15 个 v2 工具（与 UI 同服务，save 不 publish）。
  - UI：模板库（target/引用/预览）、设备 v2（Profile 顺序/active/发布/job/恢复）、
    功耗（正式/provisional/剩余/显式 light|sleep）、数据页（DataSource/字段 push-pull/采集）。
- **测试**：`cargo test --workspace`（隔离 target）81 项全过；`pio run` 两个 env 成功；
  `node tools/test-quad-preview.mjs` 7/7；Python canonical hash 与 Rust/固件一致
  （c1a2faaf/e6ba459e/430cc188）；`git diff --check` 干净。
  `cargo fmt --check` 在**未改动的既有文件**（如 `app/src/autostart.rs`）已有格式差异；
  本次新增/修改的文件已单独 rustfmt。
- **ROM（未烧录）**：`artifacts/codex-status-0.16.0-bw.bin`（1 684 208 B，SHA256
  `6C7D58BF7FC8F21DB4C8760EF711655207CB72457228074F61CEEC1CDE5D3EF2`）；
  `artifacts/codex-status-0.16.0-gray4-unverified.bin`（1 684 560 B，SHA256
  `AFE8415AFE1107AD76597C90FA45C251C70128ADE70478DD05D593CCECAC6B3D`）。
- **现场阻塞**：设备（MAC `70041DD7A340`，IP 192.168.3.163）自实现开始即 deep/离线，
  OTA 与实机清单无法执行；运行中的桥是旧二进制（未重启，未改动其数据）。
  实机验证清单见 `project-workflow/generic-display-platform-implementation/status.md`。
- **未提交**；未回退任何用户改动；未停止/替换运行中的桥。

## 架构文档：2026-09-22 — 通用多设备信息终端简化架构 v2 完成（未实现）

- Astra medium 子代理结合上一版总设计、BLE 功耗设计和累计产品决策，新建
  `docs/generic-display-platform-design-v2.md`，作为唯一现行总设计；旧版已归档到
  `docs/history/generic-display-platform-design-v1.md`，只保留决策历史。
- 核心模型收敛为每设备一个 1–8 项 Profile、`CompiledTemplate`、单一
  `active_context_id`、`DataSource + SourceSnapshot`、完整 A/B Bundle、单
  `PublishJob` 和简单 ACK；删除模板 revision/history、资源图/GC、多层发布实体
  及 Provider/Dataset 分层。
- 设备不再区分短检查/长检查，也不解释 push/pull；定时或手动苏醒只广播并执行
  Bridge 指令。Bridge 负责变化判定、完整同步阈值和正式 `PowerPlan`。push 变化
  发送含 pull 字段的完整快照；pull-only 变化不推送、不续租。
- BOOT provisional 300 秒从物理按键唤醒起算；正式计划只能由 Bridge 下发，设备
  保留低电、最大期限、超时与 `plan_id` 幂等安全边界。deep 首次实时性受会合周期
  限制，Wi-Fi light 内才可连续即时投递。
- 屏幕刷新归设备端 framebuffer diff 与面板策略：黑块不因局部反白数字变化整体
  重刷，dirty rect 对齐后按真实前后像素写入；局刷次数、擦除量、面积、时间、温度
  和基线可信度共同决定清影全刷。SSD1681/SSD2683 差异留在驱动/面板适配层。
- 本里程碑仅新增设计文档并更新交接记录；未修改固件/Bridge、未构建、部署或改变
  设备现场。新 target 的空间、会合参数、续航与面板阈值仍需实测。

## 架构文档：2026-09-21 — 通用多设备信息终端总设计完成（未实现）

- 无对话上下文的 Astra 子代理独立撰写初版，现已归档为
  `docs/history/generic-display-platform-design-v1.md`；当前总设计见
  `docs/generic-display-platform-design-v2.md`。
- 总设计把 Codex 降为 Provider，Bridge 固定四标签：模板、设备、数据、MCP；
  功耗为设备子菜单，MCP 与 UI 共用 Application Services，不得绕过安全或发布规则。
- 设备可保存多个模板但任一时刻只有一个 active；模板安装/激活时编译
  `DataRequirementPlan` + `RenderPlan`，日常会合、请求、deep 唤醒和局刷不得重新
  解析模板 JSON。ViewSnapshot 绑定 manifest、active hash、plan hash 与
  `activation_generation`，可拒绝切模板后在途旧数据。
- 模板库不做用户可见版本管理：每个 template ID + target 只保留当前最新版；Profile
  只引用 ID，显式发布时冻结一次性 ReleaseArtifact，保证排队内容不漂移，但不形成
  历史版本/回滚入口。读取设备模板降为恢复/迁移角落流程；可 canonical/target/ABI
  去重或 unresolved 占位，且不 WAKE、不 claim、不重推、不改 active。
- 多硬件分离 BSP/controller/panel profile/render target/firmware target；NOTE4 参数
  仍标为待核实。每 target 独立 ROM，模板使用独立 variant，OTA 目标防错与回滚
  能力需实证。
- 本里程碑仅新增设计与工作流文档；没有修改固件/Bridge、没有构建、部署或改变现场。

## 最新状态：2026-09-21 — 0.15.10-bw + 桥修复：修复“light 下永不休眠”（renew/心跳不再续 light + 桥主导回 deep）

- **用户报告**：设备在 light 下长时间不休眠；诊断（2026-09-21 晚）：
  1. 桥每 60s 续租 owner（`occupancy_gate`），设备 `handleClaim` 对 renew 也
     调 `noteActivity("claim")`，600s 本地 idle 永远清零；桥每 300s 心跳推送
     同样 `noteActivity("push")`。日志 `[owner] renew` 与 `[clk] tick` 数量
     约 1:1，`idle_s` 采样 45→28→9 不断回零。
  2. 桥侧 push 封包的 mode 用 `mode_str()`→`expects_deep()`（要求
     quiet≥600s 且 silent≥120s），但 10s 的 `/status.json` 轮询与 60s 续租都
     算 `note_contact`，`silent` 恒 <120s；且 `occupancy_gate` 在
     `mode_str()` 之前续租，故 push 永远带 `mode:"light"`。
  共同结果：light→deep 无可用路径（只有 `/diag?deep_now`/调试模式能睡）。
- **固件修复（0.15.10-bw）**：
  1. `handleClaim` 仅新 claim（`!keepSince`）才 `noteActivity`，renew 是协议
     保活、不续 light（设计 §6）。
  2. `/usage` 推送仅在 `usage_rev` 变化时 `noteActivity("push")`；无 rev 字段
     （旧桥）保守视为变化。心跳推送不再重置 idle。
- **桥修复（bridge-app）**：`activity.rs` 抽出 `light_wanted()`（pending /
  light dwell / quiet<600 则 light），pull 响应与 push 共用；`mode_str()` 不再
  依赖 contact 年龄。app push 循环增加 `mode_changed`：模式翻转（light→deep）
  立即推送，不等 5 分钟心跳，实现桥主导的有界回 deep。
- **验证**：
  - 0.15.10 上，renew（每 60s）与心跳推送持续时 `idle_s` 从 11 单调涨到
    543+，约 600s 时设备自然进 deep（证明固件修 1/2 生效；此前永远 <60s）。
  - 用 `/diag?idle_deep_s=3600`（RAM）排除本地 idle 后唤醒设备：桥在
    light dwell(300s)+quiet(≥600s) 到点后把 push mode 翻成 deep，设备约 60s
    后进 deep（20:55 不可达），证明桥修 3 独立生效。
  - 桥已按 watchdog-first 重启：旧 5216/14540 停止，新 PID 6392 + watchdog
    42604（bridge/target/debug）；设备 deep 期间 renew/推送按 `expects_deep`
    暂停属预期。
- **ROM**：`artifacts/codex-status-0.15.10-bw.bin`（1 644 320 B，SHA256
  `79B0412320C08CDB297FB3141FB0B3B7831FEF1BD240B13BDD553E281EDCFCD0`），
  OTA 0.15.9→0.15.10 成功。`idle_deep_s=3600` 仅为验证用的 RAM 覆盖，
  深睡重启后自动恢复默认 600。
- **测试**：`cargo test --workspace`（隔离 target）通过；`bridge-core` 新增
  单测 `push_mode_follows_quiet_not_contact`；`pio run`；`git diff --check`
  干净。
- **行为说明**：设备 deep 时每 60s 仍 pull；usage 有新变化（quiet<600s）时
  桥在 pull 响应里回 `light`，设备自动回到 light；安静 600s 后桥主动回
  `deep`。BOOT 在 deep 下仍按既有语义唤醒到 light。
- **未提交**。

## 历史：2026-09-21 — 0.15.9-bw + quad v12：task-10 落地（缓存冷启动直进模板 + Wi-Fi 图标 1Hz 闪烁 + 双帧修复）

- **需求来源**：`project-workflow/ble-rendezvous-power/task-10.md`（此前用户明确
  “仅入文档”，本窗口实现 A/B + 0.15.7 遗留的双帧修复）。
- **需求 A（缓存冷启动）**：`startNormalMode` 在 `!wokeFromDeep &&
  lastUsage.length()>0` 时先渲染缓存模板（`WIFI OFF`、Wi-Fi 图标灭、无
  `Connecting:` 页）再建连；连上后的既有 link-up 渲染即“一次局刷点亮图标”；
  失败保持模板并走既有有界退避/回深（不再画 Zzz）；无缓存仍走原
  Connecting/状态页回退。同时删除了 0.15.5 起重复两次的冷启动渲染。
- **需求 B（连接中闪烁）**：连接期间模板可见状态在 `WIFI CONN`（亮）/
  `WIFI OFF`（灭）之间按 `WIFI_BLINK_MS`（默认 1000ms）交替；`connectBest`
  等待循环驱动，失败后补一帧 settle 回 `WIFI OFF`。仅当模板已上屏且基线
  trusted + partial-ready 才启用（不会污染 Connecting 页，也不会因基线不可信
  每次变全刷）。闪烁帧在 `epdFlush` 中保持 `kind=partial, reason=blink`，
  不消耗区域 ghost 预算、不计入 30 次局刷 streak；深睡定时重试用
  `deepFastConnect`，不闪烁，因此闪烁仅在冷启动/BOOT 唤醒的一次建连内。
- **quad v12**（seed `tools/test-bridge/templates/quad.json`，version 12，
  canonical hash `430cc188`）：x=121 增加 `WIFI CONN` 的 Wi-Fi 图标（与
  BLE ON/OFF 同图），x=141 增加 `WIFI CONN` 的划叉覆盖（与 WIFI OFF 同图），
  使每次切换只改 Wi-Fi 图标格。三端同步：固件引擎无协议改动（复用
  `when.equals`）、Rust canonical/测试更新、Python seed 更新、node 预览新增
  `quad-preview-conn.png` 与“只切换图标格”断言。
- **双帧修复**：`deepNetworkCycle` 在 `leavingDeep` 画完“去 Zzz”的干净全刷后
  置 `wakeBaselineDrawn`，`startNormalMode` 据此跳过重复的 clean 首帧。
  实测 timer pull→light 日志：仅一次全刷（PULL）+ 一次局刷显图标，随后
  `epd_writes` 增量正常，无第二次全刷闪烁。
- **实测（设备 192.168.3.163，电池、light）**：`/diag?blink_test=30` →
  `avg_ms=870 worst_ms=875 refresh_avg_ms=829`，30/30 为 partial，
  `epd_busy_fails=0`、`epd_trusted=true`、区域预算未被闪烁消耗。结论：维持
  1 Hz（单次失败冷启动最坏 ≤30 tick ≈ 0.4mAh，按设计 §9 的 ~0.013mAh/次
  事件模型；无电流仪，属面板活动时长核算）；`/diag?blink_ms=N` 可现场调周期
  或置 0 关闭。新增 `/status.json` 字段：`refresh_ms`、`blink_ms`、
  `blink_on`、`blink_ticks`、`wifi_conn`。
- **真机验证（0.15.9 + quad v12）**：冷启动（OTA 后 software reset）日志顺序
  `[tpl] rendered quad (-)`（连 Wi-Fi 之前，无 Connecting 页）→ 建连中 1 次
  闪烁渲染（`blink_ticks=1`）→ `[wifi] connected` → link-up 局刷；
  `[rgn] derived n=13 whole=0`（v12 两个新元素仍并入原图标格，31 个元素
  < RGN_MAX 32）。
- **ROM**：`artifacts/codex-status-0.15.9-bw.bin`（1 644 256 B，SHA256
  `47C641DF254449751DA6678F4503AA8276B527FCC3688DFABDA076024F9B8549`），
  已 OTA（0.15.8→0.15.9，直传成功）；中间版 0.15.8-bw（含首个测量轮，缺
  `blinkAllowed` 基线保护）被 0.15.9 取代，ROM 见
  `artifacts/codex-status-0.15.8-bw.bin`（SHA256
  `FFECE66BF1062926CD18BAD27127F1FFA74CCF129CC3F7572237FBA8B0CD5006`）。
- **验证**：`pio run`；`node tools/test-quad-preview.mjs`（新增闪烁断言）；
  `cargo test -p bridge-core --test template`（quad v12/`430cc188`）；
  `cargo test -p bridge-render --test policy`；`cargo test --workspace`
  （隔离 target）；`git diff --check` 干净。
- **现场**：设备 `0.15.9-bw`（ota_0，light、在线、quad v12 已推送、rv2=0、
  `pm/rv2` 未动）；桥 debug（PID 5216 + watchdog 14540）`device_mode` 已回
  auto；诊断开关 `idle_deep_s=600`、`frame_capture=0`、`blink_ms=1000`。
  token 规则未动，未打印任何凭据。
- **待办**：物理 BOOT 按压实测闪烁（1Hz、连上常亮、无 AP 时熄灭 + 有界回深）；
  固定机位照片验收（阶段 2 黑块门槛）；90 次时钟预算长测；BOOT BLE 回归
  （legacy 推送 + `rv:2` NACK）；随后阶段 3（v2 事务）。未提交。

## 历史：2026-09-21 — 0.15.7-bw：修复离开 deep 的 Zzz 残影 + 新需求仅入文档

- **用户报告修复（0.15.7）**：自然睡眠唤醒后 Zzz 未被完全刷新，看起来仍在
  睡眠。根因：定时网络窗口拉起 light 时，`deepNetworkCycle` 先按 deep 状态
  渲染一帧，`startNormalMode` 再在它上面做**局刷**去掉 Zzz；局刷波形对深色
  图标清除不净，留下 Zzz 残影。修复：在 pull 渲染前先按响应 `mode` 切到
  light（`rtcMode=MODE_LIGHT`、`persistMode`），并把 `leavingDeep` 纳入渲染
  条件与 `forceCleanRefresh`，使“去 Zzz + 进入 light 帧”在同一次**全刷**里
  完成；`startNormalMode` 的唤醒首帧同样带 clean。现场日志验证：
  `wake=4(timer) mode=deep` → pull ok → `mode=light` → `[tpl] rendered quad
  (PULL)`；ROM `artifacts/codex-status-0.15.7-bw.bin`（1642272 B，SHA256
  `81B9EDF8F04759A9B68B13288188B989B8CB16ED1595F6EBB5A12FAA4EA89E0E`），
  已 OTA（0.15.6→0.15.7，直传 37s）。
- **已知遗留（记入 task-10，勿忘）**：同一路径在 timer pull→light 时会画两
  帧（pull 帧 + 唤醒帧），即两次全刷闪烁；RAM 标志 `wakeBaselineDrawn` 的修
  复原型已因“仅文档”要求回退，待 task-10 一起做。
- **新需求仅入文档（用户明确要求，本窗口不实现）**：`task-10.md`
  - A：冷启动若存在 usage 缓存，直接进模板（`WIFI OFF`、Wi-Fi 图标灭、
    桥显示断连），不等 Wi-Fi；连接成功后一次局刷点亮图标；失败保持模板并
    走既有有界退避/回深，不再出现 `Connecting:` 页。
  - B：Wi-Fi 连接过程中图标按 ~1Hz 闪烁，成功转常亮、失败/超时熄灭；模板
    驱动（可能 quad v12 + 三端哈希同步），只用局刷；**功耗必须实测**后再定
    周期/时长。
- **现场**：设备 `0.15.7-bw`（light、在线）；已恢复调试设置
  `idle_deep_s=600`、`frame_capture=0`，桥 `device_mode` 已回 auto。桥
  fresh debug（父 PID 5216 + watchdog 14540）。token 规则未动。
- **待办**：照片验收；90 次时钟预算长测；BOOT BLE 回归；task-10（A/B +
  双帧修复）；随后阶段 3。未提交。

## 历史：2026-09-21 — 0.15.6-bw：修复 BOOT/图标变化触发全刷 + OTA 重试事故处理

- **用户报告修复**：按 BOOT（BLE 图标切换）现在只做局刷。根因：阶段 2 的区域
  合并按“字节扩张相交”把图标并进了大黑块区域，任何图标变化都被黑块保守
  guard 判成全刷。0.15.6 改为**只按真实像素相交合并**（字节列仅作为未来
  窗口写入的元数据），区域统计按语义像素矩形并掩掉边缘字节；quad v11 得到
  13 个独立区域（两块黑块、三个图标格、时钟格、右/下行文本）。现场实测
  `[rgn] derived n=13 whole=0`，`render_mode=deep/light`（等价 BOOT 的图标
  变化）均为 `partial/ok`（dirty 143/101）。主机测试新增“图标变化必须局刷”
  断言。
- **OTA 事故（已定位、已恢复，已记入 task-8 现场笔记）**：deep 期排队的
  OTA 唤醒后每次尝试都在设备侧 `upload start` → `abort (aborted)`，桥按
  60s→2m→4m 退避重试（屏上反复闪升级页）；直接 MCP 调用则因设备已停止
  读取、客户端还在发 1.6MB 而卡到 120s 超时（看似挂死）。设备侧无故障：
  curl multipart 直传同一 ROM **26s 成功（UPDATE OK）**；重启 `bridge-app`
  后桥侧 dummy 上传也正常。结论：长跑桥进程的客户端连接状态异常，非固件
  问题。阶段 5 将加 `pending_ota` 可见性、preflight 后短延时、上传超时有界
  并报已发字节、OTA 与推送串行化。
- **现场**：设备 `0.15.6-bw`（ota_1，light、在线、电量 ~72%，quad v11）；
  0.15.6 已由 curl 上传方式落盘（桥重启后 26s 完成）。桥已重启为
  新进程（父 PID 5216 + watchdog 14540）；桥日志在
  `bridge/target/debug/data/logs/bridge-app.log.<date>`（事故证据为
  反复 `doUpdate transport error`）。token 规则未动。
- **安全发现（待处理）**：桥的错误日志会把 `?token=...` 完整打进 URL；
  且已提交的 `project-workflow/sleep-modes/prompt.md`（e22d952）含明文设备
  操作 token，当前仍可用。建议下次 BOOT 会话经绑定 BLE 轮换 token，并在
  阶段 5 对桥日志 URL 做脱敏（已记入 `task-8.md`）。
- **验证**：`pio run`；`node tools/test-quad-preview.mjs` 7/7；
  `cargo test -p bridge-core --test template` 8/8；
  `cargo test -p bridge-render --test policy` 1/1（隔离 target）；
  `git diff --check` 干净。
- **ROM**：`artifacts/codex-status-0.15.6-bw.bin`（1642192 B，SHA256
  `21404F60965F8214603D2BAED599180AD81431902B2333600FA2D4A7508F2F79`）。
- **待办**：照片验收（黑块局刷门槛，用户）；90 次时钟预算长测；BOOT BLE
  回归（legacy 推送 + `rv:2` NACK）；随后阶段 3（v2 事务）。新窗口交接见
  `project-workflow/ble-rendezvous-power/prompt.md`（不含任何凭据）。未提交。

## 历史：2026-09-21 — 0.15.5-bw：阶段 2 显示安全层 + 修复冷启动卡 Connecting 页

- **用户报告修复**：OTA 重启后曾卡在 `Connecting:` 页，按 BOOT 才回到模板。
  日志根因：冷启动首次 bridge pull 超时（`http -11`），而渲染只发生在 pull
  成功/唤醒路径。修复：Wi-Fi 上行后的冷启动统一 `renderCurrent()` 一次，
  用缓存 usage/模板或状态页替换连接页（相同帧零写入）。已随 0.15.5 部署。
- **阶段 2（task-5，显示安全层）落地**：
  1. 新增 `src/refresh_policy.{h,cpp}`：从模板推导语义区域（filled rect =
     solid、白字黑底 = inverted、`buckets[...]`/`resetCredits` = usage、
     `device.now` = clock、icon/bar/line），按字节扩张相交合并并取最保守
     类别；逐区域统计 changed/W2B/B2W/黑量；ghost 预算（黑块 4、低墨 10、
     时钟 90）；判定顺序 = 基线可信 → clean/force → 相同帧 → 推导失败 →
     >12.5% 面积 → 极性 → 黑块保守 guard → 预算 → 局刷。**照片验收前黑块
     变化一律全刷**（阶段 2 保守口径）。
  2. `main.cpp`：`epdFlush` 走策略；`forceCleanRefresh` 绕过相同帧捷径
     （设计 §8.4）；波形成功后才记账/更新旧帧，失败置 untrusted；新增
     `/status.json` 字段（`refresh_kind/reason/dirty_pixels/rgn/...`）与
     `/diag?rgn=1`、`?policy=off|on`、`?clean=1`、`?busy_fail=1`。
  3. **主机测试** `bridge/crates/render/tests/policy.rs`：build.rs 纳入
     `refresh_policy.cpp`，同一份 C++ 策略断言 none/clean/trust/force/
     低墨局刷/黑块全刷/12.5% 面积/预算耗尽与复位；`cargo test -p
     bridge-render --test policy` 通过。
  4. 现场实测（0.15.5-bw，token 门控）：`[rgn] derived n=5 whole=0`；
     `/diag?rgn=1` 五区域（顶部合并黑块 + 右下黑块 + 三行低墨），低墨行
     `partials=1 cumS=13 budget=9`；`clean=1` → `full/clean`；
     `busy_fail=1` → `epd_trusted=false`，下次 `full/trust` 恢复；
     `policy=off`+render → `partial/legacy`；`policy=on`+render →
     `full/high_ink`。
  5. 修复推导 bug：`RGN_MAX 20→32`（quad v11 约 29 个元素，合并前溢出会
     退化为整帧全刷）。
- **验证**：`pio run`；`node tools/test-quad-preview.mjs` 7/7；
  `cargo test -p bridge-core --test template` 8/8；
  `cargo test -p bridge-render --test policy` 1/1（隔离 target）；
  `git diff --check` 干净。桥生产行为未改（仅 render 测试构建加入策略源）。
- **ROM**：`artifacts/codex-status-0.15.5-bw.bin`（1641872 B，SHA256
  `078ABFC8180D21BF790CE110E97069EA270C5E3EAADF272BA55101FFD0EC6E9A`）。
- **待办**：固定机位照片验收（黑块局刷开放门槛，用户）；时钟 90 次预算的
  长时观察（deep ≥1.5h）；用户按 BOOT 的 BLE 回归（legacy 推送 + `rv:2`
  NACK）；随后进入阶段 3（v2 事务）。未提交。

## 历史：2026-09-21 — 0.15.3-bw：BLE 会合/大黑块里程碑阶段 1（基线取证 + 显示基座）

- **计划**：`project-workflow/ble-rendezvous-power/` 新增 task-4..task-9，
  对应设计 §12 阶段 1–6（显示安全层 → v2 事务 → 固件会合/lease → 桥常驻
  监听 → 小规模启用）；status.md 记录入口。桥未改，无 GATT 表变更。
- **阶段 1（task-4）完成**：
  1. **基线证据**：`artifacts/ble-rendezvous/`（status/history/log/帧 PBM；
     帧 17 399 黑像素，TL 块 7 538、BR 块 7 463、时钟格 65、Wi-Fi 格 62、
     Zzz 0）。复核发现 0.15.2 ROM 哈希此前记录少一位：实测
     `D20BBFEF10D40C47E64BAFFA5FFF70F81BE90575D29DAD66BD851C78C2CF0FCA`
     （本文件与 status.md 已更正）。
  2. **RTC/堆审计**（新 `tools/rtc-budget.py`）：RTC slow 区 7 680 B，
     当前 .rtc 用 1 668 B，余 **6 000 B**；5 000 B 旧帧理论可放但留白太少，
     阶段 2 决定只保留时钟窗 + 紧凑区域/预算表，无旧像素的区域走全刷；
     帧缓冲在堆（空闲 ~147 KB）。
  3. **BUSY 返回值传播**：`EPD_SSD1681_*` 波形调用全部返回 bool；超时不再
     静默。`main.cpp` 计数 `rtcEpdBusyFails`、`epdBaselineTrusted` 失效时
     不更新 `lastDisplayedFrame`，下一次显示强制全刷；时钟窗口失败把
     `rtcClkPartials` 拉到上限以便网络窗口重建。`/status.json` 新增
     `epd_busy_fails`/`epd_trusted`（HTML 状态页同步）。
  4. **GATT 兼容分流 + 回滚开关**：template-control 带 `"rv":2` 走 stub
     （NACK `not_ready`，绝不按旧模板流解析）；INFO/`/status.json` 增加
     `rendezvous_v`(0/2) 与 `rv_max=2`；token 门控 `POST /diag?rv2=1|0`
     持久化到 NVS `pm/rv2`（默认 0，旧路径字节不变，无 Windows 重配对）。
- **验证/部署**：`pio run`（Flash 1 599 448 B，RAM 20.5%，仅既有 NimBLE
  deprecation 警告）；`node tools/test-quad-preview.mjs` 7/7；
  `cargo test -p bridge-core --test template` 8/8（隔离 target）。MCP
  `firmware_ota` OTA `0.15.2-bw -> 0.15.3-bw` 成功；现场
  `fw=0.15.3-bw`、`rv2=0`、`rv_max=2`、`epd_busy_fails=0`、
  `epd_trusted=true`、light、电量 72%；`/diag?rv2=1/0` 往返经 HTTP 实测。
- **ROM**：`artifacts/codex-status-0.15.3-bw.bin`（1632336 B，SHA256
  `DC6227B3EC4BA01F4A0FDD55D9AE79B166743CEE2BBF17A04EC1F9AD8B1437B2`）。
- **待办**：需要用户按 BOOT 的 BLE 回归（legacy 模板推送仍可用、`rv:2`
  得到 NACK）；固定机位照片验收（阶段 2 黑块局刷开放门槛）；
  `task-5` 显示安全层实现中。未提交。

## 历史：2026-09-21 — 0.15.2 + quad v11：唤醒流程改版（先清 Zzz，Wi-Fi 图标随连接显示）

- **需求（用户修订）**：睡眠模式不显示 Wi-Fi 图标（未连接）；按 BOOT 先立即
  取消 Zzz，连接成功后显示 Wi-Fi 图标；进 deep 时 Wi-Fi 图标消失。取代 0.15.1
  的“保留 Zzz 到连上”流程；`Connecting:` 页仍只在冷启动/配网出现。
- **实现**：
  - 固件 `src/main.cpp`（0.15.2-bw）：`wokeFromDeep` 取代 `keepSleepImage`；
    deep 唤醒后先渲染正常模板（Wi-Fi 未连，`state=WIFI OFF`，图标隐藏），
    Wi-Fi 连上后重渲一帧显示图标（相对唤醒全刷的局刷）；电池失败退避路径在
    回 deep 前用 `renderSleepGlyph()` 恢复睡眠帧（全刷基线）。
  - 模板 `quad` v10→v11：Wi-Fi 图标 (121,6) 改为按 `device.state` 条件绘制
    （`BLE OFF`/`BLE ON`），删除划叉 Wi-Fi 图标；seed hash `93199731`。
- **验证**：
  - `pio run`、`node tools/test-quad-preview.mjs`、`cargo test -p bridge-core
    --test template`（已更新 quad v11/93199731）全部通过。
  - 真机：OTA `0.15.1-bw -> 0.15.2-bw`，`profile_push default` 推送 quad v11；
    `/diag?render_mode=light` + `GET /frame?which=last` → Wi-Fi 格 62 黑像素；
    `render_mode=deep` → 该格 0 黑像素、Zzz 格 39 黑像素（图标随状态正确）。
- **ROM**：`artifacts/codex-status-0.15.2-bw.bin`（1629696 B，SHA256
  `D20BBFEF10D40C47E64BAFFA5FFF70F81BE90575D29DAD66BD851C78C2CF0FCA`；
  2026-09-21 复核：此前本文件与 status.md 记录的哈希少了一位字符，以上为
  实测值）。
- **现场验证（用户按 BOOT，通过）**：桥侧 debug deep 下 `/history` seq8
  `enter-deep(aux=60)` → seq11 `boot aux=3`（EXT1 按键）→ seq12 `to-light
  aux=1`；本次 boot `epd_writes=3`（唤醒清 Zzz 全刷 + 连上显 Wi-Fi 图标局刷
  + usage 渲染）、`deep.glyph=5`（睡前 Zzz 已渲染）。用户确认新时序符合要求：
  按下先清 Zzz、无 Connecting 页，连上后 Wi-Fi 图标出现。
- **待办**：失败路径（AP/桥不可达 → 恢复 Zzz 回 deep）可断 AP 验证；提交前整理
  （未提交）。桥 debug deep 已清回 auto。

## 最新状态：2026-09-21 — 0.15.1：静默唤醒（BOOT 保留 Zzz，连上 Wi-Fi 再消）

- **需求（用户）**：深睡时按 BOOT 不再跳全屏 `Connecting: <SSID>` 页；Wi-Fi 关联
  期间面板保留睡眠 Zzz，连接成功后首帧正常渲染清除 Zzz；失败/超时保持 Zzz 并
  有界回 deep。冷启动/配网仍显示连接页。已同步进
  `docs/ble-rendezvous-power-design.md` §3（状态机图、建连超时行、§13 静默唤醒
  测试行）、`project-workflow/ble-rendezvous-power/plan.md` 与 `task-2.md`。
- **实现（`src/main.cpp`）**：`keepSleepImage = woke`（任何 deep 唤醒：BOOT/PWR
  与失败退避的定时重试）；`connectBest(showProgress)` 在唤醒路径跳过连接页；
  `startNormalMode()` 在 Wi-Fi 连上后按 `rtcDeepGlyph & 1` 调一次
  `renderCurrent()` 清 Zzz（唤醒后首帧本就是全刷基线，不基于保留图像局刷）。
- **ROM/部署**：`artifacts/codex-status-0.15.1-bw.bin`（1629616 B，SHA256
  `5F1F81A6941229476A194BBED9B09FBCBAD2F9294E59E853BB71E94FCB3E2D52`）；
  已 OTA `0.15.0-bw -> 0.15.1-bw`（现运行 ota_0），设备 light 在线、quad 正常。
- **现场验证（用户按 BOOT，通过）**：`/history` seq5 `boot aux=3`（EXT1 按键）
  → seq6 `to-light aux=1`；`deep.glyph=5`（睡前 Zzz 已渲染，bit0+bit2）；唤醒后
  `epd_writes=1`（只有“连上后清 Zzz”的首帧全刷；若仍画 Connecting 页应为 2 次），
  `/status.json` `wake=ext1`、`mode=light`、RSSI −37、Web/桥连接正常。用户现场
  确认：无 Connecting 页、连上后 Zzz 消失。
- **待办**：失败路径（AP/桥不可达时保持 Zzz 并按退避回 deep）可另行断 AP 验证；
  提交前整理（本改动未提交）。

## 最新状态：2026-09-21 — BLE 分钟会合 + 大黑块刷新目标设计完成（未实现）

- 新增 `docs/ble-rendezvous-power-design.md`：目标架构把电池 deep 模式的每分钟
  Wi-Fi pull 改为“时钟窗口局刷 → 短 BLE 会合”。会合三分支为：无指令直接
  deep、BLE 原子下发小型 usage 后 ACK/deep、`WAKE_LIGHT(lease_s)` 后关闭 BLE
  并进入有期限的 Wi-Fi light 会话。
- BOOT 唤醒目标缺省 light lease 300s；仅显式 `RENEW_LIGHT` 续租，普通状态读取、
  claim、相同 revision 和数据传输不隐式延长；模板、OTA、大包及持续交互才升
  Wi-Fi。保持 `POST /claim` 为唯一 owner 创建/转移入口，token/绑定规则不放宽。
- 大黑块不再只按全帧 changed-bit 比例判断：目标方案按语义区域、黑白转换方向、
  黑像素占比和独立 ghost budget 决定窗口局刷或全刷；高墨量区初始保守预算4次，
  基线不可信/反相/大量擦黑直接全刷。阈值均待照片、低温和功耗实测校准。
- 文档明确区分现行0.15.0与目标方案；广播1–2s、事务/lease期限、RTC容量、BLE
  能耗均为待测建议，不是已验证结果。复用现有GATT表优先，避免Windows缓存导致
  重新配对。
- 本里程碑仅文档：没有修改固件/桥、没有部署或改变现场运行状态；工作流与后续
  阶段见 `project-workflow/ble-rendezvous-power/`。

## 最新状态：2026-09-21 凌晨二 — 0.15.0：时钟/时区随 PC + 深睡切换历史 + P2 端到端过半

- **现场**：设备 `0.15.0-bw`（ota_1，USB 插电，light/deep 测试中，`tz=UTC-8:00`）；
  桥 = debug 新构建（含 60s 节奏修正，`tools/start-bridge.ps1` 重启，
  PID 23652 + watchdog 53892）。
- **新功能（未提交）**：
  1. **pull/push 时间戳**：桥在 pull 响应生成时刻用 `now` 覆盖缓存信封的
     `server_time`（此前可能滞后数十秒~分钟），并新增 `tz_offset_min`（本机 UTC
     偏移分钟，东为正）；推送信封同样重写。实测 pull `server_time` 与墙钟差 **0s**，
     `tz_offset_min=480`（`bridge/crates/core/src/http.rs`、`lib.rs`、app push 循环；
     Python 测试桥同步）。
  2. **时区随 PC（0.15.0 固件）**：`applyTzOffsetMin()` POSIX 反向符号
     （+480 → `UTC-8:00`）并持久化 NVS `pm/tz`；`configTzTime` 不再硬编码
     `CST-8`。设备实测从 `CST-8` 变为 `UTC-8:00`，OTA 往返后保持。
  3. **深睡切换历史（0.15.0 固件）**：RTC 内存环 120×12B（epoch/ev/stage/batt/
     aux），记录 boot/enter-deep/thin/net-ok/net-fail/to-light；`GET /history`
     （`?since=` 增量）+ `/status.json` 的 `hist_count/hist_head`。实测一轮
     deep 循环事件与 `deep{}` 计数一致。
- **P2 端到端（真机，调试工具驱动，免拔线）**：
  - pending 模板：deep 期 `profile_push ab-0133` → 排队 → 17:02 pull
    `mode=light pending_tpl=1` → `queued template push flushed`（mini 上屏）；
    随后恢复 quad。✅
  - 迟滞：活动后 pull 答 `light/60`（17:15）；静默 ≥600s 后答 `deep` ✅
    （按用户纠正：**拉取间隔恒 60s**，见下条）。
  - **拉取节奏纠正（用户澄清）**：900s 只属于"设备连不上 Wi-Fi/桥"的失败退避
    （`retryDelaySec`），不是桥安静期的常态。已改桥 `activity.rs`：deep 分支也
    回 `next_contact_s=60`（Python 测试桥同步、单测/docs §13.3/§13.6/§13.8 更新）。
  - OTA 排队：deep 期 `firmware_ota 0.14.12` → 排队 → pull `pending_ota=true`
    → `firmware OTA ok 0.15.0→0.14.12`；再升回 0.15.0（一次直传成功）。✅
  - 断桥重试：设备深睡时停桥 ~2 分钟 → history `net-fail`（ev5, aux=0）→
    重启桥后下个窗口 `net-ok`（200）、`retry_stage` 归零、`net_fails=1`。✅
  - 旧桥缺字段 / token：用无 `tz_offset_min`/`mode` 的旧信封 POST → accepted、
    `tz` 保持；错误 token → 401。✅
  - 手动唤醒（BOOT→light、再击→BLE）与 BLE 打断需用户现场按键，待做。
- **ROM**：`artifacts/codex-status-0.15.0-bw.bin`（1629552 B，SHA256
  `FCB9CF7560B367D1B9004B2FB424098FE5C9F56DAB1B27A8FF6F6658932771FA`）。
- **待办**：长测 ≥2h（60s 网络节奏下的电量斜率、时钟准度、残影，`/history`
  逐分钟 thin 已可用）→ 用户手动唤醒/BLE 测试 → 提交前整理。未提交。

## 历史：2026-09-21 凌晨 — P1 结案（0.14.13 睡前全刷）+ OTA 逻辑修复 + 桥侧调试工具套件

- **现场**：设备 `0.14.13-bw`（USB 插电 light 在线，`tz=CST-8`，调试开关已复位）；
  桥 = debug 构建（PID **8764** + watchdog **26792**，含 `device_*` 调试四件套）。
- **P0 保持已验证**（0.14.4 根因 #1 / 0.14.6 根因 #2，见历史节与
  `project-workflow/sleep-modes/status.md` §6）。
- **P1 结案（0.14.13）**：深睡 Zzz 不显示/残影的根因是**局刷基线漂移**——唤醒时
  `epdThinBegin` 的电源脉冲会重置面板控制器，而固件 `lastDisplayedFrame` 仍按旧
  内容做局刷，导致图标被跳过或与 BT 残影叠加。修复：进 deep 渲染睡眠图标前
  `epdPartialReady=false` 强制一次全刷（0.14.13）。用户现场确认：**睡眠中 Zzz
  显示、时钟每分钟更新**。
  取证工具：`frame_capture` + `GET /frame?which=saved`（睡前帧，与模板逐像素
  0/256 差异）、`deep.glyph`（渲染是否执行）。
- **OTA 逻辑修复（用户实测踩坑后发现并修复，0.14.12 + 桥）**：
  1. 固件未处理 `UPLOAD_FILE_ABORTED` → `Update` 卡在 "already running"、OTA 锁
     不释放、屏卡 OTA、之后所有上传必失败；现中止/写入/结束失败均清理
     （`Update.abort()` + 解锁 + 恢复界面），并加 20s 停滞看门狗与
     `POST /diag?ota_abort=1` 远程急救。
  2. 桥 OTA 前探测 2s 超时在 light sleep 下常误判离线（实测 HTTP 冷响应
     10–14s）→ 改 10s + 重试；上传前先调 `/diag?ota_abort=1` 清残留；检查响应体
     `UPDATE FAILED`（HTTP 200 也失败）；队列 flush 失败退避 60s→2m→4m→…≤30m
     （新请求立即重试）。
  3. 验证：`firmware_ota 0.14.12→0.14.13` 一次直传成功。
- **桥侧调试工具套件（默认不改行为，MCP 按需开启）**：
  - `device_sleep`（推 deep 并保持）、`device_wake`（固定 light 回在线）、
    `device_mode auto|deep|light`（绕过 10 分钟安静迟滞）、`device_contact_s s`
    （覆盖 pull 间隔，加速循环）。
  - 设备侧：`/diag?deep_usb=1`（插电可睡）、`deep_now=1`（立即睡）、
    `render_mode=deep|light`（在线渲染深睡帧）、`frame_capture=1` +
    `GET /frame?which=frame|last|saved`（帧缓冲 PBM）、`nvs_stage`（NVS 面包屑）。
- **新发现（下一窗口首批任务）**：
  1. **时钟与 PC 不同步**：pull 响应里的 `server_time` 取自 poller 生成的信封
     （可能滞后数十秒~分钟），设备每次苏醒按它校时 → 与 PC 有偏差。应在 pull
     响应生成时用 `now` 重盖 `server_time`。
  2. **时区应随 PC**：现为固件默认 `CST-8`。桥应在 pull/push 带
     `tz_offset_min`，固件按 POSIX 反向符号生成 `TZ`（如 +480 → `UTC-8:00`）
     并持久化 NVS `pm/tz`，CST-8 仅作缺省。
- **P2 待做**：pending 模板/OTA 排队冲刷、迟滞升降级的真机端到端、深睡 ≥2h
  长测；详见 `project-workflow/sleep-modes/prompt.md`。
- **ROM**：0.14.13 `artifacts/codex-status-0.14.13-bw.bin`
  `F6D9C2F085FF32A09A8A36DC66531CEAE46225F951EB1EB7803BAA0E58E05FCF`；
  0.14.12 `DD10BC1D…D135`、0.14.11 `4CC2CF0B…78AF`、0.14.10 `B01375C3…2868`、
  0.14.9 `090C2FBB…6FC82`、0.14.8 `F9797CC6…4213`；回滚
  `artifacts/codex-status-0.13.8-bw.bin`。
- **未提交**：`PROGRESS.md`、`docs/power-state.md`、`src/*`（0.14.3–0.14.13）、
  `bridge/crates/*`（activity debug_mode、device_* 工具、OTA 探测/退避）、
  `tools/test-bridge/bridge.py`、`tools/*.mjs`、
  `project-workflow/{deep-pull-test,sleep-modes}/`、`AGENTS.md`（工具清单）。

## 历史：2026-09-20 深夜（四）— P0 电池深睡双根因均已修复并现场验证（0.14.8）

- **现场**：设备 `0.14.8-bw`（时区修复版；QUAD v10 `86a51357` active）；桥 =
  debug 构建（父 PID **44016** + watchdog **51124**，:8765/:8766/:8767）。
- **P0 结论（证据链：`project-workflow/sleep-modes/status.md` §6）**：
  1. **根因 #1（0.14.4 修复）**：唤醒时 `releaseWakeHolds()` 先释放 GPIO hold
     再设电平 → GPIO17（BAT_Control 锁存）被复位后的输出寄存器瞬时拉低 →
     断电。改为无毛刺释放（先恢复 17=HIGH、6=LOW 再 `gpio_hold_dis`）。
  2. **根因 #2（0.14.6 修复）**：deep 网络窗口 pull 成功后渲染分支再次
     `epdBegin()` → 第二次 `SPI.beginTransaction()` 在 Arduino 非递归
     `paramLock` 上自锁（项目从不 `endTransaction`）。0.14.5 NVS 细码
     `nvs_stage_boot=43` 定位（44 未写）；修复：`DEV_Module_Init` 每 boot 幂等、
     渲染分支仅在 `!frame` 时重试、缓冲单次分配（顺带修泄漏）。
  3. **验证**：0.14.6/0.14.7 现场——进 deep → thin 唤醒（`clock_wakes=13`）→
     网络 pull 成功（`last_code=200 net_fails=0`）→ pull 后时钟持续更新、
     `retry_stage=0`；BOOT 单击 ext1 回 light 正常。
- **本轮新增（测试/诊断开关）**：
  - `POST /diag?deep_usb=1` + `deep_now=1`：**插电也可进 deep**（不必拔线），
    非 TIMER 复位/BOOT 唤醒自动清零；串口 `deepusb on|off`；
    `/status.json` 暴露 `deep_usb`。
  - **时区修复（0.14.8）**：桥 `server_time` 是 UTC，固件未设 TZ → 时钟一直
    显示 UTC（14:45 vs 本地 22:45）。新增默认 `CST-8`、NVS `pm/tz`、
    `/diag?tz=`；现场确认显示本地时间。
  - NVS 面包屑（`/diag?...&nvs_stage=1`，读 `nvs_stage_boot`）保留，默认关。
- **下一步**：P1（深睡 Zzz “覆盖蓝牙标识”，拍屏取证）；P2（T4/T5 真机端到端：
  deep 期 pending 模板/OTA 排队冲刷、迟滞升降级、BLE 打断、桥不可达/token
  失效/低电/旧桥兼容）；深睡长时间 soak（≥2h）与功耗斜率。
- **ROM**：0.14.8 `artifacts/codex-status-0.14.8-bw.bin`
  `F9797CC64BBDA6211BCBE9FF5CB8695DBA1F1A98F60E608C776EFC4962324213`；
  0.14.7 `B963E166…3F90`、0.14.6 `693CE5BB…D2F7`、0.14.5 `C3D1E484…9A00`、
  0.14.4 `CA263411…D479E`、0.14.3 `14A8EC17…BEC5`；回滚
  `artifacts/codex-status-0.13.8-bw.bin`。
- **未提交**：`PROGRESS.md`、`docs/power-state.md`、`src/*`（0.14.3–0.14.8 诊断/
  修复）、`bridge/crates/*`、`tools/test-bridge/bridge.py`、`tools/*.mjs`、
  `project-workflow/{deep-pull-test,sleep-modes}/`（artifacts 不入库）。

## 历史：2026-09-20 深夜（二）— sleep-modes T1–T5 实现 + 0.14.2 电池深睡待回归

- **现场**：设备 `0.14.2-bw`（USB 刷入 ota_0，next ota_1；当前插电 light，
  QUAD v10 `86a51357` active，时钟窗口 60B 就绪，USB=COM4）；桥 = 新 debug 构建
  （父 PID **25184** + watchdog **33412**，:8765/:8766/:8767，含 activity/pull/deep）。
- **已完成（未提交）**：
  1. **固件 0.14.x**（`src/main.cpp`、`EPD_SSD1681.*`、`template_engine.*`、
     `usage_client.*`）：deep/light 双模式（`rtcMode`+NVS `pm`，空闲阈值默认
     600s，`POST /diag?idle_deep_s=` 可调）；深睡分钟只做**时钟窗口直写**
     （thin wake，`epdThinBegin`+`clockTickWake`）；按 `next_contact_s` 快连
     反向拉取 `GET /usage` 并执行 `mode/next_contact_s/usage_rev/pending`；
     离线重试 1m×3→5m×3→15m；`POST /deep` 通知；pending 窗口 180s；
     新绑定 `device.mode`（deep/light），deep 时 `device.state="DEEP"`；
     串口 CLI `deep`/`light`；`/status.json` 新增 mode/deep 统计/阶段码
     `stage`/`last_wake_code`；5h 100% 的滚动 `resetsAt` 不再触发 NVS 写。
  2. **睡眠图标**：Zzz 16px（手工像素网格，另有 12/20px），与 BT 同格
     (101,6)、条件互斥；`enterDeep` 切换模式后立即重绘显示，pull 回 light 后
     重绘隐藏。生成器 `artifacts/gen-sleep-icon.py`；quad v10 已同步到
     `tools/test-bridge/templates/quad.json` 与运行时库（version 10，
     min_fw 0.14，hash `86a51357`）。
  3. **桥**（`core/activity.rs` 新增，core/app/mcp/render 接线）：
     `usage_rev`/`last_change_at`/迟滞（light 300s、静默 600s→deep）、
     pull 响应附 `mode/next_contact_s/usage_rev/pending` 并打 `device pull:` 日志、
     `POST /deep`、deep 期间推送跳过且不计失败、接触后补推、pending 模板/OTA
     排队与窗口内冲刷、`device.mode` 进入 Rust 模板校验与渲染 FFI。
  4. **T5**：`cargo test --workspace` 全绿（隔离 `artifacts/cargo-target-sleepmodes`）；
     `node tools/test-quad-preview.mjs` 全绿（含 deep 图标断言）；Python 测试桥
     实现同语义 pull/`POST /deep`；`docs/power-state.md` §13 已改为“已实现”
     并补 §13.8 实现映射。
- **P0 未解（新窗口首要任务）**：**电池深睡后失联**。0.14.0 电池深睡后花屏+
  不可达（面板电源未保持，0.14.1 已加 GPIO6/17 deep-sleep hold）；0.14.1 电池
  回归 8 分钟无 pull、BOOT 无反应（插电时用 CLI `deep` 验证定时唤醒正常）。
  0.14.2 已改为 thin 路径 `deepSleepRaw`（不碰 WiFi/BLE 驱动）、`deepSleepFor`
  先判 `WiFi.getMode()`，并加 RTC 阶段码，**待电池回归**（步骤/判读见
  `project-workflow/sleep-modes/prompt.md` §P0）。
- **P1**：用户报告深睡 Zzz “覆盖蓝牙标识”，待拍屏取证；若是深睡渲染未擦除，
  模板加 white erase rect 或引擎 icon 前清区域（三端同步）。
- **ROM**：`artifacts/codex-status-0.14.2-bw.bin`（1619392 B）SHA256
  `B5162217F6504052818D4F26A6C2E6023DDC467F4968C4132332085C68F5F00B`；
  0.14.1 `741E4FEF…E124`、0.14.0（含图标支持）`AFE8FD71…E401`；回滚
  `artifacts/codex-status-0.13.8-bw.bin`（`2E5CF982…1C5A`）。
- **专项文档**：`project-workflow/sleep-modes/{status.md（详细）,prompt.md（新窗口交接）}`。
- **未提交**：`PROGRESS.md`、`docs/power-state.md`、`src/*`、`bridge/crates/*`、
  `tools/test-bridge/bridge.py`、`tools/generate-quad-preview.mjs`、
  `tools/test-quad-preview.mjs`、`project-workflow/{deep-pull-test,sleep-modes}/`
  （artifacts 不入库）。

## 历史：2026-09-20 晚 — 时钟区域直写 A/B 实测 + v0.14 deep/light 双模式设计定稿

- **现场**：设备已回滚 `0.13.8-bw`（ota_1，next ota_0，light sleep 在线，
  电量 ~80%，`fw`/模板/owner 正常）；桥 debug 父 PID **42344** + watchdog
  **33344**；A/B 测试固件 `0.13.9-clkwin` 已 OTA 验证并回滚。
- **预留区域从激活模板计算**（测试固件实现）：扫描 `elements[]` 中
  `bind=="device.now"` 的 text 元素，quad v9（`f12`,`x=161`,`y=9`）→ 预留
  `x=161..195 y=9..20`（35×12px，窗口字节列 20..24、5×12B = **60B**）；模板无
  该元素则不预留。
- **A/B 实测（各 5 次，`artifacts/clkwin-ab-log.txt`）**：
  - A（现行：模板渲染 + 整帧局刷）**864.4ms**（862.2–866.3）；
  - B（时钟窗口直写）**795.6ms**（793.9–797.0）= build 0.45ms + 面板唤醒/
    reset 215ms + 局部波形 580ms + 睡 0.03ms；
  - 差异仅 ~69ms（渲染/diff 部分）；**面板波形与唤醒是与窗口大小无关的固定
    成本**。B 的价值 = 深睡可用（仅 60B RTC 状态，不需 template/usage/帧缓存）。
- **实现**：`EPD_SSD1681` 新增 `WakePartialWindow`/`DisplayPartWindow`
  （Y 映射 `RAM y = 199 - screen y`；0x26 只用 RTC 旧像素回填窗口）；
  `src/main.cpp` 测试块（`#ifdef CODEX_CLK_WINDOW_TEST`）保留，默认构建零影响。
- **设计定稿**：`docs/power-state.md` **§13**（deep 时钟周期/网络周期、桥
  `mode`/`next_contact_s` 决策 + 迟滞、离线 1m×3→5m×3→15m、回深 HTTP 通知、
  手动唤醒、功耗估算与开放问题）；实现计划与新窗口交接
  `project-workflow/sleep-modes/{plan,prompt}.md`。
- **未提交**：`PROGRESS.md`、`docs/power-state.md`、`src/main.cpp`、
  `src/EPD_SSD1681.{h,cpp}`、`bridge/crates/{core,app}` 与
  `tools/test-bridge/bridge.py`（rate-limit 通知/3min 兜底，已写未启用）、
  `project-workflow/{deep-pull-test,sleep-modes}/`。

## 历史：2026-09-20 下午 — deep-pull 计时测试（0.13.9-dptest，已回滚 0.13.8-bw）

- **现场**：设备已回滚 `0.13.8-bw`（ota_1，next ota_0，`fw`/`slot` 实测），
  light sleep、push/owner/Template active=quad v9 正常，电量 91%；桥 debug
  父 PID **35880** + watchdog **21568**（`tools/start-bridge.ps1` 新起）。
- **目的**：实测"深睡唤醒 → 快连 Wi-Fi → 反向拉取 bridge `GET /usage` → 进入
  light sleep"的墙钟与 CPU 时间（方案/结果：`project-workflow/deep-pull-test/`）。
- **测试固件**（`#ifdef CODEX_DEEPPULL_TEST`，`src/main.cpp`；`FW_VERSION=0.13.9-dptest`/`-dptest2`，
  经 `PLATFORMIO_BUILD_FLAGS=-DCODEX_DEEPPULL_TEST=1` 构建，默认构建零影响）：
  OTA 后首 boot 正常连接一次并把 AP channel/BSSID 存 RTC，随后 5 个周期
  `deepSleepFor(30)` → 无扫描 `WiFi.begin(ssid,pass,ch,bssid)` → `GET /usage`
  （Bearer 取设备端 `EndpointRec.token`）→ 计时/模式统计存 RTC，第 5 周期后
  进正常 light sleep 在线，供回滚 OTA。
- **结果（两轮各 5 周期，10/10 `fast=1 code=200`）**：
  - `wifi_ms`（关联+DHCP）**1.3–1.5s**（一次异常 3.3s）；`http_ms`（584B
    envelope）**0.06–0.63s**；parse ~1ms；cycle 合计 **1.5–2.0s**；加 boot+setup
    约 0.9s，**boot→拉取完成 ~2.7s**。
  - CPU 时间（dptest2 末周期，`Time since boot 2 699 874 µs`）：SLEEP
    0.59s(21%)、APB_MIN 0.62s(22%)、APB_MAX 0.74s(27%)、CPU_MAX 0.75s(27%)
    → **CPU 运行态 2.11s（78%）**，等待期间由 Wi-Fi PM 锁维持 APB 档而非
    light sleep；此前"CPU 0.3–0.7s"的估算偏乐观 3–4 倍。
  - 能耗粗算一次 cycle ≈0.017mAh（模式电流粗估；测试时在充电，未用电压
    斜率校准）：1min 间隔 ≈24mAh/天，5min ≈5mAh/天。
- **结论**：反向拉取 + 定时深睡可行且稳定；主耗时是 Wi-Fi 关联/DHCP，不是
  拉取本身；deep 窗口未启动 WebServer（桥无法 push/OTA），产品化需按需在
  窗口内起 server 或先切 light sleep。
- **回滚**：MCP `firmware_ota` `0.13.9-dptest2 -> 0.13.8-bw` 成功（`artifacts/
  codex-status-0.13.8-bw.bin`）。证据：`artifacts/deep-pull-log-dptest{,2}.txt`。
- **未提交**：`src/main.cpp`（ifdef 守卫测试代码）、`project-workflow/deep-pull-test/`、
  本文件。平台配置未改（构建 flag 走环境变量）。

## 历史：2026-09-19 深夜（三）— 0.13.8-bw：Wi-Fi 活跃窗口调优 + 1Hz 合并心跳

- **现场**：设备 `0.13.8-bw`（ota_0，next ota_1，OTA 0.13.7→0.13.8 成功），
  active=quad v9（`417f22a7`）；桥 debug 父 PID 38876 + watchdog 24332，
  owner=`1a2b`。ROM `artifacts/codex-status-0.13.8-bw.bin`（1604048 B）SHA256
  `2E5CF9826C94E6551B6C5C284AE248480035C68EC3F664A0D7A4F5D091FF1C5A`。
- **背景（官方/社区对照）**：ESP-IDF v6.1 Low Power Modes（S3）Auto Light-sleep +
  Wi-Fi 平均电流 DTIM1/3/10 = **2.45 / 1.33 / 0.93 mA**（Modem-sleep 38–40mA，
  Deep-sleep 6.9µA），推荐 HZ=1000、IDLE_TIME_BEFORE_SLEEP=3（本工程已一致）；
  社区同类唤醒源：驱动持 PM 锁（esp-idf #10368 RMT/led_strip）、lwIP 后台
  ARP/DHCP timer（#18029）。本工程最大清醒来源为 Kconfig
  `ESP_WIFI_SLP_DEFAULT_MIN_ACTIVE_TIME=50ms`（实测 wifi PM 锁 ~48.8ms/次、
  ~1.15 次/s ≈ 3.4s/min，占清醒预算约 2/3）。
- **F1（`platformio.ini` custom_sdkconfig）**：`..._MIN_ACTIVE_TIME=20`、
  `..._WAIT_BROADCAST_DATA_TIME=10`、`..._MAX_ACTIVE_TIME=60`。注意 kconfgen 对
  已有 sdkconfig 的值优先，改后须删 `sdkconfig.esp32-s3-epaper-154g` 强制按
  defaults 重生成（全量 core 重编 ~16.5 min；构建日志与生成物已确认三值生效）。
- **F2（`src/main.cpp`）**：loop 尾部改 **1Hz 合并心跳**——离线分钟与
  `device.now` 共用一次 `time()`（此前离线块仍每轮 `timeKnown()`/`time()`，是
  0.13.7 漏改的同类项）；`serviceAnnounce()` 的 IP 变化检查 1Hz 门控，并删除
  `pollWifi()` 中重复的每轮 `WiFi.localIP()` 比较；`FW_VERSION=0.13.8-bw`。
- **实测 A/B（60s 窗口，loop 25ms；`/diag` 仅供 loop_delay 曲线）**：
  - 0.13.8 稳态（无桥推送窗口）：SLEEP **92.3–93.3%**、awake/wake **1.6–1.9ms**、
    wifi 锁 **24–32ms/次**；含 5min 心跳推送的窗口被压到 ~88–90%（推送 + EPD
    约 1–2s 清醒）。
  - 对照 0.13.7：wifi 锁 48.8ms/次、awake/wake 2.27ms、SLEEP 88.1–91.5%。
  - 0.13.8 上 loop_delay 50/100/200/500ms 不再提升 SLEEP（awake/wake 随 delay
    增大、环境 Wi-Fi 流量与心跳构成下限），默认 25ms 保持。
  - 桥 `usage push -> 200 OK` 每 5min 持续成功；`/status.json`、
    `templates quad active`、EPD 正常。
- **文档**：`docs/power-state.md` §10 已补 Kconfig 与 1Hz 说明；专项
  `project-workflow/pmstats/task-3.md`。
- **待办**：提交（等用户确认）；T10 电池斜率复测（0.13.3+ 未做）；可选进一步
  （listen_interval 10→20 换 ~1s 推送延迟、MIN_ACTIVE_TIME 再降到 10–12ms、
  关 `LWIP_ESP_GRATUITOUS_ARP`）——当前收益递减，先观察实际续航。

## 历史：2026-09-19 深夜（二）— 修复 0.13.6 时钟重绘的功耗回归（0.13.7-bw OTA）+ 桥 label 重读

- **现场**：设备 `0.13.7-bw`（ota_1，next ota_0，OTA 0.13.6→0.13.7 成功），
  active=quad v9（`417f22a7`）；桥 debug 父 PID **38876** + watchdog
  **24332**（重建，含 label 重读），owner=`1a2b`。ROM
  `artifacts/codex-status-0.13.7-bw.bin`（1604080 B）SHA256
  `1E2131E875429FBA6EB5E1956E2F3D4EB59F4B8B5AC425342770D7706BBC42B7`。
- **问题（本轮排查）**：0.13.6 上线后 SLEEP **26%** / CPU_MAX **62%**
  （基线 86–93% / 7–9%），rtos0 非空闲锁 ~68%、每次唤醒 ~4.6ms（正常
  ~0.2ms）、每次实睡 7.6ms（本意 25ms）。回滚 0.13.3 / 0.13.5 ROM 做
  同网络同桥 A/B 均 90–93% → 确认 0.13.6 固件回归；又用现成 ROM 逐版
  二分 + 定向重建（60s 窗口）：原版 26.7% → 仅缓存标志位 83% → 缓存
  +1Hz 节流 **91.7–92.7%** → 时钟块完全禁用上限 93.8%。
- **根因**：0.13.6 时钟重绘块每轮 loop 对 ~8KB（PSRAM）模板做
  `activeTplJson.indexOf("device.now")` 且每轮 `timeKnown()/time()`；
  唤醒路径被反复打断/拖慢（本板 40MHz 闪存，缓存被冲掉即放大），
  light sleep 碎片化。桥侧/HTTP 流量/环境（RSSI 波动）经对照实验排除。
- **修复（0.13.7）**：`tplCacheLoad()` 加载时缓存 `activeTplHasNow`；
  分钟检查 `millis()` 门控 1Hz 才取一次 `time()`。时钟行为不变
  （EPD +1 次/min）。
- **桥 label 重读**：`core/runtime.rs` 在 `account/read` 失败/为空时每个
  轮询周期重试并打 `account label recovered`（此前每 app-server 会话只读
  一次，20:49 的 `workspace routing discovery timed out` 使用户名整段会话
  缺失）。`cargo test -p bridge-core` 15 项过；桥重建重启后
  `usage push 200 OK`。
- **验证**：0.13.7 实测 SLEEP 92.7%、CPU_MAX 6.9%、EPD +1/min；`pio run` 过。
- **待办**：桥上残留 A/B 用的 `ab-0133` profile（可留作对照）。

## 历史：2026-09-19 深夜 — 设备时钟 `device.now`（每分钟走）+ 0.13.6-bw

- **现场**：设备 `0.13.6-bw`（ota_0，next ota_1，OTA 0.13.5→0.13.6 成功），
  active=quad **v9**（`417f22a7`）；桥 debug 父 PID **17872** + watchdog
  **24352**，owner=`1a2b`。ROM `artifacts/codex-status-0.13.6-bw.bin`
  （1604000 B）SHA256
  `D70975D16B422CCB929A1655FFBE4E7052778760C37927D84291B31554EB713A`。
- **问题**：右上时间绑的是 `server_time` = 桥推送时刻，桥 5min 才推一次，
  设备只在事件（推送/状态变化/失联分钟）时重绘 → 分钟不走。
- **改动**：
  1. 新绑定 `device.now` = 设备本地时钟 `HH:MM`（`server_time` 同步 + RTC；
     时间未知时不存在）：`src/template_engine.cpp`（parseBind/evalBind/
     bindExists）+ Rust `core/template.rs`（`BindSpec::DeviceNow`）+ 测试；
  2. `main.cpp` loop：仅当 `activeTplJson` 引用 `device.now` 且时间已知时，
     分钟变化触发一次 `renderCurrent()`（不引用则维持"仅事件重绘"）；
  3. quad v9：时间元素 `server_time` → `device.now`（去 `time_format`，
     `when exists`），`min_fw=0.13.6`，已保存并推送 `default`。
- **验证**：`EPD writes 6→7 / 75s`（期间无推送）、`/log` 每分钟
  `[tpl] rendered quad`；`cargo test --workspace` 全过（隔离
  `artifacts/cargo-target-equals`）；`pio run` 过；OTA 成功。
- **注意**：电池供电且设备清醒时，每分钟一次局部刷新（~300ms）有功耗代价。
- **待办**：提交（等用户确认）；`docs/power-state.md` §7 已补 `device.now`
  与 quad v9 说明。

## 历史：2026-09-19 深夜 — `when.equals` + 状态图标落地（0.13.5-bw OTA）+ quad-status v6

- **现场**：设备 `0.13.5-bw`（ota_1，next ota_0，OTA 0.13.4→0.13.5 成功），
  active=quad（`cf5004bb`）；桥 debug 父 PID **1236** + watchdog **39116**（新
  构建，含 equals），owner=`1a2b`。ROM `artifacts/codex-status-0.13.5-bw.bin`
  （1603680 B）SHA256
  `426293AA9A64893242229A71128CF4A76A892C65A3F8602E1B226D4A634D7B0A`。
- **引擎扩展 `when.equals`**（字符串/整数，与 `equals` 渲染文本比较；与原
  `exists` 互斥；旧模板零影响）：
  - 固件 `src/template_engine.cpp:300`（parseCondition）/`:333`（conditionMatches）；
  - Rust 校验 `bridge/crates/core/src/template.rs:167`（键=bind + exists|equals，
    equals 仅 string/i64/u64）；测试 `bridge/crates/core/tests/template.rs`
    （接受/拒绝用例）；
  - `cargo test --workspace` 全过（隔离 `artifacts/cargo-target-equals`，不动
    运行中 target）；`pio run` 过；`node tools/test-quad-preview.mjs` 7 fixtures 过。
- **图标方案**（1bpp，12/16/20；生成器在 `artifacts/`）：
  - Bridge=显示器图标，`gen-bridge-icons.py` 出 `bridge`/`bridge-off`（斜杠与
    笔画间留断缝）；在线=素，`offline_mins` 或 `WIFI OFF` 时斜杠；
  - WiFi OFF 按 icons8 id=120598 方案重绘：断弧 + 中央"!"（竖线+圆点），
    `gen-bt-wifi-icons.py` 的 wifi-off 更新（12/16 两条弧、20 三条弧）；
    On 保持满弧+圆点，与 Off 同弧线几何；
  - BT 仅 `state=BLE ON` 显示（BLE OFF/WIFI OFF 隐藏）；先前误用的"坏
    bridge-off bits 当 BT 斜杠"已废弃。
- **模板 `quad-status` v6**（canonical hash `5bb8c1e7`，min_fw=0.13.5，已保存
  到运行中桥，未推送）：右上第一行 `[BT][WiFi][Bridge][时间]`（x=101/121/141，
  server_time hhmm@161）；右列底对齐（RC 在=28/46/64/82，RC 无=46/64/82，
  用白色矩形擦除实现 label 位置切换）；移除 `device.state` 文本行，左下
  5H 158 / BATT 172 / OFF|SYNC 186。
- **预览**：四态 `artifacts/final-{bleon,bleoff,wifieoff,offline}.png` +
  `quad-status-final-4up.png`（A4 候选 `quad-status-eqA4-wifi-bang.json`）。
- **待办**：
  1. 推送 `quad-status`（用户显式动作）：目前不在任何 profile，需
     `profile_save`（如 status=[quad-status]）再 `profile_push`；
  2. `docs/power-state.md` 模板协议一节补 `equals` 与 `device.state` 取值；
  3. PROGRESS/AGENTS 提及的图标资产路径与方案定稿归档。
- **提交**：`d51f529`（Firmware 0.13.5 + bridge: when equals, state-driven
  status icons, quad v8；含 `main.cpp`/`template_engine.cpp`/core 模板校验/
  测试/docs §7/本文件/prompt）；图标生成器与产物在 `artifacts/`（gitignored）。

## 历史：2026-09-19 深夜 — 图标专题：出厂 ROM 资产还原 + BT/WiFi 12/16/20（新窗口续）

- **现场**：设备 `0.13.4-bw`（ota_0）、owner=`1a2b`；桥 debug 父 PID **12000** +
  watchdog **19944**，:8765/:8766 监听。图标产物在 `artifacts/`（gitignored）。
- **出厂 ROM 分析**（备份 `D:\codex_status_backup\factory-backup-154bw\factory-full-8mb.bin`，
  8 MB；分区仅 nvs/phy_init/factory，资源编进 app；官方源码 =
  `waveshareteam/ESP32-S3-ePaper-1.54` 的 `02_Example/ESP-IDF/V2/11_FactoryProgram`，
  临时 clone 在 `C:\Users\user\AppData\Local\Temp\opencode\ws154`）：
  - 状态图标 3 个 **20×20 RGB565A8**（1200 B）：battery ROM@`0x9a1f4`、
    wendu 温度计@`0x9ab8c`、shidu 水滴@`0x9a6c0`；整屏图 `_3_`
    **200×200** RGB565A8（120000 B）@`0x7cd18`（椰子 logo，隐藏第二页）。
  - 字体：Montserrat Medium 12/16/17/20/**67**（大数字字形框 39×49）+
    MiSans Regular 20（**无 `°` 字形**，温度只显示数字）。
  - 1bpp 转换规则（工厂 flush）：`rgb565 < 0x7fff → 黑`。
  - 还原产物：`factory-rom-screen-mockup.png`（+`@3x`，出厂首页 08/10 + 图标 +
    数值）、`factory-rom-icons.png`（三图标 8×）、`factory-rom-bg-200x200.png`
    （+`@2x`）、`factory-rom-icons-template.json`/`-render.png`（20×20 bits，
    固件引擎渲染通过）；脚本 `factory-rom-recon-assets.py`、`factory-rom-recon-ui.py`。
- **BT/WiFi 图标（12/16/20，1bpp）**：
  - 参考 icons8（`icons8.com/icon/g4cZhJp3J87c/bluetooth` ios 风格，WiFi 同款
    `img.icons8.com/ios/500/wifi.png`）；按 500px 原图量得几何：竖线 x=0.5
    （y 0.02..0.98）、两三角顶点 x=0.75（y 0.28/0.72）、左臂自由端 x=0.25
    （"左缺边三角 + 右两三角 + 外轮廓包裹"；即 bluetooth-b 结构、两旗在右）。
  - 生成器 `artifacts/gen-bt-wifi-icons.py`（几何超采样+阈值，可复现）；
    产物 `icon-bluetooth-{12,16,20}.png`（+`-8x`）、`icon-wifi-{12,16,20}.png`
    （+`-8x`）、`icons-bt-wifi-sheet-10x.png`、`icons-bt-wifi-bits.json`；
    示例模板 `icons-bt-wifi-template.json`/`.png` 经 MCP `template_validate/render`
    通过。
  - 实测孔洞：12px 2×1px、16px 2×4px、20px 2×7px；描边 12px≈1px，16/20≈1.5–2px。
- **下一步（新窗口，先看 `docs/history/icons-task.md`）**：
  1. 需求确认：12px 孔 1px 是否接受；是否要反色/填充或更粗描边变体。
  2. 落地：把图标放进模板（位置/大小）；如需按状态显示（如 WIFI OFF 隐藏
     WiFi 图标），当前 `when` 仅支持 `{"bind":…,"exists":bool}`，要三端扩展条件
     语法（固件 `template_engine.cpp`、Rust `core/template.rs`、测试哈希）。
  3. 更多图标：出厂 battery/wendu/shidu 已有 20×20 bits；其余按上述风格新画。
- **未提交**：本文件、`docs/history/icons-task.md`（图标交接）、artifacts 产物（不入库）。

## 历史：2026-09-19 深夜 — device-discovery 完成：MAC 身份 + ARP/BLE 发现回退 + claim/lease 占用（固件 0.13.4-bw 已 OTA）

- **现场**：设备 `0.13.4-bw`（ota_0，next ota_1）、`192.168.1.50`、`BLE ON`
  （验证用会话，120s 后自动关）、owner=`1a2b`（本桥，host 192.168.1.100:8765，
  lease 300，60s 续约）。桥 = 最新 debug 构建：父 PID **12000** + watchdog
  **19944**，:8765/:8766 监听；
  MCP 工具新增设备类（见下）。ROM 归档
  `artifacts/codex-status-0.13.4-bw.bin`（1603200 bytes）SHA256
  `97DE8F3FB50E9469B47C10C09BDEC7B11E3E28E4EC2F528FD37D085FBEBD1B48`；
  可回退 `artifacts/codex-status-0.13.3-bw.bin`（`7BA1171D…7731`）。
- **专项**：`project-workflow/device-discovery/`（plan/task-1..4 均 done，
  task-5 本文档）。权威行为写入 `docs/power-state.md` §9.1/§9.2。
- **固件 0.13.4-bw**：
  1. 关 mDNS（`ArduinoOTA.setMdnsEnabled(false)`、去 `MDNS.addService`）；
     DHCP hostname 保留。实测 `?timers=1` 无 `mdns_timer`、`?diag=1` 无 mdns
     任务；`ipconfig /flushdns` 后 `codex-status-AABBCC.local` 解析失败。
  2. BLE info 增 `"mac"`；BLE 端到端已实测（用户单击 BOOT，`device_discover
     via=ble` 2–3 s 复用 bond 读取
     `{"fw":"0.13.4-bw","mac":"70:04:1D:AA:BB:CC","ip":"192.168.1.50","http_port":80,…}`）。
  3. `POST /claim` + owner（NVS）+ `/status.json.owner` + usage/template owner
     409 校验（`activate` 非旁路；无 owner 走旧规则且不建 owner）。
  4. MCP `firmware_ota` 实测 `0.13.3-bw -> 0.13.4-bw` 约 32 s。
- **桥**：
  - 身份：MAC 为唯一键（UDP/HTTP/BLE 学习并持久化
    `<exe>/data/bridge-app.json`）；显示名可改（MCP/面板 `device_rename`，默认
    `CodexStatus-AABBCC`）；IP 为属性；旧配置迁移无感。
  - 发现回退：`discovery.rs`（`GetIpNetTable` + UDP poke + `SendARP`，Windows）
    自动触发（HTTP 连续失败 ~20s）；`Pusher::read_device_info` +
    `device_discover {via: auto|arp|ble}`；`ble=1` cycle 顺带采纳 info。
    实测：配置 IP 改错重启 → 日志 `ARP fallback scan` →
    `device endpoint updated: 192.168.1.50 (via arp)`，随后推送恢复。
  - 占用：自动 claim/60s 续约/yield 让步；MCP `device_owner`/`device_claim
    {force?}`/`device_release`；面板设备页身份卡 + 按钮。claim 用设备 token
    缓存（`data/device-token.json`），401/缺失提示单击 BOOT。
  - 实测 409 无旁路（B usage+activate、无 hostId usage、B template、B release
    均 409）；B force 接管后 A 不推送并显示占用；A force 夺回；A release →
    B 可 claim；B `lease=60` 停续约 → 70s 后 owner `null`（不转移）；同版重传
    OTA 重启后 owner 仍在 NVS（`since` 收敛、桥自动续约）。
  - `CARGO_TARGET_DIR=artifacts/cargo-target-verify cargo test --workspace`
    全绿；面板 JS `node --check` 通过；`git diff --check` 干净。
- **下一步**：
  1. ARP 邻居表冷路径（真实换网场景）现场复测（本机无管理员权限无法删缓存模拟）。
  2. 提交本专项改动（等用户同意）。
- **未提交**：`PROGRESS.md`、`docs/history/icons-task.md`、`docs/power-state.md`、`src/main.cpp`、
  `src/owner_store.*`、`bridge/crates/{app,ble,core,mcp}`、`project-workflow/device-discovery/`。

## 历史：2026-09-19 深夜 — task-2 完成：loop 空闲默认改 25ms（0.13.3-bw 已 OTA）

- **现场**：设备 `0.13.3-bw`（ota_0，next ota_1）、`BLE OFF`、`pm_light_sleep=true`、电量 81%（墙上充电，`plugged=0` 属正常）、空闲 `loop_delay=25ms`。桥 = 最新 debug 构建：父 PID **31712** + watchdog **9844**，:8765/:8766 监听；MCP `pm_stats`/`firmware_ota` 可用。
- **结论（task-2，详见 `project-workflow/pmstats/task-2.md`）**：~100–140 次/s、~5ms 微睡眠**不是异常唤醒源**，是 `loop()` 尾部 `delay(5)`（HZ=1000 → 5 tick）→ 每个 loop 迭代一次自动 light sleep；IDF 文档对机制有述（空闲即睡到下一个唤醒点）、无次数量化口径。`loopTask` CPU 仅 1–2%/迭代，代价是每次唤醒固定开销（CPU_MAX 18–19%）。
- **证据**：
  1. **0.13.1-bw** 加 `GET /pmstats?timers=1`（`esp_timer_dump` + `CONFIG_ESP_TIMER_PROFILING`）：无 ~6ms 周期定时器；最高频 `mdns_timer` 100ms（10/s）。ROM SHA256 `81B39D9E8EE4F4AD5D50808D44E8D9214776A04343253B1FB41EFEDF7F13C485`。
  2. **0.13.2-bw** 加睡眠诊断（`CONFIG_PM_LIGHT_SLEEP_CALLBACKS`：直方图/唤醒源/均值）、`GET /pmstats?diag=1`（+任务与 run-time dump）、CLI `diag`、token 门控 `POST /diag?loop_delay=N`。60s 窗口 A/B：delay 5/10/25/50/100/200ms → 入睡 134/88/45/29.5/22.5/19 次/s，SLEEP 80/86/90/90/88/91%，CPU_MAX 18/12/—/7/7/6%，`/status.json` 往返 ~0.1/—/—/—/0.46/0.99s（含工具开销）。实际睡眠主导桶随 delay 移动；`<1ms` 桶是另一核未达 3-tick 门槛的尝试（`slept_us=0`，未实际入睡）；wake_cause `timer`≈全部、`wifi`≈1/s（beacon×`listen_interval=10`，与 DTIM 无关）、gpio=0。收益 ~50ms 后饱和（mdns 100ms 成下限），再拉长只换到成倍 HTTP 延迟 + BOOT 单击漏检/OTA 塌风险。
  3. `FREERTOS_HZ=100` 实验排除：Arduino `delay()`=`vTaskDelay(ms/portTICK_PERIOD_MS)`，HZ=100 时 `delay(5)`→0（忙轮询）只会更糟；`IDLE_TIME_BEFORE_SLEEP` 与根因无关，暂缓。
- **0.13.2 同版补丁（用户要求）**：BOOT 2s 只切模板，不再自动开 BLE（`docs/power-state.md` §4/§11 已更新）。ROM SHA256 `D8D1CEF28FEC31A31CF4A4C8B27535B510BB69CC93052E09D8FFD741BAD15482`。
- **0.13.3-bw（用户决定，当前运行）**：空闲 `loop_delay` 默认 **25ms**；`loopDelayForNow()` 在有 TCP 客户端（HTTP/OTA）时自动回 5ms。OTA 后实测 45.6 次/s、SLEEP 86%、CPU_MAX 9%。ROM SHA256 `7BA1171DB0CB6171EF2F8719A3DBA1F92A2F677AD57735438CC2D481DDAA7731`。
- **下一步**：
  1. **device-discovery 专项开工**（`project-workflow/device-discovery/`：MAC 唯一键 + 可编辑显示名、UDP/ARP/BLE 发现回退、显式 claim/lease 占用、固件关 mDNS；执行提示见 `docs/history/icons-task.md`）。
  2. 下次 OTA 全流程回归（25ms 空闲 + 传输自动 5ms 的吞吐验证）。
  3. T10 拔电电池斜率对照（5ms vs 25ms）。
  4. 0.13.1/0.13.2 诊断面（`?timers=1`/`?diag=1`/`POST /diag`）去留：倾向保留。
- **未提交**：`docs/history/icons-task.md`（新窗口交接）、`project-workflow/device-discovery/`、本文件的本次更新（`62fd7a8` 已含固件 0.13.1–0.13.3 与 pmstats 文档）。

## 历史：2026-09-19 — v0.13.0-bw：GET /pmstats + MCP pm_stats + 面板「功耗」tab

- **规格/专项**：`project-workflow/pmstats/`（plan/task-1/status）；`docs/power-state.md` §10 已补 HTTP 接口说明。
- **固件 0.13.0-bw（已 OTA）**：新增只读 `GET /pmstats`（免 token，与 `/log` 同级）返回 `esp_pm_dump_locks` 文本（`CONFIG_PM_PROFILING` 下其内部已追加 Mode/Sleep stats，勿再显式调用 `esp_pm_impl_dump_stats`，否则重复一段）。抽出 `pmStatsText()` 供 CLI `pmstats` 与 HTTP 共用。ROM `artifacts/codex-status-0.13.0-bw.bin`（1587712 bytes）SHA256 `AEE76FBACC30E58AA9FA5A54940E06B17D18316EF686DBCF5E3A33B764150555`。
- **bridge**：`core/device.rs::fetch_pmstats`（非 0.13 固件给清晰错误）；MCP 新增只读工具 `pm_stats {device_ip?}`；app 新增 Tauri `get_pmstats`（10s 缓存，避免高频读取扰动睡眠）+ `ui/index.html` 新「功耗」tab：休眠占比/light sleep 次数/平均每次休眠/拒绝次数/醒着时间、CPU 模式时间条、PM 锁表（Active>0 高亮）、原始输出；打开 tab 或手动「采样」（≥15s），不参与 3s 轮询。`get_mcp_info` 工具清单与 MCP `instructions` 已同步。
- **实测**：`GET /pmstats` 正常；MCP `tools/call pm_stats` 经 :8766 返回原文；面板 JS `node --check` 通过 + 真实输出正则解析测试通过（`40 M`/`5 %` 空格格式）；`cargo test --workspace` 16/16（隔离 target）；`pio run` OK。桥重建重启：PID **31712** + watchdog **9844**，:8765/:8766 监听。
- **稳态样本（OTA 后 ~273s）**：参考上节；**修正之前"约 1 次/秒"的口径**——实际是大量 ~6ms 微睡眠。
- **注意**：同版本重传 OTA 时 MCP `firmware_ota` 按"版本变化"判定会超时（上传实际成功、设备已重启，`uptime` 可证）——后续修复或重传前先 bump 版本。
- **未提交**：固件/bridge/UI/文档改动等用户同意。运行桥已是新构建。

## 历史：2026-09-18 — v0.12.6-bw + task-7 完成：模板推送改走 HTTP，quad v7 已上屏

- **规格**：`docs/power-state.md`（权威；§5/§9 已改为“BLE 仅身份/token，模板显式推送走 HTTP”）。专项文档 `project-workflow/power-state/`（task-1..7，task-7.md 为本次）。
- **task-7（HTTP 模板推送，完成）**：
  1. **固件 0.12.6-bw**：新增 `POST /template`（endpoint token 门控，`requestAuthorized` 不适用；query `id/version/hash/activate`，body 原始模板 JSON；`tplValidateForStorage` CRC/min_fw/dry-run → `tplStoreSave` → 可选激活 → `updateInfoExtra()` + 重绘；400/401/413/500）。未变化且仅需激活时不重写文件。ROM `artifacts/codex-status-0.12.6-bw.bin` SHA256 `82B0AF5D52F6A718324FCB9B1DC3E3D41732019D8C6208E3E389C45751FF9E63`；用 MCP `firmware_ota` 0.12.5→0.12.6（缓存 token，纯 HTTP）。
  2. **桥**：`bridge-mcp::push_templates_http(cfg, ids, activate)`——先 GET `/status.json` 比 hash 跳过未变化模板，再 `POST http://<device>/template?id=..&version=..&hash=..&activate=..`（`Authorization: Bearer <endpoint token>`，即 `cfg.token`）；激活目标最后发，未变化但非 active 时重发以激活。MCP `profile_push` 改用它（不再 `Pusher::cycle_once`）。
  3. **面板**：`app/src/main.rs` 的 `push_profile` 调同一 HTTP 函数并回显摘要；删除 `PendingPush` 与 BLE 循环中的模板消费，BLE 循环只保留 UDP `ble=1` 触发的 endpoint/token 交接（`template_ids: Some(vec![])`）。UI 文案改 HTTP 推送。
- **实测**：MCP `profile_push default` → `pushed 1 (quad); skipped 2 unchanged`；设备 `/status.json` quad `c598adc0` active、`/log` `[tpl] http saved id=quad`；重复推送 `pushed 0 (skipped 3 unchanged)`；`config-tbif`（mini 未变化但非 active）→ `pushed 1 (mini; activated)`，随后 default 恢复 quad active；错误码 401（无 token）/400（错 hash）/413（>32KB）实测。回归：cargo 16、python 4/4、node 7 fixtures、`git diff --check` 0。
- **运行现场**：桥新构建（task-7）父 PID 41820 + watchdog 子；TCP :8765/:8766 与 UDP :8767 均在；设备 `0.12.6-bw`（ota_1，next ota_0）、`state=BLE OFF`、电量 71%、`pm_light_sleep=true`、quad v7 active。
- **下一步（task-5 整机验收，剩余硬件/现场项）**：按键/LED（单击 `BLE ON`/`BLE OFF`、2s 切模板、15s AP、30s 恢复出厂）；插拔 120s 宽限与 20% 自动恢复；低电 <5% 断电；WIFI OFF 节奏（1m×3→5m×3→15m）；T9 push 延迟 / T10 电池斜率（`pmstats`+斜率，标注估算）；UDP 换 IP；停桥 >6min 看 `OFF N M`；Codex 升级重发现。
- **未提交**：task-7 改动（`src/main.cpp`、`bridge/crates/{mcp,app}`、`docs/power-state.md`、`AGENTS.md`、workflow/PROGRESS 文档）等用户同意。`AGENTS.md` 规则更新：只有“不是当前 debug/release 构建输出目录”下运行的桥/设备服务不得擅自停止；停当前目录的桥先结束 watchdog 再停父进程。
- **开放项**：Node 预览（独立 oracle）去留未定；运行数据 `bridge/target/debug/data/bridge-app.json` 的旧 `idle_template` 键被 serde 忽略，无害。

## 历史：2026-09-18 — v0.12.5 已提交（`bcd8238`）；当时缺口：模板推送必须走 HTTP（BLE 仅身份/token）

- **规格**：`docs/power-state.md`（权威）。专项文档 `project-workflow/power-state/`（plan/status/task-1）。
- **设计裁定（用户，2026-09-18）**：BLE 只负责配对/绑定、endpoint/token 下发、OTA token 取用（身份类）；其余逻辑（尤其模板推送）一律走 HTTP。当前桥的 `profile_push`（MCP/面板）仍经 BLE pusher 写 GATT → **缺口**。模板仍是显式推送动作（用户面板或 agent MCP），但传输必须改成 HTTP。
- **固件 0.12.5-bw（已 OTA，当前运行）**：常开单模式状态机（AP / BLE ON / BLE OFF / WIFI OFF·插电或电池 / 低电）；插电 = `usb_serial_jtag_is_connected()`；GP3 绿灯；BOOT 2s（切模板+BLE ON）/15s（AP）/30s（恢复出厂）；BLE 会话 120s 宽限 + `NimBLEDevice::deinit(true)`（light sleep 前提）；Wi-Fi 30s 失联判定、插电 60s 重试、电池 1min×3→5min×3→15min 深睡重试（按键唤醒清零）；低电 <5% 断电；UDP 通告 `{magic,mac,ip,port,proto,ble,fw}` → 255.255.255.255:8767；envelope `bridge.host/port` endpoint 自愈；`/status.json` 增 `state/ble_on/plugged/wifi_state/retry_stage/last_push/mac`、去窗口字段；CLI `pmstats`。0.12.1：OTA 屏名称/版本换行；去配对覆盖屏。0.12.2：`device.offline_mins` = 联系不上 bridge 的分钟数（>6min 才显示、每分钟重绘）。0.12.3：last sync 落 NVS（OTA 软复位清 RTC）；修启动时省电重关联被误判失联。0.12.4：模板只经显式推送，删除设备端自动拉取。**0.12.5：窗口按 duration 分类——新增 `buckets[...].monthly.*` 选择器（windowMins ≥43200），free/go 月窗口与 paid 周窗口可区分；模板文字（WEEK/MONTH）留在模板 JSON**。
- **协议 task-2（三端同步，全绿）**：元素 `mode` 语义删除（未知键忽略）；`device.idle_reason` 删除；`device.state` = `AP|BLE ON|BLE OFF|WIFI OFF`；`device.offline_mins` = 联系不上 bridge 的分钟数（>6min 才提供）。窗口按 duration 分类：`5h`(==300) / `weekly`(≥10080) / **`monthly`(≥43200，free/go)**。`quad` 现为 v7（`min_fw 0.12`，canonical hash `c598adc0`）：最后一行在线 `SYNC hhmm`、失联 `OFF N M`；左上标签按 `buckets[codex].monthly.remaining` 存在与否在 `WEEK`/`MONTH` 间切换。full/mini 哈希不变（`c1a2faaf`/`e6ba459e`）。Rust `core/render/mcp/app`、`tools/test-bridge/bridge.py`、Node 预览/测试同步。证据：cargo core 14/14 + ble 2/2、`cargo check -p bridge-app`、node 7 fixtures、python 4/4、`bridge-render --diff` 0 像素差。

- **桥 task-3/4（已部署运行）**：设备状态缓存（10s 缓存，面板读缓存）、UDP 8767 通告监听（已知 MAC 才可改 IP；首次仅接受配置地址；`ble=1` 触发一次 BLE 握手）、去周期 BLE 扫描（仅显式动作/UDP 请求）、envelope 带 `bridge.host/port`（设备端自愈）、`device_ip` 运行时可变且 IP 更新立即推一次；poller 在 spawn 失败后自动重新发现 codex.exe 路径（Codex 自动升级换目录）、启动时找不到 codex 不再阻塞 HTTP/MCP/BLE；隐藏 watchdog（`--watchdog <pid>`，exit 0 同退、异常退出重启、5min 内 3 次放弃并写 `logs/watchdog.log`）。**运行证据**：新 bridge 父 PID 49768 + watchdog 子 25644，:8765/:8766/:8767 均监听；`/usage` 实测 `bridge.host=192.168.1.100/port=8765` 且无 `idle_template`；MCP `bridge_status` 读设备成功；对父进程 `Stop-Process -Force` 后 watchdog 日志 `abnormal … relaunched` 且新父进程/新 watchdog 自动拉起；HTTP push 仍 `200 OK`。
- **MCP OTA（task-6，已部署并实测）**：桥新增设备 token 缓存（`<exe>/data/device-token.json`，经已绑定 BLE 链路获取；401 自动重取一次）+ MCP 工具 `firmware_ota {rom, device_ip?}`（校验 ROM → multipart 上传 `/doUpdate?token=` → 轮询 `/status.json` 确认重启后版本变化，单实例保护；`opencode.jsonc` MCP timeout 提到 180s）。**实测**：0.11.9→0.12.0 用时 40s（BLE 现场取 token）；0.12.0→0.12.1、0.12.1→0.12.2、0.12.2→0.12.3、0.12.3→0.12.4 均为纯 HTTP（缓存 token，无 BLE 会话）远程升级成功。
- **ROM**：`artifacts/codex-status-0.12.5-bw.bin`，SHA256 `7F13489935AB7CB371370FA52B4553F7B7E114D8FF59BE13F81D8BAE7C2E5A8D`（旧版本保留）。
- **设备现场**：`0.12.5-bw`、`state=BLE OFF`、quad 仍为 v6 `962cc1b2`；v7 `c598adc0` 已在仓库/运行数据但**未下发设备**——下发待 HTTP 模板推送实现（不再需要 BOOT 单击）。
- **下一步（task-7：HTTP 模板推送，新窗口从本条继续）**：
  1. **固件**：新增 `POST /template`（endpoint token 门控，`requestAuthorized` 不适用）：query `id/version/hash/activate`，body 为原始模板 JSON；`tplValidateForStorage`（CRC/min_fw/dry-run）→ `tplStoreSave` → 可选 `tplStoreSetActive` → `updateInfoExtra()` + 重绘；400/401/413 错误码。版本升 `0.12.6-bw`，用 MCP `firmware_ota` 远程 OTA。
  2. **桥**：`bridge-mcp` 新增 `push_templates_http(cfg, ids, activate)`：先 GET 设备 `/status.json` 比 hash 跳过未变化，再逐一 `POST http://<device>/template`（`Authorization: Bearer <endpoint token>`，即 `cfg.token`）；返回摘要。MCP `profile_push` 改用它（不再 `Pusher::cycle_once`）。
  3. **面板**：`app/src/main.rs` 的 `push_profile` 改调同一 HTTP 推送；删除 `PendingPush` 与 BLE 循环中的模板消费；BLE 循环只保留 UDP `ble=1` 触发的 endpoint/token 交接。
  4. **收尾**：HTTP 推送 quad v7 并核对 `/status.json` hash；更新 `docs/power-state.md` §5/§9（BLE 用途收敛为身份/token；模板显式推送走 HTTP）；跑 cargo/node/python 回归。
- **开放项**：Node 预览（独立 oracle）去留未定——用户询问其价值，未决策；若删除则用 `crates/render` golden PNG 回归替代。
- **验收步骤（task-5，剩余；task-7 完成后）**：HTTP 推送 v7；按键/LED（单击 `BLE ON`/`BLE OFF`）；15s AP / 30s 恢复出厂；插拔 120s 宽限、低电保护；T10 电池斜率 + `pmstats`；UDP 换 IP；Codex 升级重发现；停桥 >6min 看 `OFF N M`。
- **桥运行数据**：`<exe>/data/`（含 `bridge/target/debug/data/`）里 quad 已是 v7 `c598adc0`；`bridge-app.json` 的旧 `idle_template` 键被 serde 忽略，无害。
- **提交**：`bcd8238`（`Firmware 0.12.5 + bridge: single-mode power state machine, explicit template push, MCP OTA`，40 文件，提交时工作区干净）；此后仅本 PROGRESS 交接更新未提交。运行桥父 PID 50048（watchdog 304），`device-token.json` 已缓存，`firmware_ota` 可直接远程升级后续固件。

## 历史：2026-09-18 凌晨 — M3 task-7 完成主体：0.11.9-bw LIVE + PM light sleep 生效（T9 复测通过）

- **设备现场**：`0.11.9-bw`（ota_0，next ota_1）LIVE，`/status.json` 实测 `pm_light_sleep=true`、`live=true`、`channel=PUSH`、`endpoints=1`、templates full/mini/quad（quad active）、电量 87%。桥 HTTP push 在 PM 生效后实测 `200 OK`（T9 通过）。设备当前用 `stay`（USB 调试用，重启后失效）保持不睡。
- **本窗口关键根因（全部已修，均在 0.11.x 未提交改动里）**：
  1. **闪存 80MHz 不稳定（一切怪象的真因）**：本板 GD25Q64 在 80MHz 下 JEDEC ID 读成 `0B20E4`、页编程超时 → IDF 5.x 写保护回读失败/NVS+LittleFS 写坏；旧 IDF 4.4 的 ROM 实现侥幸可用。**40MHz 下 ID 正确（`00c84017`）且读写正常**。修复：`platformio.ini` `board_build.f_flash = 40000000L` + `tools/bootloader_40m_fix.py`（pioarduino 只带 80m/120m 预编译 bootloader，缺 40m ELF，pre 脚本用 80m ELF 补位；频率由 elf2image 按配置写头）。此前 OTA 后进 AP、回滚到 0.10.3、NVS 空、`restore cache fail` 全由此解释。
  2. **NimBLE 2.x 迁移**：广播塞 128 位服务 UUID 导致 `Data length exceeded`（0.11.8 起广播不再带 UUID，桥按名字扫描）；断开后不会自动恢复广播（0.11.9 在 `handleDisconnect` 里重开）。
  3. **`macSuffix()` 在新 core 下 Wi-Fi 未初始化时返回 `000000`** → 改用 `esp_read_mac()`（0.11.3）。
  4. BOOT 防误触守卫（`bootArmed`）：启动时 GPIO0 必须先读 HIGH 才接受按键；**副作用：深睡时"按着 BOOT 唤醒"不再触发 2s 配对窗口**，需先唤醒再按。
  5. 新增 CLI `stay`（USB 调试不睡）、`/status.json` 增 `wifi_slots`/`ap_reason`。
- **运维事件**：排查中做过全片擦除（NVS/LFS/绑定/token 全清），已通过 AP/串口重新配网（`wifi home-wifi <pass>` CLI）；BLE 重新配对后桥回推 endpoint/usage/模板，Wi-Fi/token 从此持久。**正常 OTA 不需要重配。**
- **双模式已撤**：`platformio.ini` 只剩 pioarduino/PM 单 env（stock env 退役，回退=OTA 0.10.3 ROM）；`listen_interval=10` + `WIFI_PS_MAX_MODEM`（0.11.6 起，`src/main.cpp configureWifiPowerSave()`）。
- **构建环境坑（沿用 task-7 §7）**：仓库改名 `codex_status` 后空格问题消失；`platform_packages` 钉 `tool-scons@4.11.1`；pioarduino 平台 `platform.json` 的 tool-scons `package-version` 改为 `4.41101.0`（否则平台会删包导致 `SCons.Tool.FortranCommon` 导入失败），备份 `artifacts/platform-espressif32-platform.json.bak`；自定义 libs 编译被打断后需删 `sdkconfig.defaults` 强制重编。
- **ROM**：`artifacts/codex-status-0.11.9-bw.bin`，SHA256 `DC5CCCAA388D112BDC9051FCEA1B4995C71C95CED3735DDFAFC09D70540BCEBF`。
- **待办**：T10 电池斜率（退出 `stay`、拔 USB，≥2h，对比 0.10.3 ~10%/h，标注估算）；T9/T11（无 token `/doUpdate` 401、正式 OTA 回归）；深睡/ext1 唤醒/按键回归；更新 `project-workflow/sleep-battery/task-7.md` §7.3 并更新 task 状态。**未提交（等用户同意）**。

## 历史：2026-09-17 晚 — M3 task-7 进行中（pm env / NimBLE 2.x / PM light sleep 代码已就位；构建环境有坑）

- **代码（未提交）**：`platformio.ini` 新增并行 `[env:esp32-s3-epaper-154g-pm]`（pioarduino 55.3.311 / Arduino core 3.3.11 / IDF 5.5.5；custom_sdkconfig 开 `PM_ENABLE`、tickless idle、CPU 掉电 light sleep、`PM_SLP_DISABLE_GPIO` 等）；`src/ble_bridge.cpp` NimBLE 1.x/2.x 双版本适配（`PeerRef`/`PEER_ARG`，2.x 广播 `setName()+enableScanResponse(true)`，回调只保留 `NimBLEConnInfo&` 版本）；`src/main.cpp` LIVE 进 `esp_pm_configure(240/40MHz, light_sleep)`、GPIO17/6/42 `gpio_sleep_sel_dis()` 睡眠保留、`/status.json` 增 `pm_light_sleep`；PM 版固件号 `0.11.0-bw`，stock env 保持 `0.10.3-bw`。原 env 回归构建 SUCCESS（337s）。
- **构建环境坑（后续窗口必读，细节见 `project-workflow/sleep-battery/task-7.md` §7）**：仓库路径 `...\codex status` 含空格，pioarduino 的 custom_sdkconfig 流程（`espidf.py:2629`）直接拒绝 → 需用 junction `C:\Users\user\AppData\Local\Temp\opencode\codex-status` → 仓库根，并让 pio 进程真实 cwd 落在 junction（分离启动；Shell 的 workdir/Set-Location 会被解析回真实路径）。盘符根/subst 根不行（`basename("P:\\")` 为空 → 生成非法 `project()`）。`pio run -v` 在 GBK 控制台会 UnicodeEncodeError 挂住构建，pm 构建勿用。
- **构建进度**：已过 whitespace 检查与 CMake 配置（LDF 跑通），首次 Arduino core 重编未跑完（输出线程挂住后已终止）；日志 `artifacts/pm-build.log`。
- **现场**：设备仍 **0.10.3-bw（ota_1）LIVE**，桥 `bridge-app` PID 43768 正常（勿停）；电池 ~30%/3.59V 且 LIVE 掉电明显，PM 固件应尽快落地；回退 ROM `artifacts/codex-status-0.10.3-bw.bin`（SHA256 `C0AABDC1…B61D`）未动。
- **下一步**：非 verbose 重跑 pm 构建 → 烧 `0.11.0-bw` → 冒烟（`pm_light_sleep`、BLE 重配/被扫、HTTP push）→ T10 电池斜率（标注估算）+ T9/T11 + 深睡/按键/OTA 回归。**未提交（等用户同意）**。

## 最新状态：2026-09-17 — 固件 0.10.3-bw 上线（M3 Plan B LIVE+推送 + ext1/PWR/token 加固）

- **0.10.3-bw 已 OTA**（当前运行 ota_1，next ota_0）：M1–M3 代码已提交 `07af884`，本版在其上做加固并修复 PWR/ext1。
- **LIVE 形态（0.10.1 起）**：DEEP 窗口 Wi-Fi 取数成功 → `enterLive()`（`WiFi.setSleep(true)` modem sleep、BLE 停播）；看门狗（Wi-Fi 断 >3min 或 10min 无同步 → 渲染 IDLE + 深睡 300s）；15min 兜底轮询；桥按指纹/5min 心跳 `POST /usage`（Bearer endpoint token，实测 200 accepted）。
- **唤醒**：`armWakeSources()` 显式配置 RTC 上下拉并把 PWR(GPIO18) 加入 ext1 掩码；实测 BOOT/PWR 深睡唤醒均 `reset=deep-sleep wake=ext1`，定时唤醒 `wake=timer`。
- **PWR 软关机**：运行态长按 3s → GPIO17 拉低断电（电池供电实测离线、RTC 域掉电；再长按冷启动 `reset=power-on`）；开机 5s 宽限避免上电长按误关机；运行态短按与关机态短按均无动作（硬件锁存需长按）。USB 供电时长按 3s 后重启兜底（锁存拉低不断电）。
- **token 持久化**：开机自动签发 32 hex 存 NVS `auth/token`，跨深睡/重启/OTA 有效；BLE `{"cmd":"token"}` 取用、`rotate:true` 轮换；仅经绑定 BLE 链路，不落 HTTP/串口；`/doUpdate` 无 token 仍 401；factory reset 清除。
- `/status.json` 增 `wake`（power-on/ext1/timer）与 `pwr`（GPIO18 电平）；串口 CLI 增 `sleep [sec]`、`pair`（LIVE 下开 BLE 窗口）。
- **桥**：HTTP push 健康（`last_push_ok_at` <360s，连续 2 次失败才判失效）时完全不扫 BLE（启动先等 8s 健康窗口）；推送指纹剔除 `server_time` 与 0% 窗口的滚动 `resetsAt`（稳态=真实变化+5min 心跳，局刷由 ~1/min 降到 ~1/5min）；BLE 失败不再污染 `last_error`/面板（HTTP 正常时仅 debug）；HTTP 成功回填 `last_sync`；tray OK 阈值覆盖 5min 心跳。已部署（bridge-app 后台运行，PID 见 `artifacts/bridge-app-run.pid`）。
- **验收证据**：`pio run`；cargo 17/17（隔离 target）；node 7 fixtures；`git diff --check` 0；OTA `UPDATE OK`（0.10.3，token/模板/bond 保留）；`/sleep` 60s/180s/900s 定时与 ext1 唤醒；401 回归。
- **待办**：T10（LIVE 电流，可用 `battery_mv` 斜率粗测）、T1（DEEP 过夜）、T7/T8 设备侧复测；T9 已验证（直推 0.95s 完成渲染；桥 3s 检查，信封刷新→推送实测 2.0s）。**M3 正身（pioarduino 自编 core + PM light sleep）准备文档：`project-workflow/sleep-battery/task-7.md`（新窗口从这里执行）。**
- ROM：`artifacts/codex-status-0.10.3-bw.bin`，SHA256 `C0AABDC117D88D8FB073306C27F0BAD5DB7967600A1ACFAF019B7EFE5780B61D`；任务文档 `project-workflow/sleep-battery/task-6.md`。

## 最新状态：2026-09-17 — 固件 0.10.0-bw 上线（M2 DEEP 窗口 + IDLE 模板协议）

- **0.10.0-bw 已 OTA**（槽位 ota_0）：boot 即 15s 取数窗口（Wi-Fi scan+last-used 选网 9s、active 新鲜时只试 active、否则同 BSSID MRU 优先、单端点 2s），窗口结束必睡（与成败解耦）；`RTC_DATA_ATTR` 保存 activeMac/activeAt/失败计数/加速窗/idle_reason/BSSID/usage 指纹；间隔 60s（前 3 窗）→300s，连续 3 败退避 900s，成功复位；连续 2 败或从未同步→用缓存 usage 渲染 IDLE 模板（无缓存回内置 `IDLE - NO LINK` 屏）；usage 缓存入 NVS `ucache`（指纹去重）；envelope 解析 `idle_template`（pin 不被 LRU 淘汰）/`active_hold_seconds`；AP 仅在无槽位或 BOOT 长按 5s 进入、5 分钟空闲入睡；`/status.json` 增 mode/idle_reason/active_mac/fail_count/window_synced。
- **协议（四方同步）**：元素 `mode: idle|live|any`（LIVE 跳过 idle、IDLE 跳过 live）；idle binds `device.state`/`device.offline_mins`/`device.idle_reason`；quad `min_fw 0.10`、`version 4`、§4.4 变体（144/158/172/186）。固件引擎、`bridge-core/template.rs`、`tools/generate-quad-preview.mjs` 全实现，Python 测试桥 envelope 增加 `--idle-template/--active-hold`。
- **桥**：BLE 节奏 20s/5s，12 轮未发现→60s；成功推送后暂停扫描至 usage 指纹变化或 5 分钟心跳；profile 推送附带 idle 模板并激活首个启用项；面板新增「待机模板（IDLE）」选择（持久化 `data/bridge-app.json`）；MCP `template_render` 支持 idle/offline_mins/idle_reason、`profile_push` 附带 idle。
- **验收已过**：`pio run`；cargo 17/17；node 7 fixtures；`bridge-render --diff` 对 live/idle 预览均 0 像素差。设备 `/status.json` 已见新字段并进入窗口间深睡。
- **超出 docs/history/sleep-plan-v4.md 的运维保护（已记录偏离）**：BLE 配对窗口、已连接 peer、OTA 上传会按住窗口（上限 10 分钟）；BLE 签发 OTA token 后按住 10 分钟，使 DEEP 下既有 token 门控 OTA 仍可用。
- **待办（T1–T8 设备验收）**：启动 bridge-app → 推送 quad v4 → T2/T3/T7/T8；运行数据 `<exe>/data/templates/quad.json` 是旧副本（种子只补缺失不覆盖），验收前需经面板保存或拷贝更新。M2 之后进入 M3（自编 core + PM + POST /usage）。
- **M2 实测补充（2026-09-17 下午）**：bridge-app 常开后设备窗口成功同步——`[wifi] usage from 192.168.1.100:8765 accepted=1`、`[tpl] saved quad v4 hash=d3253df5`、`rendered quad live (WIFI)`，设备端 quad hash 与 repo/Rust/Node 一致（T8 设备侧通过）；失败计数复位，窗口恢复 300s。注意：用户按的按钮造成 `reset=power-on / wake=0`（PWR 断电再上电），**不是** ext1 BOOT 唤醒；BOOT 唤醒仍待专门验证（深睡唤醒应为 `reset=deep-sleep`、`wake=2`）。
- **M3 已开工**：`project-workflow/sleep-battery/task-5.md`；官方 core `CONFIG_PM_ENABLE` 未编译且无 `libpm.a`，确认必须自编 core（pioarduino `custom_sdkconfig` 或 Arduino-as-IDF-component）；LIVE=Wi-Fi 保活+PM light sleep 才能被 `POST /usage` 主动推送唤醒。
- ROM：`artifacts/codex-status-0.10.0-bw.bin`，SHA256 `6CA11D955363ED71B9012FF7F8397163FB47814C33FA8A840AA7008D5E703FDF`；计划文档 `project-workflow/sleep-battery/`（task-2/3/4）。

## 最新状态：2026-09-17 — 固件 0.9.0-bw 上线（面板休眠 + 窗口 BLE + mode CLI，M1）

- **0.9.0-bw 已 OTA**（槽位 ota_1）：每次刷新后 `EPD_SSD1681_Sleep()`（mode 1 保留 RAM），绘制前 `EPD_SSD1681_WakePartial()` 重新 init 并回填 0x26 基线，局刷不回退；BLE 仅窗口内广播、入睡前 `bleAdvertiseStop()`；串口 CLI `mode auto|deep|live`（deep/live 在 M3 前等价 deep，auto 沿用 `cfg/batt`）；DEEP 下跳过分钟/电量重绘；BLE info 增 `ip`/`http_port` 且 IP 变化时刷新；`MDNS.begin()` 经 docs/history/sleep-plan-v4.md 修订移除（mDNS 由 `ArduinoOTA.begin()` 内置，`codex-status-AABBCC.local` 实测可解析）。
- ROM：`artifacts/codex-status-0.9.0-bw.bin`，SHA256 `04D9662E31F1A976646B02CA19F812B18ADC99889F9172013F72512ADB432174`；`src/*.cpp` 统一 LF 后重建逐字节一致。
- 桥：托盘"开机自启"落地（HKCU `Run\CodexStatusBridge`，勾选态与实际一致）；首轮 BLE 立即执行保持。`cargo check/build -p bridge-app` 无警告。
- 修复基线遗留失败：`0545945` 删掉 `quad.json` 的 `version` 导致 `hashes_match_python_test_bridge` 失败，已恢复 `"version": 3` 并同步断言（模板资产内容自 0545945 起已变，版本号补齐）。
- 验收证据：`cargo test --workspace` 15/15；node 预览 5 fixtures；OTA `UPDATE OK`，templates/bond/NVS 保留；BLE info 读取 `{"ip":"192.168.1.50","http_port":80,...}`；Python 测试桥取数 WIFI 渲染 quad，同分钟无变化 15s 内 `epd_writes` 不增（DEEP 窗口不增长=T3 留 M2 验证）。
- 计划与任务文档：`project-workflow/sleep-battery/`（plan/status/task-1）；M1 代码未提交；下一步 M2（固件 0.10.0 + 桥 + 模板协议 `mode`/idle binds/envelope）。

## 最新状态：2026-09-17 — 固件 0.8.0-bw 上线（/status.json + /log + 电量）

- **0.8.0-bw 已 OTA**（槽位 ota_1，实测）：新增 `GET /status.json`（fw/槽位/重置原因/uptime/SSID/IP/RSSI/电量%/mV/heap/EPD/模板 hash+active）与 `GET /log`（4KB RAM 环形缓冲 + `DevLog` tee，启动/wifi/ble/渲染事件本机可 Wi-Fi 直读）；状态页补 Battery 行；仅加 HTTP 路由与日志，不改 GATT（无需重新配对）。
- 桥侧：`bridge-core::device` 优先解析 `/status.json`（旧固件回退 HTML），面板设备页与 MCP `bridge_status` 均显示电量；ROM 归档 `artifacts/codex-status-0.8.0-bw.bin`（SHA256 `2BD21091BAD2ACDDFC389279675C1F6AE09D21BC026F4535687443DFF60EFEF7`）。
- 面板：× 按钮居中修复；设备页数据接入。
- 待办：一键 OTA（面板/MCP）；M3 版本检测。

## 最新状态：2026-09-16 — MCP 工具链 + 模板预览 + 推送配置 + 便携数据布局

- **MCP 服务**：`crates/mcp`（stdio/HTTP 同源工具：status/get/validate/render/save/profile_save/profile_push）；托盘内建 `http://127.0.0.1:8766/mcp`（Host 校验，不改 GATT）；`template_save` 返回同源渲染预览图；保存只落盘，**推送是用户显式动作**（profile 为单位，≤3 启用，总数不限，第一个启用的默认显示，顺序启用独立、拖拽排序）。
- **模板预览**：`crates/render` 把固件同一份 C++ 引擎（template_engine + GUI_Paint + 字库）编进宿主，与设备逐像素一致；渲染走**专用单线程队列**（消除并行渲染竞态）；预览缺省值 battery=75。
- **面板**：配置页（自定义下拉 + ⋯ 菜单 + 弹窗：新建/重命名/删除/添加模板[只列未加入、多选]、拖拽排序、行内开关/×/⋯、点击名称弹预览窗）；MCP tab（端口设置 + 按 agent 生成的接入提示）；tab 顺序 配置/设备/操作/MCP。
- **便携数据布局**（绿色软件）：运行数据 `<exe>/data/`（templates、profiles.json、logs、backups、bridge-app.json）；种子 `<exe>/seed/`（开发回退仓库 `tools/test-bridge/`，含 `profiles.seed.json`）；首次启动只补缺失、不覆盖；程序不写仓库。
- **quad 模板**：共同顶点居中 + WEEK/5H 用 f16（清晰），已提交 `0545945`；设备仍是旧画面（未推送）。
- 已合并为单 exe：MCP 只由托盘内建 HTTP 端点提供（`crates/mcp` 为共享库，stdio 二进制与启动器已移除）；`bridge-core.exe`/`bridge-ble.exe` 保留给 CI 与一次性推送，`bridge-render.exe` 供离线对拍。
- 待办：M2 内容（0.8.0 `/status.json`+`/log`）；一键 OTA。

## 最新状态：2026-09-16 — bridge-app 托盘框架上线（后台化 + 面板骨架 + 动态图标）

**新增 `bridge/crates/app` 生产运行形态：单实例、关窗进托盘、文件日志（artifacts/logs）、JSON 配置（artifacts/bridge-app.json，env 可覆盖）、中文菜单（升级固件/开机自启为预留项）；面板骨架（余量圆环/设备卡/模板卡/操作区）已可用；托盘图标运行时绘制（状态色 + 周余量数字徽标）。旧的 bridge-core.exe 直跑退役，由 bridge-app 统一接管 HTTP/轮询/BLE。**

- 运行验证：二次启动仅唤出面板（单实例生效）；首轮 BLE 立即执行且全部 ACK；`/usage` 200；面板截图证据 `artifacts/panel-smoke.png`；日志 `artifacts/logs/bridge-app.log.*`。
- BLE 修复：模板按设备 hash 跳过、仅必要时激活（消除每轮 full→mini→quad 轮播）；连接失败改为 15s 快速重试。
- 未完成：面板真实数据（ROM/电量/RSSI/激活模板，待固件 0.8.0 `/status.json`）；一键 OTA（M3）；开机自启勾选；release 构建迁移。

## 最新状态：2026-09-16 深夜 — 0.7.0-bw：局刷 + BLE 令牌授权已上线

**设备已是 0.7.0-bw；quad 以模板 JSON 下发并激活。今日固件链：0.5.0（quad 模板化）→ 0.6.0/0.6.1（局刷+去抖）→ 0.6.2（OTA 槽/重启原因诊断+延迟重启）→ 0.7.0（BLE 协商 token 授权 Wi-Fi 操作）。提交：`62bd15e`、`6c39ead`、`8ffc304`、`812170a`、`2fb5d95`、`2bfa034`。**

### 已完成验收链
- task2 最终自测全部重跑通过：PIO exit0（RAM 58252 / Flash 1297017，firmware.bin 1297440B）；Rust 离线隔离 target-dir 15 项（BLE 2 + envelope 7 + template 6）；node 预览生成 + 5 场景测试 exit0；git diff --check 0。firmware.bin SHA256 与候选 ROM 逐字节一致（构建可复现）。
- 提交 `62bd15e Render larger quad layout from templates`（引擎/校验器/quad.json/预览测试/工作流文档）。
- Wi-Fi OTA 0.5.0-bw 成功（HTTP 上传响应被重启中断为既有现象，版本页确认）；NVS/bond/模板存储保留，设备随即通过 Wi-Fi 拉取 quad（active 798d84cf）。
- BLE 推送 quad 正 ACK 全通过（endpoint/wifi-usage/usage/begin/end/activate；1964B），peerBonded/peerEncrypted=true（电池供电）。
- “仅模板变化、ROM 不变”演示：临时 quad v2（WEEK→WK）BLE 激活（785fd103），ROM 保持 0.5.0-bw；随后还原正式 quad 并再次 BLE 激活（798d84cf）。
- 显示修订（用户要求）：label 改为 Codex 账号用户名（app-server `account/read` 的 email @ 前缀，现显示 `alice`），去掉 `LABEL ` 前缀；取不到用户名时 label=null 整行隐藏（不再回退主机名 `?????`）。`availableCount<=0` 时信封不下发 resetCredits，quad 的 RC 行 when exists 隐藏（0 不再显示 RC 0）。桥重启后设备自动拉取 quad v2（hash 4827fea3）并重绘，ROM 不变。
- 局刷（用户要求）：数据屏默认 ~300ms 无闪烁局刷；变化超过 12.5% 像素或每 30 次局刷自动全刷清残影；分钟/电量变化触发，电池 ±2% 去抖。状态页可看 `partial ready/streak`。
- OTA 可靠性：0.6.2 起上传成功后延迟 1.5s 重启，HTTP 先返回 `UPDATE OK`（不再 curl 56）；状态页新增 Running 槽位/下一 OTA 槽/Reset reason/uptime；实测 `software` 重启、ota_0/ota_1 轮换、版本跨重启保持。
- Wi-Fi 操作授权（用户要求）：`/update`、`/doUpdate` 与 ArduinoOTA 都必须使用 BLE 绑定链路上协商的 token（32 hex，TTL 3600s，仅 RAM，不在串口打印）；无 token → 401；token 过期即作废并把 ArduinoOTA 密码随机化。协商工具 `tools/device-auth/request_token.py`（写入新特征 `e7f1a007-…`，token 经 status notify 返回，需已绑定+加密）。端到端实测：BLE 协商 → Bearer 授权 OTA 返回 `UPDATE OK`；错误 token 401。
- 注意：新增 GATT 特征后 Windows 缓存旧属性表（Rust/bleak 均报 GATT 错误），一次“解除配对 + 重新配对”后恢复；将来再改 GATT 表需同样处理（后续可评估 Service Changed 指示）。
- 证据：artifacts/ble-0.5.0-quad*.log、device-0.5.0-quad*.html、quad-preview*.png（长名预览含 RC 1、无 LABEL 前缀）、rust-live.log。

### 运行现场
- 实时 Rust 桥 PID 43156：隐藏启动（WScript 运行 `artifacts/start-rust-live.cmd`，无窗口；日志 rust-live.log/.err，PID 在 rust-live.pid），模板库含 full/mini/quad，`--interval 60`；BLE 推送用 `bridge-ble --once`（隐藏+日志）。
- 设备 192.168.1.50 / SSID home-wifi / BLE CodexStatus-AABBCC（本次已重新手动配对）；Templates 3、active quad hash 4827fea3、Running ota_0（next ota_1）、Reset reason software、EPD partial ready。
- ROM 证据：`artifacts/codex-status-0.7.0-bw.bin`（SHA256 `F8CC10E0CC679537B2A7A3ED233266B36D334A718846B7FA7C9A088A22F4CC5D`）；BLE 日志 `artifacts/ble-0.7.0-*.log`。

### 未完成/后续
- BOOT 短按本地循环模板未做人工验证（无相机/人工证据）；USB 仍拔除，无法串口核对。
- 再改 GATT 特征表需 Windows 解除配对重配；可评估 NimBLE Service Changed 指示以自动失效缓存。
- 局刷残影/对比度的长期观感待实机确认（30 次局刷自动全刷已实现）。
- 工作流角色（Luna/Sol/编码 worker）按用户指示由调度代理直接执行，不再启动子代理。

候选ROM SHA256：`72A07DB8A70922E2CDC6166AF4A6DBAD2D75D04D055F01D126AEDBACA0327AEE`。

## 历史交接（已恢复）：2026-09-16 16:16 — 用户暂停

> 历史快照：其中“源码未提交、设备 0.4.2”等状态已被 `62bd15e` 与 0.5.0 OTA 取代。

**当前设备仍为已验收的 0.4.2-bw。四象限新代码已保存，但尚未独立验证、审查、提交或部署。不要误认为设备已更新。**

### 用户最终显示规则（覆盖此前所有无穷/缺失显示讨论）
- 周额度一直存在：左上显示真实周剩余额度，保留周重置时间。
- Codex 5h 存在：右下显示真实剩余额度及 5h 重置时间；剩余0也属于存在。
- Codex 5h 不存在：右下显示 **100**，隐藏 5h 重置时间。不要显示∞，不要借用 Spark 的5h。
- 两个对角黑块104×90，坐标[4,4]与[92,106]，竖向空隙12；右上套餐/标签/周重置/RC，左下5h重置（条件显示）/电量/同步时间。

### 已保存的实现
- 固件源码版本0.5.0-bw；模板quad version1、min_fw0.5。
- 通用 text scale1..3、region、align、限定日期格式；device.battery；通用when字段存在条件。固件和Rust均增加校验。
- quad.json使用固定codex桶；缺失5h由静态text100替代，时间行隐藏。内置fallback也已改为100，旧infinity helper已删除。
- 最新usage始终保留；画面按像素比较去重；分钟/电量变化触发重绘。额外5000字节已显示帧缓存；状态页提供显示更新计数。
- 原有configTzTime(CST-8,pool.ntp.org)保留。主代理曾误判没有校时，已撤销新增校时方案；不要再次照旧讨论修改时钟。
- 预览直接读模板JSON和固件字体表。大块、f12信息字、100回退、长名称、时间隐藏已视觉检查。

### 文件和验证状态
- 修改：src/main.cpp、src/template_engine.cpp/.h；bridge/crates/core/src/template.rs、tests/template.rs；tools/generate-quad-preview.mjs。
- 新文件：tools/test-bridge/templates/quad.json、tools/test-quad-preview.mjs、project-workflow/live-template-delivery/task-2.md。full/mini文件未改。
- 工作流：project-workflow/live-template-delivery/plan.md、status.md、task-2.md；最后addendum为最终规则。task2没有review文件，切勿跳过独立验证和Sol审查。
- 编码worker报告：PIO提权构建exit0（较早一轮RAM58252、Flash1297749）；Node生成和5场景测试exit0；diffcheck0；Rust隔离target-dir离线测试exit0，共12项。
- 普通cargo test --workspace --offline因运行中bridge-core PID36828锁定target/debug/bridge-core.exe失败；不是Rust逻辑测试失败。可用隔离target-dir复测，或由主代理核实进程后安排停桥重建；worker不得自行停服务。
- 此后又修正了内置fallback为100，并于16:15:36生成新的firmware.bin，长度1297440。**最终完整自测汇总尚未收到**，恢复后先补齐最终版本自测，再独立验证。不要把早一轮结果直接视作最终通过。
- 预览：artifacts/quad-preview-missing-5h.png、quad-preview-100.png、quad-preview-longnames.png。早期quad-preview-unlimited.png为旧图，不代表当前方案。
- 候选ROM备份：artifacts/codex-status-0.5.0-bw-unverified.bin（未验收、未安装）。

### 恢复入口
1. 读取本节、task-2.md与真实git diff。HEAD仍为0a30c58，所有task2源代码未提交；保留现有修改，不要重做。
2. 补最终编码自测：PIO、cargo workspace offline（必要时隔离target-dir）、node tools/generate-quad-preview.mjs、node tools/test-quad-preview.mjs、git diff --check。
3. 独立Luna验证，Sol task-2-review-1；通过后主代理显式暂存并提交 Render larger quad layout from templates。
4. 再Wi-Fi OTA0.5.0-bw，构建并恢复Rust核心/ BLE程序，加载quad模板，BLE推送quad并核对设备ACK、active/hash和显示计数。演示仅模板变化、ROM版本保持不变，最后留正式quad。
5. 更新本文件。当前停在用户主动暂停，不是硬件阻塞；不要继续自动执行部署。

### 运行现场
- 最近设备IP192.168.1.50、SSID home-wifi，BLE CodexStatus-AABBCC /70:04:1D:AA:BB:CC；用户已拔USB，0.4.2电池Wi-Fi/BLE先前验收通过。已有配对保留，无需重新按BOOT。
- 实时桥仍为原PID36828（操作前核实身份），bridge/target/debug/bridge-core.exe；参数为绝对templates路径、CodexCLI C:/Users/user/AppData/Local/OpenAI/Codex/bin/12219cbfbcbddde7/codex.exe、--interval60。没有停桥或新OTA。
- 模板库启动时加载，未来添加quad后必须重启新Rust桥；旧设备会拒绝min_fw0.5模板，所以先完成能力固件升级。
- 编码代理/root/quad_implementation已中断，用户要求立即收尾。角色运行时model/effort元数据不可见的例外先前已获用户允许；继续按声明原生角色，不必重复询问。

候选ROM SHA256：`72A07DB8A70922E2CDC6166AF4A6DBAD2D75D04D055F01D126AEDBACA0327AEE`。

## 最新现场状态：2026-09-16 ROM 0.4.2-bw

- 提交 `e89b0b1`：修复本地模板 hash/304、下载内容校验、BLE info/status 误发4字节指针、接收缓冲清理；加入手动配对限制及不会被数据刷新覆盖的配对提示；Rust/Python JSON 分片和已配对检查。
- 两层验证各通过：固件编译；Rust11项；Python4项；diff检查。只读审查通过。过程见 `project-workflow/live-template-delivery/`。
- 已通过Wi-Fi OTA安装0.4.1-bw，串口与设备状态页双重确认。设备COM4、192.168.1.50、SSID home-wifi；BLE名称CodexStatus-AABBCC，地址70:04:1D:AA:BB:CC。
- 用户曾在旧ROM上长按BOOT至10秒误恢复出厂。经用户明确授权，已从PC保存配置通过USB恢复Wi-Fi，密码未输出或记录。端点和模板已恢复，用户已重新手动配对。
- **用户已手动配对成功**：Rust读取peerBonded=true、peerEncrypted=true；endpoint/usage设备ACK成功，Wi-Fi已保存full/mini模板。0.4.1实测发现BLE模板END栈溢出，已在0.4.2修复并OTA安装。连续三轮full/mini：6次END、6次activate成功ACK、3次加密重连、无重启；最低栈余3360字节，空闲堆183560字节。用户确认拔USB后，本机无串口；设备Wi-Fi仍在线，BLE自动加密重连，usage及full/mini的END/activate均成功ACK。电池供电无线验收通过。
- Rust核心已在后台运行真实数据桥（最初PID36828，操作前核实），之前已确认真实数据到屏幕；当前endpoint已恢复为1，Wi-Fi显示真实数据，模板2个。RustBLE可执行文件已重建。
- ROM文件：`artifacts/codex-status-0.4.1-bw.bin`；SHA256 `43622285879FB176688F0F27B13E3C858F0EC2FF6A2DAAF001BCCF7BE9DD7B3C`。OTA上传HTTP客户端超时，但设备已重启到新版本，故没有重复刷写。
- 尚未完成：四象限90px高块及模板化、完整发送端ACK关联、Rust运行工具收尾。当前0.4.2支持现有模板能力；四象限新能力仍需后续固件升级。Wi-Fi支持固件OTA；BLE仅用于配置/数据/模板，没有BLE固件OTA。
- 热修复提交 `f5e8e2e`：NimBLE栈8192、无符号CRC读取、栈余量诊断；两层编译及审查通过。当前ROM `artifacts/codex-status-0.4.2-bw.bin`，SHA256 `E7390ACA345D8628778DA1D7F235A1B60B9043AEDF8D26105FC29FDE19913554`。实机证据 `artifacts/ble-0.4.2-repeat.log`、`ble-0.4.2-serial.log`；包含不同hash模板实际保存及Wi-Fi自动恢复原模板。
- 下文为历史记录，设备状态和旧任务顺序以本节为准。

- 更新：2026-09-16（新设备已适配并刷入 0.4.0-bw：SSD1681 B/W + 全/局刷 + 四象限内置界面 + USB 串口配网；当前问题见 §9）
- 需求文档：`docs/history/request.md`（架构已定稿：Wi-Fi 主通道 + BLE 备选）
- 用途：新会话从这里接手

## 1. 设备与备份

| 项 | 值 |
|---|---|
| 旧设备 | Waveshare ESP32-S3-ePaper-1.54G（200×200 四色）；MAC `70:04:1D:AA:BB:01`；USB-Serial/JTAG COM3 |
| 出厂全片备份 | `D:\codex_status_backup\factory-backup\factory-full-8mb.bin`（SHA256 `7E9CF08B...C29F25`）+ `partitions-0x8000.bin` |
| 旧设备当前固件 | **已刷回出厂原版（全片写入 + SHA256 校验通过），准备退还** |
| 新设备 | Waveshare ESP32-S3-ePaper-1.54（**B/W 200×200，SSD1681，支持局部刷新 ~300ms**）；引脚与四色版一致（EPD: CS11/SCK12/MOSI13/DC10/RST9/BUSY8/PWR6，VBAT 锁存 GPIO17，音频电源 GPIO42） |
| 新设备待办 | 到手先做全片备份；固件需把 `EPD_1in54g` 换成 SSD1681 驱动（缓冲 1bpp=5000B，`Paint_SetScale(2)`），应用层不动 |

## 2. 固件工程（0.3.0-tpl，已编译未上板）

- 位置：`D:\codex_status`；PlatformIO + espressif32 7.1.2 + Arduino core 2.0.17
- 依赖：NimBLE-Arduino 1.4.3、ArduinoJson 7；Flash 1.27MB（40.4%）
- 分区（已更新 `partitions.csv`）：双 OTA（各 3MB）+ coredump 64KB `0x610000` + storage(LittleFS) 1.875MB `0x620000`
- LittleFS 挂载：`LittleFS.begin(true, "/littlefs", 10, "storage")`

### 已实现功能
1. Wi-Fi 多 SSID（NVS，3 槽）+ AP 配网门户（`CodexStatus-XXXX` / `codex1234` / `http://192.168.4.1`）
2. Wi-Fi OTA 双通道：网页 `/update` + ArduinoOTA（`codexota`）
3. BLE GATT：`info`(001 读) / `endpoint`(002 写) / `usage`(003 写) / `status`(004 读+notify) / `template_ctrl`(005 写) / `template_data`(006 写)
4. endpoint+token 存储（NVS `brg`，最多 8，MRU）
5. Wi-Fi 主通道：MRU → `GET /usage` → hash 不一致则 `GET /template?id=&hash=` → 渲染（304 支持）
6. 屏幕：模板渲染 + 内置数据屏兜底；显示字段签名去重（不变化不刷屏）
7. 按键：BOOT 短按切模板 / 长按 2s 配对窗口 / 超长按 10s 恢复出厂
8. 低功耗（默认关闭）：NVS `cfg/batt=true` 时同步成功后 deep sleep `next_sync_seconds`（默认 300s），BOOT 唤醒
9. ASCII 净化：所有绘制文本过滤非 ASCII（防字体表越界；中文 SSID/主机名安全）

### 0.3.0 模板系统
- 协议：`begin`(id/version/hash/len/crc) → `template_data`（2B 小端 offset 前缀，顺序写入，chunk 180B）→ `end`（CRC32+结构校验+A/B 落盘+激活）；`activate/list/delete`
- 存储：LittleFS `/tpl/`，index.json + 每模板文件；最多 4 个 LRU（active 不淘汰）；`.tmp`→rename 原子写
- 引擎：text/bar/rect/line/icon；字体 f8/f12/f16/f20/f24；颜色 black/white/yellow/red/none
- bind：`account.plan`、`bridge.label/hostId`、`server_time`、`resetCredits.availableCount/nextExpiresAt`、`device.channel/ip/sync_hhmm`、`buckets[<id>].{weekly|5h|primary|secondary|windows[n]}.{usedPercent|remaining|resetsAt|windowMins}`
- 未识别的 type/font/bind → 拒绝整份模板（dry-run 校验，绝不半渲染）

### 显示问题结论（0.2.x 已验证）
- 可用组合 = 电源循环（GPIO6 500/200ms）+ **GPIO17 HIGH（VBAT 锁存）+ GPIO42 LOW** + 硬件 SPI 20MHz mode0 + 复位低电平 20ms
- Waveshare GUI_Paint 颜色参数是反的：`Paint_DrawString_EN(x,y,s,font,bg,fg)`（传 WHITE,color 得黑字白底）
- 四色屏不支持局刷，全刷 15–20s 闪烁是硬件特性；新设备 SSD1681 局刷 ~300ms 可解决

## 3. 正式桥接（Rust / Tauri，Windows 原生）

位置：`bridge/`（cargo workspace，Rust stable 1.98.1 x86_64-pc-windows-msvc）

| crate | 内容 | 状态 |
|---|---|---|
| `crates/core` | codex CLI 定位（校验 `codex-cli`）→ `codex -s read-only -a never app-server` JSON-RPC 客户端（initialize/account/rateLimits/read，自动重启退避）；usage 信封映射（按 windowDurationMins 分类、多桶、resetCredits）；模板库（canonical JSON + CRC32、设备同构校验、分片编码）；axum HTTP `/usage` `/template`（Bearer、304）；`--once` 打印信封 | ✅ 9 测试通过；真机数据实测（plan prolite, codex weekly + codex_bengalfox 5h/weekly） |
| `crates/ble` | btleplug central：扫描 `CodexStatus-*` → 写 endpoint+token → 写 usage → 推模板（begin/chunks/end/activate）→ 订阅 status 打印；LAN IP 自动检测；循环/一次模式 | ✅ Windows 编译 + 适配器/扫描实测（当前设备是出厂固件故未找到，符合预期） |
| `crates/app` | Tauri v2 托盘 + 窗口（状态 JSON 每 3s 刷新、Force BLE sync、Reload templates），进程内跑 core HTTP + app-server 轮询 + BLE 循环；图标已生成 | ✅ 启动实测：HTTP 200/prolite，窗口标题正常 |

- hash 一致性：Rust canonical 字节与 Python 测试桥完全一致（`full=c1a2faaf`、`mini=e6ba459e`），三方（设备/测试桥/Rust）同 hash
- 提示：默认模板目录参数为 `tools/test-bridge/templates`（相对项目根目录）
- WSL 的 Rust 只适合跑 `cargo test -p bridge-core`（纯逻辑）；BLE/Tauri 必须 Windows

常用命令（在 `bridge/` 下）：
```powershell
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
cargo test                          # 全工作区测试
cargo run -p bridge-core -- --once  # 拉一次真实数据打印信封
cargo run -p bridge-core            # 起 HTTP :8765（从项目根目录加入 --templates tools/test-bridge/templates）
cargo run -p bridge-ble -- --once   # 单次 BLE 推送（upstream 默认 http://127.0.0.1:8765）
cargo run -p bridge-app             # 托盘应用（进程内含 HTTP + BLE）
```
环境变量：`CODEX_STATUS_PORT/TOKEN/TEMPLATES/INTERVAL`；`CODEX_STATUS_CODEX` 指定 codex 路径

## 4. 测试桥（Python，设备上板测试仍用它）

- 位置：`tools\test-bridge\`；`start.ps1` 后台启动（日志 `bridge.log`）/ `stop.ps1`；**必须后台运行**
- 接口：HTTP `0.0.0.0:8765`，Bearer `test-token-123`；`GET /usage`（含 templates map）、`GET /template?id=&hash=`（304）
- 模板库文件：`tools/test-bridge/templates/{full,mini}.json`（Rust 桥同源使用）
- BLE：写 endpoint →（可选）推 usage →（可选）推模板；`--push-usage --push-template --template <id>`
- 已验证：BLE endpoint、备选 usage（死端口 9999 + 42% → `SRC ble-test BLE`）、模板分片协议字段与设备/CRC 一致性
- `resetsAt` 启动时固定（`RESET_AT`），避免时间漂移触发重绘；主机名/标签已做 ASCII 净化
- 当前状态：已停止

## 5. 下一次测试（设备退还前最后一次，上板清单）

1. `pio run -t upload` + `esptool.py -p COM3 erase_region 0xD000 0x2000`（新分区表一并写入；首次启动格式化 LittleFS）
2. 等 60s 确认显示（内置数据屏；若有模板则显示模板）
3. 后台 `start.ps1` → 串口应见 `[tpl] fetched full hash=c1a2faaf` + `[tpl] rendered full (WIFI)`；屏幕显示 full 模板（USED 91%、黄条、WIFI 红字）
4. 长按 BOOT 2s → `python -u bridge.py --push-usage --push-template --template mini` → 屏幕切 mini
5. BOOT 短按 → 本地切换 full/mini
6. 可选：Rust 桥联调（`bridge-core` + `bridge-ble`），验证与 Python 桥等价
7. 测完刷回出厂（见第 6 节）

## 6. 常用操作与坑

```powershell
# 刷写（USB 不更新 otadata，用过网页 OTA 后必须清）
cd "D:\codex_status"; pio run -t upload
pio pkg exec -p tool-esptoolpy -- esptool.py -p COM3 erase_region 0xD000 0x2000
# 刷回出厂
pio pkg exec -p tool-esptoolpy -- esptool.py -p COM3 -b 921600 write_flash 0x0 "D:\codex_status_backup\factory-backup\factory-full-8mb.bin"
# 读串口（不触发复位）
pwsh -File C:\Users\user\AppData\Local\Temp\opencode\read_serial.ps1 -Seconds 75
```
- 墨水屏全刷 15–20s；启动到首屏 40–60s；Python 日志需 `-u`
- 设备 IP `192.168.1.51`（SSID `home-wifi`）；PC WLAN IP `192.168.1.100`（动态）
- 摄像头拍照读屏：`python C:\Users\user\AppData\Local\Temp\opencode\capture_cam.py out.png`
- 后台进程要求：一律 `Start-Process -WindowStyle Hidden` + 日志重定向，不在前台跑；长轮询不要内联在单条命令里

## 7. 下一步任务

1. **最后一次上板测试**（见第 5 节）→ 通过后退还旧设备
2. 新设备适配：全片备份 → SSD1681 驱动（B/W + 局刷；屏幕层重写、UI 去红黄），应用层不动
3. Rust 桥接收尾：
   - 设备记录/配对管理持久化（绑定列表、解除绑定、token 失效）
   - 配置 UI（端口/token/模板目录/同步间隔/开机自启）、模板预览渲染（Rust 渲染器对齐设备 golden）
   - 打包发布（`bundle.active=true`、安装器、Windows 资源图标）
   - 与设备联调 BLE 推送（Rust ↔ 0.3.0 固件）
4. 按键补齐：PWR(GPIO18) 长按软关机（GPIO17 拉低）；修复官方 BSP 的 BOOT/PWR 回调 bug
5. 低功耗完善：USB 在线检测、BLE 窗口唤醒流程、`next_sync_seconds` 协商
6. docs/history/request.md 未决项：O6（BLE 配对安全模式）、O10（休眠窗口/BLE OTA）、O11（跨平台）、O12（TLS）

## 8. 关键路径索引

| 内容 | 路径 |
|---|---|
| 需求/架构 | `docs/history/request.md` |
| 设备设置与调试经验 | `docs/device-setup-experience.md`（备份/分区/显示根因/BLE/坑清单，新设备移植差异在文末） |
| 固件主程序 | `src/main.cpp` |
| 固件模板模块 | `src/template_xfer.cpp` / `template_store.cpp` / `template_engine.cpp` |
| 固件 BLE | `src/ble_bridge.cpp` / `.h` |
| 正式桥接 | `bridge/crates/{core,ble,app}` |
| 测试桥 + 模板库 | `tools/test-bridge/`（`templates/full.json`、`mini.json`） |
| 出厂备份 | 本地私有备份 `factory-backup/`（不入库） |
| 参考仓库 | `token-monitor`（RPC 参考）、`clawdmeter-epaper`（SSD1681 驱动参考） |
| app-server schema | `%TEMP%\opencode\codex-app-server-schema\` |
| 脚本 | `%TEMP%\opencode\capture_cam.py`、`read_serial.ps1`、`render_tpl_check.py` |

## 9. 当前问题记录（2026-09-16，新设备 ROM 0.4.0-bw 上机后）

1. **四象限界面是固件内置屏，不是模板**（`src/main.cpp` 的 `renderUsage()`）：
   - 改布局/字号/间距都必须重新编译烧录，违背"换样式不刷固件"的设计目标。
   - 目标形态（用户已明确）：四象限作为模板之一，与 full/mini 一起用 BOOT 短按切换。
2. **模板引擎缺口（把四象限改成模板所需的三项）**：
   - `text` 无字号缩放（大数字需要 2× Font24 = 48px）；
   - 无内置图元：∞（两个圆）、电量图标（`icon` 通道有 base64 位图能力，缺资产/生成工具）；
   - 无 `device.battery` 绑定（电量是设备端数据，usage 信封里没有）→ 需新增 bind 与引擎取数。
3. **测试桥 bridge.py 两个问题**（本次联调暴露）：
   - BLE `write_gatt_char(CHR_USAGE, ...)` 单次写入超 MTU → `BleakGATTProtocolError: Invalid Attribute Value Length`，桥进程异常退出；需按 MTU 分片（设备端 `usageBuf` 已有拼包逻辑）。
   - Windows GBK 控制台下打印设备 status notify 的替换字符触发 `UnicodeEncodeError`（asyncio 回调异常）；启动时应设 `PYTHONIOENCODING=utf-8` 或解码用 errors="replace" 且不直接 print 非法字符。
4. **模板 304 语义待核实**：新设备（本地 0 模板）经 Python 桥拉模板时 full/mini 均返回 304、模板未下载；需核对 `usageTemplateGet()` 请求携带的 hash 是本地还是远端，以及 Python 桥 304 条件是否与之一致（旧设备当时能成功拉取）。
5. **局刷对比度**：数据屏已改为全刷（对比度高、~1.5s 闪）；局刷路径保留但偏灰，后续做分钟级局部更新（时钟/电量）时需重新评估对比度与残影（30 次局刷后自动全刷已实现）。
6. **本次遗留产物**：`tools/generate-quad-preview.mjs` + `artifacts/quad-preview*.png`（四象限设计稿/生成器）；新设备出厂备份 `D:\codex_status_backup\factory-backup-154bw\`（SHA256 `EE024E3E…38EC`）。
