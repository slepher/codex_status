# Bridge—设备下层通信：场景与当前实现对照

核对日期：2026-10-08。本轮只读源码并整理文档，未执行测试、设备通信或部署。**下表不是全场景验收报告。** 应有行为见 [场景文档](bridge-device-communication-scenarios.md)；具体 wire 见 [协议文档](bridge-device-protocol.md)。运行版本及既有实机证据以 [PROGRESS](../PROGRESS.md) 为准。

## 1. 本轮结论

现行源码具有 BLE 保留到认证 HTTP proof/handoff 的正常路径，也具有超时收尾、丢 handoff ACK 后 HTTP 查询及有界 disconnect。不能据此断言全部调用入口和所有故障现场均符合场景合同。尤其需要区分设备 local ready、Bridge 认证 ready、普通 BLE 释放及物理恢复窗口例外。

用户确认日常 BLE 可完成的本轮工作不启动 HTTP。现行已有 BLE 小Data/Plan路径，但 `ble_cycle` 还含 HTTP probe、同步配置/开窗及工作准入分支；本轮未证明所有“只需BLE”现场都不会因其他调度条件开启HTTP。新统一日常交换报文尚未在本轮实现，不能将方向说明视为已部署。

## 2. 场景对照

| 场景 | 当前可见实现证据 | 本轮判断 / 证明边界 |
|---|---|---|
| S01/S02 HTTP 直达 | HTTP 业务入口、Wi-Fi 应用侧机会处理与认证状态入口已存在；交汇模块不是这些 HTTP 协议的必需入口 | 具备直达机制；本轮未逐一证明 Bundle/OTA/Activate 每个调度入口在 BLE 禁用时均不依赖交汇，不宣称全入口通过 |
| S03 BLE 小数据 | `ble_cycle` 的 `ble_data` 分支发送 data；固件保存 pending 显示并在交汇收尾绘屏 | 有对应分支；显示成功仍须看结果，不由 BLE ACK 推定 |
| S04/S05 准备 HTTP 再转交 | 固件 `admitHttp`→`rendezvousWifiBegin/Tick`；Bridge `ready_and_handoff` 先认证读取 HTTP 状态，再发 handoff，之后有界关闭连接并继续已接纳 HTTP 工作 | 正常顺序存在；网络激活及每类工作组合未在本轮运行验收 |
| S06 准备失败/超时 | 两侧绝对 Budget/deadline；固件 tick 到期收尾，Bridge ready 循环预算耗尽返回错误并 close | 超时机制存在；固件允许到期关闭 BLE 而未交接，这属于失败收尾，不是正常顺序违反 |
| S07 身份/会话/占用拒绝 | Bridge `matching_http` 与 claim 核对；固件 `rendezvousProofMatches` 校验 session、owner、bridge、plan、wake 和 challenge | 有校验；明确拒绝与错误反馈需按入口及故障分别验收 |
| S08 handoff ACK 丢失 | Bridge 失败后重新读取认证 HTTP，核对原交汇 ID 与 handoff 状态，不另开窗口 | 有原交汇恢复分支；未覆盖所有响应丢失/设备重启组合 |
| S09 ready 前 BLE 意外断开 | `ready_and_handoff` 可以尝试 HTTP；显式 handoff 仍通过原 BLE link，失败后查询 HTTP | 部分路径可恢复“已转交但 ACK 丢失”；不能保证“尚未转交就断 BLE”也可完成，需独立故障验收 |
| S10 disconnect 卡住 | Bridge 先 drop 全局/设备 delivery 锁，再 `close_bounded`；关闭后按 MAC 重新取锁做已接纳 HTTP 工作 | 清理不无限阻塞的机制存在；Bridge close 返回不证明设备无线已释放 |
| S11 物理 BLE 窗口 | 固件 `v2RendezvousTick` 仅在没有 `v2ManualBleHoldActive` 时设置 released 并 `bleOff` | 显式例外存在；业务交汇结束不能写成无条件关闭 BLE 控制器 |
| S12 到期/重启/权限变化 | deadline/nonce/wake/owner 检查分别存在；Budget capped 防止阶段重新计时 | 跨重启、owner 到期和边界竞态组合不能仅凭独立检查宣布通过 |
| S13 双通道不可用 | Bridge 作业及有界机会/错误处理已存在 | 本轮未逐一审阅所有业务的总等待终态；“本次尝试有界”不等于“整个作业永不无限等待” |
| S14 换接后失联/OTA 重启 | Wi-Fi 原事务恢复、OTA 待运行镜像确认路径已有；PROGRESS 记录 .68 的 BLE 恢复网络与镜像确认闭环 | 有特定历史实机证据；不是本轮新验证，不推广为全部业务、双板或跨平台通过 |
| S15 网络映射 | `current_target` / `matching_target` 选择唯一 host_ssid；`require_same_network` 防主机换网竞态 | 有映射与重验机制；真实主机权限/环境组合仍须分平台验证 |
| S16 按需诊断 | `ble_cycle` 先有界 HTTP probe，失败才准备连接诊断；显式 requested 意图另行核对 | 有对应分支；不能将规则误解为取消必要认证状态/handoff 查询；未证明全部成功路径都没有冗余查询 |
| S17 保存/激活分离 | `automatic_on_link` 匹配内容，不符走冻结 recovery_save，保存完成再启用；`ready_and_handoff` 不接受旧网络冒充目标激活 | 有分支；跨版本与旧/新作业组合未在本轮运行 |
| S18 重试与恢复 | `automatic_on_link` 当前最多3次、冷却≥60s，认证/安全错误停止；`Connection::confirm_http` 成功归零，最新 PROGRESS 有已部署实机证据 | 源码及历史特定成功样本，非本轮新验证；不能推广为完整 RF 矩阵 |
| S19 准入及关闭顺序 | `ble_cycle` 冻结 admitted_wifi，完整读取先于 `sync_http_inner_budget` 的诊断结束逻辑；未完成读时临时窗的 complete 受条件约束 | 机制与既有测试源码存在；本轮未执行组合测试 |
| S20 取消/过期 | core Wi-Fi Job 有24h默认 TTL、sent/may_have_committed；expire 改本地终态并保留原字段 | 证明本地数据模型保留不确定性；不据此假定全部UI/后台恢复入口已正确呈现或完成查询 |

