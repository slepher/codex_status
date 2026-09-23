# Prompt：字体资产传输落地 + 模板资产解析（承接 task-7）

> 历史执行提示：其中独立字体 inventory、8 字体/48 KiB 产品上限和逐项发布方案已被 `project-workflow/note4-bridge-publish/design.md` 及待双方确认的 `protocol.md` 取代。勿据此实现增量端点。

你是 Codex Status 的固件/桥接实现代理。上一轮把**字体变成数据的引擎半边**做完了（单一字体
注册表、CSFN v1 容器、设备字体库、bridge 字体库），但**传输与模板→资产解析还没做**，且
**固件在最后一次修复后未重新编译验证**。本轮把它收尾。

先读这些（权威，不要凭记忆）：

1. `project-workflow/generic-display-platform-implementation/task-7-font-assets.md` —— 上一轮的
   完整交接：做了什么、修了哪些既有破损、**§5a 未完成项**、**§6 剩余工作**、**§8 环境事实**。
2. `docs/font-asset-format.md` —— CSFN v1 容器合同 + 实现状态表 + 两端上限必须一致。
3. `PROGRESS.md` 最上面一节、`project-workflow/generic-display-platform-implementation/status.md`
   最上面一节（2026-09-23「Font assets as engine data」）。
4. 代码：`src/template_engine.{h,cpp}`、`src/font_asset.{h,cpp}`、`src/font_store.{h,cpp}`、
   `src/main.cpp`（时钟快路径、`/status.json`、v2 端点）、`bridge/crates/core/src/platform/fonts.rs`、
   `bridge/crates/core/src/platform/{service,model}.rs`、`bridge/crates/core/src/template.rs`、
   `bridge/crates/render/{build.rs,src/ffi.cpp,src/lib.rs}`。
5. `AGENTS.md`（不变量）与 `project-workflow/generic-display-platform-implementation/prompt-font-assets-and-aa.md`
   （原始任务；其中**工作流 B 抗锯齿已被用户取消**，不要再做）。

## 用户已定的方向（不要重新讨论）

- **字体与排版是数据，不是固件**：改字重/字号只换资产，不重新刷固件。这是本任务存在的理由。
- 正文 = Noto Sans **Thin 100 @18**；大文字 = Noto Sans **Regular 400 @64**；**字号为暂定值**，
  排版与字重定稿排在引擎之后。
- **抗锯齿（2bpp/gray4）停止**。`gray4` 相关既有代码保留但不再推进。
- 小屏字族 `f8–f24`、200×200 渲染结果、legacy 通道、≤3 槽规则一律不动。
- 发布仍是用户显式动作；`profile_save_v2` / 保存模板只落盘。

## 交接的确切状态（先核对，再动手）

已完成且**宿主测试通过**：

- 单一字体注册表 `TPL_FONTS`（`src/template_engine.cpp`，9 项，**只允许追加**，因为 `CtOp.font`
  是持久化索引）：`f8 f12 f16 f20 f24 nt16 nt30 ntthin18 ntreg64`。Rust 侧
  `template.rs::FONTS` 同名单。时钟快路径走 `tplFontClockBox`/`tplFontDrawClock`。
- CSFN v1 容器由 `tools/note4-fonts/rasterize_ttf.py` 同时产出引擎头与 `.bin`；
  已入库资产 `bridge/assets/fonts/font_ntthin18.bin`（id `18c2e4ed`）、
  `font_ntreg64.bin`（id `4dc3b226`）。
- 设备解析器 `src/font_asset.{h,cpp}`；设备字体库 `src/font_store.{h,cpp}`（`/fonts/<profile>/<id>.bin`，
  tmp→复检→rename、CRC、name==id、上限 8 个/48 KiB 每资产、prune）；
  bridge 字体库 `bridge/crates/core/src/platform/fonts.rs`（内容寻址、去重、`FontPlan::for_names`、
  `diff_inventory`、上限 8 个/384 KiB）。
