# CSFN 字体资产格式（v1 ASCII / v2 Unicode 子集）

2026-10-11：v2 源码候选。当前设备部署看 PROGRESS；中文实机未验，缺口归 backlog C5。

字体是数据，同一渲染器支持不同字体/字号/字重。中文不需要逐字体驱动，也不把中文字库编入 ROM。旧 `Paint_DrawString_CN` 与字库源码保留供参考，`font12CN.cpp/font24CN.cpp` 从固件构建排除；正式中文路径是 CSFN v2 + 当前完整 Bundle + 共用模板引擎。

权威实现：生成器 `tools/note4-fonts/rasterize_ttf.py`；Bridge `bridge/crates/core/src/platform/fonts.rs`；固件/宿主 `src/font_asset.cpp`、`src/template_engine.cpp`。三侧合同一致，未知版本明拒。

## 固定头与 payload

小端，固定 64 B 头；CRC32/IEEE。

| Offset | Type | Meaning |
|---|---|---|
| 0 | u32 | magic `CSFN` / `0x4E465343` |
| 4 | u16 | version 1 或 2 |
| 6 | u16 | headerBytes = 64 |
| 8 | u32 | 整份文件长度 |
| 12 | u32 | `[64, fileBytes)` 的 CRC32 |
| 16 | u32 | blobBytes |
| 20 | u32 | glyphCount；v1 固定 95；v2 为实际数量 95–4096，另受文件字节上限限制 |
| 24 | u8 | bpp；v2 仅 1 |
| 25 | u8 | pixelFormat：0=bw；1=gray4 保留，不用于 v2 |
| 26,27 | u8,u8 | lineHeight / baseLine；`0 <= baseLine < lineHeight` |
| 28,30,32 | u16 ×3 | maxAdv（1/16 px）、sizePx、weight |
| 34,36,38 | u16 ×3 | nameLen、familyLen、coverageLen |
| 40 | u16 | filledGlyphs <= glyphCount |
| 42 | u8 | hint：0 native / 1 auto / 2 none |
| 43–63 | bytes | 保留，生成器写 0 |

Payload 顺序：`name | family | coverage | [v2 codepoints] | glyphs | blob`。

- 三段名称元数据仍为可打印 ASCII，长度分别 `<24/<32/<16`；name/coverage 非空。每段补零到偶数字节，padding 不计入头的长度。
- v1：`coverage=ascii/digits`，95 个 glyph 直接对应 `U+0020..U+007E`。
- v2：`coverage=unicode`，先放 `glyphCount` 个 LE u32 码点，再放同数量描述符。码点严格递增、无重复、必须是 Unicode scalar；禁止 surrogate、超 `U+10FFFF`、C0/C1 控制字符。前 95 项必须是完整 ASCII 序列，保留时钟/缺字索引。之后是调用者所需的稀疏子集，支持三字节中文和四字节字符。u32 表按字节读取，不要求 4 B 对齐。
- glyph 描述符固定 8 B：`u16 off, u16 adv, u8 w, u8 h, i8 ox, i8 oy`。描述符表 2 B 对齐；位图按行补齐整字节、MSB 在前，1 表示墨色。有效字形 box <=127 px，所有位图范围须在 blob 内。
- 文件必须恰好结束在 blob 末尾，无额外尾部。设备先校验长度、数量、CRC、码点、字形范围，才绑定视图。字体视图指向拥有者持有的资产字节，不复制码点/字形表。
- `font_id=crc32(whole file)` 的八位小写十六进制；改变元数据/字形/码点即改变 ID。

## Bundle、模板与兼容

Bridge 字体库按 ID 存储；Profile 的 `font_ids` 将已有比例字体槽（如 `ntthin18`）绑定到明确资产 ID。保存/导入不发布。显式发布冻结完整 Bundle，把选中的 CSFN 字节放入 `fonts[]`；设备单份内容文件持有 Bundle，安装/重启时加载绑定。无独立 font-directory 推送、无当前/回退双 manifest，原生固件 OTA 双槽不变。

`bundle_font_protocol=1` 只接受 v1；`=2` 接受 v1/v2。Bridge 对旧设备的 Unicode 发布在冻结前明确报错，要求升级固件。新 Note4 候选报告 2；1.54 的字体发布准入仍未开放（报告 0），不得声称 1.54 字体实机已支持。

模板仍用现有字体名称/编号，CT ABI 2 不变；字符串缓冲上限仍以 UTF-8 字节计算。绑定 v2 字体时，静态 text、prefix/suffix 和动态绑定文本按有界 UTF-8 解码，宽度/对齐/绘制共享同一个查字逻辑。缺字为一个 `?`；畸形 UTF-8 每个无法解析的字节为 `?`。旧内建/v1 字体保留逐字节 ASCII 净化行为。单纯保留 UTF-8 文本不等于有中文字库，必须选择对应资产。

## 生成与使用

安装 FreeType Python 绑定（现有生成器支持本机 `freetype-py` 或 `vendor/py`）；准备自己有权使用的 TTF/OTF/TTC 与 UTF-8 字符清单。生成器要求源字体包含全部 ASCII 和请求字符，缺字明确失败，不用 `.notdef` 冒充。

```text
python tools/note4-fonts/rasterize_ttf.py asset-unicode <font.ttf> 18 ntthin18 --chars <subset.txt> --asset <font.bin>
```

该命令只写 `.bin`，不生成 ROM 头文件；输出目录须已按仓库规则准备。导入 Bridge 字体库 → 在 Profile `font_ids.ntthin18` 选择输出 ID → 中文 text 使用 `font: ntthin18` → 预览 → 显式发布完整 Bundle。不得自动发布或移植另一台设备的绑定。现有 `emit ... --charset ascii|digits` 继续输出 v1。

## 资源预算与验证边界

v2 每个资产 <=48 KiB，glyphCount <=4096；双重边界同时生效，不保证 4096 个实际字形一定装得下。多个字体沿用当前 Bundle 8 项上限和逐项加载；不分配完整 Unicode 仓库。

持久字节为 `64 + padded metadata + 12*N + blobBytes`；运行期拥有一份该资产字节，码点表在其中，不另拷贝。新增每个字体视图为一个指针和计数（ESP32 32位上通常增8 B，由实际构建核对）；查字为 O(log N)，UTF-8 解码固定局部标量、无递归/额外堆。Bundle 仍需 JSON/base64 解码临时开销，不能把资产字节当整机峰值。分配失败沿用 `font_oom` 明拒。

空/单 glyph 文件因少于95项拒绝；最小 v2 为95项 ASCII；测试夹具98项（含中/文/四字节字符）为1467 B。字节上限和4096项超限拒绝另有测试；完整字体/多模板/典型与上限安装的真实峰值RAM、栈、刷新视觉和掉电恢复归 C5 实机缺口。当前候选宿主测试不能替代这些实机证据。