## 3. HTTP ready 的实际定义差异

固件 `rendezvousWifiTick` 在 Wi-Fi connected 且 IP 非零时调用 `v2Rv.httpReady()`，进入 `WaitHandoff` 并生成 challenge；HTTP 服务此前由 `rendezvousWifiBegin` 启动。这里的 ready 是**设备本地准备状态**，并未证明 Bridge 可访问服务。

Bridge `ready_and_handoff` 实际通过 HTTP 认证读取状态，并用 `matching_http` 核对身份/会话/wake/plan。存在网络激活意图时，还确认连接结果，避免旧网络上的可达性被当作新激活完成。随后才进行 handoff；完成后更新应用侧 `http_ready`。

因此固件 `rendezvous.ready` 与 Bridge `connection.http_ready` 不是同一个观察点，也不是同一报文变量；实现审阅与日志解释必须区分。这是旧实现“HTTP ready后关BLE”的成功判据，不再是最新目标规则；新场景要求BLE接纳后提前结束，HTTP报告与业务验证独立完成，尚待实施。

固件 `enterBleOn` 获取 `setBleAwake(true)` 的 `ESP_PM_NO_LIGHT_SLEEP` 锁，`bleOff` 才释放。故普通 BLE 尚在时不能将正式 `power.mode=light` 推断成 CPU 已自动 light-sleep；物理 hold 或其他 PM 锁可能继续阻止睡眠。PM 真实驻留必须另看运行证据。

