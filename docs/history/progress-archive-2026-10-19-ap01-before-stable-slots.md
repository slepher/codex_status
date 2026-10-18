# Codex Status 当前交接

更新时间：2026-10-19 01:30 香港时间（独立OTA稳定槽补充最终模拟中，未刷机）

## 部署与工作区

- Note4 `7C4FADB93408`：已部署 `0.18.104-note4 / ota_0`，本轮未访问或刷写。生产 Bridge 未启停/替换；既有 exe SHA256 `2CDC1AD87048C65CF7BBB2FAD98AC774CE321AC2E39B0F30B73B1A465DBC7CA5`，本轮未复查 PID。
- AP01 实机版本与剩余空间未知；本轮仅制作及模拟，未读实机凭据、未刷机、未提交。既有工作区改动及 prompt.md 保留。原厂0041/逆向冻结资料位于旁仓 `../cuktech-ap01-firmware`，只读使用。

## C9：0041 待刷产物已完成模拟验收

- **当前续作**：用户要求稳定槽回切放独立OTA服务。候选22已冻结：6993160 B，SHA256 `63eb98e51c9cb1e0b2783d63b51c50da0dadb3db95b1ad107fcbb0fbb970df94`。OTA18080新增slots/stable/switch，取消业务owner依赖；确认/禁止覆盖/无上传回切/持久回执已实现。候选21的47暴露LVGL绘制事件误入OTA输入分支，后续panic ROM API缺口不是首因；22已筛选输入，49/50/51最终模拟执行中，覆盖有焦点模板时OTA/回切。完成后accept-ap01-rom.py --candidate 22 --fresh 49 --update 49 --switch 50 --recovery 51生成新正式包；不以旧16或21证据代替。详见[稳定槽专项](project-workflow/ap01-template-integration/ota-stable-slots.md)。

- ROM：[ap01-0041-codex-status-20261019.bin](artifacts/ap01-platform-integration/device/ap01-0041-codex-status-20261019.bin)，**6981976 B**，SHA256 `6adb31dcaa0a35c77a38c729d298764f0c72ae7e729b6c77a21dd0b7beeb56bc`。原厂0041基线、匹配候选16 ELF/map/audit/link及验收 manifest 同目录；不是已部署版本。
- 原生维护18080/业务18081/UDP8767、已有配对、完整Bundle/原子激活/Data、原厂UI正反切页和独立维护全镜像更新已接线。常在线无PowerPlan；缺5h自动重排、RC独立保留，五个GIF保留。
- 证据：`native-qemu-31/32/33.json` 分别验证8模板安装/画面、完整ROM写入非活动槽及保护区、从同一NOR新槽恢复。四场景活动LVGL缓冲像素差异0；宿主ASan/UBSan、core162/render15/v2_state21、8种存储容量组合通过。详见[完整交付与边界](project-workflow/ap01-template-integration/release-0041.md)。
- 隔离 Bridge 候选 `bridge/target-note4-deploy/debug/bridge-app.exe`，38736384 B，SHA256 `808adc1e7307d09eaf2ccebcc4e19bf4c7f1e30bd0d66130724d88fd6a2d5546`，未启动/部署。
- 共享兼容构建：Note4 `.104`，1850992 B，SHA256 `e6345841885aedc5e3d9ce5c0d49f329e52ebcaaa4271feb9d0f971b53dbe0fa`；匹配ELF及Wi-Fi栈检查归档在同一device目录，未上传。

## 验收边界与下一步

- QEMU明确使用MAC/socket/输入/fbdev/Boot2等模型；实际原厂VFS/LVGL及RV32应用已运行。物理首启、LCD/SPI、实际free和堆栈高水位未验；不以模拟替代实机。旧0031独立维护接收器直接跨升0041未验证。
- 本次“制作ROM并模拟、不刷机”已完成。后续实机刷写需另获授权，先核对身份/当前接收通道；不自动部署Bridge。未结项统一归[backlog C9](docs/roadmap/backlog.md)，不关闭实机及macOS WK验收。
- 前卡完整归档：[2026-10-19](docs/history/progress-archive-2026-10-19-ap01-before-release.md)。
