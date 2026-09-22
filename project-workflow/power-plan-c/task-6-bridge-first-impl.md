# Task 6 — bridge_first 实现 + 统一 A/B 排期

Status: pending（文档先行；§1 spike 通过前不动协议/生产状态机）。
依赖：task-2（device_first 桥已加固）、task-5（协议候选与验收）、task-4（DFS/btpm 定值）。
原则：先实现，后 A/B；所有臂共用同一采集口径，不靠反复撤/改代码切换。

## 0. 当前实测边界（2026-09-23）

- device_first 已实现；2026-09-22/23 实测：
  - 常驻 adapter + 持续扫描 + connect 期间不停止扫描 → **隔次 connect 两次 4s 超时**
    （host 侧；停扫描后消失）；
  - 保持旧连接不 disconnect（复用 GATT 缓存、跳 discover）→ 同样隔次失败；
  - 现实现：每次机会新建 adapter（Manager 为 ZST）、事件驱动发现、命中即停扫描、
    每周期 disconnect + discover、ACK 20ms、分阶段计时；
  - 15:52:56Z 重启后连续窗口成功（1 次首败重试，无整窗丢失）。
- 现窗口节奏 ~2 分钟/次（整分钟对齐吃掉一分钟，task-5 §5 待核）。
- DFS：`240/80MHz + light_sleep` 已编译生效；A/B 臂未排期。

## 1. Windows publisher/watcher spike（先行，独立代码，不接入生产）

实现位置：`bridge/crates/ble/examples/adv-spike.rs`（dev-dependency `windows`，与 btleplug
同版本；生产 crate 不引 publisher）。输出 JSONL 到 stdout，原始记录进 `artifacts/`。

验收项（task-5 §7.2）：
1. `Start → Started → 首包`：P50/P95（同机 watcher 回环 + 第二接收端/抓包，二者分列）；
2. publisher 与 watcher 并发：同窗口内 watcher 是否仍能收到第三方广播；
3. `Stop → 扫描/连接恢复` 时长（对现设备重复 30 次，记录 P50/P95）；
4. 实际空口 AD 长度与 Manufacturer Specific Data 字节（应用 24B + Flags/长度/类型/
   Company ID），用第二接收端核对，不假设 Windows 附加 AD 与申报一致；
5. beacon 是否 non-connectable（second receiver `IsConnectable`，抓包为准）；
6. 适配器/驱动差异、系统休眠/能力缺失的失败原因枚举；
7. 期间 device_first 监听是否受影响（publisher 运行时连接现设备 ≥10 次成功率）。

Go 条件：1/2/3/5 全部有可重复数据且停止 publisher 后 device_first 不回归；
No-Go：保持 device_first，只登记失败原因，不进入 §3。

### 1.1 首轮 spike 结果（2026-09-23，`bridge/crates/ble/examples/adv-spike.rs`）

运行：`cargo run -p bridge-ble --example adv-spike -- --seconds 20 --rounds 3`
（隔离 target `artifacts/cargo-target-powerc`）；原始 JSONL：
`artifacts/adv-spike-2026-09-23.jsonl`、`artifacts/adv-spike-hold-2026-09-23.jsonl`。

- `Start → Status=Started`：状态序列 `Waiting(1) → Started(2)`，P50 17.5ms / P95 22.3ms（n=3）；
- publisher 运行期间 watcher 仍持续收包：20s 内 386–556 条，170s 内 4034 条（≈24/s）；
- `Stop → 首条广播`：P50 98.9ms / P95 109.4ms（n=3）；170s 轮次 161.8ms；
- **同适配器 watcher 收不到本机 publisher 的 beacon**（3 轮 own_count=0，Windows 过滤自身广播）：
  首包时延/实际 AD 字节/是否 non-connectable 必须用第二接收端（手机 nRF Connect 或设备侧
  扫描；本机仅一块 Realtek 适配器）；
- publisher 运行 170s 期间 device_first 会合 1/1 成功（16:15:03Z，connect 282ms，
  `v2 rendezvous complete`）：最强干扰场景下未见 GATT 回归；样本偏小，A/B 阶段补 ≥10 次。

结论：host 侧能力与恢复速度领先于候选预算（guard 0.5–2s 足够覆盖）。

### 1.2 设备侧第二接收端（2026-09-23，`/diag?blescan=N`，0.17.9-bw）

设备端新增 `bleScanJson`（`src/ble_bridge.cpp`）+ token 鉴权 `/diag?blescan=N&company=` +
串口 `blescan N`。空口与 JSONL 证据：`artifacts/blescan-device-2026-09-23e.json`。

- 扫描 12s：`total=386`、`matched=14`、`scan_end=0`（正常完成）、公司 ID 普查正常
  （101/76/6/1704/911/65535）；PC beacon 的 company=0xFFFF、`conn=false`（**non-connectable**）、
  AD `len=28`、应用 payload 恰 24B、`raw=1bffffff<24B>`（无 name/UUID 附加）；
  与 task-5 §4 的 31B 候选一致。
- 首个匹配包相对扫描开始 271ms；PC 侧 `Start→Started` P50 17.5ms、`Stop→首条广播`
  P50 98.9ms；发布 170s 期间 device_first 会合 1/1 成功。
