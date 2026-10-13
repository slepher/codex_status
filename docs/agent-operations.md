# Agent 操作细则

2026-10-09 从精简前的 `git HEAD:AGENTS.md` 提取。只在执行相关任务前读取对应节；入口规则见根目录 `AGENTS.md`。

以下历史日期、设备地址、版本和测试样本用于说明操作依据，当前部署以 `PROGRESS.md` 和 Bridge 登记为准。2026-09-26 双 ROM 授权只适用于原任务，不构成新任务的其他目标构建授权；执行范围遵循当前会话已授权内容。

## 构建与工具

固件（仓库根目录）：

```powershell
pio run -e zectrix-note4-b            # 默认构建目标；勿省略 -e
pwsh tools/pio-target.ps1 -Target 154g # 仅本次已授权的 1.54 标准 B/W ROM；见下方例外
pio run -e zectrix-note4-b -t upload  # USB 烧录（用网页 OTA 后先擦 otadata：erase_region 0xD000 0x2000）
pio device monitor
```

### 固件目标：默认只构建 Note4；本次双 ROM 实机测试例外

- **默认唯一目标：`zectrix-note4-b`。** 2026-09-26 用户明确授权依次发布 Note4 与 1.54 两份 ROM 并实机测试；本次允许为 1.54 构建且仅构建 `esp32-s3-epaper-154g`，使用 `tools/pio-target.ps1 -Target 154g` 的隔离包目录与完整 sdkconfig 快照。两个目标不得并行构建；不得构建 `-gray4`/`-btpm` 等变体。构建前核对目标，发布前核对 ROM marker、大小、SHA256、登记 MAC 与设备自报身份。该例外不代表以后默认可以构建其他 env。
- **目标切换只通过隔离脚本。** 项目根 `sdkconfig.defaults` 是**全项目唯一**生成物，被各目标争写；pioarduino `arduino.py` 会把当前环境的 `custom_sdkconfig`、MCU 与板卡指纹同该文件首行的 `# TASMOTA__...` 比对，`check_reinstall_frwrk()` 不匹配时会清理生成的 sdkconfig 文件并重装两个 Arduino framework 包。**没有**每目标 sdkconfig 快照时，交替切换必然失配、必然重装（已双向实测，单次约 15 分钟量级；`tools/pio-target.ps1` 现已带该快照，见下方 ③）。多目标并存的正确解法是按目标隔离 `packages_dir`（`tools/pio-target.ps1` 支持 `-Target note4|154g`，按目标生成 `.pio-pkgs/<target>` + 仓库内 `.pio-core`；**两侧都已在 2026-09-25 实测**，见 `docs/history/progress-archive-2026-10-03.md`「多 env 实测」与 backlog C6）。⚠️ 三点实测结论：① 只做包目录隔离**不足以**免重装：判据在仓库根那个共享文件上，切回另一目标会命中 `*** Reinstall Arduino framework ***`（隔离只保证重装发生在目标自己的包目录里、不伤对方）；② 受限沙箱下该脚本曾因 `pio` 继承到错误 cwd（`D:\Documents\project`）而假失败——现已由脚本自身 `Set-Location $RepoRoot` + `pio run -d $RepoRoot` 修掉（历史记录见 `docs/history/progress-archive-2026-10-03.md`）；默认只构建 Note4 是成本约束，本次例外由用户授权。③ 脚本另按目标**整份快照 `sdkconfig.defaults`**（`.pio-core/sdkconfig.defaults.<target>.snapshot`，构建前还原/构建后再存）来消除交替触发的 `*** Reinstall ***`：还原的是该目标自己生成过的完整文件（内容+指纹自洽），**不是** `next.md` §10 禁止的"伪造首行"；安全判据是重建 ROM 与记录值逐字节相同（154g `91937B18…`）。实测 priming 后 note4 39 s / 154g 42 s 交替**均无重装**。priming 那一次仍需重装，且该路径要工作区外写权限（沙箱内需提权）。
- 不要同时运行两个及以上 `pio` 进程；单个目标内部允许 PlatformIO 并行编译源文件。
- **重编范围（2026-09-25 实测）**：改 1 个 `.cpp` → 只重编该 TU + 重链接（note4 46 s）；改被广泛 include 的头（如 `src/template_engine.h`）→ 10 个 TU（41 s）；改 `custom_sdkconfig`/`memory_type`/`flash_size` → `*** Reinstall ***` + 重编 IDF 库（"整个项目重编"那一类）；改 `platformio.ini`（哪怕注释）→ `project.checksum` 失配 → 删 `.pio/build/<env>` 全量重编应用（不连带 framework 重装）；删 `.pio/build`/包目录/`-t clean` → 全量。⚠️ ESP-IDF 把**编译日期时间**写进镜像，所以**任何重编都会改变 ROM 哈希**——要复现记录里的哈希必须"零编译重建"（改完源码记得把 `PROGRESS.md` 的 ROM 行更新成新哈希，别让记录与产物对不上）。
- 保留 `.pio/build/<env>` 与 SCons 缓存，不要因只改应用源码而例行清理或删除构建目录。
- 每次构建后核对环境名、固件版本、产物大小与 SHA256，并把最终 ROM 路径与哈希写入 `PROGRESS.md`。

