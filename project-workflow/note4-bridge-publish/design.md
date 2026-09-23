# Note4 Bridge 发布设计

状态：设计稿，2026-09-23。本文供独立实施窗口使用；尚未实现本文的新协议，也未在 Note4 实机发布。

## 1. 目标和边界

Bridge 应能为 `epd-ssd2683-400x300-1bpp` 保存、预览、编排和显式发布 Profile。Profile 中模板与字体共同构成一次完整发布目标，但传输以内容 ID 为单位：改一个模板不重传未变字体，改一个字体不重传未变模板。设备验证全部引用后一次性生效；掉电或传输失败继续使用旧发布。字体可由 Bridge 库中的 CSFN 资产提供，也可在 Bridge 设置中指定源文件，发布时冻结确定的设备可读字节。裁切是体积优化，不是必须项；容量以实际字节和安装峰值判定，不设任意的字体数量上限。

本工作只负责 Bridge 的领域模型、编译/预览、发布计划、HTTP 客户端、UI/MCP 和宿主验证。Note4 首刷、分区、屏幕波形及 A/B ROM OTA 在独立的 `project-workflow/note4-ota-bringup/` 跟踪。Bridge 不能把“OTA ROM 已刷入”当作“增量资产协议已在设备实现”。设备端协议需由两项工作对齐后才能做实机验收。

保留现有不变量：每设备一个 Profile，1–8 个有序模板全部参与按键循环；保存只落盘，发布由用户显式发起；设备 MAC 为身份，`render_target`/`firmware_target` 双端核对；owner 只由 token 保护的 `/claim` 建立，发布不隐式 claim 或延长 PowerPlan；legacy 设备继续走原路径并明确保留 ≤3 槽限制；200×200 字体和像素输出不变。2bpp/gray4 抗锯齿不在此范围。

## 2. 当前代码基线

| 功能 | 现状 | 需要改变 |
|---|---|---|
| 模板键和编译 | `platform/model.rs` 以 `template_id + render_target` 保存；`compile.rs` 与 `bridge-render` 已支持 400×300 CTP1 | 资产字体引用进入 CTP1 并升级 ABI；宿主/设备共同验证 |
| Profile | `Profile {device_mac, template_ids, ...}`，保存时按设备能力选变体 | 增加明确的目标绑定和字体选择；保留每设备 Profile，不额外建立可复用 Profile 服务 |
| 设备能力 | `app/platform.rs::capabilities_from_status` 接受 Note4 target，但从 200×200 默认值继承宽高等字段；应用目标列表没有 Note4 | 建完整 Note4 能力合同，严格校验设备上报字段，不靠 target 字符串猜局刷或硬件验收状态 |
| 发布 | `service.rs::publish` 冻结完整 Bundle；`app/platform.rs::deliver` 调用 `v2_client::install_bundle` | 保留旧整包路径；对声明增量协议能力的设备生成完整 manifest + 差量内容，并保持单一 PublishJob/ACK 语义 |
| 大小限制 | Bridge `model.rs` 上限 512 KiB，现有 `v2_client.rs` 和设备整包上限 256 KiB | 旧路径按设备能力的较小上限预检；新路径按内容对象、存储峰值和设备报告能力检查 |
| 字体 | Bridge CSFN v1 库、设备解析器/Store 已有宿主测试；`resolve_name` 可返回多个同名版本；设备模板仍用固件索引 | 明确选择 `name -> font_id`；发布冻结资产字节；设备渲染和时钟/区域推导都用所选资产 |
| v2 UI | 能显示 target、Profile、job，有保存/发布按钮；Profile 列表基本为只读 | 可编辑顺序、初始模板、目标、字体选择；显示发布差量、字节数及错误 |

现有 `prompt-font-assets-transport.md` 的目录 inventory、8 字体上限和每项独立发布思路与本设计不一致。实施时先更新该文档或标明被本设计取代，避免两个窗口分别实现冲突协议。

## 3. Profile、目标和字体选择

### 3.1 屏幕目标

