# Task 5 — 双策略会合与窗口对齐（候选，需 spike）

Status: 需求与候选协议已整理；未实现、未冻结空口编码/参数、未验证 Windows 适配器。
task-2 的常驻 adapter/Peripheral 生命周期、discovery/INFO/ACK 提速仍优先；两策略须保留并能独立 A/B，
不通过反复撤销代码切换。task-3 分阶段数据决定预算，不能由旧采样推断 GATT 永远无法提速。

## 1. 需求、角色与共用业务

| 策略 | 会合发现与指令交换 | 后续交付 |
|---|---|---|
| `device_first`（默认/恢复） | 设备广播；PC 扫描并作为 central 连接，设备为 GATT server | 既有认证 status/data/plan；小快照 BLE，正式 light 后 HTTP |
| `bridge_first` | PC 在**每个**预定窗口重复广播指令，无工作也广播；设备扫描、验证并重复回复 StatusBeacon | ACCEPT_SLEEP 后睡眠；OPEN_WIFI 后 PC 有界等待 HTTP，复用 v2 交付 |

只倒转广播/扫描方向，不让设备发起既有 GATT 连接，不新增 PC GATT server。
设备每个 bridge_first 窗口都必须回复，包括没收到有效指令的窗口；不能用静默代替。
PC 市电承担等待/重试调度；设备仍支付有界扫描、回复、Wi-Fi 建连及等待能耗，成本绝非零。
两路径共用每设备 coordinator、MAC、owner、context、Data/Bundle/PowerPlan 和业务 ACK；
仅会合入口与 transport 分叉，不另建待办/指纹/发布模型。

## 2. 与现行 v2 的边界

总设计 §7/§8 已于 2026-09-23 同步双策略边界；`v2Rendezvous()` / `serviceV2Ble()`
仍仅实现 device_first。本任务是**已进入权威设计、仍待 spike 的候选控制面扩展**，
实现仍须经过能力协商，不能把未认证 hint 接到开 Wi-Fi 分支。

候选边界：通过当前认证通道安装的会合配置预先限定短时无线引导预算。
认证 OPEN_WIFI 只触发本窗口一次有界 HTTP 会合机会；不创建正式 plan、不修改 accepted plan_id/light deadline，
不产生 BOOT provisional、不许可业务写入。独立 bootstrap 硬截止覆盖回复、连接及首次认证握手。
只有 Bridge 在 HTTP/GATT 中的正式 PowerPlan 可设 light deadline；接受正式计划后由其实际截止及设备安全上限
接管在线期限。重复广告、探测、读取、数据、claim/renew 均不延长任何截止。
ACCEPT_SLEEP 表示本次不打开 Wi-Fi、按既有到期路径收尾，不是新的 sleep PowerPlan；
不能撤销仍有效的正式 light/BOOT 窗口。短指令只用于 deep timer 会合入口。

控制密钥绑定已授权 Bridge/设备。有效他人 owner 拒绝指令；owner 过期也不能由广告重新创建。
空闲 owner 的开网机会只允许已有绑定且已验证设备操作 token 的 Bridge（总设计 §7 的有限 claim 机会）；
随后仍需设备操作 token 的显式 POST /claim。HTTP `/v2/*` 保持 endpoint token + owner，
完整 MAC/session_nonce/context 重新握手；401/409 停写，不暗中抢占。

## 3. 协议时序与最小状态机

