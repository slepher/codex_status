# task-5 Review Round 1

## Verdict

passed

## Findings

No material finding remains. RESET/EXP 已明确归入全局模型和全局命令组；紧凑内容块通过整体平移精确垂直居中，没有扩大内部间距。

## Global Semantics

- `resetCount: 3` 与 `expiresOn: 2026-09-21` 位于模型顶层。
- 5H/WEEK 对象只包含各自 used/left。
- `globalCommands` 在同一 y=135 行左对齐 `RESET 3`、右对齐 `EXP 2026-09-21`。
- RESET/EXP 各只出现一次，且不属于任何额度窗口组。

## Centering and Spacing

- 实际五块：标题 y60..69、元数据72..76、5H82..103、WEEK110..130、全局135..139。
- 全部内容 y60..139，高80px，中点99.5；画布内部中点同为99.5。
- 上下内部留白均为59px，对应空区黑像素均为0。
- 实际间隔保持2/5/6/4px，居中未通过拉伸实现。
- 所有非框命令唯一归组且两两无重叠。

## Regression Evidence

全部账户字段、`SIMULATED`、`USER DEMO`、5H 11/89、WEEK 2/98 均清晰。两条100px bar 仍编码 remaining 89/98。编码自测与独立验证确认 PNG 1686 bytes，重复 SHA-256 `80EBA3073DB9E79CB098B5D1DBAE733B2A33D3C11B9FD8E5D0DAA13B7C0CE2A4`，200 × 200、纯黑白、单像素外框、chunks/CRC/scanlines 均正确；黑4259、白35741、其他0。源码/路径/历史与原生目视检查通过。

## Simplicity and Scope

实现只移动两个模型属性、拆分五个命令组、增加全局行、平移纵坐标并加入直接归属/居中校验。实现仍只覆盖生成器和 PNG，无额外路径或删除。

当前不是 Git 仓库，提交状态为 `inapplicable — no Git repository`。

## Completion

task-5 全部完成标准满足。

## Continuity

Next Task: none  
Next Sol: none  
Reason: RESET/EXP 全局化与垂直居中要求已完成；真实数据和固件仍等待后续决定。
