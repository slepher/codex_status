# 当前设备协议统一计划（一次性切换）

日期：2026-09-28。状态：Note4、1.54 与新版 Bridge 已切换，两台在新版 Bridge 上的再次 OTA 上传和重启换槽均实机通过。待办唯一入口为 `docs/roadmap/backlog.md` C8；本文件细化该项。项目未公开发布，目标是让当前通用显示协议成为唯一业务协议，不保留产品级双协议兼容。`sync-v1`、`diag_format=1` 与镜像摘要算法名是各自数据格式，不是旧平台协议代号，可以保留。

## 统一合同

- 名称：文档、UI、MCP 和日志称“设备协议”或“通用显示平台”；不再称当前实现为 v2。当前设计文件改名为 `docs/generic-display-platform-design.md`，当前文档链接同步更新；旧文件仅保留一行指向新文件的历史链接入口，不复制协议内容。历史归档保持原样。
- HTTP：唯一业务前缀为 `/api/`。`GET /api/status`；`POST /api/data`、`/api/plan`、`/api/activate`、`/api/bundle/{begin,chunk,commit}`、`/api/sync/{begin,page,ack,complete,arm,image}`。新固件不注册 `/v2/*` 别名，新 Bridge 不尝试旧业务路由。`/status.json`、`/log`、`/history` 是只读现场/恢复视图，不承担业务写入；`POST /claim` 与 token 保护的 `/update`、`/doUpdate`、ArduinoOTA 是身份/恢复入口，继续保留原安全条件。
- 命令信封：删除平台代号字段 `protocol:2`；保留 `device_mac`、`bridge_id`、`session_nonce`、`request_id` 及操作专属字段。HTTP 路由及 BLE `op` 决定命令；MAC、owner、token、nonce、幂等键的检查顺序保持。`sync_version=1` 只标识同步批次格式。
- BLE：保留现有 GATT UUID 表以免 Windows 缓存旧属性导致配对障碍，Template Control 特征只接当前 `op` 命令。移除 `rv:2` 分流、旧模板控制/数据处理器及 `rv2Enabled` 业务回退；无效旧帧明确 NACK。INFO 用 `rendezvous:true` 表示当前能力，删除 `rendezvous_v`/`rv_max`；命令通知用 `ack:"command"` 加 `request_id` 对应，删除 `ack:"v2"`。保留 BLE 绑定、加密、token 与会合时限。
- 能力：Wi-Fi 认证 status 与 BLE status 继续按 MAC 报告具体 `sync_v1`、`diag_format`、`image_identity`；Bridge 不再以数字 2 推断设备支持。新 Bridge 对缺当前设备能力的目标只报告不可用，不尝试旧业务。这里是受控升级期间的安全拒绝，不形成长期兼容分支。
- MCP：当前平台工具使用不带版本号且不冲突的名字，例如 `platform_device_register`、`platform_template_get/save/validate`、`platform_profile_get/save`、`platform_family_profile_*`、`platform_data_sources`、`platform_data_source_save`、`platform_data_probe`、`platform_power_view`。已有 `template_get/save/validate` 是旧单设备工具，与新名字冲突；在本轮审计调用后删除或迁到新平台服务，不保留两个业务实现。UI 文案只称平台状态和正式 PowerPlan。
- 源码命名随回归收敛：Bridge客户端现为 `device_client`/`DeviceConnection`，应用缓存/发现及日志已改当前称谓。固件 `v2_command_envelope`、`v2_status_snapshot` 和其他 `V2*`/`v2*` 私有标识仍须审计；独立历史、既有版本号、`sync-v1` 文件格式不机械替换。先确保 wire 安全与功能，再处理纯标识改名。

## 实施依赖与出口