- 已修的既有破损：`rgnSetPanel` 在匿名 namespace 里（宿主 FFI 链接失败）、`tests/compiled.rs`
  旧 1 参调用、`rgn_build` 未按画布 `set_panel`、`RGN_MAX` 32 < 400×300 模板的 38 元素、
  `Rgn::area` uint16 溢出、`build.rs` 未跟踪字体头文件、宿主 LittleFS shim 无目录语义。

**第一步（阻塞项）**：`src/font_store.cpp` 曾因设备 `File::name()` 返回 `const char*` 而编译失败，
已修复（shim 的 `name()` 改为返回 `const char*`，源码改用 `String(f.name())`），宿主 5 项测试
已复通过，但**固件自修复后没有重新编译**。先跑：

```powershell
pio run -e esp32-s3-epaper-154g      # 需要 danger-full-access；勿中断包安装
```

若 `checkprogsize` 失败，先怀疑编译进去的字体表：`nt16`+`nt30`（LVGL 裁切，blob 1598+4051 B）
已被 `ntthin18`+`ntreg64` 取代，`codex-status-a` 也不再引用，删掉可回收约 7 KB；但**先确认
没有已发布模板引用它们**。

## 工作流 A：模板 → 资产解析（ABI）

`CtOp` 目前只用编译期索引选字体，资产字体无法寻址。要求：

1. `CtTemplate` 增加 `fontRefs[]`（`{name[16], font_id[9]}`，上限如 8）与 `fontRefCount`；
   `CtOp` 增加 `fontRef`（`CT_NONE_IDX` 表示“用编译期索引 `op.font`”）。**`op.font` 语义不得
   改变**，否则已持久化的编译模板会画错；因此 `CT_ABI` 必须 +1，并在 `tplValidateCt`/反序列化
   处按新 ABI 校验。
2. 编译期（`tplCompile`）：模板里写字体名（`"font": "ntthin18"`）。名字先在注册表里找；
   不在注册表则需要该 Profile 的字体集合提供 —— 编译发生在 bridge 上，设备侧只做**解析**。
   bridge 侧编译产物里填 `fontRefs[].name` 与解析到的 `font_id`（来自字体库）；名字既不在
   注册表也不在字体库 → **整份模板拒绝**（`ct_font` 语义，不半渲染）。
3. 激活期（设备）：对每个 `fontRef` 用 `fontStoreLoad(ref.font_id, ...)` 取容器并
   `fontAssetView` 得到 `Note4PropFont`；**任一 ref 解析失败 → 整份模板/上下文拒绝**，保留旧
   状态，绝不画一半。渲染路径（`drawCtOp`、`propTextWidth`、区域推导的 `tplFontCell*`、
   时钟快路径）都要能接受“来自资产的 `Note4PropFont`”，而不是只认 `TPL_FONTS`。
4. 字体数据不可放进 `CtTemplate`（CTP1 记录）本体；`CtTemplate` 只带 id。

## 工作流 B：增量传输（单个 Profile）

沿用现有 seq/ACK 与 token 约束（`/update`、`/doUpdate`、`POST /claim` 必须带 token；不得新增
隐式续租）：

1. 设备状态上报该 Profile 已安装字体清单 `font_inventory`：`[{font_id, blob_bytes,
   pixel_format}]` + 坏文件计数（`fontStoreInventory` 已有 `badOut`），放进 `/status.json`，
   bridge 侧解析进 `ObservedState`。
2. 同步开始时 bridge 用 `diff_inventory(required, installed)` 取**差集**，**只推缺失项**；
   已装的一律不发。UI/MCP 的发布预览要说明：本次推送哪些字体、共多少字节、多少已存在被跳过。
3. 每项：`font_begin{font_id,len,crc32}` → `font_chunk{offset,payload}`（沿用 MTU/分片规则，
   BLE 单次不得超 MTU）→ `font_commit{font_id,crc32}` → 设备校验 CRC 后 ACK。
   **重复推送同一 `font_id` 必须是幂等无操作、传输字节为 0**（设备库已保证，验收要证明）。
4. Profile 字体集合替换后删除该 Profile 目录下不再需要的文件：**调用 `fontStorePrune(keep)`**
   （已实现，当前无人调用）。
