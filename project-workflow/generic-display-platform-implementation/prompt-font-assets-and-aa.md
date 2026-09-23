# Prompt：字体资产化（随 Profile 推送 + 增量同步）与渲染引擎抗锯齿

你是 Codex Status 的固件/桥接实现代理。本任务有两个工作流：**A. 把字体变成可随 Profile 推送、按内容去重同步的资产**；**B. 给渲染引擎加抗锯齿（灰度）能力**。先读 `AGENTS.md`、`PROGRESS.md` 最新节、`docs/generic-display-platform-design-v2.md`（template/target/bundle 章节）、`project-workflow/generic-display-platform-implementation/status.md`，以及 `src/template_engine.{h,cpp}`、`src/refresh_policy.cpp`、`src/bundle_store.{h,cpp}`、`bridge/crates/core/src/platform/{model,service,store}.rs`。

## 现状（不要凭想象，先核对）

- 字体**编译进固件**：小屏 `src/font8/12/16/20/24.cpp`（定宽 1bpp）；大屏 `src/font_noto.h` + `src/font_noto_nt16.h`/`font_noto_nt30.h`（比例字体，ASCII 95 字形，来自上游 Noto LVGL 字库裁切）。
- 模板用**全局字体索引**引用字体：`CtOp.font`（uint8）→ `CT_FONTS[] = {f8,f12,f16,f20,f24,nt16,nt30}`；Rust 侧 `bridge/crates/core/src/template.rs` 的 `FONTS` 是同一份名单。
- 字体表**在三个地方各有一份**：`template_engine.cpp`（`fontByName`/`CT_FONTS`）、`refresh_policy.cpp`（`rgnBuild` 与 `rgnBuildCt` 里的 `switch (op.font) case 0..4`）、`main.cpp`（`clkFontId`/`clkFontById` 的时钟专用路径）。**新增字族必须同时改这三处**，否则区域推导会退回整帧全刷（`out.wholeFrame = true`）或时钟字体识别失败。
- 大屏目标：Note4 400×300 SSD2683，`TARGET_PIXEL_FORMAT "1bpp"`、`TARGET_VERIFIED 0`、`TARGET_PARTIAL 0`；波形 LUT 未验证（缺 `CODEX_SSD2683_LUTS` 时显式 `#error`）。另有一个 `esp32-s3-epaper-154g-gray4`（`2bpp`/`gray4`，`RENDER_TARGET_ID "epd-200x200-2bpp-gray4"`）目标，标着 `blocked_by_hardware_arrival`。
- v2 语义（必须沿用的既有合同）：Profile = 1–8 个有序模板；**显式发布**才冻结一个完整 Bundle；设备侧 A/B bundle + 单一 active context；数据走 seq + CRC32 + ACK 确认；`/update`、`/doUpdate`、`POST /claim` 必须带 token；label/用户名缺失、`resetCredits<=0` 等表现由模板 `when` 分支决定。
- 关键不变量：固件引擎 / Rust canonical / Python 测试桥三方哈希一致；宿主预览与固件逐像素一致；未识别的 `type/font/bind` 整份拒绝，不半渲染；文本绘制前 ASCII 净化；legacy（≤0.15.10）保持 ≤3 槽，不得静默裁剪 8 项。

## 中途状态（接手前必读，2026-09-23）

这一节记录"已经动过但没做完"的改动。**树当前在测试目标上编译不过**，接手时先决定：继续收尾，还是按下面的回退说明还原。

### 已完成且验证过的部分

1. **大屏比例字体族 `nt16`/`nt30`**（小屏 `f8–f24` 一行未动）：
   - `src/font_noto.h`（`Note4Glyph`/`Note4PropFont` 描述符）+ 生成物 `src/font_noto_nt16.h`、`src/font_noto_nt30.h`（ASCII 95 字形、1bpp、含 `LINE_HEIGHT`/`BASE_LINE`/`MAX_ADV`/`BLOB_BYTES`）。
   - `src/template_engine.{h,cpp}`：`propFontByName`/`propFontByIndex`、`propTextWidth`、`drawPropText`；`CT_FONTS` 扩到 7 项；`tplValidateCt` 的字体上界改为 `FONT_COUNT`；新增 `tplFontCellByName`/`tplFontCellByIndex`。
   - Rust 侧 `bridge/crates/core/src/template.rs` 的 `FONTS` 同步为 7 项。
