# Codex Status 当前交接

更新时间：2026-10-11 16:40（香港时间）

## 部署与工作区

- Note4 `7C4FADB93408` 已认证运行 **0.18.80-note4 / ota_0**，登记IP `.177`，16:27:53完整镜像匹配。ROM `artifacts/note4-0.18.80-zlib-blocks.bin`，1,920,688 B，SHA256 `4CCD39B2C751C44DD6C89527742592656B3495F9700A4121D98BFDEB6C0A7B83`；同名ELF。OTA `0972b435` 单次上传，succeeded/image_verified。USB continuous/light，压缩能力1；旧模板迁移后Data441 displayed。
- Windows默认Bridge已配套更新，PID63064，计划任务启动；exe SHA256 `BD641F1E5ADC0870063B329F77F6734C22F4E22F8AB418185C081850CBE7C2FB`。旧exe/data备份 `artifacts/bridge-before-c9-20261011.exe`、`bridge-before-c9-20261011-data.zip`（ZIP CRC通过）；书桌屏未升级。
- 本轮协议、Wi-Fi元数据与BOOT/PWR修复一并提交；新模板Bundle未发布，当前屏仍用旧模板兼容路径。

## 活跃事项（A9/B6/C8）

- **B6追加交接**：诊断进程已停止设备访问；.80部署身份/结果见上方，BOOT/GATT图标与异常启动仍待[统一执行/验收](project-workflow/bridge-macos/note4-boot-gatt-followup-20261011.md)。手动窗口与自动BLE必须独立；16:08曾确认窗口有效而用户未见图标，异常启动根因仍未知。

- BOOT/联网合同及短时入睡修复已部署：[实施](project-workflow/bridge-macos/boot-network-20261011.md)、[根因/修复](project-workflow/bridge-macos/boot-five-second-investigation-20261011.md)。PWR/BOOT统一300s下界，初始化GPIO0锁存去重，时钟/交汇/底层睡眠入口保窗。新增生产窗口/按键回归、固件模块、52项ELF栈门禁通过；配套Rust回归311通过/3忽略。认证HTTP持续正常、BLE关闭、automatic_ble=false；证据 `artifacts/boot-fix78-running.json`、`note4-fix78-verify-20261011.json`。真实PWR→BOOT重叠按键、timer竞态与功耗矩阵仍需实机验收。

- [历史审核](project-workflow/bridge-macos/protocol-history-review-20261011.md)后完成五项修复：HTTP诊断尾部遵守准入；会合ready与HTTP能力分开；新写入401/403明确失败而历史未知仍查询原ID；保留409修订冲突；wifi_state合同报文本。316测试通过/3条件忽略，末次403补丁真实client7项通过；设备页、52项匹配ELF栈门禁通过。
- 13:10自然恢复会合：ready_report verified→rendezvous complete，Data414 applied/displayed；认证HTTP可用且rv.ready=false，不再错误等待。OTA后此前有connect_timeout及三轮会合失败，原激活结果经查询后自动恢复，不能抹去或归因为新修复已覆盖全部RF故障。证据`artifacts/protocol-ota76-status.json`、`protocol-device-latest.private.json`。
- **下一步/验收边界**：真实10轮自然会合、物理丢ACK/复位矩阵、屏幕闪烁与事件对齐未完成；长期RF/功耗/掉电/动态堆峰值未知。macOS原生通信与1.54未验，不结A9/B6/C8。详见[实施/矩阵](project-workflow/bridge-macos/protocol-implementation-20261011.md)、[计划](project-workflow/bridge-macos/protocol-followup-plan-20261011.md)。

## 入口

- **单份内容/统一压缩（C9）已部署**：Note4复用ROM miniz，CSZ1独立16KiB块、压缩驻留、Bridge冻结字节；下载后停内容覆盖，保留原生OTA。359项通过/3条件忽略，匹配ELF栈检查通过。[交接/限制](project-workflow/single-copy-updates/status.md)。未发布新模板或实测压缩安装峰值；旧故障根因未明。AP01实机/加载器待调查。
- **BOOT/GATT与启动计划统一续项（B6/C8）**：正式计划跨重启、Light/USB走HTTP、Sleep一次BLE、OTA确认已按用户要求补入[统一执行文档](project-workflow/bridge-macos/note4-boot-gatt-followup-20261011.md)。当前仅规划；本轮未完成源码草稿已备份并撤回，未构建/部署.81。待统一实施，.78诊断仍用保存的.78 ELF。

- 待办唯一来源：[backlog](docs/roadmap/backlog.md)
- 前次现场/USB独立事故与详细里程碑：[10月11日归档](docs/history/progress-archive-2026-10-11.md)、[10月9日归档](docs/history/progress-archive-2026-10-09.md)
- 导航与操作：[docs/README.md](docs/README.md)、[agent-operations](docs/agent-operations.md)
