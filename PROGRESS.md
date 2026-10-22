# Codex Status 当前交接

更新时间：2026-10-22，Asia/Hong_Kong。

## 当前部署

- AP01 `34F01557210B`：首尾动画修复ROM应用槽1/build`ec0a58f71f65c2830c08c9bfd7c155db`，`artifacts/ap01-wifi-animation-20261022/ap01-wifi-animation-candidate01.bin`，7045480B，SHA256`5a49f78b550fabbafcc3cde75aa1f6e9986fb9dd88320cd029cd6655d61e0cee`。已认证版本/槽1；用户确认原厂界面、旋钮、App正常及双向快转首尾不再被动画跳过。
- 救援0031/槽0保留：本轮先认证救援build`53574b55382d55a514ef5b7767db66d9`及物理槽0，再单次写应用槽1。按用户要求仅核身份/版本/槽位，未整槽读回或导出文件。历史完整校验不当作本轮读回；停止标记仍保留。
- Note4 `.106-note4`：`artifacts/bridge-macos/note4-0.18.106-plan-activation.bin`，1865680B，SHA256`36ca4c3aacdc7b27c9c7cebe4cb18df0bee4048461e189f51050b6332b24ceb4`。Mac Bridge 10-20 12:09版；Plan激活/图标实机已验。本轮Bridge未重启。

## 活跃事项与下一步

- C9：原生时序红绿确认旧动画使首尾行约357ms后才完整进入视口，100ms快转先替换高亮。固件仅关闭首尾循环滚动动画，相邻动画保留；宿主消毒器、完整原生菜单/合成驱动回归及预览通过。**现已刷应用1，用户确认双向快转首尾显示正常，此症状实机验收通过**。不扩大为全部GPIO/高速输入逐帧保证。[调查/边界](project-workflow/ap01-wifi-profiles/animation-20261022.md)、[本轮实机](artifacts/ap01-wifi-animation-rollout-20261022/physical-verification.json)。
- 本轮没有切网或修改保存列表，原网络/Xiaomi/临时UU沿用；临时项清理未执行。先前失联后用户按键重连原网的原因仍未定。本轮Bridge未重启，未提交/推送。
- 主仓库基线`f8342fb`/固件`da7a5b5`。共享宿主App保留有效帧修复已有原生红绿/宿主消毒器及实机两侧单格正常证据。本轮及先前日志/修复未提交或推送；Bridge未重启。
- A9/B6/C8：新余量端到端时延、跨会话通知、Windows部署及长期RF/合盖恢复未验，60秒兜底保留。[唯一待办](docs/roadmap/backlog.md)

## 证据

[本轮实机汇总](artifacts/ap01-entry-rollout-20261022/physical-verification.json)、[离线候选](artifacts/ap01-entry-latency-20261022/entry-offline-acceptance.json)、[审查修复](project-workflow/review-fixes-20261022/status.md)。日志/SSID/绑定原文仅在忽略的私有artifacts。旧部署与历史见[归档](docs/history/progress-archive-2026-10-22-before-review-fixes.md)。
