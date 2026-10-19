# Codex Status 当前交接

更新时间：2026-10-19 19:59 香港时间

## 部署与工作区

- 独立 worktree `D:/CodexHome/worktrees/wifi-switch-fix`，分支 `codex/fix-wifi-switch`，基线 `4868c8c`。包含 `724f3a3` Wi-Fi事务修复，排除 `ea64ff8` 未部署协议收敛；本轮改动随本次提交保存。
- Windows Bridge 19:55:58已部署：原运行路径 `D:/Documents/PlatformIO/Projects/codex_status/bridge/target/debug/bridge-app.exe`，38,739,968 B，SHA256 `66E8E7F2EA64F9569A24ECBE49B6C57C0064D1F6AADC7FF7BD7CCD0A06B937E6`，PID36464、health204。停机冻结备份 `artifacts/wifi-switch-backup-20261019-195530` 位于原仓库；264个data文件逐一哈希相同，启动前data未变。
- Note4 `7C4FADB93408` 固件仍 `.104-note4`，boot_id保持；无刷机/BOOT/网络配置重写。Windows主机 `wd21-wo` → 设备 `wd21-la`，认证HTTP ready/IP `192.168.3.177`；active `home/v8`、revision342。

## 本轮结果与边界

- Windows旧exe早于已有修复：异步activate ACK后立即断开，漏发network_start，反复window_closed。复用已有修复并补现场失败初态回归；19:57自然会合发network_start accepted，原事务 `w3_000000000000017a` completed/operation_result、回执已ACK。14条旧Job逐字段完全不变。
- 19:58独立读取SSID/MAC、认证HTTP和完整配置快照一致；context `9c3bd636edcd32e4`、commit802保留，Data823 displayed。用户确认Wi-Fi OFF/Bridge OFF图标已恢复。332项Bridge回归通过，5项条件忽略；没有wire或固件变更。[计划/验收/回滚](project-workflow/wifi-switch-fix/status.md)，私有证据 `artifacts/wifi-switch-fix/{before,deployment,verified}.json`。
- 回滚须配套冻结data备份，不能直接让旧exe读取可能含新增确认计数的数据。本轮单次恢复闭环，长期RF验收仍归B6/C8，不关闭专项。
- macOS/AP01及模板此前现场见主线交接和[模板专项](project-workflow/free-month-metering/status.md)；本轮未更新其部署。待办唯一来源[backlog](docs/roadmap/backlog.md)。
