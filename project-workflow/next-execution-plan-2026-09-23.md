# 下一阶段执行计划（2026-09-23）

依据：`PROGRESS.md` 最新节、`power-plan-c/status.md` 与
`power-plan-c/task-6-bridge-first-impl.md`、
`generic-display-platform-implementation/status.md`。本计划只安排工作，未表示实验或
Note4 实机验收已经完成。

## 排序原则

1. 先冻结并采集当前 1.54 英寸设备 0.17.9-bw / `device_first` 的可比较基线，再改会合
   策略或功耗配置。A/B 实验期间记录固件、桥版本、PC 网络形态和设备位置。
2. Note4 已到货，立即开始硬件事实盘点和通电前检查；不再以“等待设备到货”作为阻塞。
   GPIO、波形、存储规格仍须由板卡/厂商资料或实际测量确认，不凭已有骨架猜测。
3. 先让 Note4 的全刷、按键、睡眠、身份和目标防错可靠，再开放局刷、v2 Bundle 与长期运行。
4. `bridge_first` 的 spike 已通过，但生产协议与策略切换仍需实现。只有它和
   `device_first` 都稳定后，A4 的功耗/成功率对照才有意义。

## 执行顺序

| 顺序 | 工作 | 完成判据 | 依赖/注意 |
|---|---|---|---|
| 0 | 固定当前现场：记录 1.54 英寸设备、桥版本、配置与可恢复 ROM；清点 Note4 板号、屏幕排线标识、原理图/厂商 SDK、供电和调试接口 | 两台设备各有可追溯的硬件/固件身份；Note4 缺失资料清单更新 | 不改运行中桥；保存当前基线 |
| 1 | 现有设备 task-3 + A1：连续采集 30–60 分钟及至少 30 个 deep 周期，统计阶段耗时、命中/丢窗、`awake_ms`、`ble_on_ms`、时钟与 `light` 次数；与旧桥对照 | 得出当前 `device_first` 的可信基线，解释 6.2–8.6 秒唤醒和约 2 分钟会合样本 | A/B 维持同一固件和负载；旧桥对照不得覆盖当前运行实例 |
| 2 | Note4 硬件事实与最小 bring-up：确认 EPD/按键 GPIO、SSD2683 时序和两套波形、Flash/PSRAM；审计分区和帧缓存；启用独立 ROM 环境 | 无占位 `#error`，Note4 ROM 可构建；供电、BUSY、全白/全黑/测试图和三键实机可重复 | 缺资料时只推进桥端与宿主工作；首轮不启用 `TARGET_PARTIAL` |
| 3 | A2/A3：DFS 40/80 MHz 与 BT modem sleep 开/关组合，均在现有 1.54 英寸设备上测量 | 每臂至少 30 个 deep 周期；记录连接成功率、时长、功耗估算及一次 Wi-Fi light 后表现，选定配置 | 顺序沿用 task-6 §5；不把 Note4 数据混入同一统计 |
| 4 | Note4 桥端接线与完整 v2 路径：注册 400×300 target，完成画布校验、预览与模板 variant；核实 OTA 双端 target 防错；实机发布 Bundle、推 Data、按键循环、deep 恢复 | 预览/实屏目标一致；错误 target 被拒绝；Bundle、数据与上下文跨重启可恢复 | 在步骤 2 的全刷稳定后进行；保存≠发布及 token/claim/owner 规则保持不变 |
| 5 | A5：确认整分钟对齐造成的约 2 分钟间隔；必要时修正后做相同口径对照 | 会合周期分布、thin/rendezvous 比及每日能耗有前后数据 | 先保留步骤 1/3 的原始基线，再改变调度 |
| 6 | `bridge_first` task-6 §2–§4：冻结认证载荷与密钥/窗口规则；完成设备扫描、StatusBeacon、开网硬截止及桥端 Publisher/Watcher、策略配置和恢复 | 默认 `device_first`；认证配置 ACK 后按未来窗口切换；丢广播/丢回复/重放/HTTP 可用等路径可恢复 | 使用既有 coordinator、owner、Data/Bundle/PowerPlan；短广播不续 light |
| 7 | A4：同负载比较 `device_first` 与 `bridge_first`，并做停桥/失败恢复与长时间运行 | 每臂至少 30 个 deep 周期；记录无数据 wake、命中率、能耗、业务 ACK 和失败分母；据数据选择默认策略 | 步骤 6 完成后进行；Note4 可作为兼容性复核，不替代 1.54 英寸主对照 |
| 8 | 收尾：Note4 波形/BUSY 验证后决定局刷开关；固定机位拍照量化残影；汇总 24 小时功耗与两设备稳定性 | `PROGRESS.md` 记录现场、ROM/SHA256、实验结论及剩余问题 | 局刷和残影照片属面板验收；不得仅凭宿主预览判定 |

## 可并行推进的范围

- 步骤 1 的现有设备数据采集期间，可做步骤 2 的 Note4 板卡资料整理、存储审计和
  桥端 400×300 画布检查；避免同时修改运行中的桥或用于 A/B 的固件。
- Note4 全刷 bring-up 与 A2/A3 可在不同设备上进行，但须分别记录构建产物、日志和
  测试条件。共享桥的功能更改安排在 A1 基线和 A2/A3 对照结束后。
- `bridge_first` 的协议文档、安全边界和离线实现准备可提前；生产切换及 A4 放在
  基线、功耗配置和周期问题收敛之后。

## Note4 到货后仍需核实的事实

1. `EPD_SCK/MOSI/CS/DC/RST/BUSY/EPD3V3_EN` 与 `KEY_PGUP/PGDN/ENTER` 的 ESP32-S3
   GPIO 映射，并核对按键电平及唤醒能力。
2. SSD2683 的 gate、方向/入口、border、温度曲线、全刷与局刷 waveform LUT；确认
   屏幕模组/排线的准确型号，不能只按控制器型号套用波形。
3. Flash/PSRAM 型号和容量、供电/使能时序、USB/串口及 OTA 分区限制。
4. 首次通电和刷机后的实测：供电、BUSY 超时、全刷方向、按键、休眠电流；局刷必须
   等波形和 BUSY 结果可靠后才开放。

每项完成后更新对应 initiative 的 `status.md`；实机里程碑同步 `PROGRESS.md`，
固件发布时记录 ROM 路径和 SHA256。未经用户要求不提交；部署/OTA 按当次实施任务安排。