1. **冻结清单与基线。** 记录所有 `/v2/`、`protocol=2`、`rv=2`、`ack="v2"`、`rendezvous_v`、`rv_max`、MCP `_v2` 与当前文档/UI 字面；区分只读历史归档、测试名字、Bridge 本地持久 `state.json`。记录运行 Bridge/两台设备当前版本、MAC、OTA token 可取得性与可用回退 ROM；不触碰运行实例。
2. **固件和 Bridge 同一工作树改 wire。** 改固件路由、信封/GATT 分发与状态能力，同时改 `core` HTTP 客户端、`ble` 客户端、`app` 调度/OTA/MCP 服务。保留 `POST /claim` 和 OTA 安全条件。此步结束前不能重启生产 Bridge 或刷任一设备。
3. **Fake ROM 与测试同轮更新。** `device-sim` 的路由、BLE INFO/ACK/鉴权及 render 宿主适配跟随同一合同；更新 `core`/`ble`/`app`/MCP/runner 测试。至少验证无旧路径/无旧 marker、token/owner/MAC/nonce 不放宽、8 项 Profile 不裁剪、sync-v1 的持久页/恢复和 Note4 S01–S14。按 `plan.md` 顺序完成完整 Fake ROM 出口后再做设备 Tab；设备页 U01–U20 使用统一命名。
4. **文档、工具与命名收口。** 改 `docs/README.md`、当前总设计、专项 design/plan/task/device-tab、`AGENTS.md` 中当前协议称谓及 `opencode.jsonc`、MCP schema、UI 字面；历史只读文件不通篇改写。搜索检查当前源码/文档不再把平台称 v2，留下的 `v2` 都逐项解释。`state.json` 是 Bridge 本地持久格式，不能仅凭 wire 更名删除或重置；现有 `schema_version=2` 是本地快照格式，旧缓存还可能存 `transport=http_v2`。新采样写 `http`，读取旧值时只将 UI 文案显示成 HTTP；磁盘记录无损保留。其余字段如需迁移，须单独测完兼容读取再写回，保留设备登记、Profile、jobs、contexts、plans 和 token。
5. **切换前软件核对。** Note4 ROM 构建、marker/大小/SHA256/map；`cargo test` 相关 crates、显式 Bridge↔Fake ROM、MCP schema、`git diff --check`；核对新 Bridge 的认证 OTA 路径和旧字面安全拒绝。完整 Fake ROM 与设备页矩阵继续作为待办，不阻止以 OTA 可恢复为目标的阶段切换。1.54 最终构建及其增量构建问题按用户指示暂缓，不能拿早期 ROM 冒充最终。

## 两台实机分阶段切换（2026-09-28 用户接受暂时失联）

用户将 OTA 可继续升级作为首要门槛，接受切换期间的暂时业务失败，并要求尽早看到新版设备页。因此先升级 Note4、切新版 Bridge，1.54 留待最终 ROM 准备好后再升级；不为此长期保留双协议，也不让两个 Bridge 同时争设备。软件完整 S01–S14/U01–U20 与长期 RF 验收仍是未结项，不能因为允许试升级就写成通过。

**启动前准备（不触碰运行进程）：** 登记 `state.json` 当前 Note4 `7C4FADB93408`、1.54 `70041DD7A340`；每次实际操作重新核对 MAC/IP/设备自报身份。保存旧 Bridge EXE、完整 `<exe>/data/` 及两份已知可启动的旧 ROM，备份须保留 token 且不入 Git。新 Bridge 从含最终 UI 的工作树构建到隔离 target，并用复制的运行数据核对无损读取；随后构建并核验 Note4 最终 ROM 的 env、marker、target、大小、SHA256、RTC map。最低 OTA 闸门是 token 仍保护 `/update`/`/doUpdate`，镜像可上传、新 ROM 重启后能再次完成受认证 OTA；若无线 OTA 失败，则按已备 USB 恢复路径恢复后再重试，不能仅凭端点存在宣称 OTA 可用。1.54 的最终构建及其增量重编问题此时仍可延后。

