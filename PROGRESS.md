# Codex Status 当前交接

更新时间：2026-10-27，Asia/Hong_Kong。

## 当前部署

- Windows默认Bridge已部署Wi-Fi族并重启：`bridge/target/debug/bridge-app.exe`，39009280 B，SHA256 `9077bb8cb338034291612fff5280c95ad6c5be456a2f7f0b4507c12280311cf1`。health204/MCP44及族schema1读取通过；旧exe及286文件data完整备份校验，3设备Profile/许可及Bridge设置保留。[验收](project-workflow/wifi-families/status.md)

- Mac Bridge 已重建/重启：`artifacts/bridge-macos/Codex Status Bridge.app`，exe 79336800 B，SHA256 `d0f0948cafbe3c7a5fd70b9d49b3b0e4809115098e972bed38266119681d35b9`。supervisor/child 正常，8765/8766 监听。新设备默认同步，旧设置保留；AP01/Note4 ACK seq8/1011均已显示。[验收](project-workflow/default-data-sync/status.md)
- AP01 `34F01557210B`：首尾动画修复ROM应用槽1，版本`1.0.2_0041`、build`ec0a58f71f65c2830c08c9bfd7c155db`、Bundle`6a8d702a`。ROM `artifacts/ap01-wifi-animation-20261022/ap01-wifi-animation-candidate01.bin`，7045480 B，SHA256 `5a49f78b550fabbafcc3cde75aa1f6e9986fb9dd88320cd029cd6655d61e0cee`。10-22认证槽1，用户确认原厂界面/旋钮/App及双向快转首尾正常；10-23再次认证同build，未刷机、未复核槽位。
- 救援0031/槽0保留：10-22先认证build`53574b55382d55a514ef5b7767db66d9`及槽0，再单次写应用1；按用户要求未整槽读回/导出，停止标记保留。
- Note4 `.106-note4`认证通过；ROM `artifacts/bridge-macos/note4-0.18.106-plan-activation.bin`，1865680 B，SHA256 `36ca4c3aacdc7b27c9c7cebe4cb18df0bee4048461e189f51050b6332b24ceb4`。10-23未刷机。

## 活跃事项与下一步

- Wi-Fi族Windows已部署：共享族库/逐设备订阅/有线指定、Note/BW现有字段传族名、AP01列表同步与手选。Rust337/定向56、Node/Edge12通过；macOS原生/部署及实机切网归B6。[状态](project-workflow/wifi-families/status.md)。

- AP01 构建已统一到兄弟固件仓库，App 仅源码/配置及兼容入口；独立版/App版、隔离构建和旧命令验证通过，均未部署。[记录](project-workflow/ap01-unified-build/status.md)
- 未部署 platform ROM：`D:\Documents\PlatformIO\Projects\cuktech-ap01-firmware\build\unified-0041\platform.bin`，6875656 B，SHA256 `d77900bf5ce60ac9d1139a37d7a7314e75f2a6ac1f021eb7f04bfd9b440e2859`。
- 未部署 codex-status ROM：`D:\Documents\PlatformIO\Projects\cuktech-ap01-firmware\build\unified-0041\codex-status-commit.bin`，7044408 B，SHA256 `7cf24650ed8b24323e42b1b4a5400f5b05aa0d0f52c81127c2271c2c7d4e4183`。

- 默认同步已提交，core/app/mcp 338通过/5忽略；应用及完整data已私有备份，重建期间data哈希一致。Bridge监督修复已随重建部署。
- C9：共享宿主有效帧、Wi-Fi菜单及首尾循环动画修复已部署；原生红绿/宿主消毒器/菜单回归及用户视觉验收通过，不扩大为全部GPIO或逐帧保证。10-22末轮未切网/改列表，临时UU清理未执行；此前失联后按键重连原因未定。[调查与实机](project-workflow/ap01-wifi-profiles/animation-20261022.md)
- 较早四日审查候选已由上述部署记录接续，不再代表当前ROM；历史候选/资源证据见[审查](project-workflow/review-fixes-20261022/status.md)。物理RF/掉电/完整栈高水位、A9/B6/C8新余量时延/跨会话通知/Windows及长期合盖恢复仍开放，60秒兜底保留。[唯一待办](docs/roadmap/backlog.md)

## 证据

[默认同步前卡](docs/history/progress-archive-2026-10-23-before-default-data-sync.md)、[完整历史](docs/history/progress-archive-2026-10-22-before-review-fixes.md)。旧产物释放27.69GiB，部分历史链接可能失效。实机证据在`artifacts/ap01-entry-rollout-20261022/`、`artifacts/ap01-wifi-animation-rollout-20261022/`；同步备份/证据在`artifacts/default-data-sync-20261023/`。日志/SSID/绑定原文仅留私有artifacts。
