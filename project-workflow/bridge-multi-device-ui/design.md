# Bridge 多设备与模板页：实现设计

定稿日期：2026-09-23。实现依据为本文件；`plan.md` 是任务索引。用户确认“族”以
`render_target` 区分。本设计不改变设备协议、token/claim/owner 或 PowerPlan 语义。

## 当前界面修正范围（后续宽范围界面计划暂缓）

用户指出此前模板页整体改写超出要求，明确要求保持原布局和菜单样式。当前只修改
`index.html` 的模板页：在原「Profile ▾」左侧加入同样菜单样式的「族 ▾」，族键
使用 `render_target`；下拉仅列两个短名「1.54 黑白」与「Note4」。切族仅过滤
Profile 菜单与对应配置列表，不筛选原模板库卡片。原三个卡片、原模板行样式、
`⋯` 菜单位置、设备 Tab 的现有 v2 编辑器与全部原操作布局保持原样。不得把旧版
配置移入折叠区，不移动 v2 编辑器，不增加新的编辑表单或工具条。

`epd-ssd1681-200x200-1bpp` 族保留现有 `profiles.json` 配置路径，原「默认」
配置和旧版 3 槽编辑、推送操作完全保留；切到其他族时 Profile 菜单读取该族的
`FamilyProfile` 草稿，显示在同一行样式中。族草稿 1–8 项全参与循环，没有 enabled
开关，相关行仅显示顺序、初始项与删除；添加、拖拽、重命名、删除、保存调用已有
`platform_family_profile_*` 服务，目标模板变体按所选 target 过滤且预览精确取
`platform_template_get(id, render_target)` 的源。没有族草稿时显示原卡片内空态并
可从原 `⋯` 菜单新建。非 200×200 族的旧 `push_profile` 不适用，原菜单位置显示
禁用项并说明待按 MAC 设备选择；不隐式推送到全局设备。族切换只改变模板页上下文，
不得改设备 Tab 当前编辑器或设备 Profile。当前窄范围修正完成后，再实施后文的
多设备和推送菜单计划；后文 §4 关于移动编辑器与 legacy 折叠区的安排现已暂缓。

后续界面修正：用户不需要单独的模板库卡片，因「⋯ → 添加模板」已经列出候选。
移除该卡片的可见 DOM，但保留后台读取模板变体供添加操作使用。旧 Profile 行内
缩略图、点击名称的大图预览、添加模板弹窗内缩略图均须保留；Note4 族采用相同
交互和视觉位置，其缩略图与大图均按精确 `render_target` 的变体渲染。

最新 Profile 语义修正：Note4 族也采用 1.54 旧界面的模板行结构（缩略图、拖拽、
顺序、启用开关、删除及名称预览），最多保存 8 项。Note4 草稿显式保存启用的模板
ID 子集；旧草稿缺此字段时视为全部启用。没有任何启用项时草稿仍可保存，但不得
同步/发布 Profile；有启用项时只把启用项按原顺序纳入待下发设备 Profile。用户
无需选择初始项；设备协议所需 `initial_active_id` 从首个启用项自动派生，不在 UI
显示设置按钮。此修正只针对族草稿和将来的族发布事务；现有 1.54 legacy 设备的
≤3 启用槽位规则仍按旧通道工作。

最终统一要求（覆盖本节之前“1.54 走 legacy UI、Note4 走草稿 UI”的分支设计）：
两个族的模板页只用 `FamilyProfile` 这一套数据模型和一套 UI 读写逻辑。两族都使用
旧版视觉布局、最多 8 个模板条目、启用开关、拖拽、缩略图与预览；都不显示初始项
设置。`enabled_template_ids` 缺省表示全部启用；当前族没有启用项时阻止同步/发布。
真实设备 Profile/Bundle 在发布时仅接收启用项，协议初始项取首个启用项。
UI 不再直接编辑或推送 `profiles.json`，但原文件保留供 legacy API 兼容。

首次迁移应幂等、无损、只补不存在的族草稿，绝不覆盖已有草稿：把旧
`profiles.json` 中的每个配置按原顺序和 enabled 标志导入 1.54 族（legacy 来源
目标确定为 154g）；当 Note4 族没有草稿且恰有一台 Note4 设备带 v2 Profile 时，
复制该设备当前配置为 `default` / `默认` 草稿。多台候选时不任选一台；空 Profile
仍可保存。原 `profiles.json`、设备 Profile 和冻结任务不改变；迁入本身不发布。
旧固件 ≤3 槽的限制仍由未来目标设备发布预检明确拒绝超限，而不再影响两族草稿 UI。
迁移完成须按族记录持久标记；用户后来删除最后一份草稿时，重启不得从旧来源再次
自动生成。来源暂不可用时不写标记，留待下次启动重试。