1. **Note4 先 OTA。** 旧 Bridge 仍运行时，仅向 Note4 登记 MAC 入队新 ROM。上传 ACK 后，以同 MAC 的 `/status.json` 核对新版本、target、运行槽和基本无线；可取得 token 认证 `/api/status` 时再核对当前能力与 OTA 入口。旧 Bridge 在新 ROM 上无法完成旧业务确认属于已接受的错版窗口，记录为“上传 ACK + 新版本自报”，不冒称精确镜像或完整同步通过。若设备不能启动、身份不符或 OTA 恢复入口不可用，先用保留的 token OTA/USB 路径回旧 ROM，保持旧 Bridge，不进入下一步。
2. **切新版 Bridge，让设备页可见。** 按 `AGENTS.md` 的 watchdog→计划任务顺序停止旧默认实例，确认只操作当前 `target/debug` 的进程和 8765/8766；保护并快照其 data。把已在隔离目录验证的新 EXE 放入默认路径，保留原 data，按计划任务启动。核对 EXE SHA256、PID/端口、设备登记/Profile/job 数量，再对 Note4 做同 MAC 的认证 `/api/status` 和一次新 Bridge 发起的受认证 OTA，取得上传 ACK、重启后的同 MAC 自报及运行槽变化，以验证后续确实能靠 OTA 升级；同版本镜像仅用于检验升级通路，不当成新功能验证。Plan ACK 与页面实际窗口情况如实记录，可在后续版本修复。1.54 旧 ROM 在此期间可显示不可用或失联；不能对它发送旧业务或把失败标成硬件故障。此阶段已经能够看到新版设备页，但不能声称两台完成切换。
3. **1.54 稍后 OTA。** 在不中断新版 Bridge 时，先用 `tools/pio-target.ps1 -Target 154g` 单独完成最终 B/W ROM 构建、marker/大小/SHA256/map 核对（不与 Note4 PIO 并行）。准备好后开短维护窗口：停止新版 Bridge，使用保留的旧 EXE 和**独立保存的旧运行数据副本**在原端口临时运行旧 Bridge，仅为旧 1.54 走 token OTA；绝不把旧进程指向新版运行数据或并行占用端口。上传 ACK 后核对 1.54 同 MAC 的新版本、target 和 OTA 恢复入口。停止临时旧 Bridge，恢复新版 EXE 与其自己的 data，必要时按 MAC 重新认证登记以刷新能力，验证两台的 `/api/status`、owner、Profile 全 1–8 项、Plan 与下一次 BLE 会合。旧实例的 OTA job 证据单独归档，不把旧 state 文件覆盖到新版 data。
4. **异常回退。** 新 Bridge 启动/Note4业务失败时，先确认新 ROM 的受保护 OTA 或 USB 恢复可用；要回旧 Bridge 生产态，Note4 也需回旧 ROM，不能只回退 EXE。1.54 升级窗口失败时保留它的旧版/待确认状态，先恢复新版 Bridge 让 Note4 继续可用，之后重试；若已刷入新版而需整体回退，则两台都回旧 ROM 后再恢复旧 Bridge 与配套 data。任何失败不删除 data、不放宽 token/owner、不将版本观察等同精确镜像证明。

每个阶段在 `PROGRESS.md` 记录执行命令、MAC、ROM/EXE 哈希、ACK、设备自报版本、运行进程与仍未通过的功能。两次维护窗口只允许一个默认 Bridge 持有 8765/8766；旧 Bridge 数据和新版数据各自保存、各自恢复，避免互相覆盖。

**2026-09-28 执行结果：**步骤 1–3 的 OTA 闸门已过。Note4 经旧 Bridge 及一次直接认证恢复 OTA 后运行 `0.18.33-note4-b-sync1`；新版 Bridge 的 job `bde8e156` 对同 ROM 再次上传取得 ACK，认证状态同 MAC/target，运行槽 `ota_0`→`ota_1`。1.54 先完成缺失缓存的重建及 Note4↔154g 零编译复核，再由隔离旧 Bridge 用独立数据 OTA `0.18.32-bw-sync1`，job `0b59efa2` 上传 ACK、设备同 MAC/target/`ota_1` 自报；临时旧实例已停。新版 Bridge 的 1.54 再次 OTA job `fea76e01` 也一次上传 ACK、设备换槽 `ota_1`→`ota_0`。两台同版本任务留 `version_seen_unproven`，不声明逐字节镜像证明。最终新版 EXE 从含设备页的工作树构建，已由计划任务启动并保留原 data；设备页真实窗口视觉尚待验。完整证据与哈希见 `PROGRESS.md` 顶节。