每设备 Profile 增加 `render_target`，保存时从已认证设备能力初始化，之后视为该 Profile 的绑定目标。保存、预览和发布均要求它等于设备当前 `render_target`；设备刷入另一目标 ROM 后明确提示用户重新选择/迁移，不能悄悄改绑定。模板条目继续存模板 ID，具体变体由这个目标选取。这里的“屏幕族”以精确 `render_target` 表示，不使用仅凭 400×300 尺寸推断的模糊族名。`firmware_target` 是发布时的设备合同，不是 Profile 复用键。

持久化升级：旧 Profile 若无 `render_target`，只在其已绑定设备的能力可确定时一次性填入；设备未知或目标冲突则保留草稿并阻止发布，给出可操作错误。Bridge 的 Note4 能力必须记录 400×300、1bpp/BW、目标 ID、compiler ABI、设备报告的包/对象上限与 `partial`；`hardware_verified` 只能来自设备明确状态或实机验收记录，不能因为 target 名称而设真。

### 3.2 字体绑定

Profile 保存每个模板可见字体名所选的 `font_id`，或等效的 Profile 级 `name -> font_id` 映射。发布所用的映射必须唯一且完整。同名多版本时 UI/MCP 要让用户选定；不能把 `resolve_name()` 返回的所有版本都加入依赖，也不能按文件修改时间隐式选择。内置小屏字体可继续走原索引。资产字体在 CTP1 中以引用寻址，`CtOp.font` 的旧索引意义不变，ABI 升级后旧记录按既有迁移/重编译路径处理。

Bridge 字体源可以是已导入的 CSFN `.bin`，也可以是设置中指定的 TTF/OTF 路径。对于源字体，Bridge 必须有可打包运行的转换步骤，将选定字号、字重、像素格式和字形范围固定成设备支持的资产，然后校验并收入内容寻址库。不能让已排队的发布依赖外部路径之后的变化：PublishJob 冻结最终资产字节。若生产版尚未集成源字体转换，首阶段只能提供 CSFN 导入，并在 UI/MCP 明确报告 TTF/OTF 尚不可发布。设备当前只会解析 CSFN v1 的 ASCII 描述表；“不裁切”可表示保留这套格式支持的全部字形，不能宣称设备能直接渲染原始 TTF 或 CJK。

现有 Bridge/设备实现的“每 Profile 8 个字体”和“单资产 48 KiB”是待移除的临时上限，不是新产品规则。实现需检查解析器的整数宽度、设备 RAM 使用和流式落盘，再以设备实际可用空间和发布峰值约束内容；若 CSFN v1 结构本身限制某种字体，明确拒绝该格式并规划格式升级，不能静默裁切或重新引入数量上限。

## 4. 完整目标与差量传输

### 4.1 发布清单

每次显式发布冻结一个完整、规范编码的 `PublishManifest`：`job_id`、设备 MAC、`firmware_target`、`render_target`、协议/编译 ABI、Profile 顺序与初始项、每个模板的源/编译内容 ID、每个字体名的 `font_id`、各对象长度/CRC、绑定合同及清单自身长度/CRC。完整目标可从清单重建；传输包可以只包含变化对象。桥侧 job 持久化应包含所需对象的冻结字节或稳定内容寻址副本，确保排队期间编辑模板、替换字体文件、Bridge 重启都不会改变 job。

内容 ID 由规范字节计算。现有 CSFN `font_id = crc32(whole container)` 可沿用；模板源和 CTP1 分别使用已有 canonical/CRC 规则。CRC 用于完整性与去重，不是认证；设备写入仍走现有 token、会话 nonce、MAC、bridge ID、owner 和 target 校验。若相同短 CRC 对应不同字节，绝不视为同一内容；在长度/二次校验不一致时拒绝发布或升级标识格式。

### 4.2 如何确定差量

设备已认证 `/v2/status` 报告当前及可回退的**已提交清单 ID 和其引用的内容 ID**，另报告增量协议版本及剩余/安装所需存储能力。Bridge 只比较本次完整目标与这些已提交引用；不要求提供字体目录扫描式 `font_inventory`，也不把所有文件“缺失检查”作为发布前独立业务流程。设备内容存储损坏时，提交校验必须拒绝；Bridge 重新取权威状态，必要时重传受影响对象，不能把旧 manifest 的存在误当成文件可读的证明。

