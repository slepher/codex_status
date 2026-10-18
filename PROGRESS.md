# Codex Status 当前交接

更新时间：2026-10-19 02:07 香港时间

## 部署与工作区

- Note4 `7C4FADB93408` 既有部署 `0.18.104-note4 / ota_0` 未变；生产Bridge未启停/替换，既有exe SHA256 `2CDC1AD87048C65CF7BBB2FAD98AC774CE321AC2E39B0F30B73B1A465DBC7CA5`，本轮未复查PID。
- AP01实机版本、free及物理Boot2未知；本轮只制作/模拟，未访问或刷机、未读真实凭据。用户随后授权提交两个仓库，保存当前未完成验收的工作；prompt.md及本地构建产物不入库，旁仓冻结原件不改。

## C9：自动启动安全窗口已保存，待续作

- 用户要求快速结束并保存，已停止隔离测试。启动预置稳定槽/9秒健康确认/30秒超时、双表交替及旧ROM回执兼容代码已保存并交叉编译；候选23 **6995352 B** SHA256 `4571154b90b2d8e7cddc564fec068501d148a33672ad806fe4295b7296f504f0`，匹配ELF **491388 B** SHA256 `6bd74440cc7660ca13215d16902921efe33f36b637b8bdd2ccf0129f914ca048`。候选23未验收/未发布，宿主重跑与原生53中断；下一步从54干净配置重跑。见[精确交接与续作](project-workflow/ap01-template-integration/boot-safety-handoff.md)。下方候选22仅证明手动功能；不得宣称自动窗口已验证。

- **最新待刷ROM**：[ap01-0041-codex-status-ota-slots-20261019.bin](artifacts/ap01-platform-integration/device/ap01-0041-codex-status-ota-slots-20261019.bin)，**6993160 B**，SHA256 `63eb98e51c9cb1e0b2783d63b51c50da0dadb3db95b1ad107fcbb0fbb970df94`。匹配候选22 ELF 488764 B，SHA256 `d689fbfed009977496ae3631502b7c57a6fbc8fb3c9bab14e32c78c928236a32`。正式manifest/acceptance及ZIP同目录；旧候选16没有直接稳定槽回切，仍留作历史。
- OTA18080新增slots/stable/switch，认证复用绑定token，取消业务owner依赖。显式确认当前稳定槽、升级禁止覆盖稳定槽、不上传镜像回切、原事务查询/响应后重启；业务18081/context/Bundle不参与槽位决策。AP01无PowerPlan，五GIF保留。
- 原生49/50/51同镜像/NOR验证：模板有焦点时完整OTA、新槽直接回切、稳定槽重启确认；真实ff13只提交一次，两槽字节/保护区不变，三轮四场景活动LVGL像素差异0，绑定/Bundle/激活/切槽回执恢复。ASan/UBSan覆盖业务未启动、损坏/变化/重放/unknown/持久失败。修复OTA绘制事件误作输入导致的LVGL断言，未改模拟器绕过。详见[交付与边界](project-workflow/ap01-template-integration/release-0041-ota-slots.md)。
- Bridge隔离候选仍为 `bridge/target-note4-deploy/debug/bridge-app.exe`，38736384 B，SHA256 `808adc1e7307d09eaf2ccebcc4e19bf4c7f1e30bd0d66130724d88fd6a2d5546`，未部署。本次只改AP01原生端口；上一里程碑core162/render15/v2_state21及Note4兼容/栈检查见前卡。

## 边界与后续

- QEMU明确使用MAC/socket/输入/fbdev/Boot2等模型；实际原厂VFS/LVGL与ff13已执行。物理首启/LCD-SPI/free/高水位未验；传统OTA旁路未接稳定槽保护；旧0031接收器直接跨升0041未验证。不得解读为自动救砖或硬件只读槽。
- 本次制作与模拟已完成，不自动刷机/部署Bridge。实机操作需另获授权并核对身份/更新通道；未结项归[backlog C9](docs/roadmap/backlog.md)，macOS WK仍未验。
- [前卡归档](docs/history/progress-archive-2026-10-19-ap01-before-stable-slots.md)。
