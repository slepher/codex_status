# task-1 — 固件 GET /pmstats + bridge/MCP + 面板「功耗」tab

状态：in_progress（2026-09-19 开工）

## 涉及文件

| 层 | 文件 | 改动 |
|---|---|---|
| 固件 | `src/main.cpp` | `pmStatsText()`、`handlePmStats()`、路由、`FW_VERSION 0.13.0-bw` |
| core | `bridge/crates/core/src/device.rs` | `fetch_pmstats()` |
| MCP | `bridge/crates/mcp/src/lib.rs` | `pm_stats` 工具定义 + 派发 |
| app | `bridge/crates/app/src/main.rs` | `get_pmstats` 命令（10s 缓存）+ 注册 + MCP 工具清单字符串 |
| app | `bridge/crates/app/ui/index.html` | 「功耗」tab（概览/模式条/锁/原始输出） |
| 文档 | `docs/power-state.md` §10、`AGENTS.md`、`PROGRESS.md` | 记录 HTTP 接口与新工具 |

## 验收步骤

1. `pio run` 成功；ROM 归档 `artifacts/codex-status-0.13.0-bw.bin` + SHA256。
2. `CARGO_TARGET_DIR=artifacts/cargo-target-verify cargo test --workspace` 绿。
3. MCP `firmware_ota` 升级设备；`GET http://192.168.1.50/pmstats` 返回
   `Mode stats` / `Sleep stats` / `Lock stats`。
4. 面板「功耗」tab 解析显示（手动采样）；MCP `pm_stats` 返回原始文本。
5. `git diff --check` 干净；PROGRESS 更新（不提交）。

## 证据

- 固件 `pio run` OK（62s + 36s 两次，第二次去掉 IDF `esp_pm_dump_locks` 已内嵌
  stats 导致的重复段）；ROM `artifacts/codex-status-0.13.0-bw.bin`（1587712
  bytes）SHA256 `AEE76FBACC30E58AA9FA5A54940E06B17D18316EF686DBCF5E3A33B764150555`。
- OTA：设备 0.12.6 → 0.13.0（MCP `firmware_ota`）；同版本重传（修复重复段）
  上传成功但工具因"版本未变化"超时——设备实际已重启，`uptime` 可证（待办：
  判定纳入 uptime 重置，或同版本不重传）。
- HTTP：`GET http://192.168.1.50/pmstats` 返回 Lock stats（含内嵌 Mode/Sleep
  stats，单份）。
- MCP：`tools/call pm_stats` 经 :8766/mcp 返回原始文本；工具列表含 `pm_stats`。
- 面板：index.html 新「功耗」tab；`node --check` 语法通过；真实输出正则解析
  测试（node）modes 4 行、locks 5 行全部命中（含 `40 M`、`5 %` 空格格式）。
- 回归：`cargo test --workspace`（隔离 target）16/16 通过；`pio run` OK。
- 桥重建并重启（PID 31700 + watchdog 46848），:8765/:8766 监听，err 日志空。

## 稳态样本（0.13.0-bw，OTA 后 ~273s）

- SLEEP 74%（201.5s）、`light_sleep_counts` 34218（≈125 次/s，均值 ~5.9ms）、
  `light_sleep_reject_counts` 21；锁热行：wifi `APB_FREQ_MAX`(active)、
  rtos0/1 `CPU_FREQ_MAX`。→ 实际是大量微睡眠而非"一秒一次"，之前口头估计
  需修正；后续可用本接口继续观察。

