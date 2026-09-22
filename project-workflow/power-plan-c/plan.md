# Plan C — BLE 会合功耗落地计划

目标：把设备"深睡唤醒 → BLE 会合 → 回深睡"的清醒时间与能耗压到可接受范围，
同时保证分钟时钟准确、数据可达、安全语义不变。

## 现状（2026-09-22，基于未提交的 v2 收敛工作树）

| 项 | 现状 | 位置 |
|---|---|---|
| 会合窗口 | 15s | `src/main.cpp:3685` |
| 广播参数 | 未设置，NimBLE 默认 30–60ms | `src/ble_bridge.cpp:327` |
| 窗口省电 | 无（`CONFIG_BT_CTRL_MODEM_SLEEP` 未开） | sdkconfig / `platformio.ini:35` |
| DFS | 已开，min_freq=40MHz（BLE 场景偏激进） | `src/main.cpp:361-364` |
| 时钟 | 会合唤醒跳过 `deepThinWake()` 且不渲染（rv2=1 下会停更） | `src/main.cpp:4512, 4625-4628` |
| 有数据 | 小数据走 BLE，大的走 light；light 最长 600s | `coordinator.rs:390-397` |
| 现场 | 设备 0.16.7 / rv2=0；桥为 14:03 旧构建 | `PROGRESS.md` |

## 冻结参数

| 项 | 值 |
|---|---|
| 会合周期 | 60s |
| BLE 窗口 | **3s 硬上限**；收到回复即提前关（plan ack 后 150–200ms） |
| 广播 | 显式 30–60ms 快广播 |
| 渲染时机 | bridge 回复后或窗口超时后，**只渲染一次**（BLE 已关） |
| 校时 | 回复携带 `server_time` + `tz_offset_min` → 写 RTC；无回复用本地 RTC 外推 |
| DFS | BLE 场景 min_freq 80MHz（等待期自动降频） |
| BT modem sleep | 第二阶段开启（否则 3s 窗口能量不划算） |
| 数据路径 | 不变；小数据 BLE 直传，大更新 light（不做微优化） |

## 时序

```
深睡唤醒（boot，不渲染）
→ 开广播（30–60ms）
→ 等回复（≤3s）
   ├─ 有回复：校时 → ack → 关 BLE → 渲染一次 → deep
   │    └─ plan=light：时钟并入 light 首帧（不重复刷）
   └─ 超时：关 BLE → 本地 RTC 时间渲染 → deep
```

## 预算（1000mAh 电池）

| 场景 | 组成 | awake% | 能量 |
|---|---|---|---|
| bridge 快答（0.3–0.6s） | boot 0.4 + 等待 + 渲染 0.4 + 关 0.05 ≈ 1.2–1.5s | 2.0–2.5% | ~22mAh/天 |
| bridge 不在（3s 超时） | 0.4 + 3.0 + 0.4 + 0.05 ≈ 3.85s | 6.4%（24h 平均仍按 ≤5% 验收） | ~32mAh/天 |
| 无 BT modem sleep 的 3s 超时 | 射频 30–80mA | 同上 | ~50mAh/天（不可接受） |

## 任务索引

| Task | 内容 | 状态 |
|---|---|---|
| task-1 | 固件：窗口/广播/渲染时机/校时/时钟修复/DFS | 已上线 0.17.1 |
| task-2 | 桥：回复带时间、连续扫描、窗口去重 | 已上线（待压时长） |
| task-3 | 遥测与 24h 验收 | 埋点已上线；基线 pending |
| task-4 | 第二阶段：BT modem sleep + DFS 实测定值 | pending |
| task-5 | 广告会合 / 窗口对齐（候选，未冻结） | 调研完成，待 task-3 数据 |

## 调研补充（2026-09-22，社区与同类项目）

**同类项目的节能架构**
- OpenEPaperLink（802.15.4）：标签每 40s+ `check-in`，AP 维护 pending 列表，块级校验/缺失重传，
  “不逐包 ACK、不重复已拿到的数据”；平均 ~9µA。无连接概念，空闲期成本≈射频短听。
- TRMNL（ESP32+墨水屏）：**单向通信**（设备定时唤醒→Wi-Fi pull→deep），官方标称 3 个月/充，
  社区口径“15 分钟一更足够”。纯拉取路线完全绕开会合成本，代价是数据延迟。
- Inkplate/LILYGO/M5Paper：同款“定时 pull + deep sleep”。

**ESP32/社区实测（来源见 task-4/task-5）**
- esp-idf #947：BLE 常连 80–90mA；modem sleep 广播 ≈16.4mA；需 32k 晶振才进 ~4.2mA 档。
- esp-idf #15891：BLE→Wi-Fi→BLE 后残留 ~2mA（coex 资源不释放），混合通道场景需实测。
- btleplug（桥）：#301 Win11 connect 卡死、#360 discover 失败被吞（已修）、#155 Windows 默认读
  GATT 缓存、#182 Peripheral 对象跨断连保留、#453 服务变更后才需重新 discovery。

