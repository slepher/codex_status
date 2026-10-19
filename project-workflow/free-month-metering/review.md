# Review：Note4 免费月度计量模板

更新时间：2026-10-19 11:42 香港时间

## 结论

用户目标（免费账号只显示月度余额与到期时间，然后部署到 Note4）已在协议层完成：
模板 v6 已安装到设备并推送真实月度数据；屏幕未拍照，最终目视由用户确认。

## 验证证据

| 项 | 结果 |
| --- | --- |
| 模板校验 | `platform_template_validate` valid，18 requirements，含 `monthly.remaining/.resetsAt` |
| 宿主渲染 | 新增月度/付费/历史周窗口断言；`bridge-render` compiled 12/12 |
| Bundle 发布 | job `436b7227`，begin/chunk/commit HTTP 200，job `succeeded` |
| 设备提交 | `commit_seq=801`，`epd_writes=7`，`display_state=displayed`，新 context `8c851c721fe93d59` |
| 合同回读 | `source_crc=ed514e3d`（v6），18 项含 monthly |
| 数据 | `platform_push_now` ACK `applied/displayed`，`data_seq=applied_seq=793`，snapshot 仅缺 5h/weekly |
| 真实源 | plan=free，唯一窗口 `windowMins=43200`、`usedPercent=0`、`resetsAt=1794970789`，RC=2 |
| 预览 | `note4-live.png`：MONTH 100% + 11-18 10:59 + free + RC 2 |

## 关键设计取舍

- `when` 单条件（引擎无 AND），按“付费=5h+weekly、免费=仅 monthly”的互斥形态分支；5h+monthly 同时出现会重叠，已写入设计文档边界。
- 历史周窗口（无 5h、无月度）不再单独布局，配额区留空；当前真实账号已是 monthly，符合现场。
- 免费分支显示的是 `remaining`（100%）而非 usedPercent；到期时间用 `monthly.resetsAt` 默认日期格式（MM-DD HH:MM）。
- AP01 彩色模板同规则改造仅为候选（v2），未构建/未部署；其中文标签依赖 Bundle 字体，CLI 预览显示 `?` 属正常。

## 未做 / 边界

- 本轮改动与本文档同批提交；未跑完整 workspace/固件构建；未部署 AP01；未目视 Note4 屏幕。
- MCP `template_render` 的“live usage”来自旧 `/usage`，不含平台 v2 buckets，不能用于判断本次屏幕；以设备 ACK 与 probe 字段为准。
- 本机 CommandLineTools 缺 libc++ 顶层头，测试通过 `CXX` 包装脚本绕过；未改仓库构建配置。

## 后续

1. 用户目视 Note4：应看到 `Remaining` + `MONTH` + `100%` + 到期时间，plan free。
2. 若确需 AP01：部署前重跑 `cargo test -p bridge-core --test ap01_template` 并处理 Bundle 字体预览。
3. 详尽测试（workspace、AP01 原生渲染、实机矩阵）后补。