```text
device_first:
  timer → 设备广播 → PC central 连接 → 认证 status → 共用 coordinator
        → [小 Data + 业务 ACK] → 正式 PowerPlan + ACK → 关 BLE
        → sleep/渲染，或有界 Wi-Fi → 共用 HTTP 交付

bridge_first（已认证配置约定窗口、密钥、恢复周期）:
  PC: 窗口前 guard 请求启动 Publisher + Watcher；每窗口冻结一条 directive 并重复发送
      ├─ ACCEPT_SLEEP（无待交付工作）
      └─ OPEN_WIFI（已授权待办/正式同步到期/允许的 claim 恢复机会）
  设备: timer → SCAN（有界）→ 验证有效 directive → REPLY（重复 StatusBeacon）
      ├─ ACCEPT_SLEEP → 回复 ACCEPT_SLEEP → 关 BLE → 渲染 → deep
      ├─ OPEN_WIFI → 回复 WIFI_OPENING → 关 BLE → 有界 Wi-Fi/HTTP 引导
      └─ 截止无有效 directive → 回复 NO_DIRECTIVE（含原因）→ 关 BLE → 渲染 → deep
  PC: OPEN_WIFI 发出即并行等待 {StatusBeacon, HTTP 就绪}
      → HTTP 可用后核对身份/认证/owner → 共用状态/正式 PowerPlan/Data/Bundle/业务 ACK
      → 仅 BLE 回复到达则继续有界 HTTP 重试；总超时留待办到下窗口
  设备: 无正式 plan → bootstrap 硬截止关网；有正式 plan → 按实际计划截止执行
```

设备：`DEEP → DISCOVER(strategy) → REPLY(bridge_first) → DEEP | WIFI_BOOTSTRAP → WIFI_LIGHT → DEEP`。
device_first 的 DISCOVER 包含既有 GATT 交换。BOOT 始终允许 device_first，300s 从物理唤醒起算。
PC：`WAIT_WINDOW → EXCHANGE(strategy) → WAIT_HTTP | COMPLETE/RETRY_NEXT`；
WAIT_HTTP 内 BLE 监听与 HTTP 轮询并行，通过认证状态握手才进入共用交付协调器。
同设备业务交付锁不得被整段无线等待占住；取消/截止后旧窗口异步结果不能触发新交付。

分别记录 directive、回复、端口、认证状态和业务结果。WIFI_OPENING 不等于 Wi-Fi 已连接；
端口开放不等于身份验证；StatusBeacon 摘要不是 Data/Bundle/PowerPlan ACK。
BLE 回复漏收但认证 HTTP 已通时直接继续，记 `status_beacon=missing,http=authenticated`，不等 BLE 补包。
可信 NO_DIRECTIVE 证明本窗口设备活跃，可区分下行未收/未验证；两路全无只能记“未确认”，
不能由单窗口丢包推断真正失联。连续缺证据且恢复窗口失败才标疑似失联。

## 4. 最小空口候选（legacy 31B）

单个 Manufacturer Specific Data AD；不依赖 scan response/extended advertising。
保守预留 Flags 3B + AD 长度/类型 2B + Company ID 2B，应用 payload **24B**。
Company ID 合法使用、Windows 自动附加 AD 与实际长度须 spike 抓包确认，不借用第三方 ID。
不同时塞名称/128-bit UUID；device_first 保持原发现格式，两格式分别识别。

| 应用字段 | 字节 | 含义 |
|---|---:|---|
| protocol/type | 1 | 版本及 Directive/StatusBeacon；未知拒绝 |
| target_short_id | 3 | 绑定配置分配的路由短身份，不是 MAC 后缀凭证 |
| config_epoch | 4 | 密钥/策略配置代次，完整配置经认证通道协商 |
| window_seq | 4 | 本 epoch 会合窗口序号，一窗口一条冻结指令 |
| action/result | 1 | 下行 ACCEPT_SLEEP/OPEN_WIFI；上行 ACCEPT_SLEEP/WIFI_OPENING/NO_DIRECTIVE/REJECTED |
| reason/flags | 1 | 无有效包、验证失败、owner/低电拒绝、恢复等有限枚举 |
| schedule_hint | 2 | 到下个会合的秒数；仅调度摘要，不改变正式周期/期限 |
| auth_tag | 8 | 候选 HMAC-SHA-256 截断 64 bit，覆盖前 16B 与绑定上下文 |
| 合计 | **24** | 无 token、业务数据、完整 data_seq 或 MAC |