## 4. 当前状态机与具体载体细节

以下是当前实现，固定常量不是场景合同：

- 固件阶段：`Exchange → HttpStarting → WaitHandoff → AckGrace → Done`；无 HTTP 需求可由 Exchange 直接进入有界收尾。deadline 也可让未 ready 的阶段进入 Done。
- `RV_CONNECTED_MS=6000`，HTTP handoff 上限 `RV_HANDOFF_MS=30000`，ACK grace `RV_ACK_GRACE_MS=200`；admitHttp 取合法在线期限与交汇窗口的较小值，不改正式 Plan 的截止。
- Bridge `HTTP_HANDOFF_BUDGET=30s`、`CLOSE_BUDGET=2s`；`Budget::capped` 与已采样设备剩余时间共用原绝对预算。
- 当前 handoff 通过 BLE `op=handoff` 发送 `plan_id/rendezvous_id/challenge/wake_generation/wake_seq`，携当前业务会话信封。challenge 从认证 HTTP 状态获得。
- 固件 handoff proof 同时检查 owner/session、Bridge、会合 ID、challenge、Plan 与 wake；ACK grace 后普通 BLE 关闭，物理恢复 hold 是例外。
- ACK 丢失时 Bridge 认证查询原 HTTP 状态，要求同交汇 ID 且 `rendezvous.handoff=true`；没有证据就返回 `handoff_unconfirmed`，不补开新窗口。

其中“200ms grace”只保证当前状态机提供短收尾时间，不保证 Bridge 实际收到 ACK；必须保留丢 ACK 的恢复路径。

## 5. 源码与既有测试入口

| 责任 | 位置 |
|---|---|
| 固件状态机与证明校验 | [rendezvous_protocol.cpp](../src/rendezvous_protocol.cpp)、[头文件](../src/rendezvous_protocol.h) |
| 固件 HTTP 准备、状态与 BLE 收尾 | [main.cpp](../src/main.cpp)：rendezvousWifiBegin、rendezvousWifiTick、v2RendezvousTick、handleV2Status、serviceV2Ble |
| Bridge 认证 ready、handoff、锁释放/close、HTTP 后续工作 | [app rendezvous.rs](../bridge/crates/app/src/rendezvous.rs)：ble_cycle、ready_and_handoff、matching_http |
| Bridge 共享绝对预算与错误分类 | [core rendezvous.rs](../bridge/crates/core/src/rendezvous.rs) |
| Bridge HTTP/业务机会与既有测试 | [platform.rs](../bridge/crates/app/src/platform.rs)：含 http proof→handoff→close→page→complete 顺序、丢 ACK、owner 拒绝的测试 |
| Wi-Fi HTTP 确认/网络恢复 | [wifi.rs](../bridge/crates/app/src/wifi.rs) |

本轮仅确认上述测试源码存在，未重新运行。新场景验收若发现缺口，应按 [backlog](roadmap/backlog.md) 登记，不以修改本说明取代实现修复。

历史核查发现旧 non-unified Wi-Fi 分支仍会在 peer/generation/加密状态变化时取消候选，统一事务不依赖这些条件。协议取代和兼容代码退役是不同结论，见 [历史对账](bridge-device-protocol-history-audit.md)。

## 后续设计变化：设备主动 HTTP 报告（未实现核验）

2026-10-08 用户改为 BLE 接纳后提前结束普通 BLE，由设备 HTTP 客户端报告本地 ready 及原业务结果；后续 Bridge HTTP 推送另验证入站可达。见 [信封设计 §8](bridge-device-compact-envelope-design.md#8-提前结束-ble设备-http-报告的场景修订)。本文旧 handoff/sync_complete 分支证据仍描述现行实现，不证明新方案已支持；组合 Data+Light 提交、设备 HTTP report/方向认证、报告重试与旧事务恢复均需实施验收。
