# prompt.md — 新窗口交接：sleep-modes 收尾（时钟/时区同步 + P2 端到端 + 长测）

你是一个新窗口。按顺序读：`PROGRESS.md` 最新一节 → 本目录 `status.md`
§7（P1/OTA/调试工具）与 §6（P0 证据链）→ `plan.md` →
`docs/power-state.md` §13。不提交（等用户确认）。默认行为不要改，调试一律通过
MCP/`/diag` 开关按需开启。

## 现在的状态（2026-09-21 凌晨）

- 设备 `0.14.13-bw`：USB 插电、light 在线、`tz=CST-8`；调试开关（`deep_usb`/
  `frame_capture`/`nvs_stage` 的运行标志）已复位为关。
- 桥 = debug 构建（父 PID **8764** + watchdog **26792**），已含 `device_sleep/
  device_wake/device_mode/device_contact_s` 调试工具；默认 `debug_mode=0`、
  `debug_contact_s=0`，即原逻辑。
- **P0 已修并验证**：唤醒断电（0.14.4 无毛刺释放 GPIO hold）、网络窗口 SPI
  自锁（0.14.6 初始化幂等 + 去重）。
- **P1 已结案（0.14.13）**：进 deep 渲染睡眠图标前强制全刷（`epdPartialReady=
  false`），修掉"Zzz 不显示/与 BT 残影叠加"的局刷基线漂移。用户现场确认
  **睡眠中 Zzz 显示且时钟每分钟更新**。
- **OTA 已修并验证（0.14.12 + 桥）**：中止清理/停滞看门狗/`/diag?ota_abort`、
  探测 10s+重试、响应体 `UPDATE FAILED` 检查、队列退避；`0.14.12→0.14.13`
  一次直传成功。详见 `status.md` §7.2。

## 第一批任务

1. **唤醒时校准 PC 时钟（用户要求）**：pull 响应的 `server_time` 现在取自
   poller 信封（可能滞后数十秒~分钟），设备每次 pull 按它校时 → 与 PC 有偏差。
   在 pull 响应生成时用 `now` 覆盖 `server_time`（`bridge/crates/core/src/
   http.rs` pull 分支；`envelope.rs` 的信封不改）。验证：`deep_usb=1` + 快循环
   下比对面板分钟与 PC。
2. **时区随 PC（用户要求）**：桥在 pull（建议 push 也带）响应里加
   `tz_offset_min`（本机 `Local` 偏移，分钟）；固件 `applyTimezone` 按 POSIX
   反向符号生成（例：+480 → `UTC-8:00`）并持久化 NVS `pm/tz`；缺省仍 `CST-8`。
   涉及：`bridge/crates/core/src/http.rs`、`src/main.cpp`
   `applyTimezone`/`adoptServerTimeForce`。需同步 Rust/Python 测试桥的字段
   （缺省兼容旧桥）。
3. **P2 真机端到端**（用 §调试工具，不必拔线）：
   - deep 期 `profile_push` → 排队 → pull 窗口冲刷（桥日志
     `queued template push flushed`）→ 设备回 light 上屏（读 `/status.json`
     `templates` 的 active/hash）。
   - 迟滞：`usage` 变化 → 下个 pull light；静默 ≥600s → deep/900（用
     `device_mode` 对照调试）。
   - OTA 排队：deep 期 `firmware_ota`（版本需变化，如降级 0.14.12 再升回）。
   - 手动唤醒（BOOT→light、再击→BLE）、BLE 打断、桥不可达、token 失效、低电、
     旧桥缺字段兼容（按 `plan.md` T6）。
4. **深睡切换历史（用户要求）**：现在只有累计计数（`/status.json.deep{}`，
   RTC，掉电清零）与最后阶段码，没有逐次切换记录。规格（用户已定）：
   - **RTC 内存环** 120 条（≈1 分钟 1 条 → 2 小时），掉电清零可接受；
     零 flash 磨损，无需开关。RTC slow 段 7680B，当前 `.rtc.data` 仅 224B，
     120×12B 无压力。
   - 记录点：`boot/唤醒分类`、`enter-deep`（aux=next_contact_s）、每分钟 thin
     唤醒、网络窗口 `net-ok/net-fail`（aux=HTTP code）、`to-light`。
   - 字段（~12B）：`epoch`(u32)、`ev`(u8)、`stage`(u8)、`batt`(u8)、`aux`(u16)。
   - 暴露：`GET /history`（JSON 数组，时间序）；`/status.json` 加
     `hist_count`/`hist_head` 便于桥轮询增量。
   - 验证：`deep_usb=1` + `deep_now=1` 跑 5–10 分钟，`/history` 应有逐分钟
     thin/网络条目；与 `deep{clock_wakes,net_windows}` 计数一致。
5. **长测**：深睡 ≥2h（时钟准度、无花屏/残影失控、电量斜率），必要时观察
   `CLK_GHOST_LIMIT=90` 的全刷清影。

## 调试工具（默认不改行为）

MCP `http://127.0.0.1:8766/mcp`：

| 工具 | 用途 |
|---|---|
| `device_sleep` | 推 deep 并保持（跳过 10 分钟安静迟滞），60s 宽限后睡 |
| `device_wake` | pull 固定 light，回在线读 `/status.json`/`/log`/`/frame` |
| `device_mode auto\|deep\|light` | 恢复/强制 mode |
| `device_contact_s s` | 覆盖 pull 间隔（30–3600s，0=自动） |

设备 `/diag`（token 见下）：`deep_usb=1`（插电可睡）、`deep_now=1`（立即睡）、
`render_mode=deep|light`（在线渲染深睡帧）、`frame_capture=1`（睡前帧存
`/frames/last.pbm`）、`nvs_stage=1`（NVS 面包屑）、`tz=`；`GET /frame`
（`which=frame|last|saved`，PBM 1=黑）；`GET /status.json` 的
`deep{glyph,captures,...}`。

**快速闭环**：`device_contact_s 60` + `device_mode deep` → 设备
`/diag?deep_usb=1&deep_now=1` → 桥日志 `device entered deep sleep` /
`device pull: ... mode=deep next=60` → 需要读数 `device_wake` → 结束
`device_mode auto` + `device_contact_s 0` + `frame_capture=0`。

## 现场与工具

- 设备 `192.168.3.163`（http 80，token
  `3212570061e4d23600173aa04108dbd8`）；SSID `wd21-la`；USB=COM4（插上会复位）。
- 桥日志 `bridge/target/debug/data/logs/bridge-app.log.<date>`；重启桥：先停
  watchdog 再停父进程，`pwsh tools/start-bridge.ps1`；停止前确认是
  `bridge/target/debug` 的进程。
- 构建：`pio run`（勿 `-v`，GBK）；USB 刷写/OTA/回滚命令见 `status.md` §5，
  ROM 表见 `status.md` §6.6（回滚 `artifacts/codex-status-0.13.8-bw.bin`）。
- 卡死兜底：esptool 复位 COM4 → 读 `/status.json.nvs_stage_boot` 对照码表；
  深睡每次唤醒会清 RAM 日志环，取证用 `frame_capture`/NVS 面包屑，别依赖
  串口回放。
- 注意：桥推送会重置设备 idle 与 `deep_now` 的 `forceDeepAtMs`；`device_mode`
  只能在设备在线（light/pending 窗口）时生效。不提交、不落密钥、后台进程
  分离启动且日志进 `artifacts/`。
