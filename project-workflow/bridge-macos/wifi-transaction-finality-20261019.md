# Wi-Fi 事务执行终点修复（B6/C8，2026-10-19）

## Plan

用户授权修复、回归、部署 Bridge 并验证 Note4 自动切回办公室。根因是将历史结果未知等同于当前执行占用。保留旧 request/principal/namespace/sent/may_have_committed/confirmation；取消、过期、身份无法恢复与确认预算耗尽结束执行，不伪造设备结果，不重发原事务。仅未结束执行的未知写阻挡冲突操作，新事务仍走认证/owner/版本/修订校验。

## Task

- [x] 服务层先红：取消/过期未知事务持久保留，重启后允许后续独立事务。
- [x] 身份缺失/不符与有限确认重试有终态，原ID查询、不盲重发。
- [x] 真实 ble_cycle 跨网络恢复、认证失败/活动未知写反例回归。
- [x] 相关 Rust 回归与审查；备份 App/data，构建部署并自然恢复核验。

## Status

已完成。2026-10-19 10:35:27部署最终Bridge，80,984,128 B，SHA256 `82eb5ae4114f31a89f8b3d59e1194ecea45ece79abae9d75c90fbbcf9476b7fc`，managed supervisor42524，health204。冻结App/data备份 `~/Library/Application Support/dev.codexstatus.bridge/backups/wifi-finality-20261019-103459`，157数据文件启动前逐文件哈希一致、签名核验通过；初始未知历史对比仍使用首份10:21:24备份。

10:36自然BLE Plan/Data788确认；10:37:17独立办公室激活003f accepted/pending，network_start accepted；10:37:24起认证HTTP200，10:37:48按原ID确认completed/operation_result；10:37:57完整读取revision341、active=office/v235、SSID/hostSSID Xiaomi_1EBB、IP192.168.31.90/HTTP ready，固件0.18.104-note4/ota_0、boot_id不变。原三条未知事务逐字段及SHA完全一致，无删记录/清数据/猜身份/旧写重发。未刷机、未USB/BOOT/手动切网；屏幕图标未直接目视。

最终白名单证据 `artifacts/bridge-macos/wifi-finality-live-final-20261019.json`、事件 `wifi-finality-live-events-20261019.log`、部署 `wifi-finality-deployed-20261019.json`，日志与私有运行数据不入库。

## Review

共享Job终态释放执行，confirmation_pending仅保留历史结果证据；原ID查询、取消/过期不复活、身份不猜测。确认预算使用独立持久confirmation_attempts（默认0不序列化，不改旧历史字段），至多3次查询；不得用总attempt_count把发送前等待计入确认预算。新增字段仅Bridge运行JSON，非设备协议/固件/NVS变更；非零记录增加约26字节，至多32条额外约832字节JSON与每Job一个u32。旧App回滚须配套本次冻结data备份，不能读新增非零字段后直接降级。

先红证据：wifi-finality-red-20261019.log（未知无终点、取消/过期全局阻塞），wifi-finality-cycle-red-20261019.log（真实ble_cycle返回wifi_confirmation_pending）。初次回归暴露了总attempt_count混入待授权/待输入机会，已改独立计数并重跑。Node通过；WK68通过，560/1000 waiting截图已审阅；合成IPC仅证明UI。第一版10:22已部署；实机暴露异步activate返回operation_pending被当错误，跳过network_start，设备最终durable_rejection/window_closed。恢复调度只处理保存写也使控制事务确认依赖自动匹配分支；第二版补所有已发事务按原ID恢复查询（包括未知Plan只读确认），接受pending后继续正式Plan下network_start，仍不宣称事务完成。新增sent_activate_recovery先红及真实ble_cycle异步pending回归；二版全套回归/部署/实机验证进行中。


最终二版Rust333项通过/5条件忽略，另显式活动未知保护device-sim 1项通过（合计334项通过、剩余4项条件忽略），Node/WK68无需重跑（第二版无UI变更）。异步cycle初次失败为测试未模拟5秒退避结束，已仅调整测试时间推进；生产退避保留。最终日志wifi-finality-regression-final-r2-20261019.log、wifi-finality-active-unknown-fake-r2-20261019.log。
