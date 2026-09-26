# E1 时钟接线审计（2026-09-26，完成）

本表记录已完成的 Fake ROM D/E/F 实验；验收结果以 `PROGRESS.md` 最新节为准。默认生产 MAC 继续用宿主实时时钟；显式配置的本地单播 MAC 使用 `bridge_core::device_clock` 视图。`/sim/clock` 仅在隔离 Bridge 显式设置 `CODEX_STATUS_BRIDGE_SIM_CONTROL_TOKEN` 后提供。

| 路径 | 时间用途 | 当前处理 |
|---|---|---|
| `device-sim/main.rs` 的 `SimClock` | ROM boot uptime、实验单调、屏幕 wall | 单实例文件恢复；step/rate/wall 分开；boot uptime 重启归零 |
| `core/platform/service.rs` | job/OTA/认证快照、overview、full-sync、节流 | 目标 MAC 的 `device_clock::wall_secs`；持久字段仍是 Unix 秒，不把旧值解释为 uptime |
| `app/platform.rs` | 发布、投递、claim/Plan、OTA、快照与 wake history | 目标 MAC 走 `device_now`；模板/族配置保存仍用宿主 wall |
| `app/main.rs` 的设备缓存与 claim | 30/60 秒在线与续约 | 实验 MAC 使用单调时间；采样/显示时间仍保留 wall；其余系统健康指标仍用宿主 wall |
| `ble/lib.rs::stamp_clock` | `server_time` | 目标 MAC wall；本机 UTC 时区偏移仍来自真实系统 |
| `main.rs` Codex 采集、托盘健康、UDP 抑制、日志与扫描节拍 | Bridge 级任务/真实 I/O | 宿主时间；不由任一 fake MAC 加速 |
| `tokio::time::timeout`、`Instant`、TCP/read/write | 网络/线程 watchdog | 宿主单调时间；实验 step 不能消除真实 I/O 上限 |

复核：Bridge 重启从隔离数据目录恢复每 MAC 实验锚点；运行期 `/sim/run` 与 `/sim/clock` 使用互斥屏障，step 只在协作模式可用。`v2_cycle_targets`/BLE 55 s 扫描按 MAC 选时钟，认证缓存和 claim 期限用单调时间；OTA、快照及平台时间戳保留 Unix epoch 语义。`app/main.rs` 余下真实 `now_secs` 是 Codex 采集、托盘健康、UDP 日志/抑制和真实网络节拍；`app/platform.rs` 余下三处用于模板/Profile 保存，不属于某 MAC 的截止期；BLE `Instant` 与 `tokio::timeout` 是真实 I/O watchdog。F 的独立失步、24 h 联合运行与重启回放结果见 PROGRESS 顶节。
