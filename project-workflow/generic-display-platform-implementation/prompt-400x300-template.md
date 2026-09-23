# Prompt：为 Note4 设计并交付 400×300 模板

你是 Codex Status 的模板与渲染实现代理。请为 ZecTrix Note4（400×300、1bpp 黑白、SSD2683）设计一份**可验证、可保存的实际模板 JSON**，并生成预览和边界场景证据。交付物不是纯效果图。先读 `AGENTS.md`、`PROGRESS.md` 最新节、`docs/generic-display-platform-design-v2.md` 的模板/target 章节、`project-workflow/generic-display-platform-implementation/status.md`、`tools/test-bridge/templates/quad.json`、`src/template_engine.{h,cpp}`、`bridge/crates/core/src/{template,compile}.rs` 和 `bridge/crates/render` 的预览接口。

## 目标与布局

- 使用独立变体键 `template_id + render_target`，目标为 `epd-ssd2683-400x300-1bpp`、画布恰好 400×300。保留同一模板 ID 在 200×200 上的现有变体，不自动缩放或覆盖。只用引擎已支持的字体、图元、绑定与条件；显示文本做 ASCII 净化，黑白对比明确。
- 以“一眼看清配额”为主：周/月桶与 5h 桶数值最醒目；重置时间、套餐/账号、设备时间、电量与连接状态次级。利用横向空间，数字和标签有稳定的视觉层级，留足边距。尽量使用白底、细分隔与小面积黑底，避免大块反白区域增加局刷和残影负担；局刷未验证前按全刷设计。
- 遵守显示规则：5h 桶不存在时显示静态 `100` 且不显示其重置时间；`resetCredits.availableCount<=0` 时整条 RC 信息隐藏；拿不到用户名/label 时整行隐藏。对周/月桶缺失、长套餐名、0/100 边界、离线和数据过期状态给出明确表现，不能留下旧文字或重叠。字段名以现有合同为准，不发明新字段。
- 为屏幕拍摄与设备按键切换保留一致的标题/页脚结构；不要把“保存模板”写成“已发布到设备”。

## 实施顺序

1. 先核对尺寸链路：当前固件 `tplSetCanvas` 可配置 400×300，但 `bridge/crates/core/src/template.rs` 的 `validate_template` 与 `compile.rs` 的 CompiledTemplate 验证仍引用固定 200×200 `CANVAS`。查清宿主渲染入口、CTP1 元数据、边界校验和设备安装时的实际画布来源；以最小改动让 400×300 target 在 Rust 校验、C++ 编译、预览与设备合同中一致，同时保留 200×200 的原有行为。若硬件资料仍缺失，模板的宿主交付可以继续，实机发布标为待验证。
2. 先给出 400×300 布局草图与字段清单，再在项目模板库中新增 JSON 变体。矩形、文本区域、字体缩放和图标须在屏幕内，避免相邻区域覆盖；不要以运行时自动缩放来代替专用布局。
3. 生成至少四个实际像素预览：正常数据、缺少 5h 桶、RC 为 0/缺失、长文字或离线/过期。检查字形、裁切、对齐、空白、遮盖和黑白面积；根据预览调整模板。给出正常数据预览 PNG 的绝对路径及各场景文件路径。
4. 运行有针对性的校验：模板结构/绑定严格校验、CompiledTemplate 400×300 编解码、宿主与固件共享引擎的像素一致性；确认现有 200×200 模板仍能编译/预览。协议若改动，保持 Rust、固件和 Python 测试桥的 canonical JSON/hash 语义一致。记录精确命令和结果，不把仅通过预览说成实机验证。
5. 只保存模板与预览；Profile 保存、模板保存和 MCP save 都不触发发布。若 Note4 尚未完成硬件 bring-up，停止在宿主验证和可审核产物；实机发布、局刷验证留给设备验收阶段。更新 `project-workflow/generic-display-platform-implementation/status.md` 与有必要的 `PROGRESS.md`。

最终交付：模板 JSON 路径、正常及边界预览路径、布局/字段映射简表、验证结果、仍需 Note4 实机确认的事项。不要提交、刷机或自动发布。
