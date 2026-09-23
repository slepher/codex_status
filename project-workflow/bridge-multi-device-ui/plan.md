# Bridge 多设备与模板页迁移计划

Status: 待实现。需求记录于 2026-09-23；具体数据模型、迁移和发布合同见
`design.md`，本文件保留任务索引与验收。

## 用户需求与交互解释

1. Bridge 同时管理多台设备。设备以 Wi-Fi MAC 为唯一键，各自保留发现/连接信息、
   token/owner、Profile、同步与 PowerPlan、发布队列和状态；显示名、IP、BLE 地址不作主键。
2. 在「模板」Tab 顶部、截图中「默认 ▾」Profile 下拉框**左侧**新增「族 ▾」下拉框。
   族以稳定的 `render_target` 为键，关联相应画布和渲染能力；选项用人可读名称
   （如 200×200 黑白、Note4 400×300 黑白）展示。
   选择族后，右侧 Profile 下拉框只显示该族的配置，模板列表/可添加变体、预览尺寸和
   推送目标也跟随切换；不同族可各有名为「默认」的 Profile。每族记住上次选中的
   Profile；族下无配置时显示空态和「新建配置」，不借用另一族的默认配置。
3. 将目前位于「设备」Tab 的 **v2 模板配置区**迁到「模板」Tab：Profile 1–8 顺序、
   render target、添加模板、字体选择、同步开关、保存、发布预检/发布/取消。模板库与
   Profile 编辑放在同一工作区。「设备」Tab 保留所选设备的身份、在线状态、已安装/active
   模板、作业状态、功耗和恢复读取等设备实况。
4. 「推送到设备」改为子菜单，列出当前族与 Profile 对应 render target、所需模板都有该
   target 变体且设备能力满足静态约束的已登记设备；项中显示名称和 MAC，明确离线/占用/
   待会合状态。离线但可排队的设备不因暂时不可达而从列表消失。选择目标后预检并显示确认摘要，再对该 MAC
   显式发布；动态限制（例如他人占用、已有进行中的 job）由该设备的预检说明，不能
   依赖设备列表第一项或全局默认 MAC。
5. 如果当前族与 Profile 对应的设备类型仅有**一台已登记且兼容的设备**，在「推送到设备」
   按钮下方以较小的 note 文本显示其名称和 MAC，例如「目标设备：Note4 · D7A340」。
   子菜单仍可打开；note 只提示当前匹配结果，不代替显式确认。0 台显示无兼容设备原因；
   多台不暗选目标，必须在子菜单中选择。

「族」和模板变体都按 `render_target` 匹配；仅凭名称相似或尺寸相同不算兼容。
不同 `firmware_target` 的设备可以属于同一族，但发布时仍须逐台核对完整能力及目标
合同。族内可编辑 Profile 是可复用的本地草稿；选择具体设备发布时，
把选中草稿显式映射/冻结为该 MAC 的 v2 Profile/PublishJob。设备当前 Profile、
active 和已排队作业仍按**设备 MAC**保存。切换族/Profile 只是界面选择，不改变设备。

## 当前实现差距

- `bridge/crates/core/src/platform/service.rs` 已有按 MAC 的 `devices`、Profile 和
  PublishJob；核心领域模型可作为多设备基础。
- `bridge/crates/app/src/main.rs` 的 `AppCtx.device_mac` 是单个全局目标；多个 UI/Tauri
  命令和 MCP 工具从它取隐式 MAC。`platform_publish` 虽可接收 `mac`，预检、取消、
  Profile 读取、状态刷新等仍多处取全局目标。
- `bridge/crates/app/ui/index.html` 的 `refreshPlatformDevice()` 固定取
  `data.devices[0]`，v2 Profile 区放在设备 Tab，发布按钮调用无目标参数的预检。
- 截图中「默认 ▾」是 `knownProfiles` 的旧版配置菜单；`profiles.json` 的旧配置没有
  族字段，v2 Profile 则按设备 MAC 存放。设计决定为 v2 新建族草稿并保留旧配置原样，
  通过明确复制操作导入现有设备 Profile；不按名字猜族，也不自动把 legacy 的 enabled
  子集转换为 v2。旧版 `push_profile` 仍限 legacy ≤3 槽，v2 1–8 项不得进入旧通道。

## 实施顺序

| 阶段 | 工作 | 验收 |
|---|---|---|
| 1. 族与草稿模型 | 以支持的 `render_target` 建族目录；本地可编辑 Profile 以 `render_target + profile_id` 区分，持久化到现有平台状态；旧 legacy 与现有设备 Profile 原样保留，只经明确操作复制 | 同名「默认」可存在于不同族；重启后归属稳定；保存不修改设备或发布作业 |
| 2. 多设备目标模型 | 梳理现有全局设备绑定、发现、token、BLE/HTTP 调度和持久化；引入按 MAC 的多设备运行目标，保留已有独立设备数据与作业 | Bridge 重启后两台设备同时存在，不因新设备发现覆盖另一台的身份/IP/token/Profile/作业 |
| 3. 服务/API | 让 Profile 读取/保存、发布预检/发布/取消、激活、状态刷新、功耗操作等设备相关 UI 与 MCP 路径传递或解析明确 MAC；族内草稿的读取/保存传递明确 render_target；共享同一 application service | 请求始终落在指定族或 MAC；有多台设备却未指定目标时返回“请选择设备”，不使用列表首项 |
| 4. 设备选择 | 设备 Tab 提供明确的所选设备；切换后身份、状态、功耗、占用、恢复信息一并切换，刷新保持选择；设备名相同时仍由 MAC 区分 | 两台设备快速切换不会串数据；离线设备仍可查看其持久记录与待发任务 |
| 5. 模板页 | 在截图中 Profile 下拉框左侧加入族下拉框；切族过滤并恢复族内 Profile、模板变体与预览；将 v2 Profile 编辑/发布区移至模板 Tab | 空族、同名 Profile、切换后刷新、重启恢复选择均正确；1–8 项与 legacy ≤3 槽清楚分流；保存不发布 |
| 6. 推送菜单 | 从当前族/Profile 和设备能力计算兼容设备；“推送到设备”子菜单逐项选择、预检、确认后按 MAC 发布；单设备显示 note | 0/1/多台、同族/异族、离线、被占用、任务进行中均有明确结果；不会错发或自动群发 |
| 7. 实机收尾 | 用 1.54 英寸 SSD1681 与 Note4 SSD2683 两台设备核对族联动、状态、独立发布与恢复；更新 `PROGRESS.md` | 两族配置互不混用；每台 Profile/Bundle/job/plan 独立，错误 target 被拒绝，legacy ≤3 槽路径仍可用 |

## 不变量

- 每次发布是用户显式选择**一台**设备的动作；无隐式批量发布。即使模板 variant
  支持多台同类型设备，也分别冻结和跟踪每台 PublishJob，分别显示 ACK/失败。
- `save` 只落盘；token、MAC、owner/claim 与 PowerPlan 规则不变。401/409 必须在
  目标设备上清楚展示，不能切到另一设备重试或强制接管。
- 已排队的发布任务是冻结内容；切换页面或目标设备、编辑模板或 Profile 不改变旧任务。
- 切换族不能把当前 Profile 发送给旧目标；确认对话框必须同时列出族、Profile、设备
  名称/MAC、render target 和冻结内容摘要。
- 迁移界面前先接通明确 MAC 的服务路径。不能只把 DOM 移到新 Tab 后仍调用全局目标。

当前完成状态：仅记录任务；未修改 Bridge/UI 实现、未重建、未部署或实机验证。
