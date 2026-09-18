# PM Stats over HTTP + Bridge「功耗」Tab

用户需求（2026-09-19）：免 USB 读取设备 PM light-sleep 统计；bridge 面板加
「功耗」tab。权威约束：`AGENTS.md`（token/构建/显式动作）、`docs/power-state.md`
§10（pmstats 仅为调试 CLI 的历史描述将在本专项后更新）。

## 目标

1. 固件新增只读 `GET /pmstats`：返回 `esp_pm_impl_dump_stats`（Mode stats /
   Sleep stats）+ `esp_pm_dump_locks`（Lock stats）文本；FW `0.12.6-bw` →
   `0.13.0-bw`。
2. `bridge-core::device::fetch_pmstats(ip, timeout)`；MCP 新增 `pm_stats` 工具。
3. 面板新增「功耗」tab：休眠占比 / 唤醒次数 / 平均每次休眠 / 拒绝次数 /
   CPU 模式时间条 / PM 锁表 / 原始输出；手动采样。
4. 设备 OTA 到 0.13.0-bw 并实测，PROGRESS 记录 ROM SHA256。

## 决策

- 鉴权：`/pmstats` 与 `/log`、`/status.json` 一致**免 token**（只读诊断，无
  密钥；写入类路由维持 token 门控不变）。
- 采样节流：服务端 10s 缓存，前端 tab 打开 + 手动「采样」，≥15s；**不参与
  3s 轮询**——读取本身会短暂唤醒设备，高频采样会污染测量。
- 工具名 `pm_stats`（只读，可选 `device_ip`）。

## 任务

### task-1 固件接口 + core/MCP + 面板 tab

1. 固件：抽出 `pmStatsText()`（CLI `dumpPmStats` 复用）；注册 `GET /pmstats`。
2. `core/device.rs`：`fetch_pmstats`，非 0.13 固件返回清晰错误。
3. `mcp`：`pm_stats` 工具定义 + 派发。
4. `app`：Tauri `get_pmstats`（10s 缓存）+ `ui/index.html` 功耗 tab。
5. 验证：`pio run`；`cargo test`（隔离 target dir）；MCP `firmware_ota` 升级；
   HTTP 直读 `/pmstats`；重启 bridge 后 MCP/面板实测；回归 `git diff --check`。

DoD：设备 0.13.0-bw 可经 HTTP 读 pmstats；面板 tab 可解析显示；无回归；
PROGRESS 证据齐全。不提交（等用户同意）。

### task-2 微睡眠调研（进行中）

「功耗」tab 实测 ~125 次/s、均值 5.8ms 的 light sleep，调研其成因与优化
空间（官方/社区资料、下一步实验见 `task-2.md`）。
