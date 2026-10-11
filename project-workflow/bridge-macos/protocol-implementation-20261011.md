# 协议闭环实施与验收

2026-10-11；关联A9/B6/C8。依据[执行计划](protocol-followup-plan-20261011.md)。本轮保留既有工作区，不提交、不改模板发布状态。

## 修复与失败复现

| 问题 | 失败证据 | 最终改动 |
|---|---|---|
| BLE-only Light仍执行HTTP诊断 | protocol-red-20261011.log：生产ble_cycle记录status/sync_config/plan/close后出现http:/api/status | rendezvous尾部必须继承本轮_need_http；合法SyncOpen与已准入HTTP保留 |
| ready报告ACK后HTTP仍可用却被拒 | 同日志：认证transport.ready=true、同session/wake/Plan、rv.ready=false被matching_http拒绝 | 使用transports.http.ready并保留session/wake/Plan与会合身份存在检查，后续owner检查保持；Plan明确过期立即返回；新增有界错误分类日志 |
| 本次写入401/403仍等待确认 | protocol-auth-red-20261011.log：明确拒绝后sent仍true | 仅本轮原本未发送的写入遇类型化401/403清除sent围栏并失败；历史未知事务查询失败仍保留原ID/awaiting_confirmation |
| HTTP409修订冲突被改成occupied | protocol-conflict-red-20261011.log：期望revision_conflict却得到occupied | 保留合法结构化拒绝的error/stage/MAC；维持未知形状拒绝及身份核对，不将409当成功 |
| 新Wi-Fi元数据合同类型错误 | protocol-contract-red-20261011.log：wifi_state实际number、期望text | 生产v2_status_snapshot补kind19文本；wifi_blink仍数字。固件版本.76 |

另补上一轮元数据改动的MCP Env初始化默认字段、模板最低固件测试版本与Fake深睡像素预期；没有放宽模板发布最低版本或删除像素断言。

## 软件验证

执行：`cargo test -p bridge-core -p bridge-mcp -p bridge-ble -p device-sim -p bridge-app --target-dir target-note4-sync`，退出0。合计316项通过、3项条件忽略；app78、BLE17、core134及集成7/7/8、MCP3、Fake62。证据 `artifacts/protocol-final-tests-20261011.log`。`node tools/test-device-page.mjs`退出0，见`artifacts/protocol-device-page-20261011.log`。本轮未改macOS UI，未声称WKWebView实机通信通过。

| 跨阶段场景 | 已执行证据与边界 |
|---|---|
| BLE-only + sync enabled + Light | 新增app生产ble_cycle零HTTP断言，正式Light ACK保留；不伪装为物理BLE测量 |
| ready ACK先结束、HTTP后到 | matching_http正反例；FullHttp夹具ready=false/remaining_ms=0，既有6组完整会合场景通过 |
| 丢Plan/network_start/SyncOpen ACK | app完整会合恢复、单次Plan及原期限断言；same-source Fake另验证真实命令决策 |
| 错session/wake/Plan、owner冲突、Light过期 | app matching_http与owner拒绝测试、Fake生产鉴权及期限回归；没有放宽owner/claim |
| 新写入认证拒绝与旧未知查询拒绝 | 新core四种组合保留不同结果与原ID；UI现有失败/未知/预算呈现测试通过 |
| 缓存修订冲突/分页失败 | 新409真实client测试、app stale cache测试、core完整快照/错误分类测试；失败不导入空列表 |
| 原事务恢复/存储中断 | Fake丢OTA/Bundle ACK、延迟重启、诊断持久页恢复及Bridge重启相关回归通过 |
| 窗口预算与发送前失败 | core NotDispatched/查询围栏及app队列容量、未发不耗预算回归通过 |

现有测试并未穷尽计划里的每一种故障组合。特别是物理ready报告ACK丢失、分页途中真实复位、自然会合10轮与RF/屏幕对齐，仍须单独现场证据，不能以316项代替。

## 产物与资源

- Note4唯一env `zectrix-note4-b`，版本`0.18.76-note4`，marker `codex-status-ota-v1|zectrix-note4-400x300|0.18.76-note4`。
- ROM `artifacts/note4-0.18.76-protocol.bin`，1,903,824 B，SHA256 `49558ECDA0E81302180AEC36C9E00C3070F8C55E0BD9DAF9D4FCB6E56C2200B0`；匹配ELF同名前缀`.elf`。
- RAM117,964 B，Flash1,852,479 B，均与.75相同。52项匹配ELF栈门禁通过；`protocol-note4-076-build.log`与`protocol-note4-076-stack.log`。
- 固件只增加合同kind19分支；空/单条/典型/上限仓库均新增0持久字节、0长期缓冲，未改存储格式；同一文本/数字标签的峰值字符串长度不变，动态整机峰值未重测。
- Bridge最终exe SHA256 `F4919AB9676C211ACB341A9B4A67A218EC9CCD4DA481EEA32A78FB84C654FCC6`，37,772,288 B。最终传输层补403在frame解码前识别，真实device_client7项再次通过（`protocol-auth-transport-20261011.log`）；不是再次运行316项。无新增持久字段；新增检查临时借用既有JSON，错误日志不包含凭据。
- 完整旧exe/data备份：`artifacts/bridge-before-protocol-20261011-1304.exe`及`-data.zip`，未删除运行data。

## 部署现场（持续更新）

13:04部署前直接认证Note4为.75、ota_1、data_seq/applied_seq412；transports.http.ready=true而rendezvous.ready=false，独立支持阶段/能力必须分开的判断。

新版Bridge先部署PID56448；最终403传输补丁同样经备份/标准脚本部署PID11360，8765/8766同PID，最终exe hash匹配。第二次备份为`artifacts/bridge-before-final-protocol-20261011.exe`及`-data.zip`。

OTA原job `86054e26` / request `note4-protocol-76-20261011` 只排队一次，attempt_count=1、succeeded/image_verified、confirmation_checks=1。13:10:59独立认证：.76 / ota_0，boot `6c41fa7a75602fb7bff37b9feb5a91e3`，reset_reason3，完整1,903,824 B运行镜像SHA256与候选一致；Data414 applied/displayed，Wi-Fi connected，HTTP ready=true、rv.ready=false。证据`artifacts/protocol-ota76-status.json`、`protocol-device-latest.private.json`。

13:06/13:07/13:09三次会合因网络未就绪失败，Wi-Fi观察connect_timeout，不能把它们计入成功。13:09查询并退役原激活f0结果，13:10自动开启下一合法激活f1；13:10:45收到ready报告、13:10:48认证verified、13:10:50会合complete，无先前ready误判超时。无需手工重新排队OTA或替换其ID。此为一轮自然恢复成功，未完成10轮验收。

## Review

五项均有修复前失败证据与修复后回归。修改集中于现有准入、状态验证、错误透传与合同投影，不增加新的状态机/协议版本/存储层。真实10轮会合、图标闪烁、多周期RF及故障注入仍未达到总体结项闸门；A9/B6/C8不整体关闭。
