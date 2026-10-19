# 任务与验收

## T1. Note4 `codex-status-a`（400×300，v5 → v6）

- 删除 5h 不存在的全宽 WEEK 分支（原 4 个元素，`when: buckets[codex].5h.remaining exists:false`）。
- 新增 5h 不存在的月度通栏分支（`when: buckets[codex].monthly.remaining exists:true`）：
  - `MONTH` 标签（ntthin18）
  - 大字 `remainining`（ntreg96，region 376×132 居中）
  - `%` + `digit_bind`
  - `resetsAt`（region 200×26，居中）
- 付费双栏（5h+weekly）保持不变。

验收：
- `validate_template` 通过；canonical/compiled/host render 像素一致。
- monthly 数据（`windowMins:43200`，无 5h/weekly）渲染出余额与时间；weekly-only 数据不出现月度内容；付费数据与旧布局像素一致（相对基线）。

## T2. AP01 彩色模板（`codex-status-ap01`，v1 → v2）

- 生成器把 `dual=false` 分支由“周额度剩余”环改为月度环：
  - `ring` bind `buckets[codex].monthly.remaining`，`when monthly exists:true`
  - 标签 `月额度剩余`
  - 数值 + `%` + `resetsAt`
- `dual=true` 双栏保持不变。
- 重新生成 JSON 与 `font-characters.txt`。

验收：
- `cargo test -p bridge-core --test ap01_template` 通过；月度场景环覆盖率/文字像素/RC 独立断言通过；旧场景断言更新为月度语义。
- `validate_template`、`bridge_render::validate` 通过；compiled 与 JSON 渲染逐像素一致。

## T3. 测试与证据

- 新增/更新 Rust 测试覆盖：付费=双栏、免费=仅月度、monthly 缺失时免费区块不出现。
- 用 `bridge-render` 生成 Note4 月度/付费与 AP01 月度/付费 PNG，人工目视核对并留在 `artifacts/`（不入库）。
- 记录未做的验收：真机 free 账号上屏、模板保存/发布、AP01 实机。

## T4. 文档

- `docs/generic-display-platform-design.md` §4/§15 追加统一窗口规则与候选状态。
