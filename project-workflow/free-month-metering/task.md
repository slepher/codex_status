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

## T5. 2026-10-19 weekly-only 回归与发布（本轮覆盖先前验收预期）

- [x] 从7b0cf83父提交恢复原Note4/AP01 weekly-only布局；只调整存在性条件，保留历史坐标字体与数值对齐。
- [x] 在原WEEK上叠加现有MONTH；两组条件清除只覆盖额度区域，5h原双栏最终优先。无窗口隐藏标签。
- [x] 旧v6回归断言失败、新v7通过；compiled12、AP01 3、template8、core note4 1均通过。
- [x] Note4 weekly与v5、monthly/paid与v6全图像素差0；AP01三个场景与对应历史PNG完全一致。JSON/compiled往返一致；混合窗口优先级覆盖。
- [x] 运行Bridge模板与仓库v6逐字段一致，备份后保存v7，source_crc6d54e167/compiled_crcbc0a0a3a；现有Profile保持。
- [x] Note4 job cac235bb succeeded、认证source_crc6d54e167、commit802/data802 ACK displayed；屏幕仍需实机目视。
- [x] 补齐模板页族配置codex-status（Note4 Codex 额度），接口读回包含v7模板引用。

## T6. 族配置已写入但原生模板页仍空

- [x] 回归先红：空缓存→后台新增族配置→打开菜单仍空；新版改为打开菜单读取服务，Node转绿。
- [x] MCP族配置save/copy/delete成功复用templates-changed通知。
- [x] Node通过、原生WK70场景及窄宽配置菜单截图已审阅（合成IPC）。
- [x] 托管Bridge冻结159文件备份、签名校验、部署/重启；health204、配置与v7合同读回保留，app114通过/3条件忽略。
- [x] 真实运行窗口显示验收：用户确认已显示「Note4 Codex 额度」及codex-status-a模板。

## T7. AP01与1.54先修改同步（不发布）

- [x] quad v15真实monthly余额/重置，固定元数据行并保留四象限、缺5h静态100、RC/用户名缺失隐藏。
- [x] 从真实codex_snapshot→assemble_payload验证monthly.remaining=77、monthly.resetsAt=1800000000，weekly缺失为null而非借用monthly；旧版先红、新版通过。
- [x] AP01 v3、quad v15已保存运行Bridge；各自族配置已保存，id均codex-status、按render_target隔离。
- [x] UI支持AP01族选择，Node先红后绿；core template9/AP01 3/render compiled12；三目标Python canonical；WK74窄宽截图通过并审阅。
- [x] 15:26新Bridge部署/health204；真实MCP读回两目标源码逐字段一致、三族配置保留，Note4 Profile/发布jobs保持；未发布设备。
