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

## 2026-10-19 回归修复审查（覆盖上述“历史周窗口留空”结论）

原错误前提已由现场prolite推翻。Note4 v7从7b0cf83父提交取回4个通栏WEEK元素（坐标/字体/region/digit_x全部保留，仅when改为weekly存在），叠加当前MONTH。MONTH/5h清除区[12,70,376,147]避开Remaining标题（实测旧图标题最后墨迹到y67，绘图库1像素扩展也留有余量），然后绘制对应内容；避免扩展when/协议/固件。AP01 v3保留对应历史周环及月环，单窗口到双栏使用[16,43,288,126]条件清除。

回归覆盖free monthly、prolite weekly-only、双窗口、无窗口与混合窗口，旧v6在weekly-only断言失败，新版全部通过；12+3+8+1测试通过。Note4三场景与v5/v6历史全图像素差0；AP01三场景PNG字节一致。AP01预览未绑定中文Bundle字体，保留既有问号显示，未宣称实机中文字形验收。运行Bridge编译校验通过，18个requirements，49个Note4元素，编译记录仍11333 B；不改ROM/NVS或堆栈，额外绘制有界额度清除与周度元素。未提交git。

部署已完成：job cac235bb succeeded，认证合同6d54e167、commit802/data802 displayed。模板页遗漏的族配置已从当前设备Profile复制为Note4 Codex额度（id codex-status），列表接口读回确认。仅运行数据配置变化，不涉及UI代码。完整证据见status与artifacts/free-month-metering/v7-deployment-summary.json。

## 模板页刷新修复审查与边界

用户后续两次仍空的截图推翻此前UI已恢复的结论。已修复配置菜单只画旧familyProfiles的路径（菜单打开主动读服务），并补全MCP族配置变更通知；复用现有服务与事件，无新增缓存或依赖。Node先红后绿；原生WK70场景窄宽审阅；app114通过/3条件忽略。14:38已通过托管服务部署新exe6ea8904e…，159文件冻结备份核验，运行真实MCP仍有配置、health204。实际Tauri窗口等待用户确认；合成IPC测试不能代替该验收。

最终现场验收：用户已确认重建重启后的实际Bridge窗口显示「Note4 Codex 额度」及codex-status-a模板，UI显示问题闭环；设备实物屏幕目视仍是独立边界。

## AP01/1.54同步审查

quad v15修复实际月度数值及重置绑定；旧预览weekly选择器把>=43200也当weekly，现与Bridge分类一致排除月度，回归能真实发现绑定缺失。保留四象限/缺5h静态100，元数据固定PLAN28/label46/长期reset64/RC82，避免单条件when下RC存在性与周/月重置位置组合造成缺口；缺用户名和RC仍隐藏。月度区块条件清除后叠加；29个操作，17个需求；CT序列化固定11333B。core9/AP013/compiled12、Node预览及UI、三目标canonical通过；WK74场景且窄宽AP01/154菜单截图审阅。

AP01 v3不变，保存完整源码（含中文）及族Profile；当前运行桥fonts列表为空，未导入CSFN，因此中文预览/设备发布仍有字体资产边界，本轮没有宣称中文实机验收。模板/配置保存不接触AP01或154设备，Note4合同与已发布job保持。

本轮同步最终核验：15:26托管Bridge新exe cc81bf02…，159文件备份、签名/包一致；health204。AP01/154模板get源逐字段等于项目，三个族配置持久化；Note4 Profile和发布jobs与冻结备份完全一致。未发布设备，未提交git。
