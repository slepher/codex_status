# task-4 — 完整账户与双额度窗口预览

## 目标

在紧凑顶部布局内展示套餐、用户、5H 用量/剩余、WEEK 用量/剩余、reset 数量和套餐过期日期。没有真实账户数据，因此必须使用明显标记为模拟的固定夹具。

## 所有权

编码工作者只可覆盖 `tools/generate-preview.mjs` 与 `artifacts/codex-quota-preview.png`。task-1 至 task-3 及全部既有审阅文件不可修改。不得新增实现、测试或参考文件，不得删除任何文件。

## 固定模拟模型

- planName: `PLUS`
- userName: `DEMO`
- simulated: `true`
- fiveHour: used 11、left 89、resetCount 3
- week: used 2、left 98、expiresOn `2026-09-21`

必须验证两窗口 used+left 均为 100、reset 为非负整数、日期符合 `YYYY-MM-DD`。进度条编码 left：分别填充 89px 与 98px。

## 必需内容

`CODEX STATUS`、`SIMULATED`、`PLAN PLUS`、`USER DEMO`、`5H`、`89%`、`USED11 LEFT89`、`RESET 3`、`WEEK`、`98%`、`USED2 LEFT98`、`EXP 2026-09-21`。

不得冒用 `CODEX PLUS` 或 `DR.HELIUM`，不得复制参考时间 `20:53`、`15:53` 或日期 `SEP 14`、`SEP 21`。task-4 预览移除旧的 `NET OFF`、`SYNC FAIL`、`STALE`、`LAST SUCCESS`、`LAST OK 14:32`。

## 最小实现方案

保留 Node.js 内置模块、200 × 200 RGB 光栅、纯黑白调色板、点阵绘制、对齐辅助、进度条、边界/重叠验证、CRC-32 与确定性 PNG 编码。只修改模型、必需文字、补充 `P`/`H`/`W`/`-` 字形、内容带、窗口布局和验证；不得重写编码器。

## 几何契约

- 完整 200 × 200、1px 黑框，无第二圈。
- 标题带 `y=5..18`：`CODEX STATUS` x=5，`SIMULATED` 右边界 x=194，同一行不重叠。
- 元数据带 `y=21..27`：`PLAN PLUS` 左对齐，`USER DEMO` 右对齐。
- 5H 带 `y=31..54`：左侧 `5H`，中间 bar `x=54..155`，右侧 `89%`；条下 `USED11 LEFT89` 和 `RESET 3`。
- WEEK 带 `y=59..83`：左侧 `WEEK`，中间 bar `x=54..155`，右侧 `98%`；条下 `USED2 LEFT98` 和 `EXP 2026-09-21`。
- bar 内部均为 `x=55..154` 共 100px；5H 为 89 黑+11 白，WEEK 为 98 黑+2 白。
- 所有非外框内容最大 `y<=83`；`x=1..198, y=84..198` 全白。
- 四带均非空、带外无内容，相邻实际块间距为 0..6px，所有非外框命令矩形两两不相交。

## Coding Self-Tests

1. 生成器退出 0，PNG 存在。
2. 验证 PNG 签名、200 × 200、8-bit RGB、非交错、chunks/CRC、120200 解压字节、200 行 filter 0、纯黑白像素与单像素外框。
3. 验证固定模型、两个 100 总和、reset 非负、ISO 日期和所有必需/禁止内容。
4. 验证标题、元数据、5H、WEEK 字段实际可见且位于各带，命令两两无重叠。
5. 验证两条 bar 绑定 leftPercent，内部为 89黑+11白和98黑+2白。
6. 验证 contentMaxY<=83、内部 y84..198 黑像素为0、四块包围盒及三个间距0..6。
7. 连续两次生成 SHA-256 相同。
8. 验证仅引用 Node 内置模块、无网络/时间/随机/环境/API/认证/硬件输入，历史文件哈希不变，无额外路径。
9. 原生 200 × 200 目视确认全部字段清晰、右侧百分比与 bar 表示剩余、模拟标识明确、顶部紧凑且底部留白。

## Independent Verification

独立只读 runner 重复全部测试，独立确认模型、字段、两窗口总和与 bar 语义、PNG、区域、高度、留白、命令重叠、源码范围、历史完整性和原生尺寸可读性；不得依赖编码者摘要或编辑文件。

## 路径、提交与停止条件

- 仅允许覆盖两个既有实现路径；授权删除为无，不得产生额外 PNG、脚本、包或参考副本。
- 建议主题：`feat(preview): show simulated account quota windows`。
- 当前不是 Git 仓库，提交为 `inapplicable — no Git repository`，不得初始化 Git。
- 如需真实账户、API、认证、时区、硬件或无法在 y<=83 内清晰展示，或需范围外修改，则停止并报告。

## 完成标准

全部字段、模型、总和、bar、PNG、紧凑布局和诚实标识通过编码自测、独立验证与 Sol review；历史不变且无意外路径或删除。
