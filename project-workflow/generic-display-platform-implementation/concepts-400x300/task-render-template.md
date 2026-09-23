# Note4 400×300 模板首版渲染

目标：把已选 A 方案实现为可校验、可保存的 400×300 v2 模板，经本机 Bridge MCP 保存并用固件同源宿主引擎渲染 PNG。仅宿主交付，不发布设备。

步骤：
1. 让 Rust 结构校验、CompiledTemplate、宿主 C++ 引擎和 PNG 编码按 render target 使用 400×300；保留现有 200×200 行为。
2. 用现有图元/字体和字段制作 A 方案，状态栏左日期/时间、右蓝牙/Wi-Fi/主机/电池/百分比。若字段合同缺失，补最小本地字段或明确该图标的静态示意属性。
3. 构建 Bridge，通过 MCP 校验、保存；以 MCP 渲染正常示例，再检查缺失 weekly/5h 等边界。完成后更新进度和验证记录。

约束：不更新 Profile，不调用 publish/activate，不刷机。Note4 固件和局刷待硬件 bring-up 后验证。

结果（2026-09-23）：三步宿主范围已完成。MCP `template_save_v2` 返回 `saved=true`、`published=false`；`template_render` 返回两张 400×300 PNG（正常与仅 weekly/RC=0）。Rust/C++ 宿主测试与 Bridge 构建通过。设备侧仍待 Note4 bring-up；连接图标当前为静态图形。

rev 2（2026-09-23，用户反馈后）：补做第 3、4 步的其余验收——新增剩余量 0/100、离线/无数据、超长套餐/账号名场景预览，`%` 改为与数字垂直居中；`measure-preview.py` 逐像素核对状态栏对齐；`bridge-render --compare-compiled` 验证 400×300 的 JSON/compiled 像素一致与 CTP1 往返（`diff pixels: 0`），quad/mini/full 200×200 回归通过；`cargo test -p bridge-core -p bridge-render -p bridge-mcp` 83 passed/0 failed。模板已重新保存（`source_crc=2b523381`，`compiled_crc=41c31abd`），仍未发布、未刷机。细节见 `README.md` 与 `../status.md`。
