# Bridge 多设备与模板页迁移 — 状态

2026-09-23：需求与设计已定稿，见 `plan.md` 和 `design.md`。族键明确为
`render_target`。现有设备 Profile 与 legacy `profiles.json` 原样保留；v2 族内草稿
另存于平台状态，保存不发布。发布必须选择 MAC，先只读预检，再确认冻结单台任务。

`FamilyProfile`、持久化和 Tauri/MCP API 已落地。用户多次纠正后最终确定：模板页
两族统一使用族 Profile、旧版行视觉与同一套 CRUD/拖拽/预览/启用逻辑；最多 8 项，
无初始项 UI，启用子集显式保存，零启用项显示“暂不同步”。独立模板库卡片已按要求
删除，添加模板弹窗仍显示缩略图并按 `render_target` 精确预览。设备 Tab 编辑器未移动。

用户批准无损迁入。启动时旧 `profiles.json` 的 3 份配置按原顺序/启用标志导入
1.54 族；Note4 已有 `default` 草稿被保留（未覆盖）。按族完成标记防止删除后
重启自动复活，原文件与设备 Profile 保留。迁入前备份在
`artifacts/family-profile-migration-20260923-224900/`。新 Bridge 已构建并后台运行：
主进程 PID 46796、watchdog PID 15716，MCP 端点监听。内联 JS 语法、Rust 构建、
`git diff --check` 通过；未对设备发布或刷机。

按 MAC 多设备路由与统一族发布菜单仍是待办，两个族的统一推送项暂时禁用。

## 2026-09-25：Phase 3 开工 —— 按 MAC 多设备路由 + 删除桥侧 legacy/迁移

用户决定（当轮）：范围取 `design.md` 第 3 阶段（完整多设备运行路由），**并同步清掉桥侧
legacy 通道与历史迁移代码**（用户明确表示没有要求过对历史版本的支持与迁移）；改完由
实施方停桥 → 重建 → 重启并自检。现场、分步骤与验收见 `task-phase3.md`。

已完成：
- **Profile 丢失已修复并还原**：根因是 `migrate_family_profiles` 在旧源为空时循环零次、
  `complete` 仍为 `true`，于是照样写 `family-profile-154g-imported` 标记、从此永不重试
  （上次删 `target/debug` 连带删掉 `data/` 后正是这条路）。两族 `默认` 草稿已用
  `family_profile_copy_v2` 从设备 v2 Profile 还原（`mini,quad` / `codex-status-a`），
  只落盘、未发布。
- **迁移与 legacy profile 存储/≤3 槽推送已删除**：`core/src/profile.rs`、
  `core/tests/legacy.rs`、`tools/test-bridge/profiles.seed.json`、`paths::profile_seed`、
  `Config.profiles/profile_seed`、Tauri `get_profiles`/`save_profile`/`delete_profile`/
  `push_profile`、MCP `profiles_list`/`profile_save`/`profile_push`、
  `push_templates_http`/`remote_template`、排队 flush 的模板分支。`AGENTS.md` 已同步订正。
- **B1 已写好待接线**：`bridge/crates/app/src/device_runtime.rs`（按 MAC 的
  `DeviceRuntime` + `DeviceRegistry` + 单测）。

待做：A3/A4（`legacy` 设备标志、`claim_unsupported`、`device.rs` 的 HTML `/status` 回退）、
B2–B6（AppCtx 换成注册表、发现链按 MAC 路由、claim/renew 按 MAC、`platform.rs` 显式 MAC、
设备页选择器）、A5 决策（文件模板库 `Library` 去留）、重建重启与 `PROGRESS.md`/`backlog.md` 更新。