pm env（M3，pioarduino `custom_sdkconfig`）构建：仓库改名后路径无空格，直接在仓库目录运行 `pio run -e zectrix-note4-b` 即可（若路径再含空格，IDF 会拒绝，需用 junction 且让 pio 的真实 cwd 落在 junction 上）。勿用 `-v`（GBK 控制台会 UnicodeEncodeError 挂住构建）。生成物 sdkconfig*/CMakeLists.txt/.dummy 等已 gitignore。构建环境细节与踩坑见 `docs/history/workflow/sleep-battery/task-7.md` §7。⚠️ 下列这条只属于 `esp32-s3-epaper-154g`（1.54" 板；当前不构建，仅保留备查）：闪存必须 40 MHz（`board_build.f_flash` + `tools/bootloader_40m_fix.py`），否则 GD25Q64 在 80 MHz 下 ID 读错、写入失败。

Rust 桥（`bridge/` 目录；Windows 为已部署基线，macOS 正在实施/验收）：

```powershell
cargo test
cargo run -p bridge-core -- --once          # 拉一次真实数据并打印信封
cargo run -p bridge-core                    # 独立数据源进程，HTTP :8765 只提供 /health
cargo run -p bridge-ble -- --mac <WIFI_MAC> --token <ENDPOINT_TOKEN> --bridge-id <EXISTING_BRIDGE_ID> --once
cargo run -p bridge-app                     # 托盘应用（生产形态，进程内含 HTTP+BLE+MCP）
cargo run -p bridge-render -- --template <json> --out <png>   # 离线渲染对拍
```

BLE CLI 使用已绑定设备的完整 Wi-Fi MAC、现有 endpoint token 和 Bridge principal，走现行 DeviceConnection；不再按名称前缀任意选设备或进入旧 Pusher 后备循环。首次配对使用显式设备配对入口。占位 token 不替换为文档内的真实凭据，不保存带凭据的运行日志。

环境变量：`CODEX_STATUS_PORT/TOKEN/TEMPLATES/INTERVAL`、`CODEX_STATUS_CODEX`、`CODEX_STATUS_DATA`、`CODEX_STATUS_DEVICE_IP/MAC/NAME`、`CODEX_STATUS_BRIDGE_NAME`。

工具与校验：

```powershell
pwsh tools/start-bridge.ps1   # 默认实例：按需 Windows 计划任务 CodexStatusBridge 启动；立即返回，重复调用只打印 PID
pwsh tools/start-bridge.ps1 -Instance note4 -Port 8775 -McpPort 8776 -IconShape circle -DeviceMac 7C4FADB93408 -DeviceIp 192.168.3.177  # 独立实例；需先构建同一 exe
Get-ScheduledTask -TaskName CodexStatusBridge  # 查看默认实例任务状态（无登录自启动触发器）
node tools/generate-quad-preview.mjs
node tools/test-quad-preview.mjs
pwsh tools/test-bridge/start.ps1   # 后台启动 Python 测试桥；stop.ps1 停止
git diff --check                   # 提交前必查
```