tag 输入含方向域分隔及完整 device_mac、bridge_id、配置摘要，防方向反射；
K 为每对 Bridge/设备专用控制密钥，不能直接广播或复用 bearer token。
新密钥材料只经已绑定加密 GATT 配置；普通 bearer HTTP 不作为密钥保密传输。
后续非密钥策略配置可走当前认证通道，设备安全存储、Bridge 凭据存储，不入仓库/日志。
64-bit tag 是有界窗口及限速下的预算建议，须评审伪造尝试上限；CRC/明文 hasWork 不构成认证。
NO_DIRECTIVE 用设备自己预期 epoch/window 签名，不把不可信来包序号回显为可信状态。

只接受当前配置及本地预期窗口；重复同字节返回同决定，不重开网/续截止，同序号不同内容拒绝。
Bridge 工作在窗口中变化，留到下窗口或认证通道处理，不能同序号先发 ACCEPT_SLEEP 后改 OPEN_WIFI。
过期/未来窗口、旧 epoch、未知目标拒绝。序号按认证锚点和单调经过时间推进，墙钟校时不能移动
已接受窗口；正常 deep 在 RTC 保留进度，不每分钟写 flash。冷启动/保留损坏/序号回卷前强制
device_first 重新认证配置新 epoch/密钥，禁止旧密钥下序号归零；epoch 不复用。
Bridge 重启缺配置同样先恢复，不能猜序号。短身份碰撞由完整绑定和不同密钥消歧，验证失败不算在线。
该最小包不携带墙钟校时：bridge_first 空闲周期用 RTC，认证 HTTP/GATT 或恢复窗口再校时；
schedule_hint 只修正扫描预测，不作校时或 deadline 输入。各字段在本窗口内冻结，避免重复包内容变化。

## 5. 截止、Windows 收发、多设备

以下仅是 spike 起点，未实测定值，不声称 2s wake 已达标。

| 预算 | 候选/规则 |
|---|---|
| 会合周期 | 沿用正式配置 60s；先核对现有整分钟对齐偶见 2 分钟的问题 |
| guard | 候选 0.5–2s，加实测 RTC/PC 调度误差；接收时刻不等于精确发射时刻 |
| T_scan / T_reply | 候选 1–1.5s / 0.3–0.8s，有界重复收发 |
| T_bootstrap | 候选接受 OPEN_WIFI 起 15s，含回复、Wi-Fi、HTTP 认证和正式 plan；设备单调硬截止 |
| PC 单次 HTTP / 间隔 | 候选 ≤1s / 0.2–0.5s，全部重试消耗同一总预算 |
| PC 总等待 | 从窗口开启固定截止，覆盖 guard + T_scan + T_bootstrap + 小网络余量；重复回复不重置 |
| 正式会话 | ACK 回实际剩余期限；大任务先确认正式 PowerPlan 及足够预算再开始 |
| 恢复 | 候选每 10 窗口强制一次 device_first；连续 2 次无有效指令可提前恢复，有界执行 |

Windows Publisher/Watcher 可以同时请求运行，但收发是 best-effort，不能依赖单包或假定所有适配器
可靠全双工/固定发射间隔。窗口内重复同一包，生命周期操作集中在窗口边界。
控制/回复广播均为 non-connectable；角色回调只入有界队列，由主任务校验/开网，不在回调阻塞。
并发不可靠时协商固定 TX→RX 两阶段：PC 先重复指令，再一次性停 Publisher 监听；设备在约定
RX 阶段重复回复。参数经认证配置、计入扫描/回复/bootstrap 预算，不在窗口内高频 Start/Stop。
设备开 Wi-Fi 前完成回复阶段；PC 从 OPEN_WIFI 窗口开始的 HTTP 等待可与监听并行。
Publisher Aborted/能力缺失/系统休眠/Watcher 失败分别记原因，不无限补发；按恢复日程扫描
device_first。无法可靠广播的适配器保持默认策略，仅有扫描能力不够启用 bridge_first。

多设备按 MAC 独立窗口/epoch/待办，单适配器有界公平调度、错开预定窗口；容量不足拒绝新增日程或
认证重新排期，不以共享 hasWork 控制所有设备，不为拥堵延长设备截止。

## 6. 策略切换与恢复

