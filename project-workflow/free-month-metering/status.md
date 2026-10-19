# 执行状态

更新时间：2026-10-19 11:40 香港时间

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