**BLE 事实（约束设计，不是选项）**
- 广播信道固定 37/38/39（2402/2426/2480MHz）轮发，**不可约定单信道**；扫描方也固定扫三条。
- 一次广播事件 ≈1.6ms 空口；30ms 间隔 ≈1.8%/信道，60ms ≈0.9%/信道。
- 设备“回复广播”的边际成本 ≈ 扫描成本的 0.5–1%（~0.3–1 mAs），远低于一次连接（300–1000 mAs）；
  无认证，只能当 hint。心跳广播可兼任“回复/在线证明/窗口对齐”。

**由此得出的优先级（不改冻结参数，先测量）**
1. 桥侧先把单次 wake 从 6–8.6s 压到 ~3s：常驻 adapter+扫描、复用 Peripheral、已知设备跳过
   INFO、ACK 轮询 80→20ms（详见 task-2）。
2. 设备苏醒广播携带 `next_wake_in_s`（+applied_seq/battery）：PC 用预测窗口替代 250ms 连续扫描，
   并消除“偶见 2 分钟周期”的盲区（详见 task-5）。
3. idle 期“无事不连接、只收心跳/信标”作为 task-5 候选；若仍压不到 ~2s，用 task-3 数据重新
   对照 Wi-Fi 快连（1.3–1.5s+GET）与 BLE 的真实能量账。

## 验收

- 24h 遥测：平均 awake ≤5%（`awake_ms` 求和）；超时场景单列；
- 无数据周期：`light` 次数 = 0、每窗口连接 ≤1、BLE-on ≤3s、时钟每分钟更新且来源正确；
- 有数据：`applied_seq` 前进；小数据走 BLE 不产生 light；
- 停桥对照：3s 超时回 deep，本地时间上屏；
- 回归：模板/协议哈希不变、`cargo test --workspace`、`pio run`、`git diff --check`。

补充验收（2026-09-22 实测后追加）：
- 单次 wake `awake_ms` 分阶段可解释（boot/扫描、连接+发现、命令、渲染），目标：无数据周期 ≤3s、
  超时周期 ≤4s；超限需给出下一轮调参项（task-2 优化或 task-5 方案）；
- BLE 与 Wi-Fi 快连对照各 ≥30 周期（能量=时长×电流），据此决定主通道；
- task-5 启用后：窗口预测命中率 ≥95%（预测窗口内收到心跳）、重捕获次数每 24h ≤2。

## 约束

- 不改 token/claim/owner 安全语义；不新增隐式续航；
- 不 OTA、不部署、不提交、不重启运行中的服务，除非用户明确要求；
- 本计划建立在未提交的 v2 收敛工作树上，代码改动与其合并管理；
- 功耗数字为调研估算，最终以设备遥测/仪器实测为准；
- 广播信道不可约定（BLE 规范固定 37/38/39）；所有“对齐”手段必须在时间维度实现。

## 依据（调研摘要）

- ESP32-S3 DS v2.2：BLE TX 176mA@0dBm / RX 93mA；Wi-Fi TX 283–340mA / RX 88–91mA；light 240µA；deep 7–8µA；
- esp-idf #947：ESP32 modem sleep+DFS 广播 800ms ≈16.4mA、扫描 27–30mA；32k 晶振+light sleep 广播 500ms ≈4.2mA、连接 ≈1.8–2.05mA；
- STM32WB55 DS：广播 1.28s ≈13µA、10.24s ≈4µA；
- SE 121235：连接 500ms ≈26µA、50ms ≈105µA（nRF 估算）；
- SE 160813/TI SWRA347：小数据无 ACK 可用广播；需要 ACK/加密要连接，传完即断；
- 本板 32.768kHz 晶振挂在 PCF85063 RTC 上，未证实接到 ESP32 XTAL_32K_P/N → 4.2mA 档暂不可用；
- esp-idf #15891（2025，open）：BLE→Wi-Fi→BLE 后残余 ~2mA，coex 资源不释放；
- btleplug #301/#360/#155/#182/#453：Windows connect 卡死、discover 失败吞错（已修）、GATT 缓存读、
  Peripheral 跨断连保留对象、服务变更才需重新 discovery；
- OpenEPaperLink（2.4GHz 802.15.4）：标签 40s+ check-in、块级校验/缺失重传、不逐包 ACK，平均 ~9µA；
- TRMNL：单向通信（设备定时 pull、服务端不推），3 个月/充，社区“15 分钟一更足够”；
- BLE 广播空口：一次广播事件 ~1.6ms，30ms 间隔 ~1.8%/信道、60ms ~0.9%/信道；回复广播边际成本
  约为一次扫描窗口的 0.5–1%，为一次连接的 1/100–1/1000。
