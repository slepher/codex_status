# Codex Status 当前交接

更新时间：2026-10-22，Asia/Shanghai。

## 当前部署（本轮未变）

- AP01 `34F01557210B`：应用06/槽1，build `5757fdd677800d1412d8795c1cb2dbe0`；救援0031/槽0保留。菜单码OTA、切槽/回切、原厂界面/旋钮已验；Bundle `6a8d702a`。ROM `artifacts/ap01-device-code-ota-20261021/ap01-0041-menu-ota-candidate06.bin`，7019304B，SHA256 `82d33c493f7625a2b22588210d6492897af93b4f7eba05d3559abd9f15197e18`。
- Note4 `.106-note4`：`artifacts/bridge-macos/note4-0.18.106-plan-activation.bin`，1865680B，SHA256 `36ca4c3aacdc7b27c9c7cebe4cb18df0bee4048461e189f51050b6332b24ceb4`；Mac Bridge 10-20 12:09版本，exe SHA256 `7ad7ee3049e26cc0ed209f8240cc8eed945a4bc68c4d3b0666dceeedf70d0b40`。Plan激活和图标实机已验。

## 活跃事项与下一步

- **四日审查修复：两仓库修复分别提交、未部署**。维护与自动回退解耦，OTA先确认安全启动槽再写；修正Wi-Fi回退凭据、Bridge子任务监督、菜单顺序及GET内存浪费；收敛工具重复/打包输入。详见[修复与验证](project-workflow/review-fixes-20261022/status.md)。
- Bridge App离线126通过/3条件忽略；AP01 Mac/Linux ASan/UBSan、边界检查通过。james已完成GCC8.4 RV32审计/链接/重建，菜单Context 52B；原生QEMU菜单/切网夹具/重启恢复通过、原厂RGB565截图已审阅，静态栈不等于实机高水位。**仅候选、未部署**：`artifacts/review-fixes-20261022/james-native/review-candidate.bin`，7043256B，SHA256 `8ee11c4204c2b5df01aa42f3bf97a17eeb2230df705386dfbb0c3729a3f8a5d7`。仍不刷机。
- C9：此前Wi-Fi菜单候选03、共享宿主及边界迁移均未部署；本轮源码已更新，旧候选不含修复。RF/LCD/物理掉电/高水位仍开放。[Wi-Fi专项](project-workflow/ap01-wifi-profiles/status.md)
- A9/B6/C8：真实新余量完整时延、上游跨会话通知原因、Windows部署与长期RF/合盖恢复未验；现有60秒采集兜底保留。[待办唯一入口](docs/roadmap/backlog.md)

## 历史与证据

[完整前卡与部署哈希](docs/history/progress-archive-2026-10-22-before-review-fixes.md)。本轮未访问实机、未重启Bridge、未修改运行data。