## 1. 身份与状态边界

| 对象 | 唯一键 | 内容 | 保存位置 |
|---|---|---|---|
| 族 | `render_target` | 画布、像素格式、可用模板变体 | 由已支持的 target 合同列出；不按固件型号拆分 |
| 族内 Profile 草稿 | `(render_target, profile_id)` | 名称、1–8 个有序模板、初始项、字体选择、字段绑定、同步配置 | `PlatformService` 的 `platform/state.json`，与其他 v2 运行数据一起原子保存 |
| 设备 | 规范化 Wi-Fi MAC | 名称、IP、能力、实况、owner 镜像 | 现有 `DeviceRecord`，继续按 MAC 保存 |
| 已应用的设备 Profile | MAC | 该设备当前选择的完整 Profile 快照 | 现有 `DeviceRecord.profile` |
| 发布任务/计划 | MAC | 冻结 Bundle/对象、ACK、PowerPlan | 现有按 MAC 的状态与队列 |

不同 `firmware_target` 的设备可以共享一个 `render_target` 族；每次发布仍须对选中设备
校验 `firmware_target`、ABI、容量、模板数、资源、认证与 owner。显示名称和 IP 不参与
选择的持久身份。

### 族内草稿模型

新增 `FamilyProfile`，字段为 `render_target, id, name, template_ids, initial_active_id,
font_ids, bindings, sync_enabled, full_sync_s, updated_at`。它不含 MAC，也不含 legacy
`enabled` 子集。`id` 在族内唯一，不跨族唯一；`template_ids` 可为空以保存草稿，非空
时每个 ID 必须存在该 `render_target` 的最新版变体。保存只落盘，**不更改任何设备**。

草稿保存在现有 `PersistedState` 中，使用 `#[serde(default)]` 读取旧状态。加载旧数据
时不自动从 legacy `profiles.json` 或任一设备的 Profile 猜出共享草稿；两类旧数据原样
保留。模板页提供明确的“从所选设备复制为族配置”操作，复制目标的 Profile 内容为新
草稿，名称/ID 由用户确认。旧 legacy 配置仍由原路径管理和推送，页面明确标为
“旧版配置（≤3 槽）”；若将来需要转换为 v2 草稿，另做显式导入，不能把 disabled 项
默默丢掉或把 8 项裁为 3 项。新族没有草稿时显示空态和“新建配置”。

## 2. 服务与发布合同

`PlatformService` 新增族草稿 list/get/save/delete/copy-from-device 操作。save 先检查
`render_target` 为已支持目标，再验证 1–8 顺序、初始项、字体和每个模板变体；delete
只删除草稿，不影响已应用设备 Profile 或已冻结任务。UI、Tauri 与 MCP 共用这些服务
操作。旧 `profiles.json` 和 legacy `push_profile` 保持原语义。

第一段实现的 Rust 接口固定为：

```text
family_profiles(render_target: Option<&str>) -> Vec<FamilyProfile>
family_profile_get(render_target: &str, id: &str) -> Option<FamilyProfile>
family_profile_save(profile: FamilyProfile, now: u64) -> Result<FamilyProfile>
family_profile_delete(render_target: &str, id: &str) -> Result<bool>
family_profile_copy_from_device(mac: &str, id: &str, name: &str, now: u64) -> Result<FamilyProfile>
```

`FamilyProfile::validate()` 复用设备 Profile 的 1–8 项、去重和初始项规则；保存时查
每个 `TemplateKey(id, render_target)` 存在，所选字体在库中且确实被这些模板引用。
`id` 限 ASCII 字母/数字/`_`/`-`，与现有 legacy 配置 ID 规则一致；显示名可用中文。
字体像素格式、ABI、容量以及 owner/job 状态留到所选设备的发布预检。族目录使用
`model.rs` 已定义的三个 `RENDER_TARGET_*` 常量；不从设备名、画布尺寸或模板 ID
推断新族。`family_profile_copy_from_device` 只在调用者指定精确 MAC 和新草稿 ID 时
复制，目标族来自该设备已核实的能力；ID 已占用就拒绝，不覆盖。

