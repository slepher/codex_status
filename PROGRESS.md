# Codex Status 当前交接

更新时间：2026-10-19 20:43 香港时间

## 部署与工作区

- Windows 19:55切网修复已部署：exe SHA256 `66e8e7f2ea64f9569a24ecbe49b6c57c0064d1f6aadc7ff7bd7ccd0a06b937e6`；冻结264文件备份 `artifacts/wifi-switch-backup-20261019-195530`。自然会合认证HTTP恢复、原ID017a completed/revision342，14条旧Job不变，用户确认OFF图标恢复。[验收/回滚](project-workflow/wifi-switch-fix/status.md)。
- Note4 `7C4FADB93408` 已升级 `.105-note4/ota_1`，冷启动保护生成context `443f617ea34aa91a`/commit803，原Bundle Job保留，Data828 displayed。macOS 15:26已部署AP01菜单与族配置；具体模板/部署哈希见[前卡](docs/history/progress-archive-2026-10-19-before-r3-r5.md)。本轮Windows Bridge已重编译/部署，Note4直接OTA已完成；未发布Bundle。
- 用户要求的 `codex/fix-wifi-switch` 已合并至master（`93bb386`）；其worktree已移除，分支保留。R3–R5已提交 `9e748eb`。

## 本轮结果与后续边界

- **B6/C8 R1–R5已部署Windows/Note4、短时验收完成**：删除重复Wi-Fi执行/全局缓存、私有JSON会合状态、同步HTTP worker及旧JSON会话/op路径；共享Job、ConnectionFacts、RendezvousContext、异步HTTP与CommandContext已替换。491回归通过/5条件忽略，合并后App114通过/3条件忽略；两款ESP构建及匹配ELF栈通过，AP01 RV32/宿主ASan通过。[删除清单、迁移、验证与资源](project-workflow/bridge-macos/protocol-convergence-status.md)。已冻结264文件备份；不做旧wire兼容，旧Bridge回滚须恢复备份。15分钟/31采样完成：OTA后7次自然BLE交汇、Data828 displayed，16条Wi-Fi Job/Profile未变，末态Sleep，health204。
- C9：既有AP01候选25单次安装结果未知，禁止重派；本轮C2候选未刷写且flashable=false。物理首启/掉电/LCD/恢复ABI及独立更新未验。[现场](project-workflow/ap01-template-integration/flash-0041-20261019.md)。B6/C8长期RF/原生BLE/Flash实机矩阵仍开放，下一步按[backlog](docs/roadmap/backlog.md)独立验收。

## 镜像与部署

- Note4 **已认证运行ota_1** `0.18.105-note4`：`.pio/build/zectrix-note4-b/firmware.bin`，1850016 B；SHA256 `5433769a050a7fe717f52d3d1b5c0c1f76e66a2ce12b5e4cce1880eee7059608`。
- 1.54候选未部署 `0.18.37-bw`：`.pio/build/esp32-s3-epaper-154g/firmware.bin`，1831776 B；SHA256 `b76365e408f338be39a393c64dcf143da5c4506ff2c9da2b4cc72e2f2397e7a4`。
- AP01候选未部署：`artifacts/ap01-platform-integration/device/ap01-native-0041-c2-candidate.bin`，6994312 B；SHA256 `c2305894caf41af7e30501d5a3c39a8458ff0f0fae179d1b27573e8463b53221`。匹配ELF/manifest及边界见状态文档。

新Windows exe SHA256 `33595924e6d9791e4c9fef1405710b3ce58b9e154855dc400d5f37b807e291ed`；冻结264文件备份 `artifacts/protocol-convergence-backup-20261019-c2`。[部署/15分钟观察结果](project-workflow/bridge-macos/protocol-convergence-rollout.md)。
