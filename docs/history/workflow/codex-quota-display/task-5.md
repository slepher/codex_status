# task-5 — 全局 RESET/EXP 与垂直居中

## 目标

将 `RESET 3` 和套餐到期日期设为账户级全局信息，不属于 5H 或 WEEK；保持内容自然紧凑，通过整体平移在 200 × 200 内上下居中，不扩大块间距。

## 所有权

编码工作者只可覆盖 `tools/generate-preview.mjs` 与 `artifacts/codex-quota-preview.png`。task-1 至 task-4 及既有 reviews 不可修改；不得新增文件或删除任何文件。

## 模型与命令组

保留 PLUS、DEMO、simulated、5H 11/89、WEEK 2/98。将 `resetCount: 3` 与 `expiresOn: 2026-09-21` 移到模型顶层；两个窗口不得拥有这两个属性。

布局明确分为 `titleCommands`、`metadataCommands`、`fiveHourCommands`、`weekCommands`、`globalCommands`。RESET/EXP 各只出现一次且仅在 global 组；全局行左侧 `RESET 3`，右侧 `EXP 2026-09-21`。

## 几何契约

- 画布 200 × 200，完整 1px 黑框，纯黑白。
- 实际像素范围：标题 y60..69，元数据72..76，5H82..103，WEEK110..130，全局135..139。
- 全部非外框 contentMinY=60、contentMaxY=139、中点99.5；内部上下留白各59px。
- 上方内部 y1..59 与下方 y140..198 全白。
- 实际块间距严格为 2/5/6/4px，不得拉伸。
- 所有非框命令唯一归组、位于对应区域且两两不相交。
- 保留标题/元数据左右对齐、bar x54..155、内部 x55..154，5H 89黑+11白，WEEK 98黑+2白。

## 不变量

全部 task-4 字段继续可读，`SIMULATED`/`USER DEMO` 明确；两窗口 used+left=100，bar 编码 remaining。PNG 保持确定性的 200 × 200、8-bit RGB、非交错、仅 `IHDR`/`IDAT`/`IEND`、纯黑白；无网络、时间、随机、环境、API、认证、硬件或外部依赖。

## Coding Self-Tests

1. 生成器退出0；模型确认 RESET/EXP 顶层且窗口无对应属性；命令组确认 RESET/EXP 仅在全局组各一次。
2. PNG 签名、尺寸、RGB、chunks/CRC、scanlines、纯黑白、单像素外框全部通过。
3. 精确计算 contentMinY60、contentMaxY139、中点99.5、上下留白59/59，上下空区黑像素0。
4. 五块实际包围盒与间隔2/5/6/4准确；命令唯一归组且零重叠。
5. 全部字段和横向对齐回归；全局 RESET/EXP 同行、不重叠、左右对齐。
6. 两条100px bar 保持89/11和98/2并绑定 leftPercent。
7. 连续两次生成 SHA-256 相同。
8. 源码、路径和 task1-4 历史完整性通过。
9. 原生尺寸确认内容视觉垂直居中、间距自然、RESET/EXP 明显为全局信息、全部字段清晰。

## Independent Verification

独立只读 runner 重复全部测试，独立验证模型归属、命令组、PNG、精确中点与留白、五块/间距、重叠、bar、字段、源码、历史和原生目视；不得依赖编码者摘要或编辑文件。

## 路径、提交与停止条件

只允许覆盖两个既有实现路径；授权删除为无。建议主题 `style(preview): center global quota summary`。当前非 Git，提交不适用，不得初始化 Git。如需真实数据、外部依赖、范围外修改，或无法保持自然间距实现居中，则停止报告。

## 完成标准

RESET/EXP 全局归属正确，内容精确垂直居中且不拉伸，全部回归、自测、独立验证与 Sol review 通过，历史不变、无意外路径或删除。
