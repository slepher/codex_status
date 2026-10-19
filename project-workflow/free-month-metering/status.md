# 执行状态

更新时间：2026-10-19 11:40 香港时间

## 最新：AP01与1.54已修改并同步（用户要求先不发布）

- 1.54 quad v15：真实monthly.remaining与monthly.resetsAt分支，修复只换MONTH标签却仍读weekly的遗漏；预览工具weekly分类也排除monthly。保留四象限/缺5h静态100；元数据固定PLAN y28、用户名46、长期窗口重置64、RC82，缺用户名/RC隐藏。source_crc abd79ca9、compiled_crc63896a31。
- AP01 v3已保存，source_crc9bc56e9a、compiled_crce17d4238，35操作；weekly-only恢复历史布局、monthly通栏、5h双栏优先。当前Bridge字体列表为空，未导入中文CSFN（避免本轮扩大为字体/发布改造）；不宣称中文预览与实机验收。
- 两族配置id均codex-status，按render_target隔离；名称1.54 Codex额度/AP01 Codex额度，各含单模板及完整9远端字段绑定。保存均published=false；未注册/发布这两类设备，Note4 Profile与发布jobs逐字段保持。
- Bridge UI补AP01可选族；Node先红后绿，core template9/AP01 3/render compiled12、Node预览/UI、三目标Python canonical通过；原生WK74场景、窄宽AP01/154菜单截图已审（合成IPC）。quad四场景C++/compiled/往返逐像素一致并审图。
- 15:26:30托管部署exe80984128 B，SHA256 cc81bf028101309f00b4b472417b737c4b317378fd7d911f8adc766c99e535d6；停机冻结159文件App/data备份~/Library/Application Support/dev.codexstatus.bridge/backups/cross-family-sync-20261019-152556，启动前data不变、签名/部署包校验通过。health204，重启后真实MCP读回两目标源码与项目逐字段一致、三族配置保留。证据artifacts/free-month-metering/cross-family-final-summary.json及artifacts/bridge-macos/cross-family-*。
- 本轮修改随本次提交保存；不发布设备。运行新菜单的用户目视仍独立于合成WK；此前Note4实际窗口验收已通过。

## 最新：14:38 模板页配置旧缓存修复已部署

- 用户两次截图及补发模板事件后仍反馈空白，撤回此前“点模板就会显示”的结论；确认不是配置未落盘。Node复现UI首次空数组→后台新增→打开菜单仍渲染旧数组，先红后绿。修复打开配置菜单await refreshProfiles；MCP族配置save/copy/delete成功复用templates-changed。
- Node通过；原生WK70场景、560/1000配置菜单截图已审阅，范围为生产HTML+合成IPC；Rust app114通过/3条件忽略。
- 14:37:41托管停机并冻结App/data，159文件SHA一致；14:38:14构建、签名与部署包校验后托管启动。新exe80,984,128 B / SHA256 `6ea8904e315257784ba511034b8b90cc484298a0150c28cdc494ff073e8395b2`；备份 `~/Library/Application Support/dev.codexstatus.bridge/backups/template-profile-refresh-20261019-143741`，启动前data不变。
- 新服务health204、真实MCP列表包含Note4 Codex额度及codex-status-a；v7合同6d54e167与context9c3bd636edcd32e4保留。本轮不重复发布设备模板。
- **用户已确认真实运行窗口显示「Note4 Codex 额度」及codex-status-a模板，原生窗口验收完成。** 确认来自用户实际窗口反馈，独立于MCP读回与合成WK测试。无提交。

## 最新：2026-10-19 weekly-only 修复与部署

- 用户要求从历史提交恢复仅WEEK，再叠加当前MONTH；Note4 v7与AP01候选v3已按此实施，5h双栏优先，无窗口无额度标签。旧weekly-only留空规则被撤销。
- Note4 WEEK对照历史v5、MONTH/paid对照v6均全图像素差0；AP01三场景与历史PNG一致。12+3+8+1模板测试通过，旧v6在weekly-only回归先失败，新版通过。清除区已避开Remaining标题。
- 运行Bridge实际v6与仓库逐字段一致，备份到artifacts后保存v7（source_crc `6d54e167`、compiled_crc `bc0a0a3a`）。现有设备Profile保持，完整Bundle job **cac235bb**，CRC `dfcd3143`，14:26:59–14:27:04 begin/chunk/commit全部HTTP200，最终succeeded、ACK applied/displayed/flash。
- 认证新context `9c3bd636edcd32e4`、合同source_crc一致、commit_seq802；随后完整数据data_seq802 ACK applied/displayed，最终认证data_seq=applied_seq802、display_state=displayed。MAC `7C4FADB93408`、固件仍`.104-note4 / ota_0`。现场源prolite，仅weekly，剩余98%。
- 用户截图指出模板页无配置：原设备Profile存在，但family_profiles为空。已通过`platform_family_profile_copy`补齐Note4族配置`codex-status / Note4 Codex 额度`，保留全部bindings/full_sync/sync，读回列表确认引用同一v7模板。模板页再次点击“模板”标签会重新读取配置；无UI代码改动。
- 证据在`artifacts/free-month-metering/v7-*.json`及`*-v7.png`/`ap01-*-v3.png`，不入库。无重编运行Bridge、无ROM变更/手动复位。AP01仅项目候选，未保存/发布到其设备。**本轮未提交git；实物屏幕未目视，不把ACK冒充照片验收。**