2. **400×300 模板已切到 `nt16`/`nt30`** 并按像素实测重新对齐（字号变小后主数值区域下移 5 px，"%"/RESET/页脚等逐个对齐）。
3. 验证证据（在中途改动**之前**取得）：`--compare-compiled` 对 400×300 与 quad 均为 `diff pixels: 0`、CTP1 序列化往返 0 差异；`cargo test -p bridge-core -p bridge-render -p bridge-mcp` 全绿（83 passed）。
4. `tools/note4-fonts/crop_lvgl_font.py`（LVGL 字体解析/裁切/emit/specimen，含打包模型自检）与 `tools/note4-fonts/README.md`；`pio run` 三个 env 均 SUCCESS（该次构建**早于**下面的中途改动）。

### 中途未完成（当前状态）

- `bridge/crates/render/src/lib.rs`：已新增 `set_panel(w,h)` 与 FFI `codex_set_panel`，并把 `rgn_build_compiled(blob)` 改成 `rgn_build_compiled(blob, width, height)`。
  **唯一未跟上的调用点**：`bridge/crates/render/tests/compiled.rs:137` 仍是 `rgn_build_compiled(&blob)`。
  现状：`cargo check -p bridge-render` 通过，`cargo check -p bridge-render --tests` 报
  `E0061: this function takes 3 arguments but 1 argument was supplied`。
  - 继续收尾（推荐）：该测试里传模板画布，例如 `rgn_build_compiled(&blob, canvas_size(&tmpl).unwrap().0, canvas_size(&tmpl).unwrap().1)`。
  - 回退：删掉 `set_panel`/`codex_set_panel`，签名改回 `rgn_build_compiled(blob)`（代价：区域推导只能用默认面板几何）。
- `rgn_build(template)`（JSON 路径）**尚未**按模板画布调用 `set_panel`，因此宿主在 400×300 上做区域推导仍会使用 200×200 几何。这一步没做完。
- `bridge-render` CLI 还没有 `--regions`（原计划用它打印 `rgn_dump()` 作为区域推导证据）。
- `src/refresh_policy.cpp` 已改为调用共享的 `tplFontCellByName`/`tplFontCellByIndex` 并删除本地 `fontByName`；**这次改动之后没有重新编译过固件**（需要重跑 `pio run` 确认）。
- `src/main.cpp` 时钟快路径的 `clkFontId`/`clkFontById` 仍只认索引 0..4：noto 模板的 `device.now` 会打 `[clk] clock font unknown` 并放弃时钟快路径（优雅退化，不是崩溃），未修。
- 三处字体表只收敛了两处（`refresh_policy` 已走 helper）；`main.cpp` 时钟路径仍是独立的一份。

### 未开始（本 prompt 的工作流 A/B 本体）

- 字体资产化：`font_id` 内容寻址、Profile 绑定、设备按 Profile 存字体、单 Profile 增量同步、失败路径、体积审计；
- 抗锯齿：2bpp/灰度目标、字体 2bpp 量化、灰度渲染与刷新策略、宿主灰度 PNG；
- 单一字体注册表（含时钟路径）与 `--regions` 验证入口。

## 工作流 A：字体作为 Profile 绑定的资产

### 已定的设计约束（不要再引入设备端共享）

- **不做设备端跨 Profile 共享**：字体按 Profile 归属存放，同一个字体被两个 Profile 用到时，设备上允许存在两份；不需要引用计数、不需要 LRU/GC、不需要"卸载正在使用的字体要拒绝"这类设备端共享逻辑。
- **同步一次只针对一个 Profile**：一次同步 = 发布/推送某个 Profile 所需的字体集合，设备只需持有被同步的那个 Profile 的字体；换 Profile 时按新集合替换旧的即可。
- **共享与去重放在 bridge 端**：桥自己维护字体库（按内容 id 去重、一份 blob 可服务多个 Profile）；推送前比对设备已有哪些字体，**只上传缺失的**，已有的一律不发。

### 目标

Profile 除了模板，还声明它需要的**字体集合**；发布/推送到设备时，只传设备缺失的字体（逐个 Profile 同步）；设备按 Profile 保存字体；模板通过稳定的字体引用取字形。

### 要求

