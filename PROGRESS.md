# Codex Status 当前交接

更新时间：2026-10-23 10:14，Asia/Shanghai。

## 当前部署

- Mac Bridge 已从当前工作区重建/重启：`artifacts/bridge-macos/Codex Status Bridge.app`，exe 79336800 B，SHA256 `d0f0948cafbe3c7a5fd70b9d49b3b0e4809115098e972bed38266119681d35b9`。LaunchAgent supervisor/child 正常，8765/8766 监听。新设备默认同步，旧设备保存值保留；AP01/Note4 成功投递 ACK 分别 seq8/1011，均已显示。[验收](project-workflow/default-data-sync/status.md)
- AP01 `34F01557210B`：本轮认证 `1.0.2_0041`、build `ec0a58f71f65c2830c08c9bfd7c155db`、Bundle `6a8d702a`，与前卡应用06不同；本轮未刷机，匹配ROM/槽位未重新核验，不沿用旧哈希冒充当前。救援0031槽0为前次记录。
- Note4 `.106-note4` 认证通过；ROM `artifacts/bridge-macos/note4-0.18.106-plan-activation.bin`，1865680 B，SHA256 `36ca4c3aacdc7b27c9c7cebe4cb18df0bee4048461e189f51050b6332b24ceb4`。本轮未刷机。

## 活跃事项与下一步

- 默认同步改动纳入本次提交，core/app/mcp 合计338通过/5忽略；应用及完整运行data已私有备份，重建期间数据哈希一致。此前已提交Bridge监督修复随本次重建部署；AP01固件修复仍仅候选。
- 四日审查AP01候选 `artifacts/review-fixes-20261022/james-native/review-candidate.bin`，7043256 B，SHA256 `8ee11c4204c2b5df01aa42f3bf97a17eeb2230df705386dfbb0c3729a3f8a5d7`，未刷；RV32/原生菜单/切网夹具/重启恢复已验，物理RF/LCD/掉电/栈高水位未验。[审查](project-workflow/review-fixes-20261022/status.md)
- C9 Wi-Fi菜单与边界迁移固件尚未部署；A9/B6/C8 实际新余量完整时延、跨会话通知原因、Windows及长期合盖恢复仍开放，60秒采集兜底保留。[待办唯一入口](docs/roadmap/backlog.md)

## 历史与证据

[本次前卡](docs/history/progress-archive-2026-10-23-before-default-data-sync.md)保留既有未提交清理记录。[完整历史](docs/history/progress-archive-2026-10-22-before-review-fixes.md)；旧产物已释放27.69GiB，部分链接可能失效。证据/私有备份在 `artifacts/default-data-sync-20261023/`。