## 以下为11:40历史记录（旧留空规则已被上方修复覆盖）

## 已完成

- Note4 `codex-status-a` v5 → v6：删除无 5h 时的全宽 WEEK 分支，新增月度通栏（`MONTH` + `monthly.remaining` + `%` + `monthly.resetsAt`），付费 5H+WEEK 双栏未改。
- AP01 `codex-status-a` v1 → v2（候选用，未部署）：生成器把无 5h 分支改为月度环（`月额度剩余` + `monthly.remaining` + `%` + `monthly.resetsAt`），重新生成 JSON 与 `font-characters.txt`。
- 测试：`cargo test -p bridge-render --test compiled` 12/12、`-p bridge-core --test ap01_template` 3/3、`-p bridge-core --test template` 8/8、`-p bridge-core --lib note4_template` 1/1 全绿；新增 Note4 月度/付费/历史周窗口渲染断言。
- 预览：`artifacts/free-month-metering/` 下 Note4 monthly（MONTH 77% + 到期 01-21 10:53）、Note4 paid（5H 90% / WEEK 70%，与旧版一致）、Note4 legacy weekly（配额区空）、AP01 monthly/paid（AP01 中文标签需 Bundle 字体，CLI 预览显示 ?）。
- 实时数据预览 `note4-live.png`（用 `platform_data_probe` 的真实字段构造 usage 后渲染）：`MONTH 100%`、到期 `11-18 10:59`、plan free、RC 2，与设备推送内容一致。注意 MCP `template_render` 省略 usage 时走旧 `/usage`（无 buckets/monthly），会渲染成空配额区，不能用它判断平台 v2 屏幕。
- 统一规则写入 [design §4 末尾](../../docs/generic-display-platform-design.md) 与 [power-state.md](../../docs/power-state.md)。

## 部署（Note4，用户明确要求）

- 通过运行中 Bridge MCP（`127.0.0.1:8766/mcp`）：`platform_template_validate` 通过（source_crc `ed514e3d`，compiled_crc `fa608f19`，requirements 18 项，含 monthly）；`platform_template_save` 保存 v6；`platform_profile_save` 写入 Profile（`["codex-status-a"]`、initial active 同、sync=true、render_target `epd-ssd2683-400x300-1bpp`）；`platform_publish_preview` 返回 `full_bundle`、字体 compiled_in、无必需字体资产。
- `platform_publish` 冻结 Bundle job **`436b7227`**（CRC `9da49ba7`），11:38:45–11:38:50 设备进入可达窗口：`/api/bundle/begin` 200、chunk 200、`/api/bundle/commit` 200；job 终态 **`succeeded`**，设备 `commit_seq=801`、`epd_writes=7`、`display_state=displayed`。
- 新 active case / 合同已认证读回：`active_context_id=8c851c721fe93d59`，`data_contract.source_crc=ed514e3d`（v6），requirements 18 项且包含 `buckets[codex].monthly.remaining/.resetsAt`。
- 真实源核验：`platform_data_probe` 返回 plan=**free**，仅一个窗口 `windowMins=43200`、`usedPercent=0`（remaining 100）、`resetsAt=1794970789`，另有 `resetCredits.availableCount=2` —— 与用户“免费只有月度余额”一致。
- 数据推送：`platform_push_now` 原始 ACK `{"op":"data","result":"applied","display_state":"displayed","data_seq":793}`；设备 `data_seq=applied_seq=793`，snapshot 仅缺 5h/weekly（月度已包含），无 `incomplete`。协议层部署成功。
- **边界**：未拍照/未目视屏幕；屏幕内容（MONTH 100% + 到期时间）由用户现场确认。

## 环境备注

- 本机 CommandLineTools 的 `usr/include/c++/v1` 缺少顶层 libc++ 头，`cargo test` 默认构建失败；用 `CXX` 包装脚本追加 `-isystem .../MacOSX.sdk/usr/include/c++/v1` 后通过。属本机工具链问题，未改仓库构建配置。

## 未做

- 本轮改动随提交保存；未部署 AP01（模板仅候选）；Note4 实机未目视验收；未跑完整 workspace 测试（用户同意后补）。

## 提交交接

用户已授权提交本轮Note4/AP01/1.54模板修复、族配置刷新与AP01入口及对应测试/文档。运行数据、token/密码、字体资产与日志/截图均不入库；AP01中文缺字与排版仍需正确字库后的验证，不冒充已修复。