发布分两个步骤，两个步骤都显式携带 `device_mac, render_target, profile_id`：

`PlatformService` 的新增入口固定为：

```text
family_publish_preview(mac: &str, render_target: &str, profile_id: &str) -> Result<Value>
family_publish_checked(mac: &str, render_target: &str, profile_id: &str,
                       expected_fingerprint: &str, now: u64, bridge_id: &str) -> Result<Value>
```

1. **只读预检**：从族草稿构造候选设备 Profile，验证目标设备完整能力及动态状态，
   不写入 `DeviceRecord.profile`、不建立任务、不 claim、不改变同步或电源状态。返回
   设备名称/MAC、族、Profile 名称/ID、模板顺序、目标合同、预计传输与
   `draft_fingerprint`。离线设备可预检并排队；他人占用、目标不符、未完结 job 给出
   明确阻止原因。
2. **确认发布**：请求包含同一三元组和预检所得 `draft_fingerprint`。服务在同一
   串行临界区重新读取草稿和设备，核对指纹/能力/动态状态，生成该 MAC 的设备
   Profile 和冻结任务；若草稿或设备能力已改变则拒绝并要求重做预检。随后复用现有
   按 MAC 交付流程。失败不得留下半更新 Profile/同步设置；已有冻结任务不受后续
   草稿编辑影响。业务 ACK 仍是成功依据。

现有单设备 `profile_save/publish` API 保留用于旧调用者；新的族发布 API 不用先调用
`profile_save` 再预检，因为那会在用户确认前改变设备。所有面向设备的 v2 UI/MCP
操作增加明确 MAC 参数；仅当系统恰有一台已登记设备时允许兼容性默认。多设备时
缺少 MAC 返回“请选择设备”，不能取列表首项。

## 3. 多设备运行路由

保留当前 `PlatformService` 按 MAC 的核心状态。Bridge 的发现结果先规范化 MAC，
再更新对应设备的 IP、名称、能力及 `last_seen`；同一 MAC 的新 IP 替换其旧 IP，
其他 MAC 不受影响。把当前 `AppCtx` 单组 `device_ip/device_mac/device_name` 与相关
缓存、续约和离线标记改为按 MAC 的运行记录；启动时从现有配置导入旧单设备记录，
不覆盖 `platform/state.json` 中其他设备。BLE、UDP、ARP 和 HTTP 的每条结果均以
认证的 MAC 路由，错 MAC 拒绝，不“切换全局设备”。

`device_link(mac)` 从所选 MAC 的 IP 和该设备的认证材料构造；共享的 Bridge 身份
仍是同一个 `bridge_id`。没有对应材料时返回 waiting/需要绑定，不借用另一台设备
的 token。当前 `bridge-mcp` 的 `data/device-token.json` 是单设备操作 token 缓存；
多设备版改成按 MAC 的本地缓存。旧缓存只可在旧配置含明确 MAC 且请求正是该 MAC 时
沿用，否则经对应设备的绑定加密 BLE 重新获取，不从一个设备复制给另一个。Bridge
业务 endpoint token 可以沿用其现有桥级值，但只能在该设备已认证配置该值后用于请求。
后台逐设备处理独立待发任务、Data 与 PowerPlan；全局传输锁可以串行化
无线交付，但不合并状态或 ACK。legacy 的单设备入口先保留兼容路径，不能把 v2 的
8 项送进 ≤3 槽通道。

## 4. 界面行为

第二段的 Tauri 命令名称固定为 `platform_family_profiles`（返回
`{families:[{render_target,label}],profiles:[FamilyProfile...]}`）、
`platform_family_profile_save(profile)`（返回 `{saved:FamilyProfile,published:false}`）、
`platform_family_profile_delete(render_target,id)`（返回 `{deleted:bool}`）、
`platform_family_profile_copy_from_device(mac,id,name)`（返回 `{saved:FamilyProfile}`）。
MCP 对应工具名为 `family_profiles_v2`、`family_profile_save_v2`、
`family_profile_delete_v2`、`family_profile_copy_v2`，参数/结果同义。Tauri 和 MCP
都只调用同一 `PlatformService` 方法，不直接编辑 JSON 文件。
每个请求/响应都带原始 `render_target`，不以友好名称作键。族目录包含当前三个
`RENDER_TARGET_*` 常量；无设备的族仍可编辑草稿。族菜单与设备列表都按稳定键排序，
与发现顺序无关。