### Wi-Fi 配置/事务存储（2026-10-13修订）

当前源码保留WFS4配置编码，事务回执/高水位/执行代际独立WTX1，通过WCM1提交记录原子选择配对。现有完整WFS4直接校验读取；读取不改字节，首次实际持久操作确认成功后退役旧混合store3。不要为此安排WFS4→WFS5转换或要求重新配网。实际部署版本仍看PROGRESS。BRS读取兼容旧逐键/缺身份BRS2，旧条目保留但不授权、不参与投递；完整绑定继续工作。修复旧绑定仅通过物理窗口和绑定加密链路的现行登记，保持已有namespace，缺失时新签发，不猜旧事务身份。原生OTA不以USB导出或绑定库转换为前置。

### 历史离线 Wi-Fi 显式格式归档工具

以下工具仅保留历史WFS5归档转换与取证测试；输出**不是当前设备可部署的存储格式**，不得将其写入新ROM设备。已部署WFS4不需要此工具。


在 `bridge/` 中运行 `cargo run -p bridge-render --bin wifi-store-convert -- <私有输入blob> <新的私有输出blob>`。接受WFS3/WFS4/WFS5及已核对Note4布局的v1/raw-v2/WFZ2，输出WFS5；显式格式复用固件编解码，旧ABI按固定小端偏移读取并校验CRC，不转换为宿主原生结构体。不打开设备、不运行启动恢复、不推进执行代际、不把pending回执改成终态。输入是从NVS提取的完整单个blob，不是分区dump。

输入/输出目录必须已存在并按私有数据管理。工具限制输入8192 B，校验输出后写入排他创建的 `<输出>.pending`，同步与读回核对，再以同目录硬链接原子发布，拒绝覆盖已有输出/暂存文件；只删除本次创建的暂存文件。中断残留或不支持硬链接会明确失败，不自动删除他人文件。CLI只打印长度，不打印密码、token或记录内容。

旧ABI支持范围是Note4 ELF核对的v1 2296 B、v2 5248 B，WFZ2只允许解压为该v2；其他布局拒绝。未确认旧记录不得补猜执行代际或高水位，当前格式校验不成立时拒绝转换。工具不处理Bridge state.json、不替代原TransactionKey/未知作业/高水位的联合清单与可执行围栏验收，也不构成部署授权。源备份、匹配设备身份和转换结果哈希应在私有发布证据中核对；未确认事务缺字段时保持写入阻断，不能靠当前绑定猜测。该段为WFS5候选阶段的历史工具说明。当前运行格式以本节开头的WFS4/WTX1说明为准；旧raw blob和已有零长度键仍明确拒绝并保留，BRS/Bridge身份与真实数据验收独立进行。

## Bridge 启停与实例

- **默认 Bridge 启动**：从非受限（提权）命令运行 `pwsh -File <仓库绝对路径>\tools\start-bridge.ps1`。标准默认实例（8765/8766、方形图标、无设备参数）会注册/复用当前交互用户的**按需 Windows 计划任务 `CodexStatusBridge`**，由任务计划程序启动 `<repo>/bridge/target/debug/bridge-app.exe`；任务无登录触发器、无运行时限，重复启动只报告现有 PID。2026-09-27 实测：旧 `UseShellExecute=true`/`cmd.exe` 虽分离 stdio，退出 Codex 时 Bridge 与 watchdog 仍一起退出；改由计划任务启动后主进程父级是任务计划程序的 `svchost.exe`，8765/8766 正常监听，**关闭 Codex 后持续运行仍待跨会话复核**（见 backlog）。受限沙箱的 `Get-NetTCPConnection` 看不到监听者，启动会假失败；不得从受限工具会话直接起桥。桥日志在 `<exe>/data/logs/bridge-app.log.<UTC 日期>`，计划任务不依赖 `artifacts/bridge-app-run.out/.err`。带自定义端口、设备参数或命名实例仍走脚本原有的分离启动路径，未验证其退出 Codex 后的存活性。其他后台服务仍须立即返回、不继承工具会话的 stdio；不在前台跑长轮询。

