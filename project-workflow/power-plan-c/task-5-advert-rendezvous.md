# Task 5 — 广告会合 / 窗口对齐（候选，未冻结）

Status: 候选（依赖 task-3 的分阶段计时与通道对照；不达标才启动）。
目标：把无数据周期的单次 wake 从当前 6.2–8.6s 压到 ~2s，并让 PC 只在预测窗口工作。

## 背景

- 当前会合：设备广播 → PC 扫描/connect/GATT discovery/3 条命令，Windows central 的 connect+
  discovery 不可压缩到 3s 内，设备被迫给 6s 连接预算，wake 超预算。
- 成本模型：一次广播事件 ~1.6ms；设备“回一个广播”≈ 扫描窗口成本的 0.5–1%，是一次连接的
  1/100–1/1000；空闲期真正贵的是“连接”而不是“广播”。
- 约束：BLE 广播信道固定 37/38/39，**不可约定单信道**；对齐只能在时间维度做。
- 同类参考：OpenEPaperLink（40s check-in + 块级重传、不逐包 ACK，~9µA）；TRMNL（单向 pull，
  3 个月/充，“15 分钟一更足够”）。

## 设计草案

1. **设备苏醒心跳（always-on，成本≈0）**
   - 窗口开始先发 200–300ms 快广播，payload：`mac_suffix(3) + applied_seq(4) +
     next_wake_in_s(2) + battery(1) + flags(1)`（legacy 31B 足够）；
   - 作用：PC 对齐下一次窗口、判断在线/进度；心跳兼任“回复广播”，无工作时不额外 TX；
   - 无认证 → 只作调度 hint，数据/确认仍走 GATT。
2. **PC 对齐（替代 250ms 连续扫描）**
   - 收到心跳记 `t_arrival` 与 `next_wake_in_s`，预测 `t_next = arrival + next_wake - guard`；
   - guard 覆盖 Windows watcher 冷启动（0.5–2s）+ RTC 漂移（200–500ms）；
   - 窗口内保持扫描/连接；连续两次错过 → 短时连续扫描重捕获并重建预测。
3. **v1（低风险）**：有工作时仍在设备扫描窗口内连接——只是 PC 不再全天候扫描。
4. **v2（反向会合，可选）**：PC 在预测窗口前广播非连接性 work beacon
   （`hasWork + seq + time`，payload ≤31B）；设备扫描 1–1.5s：
   - `hasWork=false` → 渲染时钟、回 deep（全程无连接）；
   - `hasWork=true` → 设备回心跳/发起连接，数据走既有 GATT/HTTP 路径；
   - 注意：PC 侧 `BluetoothLEAdvertisementPublisher` 不给间隔/信道控制，且“广播+扫描并发”
     需先实测（部分驱动串行）；beacon 必须 non-connectable，避免被邻近设备连接。
5. **进阶（暂不做）**：把 ≤31B 控制数据直接放广播（无 ACK）；块级广播传大 payload 属
   OpenEPaperLink 路线，超出 S3 现有 GATT 通道，收益与复杂度需单独评估。

## 验收（启用后）

- 无数据周期：`awake_ms` ≤2s（分阶段计时可解释）；扫描窗口 ≤1.5s；
- 预测命中率 ≥95%（预测窗口内收到心跳）；重捕获 ≤2 次/24h；
- 有数据周期：applied_seq 前进、transport=ble 正常；
- 不回归：连接成功率、模板/协议哈希、`cargo test --workspace`、`pio run`、`git diff --check`。

## 风险

- Windows 广播/扫描并发能力未知（需先做可行性 spike，1 天内可判）；
- 预测窗口依赖设备 RTC 漂移与桥时钟质量；错过必须有重捕获，不能“越等越偏”；
- 心跳明文可伪造：只影响调度；若攻击者伪造 hasWork 只会造成多余连接（用量可控），不影响安全语义；
- 复杂度上升：失败时自动回退到“连续扫描 + 连接”的现行路径，禁止半启用状态。

## 依据

- esp-idf #947 / #15891；btleplug #301/#360/#155/#182/#453；
- OpenEPaperLink README（9µA、40s check-in、4096B 块、不逐包 ACK）；
- TRMNL（3 个月/充、单向通信、15 分钟一更）；
- BLE adv 空口：事件 ~1.6ms，30ms 间隔 ~1.8%/信道、60ms ~0.9%/信道；信道不可约定。