1. **字体标识符**：内容寻址。`font_id = crc32(canonical font payload)`（与现有 canonical JSON/CRC 方案一致），并带一份描述符：`{id, name, family, size_px, bpp(1|2), pixel_format(bw|gray4), coverage("ascii"), line_height, base_line, blob_bytes, glyph_count, metrics_format}`。描述符进 Profile/状态上报，**字形数据不进描述符**。
2. **模板怎么引用**：不要再用全局索引。改成 Profile 作用域的字体槽：模板里写字体名（如 `"font": "nt30"`），桥在编译/发布时把名字解析为该 Profile 字体集合里的**槽位**，记录 `{font_id}`；设备侧按 `font_id` 在**该 Profile 的字体目录**里查找。**找不到字体 → 整份模板拒绝**（与现有 `ct_font` 语义一致，不半渲染）。
3. **设备字体存储**：按 Profile 分目录（如 `fonts/<profile_id>/<font_id>.bin`），不要塞进 CTP1 记录。优先放 16 MB 板的 `assets` 分区（4 MB，当前内容近乎空），小屏回退 LittleFS；带 CRC、长度上界、写入预算与掉电/半写恢复；字体不可变 → 写一次永不改写，升级即换 `font_id`；替换 Profile 字体集合时删除该 Profile 目录下不再需要的文件。
4. **增量同步协议（单个 Profile）**：同步开始时，设备上报**该 Profile**当前已安装的 `font_id` 列表（`font_inventory`，含 blob_bytes/`pixel_format`）。桥按该 Profile 的需要集合取差集，**只推送缺失项**；每项：BEGIN(id,bytes,crc) → CHUNK → COMMIT → 设备校验 CRC 后回 ACK。对同一 `font_id` 重复推送必须是幂等无操作（第二次传输字节为 0）。沿用现有 seq/ACK 语义，不引入新的隐式续租。
5. **与 Bundle/Context 的关系**：Bundle 只引用 `font_id`，不携带字体数据；字体属于"Profile 的资产"，与 Bundle 一起被替换/回滚（A/B 切换时按各自 Profile 的字体集合校验，缺字体就拒绝切换到该 Bundle，而不是画错）。
6. **失败与边界**：磁盘不足、传输中断、CRC 不符、字体描述符与 `bpp/pixel_format` 不匹配（例如把 gray4 字体推到 1bpp 目标）、目标 `render_target` 与字体 `pixel_format` 不符 → 全部显式报错并保留旧状态；不得出现"半个字体文件被当成有效字体"。
7. **桥侧与 UI/MCP**：桥侧字体库要能列出被哪些 Profile 引用（共享只在这一层体现）；`platform_publish` 预览要说明"本次将推送哪些字体、多少字节、哪些已存在会被跳过"；新增只读工具列出字体库、Profile→字体依赖、设备已装清单；`profile_save_v2` 只落盘不发布（沿用现有规则）。UI 要能看到字体占用（分区/字节）。
8. **体积审计**：给出实测数字：各字体 blob 字节、单 Profile 依赖集合总量、`assets` 分区（4 MB）与 LittleFS 的占用与余量；说明与 ROM 原资产的共存策略。
9. **向后兼容**：不认识远程字体的旧固件必须**整份拒绝**使用该字体的模板（而不是画错）；小屏字族（`f8–f24`）保持编译进固件不变；legacy 通道不动。

## 工作流 B：渲染引擎抗锯齿

### 目标

在**大屏灰度目标**上支持灰度字形（4 级），1bpp 路径行为完全不变。

### 要求

