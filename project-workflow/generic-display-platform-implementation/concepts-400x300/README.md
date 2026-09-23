# Note4 400×300 界面概念

三张候选图是设计稿，均为 400×300、纯黑白、顶部状态栏。共同示例：5h 剩余 94%、周额度剩余 25%、RC 2、Plus、账号 99%、同步时间 15:55。数据仅用于比较版式；未接入模板引擎，也未发布设备。

另有三张 `concept-*-pro-weekly.png`，展示 Pro 账号只有 weekly 桶时的对应布局。它们沿用各自双桶版本的状态栏、字体、分隔线、元数据区与页脚；weekly 补到主信息位，缺失的 5h 数字及重置时间不占位。RC 2 仍作为示例展示，实际无 RC 时隐藏。现有项目显示规则规定“5h 桶缺失时显示静态 100、隐藏其重置时间”。这组三图提出的是不同的产品行为，选定后须先同步修改该显示规则，再实现模板条件布局。

视觉参考：[OpenAI Codex 官网](https://openai.com/codex/)与[Codex 桌面应用介绍](https://openai.com/index/introducing-the-codex-app/)的简洁文字层级、留白和细分隔线。没有复制官网界面或标志图形。状态栏布局也参考了用户提供的 Note4 实物照片。

| 方案 | 图 | 设计重点 | 取舍 |
|---|---|---|---|
| A 双主数值 | `concept-a-editorial.png` | 两个额度占屏幕主体，一眼读数 | 周和 5h 同等醒目，其他状态较小 |
| B 信息列表 | `concept-b-list.png` | 类任务列表的标题、序号、细线和留白 | 文字说明最清楚，数字相对较小 |
| C 仪表行 | `concept-c-meter.png` | 大数字配长条，同时看剩余量和相对比例 | 进度条消耗横向空间，信息密度较高 |

## 已选方案与状态栏规格

2026-09-23：选用 A 双主数值方案；单 weekly 场景沿用 A 的字级、线条和页脚。后续以此为基础制作实际模板，不再继续出概念图。

状态栏从上到下为：上下各 4 px 留白，中间 **28 px 内容区**，合计 **36 px**；底部分隔线另占 1 px，位于 y=36。左右外边距各 12 px。左侧依次为日期 `MM/DD`、时间 `HH:MM`，两者间隔 8 px，均用 `f16`。右侧顺序固定为蓝牙、Wi-Fi、主机、电池图标、电量百分比。四个图标各占 **20×20 px** 图标盒，实际笔画可用约 18×18 px（电池轮廓约 18×12 px）；图标之间留 4 px，电池图标到百分比留 4 px，百分比也用 `f16`。整个右侧组贴近 12 px 右边距；`100%` 的宽度也已检查。图标轮廓按 1bpp 绘制，主要笔画至少 2 px。时间与右侧状态组在 28 px 内容区内垂直居中。

状态栏只承载设备状态，不显示 `CODEX` 标题。主内容区从分隔线下方开始；标题/额度信息按 A 方案排布。状态图标的显示语义及实际字体像素尺寸在模板实现与真机试显时校准。

实现前需补日期绑定：现有 `device.now` 只输出本地 `HH:MM`，没有独立的设备本地日期绑定；不能把示例 `MM/DD` 写死。蓝牙/Wi-Fi 图标可复用当前 16×16 图标体系，电池百分比绑定现有 `device.battery`。

共同原则：白底、少量黑色面积，顶部状态栏固定，左右留 12–18 px 边距。底部保留同步时间与页码。5h 缺失时显示静态 `100%`，不显示重置时间；RC 为 0 或缺失时隐藏该项；账号 label 缺失时隐藏整项。周/月桶缺失、离线和过期状态仍需在选定方案后补对应状态稿。正式模板需以当前引擎支持的字体、图元及绑定重新实现与验证。

使用 Windows PowerShell 运行 `render-concepts.ps1` 可重绘三张设计稿。

## 首版实际模板与预览

`codex-status-a-400x300.json` 为已选 A 方案的 v2 模板源；`status_icons.py` 生成 20×20 图标及放大检查图，`make-template.py` 写出 JSON。为适配当前 CTP1 的单资源长度上限，每个图标拆为左右两个 10×20 位图，最终视觉尺寸仍是 20×20。`bridge-off-icons8-20203-100.png` 来自用户选定的 [Icons8 TV Off 原图](https://img.icons8.com/?size=100&id=20203&format=png&color=000000)，在本项目命名为 **Bridge Off**；`Bridge On` 使用同一显示器轮廓加勾。`icons8-battery-59804-100.png` 来自用户选定的 [Icons8 电池原图](https://img.icons8.com/?size=100&id=59804&format=png&color=000000)，据此外框生成 0/25/50/75/100% 五档图标。模板用同一外框与 `device.battery` 进度条实时填充，右侧数字也来自 `device.battery`。蓝牙和 Wi-Fi 沿用此前准备的同尺寸黑白图形；状态联动待完成。

2026-09-23 二次修订：状态栏文字与电池填充按像素实测重新对齐（见下节），新增离线行，主区 `%` 改为与数字垂直居中。当前版本已通过本机 Bridge MCP 的 `template_validate_v2` / `template_save_v2`，仅保存到 v2 库（`source_crc=2b523381`，`compiled_crc=41c31abd`，`published=false`），未发布、未刷机。

### 状态栏对齐修订（2026-09-23）

用户反馈“电池电量没和外壳居中、文字没有上下居中”，随后选定主区 `%` 与数字垂直居中。实测（`measure-preview.py`，纯标准库 PNG 读数，先跑 `--selftest` 自检）确认两处偏差及成因：

- 共享引擎 `GUI_Paint.cpp` 的 `Paint_DrawPoint()` 对 1×1 点写入 `Xpoint-1, Ypoint-1`，因此所有经线段/矩形绘制的图元（分隔线、条填充、无 region 文本）整体左移上移 1 px；图标走 `Paint_SetPixel` 不受影响。`Paint_DrawRectangle(..., DRAW_FILL_FULL)` 的填充循环是 `Ypoint < Yend`，填充高度比声明少 1 px。两者都是随仓库带入的供应商行为，**同时作用于固件与宿主**，为遵守“保留 200×200 原有行为”的约束，本次不改引擎，只在 400×300 模板内补偿。
- 修订前后的实测（黑像素 ink bbox 中心线与图标中心线 17.5/18.0 对比）：日期 14.0 → 18.0、时间 13.5 → 17.5、电量百分比 15.5 → 17.5；电池填充由“外壳内腔 10 行中偏上 5 行（14..18）”改为对称 6 行（15..20），填充宽度 12 → 14 px，100% 时正好顶到内腔右缘。
- 主区 `%`：原按基线落在数字右下（ink 中心 154.5，数字中心 132.5），现将三个 `%` 元素（5h、双桶 weekly、单桶 weekly）的 `y` 由 149 改为 127，实测 ink 中心与数字中心同为 132.5。

### 边界场景预览

全部为宿主引擎（固件同源 C++）渲染的 400×300 纯黑白 PNG，`colors=2` 已逐张校验：

| 场景 | 文件 | 输入 |
|---|---|---|
| 正常数据 | `mcp-preview.png` | `sample-usage.json`，电池 75%，5h 94% / weekly 25% / RC 2 |
| 仅 weekly（无 5h 桶）+ RC=0 + 无 label | `mcp-weekly-only.png` | `sample-weekly-only.json`；weekly 提升到主位，5h 块与其重置时间不占位，RC 行隐藏 |
| 剩余量 0 边界 | `mcp-bucket-0.png` | 5h 与 weekly `usedPercent=100`（remaining 0） |
| 剩余量 100 边界 | `mcp-bucket-100.png` | 5h 与 weekly `usedPercent=0`（remaining 100），RC 3 |
| 离线/数据缺失 | `mcp-offline-180m.png` | `usage={}` + `offline_mins=180`；数值显示 `--`、页脚 `OFF 180M`，无旧文字残留 |
| 长套餐名/长账号名 | `mcp-longtext.png` | plan `ENTERPRISE ANNUAL PLAN 2026`、label `CORPORATE ACCOUNT SUBSCRIPTION TEAM`；按 region 硬裁剪，不与 RC 重叠 |
| 电池档位 | `mcp-battery-{0,25,50,75,100}.png` | `sample-usage.json`，电池 0/25/50/75/100%，最宽 `100%` 不裁切 |
| 状态栏放大 | `statusbar-zoom.png` | `mcp-preview.png` 的 y 0..39 行 2× 最近邻放大 |

### 布局与字段映射

状态栏（36 px + 1 px 分隔线，左右边距 12 px）：

| 位置 | 元素 | 绑定 / 条件 |
|---|---|---|
| x=12,y=12 | 日期 `MM/DD`，f16 | `device.date`（本地字段，时钟未知时不绘制） |
| x=75,y=12 | 时间 `HH:MM`，f16 | `device.now`（本地） |
| x=248/272/300,y=8 | 蓝牙 / Wi-Fi / 主机（Bridge On）各 10×20×2 | 静态位图，无连接状态绑定 |
| x=320/330,y=8 | 电池外壳 10×20×2 | 静态轮廓 |
| rect 324,16,14,7 | 电量填充条（`max=100`，无边框、无底色） | `device.battery`（本地数值，扩展的数值绑定） |
| region 344,10,44,20 | 电量百分比，f16 | `device.battery` + `%` |
| line y=36 | 状态栏分隔线 | — |

主区与页脚：

| 位置 | 元素 | 绑定 / 条件 |
|---|---|---|
| x=16,y=51 | `Remaining` | 静态 |
| line x=199,y=56..211 | 双栏竖分隔线 | `buckets[codex].5h.remaining` 存在 |
| x=16,y=83 / region 16,103,132,70 / x=132,y=127 / region 18,201,170,18 | `5H` 标签 / 数值 f24×2 居中 / `%`（与数字 ink 中心对齐）/ `RESET`+`hhmm` | `buckets[codex].5h.remaining` 存在（重置时间用 `time_format=hhmm`） |
| x=219,y=83 / region 217,103,132,70 / x=333,y=127 / region 219,201,168,18 | 双桶布局的 `WEEK` 标签 / 数值 / `%` / `RESET` | `5h` 存在 |
| x=105,y=83 / region 105,103,132,70 / x=221,y=127 / region 105,201,168,18 | 单桶布局的 `WEEK` 标签 / 数值 / `%` / `RESET` | `5h` 不存在（Pro 仅 weekly） |
| line y=221 | 主区底部线 | — |
| region 16,234,83,20 | 套餐名 | `account.plan`（缺失显示 `--`） |
| region 104,234,193,20 | 账号/用户名 | `bridge.label`，存在才绘制 |
| region 314,234,73,20（右对齐，前缀 `RC `） | 重置券数量 | `resetCredits.availableCount` 存在才绘制（桥在 `<=0` 时不下发该字段） |
| line y=269 | 页脚线 | — |
| x=16,y=276 | `SYNC <hh:mm>` | `device.sync_hhmm`，`device.offline_mins` 不存在时 |
| x=16,y=276 | `OFF <n>M` | `device.offline_mins` 存在时（固件在超过 `BRIDGE_LOST_MIN=6` 分钟后给出） |
| x=333,y=276 | `CODEX` | 静态（页脚结构不随状态变化） |

月桶未出现在 A 版式内（该版式只呈现 5h 与 weekly 两个主数值）；周/月桶缺失的表现为对应 `when` 分支不绘制，不留旧文字。

### 校验记录（2026-09-23，宿主范围）

| 命令 | 结果 |
|---|---|
| MCP `template_validate_v2`（本文件 JSON） | `valid=true`，`compiler_abi=1`，12 条 requirement，8 个资源 |
| MCP `template_save_v2`(`codex-status-a`, `epd-ssd2683-400x300-1bpp`) | `saved=true`、`published=false`、`source_crc=2b523381`、`compiled_crc=41c31abd` |
| `cargo run -p bridge-render -- --template <400x300 JSON> --usage sample-usage.json --out note4-a.png --compare-compiled --battery 75 …` | 400×300：JSON 与 CompiledTemplate 路径 `diff pixels: 0`，`compiled serialize/deserialize round-trip diff pixels: 0` |
| `cargo run -p bridge-render -- --template tools/test-bridge/templates/quad.json --compare-compiled` | 200×200 回归：`diff pixels: 0`，round-trip 0 |
| `bridge-render --template mini.json/full.json --out …` | 现有 200×200 模板仍能校验并渲染 |
| `cargo test -p bridge-core -p bridge-render -p bridge-mcp`（隔离 `CARGO_TARGET_DIR`） | 全绿（详见 `../status.md`） |
| `python measure-preview.py <png> …` | 对齐实测与 `--selftest` 通过；各预览 `400x300 colors=2` |

`--compare-compiled` 与画布感知的 `--diff` 是本次新增的宿主校验入口（`bridge/crates/render`）：它们同时修掉了“compiled 渲染路径默认按 200×200 分配帧缓冲、`png_to_bits` 只认 200×200”的缺口，因此 400×300 的 JSON/compiled 像素一致性与 CTP1 编解码现在可复现验证。宿主预览不等于 Note4 实机验收。

### 96px 数字排版（2026-09-24）

`codex-status-a-400x300.json` 已改用 `ntreg96` 显示双桶与 weekly-only 数字。双桶区域为 `[12,62,176,132]`、`[212,62,176,132]`；weekly-only 数字区域为 `[12,62,376,132]`，`WEEK` 与重置时间居中。百分号保留为小字，双桶分别位于 x=183/383，weekly-only 位于 x=285；重置时间保留，所有可见 `RESET` 字样已移除。宿主预览确认 `100` 不裁切，weekly-only 数字与时间居中。

`status_icons.py` 生成带单条 2px 斜线的 `bluetooth-off`、`wifi-off` 变体和阶梯式 `zzz` 深睡图标，均有 20×20 位图及放大 PNG。模板使用正常图标底图，并在 BLE OFF、Wi-Fi OFF/CONN/AP/DEEP 等状态叠加对应的斜线图标；deep 模式在状态栏 Bluetooth 左侧绘制 `zzz`，离线 Bridge 状态叠加 Bridge Off。黑色图标像素只能增加，因此此处以正常图标为底、断开图标作条件叠加。完整状态映射需要 57 个操作和 16 个不同半片资源，现已通过 ABI 2 的 64-op/16-resource 宿主编译与往返渲染校验。

### 仍待确认

- 配额缺失的表现已按用户澄清确认为**模板属性**（不写进 `AGENTS.md` 的全局规则）：200×200 `quad` 用静态 `100` 分支，400×300 `codex-status-a` 用“隐藏 5h 块、weekly 提升到主位”分支；`AGENTS.md` 对应条目已改为按变体描述。
- 蓝牙/Wi-Fi/主机的在线离线独立绑定仍未定义，图标目前是固定图形。
- Note4 实机：GPIO/波形/bring-up 未做，局刷未验证，模板与预览均为宿主交付。
