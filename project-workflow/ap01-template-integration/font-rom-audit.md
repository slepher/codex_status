# AP01 0041 ROM字体核查（2026-10-20）

## Plan

只读原厂0041归档，确认SHA256后检查原生LVGL字体回调、描述符与全部cmap类型；扫描整个文件寻找遗漏字体，确认Y与完整英文字库是否存在。不给设备发请求，不修改设备、原件、Bridge或当前预览。

## Task

- 对照现有55套字体清单，检查提取器字符映射语义。
- ROM全文件结构扫描与已知回调反向引用交叉验证。
- 对Y及A–Z/a–z/0–9验证实际gid、字形范围、非空位图及覆盖；导出本地字形证据。
- 记录肯定发现、否定搜索边界和剩余未知，修正之前过宽结论。

## Status

已完成。原件现路径为旁仓库`roms/0041/ap01-1.0.2_0041.bin`，旧报告的artifacts路径已迁移；6769704 B，SHA256 `972db4c136c7ed9e24a83c07c1a7fd62040ca018b08ca285216d26b1fee3c6b9`，只读且复核一致。

**结论：页面子集确有裁剪，但之前恢复清单遗漏了ROM中的完整ASCII字库。** 不能再写“原厂ROM没有Y”。

| 项目 | 已验证结果 |
|---|---|
| 新找到的完整英文字体 | LVGL **Montserrat 14**，wrapper `0xa0262f60`（文件偏移`0x263f60`），descriptor `0xa025fad8` |
| 覆盖 | ASCII U+0020–U+007E共95字符，含A–Z/a–z/0–9；另62扩展/图标字形，共157映射字形 |
| 原始规格 | 标称14px，行高16、baseline3，4bpp未压缩；带完整class kerning |
| Y | gid58，11×10，advance145/16=9.0625px，x=-1/y=0；位图VA`0xa02603c1`、文件偏移`0x2613c1`、55B |
| Y位图SHA256 | `b4b5ef35269899ad3d7f4a970869ee3d33db29f448f346b8c45b363bf1d7332b` |
| 之前55项为何缺Y | 固定手选清单未含完整字库；ascii17`0xa02549ac`与clock21`0xa02549f4`的Y确为FORMAT0_FULL offset0→gid1→空格（0×0），并非Y位图解码失败 |

完整ROM逐字节搜索既有原生fmt_txt回调对`0xa00e4864`/`0xa00e45aa`，识别67个有效字体描述符；原清单55项全部找到，遗漏12项，其中包含上述完整ASCII字库。这里只声称这个回调家族的覆盖，不排除其他字体格式/自定义回调。找到完整字体已足以排除“ROM无Y”。

原恢复器还存在范围限制：只实现cmap0/2/3，明确拒绝SPARSE_FULL=1；仅接受≤4cmap、4bpp、未压缩。FORMAT0_FULL零offset在原生LVGL中仍是合法gid别名，原恢复器将多数零offset跳过；本次扫描忠实保留别名，并检查位图非空来判断可见字符。**本次不修改旁仓库恢复器，也不更新已有布局预览**。

## Review

独立核查与可复现脚本均完成。与[官方LVGL v8.3.11 Montserrat14](https://raw.githubusercontent.com/lvgl/lvgl/v8.3.11/src/font/lv_font_montserrat_14.c)比对：158条字形度量（含gid0）、8832B位图、2份cmap/62稀疏项、158B左右class mapping及61×49=2989B kern values全部一致；因此是整套字体身份匹配，不是仅凭Y外形猜测。官方源SHA256 `cb0e6923dd76cceecdb655135f0d2f85e5a978571226149640bf30135bce8d28`。

主工具再与本地LVGL9.2.1的同名字体作全部字形、字距表及映射核对，也全部一致。该C源SHA256 `140f65aa4327f17d587bb51b63e3f731523e4d9a24e40a9219d21ca7c84f0f11`；源码包装版本不同但字形数据相同。位图SHA256 `a44d3911b8ce8ac37033390b2fc4c1469cf10b87b15afc06ba65144ab358c4df`。

REA使用明确RV32 raw映射base`0x9ffff000`/entry`0xa0000000`读取交叉确认：

- wrapper：`ev_6089b8cca8fbb3a6d386194a3a65a0aa20435ba3152f58987eab69671a1f846a`
- Y位图：`ev_f6a75d5592d30342eafe5e4d9f0e778d280bb874f8edc476ee418804b75ec838`
- descriptor：`ev_b556a9fba30198916e942a3aa06fe4dd82e116d7aa44a80d1f5318107556d90b`
- cmaps：`ev_b0d48e19de0654a45782b5f53548c0dee78d00b7c7e82dc696c084ca24113da9`
- wrapper引用：`ev_1625534085ecd80eae31dfc17456de0541658d153489913d85d525d93b91dcc6`（三处引用，仅静态观察，不推断运行默认字体）。

证据在[本地目录](../../artifacts/ap01-font-audit-20261020/)：`font-audit.json`、`reference-comparison.json`、5份REA记录、Y原始位图和`complete-english-original.png`/`complete-english-3x.png`。字形表来自ROM原始像素、未使用替换字体绘制正文，已目视确认含完整字母和SYNC。

复现：提权预建out目录，在WSL执行`python3 -B tools/audit-ap01-rom-fonts.py --rom <旁仓库>/roms/0041/ap01-1.0.2_0041.bin --extractor <旁仓库>/tooling/scripts/rehost-assets.py --out <预建目录> --specimen --reference-font /home/slepher/nuttx-ap01-template-20261018/apps/graphics/lvgl/lvgl/src/font/lv_font_montserrat_14.c`。脚本检查ROM身份、所有已恢复字体均被扫描覆盖及完整字体逐字节比对；无设备网络请求，不更改原始ROM/Bridge/生产Bundle/已有预览。

边界：完整ASCII不是完整Unicode拉丁文/中文。新找到的是14px原尺寸，不能据此宣称存在20/21px完整英文字库；不无损放大位图、不声称实屏新字号已验。本次只核查和导出证据，后续选字体/调整布局仍需另行实施。