- **停止或重建默认 Bridge**：只操作当前 `bridge/target/debug` 对应的进程，不擅自停止别处运行的实例。先结束同 exe 的 `--watchdog <主 PID>`，再 `Stop-ScheduledTask -TaskName CodexStatusBridge`；若任务已是 `Ready` 而端口仍被 watchdog 拉起的主进程监听，再按监听 PID 停该主进程。重建 exe 前确保两者都退出，保留 `<exe>/data/`，构建成功后用上面的脚本按需重新启动。Bridge UI 嵌入 exe：修改 `bridge/crates/app/ui/index.html` 后须在**包含该 UI 改动的工作区**重建并重启；2026-09-26 曾从干净 worktree 构建，导致旧界面覆盖新版。停止/重启前核对路径、端口及任务状态。

- **旧的 watchdog 纯重启路径**：`pwsh tools/restart-bridge.ps1` 只强杀主进程，依赖 watchdog 拉回；2026-09-25 曾在旧分离启动方式下验证，但**改为计划任务后尚未验证任务状态与新进程归属，不要用它代替上面的计划任务启动/重建流程**。watchdog 仅在非零退出码时重启，且 5 分钟内 3 次异常退出就放弃（见 `<exe>/data/logs/watchdog.log`）。构建时运行中的 exe 被锁住，必须先按上述顺序停进程。需要反复重建/启动时，非受限权限可通过本会话权限预设管理；不要为了绕过审批而在受限沙箱中启动后台桥。

- 命名 Bridge 实例必须指定不同 HTTP/MCP 端口；各自运行数据在 `<exe>/instances/<name>/data`，默认实例保留 `<exe>/data`。命名实例不监听设备固定 UDP 8767，只走精确 MAC 的 HTTP/BLE；不同实例 owner ID 不同，同一设备仍由 claim/lease 决定占用。托盘背景形状可用 `-IconShape square|circle|diamond` 区分。

## 运行数据路径

- 便携数据布局：Windows 运行数据 `<exe>/data/`，种子 `<exe>/seed/`（开发回退 `tools/test-bridge/`）；macOS 数据在 `~/Library/Application Support/dev.codexstatus.bridge/data`，命名实例在同根 `instances/<name>/data`，安装身份在同根 `installation-id`，打包种子在 `.app/Contents/Resources/seed/templates`，显式 `CODEX_STATUS_DATA` 优先。程序不向 `.app` 内写数据。⚠️ **`<exe>/data/` 就在 `bridge/target/debug/data/` 里**——`cargo` 报 `os error 5`（`target/debug/.fingerprint/...` 这类沙箱旧文件缺能力 ACE，`Set-Acl` 也改不动）而想"删 `target/debug` 重建"时，**先把 `data/` 备份出去**：它装着 `state.json`（设备登记/Profile/jobs/contexts/plans）、模板库、`device-token-*.json`、`bridge-app.json`（endpoint token）。2026-09-25 有一次真实事故（见 `docs/history/progress-archive-2026-10-03.md`「桥：契约刷新修复 + ACL 闸门绕过」节）：删目录连带删掉运行时数据，靠写回配置 + 从 `tools/test-bridge/` 与仓库夹具重导模板 + 重新登记设备/重建 Profile 才恢复。**更安全的做法是 `cargo build/test --target-dir <仓库内新目录>`**，完全不碰现有 `target/debug`。

## 模板缺省显示

- 2026-10-11最新字段候选：device.state已退役，JSON/持久CT/Bridge/预览均拒绝，不自动兼容；只使用wifi_state、wifi_blink、ble_manual_active、mode等独立字段。旧已安装模板加载不支持时进入内置恢复并报告content_error，文件保留，需显式发布替换。模板为quad v14、Note4 codex-status-a v5；2026-10-11 22:09 Note4新.83镜像、实际v5与配套Bridge已部署并认证核验，见PROGRESS。旧合同升级需先交接有效模板，再切最终Bridge。