新设备或不支持增量能力的设备沿用完整 Bundle 发布。增量协议不可因未知能力而自动启用。一次 job 只走一条确定的发布路径；不得在半次安装中从新协议切换到旧协议。

### 4.3 设备合同（Bridge 实施的依赖）

建议沿用现有 `/v2/bundle/*` 认证和 ACK 语义，扩展为“开始清单安装 → 传对象 → 提交清单”的版本化协议；具体 URL 和 JSON 字段由固件与 Bridge 同步定稿，不复用旧 `bundle_begin` 请求体假装兼容。Bridge 客户端只负责发送冻结内容，不维护第二个 owner 或 PowerPlan 状态机。

1. BEGIN 带 job、目标、清单 ID/长度/CRC及预期 active context。设备返回已接收对象/续传位置，重复 BEGIN 幂等。
2. 对每个缺少的对象按 offset 分片传输；同 offset 同内容重试返回确认位置，冲突/越界/CRC 错误明确拒绝。大字体流式写暂存文件，不要求整份进入设备 RAM。当前 HTTP 为发布主通道；BLE 仍承担会合与小数据，不在本阶段承载大字体。
3. COMMIT 在设备验证所有模板、字体格式/target、引用、存储和当前 context 后，原子切换 active manifest 并返回持久 ACK。缺任何引用都不得半渲染。只在 ACK 被 Bridge 记录后标记任务成功；ACK 丢失通过 job ID 和已提交清单查询恢复。
4. 设备保留当前与回退清单所引用的内容。旧内容清理在新清单持久提交且两份引用集合可确定之后进行；安装失败保留原清单。空间预检包含新对象、暂存对象、当前与回退引用及文件系统开销；不先删唯一有效副本。

该模型把旧版“一个物理文件即完整 Bundle”改为“一个完整 manifest 引用已验证内容集合”。因此须同步修订 `docs/generic-display-platform-design-v2.md` 的自包含与 A/B 存储描述。A/B **应用 ROM** 是另一层概念，不能与 A/B **发布清单** 混称。

## 5. Bridge 工作流与界面

### 保存

模板保存只验证/编译并落盘；Profile 保存验证 target、顺序、绑定和字体选择，只落盘。更新字体源或选择不自动发布。保存时应检查 Bridge 是否能生成/读取选定资产；设备空间检查在发布预检阶段做。

### 发布预检与预览

显式发布前给用户可审阅的报告：设备身份/target/ABI、Profile 顺序、模板源与编译哈希、字体名/来源/最终 ID/字节、复用内容数量与字节、实际待传对象数量与字节、设备端峰值空间估算、当前/回退清单及可能的 ABI 迁移。超设备上限、未知字体、目标不符、同名未选版本、源文件不可读、未验证像素格式、空间不足均拒绝并指出对象。发布按钮确认的是这份冻结快照；保存操作不会排队。

v2 UI 必须能编辑模板顺序和初始 active、看到匹配的 Note4 变体、选择字体版本/来源，并显示发布阶段、进度、已传字节、失败原因和最终 ACK。MCP `profile_get_v2`/`profile_save_v2`/`platform_publish` 与 UI 共用同一 application service；增加只读的发布预检/字体依赖视图即可，不另做一套发布逻辑。若沿用旧 UI 的“最多启用三项”控件会误导 v2 用户，v2 页只显示 1–8 项全循环语义。

### 会合与重试

继续使用现有 `coordinator`、设备发现、显式 claim/lease 和 PowerPlan。发布处于等待时可等下次会合；大对象需要的在线时间只能由正式 PowerPlan 授予。401/409 立即停写并显示 token/占用原因；断线保留 job 和 offset，下一次按设备权威状态续传；用户取消只取消未提交 job，不把已生效清单回滚。每次状态变化均保留清晰的 waiting/sending/unknown/succeeded/failed 状态。

