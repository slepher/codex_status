# 状态

2026-10-11 23:07（香港时间）：本轮源码、双ROM候选和本地软件验证完成；未提交、未推tag、未启动CI、未重启Bridge、未访问/刷写设备。部署仍为PROGRESS原记录的.83/v5。

## 实现与证据

- 发布工作流保留Note4/1.54串行构建，Windows x64和macOS arm64/x64原生任务；手动只产物，标签且全部成功才draft Release。ROM从真实marker识别版本/target，核对双OTA槽容量，输出bin/清单/USB配套zip（bootloader、分区表、boot_app0、应用、匹配ELF、CSV及哈希）。去掉不存在的profiles.seed.json依赖。
- 中文走CSFN v2稀疏码点表，保留v1；有界UTF-8度量/绘制/缺字回退共用C++。静态text/prefix/suffix超字节容量明拒，不截断汉字。Profile保存/预览不要求先升级设备；真正发布时要求bundle_font_protocol=2，旧设备不创建任务。旧CN源码保留，两份ROM的Font12CN/Font24CN符号均不存在。
- Base64预览改用已有base64库；无新运行依赖。
- [安装指南](../../docs/build-install.md)提供clone后编译当前OS Bridge并安装指定目录、USB身份核对/备份/分区/刷机/配网/认证验收的可复制Agent提示词。

证据均在 `artifacts/release-fonts-20261011/`（不入库）：

| 验证 | 结果/文件 |
|---|---|
| core/render/app完整回归 | 309 passed / 3 ignored，20 suites；`regression.log` |
| 最后字体错误文案/发布预览修订 | 字体18项+能力预检1项通过；`font-final.log` |
| 生成器与ROM阶段校验 | `python -B tools/test-unicode-font.py` / `test-release-rom.py`通过；YAML矩阵和Python语法通过；Git Bash `-n`通过 |
| CSFN v2双解析器 | 空/单项拒绝，95/98项、4088零位图描述符接受，4089字节越界/4097计数越界拒绝；无序/重复/surrogate/超范围码点与截断拒绝 |
| 中文语义 | 三/四字节、动态bridge.label、宽度、缺字、非法UTF-8、编译模板像素一致、Bundle安装与重启恢复通过 |
| 实际字库预览 | 本机msyh.ttc仅用于私有试验，107 glyph/3685 B/ID6c0dac40；`chinese-preview.png`已目视可读，`preview.log` JSON/CT/序列化恢复像素差0。不可将该私有字体产物当可再分发资产 |
| 最终ROM | 两目标构建成功；各64项Wi-Fi栈门禁通过，stage清单及zip逐文件哈希验证通过 |
| 字体资源 | `font-resource-audit.json`：两板fontAssetValidate局部帧64 B、fontAssetView32 B、fontGlyph32 B、propTextWidth48 B、drawPropText112 B；不是完整动态调用链/整机峰值 |

## 候选产物（未部署）

| env / version | ROM大小 / SHA256 | RAM / Flash |
|---|---|---|
| zectrix-note4-b / 0.18.84-note4 | 1,838,880 B / `7a2e009a512111660a1e51168bcf880075c554be664f7b5682d7c12037847b94` | 118996 / 1787111 B |
| esp32-s3-epaper-154g / 0.18.36-bw | 1,820,816 B / `285fc9051e855de9ff9b05ab48473cc015ef140879ca84909e28c76f5d3b31b6` | 103912 / 1769904 B |

最终保留ROM分别为上述证据目录下 `codex-status-0.18.84-note4.bin` / `codex-status-0.18.36-bw.bin`；同名json含env、目标、ROM及ELF完整hash、两OTA槽尺寸与USB文件hash；同名`-usb.zip`包含匹配ELF。Note4 ELF `8d8a409b93f6dfc938fa43626c80be7dcc7749ae9b3adf4b3e0a98f005569553`；1.54 ELF `d8a8c8b43d97fabb8d5f6f9d5d3de427fc6f1c8eaf2d358751a55578f83fe25e`。最终隔离目标恢复Note4。

Note4相对已部署.83记录：静态RAM +80 B、Flash +960 B；不声称中文字库排除带来同等ROM缩减。字体资产持久/加载量由实际N和blob决定，无完整CJK预分配；单份v2最多48KiB。最小95项（空/单项明拒）、夹具98项1467 B、真实107项3685 B、4088空字形上限49148 B均有边界依据；实际中文字形上限明显更低。安装时还存在JSON/base64临时峰值，未测为未知。

## 尚未完成

唯一待办归backlog C5/C6：中文RF/EPD/典型和上限动态资源/掉电实机矩阵；1.54仍报告字体准入0；1.54最新ROM的认证运行/OTA/USB/配网发布闭环；GitHub矩阵首次实跑、macOS两架构真实安装/Developer ID签名与公证、干净Windows首次安装。软件构建通过不关闭这些缺口。
