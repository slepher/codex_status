# Prompt：为 Note4 400×300 1bpp 墨水屏选字体

> 用途：整段贴给联网搜索 / ChatGPT / 字体站客服式检索。目标不是"找一个好看的字体"，而是**在 1 位黑白、无抗锯齿**的前提下，找到阈值之后仍然细而不糊、和原厂 ROM 画面同气质的大字号显示字体。

---

## 一、背景（贴过去）

我要给一台自制固件的墨水屏设备选显示字体：

- 面板：4.2 英寸 **400×300**、**1bpp 黑白**（SSD2683 控制器），约 **119 DPI**，横屏。
- 渲染：自己写的位图引擎，**没有抗锯齿**。字形最终按阈值二值化成 1 位（纯黑/纯白），所以唯一的评价标准是：**二值化之后笔画不断、粗细均匀、间距不挤、小字号不糊**。
- 只做英文/数字（ASCII 0x20–0x7E）与小字号正文；中文不在本次范围。
- 字体要嵌进固件并随固件分发，许可必须是可再分发的（SIL OFL / Apache-2.0 / 公有领域）。
- 交付物：**TTF/OTF 静态权重**（或可变字体），我会用 `lv_font_conv` 自己生成任意字号的 1bpp 位图表。

## 二、参考图（原厂 ROM 的实拍画面）特征

- 主视觉是**大号时钟**（`13:18`），数字高约 **100 px 上下**，笔画**极细**（大约 2–3 px 细线），字身高挑、内白（counter）开阔、留白非常大；
- 左右两侧有约 18–22 px 的小号正文/日期，同样偏细；
- 整个画面**只有细线条，几乎没有实心块**——这正是 1bpp 能干净呈现的原因，也是我要的味道；
- 没有衬线、没有装饰、没有几何夸张；偏"现代无衬线 · 特细体"。
- 我的判断：这接近 **Noto Sans ExtraLight / Thin** 一类权重（而不是 Regular）。请一并验证或纠正这个判断。

## 三、请给我 5–10 个候选，并排序

筛选条件（重要性从高到低）：

1. **气质接近参考图**：现代无衬线、Thin / ExtraLight / Light 权重、数字高挑、字怀开阔；
2. **大字号可用**：在 60–120 px 仍保持稳定细笔画（有独立静态 Light/Thin 权重，或可变字体的 wght 轴覆盖到 100–300）；
3. **数字细节**：有 **tabular / lining figures**（等宽数字，时钟跳秒不抖）；
4. **1bpp 友好**：笔画粗细（stem width）明确，说明它在二值化后是否会断线、糊字、笔画粘连；若有为 **e-ink / 1bpp 位图渲染 / hinting** 专门优化的字体（含像素字体），优先列出；
5. **许可可嵌入固件**：SIL OFL / Apache-2.0 / 公有领域，给官方下载链接（Google Fonts、GitHub 官方仓库优先）；
6. 说明**推荐字号**与**是否需要 condensed/窄体**（400×300 横屏、时钟要占满宽度）。

## 四、回答格式（请照这个表给）

| 字体名 | 推荐权重 | 许可 | 官方下载链接 | 适合字号 | 笔画粗细（px@推荐字号） | 1bpp 阈值后表现判断 | 与参考图接近度(1–5) | 备注 |

最后给出：

- **首选 1 个 + 备选 2 个**，并说明为什么；
- 时钟数字是否值得**单独用一个 display 字体**（只取 `0-9` 和 `:`），正文另用一个；
- 窄体（condensed）与常规宽度哪个更适合 400×300 横屏，给出建议；
- 如果认同"参考图≈Noto Sans ExtraLight"，请给**直接可下载的 Noto Sans / Noto Sans Display ExtraLight + Thin 静态权重**链接（Google Fonts 或 notofonts GitHub）。

## 五、附：请一并评估这些已知候选

Noto Sans / Noto Sans Display（Thin、ExtraLight）、Roboto（Thin、Light）、Inter（Thin、ExtraLight）、IBM Plex Sans（Thin、ExtraLight）、Source Sans 3（Light）、Barlow / Barlow SemiCondensed（Thin、Light）、Archivo Narrow（Light、Thin）、Oswald（Light）、Saira Condensed（Light）、Big Shoulders Display（Thin、Light）、Rajdhani（Light）、Jost（Light）、Manrope（ExtraLight）、Public Sans（Thin）、Geist（Thin）、Chivo / Archivo（Thin）、DIN 风格开源替代（D-DIN、Inter Display）、以及任何**专为墨水屏/低分辨率 1bpp 优化**的字体。

---

## English version（贴给英文站点/搜索更有效）

> I need display fonts for a 4.2", 400×300, **1 bpp monochrome** e-paper panel (~119 DPI) driven by a custom bitmap renderer with **no anti-aliasing** — every glyph is thresholded to pure black/white, so the only criteria are: strokes must not break, weights must stay even, and spacing must not clog after binarization.
>
> Reference look (photo of the stock ROM UI): a very large clock (~100 px digits) in an **ultra-thin / extra-light** modern sans, tall digits, wide counters, hairlines around 2–3 px, generous white space, no serif, no decoration. Small 18–22 px text in the same light weight.
>
> Please shortlist 5–10 **TTF/OTF static weights** (or variable fonts) that:
> 1. match that ultra-light grotesque look,
> 2. stay clean at large display sizes (60–120 px) and include true Thin/ExtraLight weights,
> 3. offer **tabular lining figures**,
> 4. are known to survive 1 bpp thresholding (mention stem width and any e-ink/pixel-font alternatives),
> 5. are licensed for embedding and redistribution (SIL OFL / Apache-2.0 / public domain) with official download links.
>
> Answer as a table: font | recommended weight | license | official link | best size | stem width at that size | 1 bpp verdict | closeness to reference (1–5) | notes. Then give one first choice plus two alternates, say whether the clock should use a dedicated display font while body text uses another, and whether condensed or normal width suits a 400×300 landscape panel.

---

## 六、选到之后怎么用（我们自己这侧已就绪）

1. `npm i lv_font_conv`（上游 xiaozhi 组件就是用 Node 的 `lv_font_conv` 把 TTF 生成 LVGL C 字库的），例如：
   `npx lv_font_conv --font NotoSans-ExtraLight.ttf --size 96 --bpp 1 --format lvgl --range 0x20-0x7E -o font_nt96.c`
2. 用本仓库 `tools/note4-fonts/crop_lvgl_font.py` 把它裁成引擎格式（ASCII、1bpp、每字形 6 字节描述符）：
   `python tools/note4-fonts/crop_lvgl_font.py emit <font_nt96.c> nt96 --out src`
3. 在 `src/font_noto.h` 里挂上新的 `Note4PropFont`，模板里就能用 `"font": "nt96"`。

所以**只要给我 TTF/OTF 静态权重即可**，字号和图位深我们自己生成。当前大屏字族是 `nt16`/`nt30`（来自上游 `16_4`/`30_4`，即 **Regular** 权重）——这正是它看起来"平、没体现能力"的原因；参考图那种细笔画需要 **ExtraLight/Thin** 权重 + 更大的字号。