## 6. 实施顺序与接口分界

1. **能力与目标**：补 Note4 capability 注册和状态解析，修正宽高从 200×200 继承的错误；增加目标/ABI/包上限校验与测试。
2. **Profile/字体选择**：持久化 `render_target` 与明确的字体映射；迁移旧草稿；打通字体源导入、CSFN 校验和发布冻结。先用现成 CSFN 完成闭环，再接可打包的 TTF/OTF 转换。
3. **编译与预览**：固件/Bridge/宿主共享 ABI 升级，资产字体对拍；当前 400×300 A 模板和 200×200 fixture 回归。
4. **manifest 与 planner**：纯函数生成完整目标和与设备已提交引用的差量；容量预检、job 持久化、重试。旧整包路径保留。
5. **传输**：与设备端确定版本化字段后实现 HTTP 客户端、续传、ACK 与恢复；不要在协议未定时猜端点。
6. **UI/MCP**：目标选择、Profile 编辑、字体来源/依赖、发布预检和任务状态。
7. **验收**：先宿主模拟传输/掉电，再用已刷入相同协议的 Note4 实测。Note4 A/B ROM OTA 的进展只决定可否进入实机阶段，不替代 Bridge 协议验收。

与固件窗口的交接物：manifest 字段及 canonical 编码、对象 ID/长度/CRC、设备状态摘要、BEGIN/CHUNK/COMMIT 请求和 ACK 示例、两端上限、错误码、ABI 号与断电时序。双方定稿后放在本目录的 `protocol.md`；修改模板协议时同步固件、Rust canonical 和 Python 测试桥。

## 7. 验收矩阵

| 场景 | 必须观察到 |
|---|---|
| 只改模板 | 完整目标更新；字体传输字节为 0 |
| 只改一个字体 | 未变模板和其余字体不传；该字体内容 ID 改变；设备显示新字形 |
| 同名两个字体版本 | 未明确选择时拒绝；选定后 job 冻结一个 ID |
| 外部字体路径在排队后变化 | 已排队 job 的字节/ID 不漂移 |
| 重复发布相同内容 | 不重传对象；设备返回幂等 ACK，不重复生成显示变化 |
| 分片中断/掉电/错误 CRC | 旧清单可启动并渲染；续传或明确失败，不出现半个字体 |
| ACK 丢失、Bridge 重启 | 由设备已提交 job/清单恢复成功状态，不重复切换 |
| 空间不足或 target/ABI 不符 | 发布预检或设备明确拒绝，旧发布保留 |
| Note4 实屏 | 400×300 预览与编译渲染逐像素一致；完整 Profile 按键循环、重启恢复和数据更新正常 |
| 200×200 与 legacy | 原渲染和旧通道回归；legacy 超三项显式拒绝 |

宿主验证至少执行 `cargo test -p bridge-core -p bridge-render -p bridge-mcp`（运行中的 Bridge 占用 `bridge/target` 时使用隔离 `CARGO_TARGET_DIR`），400×300 与 200×200 的 `bridge-render --compare-compiled --regions`，以及协议模拟器的断线、CRC、重复提交和掉电用例。实机验收记录设备/Bridge 版本、ROM 和 manifest 哈希、传输字节、ACK、当前/回退 ID；预览和宿主测试不代替实屏证据。完成前运行 `git diff --check`，不要提交，按里程碑更新本目录 `status.md` 与 `PROGRESS.md`。

## 8. 实施窗口启动说明

在新窗口先读本文件、`AGENTS.md`、`PROGRESS.md` 最新节、`docs/font-asset-format.md`、`docs/generic-display-platform-design-v2.md` 及当前工作树。先建立 `project-workflow/note4-bridge-publish/plan.md` 和 `status.md`，核对同期 Note4 ROM 窗口的协议/ABI 状态，再按 §6 实施。不要把本设计当成已部署功能；不要覆盖另一窗口的固件或工作流文件。如果设备协议事实与本文冲突，记录具体差异并停止相关传输实现，通知用户协调合同；其余独立的 Bridge 工作可继续。
