# task-6 — 每窗口模拟刷新时间

## 目标

为两个额度窗口分别增加下一次刷新时间：5H 显示 `NEXT 20:53`，WEEK 显示 `NEXT SEP 14 15:53`。刷新时间属于各自窗口；全局 `RESET 3` 和 `EXP 2026-09-21` 必须继续只属于顶层模型和全局命令组。全部值仍是固定模拟夹具，不代表真实账户、实时时间、时区或刷新协议。

## 所有权

编码工作者只可修改或覆盖：

- `tools/generate-preview.mjs`
- `artifacts/codex-quota-preview.png`

调度者独占工作流文件。task-1 至 task-5 及其 reviews 是不可变历史。不得新增实现、测试、图片或参考文件；保留所有无关改动。

## 模型与内容契约

- `fiveHour`: used 11、leftPercent 89、`nextRefresh: '20:53'`。
- `week`: used 2、leftPercent 98、`nextRefresh: 'SEP 14 15:53'`。
- 顶层保持 planName PLUS、userName DEMO、simulated true、resetCount 3、expiresOn 2026-09-21，且不得有 nextRefresh。
- 两窗口不得拥有 resetCount/expiresOn；5H/WEEK 不得串用对方的刷新值。
- 24 小时与 `MMM DD HH:MM` 格式必须验证。
- 保留全部 task-5 文本；新增 `NEXT 20:53` 和 `NEXT SEP 14 15:53`，各恰好出现一次。
- 5H NEXT 只属于 `fiveHourCommands`；WEEK NEXT 只属于 `weekCommands`；`globalCommands` 不得含 NEXT。
- RESET/EXP 各出现一次且只属于 global；5H/WEEK 不得含 RESET/EXP。

## 实现与几何契约

- 保留内置 Node 模块、4×5 字体、纯黑白 RGB 光栅、CRC/PNG 编码、确定性输出和五个具名命令组。
- 只增加两个 nextRefresh 模型字段与两条 NEXT 文本，并调整纵坐标和验证。
- 画布 200×200，完整且恰为 1px 的黑色外框。
- 实际范围：标题 53..62、元数据 66..70、5H 76..103、WEEK 110..137、全局 142..146。
- 命令坐标：标题 y53/SIMULATED y55；元数据 y66；5H bar y76、标签 y78、明细 y90、NEXT y99；WEEK bar y110、标签 y112、明细 y124、NEXT y133；global y142。
- 实际间距为 3/5/6/4；内容中点 99.5；上下内部留白均 52px。不得通过扩大间距居中。
- NEXT 文本在各自 bar 的 102px 横向区域内居中。
- bar 保持 x54..155，内部 x55..154 共100px；分别 89黑+11白与98黑+2白，继续编码 remaining。
- 所有非框命令唯一归组、位于对应区域、两两不重叠。
- PNG 必须为 8-bit RGB、非交错，仅 IHDR/IDAT/IEND，40000 像素严格纯黑或纯白。
- 禁止动态时间、系统时钟、随机、环境、网络、API、认证、硬件或外部输入。

## Coding Self-Tests

1. 运行 `node .\tools\generate-preview.mjs`，预期退出 0 并产生目标 PNG。
2. 解码检查 PNG 签名、200×200、RGB/8-bit/非交错、chunks/CRC、120200 解压字节、200 行 filter 0、纯黑白和单像素外框。
3. 检查模型字段、格式、窗口/global 归属、NEXT/RESET/EXP 唯一性和五个命令组。
4. 独立计算实际五块范围、3/5/6/4 间距、中点99.5、留白52/52、区域外无内容、命令零重叠。
5. 检查全部既有字段、NEXT 位于相应 bar 下方、bar 89/98 remaining 和 RESET/EXP 同一 global 行。
6. 连续生成两次并确认 SHA-256 相同。
7. 检查 imports 仅为 `node:fs`/`node:zlib`、无动态输入、仅两个实现路径变化、历史文件不变。
8. 原生 200×200 目视确认 NEXT 归属明确、RESET/EXP 全局语义清楚、整体居中、间距自然、无裁切重叠。

## Independent Verification 与 Review

独立 runner 必须从真实源和 PNG 重新执行上述生成、解码、模型、归属、几何、bar、确定性、范围与原生尺寸检查，不依赖编码摘要且不得编辑。随后 Sol 只读审阅 task-6、实现、PNG 与两层证据，直到无实质问题。

## 完成与停止条件

只有两层测试和 Sol review 全部通过、所有契约满足、历史不变且无意外路径时才完成。若需要第三个实现路径、真实时间/API/认证/网络/硬件决定，NEXT 无法保持窗口专属，RESET/EXP 归属回退，或无法同时满足自然间距、精确居中与可读性，则停止并报告。当前非 Git 仓库，提交状态不适用，禁止初始化 Git。
