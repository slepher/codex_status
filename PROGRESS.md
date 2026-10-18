# Codex Status 当前交接

更新时间：2026-10-18（11:43 香港时间，Windows Bridge首轮用量修复已部署并验首轮；AP01材料已迁出）

## 当前现场

- Note4 **7C4FADB93408**：**0.18.104-note4 / ota_0**，固件未变；Windows默认Bridge已重建部署至`bridge/target/debug/bridge-app.exe`（38,095,360 B，SHA256 `2CDC1AD87048C65CF7BBB2FAD98AC774CE321AC2E39B0F30B73B1A465DBC7CA5`）。原data/凭据保留，主PID32532/watchdog36928，计划任务及8765/8766正常。历史交接按日期存于 `docs/history/`。
- AP01 现场版本仍取用户报告（未访问/刷写设备、未读凭据、未启停Bridge、未开公网/局域网服务）。
- **2026-10-18 结构变更**：AP01 原厂 ROM、逆向报告与知识、QEMU/REA 证据、Ghidra 项目及构建工具已迁至同父目录 `cuktech-ap01-firmware` 仓库；本仓库的 `artifacts/ap01-research`、`project-workflow/ap01`、`tools/ap01`、`firmware/ap01` 已删除，不再保留副本。

## AP01 对接（本仓库职责）

- 对接实现与设计在本仓库完成：Bridge 侧数据源/显示适配、设备端服务与认证、C9 单份更新与压缩。待办仍归 [backlog C9](docs/roadmap/backlog.md)。
- 逆向依据、ROM 候选与模拟验收查 `cuktech-ap01-firmware` 仓库：报告在 `reverse/reports/ap01/`，ROM 清单在 `roms/CATALOG.md`，QEMU/REA 证据在 `evidence/`。
- 该仓库只把文本纳入 Git（约 33 MiB）；二进制、Evidence 与 Ghidra 环境留在本地归档，按 `checksums/SHA256SUMS.txt` 对账。

## 风险与工作区

- **2026-10-18 首轮用量已验**：11:43重启后首轮BLE Data738/CRC`cf770c57`获ACK，认证回读同MAC/.104/applied738/displayed；周剩余98%非missing，仅5h桶两项真实缺失，源重建CRC一致。285回归通过/3条件忽略，部署日志补充后core159通过。本轮首联系时源已ready，冷源等待分支由回归覆盖；未目视验屏。见[A9证据](project-workflow/bridge-macos/first-contact-usage-20261018.md)。
- QEMU 模型是显式合成（PEC/API 表/MACHW/NOR/Boot2 均为模型假设），不证明物理设备；AP01 首刷与恢复路径未验证，结论以 ap01 仓库报告为准。
- 本仓库不再持有 AP01 原件、密钥或 Flash 证据；需要核对时回到 ap01 仓库本地归档。
- 既有未提交改动保留，不回退。操作遵循[细则](docs/agent-operations.md)。