5. 失败与边界：磁盘不足、传输中断、CRC 不符、`bpp/pixel_format` 与目标不符、超上限（每 Profile
   8 个 / 384 KiB）→ 全部显式报错并保留旧状态；不得出现“半个字体被当成有效字体”。
6. 桥侧与 UI：`platform_publish` 预览加字体报告；新增只读 MCP 工具列出字体库、Profile→字体
   依赖、设备已装清单；UI 显示字体占用（字节/个数）。

## 验证（给出精确命令与结果，不要把预览当实机验证）

- `cd bridge; $env:CARGO_TARGET_DIR='D:\Documents\PlatformIO\Projects\codex_status\bridge\artifacts\cargo-target-fonts'; cargo test -p bridge-core -p bridge-render -p bridge-mcp`
  （上一轮基线 **110 passed / 0 failed**；`bridge/target` 被运行中的 `bridge-app.exe` 占用，
  必须用隔离 target 目录），新增解析/传输测试后必须仍然全绿。
- `cargo run -q -p bridge-render -- --template <400x300 tpl> --usage <usage> --compare-compiled --regions`：
  `diff pixels: 0`、往返 0、两条路径区域数一致。
- 传输：同一 `font_id` 连推两次，第二次传输字节为 0；写入中途掉电/截断后重启，旧字体仍可用
  （设备库的宿主测试已覆盖这两条，协议层要再覆盖一次）。
- `pio run -e esp32-s3-epaper-154g` 通过，并在 `PROGRESS.md` 记录 ROM 路径与 SHA256。
- 体积审计：各字体 blob 字节、单 Profile 依赖集合总量、LittleFS（及后续 `assets` 分区）占用与余量。

## 非目标

- 不做抗锯齿/2bpp 灰度；不改小屏字族与 200×200 渲染；不动 legacy ≤3 槽；
  不自动发布/推送；不把 8 项 Profile 静默裁剪成 3 项；不做 CJK。
- 本轮**不追求排版定稿**：`codex-status-a` 已按暂定字号重生（正文 `ntthin18`、大数字
  `ntreg64`，大数字区域高度 = 一行 88 px，见 `concepts-400x300/make-template.py` 注释），
  排版微调与字重定稿留到引擎就绪之后。

## 需要先回答的问题（动手前给结论）

1. `fontRefs[]` 的上限取多少（模板内不同资产字体数）？超出时报错还是拒绝编译？
2. 字体同步挂在哪条通道、哪个端点：复用 bundle 的 seq/ACK 与 `bundle_*` 命名，还是独立
   `font_*` 消息？两条通道（Wi-Fi HTTP / BLE）如何共用同一份设备端状态机？
3. Profile 切换时字体目录如何处理：立即 `prune` 到新集合，还是保留旧集合直到新集合传输完成
   （回滚安全 vs 磁盘占用）？
4. `nt16`/`nt30` 是否从固件里删除（回收约 7 KB）？删除后对已发布/已持久化模板的影响与迁移路径？
5. 资产字体是否也要走 `assets` 分区（16 MB Note4 板，4 MB）？LittleFS 过渡期的容量上限与
   未来迁移策略。
6. `crop_lvgl_font.py`（`nt16`/`nt30` 的来源）目前没有容器产出能力：是补上并让它们也进 bridge
   字体库，还是就此冻结只留编译进固件的版本？

## 工作方式

- 先计划再动代码；多步工作写进 `project-workflow/generic-display-platform-implementation/`。
- 改模板协议/字体协议必须同时改固件、`bridge/crates/core`（Rust canonical）与 Python 测试桥，
  保证三方哈希一致；未识别的 `type/font/bind` 整份拒绝。
- 每完成一个里程碑更新 `PROGRESS.md` 与 `status.md`（现场、证据、待办）。
- 提交信息英文祈使句；**未经用户要求不要提交**。
- **不要**在包安装/构建过程中杀 `pio`/`python`（会损坏 `~/.platformio/packages`）；
  后台任务必须立即返回并把日志重定向到可写目录（见 task-7 §8 的沙箱事实）。
