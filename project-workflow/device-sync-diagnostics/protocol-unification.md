# 当前设备协议统一计划（一次性切换）

日期：2026-09-28。状态：源码实施中，尚未完成设备切换。待办唯一入口为 `docs/roadmap/backlog.md` C8；本文件细化该项。项目未公开发布，目标是让当前通用显示协议成为唯一业务协议，不保留产品级双协议兼容。`sync-v1`、`diag_format=1` 与镜像摘要算法名是各自数据格式，不是旧平台协议代号，可以保留。

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
5. **软件验收门槛。** Note4 ROM 构建、marker/大小/SHA256/map；`cargo test` 相关 crates、显式 Bridge↔Fake ROM、MCP schema、`git diff --check`；证明新 Bridge↔新 ROM 全通，旧字面安全拒绝。1.54 最终构建及其增量构建问题按用户指示暂缓，不能拿早期 ROM 冒充最终。软件通过不等于实机已换版。

## 两台实机一次性切换

前置：上节软件出口通过，并明确取得本次实机 OTA/Bridge 重启授权；登记记录才是 MAC/IP 权威。保留旧 Bridge 可执行文件及完整 `<exe>/data/`，备好两台与目标匹配的旧 ROM；记录旧/新 ROM SHA256、marker、目标、设备自报 MAC/版本及电量。只用已有带 token 的 OTA/claim，不绕过认证。

1. 新 Bridge 先构建到隔离目录并测试，**不替换正在运行的旧 Bridge**。旧 Bridge 仍能通过旧 `/v2/*` 与两台旧设备通信；新固件保留 `/status.json` 和 OTA 恢复入口。
2. 用旧 Bridge 逐台上传新 ROM，先 Note4，再在1.54最终构建获准且通过时处理1.54。每次上传前核对目标 MAC/ROM target/hash，上传 ACK 后从该 MAC 的 `/status.json` 验证新版本、身份及基本无线；旧 Bridge 可能无法从新 `/api/status` 完成原 job 的最终业务确认，此阶段只将其记为上传 ACK+独立设备版本观察。不能把这种暂时不可管理状态记为完整成功。未完成1.54时不切换生产 Bridge，除非单独制定让旧设备保持安全离线的临时方案。
3. 两台均确认新固件后，按 AGENTS 顺序停当前 `target/debug` watchdog/计划任务主进程，确认端口和 exe 路径，保护 `<exe>/data/`，替换为已测新 Bridge 后按计划任务启动。认证 `/api/status` 逐台核对 MAC/target/owner/Profile 1–8、Plan、token、sync 配置和 OTA job；显式刷新状态不发布模板。观察每台下一次实际 BLE 会合及 HTTP 数据/Plan ACK，再完成切换记录。
4. 失败回退：如果任一 OTA 尚未确认，保留旧 Bridge；可用保留的 token OTA/状态入口为已升级设备刷回相应旧 ROM，再核对旧 `/v2/status`。如果新 Bridge 启动后出错，先恢复旧 Bridge 可执行文件与原 data，随后逐台用保留 OTA 入口回旧 ROM；不以删除 `data/` 或放宽 owner/token 作为回退手段。若无线/OTA 入口不可用，停止切换并按已授权的 USB 恢复流程处理，不擅自猜测设备状态。

旧 Bridge 与新 ROM短暂错版只是上述 OTA 过渡窗口，不实现产品运行时双协议选择；生产 Bridge 切换完成后仅使用 `/api/*`。每一步保存设备身份与成功等级，精确在机镜像哈希仍以设备实际支持的证明为准。