1. **先确定面板能力**：SSD2683 的灰度波形 LUT 必须来自实测/厂商资料（`CODEX_SSD2683_LUTS`），未验证前只允许**宿主渲染 + 预览**落地，设备侧保持 `#error`/拒绝。把"已确认/未确认"写进目标头文件与 status。
2. **像素格式**：新增 `epd-ssd2683-400x300-2bpp-gray4`（或等价）render target 与 pixel format `2bpp/gray4`；模板、字体、Bundle、OTA 目标校验全部按 target 分流；1bpp 目标不接受 2bpp 字体，反之亦然。
3. **字体数据**：裁字工具（`tools/note4-fonts/crop_lvgl_font.py`）增加 **2bpp 输出**：从上游 4bpp 源量化到 4 档（阈值/抖动策略要固定并写成测试），字形按 2bpp 连续打包（注意上游 LVGL 是**每字形连续打包、行不补齐**，本仓库引擎表是**行补齐**，转换规则要写清楚）。给出 1bpp vs 2bpp 的体积对比。
4. **绘制路径**：`template_engine.cpp` 的文字绘制按像素格式选择 ink/level 写入；背景色、region 裁剪、`scale`、对齐与自动缩放逻辑在灰度下语义一致；不得让灰度影响 1bpp 的像素结果（对拍必须证明）。
5. **区域/刷新策略**：`refresh_policy.cpp` 的区域推导同时支持**比例字体**与灰度（现在 `op.font` 只认 0..4，遇到新字族会整帧全刷——这是必须先修的缺口）；明确灰度下的刷新策略：默认全刷，只有在波形与残影实测通过后才启用灰度局刷，并把策略写进文档。
6. **字体注册表收敛**：把三处重复的字体表（`template_engine.cpp`、`refresh_policy.cpp`、`main.cpp` 时钟路径）收敛成**单一注册表**，名字/属性/索引只有一份来源；工作流 A 的字体槽解析也走它。这一步是 A、B 的共同前置。
7. **宿主链路**：`crates/render` 支持 2bpp 渲染与灰度 PNG 输出（`bits_to_png_size` 扩展），`--compare-compiled` 覆盖 2bpp，JSON/compiled 逐像素一致。
8. **代价说明**：灰度刷新更慢、更易残影、多数 EPD 的局刷只支持黑白；在文档里明确"开启 AA 就等于放弃现在的局刷指标"，并给出实测数据（刷新耗时、残影对比）。

## 实施顺序

1. 收敛字体注册表（三处 → 一处），确认 1bpp 行为与既有测试完全不变。
2. 修 `refresh_policy` 对新字族（比例字体）的区域推导；宿主对拍证明与 JSON 路径一致。
3. 工作流 A：字体标识符 + 描述符 + 设备字体库 + inventory/差集推送 + 幂等 + 失败路径 + 体积审计。
4. 工作流 B（宿主先行）：2bpp 字体量化与渲染、灰度 PNG 预览、对拍与测试；设备侧等 LUT 实测后再启用。
5. 更新 `PROGRESS.md`、`status.md` 与本目录的 task 文档。

## 验证（给出精确命令与结果，不要把预览当成实机验证）

- `cargo test -p bridge-core -p bridge-render -p bridge-mcp`（隔离 `CARGO_TARGET_DIR`）；
- `cargo run -p bridge-render -- --template <tpl> --usage <usage> --compare-compiled`：JSON/compiled 逐像素 0 差异，1bpp 与 2bpp 各一组；
- 字体同步：同一 `font_id` 连推两次，第二次传输字节为 0；写入中途掉电/截断后重启，旧字体仍可用；
- 分区/体积：给出 `assets` 与 LittleFS 的实测占用；
- `pio run`（含新目标）编译通过；设备侧灰度在 LUT 实测前不得声称可用。

## 非目标

- 不改小屏字体族（`f8–f24`）与其渲染结果；
- 不自动发布、不自动推送（保持"发布是用户显式动作"）；
- 不把 8 项 Profile 静默裁剪成 3 项；
- 中文/CJK 字形不在本次范围（上游 common 字符集与 CBIN 方案另行评估）。

## 需要先回答的问题（实现前给出结论）

1. 字体库放 `assets` 分区还是 LittleFS？两者混用时的优先级与迁移策略？
2. 单个 Profile 的字体集合上限（个数/总字节）定多少？超出时报错还是拒绝发布？
3. 模板字体引用用"名字"还是"内容 id"？名字便于人读，id 便于去重——是否两者都存（名字 + 解析后的 id）？
4. 抗锯齿的收益是否值得放弃局刷？给出一组实测对比（同画面：全刷耗时 vs 局刷耗时、灰度 vs 1bpp 的残影）后再决定是否在 Note4 上启用。
5. 是否保留"字体编译进固件"作为兜底（例如 `nt16` 常驻固件，只把大字号/多权重放远程）？
6. 桥侧字体库的来源：是否把 `tools/note4-fonts` 生成的表直接作为桥的字体库源（与固件内置表同源），以及如何避免"桥里的字体"和"固件里的字体"两份实现漂移？
