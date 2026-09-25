# task-7 — 删除重复标签的精简预览

## 目标

删除可见 `CODEX STATUS`、`SIMULATED`、`USED11 LEFT89` 与 `USED2 LEFT98`。保留套餐、演示用户、两个额度窗口的剩余进度条与百分比、各窗口刷新时间，以及全局 RESET/EXP。底层模型继续是固定模拟模型，不代表真实账户或实时数据。

## 所有权

编码工作者仅可修改或覆盖：

- `tools/generate-preview.mjs`
- `artifacts/codex-quota-preview.png`

调度者独占工作流文件；task-1 至 task-6 及 reviews 为不可变历史。不得创建其他实现、测试或图片文件。

## 模型与内容契约

- 保持 planName PLUS、userName DEMO、`simulated:true`、resetCount3、expiresOn2026-09-21。
- 保持 fiveHour used11/leftPercent89/nextRefresh20:53，week used2/leftPercent98/nextRefresh SEP 14 15:53。
- used 值仍参与每窗口合计100的模型验证，但不再显示 used/left 明细。
- 可见文字仅包含：metadata 的 PLAN PLUS/USER DEMO；5H 的 5H/89%/NEXT 20:53；WEEK 的 WEEK/98%/NEXT SEP 14 15:53；global 的 RESET 3/EXP 2026-09-21。
- 绘制命令不得含 `CODEX STATUS`、`SIMULATED`、`USED` 或 `LEFT`；`titleCommands` 和 title band 必须删除。
- 命令组严格为 metadataCommands、fiveHourCommands、weekCommands、globalCommands，按此顺序。
- NEXT 仅属于各自窗口；RESET/EXP 各一次且 global-only。模型顶层无 nextRefresh，窗口无 resetCount/expiresOn。

## 几何契约

- 200×200 画布，四边连续且恰为 1px 黑框。
- metadata 实际 y68..72：PLAN/USER y68。
- 5H 实际 y77..96：bar y77，标签/百分比 y79 scale2，NEXT y92。
- WEEK 实际 y103..122：bar y103，标签/百分比 y105 scale2，NEXT y118。
- global 实际 y127..131：RESET/EXP y127。
- 间距严格 4/6/4；内容 y68..131，中点99.5；内部 y1..67 与 y132..198 全白，各67px。
- 不得扩大块间距填充高度。
- bar 外部 x54..155，内部 x55..154 共100px；5H 89黑/11白，WEEK 98黑/2白，均表示 remaining。
- NEXT 在对应 bar 的102px横向区域内居中；所有命令唯一归组、位于区域内、两两不重叠。

## 渲染不变量

- 仅用 `node:fs`、`node:zlib`，保留4×5字体、CRC和PNG编码。
- PNG 为200×200、RGB8、非交错，仅IHDR/IDAT/IEND，40000像素只含纯黑/纯白，确定性输出。
- 禁止系统时钟、随机、环境、网络、API、认证、硬件或外部输入。

## Coding Self-Tests

1. 运行 `node .\tools\generate-preview.mjs`，退出0并仅覆盖目标PNG。
2. 检查固定模型、used+left合计、NEXT和global所有权、四组顺序；无title group及四类被删除文字；保留文字各出现一次。
3. 解码PNG验证签名/chunks/CRC、RGB8、非交错、120200字节、filter0、纯黑白、恰好1px外框。
4. 独立断言四块范围68..72/77..96/103..122/127..131，间距4/6/4，中点99.5，留白67/67，命令零重叠。
5. 检查bar x54..155、内部100px、remaining填充89/98。
6. 连续生成两次并确认SHA-256一致。
7. 检查imports/无动态输入/仅两个实现路径变化/历史不变。
8. 原生200×200目视确认被删标签不存在，其余字段清晰，布局紧凑居中、无裁切重叠或拉伸。

## Independent Verification 与 Review

全新独立 runner 从真实源和PNG重做生成、解码、确定性、模型、负向内容、四组所有权、几何、bar、范围和原生检查，不依赖编码摘要且不编辑。随后 Sol 审阅任务、实现、PNG及两层证据，直到无实质问题。

## 完成与停止条件

只有删除项全部消失、保留字段完整、模拟模型诚实、所有几何/PNG/范围契约满足、两层测试和Sol review通过时完成。当前非Git目录，提交不适用且禁止初始化Git。若需要网络、依赖、第三个实现路径、真实API/认证/时区/刷新协议，或无法同时满足归属、自然间距和居中，则停止报告。
