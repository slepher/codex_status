# Codex Status 当前交接

更新时间：2026-10-13 17:35（中国标准时间；ROM/Bridge已部署；源码随本次提交归档）

## 当前部署与存档

- Note4 **7C4FADB93408**：**0.18.99-note4 / ota_1**，直接原生OTA上传1次，认证运行SHA256完全匹配BIN；reset_reason3（软件重启），旧ota_0未写。ROM `artifacts/bridge-macos/note4-0.18.99-bindings-compatible-candidate.bin`，1,839,968 B，SHA256 `e8be15320a46efe2b930dccfa4c73fb489a04cd6126c00eeab4e261d40e477dd`。ELF/map见`brs-compatible-candidate.json`。
- 最终Bridge原路径17:32已部署，托管PID1941；20,739,536 B，SHA256 `df4d523d318446dfb78a7ad9a46aa8245ab35acb0efbbfde29e14782e210a7b2`，签名通过、health204/MCP43。旧App/data/身份与本机`.85`已私有存档，替换前data再次核验；实际`.83`BIN此Mac缺失，不冒充已备份。
- 认证内容ready；新Bridge启动自动恢复Data至567，随后569收到applied/displayed ACK，回读确认569；Codex数据源good/18字段。证据 `artifacts/bridge-macos/brs-compatible-deployment-final.json`，完整响应在私有备份。[部署记录](project-workflow/bridge-macos/deployment-20261013.md)。

## 活跃事项 B6/C8

Wi-Fi任务异常重试已修复并部署：取消/到期停止自动查询，身份缺失/变化needs_attention，通用失败不覆盖；已在途真实结果仍可确认。新请求按自身ID即时尝试，忙/离线有明确反馈。Core153+22集成、App94/3条件忽略、UI66及窄宽截图通过。现场两条恢复cancelled，activate停止自动重试，跨155.6秒状态/次数/时间戳不变；Codex good、本地3目标/缓存revision265保留。新私有备份`backups/wifi-task-state-20261013`核验134文件。设备当前HTTP不可达，在线直达由模拟服务验证，未新建实机测试任务。[调查/修复](project-workflow/bridge-macos/wifi-read-queue-investigation-20261013.md)，证据`artifacts/bridge-macos/wifi-state-deployed.json`。

配置WFS4/事务WTX1独立存储，WCM1原子选择，实时观察留RAM。现存WFS4直接读、首次真实持久变更才拆分；实机4网络/2回执/2游标、revision265、ready，未强制改格式或重配网。

旧逐键/缺身份BRS2保留，不完整条目不授权/投递，完整绑定不受影响；物理修复保留已有namespace，缺失时新签发，不猜身份。取消USB导出绑定刷机前置。

验收补修Bridge：同context的认证设备序号回退使旧ACK基线失效并重推完整快照；保留未知in-flight、序号和PowerPlan。最小回归先红后绿；Core152+集成22、App93通过/3条件忽略。此前存储ASAN/9处分配故障、BLE/Fake endpoint、原生OTA回归及`.99`匹配ELF64项栈门禁通过。

## 边界与下一步

本次存档/原生OTA/新Bridge部署、数据恢复及Wi-Fi任务修复闭环完成，源码随本次提交归档。NVS16 KiB，Wi-Fi组合上限临时12,828 B未含绑定/索引/GC；真实最大容量/堆/Flash掉电及其余P0–P6仍待验收。BRS上限2856 B。唯一待办：[backlog](docs/roadmap/backlog.md)；[操作细则](docs/agent-operations.md)。