- Note4 .81新增本地数值绑定 `device.ble_manual_active=0/1`：图标只表示手动GATT窗口，自动BLE和实际射频状态另记。实际codex-status-a v4已发布（保留用户原布局）；仓库夹具不能直接替代实际模板。数据字段表在同名模板发布后也可能变化，Bridge须认证回读新data_contract再发Data。

- Wi-Fi元数据（2026-10-11源码候选）：`device.wifi_state=idle/connecting/connected/off`；`device.wifi_blink=0/1`仅connecting存在。固件不决定图形；quad v13、Note4 codex-status-a v3由when实现等待隐藏、连接交替闪烁、成功常亮、终态OFF。Bridge图标仍仅依联系新鲜度，不从Wi-Fi阶段推断。新模板需对应支持新绑定的固件，保存不等于发布；.75部署状态见交接卡。

- 显示规则：配额/账号缺失的表现是**模板属性**，由各变体自己的 `when` 分支决定，不用全局规则统一。已实现的两种：200×200 `quad` 在 5h 桶不存在时显示静态 `100` 并隐藏其重置时间；400×300 `codex-status-a` 在 5h 桶不存在时隐藏 5h 块并把 weekly 提升到主位。共享的数据侧约定：桥在 `resetCredits.availableCount<=0` 时不下发该字段，模板按 `exists` 隐藏 RC 行；`bridge.label` 取不到用户名时模板不绘制该行。新增变体按该 target 的产品意图选择分支，并同步更新本行。

## 硬件与串口诊断

**设备以桥的登记记录为准**（`<exe>/data/platform/state.json` 的 `devices[].identity`），启动时不要照抄本文的旧地址：

| 设备 | MAC | 登记 IP | 备注 |
|---|---|---|---|
| 1.54" 200×200 | `70041DD7A340` | `192.168.3.163` | 显示名"书桌屏"；BLE 名形如 `CodexStatus-<MAC后缀>` |
| Note4 400×300 | `7C4FADB93408` | `192.168.3.177` | 显示名"Note4"；曾用 IP `192.168.3.177`/Wi-Fi `wd21-la` |