默认 device_first。能力握手确认版本、空口、密钥、时序及恢复齐备才启用 bridge_first。
经当前认证通道提交完整配置（strategy、epoch、安全密钥配置、窗口锚点、预算、恢复日程、未来生效窗口），
设备验证并原子保存后回配置 ACK，当前会合结束后才在指定未来窗口生效；重复同配置幂等、冲突拒绝。
Bridge 收 ACK 前不显示“已切换”；ACK 丢失记 unknown，经认证读取/重试同配置。
过渡期 Bridge 保留旧策略监听，并按已发送候选新日程广播，直至 ACK/认证状态确认；
有限 ACK 交握不能保证双方同时知晓，设备持久配置与固定恢复窗口负责收敛。
周期恢复不依赖是否收到指令，即使设备持续收到有效下行而 PC 持续漏收回复，仍能核对配置。
恢复窗口本身不永久改 strategy；持久回退仍走认证配置 ACK。BOOT 总可 device_first，300s 不变。
密钥/epoch 损坏、撤销配对、配置不支持安全回 device_first；周期会合不开新配对。

## 7. spike 与验收

1. 先测 task-2 后的 device_first，再同负载/PC 网络形态独立 A/B bridge_first，各 ≥30 周期；
   分别记录 RX/TX/connect/discovery/INFO/HTTP/render 时长、全部失败分母及重捕获次数。
2. Windows 并发和一次 TX→RX 退化、实际 31B AD、蓝牙/系统重启、当前适配器成功率；
   无数据 ≤2s、命中率 ≥95% 仅候选目标，非兼容适配器不能默认启用。
3. 注入下行全丢、上行全丢但 HTTP 成功、只有回复/HTTP 超时、两路全丢、重复/冲突/重放/
   错目标/错密钥、owner 冲突/到期、PC 退出、设备冷启动、切换 ACK 丢失、RTC 漂移、多设备排队。
4. 每窗口设备有回复发送尝试，PC 分别记录 beacon/HTTP/业务 ACK；仅成功业务 ACK 更新
   指纹/full_sync_deadline/job/plan。NO_DIRECTIVE ≠ ACCEPT_SLEEP，WIFI_OPENING ≠ 交付完成。
5. PC 崩溃、连续重复包、认证失败、半包、无正式 plan 均不导致无限开网；原正式计划幂等、BOOT、
   pull-only/claim 和显式发布规则不回归。完整回归在后续实现阶段运行。
6. 能耗计入扫描、回复、Wi-Fi 建连/等待、渲染、恢复窗口；无电流仪仅报估算区间，
   不以“回复成本≈0”或跨项目标称续航代替本板证据。

实现门槛：spike、预算/密钥生命周期/合法 AD 标识定值后再安排源码任务；权威总设计已同步，
不代表候选参数已冻结或现有协议已经实现。
本轮只改文档。task-1 §9 的明文 hint 仅适用于旧发现提示，不适用于本控制面；
task-3 的小数据 BLE 验收归 device_first，bridge_first 验收 HTTP 快照与相同业务 ACK，分记 transport。

## 8. 已核对复用点（2026-09-22）

- `bridge/crates/ble/src/lib.rs`：V2Connection::connect/command/close，PC central、MAC 核对、20ms ACK；
  当前注释明确 Windows disconnect 清 GATT 缓存，不能假定可跳过 discovery。
- `bridge/crates/app/src/main.rs`：250ms v2 机会循环/55s 去重，尚非常驻 Publisher/Watcher 调度。
- `bridge/crates/app/src/platform.rs`：ble_cycle/cycle、occupancy_gate、plan_for_rendezvous/note_ack，共用业务入口。
- `src/ble_bridge.cpp`：NimBLE server、绑定/加密、原 GATT/广播；扫描/StatusBeacon 尚需实现。
- `src/main.cpp`：serviceV2Ble/v2Rendezvous/applyV2Plan 与 /v2/*；timer 目前开网依赖正式 plan。
- `docs/generic-display-platform-design-v2.md` §2、§6–8、§10：身份/owner、正式 PowerPlan、ACK、多设备边界。
