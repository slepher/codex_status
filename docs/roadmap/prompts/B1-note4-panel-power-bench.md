# prompt — B1：Note4 屏幕供电双模式 + 帧缓存自愈的实机测量

> 对应 `docs/roadmap/backlog.md` 的 **B1**。**本任务不需要改代码、不需要重编固件。**
> 用法：整份交给一个新窗口执行（需要用户在场配合按按键、看屏幕）。

## 你是一个新窗口。先读这些，不要通读仓库

1. `AGENTS.md` — 操作约定（**固件只构建 `zectrix-note4-b`**；后台进程分离启动；不提交密钥）。
2. `PROGRESS.md` — 最新现场。
3. `docs/roadmap/backlog.md` §0 与 §3（B1）。
4. `docs/power-state.md` 里关于面板供电与深睡缓存的段落。
5. 细节背景（已归档，按需查）：`docs/history/workflow/note4-panel-power/status.md`。

**不提交 git**（除非用户明确要求）。

## 关键前提：功能已经在设备上了，你只做测量

**不要重编固件。** 已核实：

- `panel_pwr`（NVS `pm/panel_pwr`，`keep` / `off_cache`）与 `note4RestoreFrameBaseline()`
  都引入于提交 `e226d8e`；
- 该提交**早于** `0.18.23` 的源码提交 `af2b607`——`git show af2b607:src/main.cpp` 里两处都在；
- `af2b607..HEAD` 没有再改动 `src/`；
- → **已 OTA 到 Note4 的 `0.18.23-note4-b` 就包含这两项功能。**

`artifacts/codex-status-0.18.21-note4-b-panel-modes.bin` 是过期的中间候选，**忽略它**，不要刷它。

## 现场

| 项 | 值 |
|---|---|
| Note4 | MAC `7C4FADB93408`，桥登记 IP `192.168.3.177`，已装 `0.18.23-note4-b` |
| 桥 | `bridge/target/debug/bridge-app.exe`（2026-09-25 04:43），MCP 在 `http://127.0.0.1:8766/mcp` |
| 已知良好状态 | 最新 job `1b4500ad` = `succeeded`，`data_seq = applied_seq = 57` |

如果桥没在跑：`pwsh tools/start-bridge.ps1`（必须立即返回）。如果设备 ping 不通，先让用户按键唤醒，再读 `GET http://192.168.3.177/status.json`。

## 命令速查

```powershell
# 读状态（有 token 时带上；纯读 /status.json 与 /log 免 token）
curl.exe -s http://192.168.3.177/status.json
curl.exe -s http://192.168.3.177/log

# 切换面板供电模式（需要设备 token；token 只在设备 NVS，经绑定 BLE 取用）
#   优先用桥内建 MCP 或既有工具，不要把 token 写进任何文件
# 串口兜底：panelpower keep | panelpower off_cache
```

设备侧可读字段：`/status.json` 的 `panel_power_mode`、`epd_busy_fails`、`epd_streak`、
`clk_partials`、`data_seq` / `applied_seq`、电量。

## 测量任务（每项都要留下可复核的原始记录）

### 1. 模式切换与回读
- 用受鉴权路径把 `pm/panel_pwr` 在 `keep` 与 `off_cache` 之间切换各一次。
- 每次切换后回读 `/status.json.panel_power_mode` 与 `/log`，确认：
  - 值真的变了、且**跨一次重启仍保持**（NVS 持久化）；
  - 两种模式下 SSD2683 的内部高压都在两次刷新之间被关掉（看 `/log`，不要只看文档）。

### 2. 深睡帧缓存是否真的被用到
- 让设备进深睡（可借桥的 `power_plan` / 设备 `/diag`），记录 `enter-deep` 前后 `/log`。
- 唤醒后检查：
  - 是否走了**缓存恢复**路径（而不是全刷兜底）；
  - `note4RestoreFrameBaseline()` 是否被走到（它的自愈语义：恢复前先清 `rtcNote4FrameHash`，
    只有缓存与时钟窗口检查全部通过才重新置位）。**这一条以前从未在实机上被证实命中过。**
- 人为制造一次缓存失效（例如模板/context 变化后再深睡），确认它**保守退回全刷**而不是画出错帧。

### 3. 分钟级局刷波形与残影
- 连续观察 ≥30 分钟深睡：记录每分钟时钟窗口是否局刷、`clk_partials` 是否递增、`epd_busy_fails` 是否增长。
- 用固定机位拍照（或用户协助拍摄）记录残影累积情况；`CLK_GHOST_LIMIT` 当前是 **90**
  （`src/main.cpp:118`），观察是否需要在 90 次之前就清一次。
- 拍照时记录：模式（keep/off_cache）、距上次全刷的分钟数、是否刚发生会合。

### 4. 电流对照（`keep` vs `off_cache`）
- 两种模式各测一次深睡静态电流与一次唤醒峰值。测量手段由用户提供（USB 电流表 / 万用表 /
  带计量的电源），**不要凭估算写结论**。
- 记录：档位、量程、稳定时长、读数范围，以及同一次测量里的固件版本与电量。
- 若用户暂时无法提供测量手段，**明确写"未测"**，不要用设计文档里的模型值冒充实测。

### 5. 重复按键与定时唤醒
- 连续 ≥10 次 ENTER 按键唤醒：每次记录 `wake`（应为 `ext1`）与唤醒后是否正常刷新。
- 连续 ≥10 次 timer 唤醒：确认没有出现"缓存基线漂移"或"全刷兜底"的异常模式。
- 注意：**打开串口会复位板子**，会毁掉一次按键唤醒的现场。取证顺序应为
  "先按按键 → 立刻读无代理 `/status.json` 与 `/log` → 最后才考虑串口"。

## 明确不要做

- **不要重编固件**，不要构建 `esp32-s3-epaper-154g`（交替构建会触发 framework 重装，实测单次约 15 分钟量级）。
- **不要刷任何 ROM**（`0.18.21` 候选是过期的；本任务只需要测量）。
- **不要**发布模板、不要 OTA、不要动设备 token。
- **不要**用 `git stash`（本机上它会丢文件——见 `PROGRESS.md` 的事故记录）。
- **不要**提交 git；不要把 Wi-Fi 密码或 token 写进任何文件、日志或你的报告。
- **不要**为了"让数据好看"而裁剪读数：测不到就写"未测"，样本不足就写清样本数。

## 交付物

在 `PROGRESS.md` 追加一节（现场 / 证据 / 待办），至少包含：

1. 两种模式各自切换与回读的结果，以及是否跨重启保持；
2. 深睡缓存恢复是否命中、`note4RestoreFrameBaseline()` 是否被走到（有则给 `/log` 证据）；
3. 分钟局刷与残影的观察结论（含照片文件名，放 `artifacts/`），以及 `CLK_GHOST_LIMIT` 是否需要调整的建议；
4. 电流对照结果（或明确写"因缺测量手段未测"）；
5. 重复按键/定时唤醒的样本数与异常；
6. 如果 `CLK_GHOST_LIMIT` 判定需要从 90 调到 ~30，**不要顺手改**——在 `docs/roadmap/backlog.md` 的 A3 里记下实测依据，代码改动走 A3 那个任务。

## 需要用户配合

- 按 ENTER 唤醒、帮忙拍照、提供电流测量手段。
- 若要构造"缓存失效"场景，需要用户同意改一次模板/Profile。