- ✅ 已核实（2026-09-25）：设备 `/status.json` + ARP 实测 1.54 就是 `70041DD7A340` @ `192.168.3.163`，与本表及 `state.json` 登记一致。本文旧版写的 `192.168.1.50` 与 `70:04:1D:AA:BB:CC` **作废**，不要再引用。1.54 于 2026-09-25 OTA 到 `0.18.23-bw`；2026-09-26 经按 MAC 排队 OTA 后认证状态已见 **`0.18.25-bw`**。Note4 同法已见 **`0.18.25-note4-b`**；精确运行镜像哈希仍未由设备证明，见 `PROGRESS.md` 顶节。
- USB 串口地址动态（Windows COM3/COM4/COM5，macOS 当前常见 `/dev/cu.usbmodem14101`），访问前枚举 VID/PID/serial 并核对登记 MAC。Note4 使用 ESP32-S3 原生 USB-Serial-JTAG（`ARDUINO_USB_MODE=1`），不是普通外置 UART 转接器。
- **串口复位须按证据区分，不得以“可能复位/崩溃”为由跳过用户授权的串口诊断。** 2026-10-04 01:21 的 pyserial 短时读取在预设 `DTR=false/RTS=false` 后仍捕获 ROM `rst:0x15 (USB_UART_CHIP_RESET)`；证据 `artifacts/bridge-macos/restart-event-usb-{capture.json,serial.raw.log}`。这证明该主机/设备/打开流程曾触发 USB 硬件复位，不能泛化成所有串口打开必复位，也不能把它当作固件 panic。2026-10-04 20:36–20:38 在同一Note4 `.59`/macOS上完成对照：原始 `os.open/read/close`（不写modem ioctl）及跳过 `_update_dtr_state/_update_rts_state` 的pyserial各监听15s，boot_id不变；普通pyserial预设false/false后打开立即出现同一ROM复位。再分步对照：在跳过modem设置的连接上单独执行 DTR=false 即触发USB复位，随后RTS=false不触发第二次复位。结论限该设备/主机及当时起始控制线状态，不能推断RTS在所有状态均无影响；pyserial POSIX open 的DTR/RTS依次设置与最终值须分别考虑。
- **保现场串口监听已实测可用**：优先原始port读取（不发送DTR/RTS ioctl）；如用pyserial，须在open前用专用监听子类跳过 `_update_dtr_state`、`_update_rts_state`，并避免open丢弃接收缓冲；不要误以为预设false/false就安全。对照中pyserial打开后清除termios `HUPCL`，关闭前后boot_id仍不变；所有变体先核对MAC，并保存boot身份/复位文本。不要把这次15s样本当跨平台长期保证。复位报告与监听recipe见 `project-workflow/bridge-macos/note4-panic-20261004.md`；完整串口原文/boot_id对照在私有备份 `~/Library/Application Support/dev.codexstatus.bridge/backups/note4-panic-20261004-2027/`。
- **Windows例外（2026-10-07 .69核验）**：跳过上述pyserial `_update_*` 方法并不足以保现场，`serialwin32._reconfigure_port`仍经`SetCommState`配置DCB的DTR/RTS。本轮监听重开期间认证出现USB reset_reason11及boot改变，不能误判为固件panic，也不能将Mac监听子类保证推广到Windows。Windows应使用不改DCB控制线的原生读取，并以打开/关闭前后认证boot_id、uptime及ROM文本验证；未验证前优先网络连续采样。原.68 CRC有效dump的panic另有独立实证。
- esptool 默认/`usb-reset` 进入 ROM 下载模式以及结束后的 `hard-reset` 是**工具主动复位**，与被动监听分开记账；`--before no-reset` 不会让正在运行的应用自动具备 flash 读取能力。打开/关闭串口前后记录时间、工具/参数、控制线策略、boot_id/uptime/reset_reason，并用 ROM reset 文本核验；公共 `reset=unknown` 不能替代 ROM 原因。固件 `panic` / 认证 `reset_reason=4`、WDT、brownout、深睡唤醒和工具 USB 复位分别判断。
- 用户授权后可通过串口只读导出 dump：先保存可获取的网络现场，核对实际分区表及 MAC；当前 Note4 coredump 为 `0xB00000` 起、`0x100000` 字节。先私有保存原始 dump、实际分区表、匹配 ELF 与 SHA256，再校验完整性并解码；dump 可能含凭据，不提交、不完整打印。导出不得顺带擦 dump、写 ROM/分区/NVS；工具进入下载模式及恢复应用的复位单列记录。现有配置允许新 panic 覆盖旧 dump，应优先保全。
- OTA：上传成功后延迟 1.5s 重启，HTTP 先返回 `UPDATE OK`（客户端超时属既有现象）；双槽 ota_0/ota_1 轮换。
- 状态可读：`GET /status.json`（恢复/现场视图）、认证 `POST /api/status`（48 B frame业务状态，不能发送裸JSON）、`GET /log`（统一诊断流视图）与 `GET /pmstats`（PM light-sleep 统计/锁，0.13.0+，只读免 token）。Bridge 注册须核对结构化状态和认证状态的 MAC；旧 HTML 状态页回退与 `claim_unsupported`（固件 <0.13.4）已删除。
- `cargo test --workspace` 可能因运行中的 `bridge-core.exe` 锁定 `target/debug` 失败（不是逻辑失败）；改用隔离 `CARGO_TARGET_DIR` 复测，或核实进程后由用户决定是否停桥。
- Windows 控制台为 GBK：Python 桥启动时设 `PYTHONIOENCODING=utf-8`，避免 status notify 打印异常。
- BLE 写入须按 MTU 分片（usage/模板），单次超 MTU 会 `Invalid Attribute Value Length`。
- 更多历史踩坑（1.54 闪存 40 MHz、Note4 40 MHz bootloader 修 NVS/LittleFS、PSRAM、沙箱 ACL/`git stash` 事故）见 `docs/history/progress-archive-2026-09-25.md` 与 `docs/history/next-2026-09-25.md`，不要凭记忆重做。
