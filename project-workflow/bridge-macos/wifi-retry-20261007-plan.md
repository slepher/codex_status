# Bridge Wi-Fi 重试预算修复

## 范围与合同

成功认证HTTP交接后清空连续失败预算；BLE仅报告已关联不冒充完整成功。断网后新有限周期有3次机会，连续失败仍停止，重启/普通读取不重新授予预算。连接中/未知原事务优先确认，不因显示耗尽重发写。耗尽在MCP/UI明确展示，并提供既有use_current显式重试入口；凭据/安全失败仍提示修正。

## 实施与验证

1. 成功HTTP共用更新点归零attempts，检查全部调用点及持久化。
2. 共用Connection投影明确retry_exhausted，保留实际错误与未知/进行中状态；UI新增显式重试按钮。
3. 回归成功→断网→匹配、restart持久化、连续3次失败、密码/安全拒绝、未知事务不新增ID、UI操作/错误反馈；运行app/core与生产HTML测试。
4. 仅默认WindowsBridge按同exe watchdog→计划任务→主进程停机，保留data备份并构建/重启；无固件/USB/配对/网络配置修改。真实读取确认成功预算归零。Mac原生WK不能由Windows测试替代。

状态：Windows实现、回归、部署与实机成功预算归零均完成；Mac原生UI/部署和长期RF矩阵未验。

## 完成证据与审阅

- 首轮实机核验发现后台status路径未经过app::wifi::http_ready，因此将成功更新统一为Connection.confirm_http，并放入核心认证状态入口；严格MAC/序号/大小验证后同一次持久化，失败回滚连接及状态。仅HTTP确认，BLE与拒绝输入不清零，requested保留。
- core125/app70（1既有ignored）与生产HTML测试通过；状态投影保留密码/安全拒绝、原已发送事务优先确认，显式重试错误可见。未改无线窗口、协议存储或固件。
- 默认Bridge SHA256 BDC2818BA2EF3DB65482AF94230B366CFD89C512967E5D463C654468EB158F52/PID58704/计划任务与8765/8766；旧exe/data备份保留。真实后台HTTP将attempts2归零，主动状态刷新两次ok，原14事务和映射不变，.69同boot。证据artifacts/ble-info-panic-20261007/retry-*-proof.json及最终测试/构建日志。
- Windows/WSL原生WK脚本缺swiftc，不能宣称Mac截图或真实Mac通信通过。长期电池/RF和Windows原生保现场串口仍在B6，不以此次HTTP成功结项。
