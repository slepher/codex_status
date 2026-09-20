# sleep-modes 全远程调试流程（2026-09-20 设计并执行）

目标：**不依赖拔插/按键/看屏**，用桥侧调试工具 + HTTP + 日志驱动 deep/light
闭环与端到端验证。物理不可达时的兜底（esptool 复位读 NVS）单独标注。

## 工具（都在桥 MCP `http://127.0.0.1:8766/mcp`，app 侧实现）

| 工具 | 作用 |
|---|---|
| `device_sleep` | 立即强推 `mode=deep`，设备按固件 60s 宽限进深睡 |
| `device_wake` | 让下一次 pull 返回 `light`（并推 `mode=light` 取消待睡），设备回在线可读日志 |
| `device_contact_s {s}` | 覆盖 pull 下发间隔（30–3600s；0=自动 60/900），加速循环 |
| `bridge_status` / `device_owner` | 只读：桥与设备状态/占用 |

设备侧（设备在线时，token 见 `status.md` §5）：
`GET /status.json`（`stage`/`deep{}` 计数/`nvs_stage_boot`）、`GET /log`、`GET /pmstats`、
`POST /diag?idle_deep_s=60&nvs_stage=1[&deep_usb=1][&deep_now=1][&tz=...]`。

判定信号：桥日志 `device entered deep sleep` / `device pull: ... -> mode=... next=...` /
`queued template push flushed`；设备 `deep.clock_wakes/net_windows/net_fails/last_code`；
`nvs_stage_boot`（卡死时复位后读）。

## 标准闭环

1. 会话开始：`device_contact_s s=60`；设备在线时
   `POST /diag?idle_deep_s=60&nvs_stage=1`。
2. `device_sleep` → 桥日志 ~60s 内 `device entered deep sleep`。
3. 观察 deep 循环：每 ~60s `device pull: ... mode=deep next=60`（debug 覆盖）；
   期间 thin 唤醒由“下一次 pull/计数器”间接证明。
4. `device_wake` → 下一次 pull `mode=light` → 设备在线；读 `/status.json` 与
   `/log`，记录 `deep{}` 计数、`stage`、`retry_stage`、`net_fails`。
5. 结束：`device_contact_s s=0` 恢复自动；`deep_usb`/`idle_deep_s` 重置（重启或
   `/diag`）。
6. 卡死兜底（USB 可用时）：esptool 复位 COM4 → 读 `/status.json` 的
   `nvs_stage_boot` 对照 `status.md` §6 码表；USB 不可用则只能等/请人插电。

## P2 用例（按闭环执行）

- **pending 模板**：deep 中 `profile_push default` → 返回“已排队”；下一个 pull
  窗口桥冲刷（`queued template push flushed`），设备回 light 且模板上屏；读
  `/status.json.templates`（active/hash）核对。
- **迟滞**：`device_wake` 后设备驻留 light（`LIGHT_HOLD_S=300`）；静默 ≥600s →
  下一次 pull 回 `deep`。用 `device_contact_s=60` 可在数分钟内观察到切换。
- **OTA 排队**：deep 中 `firmware_ota`（ROM 目标版本需与当前不同）→ 排队 →
  pull 窗口重放（设备 pending 窗口 180s）。测试可用 `artifacts/codex-status-0.14.7-bw.bin`
  降级再升回 0.14.8。
- **手动/BLE/断网/低电/旧桥兼容**：需物理或断网模拟，按 `plan.md` T6 单列。

## 执行记录（本次）

（见下方追加）
