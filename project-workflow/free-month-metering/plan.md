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

## 2026-10-19 prolite 回归修复计划

用户已授权修改项目模板、更新 Bridge 并发布 Note4。恢复按实际窗口组合显示：weekly-only 通栏、monthly-only 通栏、5h 存在时原双栏优先；月度优先于周度通栏。使用现有顺序绘制：单窗口区块在前，5h 条件清屏后绘制原双栏，避免单条件 when 的重叠，无协议/固件改动。同步修正 AP01 候选，不发布 AP01。

任务：先修订 weekly-only 回归并运行失败；修改 Note4 v7 与 AP01 v3；跑 compiled/core 模板测试及单/双/混合窗口像素对照；读取运行 Bridge 模板/Profile 后校验保存，预览并发布 Note4 完整 Bundle，等待同 job 终态并认证核对合同与数据 ACK；记录部署边界。保持现有运行 Bridge，无需重编/重启；不提交 git。

## 14:30后模板页缓存修复

用户两次截图确认仍空：只读MCP配置存在不等于窗口已刷新。回归先红复现：UI首次读取空配置→MCP持久新增→打开配置菜单，仅重画内存数组，仍显示此族暂无Profile。修复菜单打开时调用现有refreshProfiles；MCP族配置save/copy/delete成功复用templates-changed通知。运行Node+WK窄宽截图、Rust app测试；构建隔离候选，冻结App/data备份，按托管服务停止/部署/启动，保留v7/族配置与历史未知事务，验收真实MCP读回及窗口。设备模板不重复发布。

## 后续：AP01与1.54先修改同步（不发布设备）

用户明确授权。AP01核验既有v3；1.54 quad v15增加真实monthly数值/重置合同，保留四象限及缺5h静态100，元信息固定PLAN28/label46/长期窗口reset64/RC82，使周/月分支与RC独立；保留用户名/RC缺失隐藏。回归从编译需求经过真实codex_snapshot/assemble_payload验证月度字段，避免旧预览weekly选择器吞月度掩盖缺失合同。UI补AP01族可选，Node先红后绿及WK窄宽验证。保存两目标模板及可见族Profile，冻结备份重建重启Bridge以嵌入UI，核对两族配置/源CRC和Note4发布不受影响；不注册/发布AP01或1.54实机，不提交。
