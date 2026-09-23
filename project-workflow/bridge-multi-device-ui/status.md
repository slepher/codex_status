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
