# note4-fonts — LVGL 字体裁切工具

从 xiaozhi/Noto 组件（`artifacts/xiaozhi-fonts-2.0.0-component.zip`）里取出 Note4 ROM 用的
Noto Sans 字体，裁成 Codex Status 引擎能用的 1bpp ASCII 表。

## 上游字体格式（LVGL `fmt_txt`）

生成的 `font_*.c` 由三部分组成，裁切时三条都要用：

1. `cmaps[]`：`cmaps[0]` 是 `range_start = 32, range_length = 95, glyph_id_start = 1,
   type = FORMAT0_TINY`，即 ASCII 0x20–0x7E → glyph id = 1 + (cp - 32)。
2. `glyph_dsc[]`：`{.bitmap_index, .adv_w, .box_w, .box_h, .ofs_x, .ofs_y}`。
   `adv_w` 是 1/16 px 单位的步进；`bitmap_index` 是**字节**偏移。
3. `glyph_bitmap[]`：单一 blob。**每个字形连续打包，行间不补字节**，长度 =
   `ceil(box_w * box_h * bpp / 8)`。本工具用全部 891 条描述符验证过该模型
   （`report` 最后一列 `model` 全为 `ok`）；按“每行补齐到字节”解会在奇数宽度字形上逐行错位。
   另外生成器会把小值写成 `0x0`（1 位十六进制），解析必须接受 1–2 位。

已确认的坑：4bpp 直接 `>= 8` 阈值化即可；上表的 `14_1` 是 1bpp 原生（ExtraLight 变体，笔画偏细），
其 alpha 只有 0/1，阈值必须用 1，否则整张图为空。

## 用法

```powershell
# 先从组件 zip 解出需要的 .c（artifacts/ 已 gitignore，解到 TEMP 即可）
python tools/note4-fonts/crop_lvgl_font.py report  <font_a.c> <font_b.c> ...
python tools/note4-fonts/crop_lvgl_font.py emit    <font.c> nt16 --out <dir>
python tools/note4-fonts/crop_lvgl_font.py specimen <font.c> "94 25 100%" out.png --scale 3
```

`emit` 产出一个 1bpp blob + 每字形 6 字节描述符（byte_offset, adv_w, box_w, box_h, ofs_x, ofs_y）的
C 头文件，正是引擎需要的比例字体数据。

## ASCII 裁切后的体积（95 字形）

| profile | 源字形 | 完整 blob | ASCII 源字节 | 裁成 1bpp | +描述符 |
|---|---|---|---|---|---|
| `noto_sans_basic_14_1` | 891 | 10.8 KB | 648 B（本身 1bpp） | 648 B | 1.2 KB |
| `noto_sans_basic_16_4` | 891 | 67.0 KB | 4.2 KB | 1,598 B | 2.2 KB |
| `noto_sans_basic_20_4` | 891 | 102.2 KB | 6.0 KB | 2,109 B | 2.7 KB |
| `noto_sans_basic_30_4` | 891 | 223.2 KB | 13.5 KB | 4,051 B | 4.6 KB |

对比现有固件定宽字模（`src/font*.cpp` 的 C 数组）：font16 = 3.0 KB、font24 = 6.8 KB。
即“换成 Noto”并不会更占 flash：16 px + 30 px 两档约 6.8 KB，四档全上约 10.7 KB。
上游那个 2.6 MB 的 `cbin` 是 common 字符集（含 CJK）才需要的，只做 ASCII 界面用不到。

## 与引擎的差距（替换字体前必须解决）

现有 `sFONT` 是**定宽 1bpp 表**（`table + Width + Height`，索引 `c - ' '`），
Noto 是**比例字体**（每字形 adv/box/ofs 都不同）。因此需要：

- 新增一个比例字体描述符 + 对应绘制路径（按 `adv_w` 步进，按 `box/ofs` 落笔），
  或把 Noto 字形按固定 cell 重排（会丢掉比例间距，数字仍可接受，正文会变丑）。
- 现有 `f8/f12/f16/f20/f24` 保持不变，新字体用新名字（如 `nt14/nt16/nt20/nt30`），
  这样 200×200 模板的渲染结果不受影响；`CT_FONTS` 与 `tplValidateCt` 的 `op.font > 4` 上界要同步放宽。
