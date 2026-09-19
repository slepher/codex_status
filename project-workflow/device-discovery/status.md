# device-discovery 专项状态

- 2026-09-19：建专项（plan/task-1..5）。来源：task-2 发现 mDNS 每 100ms 唤醒
  10 次/s + 桥以 IP 指代设备、桥重启后换 IP 无法自愈；用户新增"设备占用
  claim/lease"需求（task-4）。
- 决策（用户，含修正）：**MAC 为唯一键；名字为可编辑显示名（可重名，显示时
  作为主要对象，MAC/IP 为属性）**；BLE 主动读取为手动兜底；ARP 自动回退；
  关设备 mDNS；**占用只走显式 `/claim`，由 bridge 按设备空闲情况自动决定**
  （用户只做强制接管/释放/恢复）；设备侧继续校验 owner，但不靠 activate/
  BSSID/首个同步等例外转移归属。
- 2026-09-19 完成：task-1（身份/名字/持久化/迁移/改名）、task-2（ARP 回退 +
  BLE info 采纳 + `device_discover` + 面板/MCP）、task-3/4（固件 0.13.4-bw：
  关 mDNS、info 带 mac、`/claim`/owner 校验，已 OTA 并实测占用全流程）、
  task-5（文档/回归）。证据见各 task 文件与 `PROGRESS.md`。
- BLE 端到端已实测（用户单击 BOOT）：`device_discover via=ble` 复用 bond 读取
  info（含 `mac`/`ip`/`http_port`），桥日志 `device info: {...}`。自动 `ble=1`
  通告触发路径本次未捕获到该广播（UDP 广播偶发丢失），机制未变、手动路径已验证。
- 遗留：ARP 邻居表冷路径（真实换网）未复现，留待现场。
- 桥当前为 debug 构建（PID 5260 父 + watchdog），固件 0.13.4-bw（ota_0）。
- 未提交：本目录、`prompt.md`、`PROGRESS.md`、`docs/power-state.md`、固件与桥
  代码（等用户同意）。