- 期间踩到并修复的三个实现问题（0.17.3→0.17.9）：
  `NimBLEScan::start()` 单位是**毫秒**且**异步返回**（必须等 `isScanning()` 结束再摘回调，
  否则整段扫描用默认回调 → total=0）；Wi-Fi modem sleep 会饿死共存 BLE 扫描（扫描期
  `WIFI_PS_NONE`）；诊断 JSON 根节点误建为数组导致摘要字段丢失。
- Go：第 1/2/3/5 项均有可重复数据；spike 判定通过，可进入 §2–§4 实现。

## 2. 常量与安全配置（spike 通过后）

- 冻结 24B 应用 payload 布局（task-5 §4）、Company ID 合法途径、config_epoch/window_seq
  推进规则、64-bit tag 输入域（含方向分隔、完整 MAC/bridge_id/配置摘要）；
- 密钥材料只经已绑定加密 GATT 配置；设备/桥凭据存储，不入仓库/日志；
- 在权威设计文档同步“广播仅 hint、短指令不续 deadline”边界。

## 3. 固件（`src/ble_bridge.cpp`、`src/main.cpp`）

1. `SCAN`：timer deep 唤醒后按已认证配置进入有界扫描（期间不连 Wi-Fi），验证 directive；
2. `REPLY`：每窗口必回 `StatusBeacon`（ACCEPT_SLEEP / WIFI_OPENING / NO_DIRECTIVE /
   REJECTED），重复发送、字段窗口内冻结；
3. `OPEN_WIFI`：回复后关 BLE，进入 bootstrap 硬截止的开网/HTTP；`ACCEPT_SLEEP` 按既有
   到期路径收尾；两者都不创建正式 plan、不改 light deadline；
4. 配置/切换：认证通道下发 strategy/epoch/密钥/窗口锚点/预算/恢复日程，原子保存并 ACK，
   指定未来窗口生效；BOOT 始终 device_first；
5. 遥测：扫描/回复/开网/bootstrap 分阶段时长与结果入 `/history`、`/status.json`。

## 4. 桥（`bridge/crates/ble`、`bridge/crates/app`）

1. Publisher 调度器：窗口前 guard 启动，每窗口冻结一条 directive 重复发送；Publisher/
   Watcher 生命周期集中在窗口边界，不在窗口内高频 Start/Stop；
2. 监听 StatusBeacon（有界队列），与 HTTP 并行等待；认证 HTTP 通则继续交付，
   记 `status_beacon=missing,http=authenticated`；
3. 会合结果进入现有 `platform::cycle`/application service/coordinator（MAC、owner、
   状态/正式 plan、Data/Bundle、业务 ACK 全复用）；
4. 策略默认 device_first；认证配置 ACK 后才在指定窗口切 bridge_first，保留旧策略监听与
   周期恢复；失败回退不依赖广播。

## 5. 统一 A/B 排期（每臂 ≥30 个 deep 周期）

共同口径：固定负载与 PC 网络形态（有线/5GHz/2.4GHz 记录）；桥 `bridge_ble=debug` 分阶段
计时；设备 `/history`（`aux/dur_ms/src`）+ `/status.json`（`awake_ms/ble_on_ms/render_ms/
deep.acc_*`）；`node tools/estimate-power.mjs` 输出；`light` 次数、每窗口连接事务数、
首败重试、丢窗口、重捕获单列；每臂前先跑 ≥10 个周期清 OTA/软复位影响。

| 编号 | 臂 | 对照 | 目标/判定 | 归属 |
|---|---|---|---|---|
| A1 | 桥 device_first：task-2 前（`e1e93fc`）vs 现实现 | 同固件 0.17.2/pm-80 | 分阶段计时、丢窗口率、命中率 | task-2 |
| A2 | DFS min_freq 40 vs 80 | 同 base env、其余不动 | `/pmstats` mode residency 是否真降频；`awake_ms`/连接成功率不回归 | task-4 |
| A3 | BT modem sleep off/on × DFS 40/80（2×2，含“经历一次 Wi-Fi light 后”） | task-4 §风险 #15891 | 窗口电流/时长、连接成功率、coex 残余 | task-4 |
| A4 | device_first vs bridge_first | A1 后、同负载 | 无数据 wake ≤2s、命中率 ≥95% 候选目标；失败分母分列 | task-5/6 |
| A5 | 会合节奏：现 ~2min vs 修复后 60s（若实现） | 同臂 | 深睡唤醒次数、thin/rendezvous 比、daily mAh | task-5 §5 |

排期顺序：A1（快速，验证 task-2）→ A2/A3（固件 A/B，一次切 env 跑完）→ A5（视节奏修复）
→ A4（bridge_first 实现与 spike 通过后）。A4 的 No-Go/No-Implement 情形只登记，不阻塞
A1–A3。

## 6. 回归与交付

- 每步：`cargo test --workspace`、`cargo fmt --check`（新增行）、`git diff --check`、
  `node tools/test-quad-preview.mjs`；固件改动另跑 `pio run`（两个 env）；
- 原始日志/CSV 进 `artifacts/`；结果与偏差进 `status.md`；
- 不改 token/claim/owner/MAC 身份语义；未经用户明确要求不提交、不 OTA。
