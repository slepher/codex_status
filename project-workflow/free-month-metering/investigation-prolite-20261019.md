# free → prolite 余量区空白调查

2026-10-19 14:20（Asia/Shanghai）。范围：只读调查与宿主重放；未改模板、未发布、未重启或刷机。

## 结论

7b0cf83 将 Note4 v5 的 weekly-only 全宽分支替换为 monthly 分支，同时保留原有付费两栏依赖 5h 存在的条件。错误前提是“付费必有 5h+weekly”。当前现场 plan=prolite，Bridge 源快照仅有 weekly(windowMins=10080, usedPercent=2)，即 remaining=98，无 5h/monthly。因此两个分支均隐藏，虽然 weekly 数据有效也完全不绘制。AP01 候选同样删除了 weekly-only 分支，有相同覆盖缺口，未进行实机验证。

## 证据与复现

- 本机运行 Bridge MCP `platform_data_sources` 返回 source=codex、quality=good、plan=prolite、weekly used=2；保存到 `artifacts/free-month-metering/prolite-live.sources.json`，按这些值构造 `prolite-live.usage.json`（不是原始 app-server 响应）。没有单独取上游原始响应；本调查不解释上游为何没有 5h。
- 持久认证合同 source_crc=ed514e3d、context=8c851c721fe93d59，对应已部署 v6；缓存 data_seq=applied_seq=801。当天日志 06:13:30 UTC（14:13:30 本地）seq801 完整快照仅缺 5h/monthly，weekly 未缺；06:13:31 原 seq BLE ACK applied。设备状态为 displayed；照片直接证实余量区空白。
- 同一真实值构造的 usage，运行现有 `bridge/target/debug/bridge-render --template <v6或v5> --usage artifacts/free-month-metering/prolite-live.usage.json --out <png> --compare-compiled --battery 81 --sync 14:14`：v6 配额区全空；v5 显示 WEEK 98% / 10-26 12:51。两次 JSON/compiled 与序列化往返像素差均0。
- 重放图片：`artifacts/free-month-metering/prolite-live-v6.png`、`prolite-live-v5.png`；v5 基线来自 `git show 7b0cf83^:bridge/crates/core/tests/fixtures/codex-status-a-400x300.json`。
- 根因位置：Note4 模板 weekly 元素 when 使用 `buckets[codex].5h.remaining exists:true`，约381行；原全宽WEEK元素在7b0cf83改为MONTH，约423行起。
- 测试缺口：`bridge/crates/render/tests/compiled.rs` 209–210 行将 weekly-only 场景命名 legacy_weekly，明确断言该区无墨迹。测试固定了错误产品预期，而不是覆盖切回 prolite 的真实形态。

## 修复方向（尚未实施）

按实际窗口组合覆盖 5h+weekly、仅weekly、仅monthly；恢复 weekly-only 展示并保留 monthly。不能简单恢复所有“无5h”元素：引擎每个when只有一个条件，需要处理月度与周度分支互斥及5h+monthly已知边界。同步修正Note4/AP01候选的测试和规则文档，使用free→weekly-only prolite→双窗口的序列做回归。模板保存不等于发布，设备发布须按用户明确指令。

## 14:30后模板页仍空的独立调查

MCP列表与磁盘均有Note4族配置，用户截图及补发事件后的现场反馈仍空。已排除“根本没有写入配置”，尚不把接口读回当窗口验收。新增Node回归真实复现UI模式：先读空数组，后台保存族配置后，打开配置菜单只renderProfileMenu而不重新读服务，持续显示旧空数组。另一代码缺口是MCP族配置save/copy/delete没有触发templates-changed（只有template_save会触发）。修复菜单打开await现有refreshProfiles，成功的族配置变更复用现有通知，无新缓存层/协议。此前“再次点模板即可看到”的现场结论撤回。

Node先红后绿，原生WK70场景（560/1000各35）通过；截图只证明生产HTML和合成IPC渲染，真实运行窗口仍须在部署后核对。已确认只有一个托管Bridge子进程同时监听8765/8766，原exe SHA82eb5ae4…符合交接卡。
