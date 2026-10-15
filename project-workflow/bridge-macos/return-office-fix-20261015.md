# 自动会合预算修复（B6/C8，2026-10-15）

## Plan

用户授权修复、回归测试、部署Bridge及无BOOT自然恢复验收。复用DeviceConnection现有命令预算（单命令5s上限与认证RF绝对期限取小值）；删除应用层700ms外部取消，准备失败明确终止本轮。保持真正未知写入的隔离围栏，不清标志冒充完成。先用现有Fake HTTP分片入口重现真实ble_cycle调用链。

## Task / status

- [x] 回归先红：wifi_match超过700ms仍在合法RF期限，plan/关闭/下一会合可继续。
- [x] 统一预算并传播准备错误；覆盖完整写丢ACK、写未完成及短RF预算耗尽，错误后不得继续发plan。
- [x] 相关Rust回归、diff审查；备份当前App及data并核验。
- [x] 构建与托管部署Bridge，保留data、确认实例和哈希。
- [x] 电池自然唤醒、BLE会合、Wi-Fi认证HTTP恢复，无BOOT；更新交接。

## Review

待验证；不改固件、不主动唤醒设备、不修改Wi-Fi配置。

## 实施与软件验证

真实ble_cycle回归先红：900ms写导致not_sent、后续原生NativeLease隔离；丢wifi_match ACK导致继续plan/sync并返回ble_write_result_unknown。证据wifi-prepare-red-20261015.log。修复后两项通过，包含生产NativeLease/共享disconnect future、下一次会合及HTTP就绪，而非只测辅助函数。

删除700ms准备外层取消；prepare_connection返回错误，失败结束本轮。原10s会合外层timeout改为一次性设置在DeviceConnection上的绝对phase deadline，与每命令5s/设备认证RF期限取最早值；endpoint/CMD共同遵守，后续命令和RF刷新不续期。命令层保留not_sent/WriteUnknown/AckUnknown；未知写不清旗、不复用。工作结束才切到既有尾阶段预算。本机SSID查询无GATT副作用，允许同phase预算取消；不再取消正在写入的radio future。

App96/BLE27/core153及core集成22，共298通过/3原有忽略；日志wifi-prepare-regression-20261015.log。新增phase预算单测覆盖重复命令不续期、RF刷新不能覆盖phase期限、过期和显式阶段切换；现有取消控制写测试继续证明未知写围栏保留。最终补齐本机SSID查询期限后重跑两项入口回归，两项通过（wifi-prepare-final-20261015.log）。UI未修改，未运行UI截图验收；未改固件。

## 部署（10:21）

系统Bash3在空build_args数组与set -u组合下退出，首次打包未修改App即按finally恢复旧实例。修正build-bridge-macos.sh数组展开，Bash3空/debug与--release参数数量自检及bash -n通过。未为此变更构建profile。随后重新停止已核实同路径的托管实例及子进程，确认8765/8766无残留监听，冻结备份App/data/身份/plist，153文件SHA256核验完全一致；构建候选、codesign --verify --deep --strict通过，复制同路径后再次核验与签名校验，启动托管实例。

备份：`/Users/slepher/Library/Application Support/dev.codexstatus.bridge/backups/wifi-budget-20261015-102032`。旧exe SHA256 `50a79f187388bea00c272e611492358e681d7528cfb2c70a0519253a69b9b5d5`；新debug exe 79,432,512 B、SHA256 `4329f4be5fa68aa6e16ddd9ae3052ca5a3375eac7923402f31d5621a6e3bad64`。本次保留debug构建符号，未strip；不可用旧已精简exe字节数推断代码增量。部署前后data哈希相同；部署元数据`artifacts/bridge-macos/wifi-budget-deployed-20261015.json`，构建日志`wifi-budget-build-20261015.log`。新supervisor99172，10:21:12生产启动日志，扫描已恢复；等待自然会合，无BOOT。

## 无BOOT实机验收（10:23–10:25）

用户确认设备距Mac约1m、屏幕正常更新，未按BOOT，10:23已成功会合。生产扫描33于10:23:05自动发现MAC，认证wake_generation951072247/wake_seq1048；status/owner/claim/sync_config/wifi_match/Plan成功。wifi_match写79ms＋ACK61ms，使用剩余4169ms真实RF预算；Plan3999 ACK applied/600s，完整Data617成功。关闭0ms，无新隔离。

本轮SyncOpen丢ACK，network_start完整写后读回失败，均保持AckUnknown/result_unknown；会合HTTP proof当轮预算耗尽仍如实记失败，不能称所有诊断同步成功。随后已有正式Light使设备自动联网，10:23:19网络报告、10:23:21起多轮认证HTTP200；后续独立MCP full status刷新成功，确认.102/MAC/wake1048/timer/reset_reason8（正常深睡）、HTTP ready=true、bridge_reachable=true/error=null、Plan3999剩496s，manual/provisional均0。早期full status分页有transport_timeout，未将HTTP单页200当作完整状态成功；最终完整认证读取通过才验收。

证据：wifi-budget-live-20261015.log、wifi-budget-natural-recovery-20261015.json（已递归脱敏）。固件和分区/NVS均未修改；实机本轮正常wifi_match很快，>700ms的确定性保障由真实ble_cycle慢写注入回归提供。B6/C8长期RF、24h日志和其他硬件矩阵仍开放。

## 最终 Review

复核prepare_connection唯一调用者、carrier所有constructor、phase/RF合并及阶段切换，取消后未知旗保留；无新增依赖。App96/BLE27/core153+22集成=298通过/3原有忽略；最终host查询预算变更后的两项入口回归通过；构建/签名/health204及diff --check通过。未改UI、未运行UI截图测试。按授权部署，无提交。