- 「模板」Tab 顶部从左至右为「族 ▾」「Profile ▾」「⋯」。族菜单列出已支持的
  `render_target`，友好名称旁可显示精确 target。选择族只更新本地编辑上下文，
  不修改任何设备。每族记住上次选中的 Profile（仅 UI 偏好）；族下同名 Profile
  可并存，切族后 Profile 菜单、添加模板、预览尺寸和推送菜单都重新计算。选择偏好
  可用浏览器本地存储，失效 ID 清除并回到该族第一项；不写 Bridge 业务状态。
- v2 Profile 编辑区从设备 Tab 移到模板 Tab；模板库按当前族过滤。若某族没有
  已登记设备，仍可编辑草稿；发布菜单说明“尚无兼容设备”。旧版 200×200
  `profiles.json` 管理区移入明确标注的“旧版配置（≤3 槽）”折叠区，保留其
  新建/重命名/删除和旧通道功能；旧版配置不出现在 v2 Profile 菜单。
- 「推送到设备 ▸」子菜单列出 `render_target` 相同且静态能力可支持该草稿的全部
  已登记设备，显示名称、MAC 和离线/占用/待会合提示。离线不自动隐藏；动态阻止
  项在预检中给原因。0 台显示空态，1 台在按钮下方以小字显示“目标设备：名称 · MAC”，
  多台不自动选。点击一台先打开预检/确认窗口，确认后只发布到该 MAC。
- 「设备」Tab 有明确设备选择器。身份、实况、已安装/active 模板、作业、功耗与恢复
  读取都按选中 MAC 刷新；切换不改变模板 Tab 的族选择。设备列表保留离线记录。
- 族草稿保存、设备 Profile 应用、发布排队与设备 ACK 分别显示状态；保存不发布，
  排队不等于设备已显示。确认窗口必须显示族、Profile、设备名称/MAC、目标合同与
  冻结摘要，避免同名设备或同名 Profile 错发。

模板页实现时使用独立的 `selectedRenderTarget`、`selectedFamilyProfileId` 与
`selectedDeviceMac` 状态；不要复用当前 `platformProfile` 同时表示族草稿和设备
Profile。首次打开优先恢复本地 UI 上次所选族，失效时选当前已选设备的目标，再失效
时选受支持目录第一项。`pt-render-target` 的可编辑选择由族下拉框取代；保存草稿时
请求中的 target 必须等于所选族。模板库仅展示该 target 的变体；同 ID 预览先调用
带 `render_target` 的 `platform_template_get` 取精确源 JSON，再以 JSON 参数调用现有
`platform_template_preview`，不依赖“不带 target 的首项”。设备 Tab 的状态刷新仅
更新 `selectedDeviceMac` 对应的实况，不覆盖族草稿编辑状态。阶段性界面改造期间，
尚未接通族发布事务的按钮必须明确禁用，不能回退到隐式全局 MAC 发布。

## 5. 分段实现与客观验收

1. **族草稿与 API**：新增模型/持久化/验证和明确的读写命令；旧平台状态与旧
   `profiles.json` 原样可读。两族可各有 `default`，重启后仍独立；保存不改设备。
2. **模板页联动**：族下拉框位置、过滤、空态、创建/重命名/删除与模板预览；旧
   legacy 配置放单独标识区域。切族后不沿用另一族 Profile ID 或模板选择。
3. **多设备路由**：Bridge 同时发现/保存 1.54 英寸与 Note4；各自 IP/token/
   owner/队列不串；UI/MCP 指定 MAC，未指定时不选首项。
4. **发布菜单与事务**：0/1/多设备、同族多台、跨族、离线、占用、旧任务进行中
   均有明确结果；预检只读，确认后单台冻结，失败无半更新。
5. **实机与回归**：两设备分别发布/重启恢复；legacy ≤3 槽与 v2 1–8 项不混用；
   不改 token/claim/owner/PowerPlan 语义。每段只跑对应的最小有效检查；运行中
   Bridge 的替换/重启与设备发布是单独的实施步骤。

本文件是设计合同。实现代理只处理分配的阶段与文件，不自行改模型、迁移或发布语义；
若代码约束使合同无法成立，停在该点报告证据，由主代理决定修订。
