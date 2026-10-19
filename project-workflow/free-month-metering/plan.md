# 免费账号月度余额计量模板（候选，未发布）

更新时间：2026-10-19 11:35 香港时间

## 用户结论

免费账号只有月度（`windowMins >= 43200`）余额；免费布局只显示月度余额与到期（重置）时间，不再显示 WEEK/周额度占位。

## 统一规则（两个目标一致）

1. 有 5h 窗口（付费账号）：保持现有双栏布局（5H + WEEK 或周环 + 5h 栏），不改视觉。
2. 无 5h 且存在月度窗口（免费/Go）：只渲染月度区块：
   - 大字 = `buckets[codex].monthly.remaining`
   - `%` 按位数对齐，`digit_bind = buckets[codex].monthly.remaining`
   - 重置/到期时间 = `buckets[codex].monthly.resetsAt`
   - 标签：Note4 `MONTH`；AP01 彩色 `月额度剩余`
3. 无 5h、无月度（历史周窗口账号）：不再单独布局，配额区留空（原 5h-absent 的 WEEK 全宽布局删除）。如现场出现该账户形态，另行设计后补。
4. 每条元素只能有一个 `when` 条件（引擎不支持 AND），因此规则依赖两种账号形态互斥：付费 = 5h+weekly、免费 = 仅 monthly。若出现 5h 与 monthly 同时存在，双栏与月度区块会重叠，属已知边界，不在本次扩展。

## 写入范围

- `bridge/crates/core/tests/fixtures/codex-status-a-400x300.json`
- `tools/generate-ap01-template.mjs` + `tools/test-bridge/templates/codex-status-ap01.json` + `project-workflow/ap01-template-integration/font-characters.txt`
- `bridge/crates/core/tests/ap01_template.rs`，`bridge/crates/render/tests/compiled.rs` 等模板断言
- `docs/generic-display-platform-design.md`、本目录

## 不做

- 不改固件/协议/引擎（`monthly` 选择器自 0.12.5 已存在，Note4 运行 0.18.104 支持）。
- 不保存模板到 Bridge 库、不发布 Bundle、不刷机、不提交 git；发布是用户显式动作。
