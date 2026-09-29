# Codex Status 项目进度（交接文档）

## 无桌面干扰的软件回放（2026-09-29 香港时间）

- 用户要求继续可在后台完成的验证。新增 `tools/run-sync-headless.mjs`，在 ignored 目录启动两个隐藏 Fake ROM 和一个命名隔离 Bridge（独立 loopback 端口、实例 data、模拟 MAC），真实 Bridge 客户端经 BLE/HTTP 与设备协作；`tools/fake-rom-runner.mjs` 计入一次性 sync 开网、可排正式 light Plan。测试夹具为首轮 HTTP 基线在**隔离实例**预置已知假设备操作 token；该步骤不是 DeviceConnection 取 token 的验证。所有测试进程完成后退出；未操作桌面、默认 Bridge 或真实设备。最终测试候选 `bridge/target/rollout-20260928/debug/bridge-app.exe` SHA256 `BD7462B4F91327FF378F7387AA229B7D1A3CA63A6D76EEA6F8E85BFD95BC11C5`；默认 EXE 仍为下节的 `D049C281…`，主 PID 18332/watchdog 28596、计划任务 Running、8765/8766 均正常（测试前后只读核对）。
- 双 target **虚拟 24 小时**：最终候选的 `artifacts/rollout-20260928/sync-headless-fHGxy0/summary.json` / `trace.ndjson`；2,785 个事件、两台各达到 rounds=15、client serial 1–97 连续、各 97 份完整归档，最新归档大小/SHA256 与 complete checkpoint 一致。各 1 个 gap 来自基线新代际迁移，后续周期批次无 gap；隔离 Bridge 日志 0 warning/error。虚拟时钟按事件推进，实际耗时数分钟，不代表 24 小时真实 RF/电源运行；此场景未注入 light、进程重启或故障。
- 单独 light 场景 `sync-headless-Fr7aoo`：第 10 虚拟分钟排正式 light Plan，A 的 3 份归档含 `light_enter`、`light_exit`，最终回 deep；B 独立达到第 15 轮并完成周期同步。最初脚本把 light 重置轮数误套周期断言导致一次假失败；修正后重跑通过。light 日志有 1 次状态探测超时和一次直连 Plan 超时/发送失败，随后队列 Plan 经 BLE 实际生效；这仍是待观察的离线直连边界。Bridge 进程重启场景 `sync-headless-8pkmTS` 和 Fake ROM A 进程重启场景 `sync-headless-P9La9c` 均从原隔离 data 继续后续批次；重启切点在基线 complete 后，**不**证明分页中途/完成意图中途的强退恢复。
- 同 seed 的两次独立虚拟 1 小时运行 `sync-headless-PkuJJk` / `sync-headless-ePk3ev`：各 117 事件、两台各 5 批；原始 trace SHA256 均为 `598B32A7FB6C3B4ADC7FB57BF9D99F397DCA96DB650A1451CC5C02E95A02C297`。10 份归档去除实例 Bridge ID 后的内容哈希、诊断记录字节哈希一致，每次自身原文件 SHA 已分别由 harness 对 checkpoint 核验。这是独立初始化回放，尚非 F3 要求的同一静止磁盘快照复制回放；Frame CRC 因无 Bundle 均为 0。
- 长跑暴露出一次性 Wi-Fi 完成后 Bridge 无条件再次尝试传输会等约 34s；隔离候选现改为 complete 后仅在可持续的 light 窗口用 1s 认证状态探测仍有的义务，深睡一次性窗口直接结束，并避免 sleep Plan 后无有效 light 窗口时再做第二次同步。最终候选 `cargo test -p bridge-app --bin bridge-app --target-dir target/rollout-20260928` 43 通过/1 ignored，`node tools/test-device-page.mjs`、两脚本语法、`git diff --check` 通过（仅 CRLF 提示）。这些 Bridge 修正**尚未部署到默认 EXE**。
- 尚待：F3 异步 virtual_wait/延迟页与 next_event 调度、同一磁盘快照回放、S01–S14 全矩阵及故障切点/分页中途强退、U01–U20 全矩阵、最终 Tauri 窗口视觉与必要硬件烟测。窗口检查不可用不阻塞上述后台验证；本轮未做固件构建、OTA、真实设备写入或默认 Bridge 重启。唯一待办见 `docs/roadmap/backlog.md` C8。

## 书桌屏诊断恢复与设备页运行版（2026-09-29 11:45 香港时间）

- 默认 Bridge 已从当前工作区隔离构建并按 watchdog→计划任务顺序更新。安装路径 `bridge/target/debug/bridge-app.exe`，SHA256 `D049C28122136E9821D722AF6183B3DE57829CE85EC35025F81A570FA0CA338C`；更新前 EXE 另存 ignored `artifacts/rollout-20260928/bridge-app-pre-device-tab.exe`。当前主 PID 18332、watchdog 28596 均为同一 EXE；`CodexStatusBridge=Running`，8765/8766 均由主 PID 监听，`bridge/target/debug/data/platform/state.json` 保留（核对时 146837 B）。PID 只是当时现场，复查须重新查路径和端口。
- 书桌屏旧 checkpoint `1-cc96959b5a75e68e9559837e5eada4fd` 原为 `awaiting_device_ack`、981 B，设备已不再持有旧批次。新增恢复逻辑只在认证 `/api/status` 的 MAC、旧归档身份/长度/SHA、device pending/last_completed 与 serial 核对后写 `retired-lost-1-cc96959b5a75e68e9559837e5eada4fd.json`（原因 `device_batch_lost_without_ack`），原文件仍在、SHA256 `E7E82E3357E107979F5E266BD86F821B98A897C418E89AD1F7A9BE4C57759187`；未把旧批次伪写为设备完成。随后真实设备新批次 `2-93bb2b0a8b21fd0a87b0865f4e313a00` 归档 6121 B，SHA256 `A43956186831D1B120FF21AD087F689DC3E5856A0EFF8181D03F4C0467FC39F5`，checkpoint 为 `complete`、offset=6121、client_serial=2；Bridge 代码仅在设备 `complete` 应答后标记此状态。11:39 后又有同 MAC 的认证 BLE status/Plan ACK，会合继续。设备页的完整 Wi-Fi 快照仍显示旧采样时间；不得把 BLE 联系说成新 Wi-Fi 快照或长时稳定性证明。
- 设备 Tab 源码现移除屏幕内容、模板/Profile 编排、字体编辑与发布操作，保留独立的数据投递许可、已安装/active/显示结果/任务只读信息和固件、连接、运行、显示四组详情；模板 Tab 和数据 Tab 布局未新增迁移。数据许可只持久修改该设备 `sync_enabled`，不发布 Bundle。字段显式 null+reason 保留原值及原采样时间。实际 Tauri 旧布局已目视查到概览卡换行过长并修成主值/采样年龄两行；**重启后的最终窗口仍待再次打开目视复查**。
- 软件证据：`node tools/test-device-page.mjs` 通过；`cargo test -p bridge-app -p bridge-core -p bridge-mcp --target-dir target/rollout-20260928` 为 app 43 通过/1 ignored、core 94 加集成 21、MCP 3 通过；显式运行 ignored `sync_v1_bridge_archive_against_note4_fake_rom` 1/1；`cargo test -p device-sim --test bootstrap --target-dir target/rollout-20260928 -- --test-threads=1` 44/44。Fake ROM 新增 S10 同版本不同字节的 active 运行镜像测试，真实运行分区哈希则已有上节 Note4 实机证据。`git diff --check` 通过（仅 platform.rs CRLF 提示）。完整 S01–S14、双 target 24h 与 U01–U20 矩阵尚未验收；不将这些局部测试记为软件最终出口。此轮无固件源码改动、构建或 OTA，实机 ROM/精确哈希沿用下节。

## OTA 恢复路径简化与双设备部署（2026-09-29）

- 用户确认：正确设备操作 token 应能直接 OTA 一个有效 ESP 镜像；A/B 分区使刷写失败可恢复，旧 sync arm/ticket 不应成为故障恢复门槛。新版固件 `/doUpdate` 保留 BLE 签发的操作 token 与 ESP `Update` 镜像校验，移除强制 arm/ticket/owner；正常 Bridge 任务仍在上传前核对登记 MAC、目标、冻结镜像大小与 SHA256。固件增加仅凭操作 token 读取运行分区指定前缀 SHA256 的 `GET /api/ota/image`；认证 `/api/status` 宣告 `ota_auth=token`。开机删除 NVS 中旧 `sync-ota/arm`，以免 A/B 回退时旧 arm 重现。旧 ROM 到新 ROM 的这一次过渡仍按旧门槛执行；新版 Bridge 按能力选择旧/新路径。方案及验证项见 `project-workflow/device-sync-diagnostics/ota-recovery.md`。
- Rust `cargo test -p bridge-core -p bridge-mcp -p bridge-app --target-dir target/rollout-20260928` 全通过；新版 Bridge 隔离构建成功，`bridge/target/rollout-20260928/debug/bridge-app.exe` SHA256 `74C6228946D3749A67035F9FFD9B9E9260EBBAFC418E9B54D88BD0353624FDB1`。旧生产 EXE 已备份到 ignored `artifacts/rollout-20260928/bridge-app-pre-ota1.exe`，SHA256 `D4CAD6733F47971A71126337365D5D55B197EB8C2682700A4BB5907B87A7DA89`；按 watchdog→计划任务顺序停旧进程，替换并按计划任务启动新版。`bridge/target/debug/data/platform/state.json` 仍在（134,151 B，安装前核对）；最终默认 Bridge 主 PID 10760、watchdog 34496、计划任务 Running，8765/8766 均由主 PID 监听，EXE SHA256 与隔离产物一致。
- Note4 最终 ROM `artifacts/rollout-20260928/note4-0.18.35-ota1.bin`，1,648,624 B / SHA256 `41882149824B917D252046721474684BFF6193C520625A4C1D0A3B6A7AF505E4`，marker `codex-status-ota-v1|zectrix-note4-400x300|0.18.35-note4-b-ota1`。通过旧 ROM 的真实诊断确认清除旧 arm 后，认证 OTA `UPDATE OK`；同 MAC `7C4FADB93408` 自报版本更新、`ota_0`→`ota_1`、软件重启，`ota_auth=token`，新接口读取的运行镜像前 1,648,624 字节 SHA256 与候选完全相同。随后在新版上**不调用 arm、不申领 owner、不附 ticket、不带 target 参数**，仅用正确设备操作 token 对同一 ROM 做一次直接 OTA：HTTP 200 `UPDATE OK`，同 MAC 从 `ota_1`→`ota_0`、软件重启、版本仍 `0.18.35-note4-b-ota1`，独立运行镜像前缀 SHA256 再次完全匹配。这是新版 token-only 恢复通路的实机验证；没有以相同版本自报代替精确镜像证明。
- 书桌屏最终 ROM `artifacts/rollout-20260928/154g-0.18.34-ota1.bin`，1,631,488 B / SHA256 `DDEE1ECAEB94714165E639CCD98C0327AFA5B3A46328153028C013B666249E1D`，marker `codex-status-ota-v1|codex-status-154g|0.18.34-bw-ota1`。COM4 `esptool read-mac` 精确核实 `70041DD7A340`；只写 `0x10000` 新 ROM、擦 `0xD000/0x2000` otadata，不擦 NVS/LittleFS。写后 `Hash of data verified`；从 0x10000 回读 1,631,488 字节与 ROM 逐字节相同。串口 CLI 自报新版并连接原 IP `192.168.3.163`；Bridge 随后发 sleep Plan，HTTP 暂不可达，未把这次 USB 写入当作新 OTA 路径验收。

## 双设备 panic 的 USB 崩溃栈与修复候选（2026-09-29）

- 书桌屏 USB COM4 经 `esptool read-mac` 核实为 `70:04:1d:d7:a3:40`。只读提取 coredump 分区 `0x610000`/`0x10000` 至 ignored `artifacts/rollout-20260928/desk-coredump-partition.bin`（SHA256 `F7A7496B2CC952E99A6F4C468059AA11C27A2DC939EE97AB58B4B2F7043A839E`），用与故障 ROM `0.18.32-bw-sync1`/SHA256 `108E82C8DE06B45CBF19D1CDED38F7A9C7B2F65D54421484661C9D1DAD5B62FD` 匹配的 ELF 解码；文本证据为 `artifacts/rollout-20260928/desk-panic-core-info.txt`。崩溃任务是 `loopTask`，`exccause=0x41 DebugException`，PC `xPortEnterCriticalTimeout`，栈 8192 字节中使用 8048、仅余 128；回溯在 `0xa5a5a599` 处损坏。结合 `syncProtocolRun` 原 3072 字节栈帧及 `sync_complete` 请求后重启，最强解释是主循环栈耗尽，不能从已损坏回溯确定唯一触发指令；协议拒绝本身不会形成该异常。
- Note4 已认证自报 `0.18.33-note4-b-sync1`、`reset=panic`；批次 4 完成，批次 5 共 3725 字节在分页传输阶段仅 ACK 1024 字节。两目标共用 `syncProtocolRun` 且原 ELF 栈帧同为 3072 字节，因此同因高度可疑，但 Note4 的崩溃串口输出尚未取得，不能声称两个崩溃栈完全相同。`partitions_note4.csv` 没有 coredump 分区（尽管 sdkconfig 启用 flash coredump），无法像书桌屏一样从 flash 回读旧栈；需要 USB 串口在线捕获下次 panic。为免 Bridge 持续触发旧固件同步，已按 watchdog→计划任务顺序暂停默认 Bridge；当前任务为 `Ready`，无默认 `bridge-app.exe` 运行。两台设备当次 `/status.json` 均因深睡不可达；现场以本节为准。
- 修复候选把同步分页 1024+1369 字节缓冲及镜像校验 4096 字节缓冲移到堆，并用 Arduino 弱符号覆盖将 `loopTask` 栈增至 12288 字节。两个目标串行增量构建成功（Note4 45.05s、154g 38.76s，无 framework 重装）；154g ELF 的 `syncProtocolRun` 栈帧 3072→800 字节、`handleSyncImage` 4512→448 字节，覆盖符号反汇编返回 12288。候选 ROM：`artifacts/rollout-20260928/note4-0.18.34-stack1.bin` 1,651,424 B / SHA256 `4B03F068F3793424791A1B696C297C8A4A091D90D3CE5E4400B0BDBFF5273DF4`，marker `codex-status-ota-v1|zectrix-note4-400x300|0.18.34-note4-b-stack1`；`artifacts/rollout-20260928/154g-0.18.33-stack1.bin` 1,634,208 B / SHA256 `40921266D284DFD9FAB3D5161C44D40116A0AFD0963BA2F8726DFAFB3C6D8CAA`，marker `codex-status-ota-v1|codex-status-154g|0.18.33-bw-stack1`。
- **书桌屏已 USB 刷入 154g 修复 ROM**：COM4 `esptool read-mac` 再证 `70:04:1d:d7:a3:40`，只擦 OTA 选择区 `0xD000/0x2000`，未擦 NVS/LittleFS。首次 `pio-target.ps1 -Target 154g` 上传在 PlatformIO GBK 进度条输出时 `UnicodeEncodeError` 卡住；核对进程树后终止该次上传，随后以 `esptool write-flash --no-progress` 写入同目标 40 MHz bootloader、partition、boot_app0 及上述 SHA 的 `firmware.bin` 至 `0x10000`，四段均获 `Hash of data verified`，退出码 0。重启后串口 CLI 自报 `fw=0.18.33-bw-stack1`、MAC 所属 IP `192.168.3.163`；连续 35 s 未见 panic，`diag` 的 `loopTask` 栈余量约 7900 字节。此次是 USB 恢复，不能记作 OTA 通路通过；HTTP `/status.json` 尚未取得有效响应，当时 Bridge 尚未恢复。原 `0.18.25-bw` 回退 ROM 保留。
- **Note4 已经 agent 走认证 Wi-Fi OTA 刷入修复 ROM**：登记 MAC `7C4FADB93408`、target、原 `0.18.33-note4-b-sync1`/`ota_1` 和候选 ROM 大小/哈希均在上传前核对。保存的设备操作 token 用只读 `/update` 确认为有效；直接 `/doUpdate` 两次均在上传前被 HTTP 401 拒绝，原因是设备保留旧 OTA arm，而 `sync.enabled=false` 只是当前 owner 与内部同步 owner 不匹配。恢复 agent 自建临时 claim 后，以原 bridge ID `8c94` 和**同一个** job ID `ota-stack1-note4-20260929` 在同一进程取回 `/api/sync/arm` 的 ticket（HTTP 200 `applied`）并立即上传；唯一获准写镜像的上传返回 HTTP 200 `UPDATE OK`。独立只读复核 `/status.json` 与认证 `/api/status` 同 MAC 自报 `0.18.34-note4-b-stack1`、运行槽 `ota_1`→`ota_0`、`reset=software`/原因码 3、uptime 95 s→187 s，无短时 panic。设备不自证整镜像 SHA；本次 OTA 路线由 ticket、上传 ACK、重启版本及换槽共同支持。
- **默认 Bridge 已恢复**：计划任务 `CodexStatusBridge=Running`，主 PID 59260 为仓库 `bridge/target/debug/bridge-app.exe`（SHA256 `D4CAD6733F47971A71126337365D5D55B197EB8C2682700A4BB5907B87A7DA89`），8765/8766 均由该 PID 监听，原 data 保留。Note4 原卡在 1024/3725 字节的批次 5 已在新 ROM 上完成，随后批次 6、7 的归档文件也已出现；期间认证状态 uptime 增长，未见 panic。书桌屏 BLE `data`/`plan` 有 `applied` ACK；旧设备操作 token 导致 `/claim` 401，已通过精确 MAC、已绑定加密 BLE 重新取得并更新 Bridge 保存 token，后续 `/claim` 200、`sync_config`/`sync_open` 均 `applied`。其 Bridge 旧归档 checkpoint 对设备已不存在的旧批次发 `sync_complete` 得 409 `batch_conflict`，这是回退留下的恢复状态，未伴随新的 panic；按 backlog A0 跟踪，暂不把业务同步标为完成。Note4 后续 Plan 为 0s，设备进入深睡后只读 HTTP 超时不能单独解释为 panic。
- 后续只读复核 Bridge checkpoint：Note4 的客户端批次 8 `8-6f5f84419b7a72c114a92c96f0a52b02` 已为 `complete`、归档 1050 字节；书桌屏旧批次 `1-cc96959b5a75e68e9559837e5eada4fd` 仍为 `awaiting_device_ack`、归档 981 字节。未手改运行数据或伪造设备确认。
- `cargo test -p bridge-render` 全部通过；显式运行默认 `#[ignore]` 的 `sync_v1_bridge_archive_against_note4_fake_rom` 也通过。Fake ROM 与固件共用 C++ 同步逻辑，但运行在 Windows 宿主线程，不模拟 ESP 的 8 KiB `loopTask` 栈；因此该测试能验证协议和归档，却未覆盖这次的硬件栈预算。`git diff --check` 通过。剩余实机步骤只在 `docs/roadmap/backlog.md` A0 维护。

## 书桌屏 panic 与 OTA 回退（2026-09-29 00:19 香港时间）

- 书桌屏 MAC `70041DD7A340` 在 `0.18.32-bw-sync1` / `ota_0` 自报 `reset=panic`，运行时间反复归零；不是正常墨水屏刷新。新版 Bridge 原 OTA job `fea76e01` 仅一次上传 ACK，没有循环刷写。认证 `/api/status` 显示上一轮同步批次 `1-cc96959b5a75e68e9559837e5eada4fd` 的 981/981 字节已 ACK、诊断归档在 `bridge/target/debug/data/platform/diagnostics/70041DD7A340/`，但 `pending_batch` 仍在且 `confirmation_pending=true`。两次尝试 `POST /api/sync/complete` 均超时，随后设备 uptime 归零且旧批次仍在；这是本次观察到的 panic 触发路径，精确栈因当时未接串口尚未确认。新回退 job `ff8a3dc9` 被旧 OTA arm 拒绝 `409 batch_conflict`，未取得上传 ACK。
- 按 watchdog→计划任务顺序暂停默认 Bridge。用户接上 USB；恢复用一次精确 MAC/token 校验后以 `8c94-recover` 临时 claim，经已配对 BLE 的 `status` 与 `sync_config(enabled=false)` 均获 `applied` ACK。随后认证 `/api/status` 同 MAC 显示 `sync.enabled=false`、`confirmation_pending=false`，证明旧 arm 清除。回退 ROM `artifacts/154g-0.18.25-bw-rollback.bin` 为 1,741,040 B、SHA256 `5D875228CAD845F3F769270F398A50F968EC82A77BA343E200BD8FE66A9FD120`、marker `codex-status-ota-v1|codex-status-154g|0.18.25-bw`；一次直接认证 `/doUpdate` 收到 `HTTP 200 UPDATE OK`。同 MAC 重启后自报 `0.18.25-bw`、`ota_1`（原 `ota_0`）、`reset=software`，16:20 UTC uptime 90s；设备不自证运行镜像 SHA。
- 默认 Bridge 已通过计划任务恢复（当时主 PID 58548），job `ff8a3dc9` 已请求取消后续尝试，恢复占用已释放；不要再部署故障 ROM `artifacts/rollout-20260928/154g-0.18.32-sync1.bin`。当前新版 Bridge 的 `/api/status` 对回退固件返回 404，书桌屏业务同步暂不可用；Note4 仍由新版 Bridge 管理。下一步仅以 `docs/roadmap/backlog.md` 的事故项为准：定位 1.54 `sync_complete` 硬件 panic，修复并在 USB 串口观察下验证后再升级书桌屏。

## Note4 → 新版 Bridge → 1.54 升级已完成（2026-09-28）

- 用户授权按顺序执行：先完成 Note4 与 Bridge 的升级及 OTA 回归，**此前不构建 1.54**；成功后回头处理 1.54 的构建问题。以下各条按发生顺序记录，早期 PID/排队状态仅表示当时现场，最终状态见本节末尾。
- 已在 ignored `artifacts/rollout-20260928/` 备份旧 EXE、完整运行 data（75 个文件）及两份旧 ROM；旧 EXE SHA256 `492D618EAB0309F26643DEACADE6738729A0F1BEBEEE8D70FD7B693AA6752A56`，Note4 回退 ROM `0EED0D7007B70FC1E86BFA3087D99191FA7479AEE2A8C1CC67F09D1777584047`。新版 Bridge 从当前含设备页的工作区构建到隔离 `bridge/target/rollout-20260928/debug/bridge-app.exe`，34,003,456 B，SHA256 `DC9301C9E50F920985E7CF47FBDC5B73D1ABB35885FD6D20C07254D43D1A6BDC`；尚未运行。
- 仅执行 `tools/pio-target.ps1 -Target note4`，23.88s 成功且未重编；`.pio/build/zectrix-note4-b/firmware.bin` 1,652,208 B，SHA256 `6AE97319BB2712E4152C5F5C7570B7C376EE87F928124B1B2A74416F643E7702`，marker `codex-status-ota-v1|zectrix-note4-400x300|0.18.32-note4-b-sync1`，`.rtc.data=0x1380`/RTC SLOW `0x1e00`。旧 Bridge 按登记 MAC `7C4FADB93408` 上传 job `852d1c67`，冻结哈希与上述一致、`upload_ack=true`；旧 Bridge 因新协议无法完成其业务确认，job 留在 `awaiting_confirmation`/`upload_ack`。设备 `/status.json` 同 MAC 自报 `fw=0.18.32-note4-b-sync1`、同目标 marker、运行槽 `ota_1`、下一槽 `ota_0`；用登记 endpoint token 读 `/api/status` 得相同 MAC/版本/target、`sync_v1=1`、`running_slot=ota_1`。精确在机镜像 SHA 尚不能由这些自报证明；新版 Bridge 再次 OTA 尚未执行。
- 已按 watchdog→计划任务顺序停止旧 Bridge，另存旧 data 的 OTA 后快照；把隔离构建 EXE 安装到默认路径并由计划任务重启。新 Bridge 认证读取 Note4，`/api/plan` plan 1124 ACK、600s，`/api/data` seq 199 ACK 为 `applied` 但 `display_state=failed`（暂时业务失败，待后续修复）。新 Bridge 再次 OTA job `e29b4711` 首次因 `/api/status` 的冒号 MAC 与内部紧凑 MAC 直接比较而停在预检；已在 `bridge/crates/app/src/platform.rs` 用既有 `DeviceIdentity::normalized_mac` 修正，隔离增量构建 12.73s 成功。修复版 Bridge 曾以主 PID 57816 运行，安装 EXE 34,003,456 B、SHA256 `30AC13DE0696B9D199C731AE9D0D33B0FDBE2A889F14D8832A14FDD7595A5984`。其 job 后续预检进到 `sync-v1 is not configured`，尚无第二次上传 ACK；该未上传 job `e29b4711` 已取消。
- Note4 在 light 到期后两次 BLE timer 会合（wake seq 3/5）都完成同 MAC 身份验证，但新 Bridge 的 `status` 命令写入后 5s 没有 ACK。现场代码计算 BLE 状态 JSON 约 601 B；本地 NimBLE-Arduino 的特征值上限 512 B，超长 `setValue` 清空值且未处理失败。已在 `src/main.cpp` 缩短**仅 BLE** 状态回复（保留 HTTP 完整状态），估算当前字段约 489 B；只重编 `zectrix-note4-b` 的 `main.cpp` 并重链接，67.36s 成功、无 framework 重装。修复 ROM `artifacts/rollout-20260928/note4-0.18.33-sync1.bin`，1,651,936 B，SHA256 `9E8A6C1812D981E1FF3CBC8BA9B83DB0C0567685CA614D193C8D0078C94E62EA`，marker `codex-status-ota-v1|zectrix-note4-400x300|0.18.33-note4-b-sync1`，`.rtc.data=0x1380`/RTC SLOW `0x1e00`。修复尚未上机验证。
- 中间恢复窗口曾用备份 EXE `artifacts/rollout-20260928/bridge-app-old.exe`（主 PID 50252、watchdog 39204），单独使用同目录 `data/` 旧快照，占 8765/8766；计划任务中的新版 Bridge 当时已停止，其 `bridge/target/debug/data/` 与备份 `new-data-before-temp/` 均保留。旧 Bridge 按 Note4 MAC 冻结并排队修复 ROM job `2253f188`，但因新版设备 BLE status 超长，它也无法给设备正式 Plan，job 未上传。USB 串口未检测到，因此通过用户的 BOOT 唤醒开启 Wi-Fi 恢复窗口；旧 Bridge 任务随后取消、临时进程已停止。
- 用户短按 Note4 BOOT 后，`/status.json` 同 MAC 自报 ext1 唤醒、Wi-Fi connected、旧版 `0.18.32-note4-b-sync1` 在 `ota_1`、下一槽 `ota_0`。旧 Bridge 因新固件 BLE 回复仍不能给正式 Plan，未尝试 job `2253f188`；已取消该未上传 job 并停止临时旧进程，防止双重上传。使用已保存设备 token、精确 MAC/版本/槽与 ROM SHA256 预检，**仅一次**直接认证 `POST /doUpdate` 修复 ROM，收到 HTTP 200 `UPDATE OK`；设备重启后同 MAC/marker 自报 `0.18.33-note4-b-sync1`、运行 `ota_0`/下一槽 `ota_1`。这证明本次 Wi-Fi OTA 成功，但尚未证明新版 Bridge 的后续 OTA 通路。
- 固定默认路径中的新版 Bridge EXE/data 未被临时旧实例覆盖；已由计划任务重新启动（主 PID 22904，8765/8766），认证 `/api/status` 看到 Note4 同 MAC、`0.18.33-note4-b-sync1`、target 和 `ota_0`。新版 Bridge 再次 OTA 回归 job `bde8e156` 冻结相同修复 ROM，目前仅 `queued`。设备 `sync.enabled=false`，已请求用户再短按 BOOT 打开清醒态 BLE 窗口，验证修短后的状态 ACK 与 `sync_config`；在 Bridge 的第二次 OTA 上传 ACK 和重启槽证据出现前，不标 OTA 路线闭环。
- **阶段 OTA 闸门已过（14:45–14:52 UTC）**：未等到第二次手动 BOOT，Note4 自动 timer BLE 会合（wake seq 3）即以同 MAC 通过身份验证；`status`、`sync_config`、`data`、`plan`、`sync_open` 均获 `applied` ACK，证明 0.18.33 的 BLE 状态回复可读，设备 `sync.enabled=true`。排队 job `bde8e156` 随后暴露两处 Bridge sync HTTP MAC 格式问题：`device_client::sync` 回复身份比较未归一化、请求仍用紧凑 MAC 而固件要求冒号格式。已在共用客户端改为请求发送冒号 MAC、回复按紧凑 MAC 严格比较；隔离构建并安装的新默认 Bridge EXE 34,005,504 B，SHA256 `83837170F7F3556F3528404C1D72A360C87743262CF922C4B72ED90E96217EF8`，计划任务重启后主 PID 13132，8765/8766 属该进程，原 `data/` 保留。
- 修正后同一 job `bde8e156` 的 **`upload_ack=true`**；Note4 认证 `/api/status` 自报 MAC `7C:4F:AD:B9:34:08`、target `zectrix-note4-400x300`、`fw=0.18.33-note4-b-sync1`、`running_slot=ota_1`、`sync.enabled=true`，与上传前 `ota_0` 形成重启换槽证据。因此新版 Bridge 的受认证再次 OTA 通路已实机跑通。因刷的是同版本 ROM，job 留 `awaiting_confirmation` / `version_seen_unproven`，设备仍不能证明运行镜像逐字节 SHA256；不可写作 `image_verified`。新 Bridge 的其他业务完整矩阵、设备页真实窗口视觉及 1.54 最终 ROM 尚未验收；本阶段**未构建 1.54**。
- **按用户顺序回查 1.54 构建（15:00–15:06 UTC）**：本阶段 Note4/Bridge OTA 闸门过后才开始。检查发现 `.pio/build/` 当时仅余 Note4，1.54 对象目录与 `firmware.bin` 均不存在；PlatformIO 本机 `run/helpers.py` 的 `clean_build_dir` 在 `project.checksum` 不符时删除整个 `.pio/build`，而 `compute_project_checksum` 含源码文件清单。此前本轮删过旧源码文件并构建 Note4；因此当前首次 1.54 编译不可能是增量，不能把缺缓存误判为交替目标强制全编。按 `project-workflow/target-build-cache/plan.md` 串行实测：154g 首次 138.47s / 318 个对象，**无 framework 重装、无 IDF 库重编**；同目标原样重跑 20.16s / 0 编译，切 Note4 20.90s / 0 编译，再切 154g 19.94s / 0 编译。两侧 ROM SHA256 在切换后不变；`project.checksum=2b65f4e5ca8aa416294b2672e068cfc5b8ad544a`。日志在 ignored `artifacts/rollout-20260928/{154g-prime,154g-noop,note4-cache-switch,154g-cache-switch}.log`。
- **1.54 标准 B/W ROM 与实机 OTA（15:09–15:11 UTC）**：仅 `tools/pio-target.ps1 -Target 154g`，没有构建 gray4/btpm。最终 ROM `artifacts/rollout-20260928/154g-0.18.32-sync1.bin`，1,634,832 B，SHA256 `108E82C8DE06B45CBF19D1CDED38F7A9C7B2F65D54421484661C9D1DAD5B62FD`，marker `codex-status-ota-v1|codex-status-154g|0.18.32-bw-sync1`，镜像头 `E9-07-02-30`（8 MB/40 MHz），`.rtc.data=0x11bc`。短维护窗口中先停新版默认 Bridge 并备份 data，再以保留旧 EXE + **独立**旧 data 单独占 8765/8766；旧 Bridge job `0b59efa2` 一次上传 ACK。设备 `/status.json` 与旧 endpoint token 认证 `/api/status` 同 MAC `70:04:1D:D7:A3:40` 自报新版、target、运行 `ota_1`/下一槽 `ota_0`，电量约 54%。旧 job 留 `awaiting_confirmation/upload_ack`，因为旧 Bridge 无法用新 `/api/status` 完成业务确认，不影响已观察的 OTA 启动。
- 已停临时旧 Bridge（主 PID 49292/watchdog 15828），原 data/job 留在 ignored `artifacts/rollout-20260928/data/`；新版计划任务恢复，EXE SHA256 仍 `83837170…17EF8`，主 PID 43820 监听 8765/8766，原 data 两台登记仍在。新版 Bridge 已认证读取 1.54 同 MAC/版本/target/`ota_1`，正式 HTTP Plan 1084 ACK `applied/600s`；1.54 尚 `sync.enabled=false`，等待下一次 BLE 配置。Note4 此时深睡离线属预期。新版 Bridge 旧的 1.54 job `13da7506`（早期 b46 已上传版）已标记取消后续尝试；现排队同版本回归 job `fea76e01`，只在取得上传 ACK + 重启证据后才算其 OTA 通过。
- **1.54 新版 Bridge 回归已通过（15:15–15:33 UTC）**：自动 timer BLE 同 MAC 会合的 `status`、`sync_config`、Plan 1086 和 `sync_open` 全获 `applied` ACK；HTTP 数据 seq 124 亦 `applied/displayed`。最初同版本 OTA job `fea76e01` 久留 `queued/attempt_count=0`：OTA 前 `cycle` 两次执行诊断 `sync_http`，1.54 的 Wi-Fi 诊断连接各等约 15s 超时，使第二次占用检查面对已超过 30s 的在线身份缓存而拒绝；Plan 本身已 ACK 600s。将**已排队且本轮要投递**的 OTA 置于诊断传输之前，保留其独立占用/MAC/token/nonce 检查；诊断留到后续周期。最终安装新版 Bridge EXE 34,006,016 B，SHA256 `D4CAD6733F47971A71126337365D5D55B197EB8C2682700A4BB5907B87A7DA89`，计划任务主 PID 23608（端口 8765/8766）。同一 job 随即 `attempt_count=1`、**`upload_ack=true`**；设备认证 `/api/status` 自报 MAC `70:04:1D:D7:A3:40`、target `codex-status-154g`、`fw=0.18.32-bw-sync1`、`running_slot=ota_0`、`sync.enabled=true`，与上传前 `ota_1` 构成重启换槽证据。任务 `awaiting_confirmation/version_seen_unproven`：同版本 OTA 不足以证明精确在机 SHA256，不能写成 `image_verified`。
- 两台 ROM 都已升级并由当前新版 Bridge 的受认证 OTA 再次上传、重启换槽实测，旧 Bridge 临时实例已停，默认计划任务实例保留两台登记和运行 data。设备页代码已随最终 EXE 部署；`node tools/test-device-page.mjs` 通过，真实 Tauri 窗口视觉仍待观察。完整 Fake ROM S01–S14、设备页 U01–U20、长期 RF 与精确镜像字节哈希仍在 backlog C8；这些未验项不影响本阶段 OTA 可继续升级的结论。
- 最终复核：`cargo test -p device-sim --test bootstrap` **43/43**，`cargo test -p bridge-app --bin bridge-app` **42 passed/1 ignored**，设备页 Node 场景测试通过，`git diff --check` 无空白错误（仅既有 CRLF 提示）。默认计划任务 `Running`，主 PID 23608 独占 8765/8766，旧实例进程数 0，EXE SHA256 `D4CAD673…A7DA89`；运行 `state.json` 两台登记及 Profile 全项保留：1.54 `quad,mini`、Note4 `codex-status-a`。本轮改动尚未提交。

## 分阶段 OTA 升级路线（2026-09-28，原计划）

- 用户接受临时业务失败，只要求后续仍能 OTA 升级；因此先用现有旧 Bridge OTA Note4，再把含新版设备页的 Bridge 构建并切为唯一默认实例。新 Bridge 对 Note4 的受认证再次 OTA 成功是关键证据；Plan、同步和页面细节失败如实记录，后续 OTA 修复。1.54 最终 B/W ROM 与增量重编问题继续暂缓，此期间可暂时失联；其 ROM 准备好后短暂停新版 Bridge，用备份的旧 EXE 加独立旧 data 在原端口 OTA 1.54，再恢复新版。全过程同一时刻只运行一个默认 Bridge，不互相覆盖 data。
- 执行路线及回退边界见 `project-workflow/device-sync-diagnostics/protocol-unification.md`「两台实机分阶段切换」；待办见 `docs/roadmap/backlog.md` C8。每步前核对登记 MAC、设备自报身份、ROM marker/大小/SHA256 和 token；保留旧 EXE/data/ROM。此节是制定时的计划，实际进度以上节为准。完整 S01–S14/U01–U20、视觉与长期 RF 验收仍未完成。

## 设备页基础整理（2026-09-28，源码完成，未部署）

- 按用户要求先提交前一阶段，提交为 `c1ab267 Implement Note4 sync diagnostics and unify device protocol`。其 Note4 中间 ROM 的目标、路径、大小、SHA256 与未发布边界仍以下节为准；本轮设备页只改 Bridge，不重新构建固件。1.54 构建及其增量重编问题继续暂缓，以 Note4 为准。
- 设备 Tab 已去掉 Codex 周余量卡、全局“暂停推送/立即同步”按钮和禁用占位；全局托盘动作仍在。概览从按 MAC 的认证快照读取固件、radio、runtime、display、power、sync 组，显示采样年龄和未提供原因；最近 BLE 联系不刷新旧 Wi-Fi 组。0、false、空模板列表均作为有效值。owner 从未读取时不显示为空闲，lease 剩余标为采样时观察。PowerPlan 的发送与接受分开，电池“下限”纠为采样电量，PM 展开不主动请求设备。
- 设备选择增加代次/请求序号，A→B→A 的旧结果不得回写；切换立即清除前台指标、PM 原文与锁。Profile 草稿在本地刷新时保留；改名、claim、发布、恢复与 Plan 指定并冻结 MAC，恢复摘要还由 Bridge 核对 MAC。`get_pmstats` 先校验 `/status.json` 自报 MAC，再读取同 IP 的 `/pmstats`；失败保留同 MAC 最近成功采样与单独错误，10s 节流按最近尝试。模板/Profile/字体编辑入口按 `device-tab.md` M07 暂留设备页，具体迁移布局尚未做。
- 证据：`cargo test -p bridge-app --bin bridge-app` **42 passed、1 ignored**；`node tools/test-device-page.mjs` 覆盖多设备无默认选择、A→B→A 旧响应、0/false/[]、PM被动展开与失败保留、草稿、错 MAC 恢复、无 Plan ACK；脚本语法与 `git diff --check` 通过。浏览器安全策略禁止 `file:` 本地预览且明确不允许绕过，因此 560×680/窄窗口的真实视觉检查未做；已静态检查响应式断点与长文本折行。源码尚未嵌入生产 `bridge-app.exe`、未重启 Bridge、未 OTA/操作设备；完整 U01–U20、Fake ROM S01–S14 与实机仍按 backlog C8 待验。

## sync-v1 生产路径进行中（2026-09-28，未部署）

- 当前设备协议唯一化实施中，计划见 `project-workflow/device-sync-diagnostics/protocol-unification.md`：业务HTTP为 `/api/*`，不发送 `protocol=2`/`rv=2`，BLE `ack="command"`，MCP当前工具为 `platform_*`；固件/Fake ROM在副作用前拒绝旧帧。Bridge模块改为 `device_client`/`DeviceConnection`，应用缓存/发现名称和运行日志收敛；本地 `state.json` 的 `schema_version=2` 保留，旧 `http_v2` 缓存仅作UI显示映射。旧BLE使用量/模板写入及Bridge旧Wi-Fi使用量推送路径已从运行调度切断；MCP旧模板get/validate/save工具已移除，当前设计移至 `docs/generic-display-platform-design.md`。Bridge core 91/91+集成21/21、app 42/42、MCP 3/3，Bridge→Fake ROM显式归档1/1；Fake ROM bootstrap 43/43（新增无Bundle的Note4定时BLE→正式Plan→首个Bundle安装），render共享信封旧marker拒绝测试通过。Bridge本地8765仅保留 `GET /health`，旧 `GET /usage`/`GET /template`/`POST /deep` 已删除并经路由测试拒绝。固件已移除旧Wi-Fi/BLE usage处理器、`/usage`深睡拉取、`/deep`通知和独立HTTP客户端；未配置设备的timer wake改走BLE会合，取得正式light Plan后才开放当前HTTP Bundle安装路径。此启动路径Fake ROM已验，实机未验；C++私有标识、S01–S14余项与设备Tab仍未完成，生产Bridge/设备均未切换。Note4当前中间ROM `.pio/build/zectrix-note4-b/firmware.bin`，目标 `zectrix-note4-b`，1,652,208 B，SHA256 `6AE97319BB2712E4152C5F5C7570B7C376EE87F928124B1B2A74416F643E7702`，marker `codex-status-ota-v1|zectrix-note4-400x300|0.18.32-note4-b-sync1`，map `.rtc.data=0x1380`/RTC SLOW `0x1e00`（余量2688 B）。因删除源码文件，本次Note4重建了应用和Arduino库对象（76.76s），未见framework重装；不把本次构建当成1.54增量问题的诊断。1.54按用户指示暂缓，旧镜像不是本轮可切换镜像。
- 当前工作树开始时干净；专项 `plan.md`/`task.md` 与新 `device-tab.md` 由并行文档审查写入，本实施保留其改动。`src/v2_sync.*` 已建立 4096 B RTC 诊断环、逐记录 CRC、代际/序号、15 轮、退避和一次性清零；旧 RTC wake 数组与文字 `/log` 改为同流派生，日志去除 AP 密码和原始 SSID。`src/v2_sync_store.*` 已实现 16 KiB 冻结文件、A/B 元数据和完成收据，Bundle/字体预留 40 KiB；设备端提供 begin/page/ack/complete、BLE sync_config/open、默认关闭的能力协商与受认证约束的 Wi-Fi 开网。Bridge 已有每 MAC 的 `.part`/checkpoint/final 持久传输及 ACK 后续传；普通 BLE 与 light 会话已接线，但仍需 Fake ROM 全链验证。
- OTA arm/ticket、上传字节 SHA 检查、运行分区前 N 字节 image 接口及 Bridge 精确比对已接入；Bundle commit 写入同步义务并区分显示失败。Bridge 归档有最近 128 批/32 MiB 上限与 retention_floor、跨批同 generation+seq 异字节拒绝、owner 转移旧 part 保留；认证 Wi-Fi 状态按 identity/firmware/radio/runtime/display/power/jobs 分组采样时间，省略字段不抹去旧值，0/false/空数组保留。仍待 Flash 断电切点、完整 confirmation 观察、401/409 与超时故障矩阵、Fake ROM S01–S14 和设备页验证。真实 ESP 分区读取字节域和 OTA 启动仍需 H02，不能把宿主哈希测试当实机证明。设备未刷写、生产 Bridge 未重建/重启；本阶段不得发布。
- 固件 `src/v2_sync_protocol.*` 已把冻结 JSON、begin/page/ack/complete 与批次字段从 `main.cpp` 抽成生产/宿主同用入口；Fake ROM `sim_sync.cpp` 明确绑定单进程单设备 LittleFS dataDir，仅适配虚拟 RTC、目录与采样。Note4 Fake ROM 已接 BLE sync_config/open 和 HTTP 四端点；同源入口真实 HTTP 最小往返测试通过（冻结、分页、前缀 ACK、SHA、重复 complete），Bridge 真实归档器对 Note4 Fake ROM 的显式集成测试也通过，生成文件并核对身份。测试暴露并修正了空日志批次误判 capacity、返回对象临时字符串失效、Bridge compact MAC 与设备冒号 MAC 不匹配三处问题。仍未完成故障注入、S01–S14、24h 双机回放与设备 Tab；上述最小测试不可代替其验收。
- Note4 Fake ROM 增加 S01（15轮）、S02（light入/离场）、S03（慢分页跨正式deadline及90s无进展反例）、S04（协议重放/摘要冲突）、S05（超时后的整窗退避）、S07（强退后续传、blob/元数据和ACK丢回复切点）、S08（环溢出/CRC/文本脱敏）、S12（认证/owner/session/低电）具名用例；尚非各场景完整验收。S07揭露设备ACK偏移只存RAM，已改为随每次增加写入A/B元数据，进程强退后从64字节持久偏移恢复。1.54曾构建较早源码，但最终源码切换引发较广库重编，按用户指示暂缓；不能引用旧ROM作为本轮镜像。

## 设备同步与诊断协议设计（2026-09-28，仅文档）

- 按本轮讨论与 Astra 修订稿建立 `project-workflow/device-sync-diagnostics/design.md`、`plan.md`、`task.md`、`status.md`，并在唯一待办清单 `docs/roadmap/backlog.md` 的 C8 登记。范围：设备页去 Codex 余量，模板/屏幕内容/字体归模板页（布局暂不设计）；普通 BLE 精简；自上次成功 Wi-Fi 同步后 15 次深睡 BLE 会合触发一次性 Wi-Fi 完整批次同步；正式 light 入口/退出各同步且成功即重置计数；OTA/Bundle 应用后经 Wi-Fi 报告；普通 RAM `/log` 与跨深睡 `/history` 统一为可确认诊断流。
- Fake ROM 已模拟设备侧 HTTP 服务和 Bridge 客户端请求，也有 BLE loopback；现有 HTTP 可达性绑定模拟 light 状态，需扩展一次性 Wi-Fi 状态后才可验证新通道。修订计划把状态机、幂等、游标、错误注入和多设备隔离等大部分校验交给 Fake ROM；实机无线仅验证真实 BLE/GATT、ESP32 Wi-Fi 关联/DHCP/HTTP、无线关闭与 deep/light 切换，以及 RTC/Flash/bootloader 行为。用户已确认 Wi-Fi 同步沿用现有 Bridge→设备 HTTP 请求方向，另一项设备→Bridge 的候选协议不纳入本专项。
- Astra 随后将专项四文档定为可交接的 `sync-v1` 实施合同：新增端点/信封、15 轮与 light 边界状态、4096 B RTC 单流及有界 Flash 冻结副本、分页持久 ACK/complete、异常退避、OTA/Bundle 证据等级、Fake ROM S01–S14 断言均已写明。源码/设备仍未实施或验证。两目标 RTC map、40 KiB 文件预留、运行分区哈希字节域及 Flash 断电恢复为实施门槛；总设计 §7 的旧截止规则须在实现时同步修订。

## B 方案双设备发布：modem sleep、240/80 MHz、4s/6s（2026-09-28）

- 用户选定 B，并追加 1.54 B/W 同改。两台均在 `v2Rendezvous()` 的 BLE 启用前配置 DFS `max=240/min=80 MHz, light_sleep=false`，控制器就绪后调用 `esp_bt_sleep_enable()`；未连接广播截止 4s、已连接等指令截止 6s，成功 Plan ACK 后沿用 200ms 提前收尾。`platformio.ini` 的 BT modem sleep/main XTAL 配置进入两目标生成的 sdkconfig。普通 light 会话仍用原有电源配置。主工作区保留未完成 RF1 诊断，故正式 ROM 从干净 managed worktree `C:\Users\cogic\.codex\worktrees\note4-b46-release\codex_status` 的仅 `platformio.ini`/`src/main.cpp` 改动顺序构建。主工作区 Note4 `-rf1` 开发 ROM 已同步重建：`.pio/build/zectrix-note4-b/firmware.bin`，1,762,336 B，SHA256 `65945E6A69853AB9E5CC5705ACFE07D94529D53AC7993A8683204C5B9DA11DB6`，marker `codex-status-ota-v1|zectrix-note4-400x300|0.18.31-note4-b-b46-rf1`；**不是**发布镜像。
- Note4 干净 ROM `artifacts/note4-0.18.31-b46-clean.bin`：`zectrix-note4-b` / `zectrix-note4-400x300`，1,758,656 B，SHA256 `41B1DEC4C2135EBCDAA78798CBCF8B4F592F288251F469E616C85950D6AEF987`，marker `codex-status-ota-v1|zectrix-note4-400x300|0.18.31-note4-b-b46`。MAC `7C4FADB93408` 的 OTA job `4e25cd44` 上传 ACK、认证版本观察 `0.18.31-note4-b-b46`；正式 sleep Plan 848 ACK。03:41–03:57 UTC Bridge 记录 8 次新版本后的完整认证 BLE 会合，另有未命中的窗口，不能声称 100% 成功。精确在机镜像 SHA 仍不可由设备自证。旧版回退镜像保留 `artifacts/note4-0.18.25-rollback-rebuilt.bin`，SHA256 `0EED0D7007B70FC1E86BFA3087D99191FA7479AEE2A8C1CC67F09D1777584047`。
- 1.54 干净 ROM `artifacts/154g-0.18.31-b46-clean.bin`：`esp32-s3-epaper-154g` / `codex-status-154g`，1,744,736 B，SHA256 `0609FA690D232D511732C3453F7D903913F41C1B3022A3FE3A47547EEE8EF79D`，marker `codex-status-ota-v1|codex-status-154g|0.18.31-bw-b46`。MAC `70041DD7A340` 的 OTA job `13da7506` 上传 ACK、认证版本观察 `0.18.31-bw-b46`；设备状态仍是 `quad`/`mini`，电量 64%。正式 sleep Plan 803 ACK 后，03:54 UTC 首次新固件 deep timer BLE 会合及 Plan 804 ACK 成功；随后 light Plan 805 ACK 用于运行时核查，设备 `/log` 中 `BLE 240/80MHz no light: ESP_OK` 与 `BT modem sleep: ESP_OK` 均已见，证据 `artifacts/154g-b46-runtime-log.txt`、`artifacts/154g-b46-pmstats.txt`。PM 表含 light 会话，不能归为 BLE 专属驻留。结束时正式 sleep Plan 806 ACK，04:00 UTC 下一次 timer BLE 会合及 Plan 807 ACK 继续成功。旧版回退镜像 `artifacts/154g-0.18.25-bw-rollback.bin`，1,741,040 B，SHA256 `5D875228CAD845F3F769270F398A50F968EC82A77BA343E200BD8FE66A9FD120`。
- 这些上机记录只验证版本切换、运行时启用与初步 GATT 会合；4s/6s 的长期成功率、B 相对 C 的**整机电池端净节电量**、1.54 的 BLE 专属频率驻留仍未测。Bridge 未改版，`git diff --check` 在主工作区与干净工作树均通过；未提交。

## Note4 每分钟能耗预估表（2026-09-28）

- 根据 C 组 5s/9s 实机记录整理 `project-workflow/power-plan-c/energy-budget-per-minute.md`：外推每 60s 一次成功 BLE 会合、一次时钟局刷、无 Wi-Fi/light 会话。33 个完整 timer BLE 周期平均清醒 3.717s；设备累计 BLE-on 约 2.304s/轮，混合周期渲染均值约 0.765s（只用于拆分非 BLE 清醒段）。**BLE 窗口主控 PM 驻留**为 40/80/240 MHz 各 35.0%/33.1%/31.9%，每轮约 0.798/0.753/0.726s；局刷及其余启动阶段没有同口径分档记录。以 Espressif 示例 17.9mA BLE、项目旧 30mA CPU 情景、芯片 8µA deep 假设，互斥阶段合计 **0.023354mAh/分钟**（约 33.63mAh/天）；若 BLE 平均电流改取 40/120mA 情景则合计 0.037497/0.088693mAh/分钟。同 ROM、同 3s/6s 截止的 B/C 实测分别为 32/32、30/34 回答；C 的 40 MHz 档 1.092s/轮，如仅代入芯片手册的 40/80 MHz WAITI 差，条件收益为 0.00267–0.00525mAh/分钟（3.84–7.55mAh/天），**不是整机 B→C 净节电量**；改窗口后的 C59 不能与 B 做同条件因果比较。**这些是模型情景，不是 Note4 电池端实测或可靠上下界**；面板、高压驱动、PSRAM、稳压器、GATT 工况与 Wi-Fi/light 偶发会话均缺独立电流积分，真实整机总量仍未知。下一步需按模块/事件在电池端积分测量后替换假设。没有改固件/Bridge，也没有 OTA。

## Note4 C 组延长窗口 5s/9s 实机对照（2026-09-28，已回退）

- 用户要求调整 C 组参数测试：保留 BT modem sleep、40 MHz DFS 下限、main XTAL、15s 实验会合节奏及测试 Bridge 的 8s 同 MAC 冷却，仅把设备未连接截止 3→5s、已连接等命令截止 6→9s。Note4 `7C4FADB93408` 经测试 ROM OTA job `b43cbe2b` 上传 ACK、同 MAC 自报 `0.18.30-note4-b-btpm-c59`；测试 ROM `artifacts/note4-0.18.30-btpm-c59-test.bin`，目标 `zectrix-note4-b`、target `zectrix-note4-400x300`、1,764,368 B、SHA256 `4E70D593D0CE871E1817D69B4C278A6F2284E59078F230B2D3EB8AF36A1BE06F`。设备不提供精确在机镜像哈希证明。Bridge 临时 EXE 单元测试 3/3 通过。
- 正式 sleep Plan 803 ACK 后重置计数，设备 `/c59.json` 累计 **34/34 个 BLE 窗口收到有效答复**，窗口均值 2.277s，APB_MIN(40)/APB_MAX(80)/CPU_MAX(240) 驻留 35.0%/33.1%/31.9%，SLEEP 0。`/history` 在读取时保有前 33 个完整 timer BLE 窗口，均 answered；其中两轮连接花约 4.2/4.3s，旧 3s 截止会提前结束，5s 上限确有实际覆盖；没有观察到需要连接后等命令超过 6s 的轮次。旧 C 是 30/34、平均窗口 2.858s、40 MHz 驻留 38.2%。本轮均值短 0.581s 是观察值而非延长窗口必然省电；未交错测试、样本各 34 轮、无板级电流仪，不能把 34/34 归因于参数，也不能声称净电量收益。原始 `artifacts/c59-final*` 与 `artifacts/c59-bridge-full-window.log` 已保存（ignored）；分析见 `project-workflow/power-plan-c/task-4-modem-sleep.md`。
- 结束时正式 light Plan 813 ACK，随后 OTA job `0465223c` 用 `artifacts/note4-0.18.25-rollback-rebuilt.bin`（1,758,832 B，SHA256 `0EED0D7007B70FC1E86BFA3087D99191FA7479AEE2A8C1CC67F09D1777584047`）回退；上传 ACK、同 MAC 预期版本 `0.18.25-note4-b` 已观察，confirmation=`version_observed`。正式 sleep Plan 816 ACK `applied/0s`。默认 Bridge 从备份恢复生产 EXE，SHA256 `492D618EAB0309F26643DEACADE6738729A0F1BEBEEE8D70FD7B693AA6752A56`；计划任务 Running，主 PID 18100 + watchdog 17428，8765/8766 监听，`bridge/target/debug/data/platform/state.json` 保留；回退后 02:37/02:41/02:43/02:45 UTC 仍记录 Note4 完整 BLE 会合。四份实验覆盖的源码按测试前 SHA 恢复，保留用户原有 RF1 与其它未提交改动；无提交。
- 按唯一 Note4 目标 `pwsh tools/pio-target.ps1 -Target note4` 重建正常工作区 ROM 成功：`.pio/build/zectrix-note4-b/firmware.bin`，marker `codex-status-ota-v1|zectrix-note4-400x300|0.18.26-note4-b-rf1`，1,762,160 B，SHA256 `FD01799B6494BD11325A4AA0EA3C148DBE6EF9A702D5BC8F4E825F1C231E0EA7`。这是**本地 RF1 工作区产物，不是实机生产版本**；因 IDF 内嵌构建时间，与上次同源 ROM 哈希不同。`git diff --check` 通过。

## Note4 BT modem sleep / DFS 三臂实机测试（2026-09-28，已回退）

- 用户要求先把 Bridge 会合节奏与 15s 设备测试周期匹配，避免把 33 次唤醒却仅 12 次 GATT 当成有效的同条件采样。实验 Bridge 只针对测试版本 `0.18.29-note4-b-btpm` 把同 MAC 成功后的 55s 冷却临时改为 8s；单元测试 3/3 通过。旧 33/12 只保留作误配证据。正式三组重新清零设备计数，都是 Note4 `7C4FADB93408` 的真实 deep timer BLE 窗口：A（BMS 关/80 MHz）33/34 回答，B（BMS 开/80 MHz）32/32，C（BMS 开/40 MHz）30/34；平均窗口 2.848/2.950/2.858s。APB_MIN 驻留 1.0%/24.9%/38.2%，SLEEP 均为 0（本轮关自动 light sleep）。C 的 4 次未答细分为 3 次 3s 内无连接、1 次已见链路但额外 6s 内无命令；Bridge 对其中多轮发现广播却 GATT `connect_failed`，不能仅归因于设备频率或单纯扫描错过。B 值得下一轮电流仪与长时连接回归；C 当前连接率与 CPU_MAX 比例（31.8%）不支持直接发布。完整口径、原始证据、条件 mAh/day 估算见 `project-workflow/power-plan-c/task-4-modem-sleep.md` 末节；`artifacts/btpm-{a,b,c}-bridge8-*` 与 `artifacts/btpm-bridge8-rendezvous.txt` 均 ignored。无板级电流仪，**不能把模式驻留或官方芯片电流当成 Note4 实测耗电**。
- 实验 ROM `artifacts/note4-0.18.29-btpm-fast-test.bin`（目标 `zectrix-note4-b`，固件 target `zectrix-note4-400x300`，1,767,616 B，SHA256 `89681437DC39CB684C401B3A9EBC50184089FA32E5E33AB80C4812674F92F402`），OTA job `797fcc7a` 上传 ACK 且同 MAC 自报 `0.18.29-note4-b-btpm`，设备不提供精确在机镜像哈希证明。实验参数在回退前设回 60s/arm A；生产回退 ROM `artifacts/note4-0.18.25-rollback-rebuilt.bin`（1,758,832 B，SHA256 `0EED0D7007B70FC1E86BFA3087D99191FA7479AEE2A8C1CC67F09D1777584047`），OTA job `e4ad73c7` 上传 ACK、同 MAC 版本 `0.18.25-note4-b` 已观察，confirmation=`version_observed`；正式 sleep Plan 794 ACK `applied/0s`，其后自然会合继续 ACK，pending light=null。
- 默认 Bridge 的生产 EXE 已从 `artifacts/bridge-app-before-btpm.exe` 原 SHA256 `492D618EAB0309F26643DEACADE6738729A0F1BEBEEE8D70FD7B693AA6752A56` 恢复，经计划任务重启后 PID 40400 + watchdog 44424，8765/8766 监听，`bridge/target/debug/data/platform/state.json` 保留。临时 Bridge 冷却补丁已从源码移除。临时 Note4 固件代码和 sdkconfig 覆盖也已移除，保留原先工作区的 RF1 诊断改动；`pwsh tools/pio-target.ps1 -Target note4` 重建成功，当前工作区 `.pio/build/zectrix-note4-b/firmware.bin` 为 `0.18.26-note4-b-rf1` marker、1,762,160 B、SHA256 `AF71EC8A63EE04111BEFDFDBB68519980C8F59673BF1A4FAF00C7873D85F1C8B`。**该工作区 ROM 不是实机生产 ROM**。`git diff --check` 通过，未提交。

## Note4 唤醒后 BLE 前启用 DFS：80/10 MHz 临时 A/B（2026-09-28，已回退）

- 用户要求实机测试，按登记 MAC `7C4FADB93408` 顺序 OTA 两份仅改变早期 DFS 下限和版本标记的 Note4 测试 ROM：`artifacts/note4-0.18.26-early80-test.bin`（1,763,456 B，SHA256 `44159A382AD6323ACC4805A3682FF7984E21C1493CD390F3D47CB0877F71B6C5`，job `78be0c6e`）与 `artifacts/note4-0.18.26-early10-test.bin`（同大小，SHA256 `EC2708FB1B8B2ABD75A7D1D31B6062718ACD0684BC99210FE89E8B3EC664DA66`，job `4f620751`）。两者环境均为 `zectrix-note4-b`，marker 分别为 `codex-status-ota-v1|zectrix-note4-400x300|0.18.26-note4-b-early80` / `...early10`；上传 ACK 与同 MAC 预期版本均已观察，精确运行镜像 SHA 仍无法由设备自证。
- 两臂都在 timer wake 的 `v2Rendezvous()` **启用 BLE 前**执行 `esp_pm_configure(max=240,min=80/10,light_sleep=false)`，均返回 `ESP_OK`；随后正式 Light Plan 再调用原有 240/80 + 自动 light sleep 配置。原始 `/log`/`/status.json` 在 ignored `artifacts/note4-early80-blewake-20260928-000239-*` 与 `artifacts/note4-early10-blewake-20260928-000654-*`。80 臂 Bridge GATT 连接于广播后约 1.723s、Plan 709 ACK；10 臂约 2.043s、Plan 713 ACK，之后 Wi-Fi/light、模板 `codex-status-a` 与显示均正常。每臂只有一次计划成功窗口，320ms 连接差不能归因于频率，也不能证明长期稳定性。
- 以 **PM 模式累计在窗口起止的差值**计：80 臂 BLE 等待 1,722,733µs，其中 APB_MIN(80) `0`、APB_MAX(80) `1,336,940µs`、CPU_MAX(240) `385,793µs`；10 臂等待 2,043,131µs，其中 APB_MIN(10) **`0`**、APB_MAX(80) `1,588,741µs`、CPU_MAX(240) `454,390µs`。两臂均约 78% 时间在 80 MHz 档，约 22% 在 240 MHz 档。`getCpuFrequencyMhz()` 在每次运行的任务循环读到 240，是运行点采样，错过 `delay(10)` 期间的低频驻留；**不能用 173/173 或 205/205 个 240 MHz 快照代表整段窗口**。ESP-IDF 文档解释 BT 控制器启用后持有 APB 80 MHz 锁；当前 base env 未启用 BT modem sleep。10 臂窗口开始前显示的 APB_MIN(10) 累计约 0.106s 已在起点存在，可能包含重新配置前的 PM 历史，不能当作本次 10 MHz 收益。
- 因此“早启用 DFS”确实让 BLE 等待产生大量 80 MHz 驻留，而把下限从 80 改到 10 在当前 BLE 广播/GATT 窗口**没有增加任何 10 MHz 驻留**。没有电流仪，不报告 mAh 节省；短样本未见配置拒绝、重启或本次 GATT/Plan 失败，但无法排除 10 MHz 对其他场景的副作用。若未来启用 BT modem sleep/改低功耗时钟，须重新测锁和连接稳定性。
- 已回退到生产 `0.18.25-note4-b`：`artifacts/note4-0.18.25-rollback-rebuilt.bin`（1,758,832 B，SHA256 `0EED0D7007B70FC1E86BFA3087D99191FA7479AEE2A8C1CC67F09D1777584047`），OTA job `87022d42` 上传 ACK、同 MAC 版本与目标 marker 已观察；正式 sleep Plan 716 ACK `applied/0s`，下一次自然会合又 ACK sleep Plan 717，Bridge pending=null。临时代码已移除；工作区 Note4 `.pio/build/zectrix-note4-b/firmware.bin` 恢复 `rf1` marker、1,762,512 B、SHA256 `92CC91991CB63E4D8771C6418CD2C18768CAE18C44119C5DAB2DEF32158BD06D`。设备与工作区产物版本不同，勿混淆。

## Note4 BLE GATT 等待频率实机定点测量（2026-09-27，已回退）

- 用户本次授权的临时 OTA 范围内，按登记 MAC `7C4FADB93408` 做了真实 deep timer 唤醒、设备 BLE 广播、Bridge GATT 连接和正式 Light Plan。最终采样 ROM `artifacts/note4-0.18.26-pmw5-test.bin`（ignored），环境 `zectrix-note4-b`，marker `codex-status-ota-v1|zectrix-note4-400x300|0.18.26-note4-b-pmw5`，1,762,928 B，SHA256 `9AE206230AD511ED109B96B577CB6F4A8BD350DEEC829C690A1CAE812A80F3A7`；OTA job `ba0ee142` 上传 ACK 且同 MAC 版本已观察，精确在机镜像 SHA 仍不可自证。
- 设备原始日志 `artifacts/note4-pmw5-blewake-20260927-234706-log.txt` 与状态 JSON（ignored）：广播等待起点 `cpu_now=240MHz`，Bridge 建立连接时 `wait_ms=1401`、`cpu_now=240MHz`；等待循环的 141 次**任务运行点**采样均为 240MHz；正式 Light Plan 699 后才记录 `esp_pm_configure(light_sleep=1, 240/80MHz): ESP_OK`。Bridge 对同 MAC 的 Plan 699 BLE ACK `applied/600s`。代码路径 `setup()` 的 `v2Rendezvous()` 在 `startNormalMode()`/`configurePowerManagement()` 之前，因此该阶段尚未按应用配置启用 80/40 MHz DFS。**口径更正（2026-09-28）**：运行点采样会错过 `delay(10)` 间隔的低频驻留，不能据 141/141 快照断言整段 1.401s 都是 240 MHz；当次 BLE 阶段缺 PM 模式表，完整驻留未知。见上节早启用 DFS 后的窗口差值。
- 前两版临时 PM 表记录器在 BLE 阶段未取得模式表：该阶段尚未调用 `esp_pm_configure`，且 `DevLog.printf` 对长文本有截断；相关实验 ROM `pmw3` 1,763,056 B / SHA256 `13AA9467FE9C171A80A321E648E3FB990DB2872A5BEC3527BFC7F5FCD01A3DB6`、`pmw4` 1,762,816 B / SHA256 `C5D4D8BFC589ED05DB3A87E57EDD2F6E51EFD72DE03945070C2059DCCD88DC07` 只作排错证据，不作频率结果。先前 DFS 80/40 短测的 `/pmstats` 都是进入 light 后的累计，不能归因到 BLE 等待。
- 测后用 `artifacts/note4-0.18.25-rollback-rebuilt.bin`（1,758,832 B，SHA256 `0EED0D7007B70FC1E86BFA3087D99191FA7479AEE2A8C1CC67F09D1777584047`）OTA job `1b5d54da` 回退：上传 ACK、同 MAC `fw=0.18.25-note4-b`/目标 marker 已见；正式 sleep Plan 702 ACK `applied/0s`。临时采样代码全部移除，Note4 重建后 `.pio/build/zectrix-note4-b/firmware.bin` 恢复 `rf1` marker、1,762,512 B、SHA256 `92CC91991CB63E4D8771C6418CD2C18768CAE18C44119C5DAB2DEF32158BD06D`。设备为生产版、工作区构建产物为未发布 `rf1` 实验版。

## Note4 DFS 80/40 MHz 实机短测（2026-09-27，已回退）

- 用户明确授权本次 Note4 临时 OTA 与回退。当前生产 `0.18.25-note4-b`（80 MHz）先接收正式 light Plan 665；同 MAC `7C4FADB93408`、IP `192.168.3.177` 的 `/pmstats?diag=1` 在 boot 142.449s 时记录 SLEEP 121.756s、APB_MIN 80MHz 0.101s、APB_MAX 80MHz 9.597s、CPU_MAX 240MHz 10.981s。证据 `artifacts/note4-dfs80-live-20260927-224323-{status.json,pmstats.txt}`（ignored）。这是整次 light 会话累计，不单独代表 BLE 等待。
- 只改 `FW_VERSION` 与 `cfg.min_freq_mhz=40` 的 Note4 测试 ROM：`artifacts/note4-0.18.26-dfs40-test.bin`，环境 `zectrix-note4-b`，marker `codex-status-ota-v1|zectrix-note4-400x300|0.18.26-note4-b-dfs40`，1,762,448 B，SHA256 `BB0845CE36D99542CA13E2611B5FECC269A4E387720EA113E8E4C2A12CD27A33`；OTA job `24dc8173` 上传 ACK、同 MAC 版本已观察。精确在机镜像 SHA 仍不可自证。
- 40 MHz 的真实 deep timer 会合后 light Plan 670 BLE ACK；boot 29.885s 的 PM 累计：SLEEP 15.675s、APB_MIN **40MHz 0.101291s**、APB_MAX 80MHz 8.549s、CPU_MAX 240MHz 5.546s；同一次 boot 116.614s 时 APB_MIN **仍为 0.101291s**。这证明 40MHz 配置可出现，但现有等待/随后 light 会话没有可观的 40MHz 活跃 CPU 驻留；SLEEP 行标注 40M 时 CPU 实际停钟。原始证据 `artifacts/note4-dfs40-blewake-20260927-225626-*`、`artifacts/note4-dfs40-light-20260927-225756-*`（ignored）。无电流仪，不能据此给 mAh 收益；PM 计数覆盖整次唤醒，不能把每个状态精确归属到 BLE 阶段。
- 40 MHz 的 4 次 timer BLE 窗口：2 次回答（seq 3/9，其中 seq 9 接受 Plan 670）、2 次 `no_connection`（seq 5/7），设备 `/history` 在 `artifacts/note4-dfs40-wake-history.json`；现有 80 MHz 历史样本有 24 answered/9 no_connection（另有未完成行），不同时间、样本小，不能判断 40 MHz 是否改变连接成功率。A2/A3 的 ≥30 周期同条件 A/B 与真实电流仍在 backlog C4，未验证 20/10 MHz。
- 已用已备 `artifacts/note4-0.18.25-rollback-rebuilt.bin`（1,758,832 B，SHA256 `0EED0D7007B70FC1E86BFA3087D99191FA7479AEE2A8C1CC67F09D1777584047`）回退，OTA job `968726b9` 上传 ACK、同 MAC 认证状态重新见 `fw=0.18.25-note4-b`，正式 sleep Plan 673 ACK；回退后下一次 BLE 会合又收到 sleep Plan 674 ACK 并正常结束。精确运行镜像 SHA 仍不可自证。工作区 `src/main.cpp` 两处临时改动已还原，Note4 build 重新成功，`.pio/build/zectrix-note4-b/firmware.bin` 为原 `rf1` marker，1,762,512 B / SHA256 `92CC91991CB63E4D8771C6418CD2C18768CAE18C44119C5DAB2DEF32158BD06D`。设备是生产版，构建产物是实验版，勿混淆。

## bridge_first 正常时间窗口测量边界（2026-09-27）

- 审计现有24h Fake ROM：它支持设备/Bridge独立时钟速率、timer wake和BLE GATT会合，但每窗口由`bridgeRun(...,'ble')`直接建立device_first联系；没有Bridge Publisher、设备扫描窗口或空口重叠/丢包模型，不能验证正常bridge_first的交会准确率。上节A/B/C实机P0也由HTTP触发清醒设备，不是定时deep窗口。
- 新增标准库几何模型`tools/test-bridge-first-window.py`，示例60s周期、设备扫描1.5s、PC在预测点前1s至后1.5s广播。相位误差-2s/+1s时只剩0.5s几何重叠，-2.5s/+1.5s时为0；无重校且理想线性+100ppm时约2.78h后低于模型自选0.5s阈值，每10窗校准则约60ms漂移、1.44s重叠。阈值/ppm均为敏感度假设，不是Note4实测漂移或收包保证。脚本运行通过，详见协议§9.9。
- 同MAC Note4实际固件`0.18.25-note4-b`，正式light Plan633 ACK后，用Windows独立Publisher连续90s与token诊断扫描：1s窗口8/12命中，2s窗口9/12命中；PC Started事件与每轮设备`matched/first_match_ms`证据在ignored `artifacts/window-normal-publisher.jsonl` / `artifacts/window-normal-note4.jsonl`。诊断是active scan、Wi-Fi light、HTTP触发，不是bridge_first预定短广播/deep计时；不能报作真正交会成功率。测试后正式sleep Plan634 ACK `applied`、剩余0，Publisher已自然退出；未重刷固件、未改生产Bridge。

## bridge_first 策略内广播恢复 RF P0（2026-09-27，实机筛查完成）

- 用户明确长期错窗恢复仍属 `bridge_first`，不沿用 `device_first`。`project-workflow/power-plan-c/task-5-advert-rendezvous.md` §9 定义 A（PC 长广播）、B（设备短广播）、C（PC 周期广播）和 Bridge 长期离线、设备重启、移出覆盖再返回的场景；§6 旧 GATT 恢复仅留历史对照。移出覆盖以受控断包模拟，不要求用户搬动设备，也不当成真实移动射频测量。
- 本轮实现仅测 RF 收发与时间：Note4 token 保护 `/diag?realign=a|b|c&run_id=N`；PC 独立 `advert-realign` example。测试帧 Company ID `0xFFFF`、按 run_id 匹配；无认证、epoch/持久化、实际 deep、业务命令或正式排期变更，不能据此宣称生产恢复完成。Luna 实机比较已完成，原始失败轮也保留。
- 安装前 R0 单向基线：Windows `adv-spike` Started 延迟 18.653 ms；Note4 token 诊断 12 s 被动独立接收证据（诊断 API 当时为 active scan）：共见 272 包，其中 test Company ID 14 包，首包 828 ms，匹配帧均 non-connectable，RSSI 约 -58 至 -65 dBm。原始记录在 ignored `artifacts/r0-concurrent-adv-20260927-2045.jsonl`。这仅证明当前主机能发且 Note4 能收，不能证明双向恢复成功。
- Note4 实验 ROM：`.pio/build/zectrix-note4-b/firmware.bin`，环境 `zectrix-note4-b`，marker `codex-status-ota-v1|zectrix-note4-400x300|0.18.26-note4-b-rf0`，1,762,448 B，SHA256 `20CC9A78EB4FC021AC90692C13D82DE225DF607EA31CBEF36424F6DE89C16A09`；同一文件保存于 ignored `artifacts/note4-0.18.26-rf0-experiment.bin`。回退镜像 `artifacts/note4-0.18.25-rollback-rebuilt.bin`，marker `0.18.25-note4-b`，1,758,832 B，SHA256 `0EED0D7007B70FC1E86BFA3087D99191FA7479AEE2A8C1CC67F09D1777584047`。回退镜像由原源码重新构建，因 IDF 编译日期而与原在机哈希不同，版本与行为代码相同。
- 实验 OTA 按登记 MAC `7C4FADB93408` 入队，job `f06ee12d`，冻结镜像哈希/大小与上项一致，上传 ACK 已收到；同 MAC 的认证 `platform_status_refresh` 已见 `fw=0.18.26-note4-b-rf0`、`protocol=2`、模板 `codex-status-a`。精确运行镜像 SHA 仍不可由设备证明，job 可停在 `awaiting_confirmation`。测试结束需核对 PowerPlan/owner/上下文与队列；若实验 ROM 不保留，按已备回退镜像单独 OTA。
- Luna 的首批 Note4+Windows 双端 A/B/C 各3轮（`artifacts/rf0-{a,b,c}-{1,2,3}.{diag.json,jsonl}`）：完整收到 OFFER 分别为 **A 1/3、B 3/3、C 0/3**，9轮全部 `deadline_ok=true`。A唯一成功轮可用广播首包134ms、OFFER首包1378ms；B三轮OFFER首包39/477/1127ms；C第三轮收到可用广播（首包340ms）但设备仍扫满1.5s才发挑战，PC未见挑战。该样本仅说明本环境的短轮射频，不足以推断生产可靠率。测试后 sleep Plan 617 ACK、pending=null。
- 为检验C失败的可修复因素，设备命中可用广播或OFFER后立即停扫，重新构建 Note4 `0.18.26-note4-b-rf1`：`.pio/build/zectrix-note4-b/firmware.bin`，marker `codex-status-ota-v1|zectrix-note4-400x300|0.18.26-note4-b-rf1`，1,762,512 B，SHA256 `92CC91991CB63E4D8771C6418CD2C18768CAE18C44119C5DAB2DEF32158BD06D`；同一镜像保存于 ignored `artifacts/note4-0.18.26-rf1-experiment.bin`。OTA job `5ff7c0dc` 与 light Plan 619 已执行，随后进行了相同条件复测。
- `rf1` 同 MAC 认证版本已见，OTA job `5ff7c0dc` 上传 ACK 与版本观察均完成。Luna 相同条件 A/B/C 各3轮：完整OFFER **A 1/3、B 3/3、C 0/3**，9轮均守6s截止；A成功轮1.149s、B三轮1.293/1.882/1.452s。C一轮107ms就听到AVAILABLE，但PC未见设备CHALLENGE；早停不足以修复C。C另试1s/5s相位各一轮均未发现；B停独立harness一轮按截止失败、下轮重开成功，作为覆盖中断注入。没有实际移动设备、停止生产Bridge、真实deep/冷启动或认证排期恢复。详细原始证据与取舍在 `project-workflow/power-plan-c/task-5-advert-rendezvous.md` §9.8。Luna最终sleep Plan 622 ACK，pending=null，Profile仍为 `codex-status-a`。
- 已撤去临时诊断固件：同MAC回退OTA job `be1e7c3d`，冻结 `artifacts/note4-0.18.25-rollback-rebuilt.bin`（1,758,832 B，SHA256 `0EED0D7007B70FC1E86BFA3087D99191FA7479AEE2A8C1CC67F09D1777584047`），上传ACK与认证版本观察均完成；设备认证状态 `fw=0.18.25-note4-b`、MAC一致、active模板 `codex-status-a`。精确运行镜像SHA仍不可由设备自证，job停在 `awaiting_confirmation/version_observed`。正式sleep Plan 627 ACK `applied`/剩余0，coordinator pending=null。
- 工作区源码与 `.pio/build/zectrix-note4-b/firmware.bin` 仍是 `rf1` 实验版本，供后续P1继续开发；**实机已回到0.18.25**。不可把工作区当前ROM误当作实机当前ROM，也不可把实验入口当生产恢复策略。

## bridge_first 候选协议修订（2026-09-27，仅文档）

- 按用户确认更新 `project-workflow/power-plan-c/task-5-advert-rendezvous.md` 与 `docs/generic-display-platform-design.md`：无需开网时设备在有界窗口重复发送同一条认证状态广播，含本窗结果和电量；Bridge 漏收只记未确认，不判离线、不阻止设备休眠。24B 候选空口包以 1B 电量及 1B 保留替换旧 `schedule_hint`，保留短身份、epoch/window 与 64-bit 认证标签。
- `OPEN_WIFI` 后改为设备接入局域网并主动请求 Bridge HTTP 服务；新增反向 v2 hello/exchange 候选、双向认证、业务 ACK/幂等与正式 PowerPlan 截止规则。现有 Bridge→设备 HTTP 端点与客户端不能直接视作已实现反向交换；显式 `POST /claim` 的反向封装须单独评审并同步不变量，未协商时 owner 空闲/到期走 `device_first`。默认/恢复策略不变。
- 补充 Bridge 长期不运行/双方窗口错位的持续恢复：连续错过有效指令或校准检查失败后，设备每轮有界地走 `device_first`，Bridge 重启后不依赖旧窗口锚点扫描；经绑定加密 GATT 重新校准单调时间误差、安装新 epoch 的未来生效窗口并认证确认后才切回。RTC 失效、掉电、ACK 丢失和旧广播重放均有明确收敛/拒绝路径；临时恢复不改持久用户策略或正式电源期限。
- 已在待办唯一来源 `docs/roadmap/backlog.md` C4 登记剩余 spike、认证/claim 评审、实现和实机验收。本轮不改代码、不构建/发布 ROM、不操作设备；文档定值与功耗结论仍待实测。

## Bridge 配置行删除按钮比例修复（2026-09-27）

- 用户截图显示「已启用」开关旁的删除按钮被拉成高灰色椭圆。根因是新版全局 `button { min-height: 32px }` 覆盖旧 `.xbtn` 的 `height: 22px`；现对 `.xbtn` 明确 `min-height: 22px`，保持正圆，默认使用浅色底/弱强调字色，悬停才显示错误色，并为图标按钮加可读的 `aria-label`。未改模板启用/删除行为。
- 当前工作区 `cargo build -p bridge-app` 成功（8.01s），`git diff --check` 通过；计划任务 `CodexStatusBridge` 已重启，新 exe 33,891,840 B，SHA256 `492D618EAB0309F26643DEACADE6738729A0F1BEBEEE8D70FD7B693AA6752A56`。主 PID 32972 监听 8765/8766，state 仍有设备 2、模板 3。实际窗口截图待再次确认。

## Bridge 新界面重新进入运行版（2026-09-27）

- 用户发现桥界面退回旧风格。根因：2026-09-26 为 OTA 修复从干净 worktree 构建并替换了 `bridge/target/debug/bridge-app.exe`；当时新版 UI 仍是当前工作区未提交的 `bridge/crates/app/ui/index.html` 改动，故未进入该 exe。新版源码并未丢失。
- 已先停 `CodexStatusBridge` 的 watchdog→任务主进程，再在当前工作区 `cargo build -p bridge-app` 成功（1m11s），不清理或迁移 `<exe>/data/`。新 exe 33,892,352 B，SHA256 `95358F9C0FE294ED2AC676E01AC72723ECA0F7997C83B26D56F5BAB5CEC468E9`；计划任务已重新启动，主 PID 21012 监听 8765/8766，平台 state 仍有设备 2、模板 3。`git diff --check` 通过。窗口实际视觉尚待用户打开托盘确认。

## Bridge 独立于 Codex 启动（2026-09-27，待关闭 Codex 复核）

- 用户实测：2026-09-27 02:23 经旧 `tools/start-bridge.ps1` 的 `UseShellExecute=true`/`cmd.exe` 分离启动后，Bridge 主进程 PID 56044 和 watchdog PID 28360 曾监听 8765/8766，但退出 Codex 时两者仍一起退出。`bridge/target/debug/data/logs/watchdog.log` 没有这次异常退出记录；分离 stdio 不足以证明脱离调用方生命周期。
- 在当前交互用户下注册**按需、无登录触发器、无限运行时限**的 Windows 计划任务 `CodexStatusBridge`，由任务计划程序启动现有 `bridge/target/debug/bridge-app.exe`，不重建 exe、不触碰 `<exe>/data/`。当前任务状态 `Running`，Bridge 主 PID 24568 的父进程是 `svchost.exe` PID 2860，watchdog PID 2704；HTTP 8765/MCP 8766 均由 24568 监听。`tools/start-bridge.ps1` 的标准默认实例现在复用该任务；重复调用确认只报告现有 PID。脚本语法与 `git diff --check` 通过。
- 关闭 Codex 后进程是否仍在的**跨会话实测**尚未完成，列入 `docs/roadmap/backlog.md`。计划任务只负责按需启动，本次没有设置登录自启动；标准默认实例以外的旧分离启动路径未改。

## 两台设备离线深睡时钟亚秒丢失修复（2026-09-26，双 ROM 已发布）

- 用户实测 Bridge 离线一天：Note4 约慢 15 分钟，1.54 约慢 8 分钟。`deepSleepRaw()` 原用整秒 `time(nullptr)` 保存锚点，`restoreTimeFromRtc()` 每次把微秒清零；分钟唤醒时清醒阶段的亚秒可能反复丢失。现保存完整 `gettimeofday()` 微秒 epoch，并以 RTC 连续微秒差恢复完整 `timeval`。两台共用此路径；内部 RC 的物理漂移仍须上机复测。
- 顺序构建成功：Note4 `.pio/build/zectrix-note4-b/firmware.bin`，`0.18.25-note4-b`，1,758,832 B，SHA256 `A976A38C03FB6ECCF02E6E648C0724D1F875CE2FA3701508519C0064DA718135`；1.54 `.pio/build/esp32-s3-epaper-154g/firmware.bin`，`0.18.25-bw`，1,741,040 B，SHA256 `5D875228CAD845F3F769270F398A50F968EC82A77BA343E200BD8FE66A9FD120`。目标与版本 marker 均核对通过；RTC slow headroom 分别 3,728 B / 4,180 B。
- 旧 `0.18.24` OTA 任务各自停在上传 ACK 且认证见版本后的 `awaiting_confirmation`，原队列会拒绝新任务。Bridge core 现允许此状态被显式新版本任务接替，仍拒绝未收到 ACK/版本证据的任务；定向回归通过。为避免带入另一项未提交 UI，从干净 worktree 构建 Bridge；原 exe 与平台 state 已备份在 ignored `artifacts/`，新 `bridge/target/debug/bridge-app.exe` SHA256 `96C3FBA27DBE917E30CEF32A26B9C8E5241686D8A96C7AFF7D9BAF04FCE21F1A`，已按 watchdog→主进程顺序替换启动，设备 2、模板 3 保留。
- 用户授权顺序 OTA。Note4 MAC `7C4FADB93408` job `23b3310b`（request `rtc-micros-20260926-note4-01825`）冻结镜像与上述 SHA256/大小一致；一次上传 ACK 后，同 MAC 认证 `/v2/status` 从上传前 `0.18.24-note4-b` 变为 `0.18.25-note4-b`，`observed_at=1790433669`，任务保留 `awaiting_confirmation` / `version_observed`（设备未提供精确运行镜像 SHA）。首台确认后才排 1.54 MAC `70041DD7A340` job `6de8b626`（request `rtc-micros-20260926-154g-01825`）；同样一次上传 ACK，认证版本 `0.18.24-bw` → `0.18.25-bw`，`observed_at=1790433882`，任务 `awaiting_confirmation` / `version_observed`。两台模板/Profile 原状态保留，1.54 认证电量 12%。离线 24 h 准度复测仍列在 backlog，不能把版本变化当成漂移改善的证明。

## Fake ROM D/E/F 软件验收完成（2026-09-26）

- 用户范围为 D/E/F；G 整套实机回归取消。M01–M18 按错误类别取代表软件 case，明细见 `docs/history/workflow/fake-rom-simulator-def/evidence.md`。D/E/F 不能证明的 Windows GATT 缓存、面板 BUSY/LUT、真实 Flash/RTC 掉电边界仅记录在同目录 `hardware-cases.md`；没有实现或执行硬件 case。
- Fake ROM 已用固件同源 C++ 处理 Bundle/Data/Activate/Plan/claim 与画面；两族真实 v2_client 安装、1–8 项循环、A/B 文件恢复、丢 ACK 后认证对账、有限版本 OTA 与 USB/低电软件判定通过代表性测试。timer wake 仅开放有界 fake BLE 窗口，真实 `bridge_ble::V2Connection` 经显式 loopback 注册表连接、分片与身份/nonce 校验；正式 BLE Plan 后 HTTP 才开放。隔离 Bridge 的 `CODEX_STATUS_SIM_COOPERATIVE=1` 关闭后台轮询，`/sim/run` 单轮与 `/sim/clock` step 通过在途屏障；设备与 Bridge 各 MAC 的单调/wall 时钟可暂停、推进、跨进程恢复，真实 MAC 保持宿主时钟。
- 隔离 1.54 Fake A `0200000000A1` 与 Note4 Fake B `0200000000B2` 从同一磁盘快照完整协作重跑 **24 h 虚拟时间、2,861 个事件、各 1,430 次 timer wake**；前 10 min 是 light Plan，随后按分钟 timer wake。runner 逐个要求 BLE 窗口的真实 `V2Connection` 会合返回成功，否则立即失败；本次全程无失败。最终 `artifacts/fake-rom-f/final-day.ndjson`（ignored）SHA256 `A648B85ACE46DA6CF9988D63C776E1798F723DA808E142D7E95A5D0B416AE8C9`。修复宿主模板时钟前的旧 24 h trace `day.ndjson` 为 2,880 事件/各 1,440 wake，只保留作历史诊断，不算最终验收。另 10 min 失步输入 A 1.0×、B 设备 1.1× / Bridge 0.9×，A 增 10 wake、B 增 11 wake，B 设备/桥时间独立。所有端点均为 127.0.0.1，运行数据在独立 `artifacts/`，生产 Bridge/实机未触碰。
- 首次同快照双回放发现画面 CRC 不同：共享模板引擎在宿主渲染 `device.now/date` 时读真实 `time(nullptr)`。现由 `TplEnv` 可选实验 wall 时间供 Fake ROM 使用；默认生产路径继续读设备时钟。新增跨设备 1 h wall 偏移像素测试。修复后同一磁盘快照两次进程重启、各 1 h/101 事件的归一化 trace 与帧 CRC **逐字节相同**，`replay3.ndjson`/`replay4.ndjson` SHA256 均 `76AC0223DA6E40372B4F5B391DA0A83D83FE2D16E7E1879D7FCD4D359DB7CF84`。旧失败 trace 留在 ignored artifacts 用于归因，不算通过证据。
- 隔离 Cargo target 下 `cargo test -p bridge-render -p device-sim -p bridge-core -p bridge-ble -p bridge-app` 通过（device-sim bootstrap **32/32**、core 90/90、app 39/39、ble 14/14、render 全部）；`node --check tools/fake-rom-runner.mjs`、`git diff --check` 通过。仅按默认目标 `pwsh tools/pio-target.ps1 -Target note4` 重建成功，ROM `.pio/build/zectrix-note4-b/firmware.bin`，`0.18.24-note4-b`，**1,758,720 B**，SHA256 **`2008735108E95CEC2511527A0A5B028496B0E1B947EC4D53F8ACD6B7BF22C9D0`**，target/version marker 均在；未刷写。隔离 Bridge/watchdog/Fake 进程已全部停止。

## 双设备 ROM 顺序发布与实机验证（2026-09-26，主路径完成）

- 用户本轮明确授权顺序发布 Note4 与 1.54 两份 ROM 并实机测试；`AGENTS.md` 的默认只构建 Note4 成本约束已增加本轮 154g 标准 B/W 目标例外，两个 PIO 进程不并行。现场以 `bridge/target/debug/data/platform/state.json` 登记为准：Note4 `7C4FADB93408`、1.54 `70041DD7A340`。
- Note4 已发布 ROM：`.pio/build/zectrix-note4-b/firmware.bin`，`0.18.24-note4-b`，1,759,040 B，SHA256 `7B123BA616D3E75E38BFF2D8EBD9F3C945B2FF25CB9BCF4FC2B256C83E3A832D`，含 `codex-status-ota-v1|zectrix-note4-400x300|0.18.24-note4-b` marker。
- 新构建 1.54：`tools/pio-target.ps1 -Target 154g` 单目标成功（51.36 s，无 framework 重装）；`.pio/build/esp32-s3-epaper-154g/firmware.bin`，`0.18.24-bw`，1,740,832 B，SHA256 `55B8BC9421CE1CA2DE981982A37318271B25A99DAF866F824B116C9FF1B30D78`，含 `codex-status-ota-v1|codex-status-154g|0.18.24-bw` marker；镜像头 `e9 07 02 30` 对应 8 MB / 40 MHz。
- 两台登记 IP 上 `/status.json` 本轮各 4 s 超时；设备操作 token 文件均在。先向 Note4 MAC 持久入队 OTA：job `a0959afd`，request `sleep-aware-20260926-note4-01824`，冻结 SHA256/大小与上述 ROM 一致。自然会合后 1 次上传收到 `UPDATE OK`；随后同 MAC 认证 `/v2/status` 快照 `fw=0.18.24-note4-b`、`observed_at=1790408480`。旧 ROM 的上传前认证 `fw` 未知，因此 job 保留 `awaiting_confirmation` / `version_seen_unproven`，未重刷；公开状态在设备再次深睡时不可读，精确运行镜像 SHA256 仍未证明。
- 完成首台上传与新版本认证观察后，才向 1.54 MAC 入队：job `584b67f5`，request `sleep-aware-20260926-154g-01824`，冻结 SHA256/大小与上述 1.54 ROM 一致。自然会合后 1 次上传收到 `UPDATE OK`；随后同 MAC 认证状态见 `fw=0.18.24-bw`。两台 OTA 均只上传 1 次，仍为 `awaiting_confirmation` / `version_seen_unproven`，没有精确运行镜像 SHA256 证据。
- 发现并修正：OTA 有 ACK 且认证见预期版本后仍因待精确确认永久阻挡后续模板发布；当前只解除对独立业务的阻塞，OTA 状态/不重刷约束保留。重建 Bridge 后，Note4 Bundle job `e48f608e` 与 1.54 job `98d2c0b9` 均自然会合 `succeeded` 且认证 `committed_job_id` 匹配；1.54 另一个 job `776b2f66` 在 waiting 时跨 Bridge 重启，之后自然交付 `succeeded`。运行数据保留设备 2、模板 3、族配置 2。
- 电源与显示：Note4 显式 light PowerPlan `209` 收到 600 s ACK；显式 sleep `210` 收到设备 applied/0 s，发现 Bridge 未记 sleep ACK 后已修正并重建。最终显式 sleep `214` 的 HTTP ACK `applied/0` 已由 `power_view_v2` 复核 last_sent/last_accepted 均为 214、剩余 0。错误 token 的只读 `/v2/status` 探针限时 50 s / 17 次均无 HTTP 响应，401 未获结论，随后已结束临时 light 请求。Note4 认证 `display_state` 曾在 failed/displayed 间交替；用户看见实物屏幕为正常休眠画面，渲染报告异常待设备日志归因。
- 最终 Bridge `bridge/target/debug/bridge-app.exe` 33,591,808 B，SHA256 `18099053D269073F4D708C2CE3D4F26EA65051466A0A90547F5B9B78A81C6BEF`；主 PID `16692`、watchdog `30368`，HTTP/MCP 监听 8765/8766。core OTA 定向测试、app 38 项与 `git diff --check` 通过。剩余实机矩阵：错 IP/401/409 注入、长期 PowerPlan 截止期、精确镜像身份、Note4 间歇 display_state=failed 归因；待办唯一来源见 `docs/roadmap/backlog.md`。

## Bridge 休眠快照与按 MAC 延后操作：S1–S4 实施，S5 待实机（2026-09-26）

- 设备页从平台持久记录按需初始化每台已登记设备的 runtime；保存 MAC 核对且 endpoint-token 认证的 `/v2/status` 有界快照、采样时间、BLE 认证联系时间与单独的最近尝试。正常深睡显示预计休眠/等待联系，不清空旧快照；401/错 MAC 单列阻塞。公开 `/status.json` 仅作近期补充，不再作为发布确认。MCP `platform_overview` 返回同一记录。当前设备未刷新 ROM 时认证 `fw` 字段缺失，旧快照如实显示未知。
- 模板发布冻结后等待下次认证会合；重启时 `Sending → Unknown`，禁止未经状态对账重发。OTA 入队先校验 `codex-status-ota-v1|target|version` 镜像标识，再冻结文件、SHA256/大小与目标 MAC 并持久化；MCP `firmware_ota(_status/_cancel)` 共用 app service。按 MAC 的认证、正式 PowerPlan、claim 与互斥执行；旧的 selected-MAC/global pending OTA flush 已删除。上传 ACK、预期版本观察和精确镜像确认分层；当前精确身份不可用，保留 `awaiting_confirmation`、不自动重刷。
- Q1–Q3 决定：同类任务冲突，queued/waiting 须显式取消再新建；同 MAC OTA/发布各一项按创建时间串行（同秒 OTA 先）；版本只作限定证据，`image_verified` 才可作为 OTA 精确完成。详见 `project-workflow/sleep-aware-bridge/design.md`。未提交、未刷写真实设备、未执行真实发布/OTA。
- 验证：core 平台服务 20 项、app 38 项、mcp 2 项通过；UI 内联 JS `node --check` 与 `git diff --check` 通过。最终 Bridge `cargo build -p bridge-app` 成功：`bridge/target/debug/bridge-app.exe` 33,589,248 B，SHA256 `ABFD76695CF453B757BE8C7C8F4CED3500AA28693AAD60A2E55C05AF427722E1`。watchdog → 主进程安全停旧版再启动：主 PID `49864` 监听 8765/8766，watchdog `18940`；MCP overview 返回设备 2，运行数据保留模板 3、族配置 2，当前两台最近尝试均 offline（正常深睡），没有 OTA 任务。
- 固件只构建 `zectrix-note4-b`，版本 `0.18.24-note4-b`：`.pio/build/zectrix-note4-b/firmware.bin` 1,759,040 B，SHA256 `7B123BA616D3E75E38BFF2D8EBD9F3C945B2FF25CB9BCF4FC2B256C83E3A832D`；二进制核对含目标/版本 marker。因共享 `sdkconfig.defaults` 指纹，首次 `pio run -d <repo> -e zectrix-note4-b` 触发 framework 重装，历时约 15m48s；没有构建其他 env。
- S5 剩余：两台设备自然认证会合的真实任务交付/错误矩阵与 PowerPlan 截止期、Note4 新 ROM 上机后的认证 `fw` 验证，以及运行镜像精确摘要能力。当前任务未授权刷写，故不把单测当作实机验收；唯一待办入口已更新 `docs/roadmap/backlog.md`。

## 休眠设备状态与延后操作：Luna 调查 + Astra 方案（2026-09-26）

- 用户确认合同：设备日常休眠属正常；设备页显示上次成功通信数据与时间，显式模板发布/OTA 安排在目标 MAC 下次认证会合，不把暂时不可达报成设备故障。`has no runtime record` 是桥的本地记录缺口，需单独修复。
- Luna 只读核查桥端与 ROM：`get_device_status` 没用现有的 `ensure_runtime_record`；设备页完整快照仅在进程内存；模板发布已有按 MAC 持久任务；OTA 只有部分 MCP 错误分支的内存队列，flush 从当前 selected MAC 取目标且传 `mac`，底层 OTA 读取 `device_mac`；现有 OTA 成功只看 60s 内版本变化。固件定时 deep 唤醒是有界会合，`/v2/status` 有 endpoint token 鉴权。
- Astra 基于以上证据写 `project-workflow/sleep-aware-bridge/plan.md`、`design.md`、`task.md`、`review.md`：最后成功快照/尝试分离、按 MAC 持久 OTA、会合交付、ACK/重启确认、取消与故障矩阵。明确当前能力与提案的差别，精确镜像确认需要新增 ROM 能力。Q1 任务替换、Q2 OTA/发布排序、Q3 版本确认门槛留作产品决策并给安全默认建议。
- **仅文档，未改桥/ROM 实现，未构建、重启或触碰设备。** 待办唯一来源仍是 `docs/roadmap/backlog.md`。

## Bridge 模板菜单越窗修复（2026-09-26）

- 用户截图显示模板页打开「配置」下拉菜单后产生页面横向滚动条。根因是菜单绝对定位 `left:0`，按钮靠右时 210px 宽菜单向窗口外延伸并扩大文档 `scrollWidth`。
- `ui/index.html` 将该菜单与「⋯」菜单一样右对齐；菜单宽度限制在视口内，长状态/模板名允许折行，按钮组窄窗口时靠右排列。普通 WebView 内容仍会被 Tauri 原生窗口裁切，不采用窗口外渲染。
- 内联 JS 语法、HTML DOM id 唯一性、`git diff --check` 通过；`cargo build -p bridge-app` 成功（9.34s）。新 exe `bridge/target/debug/bridge-app.exe` 33,404,928 B，SHA256 `7AB1C3E014E3597E40C0B5DFE46AE678A11A2274943F767A668AE0F5CAC5CC8D`。
- 已按 watchdog → 主进程顺序停旧版，并从非受限命令启动新版；主进程 PID `54980` 监听 8765/8766，watchdog PID `4716`。运行数据目录未清理；设备 2、模板 3、族配置 2 仍在。用户截图已确认旧版现象；本轮 UI 捕获工具再次返回空清单，修复后的实际窗口截图未能自动验收。

## Bridge 新界面已重建并重启（2026-09-26）

- 用户要求让上一轮 UI 第一版进入运行中的 Bridge。先核对主进程 `47028`（HTTP 8765/MCP 8766）与同路径 watchdog `22924`，按 watchdog → 主进程顺序停止；没有清理 `target/debug` 或 `data/`。
- `cargo build -p bridge-app` 成功（33.72s），新 exe `bridge/target/debug/bridge-app.exe` 33,404,928 B，SHA256 `C916F25F36A92C84CC94128A244A5A847EDF17D3A32FCB65582CBB94EAB2D5BB`。
- 非受限启动脚本成功；新主进程 PID `23880` 监听 `0.0.0.0:8765` 与 `127.0.0.1:8766`，另有同路径 watchdog PID `55124`。运行数据保留：`state.json` 中设备 2、模板 3、族配置 2；`git diff --check` 通过。
- 尚未肉眼验收 Tauri 窗口布局；该窗口默认隐藏在托盘，用户打开后可直接查看新版。

## Bridge 界面视觉与内容第一版（2026-09-25）

- `bridge/crates/app/ui/index.html`：四个 Tab 换成统一的浅色纸面、深绿强调、清晰卡片/按钮/导航风格；模板页保留现有功能内容。设备页按当前屏幕与连接、余量、占用、平台状态、Profile/发布排序，固件细节和恢复收起；数据页突出来源和字段快照，测试 JSON 收起；MCP 页突出接入地址与复制提示，端口设置收起。去掉一直没有赋值的设备页 5H 余量占位。
- 不变更后端命令或设备协议。内联 JS 语法、HTML DOM（72 个 id 无重复，四个 Tab 对应）和 `git diff --check` 均通过。
- **未重建/重启运行中的桥**：UI 是嵌入 exe 的，当前窗口仍是旧版。Windows UI 捕获工具返回空窗口/浏览器清单，本轮没有实际截图验收；下一轮按用户反馈做视觉细化。工作记录见 `project-workflow/bridge-ui-redesign/`。

## 桥：Profile 丢失根因修复 + 桥侧 legacy/迁移清除 + 按 MAC 多设备路由（2026-09-25，进行中）

专项现场在 `project-workflow/bridge-multi-device-ui/task-phase3.md`（含分步验收、收尾流程、自检清单）；
设计合同 `design.md` 已加"2026-09-25 修订"注明哪些 legacy/迁移条目作废。

### 用户诉求与决定
用户发现"模板页里的 profile 没了"，要求从 code base 还原；随后要求实现"设备页切换设备"，并选定
**design.md 第 3 阶段的完整按 MAC 多设备路由**，且**同步清掉桥侧 legacy 通道与历史迁移代码**
（用户明确表示没有要求过对历史版本的支持和迁移）。改完由实施方停桥→重建→重启并自检。

### ① Profile 丢失的根因（已修）
`migrate_family_profiles()` 在旧 `profiles.json` 缺失/为空时循环一次都不跑，`complete` 仍为 `true`，
于是**照样写"已导入"标记** `data/family-profile-154g-imported` → 从此永不重试。上次删 `bridge/target/debug`
连带删掉 `data/` 后正是这条路：标记落盘、`family_profiles` 却是空的。**这不是设备或模板问题，是一个静默成功的迁移。**
- **还原**：用**运行中桥**的 MCP `family_profile_copy_v2`（只落盘、不发布）从设备 v2 Profile 还原两族草稿：
  `epd-ssd1681-200x200-1bpp / default / 默认 → mini,quad`、`epd-ssd2683-400x300 / default / 默认 → codex-status-a`，
  绑定全部指向真实源 `codex`。核实 `state.json`：`family_profiles=2`、`devices=2`、`templates=3`。
- **修复**：删除 `migrate_family_profiles` + `write_family_import_marker` + 调用点，并删掉陈旧标记文件；这条路径不复存在。

### ② 桥侧 legacy 通道与迁移已删除（已验收）
事实基础：**UI 已不再调用任何旧命令**（`get_profiles`/`save_profile`/`delete_profile`/`push_profile` 在
`ui/index.html` 中 0 引用），两台登记设备均 `legacy=false`、`compiler_abi=2`——旧通道在桥里已是死代码。
删除清单：`core/src/profile.rs`、`core/tests/legacy.rs`、`tools/test-bridge/profiles.seed.json`、`core::paths::profile_seed`、
`Config.profiles/profile_seed` + 两个环境变量、`ensure_runtime` 的 profile 拷贝、Tauri `get_profiles`/`save_profile`/
`delete_profile`/`push_profile`、MCP `profiles_list`/`profile_save`/`profile_push`、`push_templates_http`、`remote_template`、
排队 flush 的模板分支、platform 的 `DeviceRecord.legacy` 字段与 `migrate_legacy_profile`、`claim_unsupported`（固件 <0.13.4）、
`core/src/device.rs` 的 HTML `/status` 回退。现在**非 v2 固件被直接拒绝注册**、桥只读 `/status.json`。
- **验收**：`cargo check --workspace --all-targets` → **Finished，exit 0**（含全部 test target，无 `unused` 警告）；
  全 bridge 只剩一处 "legacy" 字样，是 `platform.rs` 里故意保留的拒绝文案 `device reports a legacy protocol`。
- **文档订正**：`AGENTS.md`（旧 ≤3 槽规则、MCP 旧工具表、`profiles.seed.json`、HTML 回退）、`README.md`（悬挂引用）、
  `design.md`（加修订说明）。
- 规模：`git diff --stat` 15 文件 **+217/-1162**（该阶段）。

### ③ 设备页选择器（前端已完成）
`ui/index.html` 设备页顶部新增「当前设备（点击切换）」：列出全部已登记设备，当前项高亮 + `当前` 徽章，
点击弹确认框才切换，选择持久化于 `localStorage['codex-status-device-mac']`。选择语义按 `design.md` §2：
**记住的 MAC 仍在则用它；否则用桥自己的设备；两台以上且未选择时不猜、提示"请先在上方选择设备"（不取列表首项）**。
设备页 15 处设备相关调用全部显式携带 MAC（`get_status`/`get_device_status`/`rename_device`/`device_discover`/
`claim_device`/`release_device`/`platform_status_refresh`/`platform_power`/`platform_plan`/`platform_publish(_preview/_cancel)`）。
校验：内联脚本 `node --check` exit 0；`#registered-devices` 唯一；UI 内 `legacy|claim_unsupported` 为 0。**切换不推送、不改模板页族选择。**

### ④ 按 MAC 运行路由（已完成并实机自检）
`AppCtx` 的单组 `device_mac/ip/name` 与单设备缓存已换成 `devices: Mutex<device_runtime::DeviceRegistry>`
（新文件 `bridge/crates/app/src/device_runtime.rs`：按 MAC 的 `DeviceRuntime` + `DeviceRegistry` + 8 个单测）。
`ctx.device_mac/ip/name/*_cache/last_claim_at/yielded/fail_streak/ip_dirty/pending_ota_rom` 在 `bridge/crates/app/`
里**引用为 0**（grep 核实）。发现链按认证 MAC 路由：`observe_device` 取代了 `learn_mac` 的单全局身份规则
（只写该 MAC 自己的记录、不覆盖用户起的名）；`note_failure(ctx, mac) >= 2` 触发 ARP 回退、`arp_running` 仍作全局闸门。
owner/claim/yielded/note/pending 均按 MAC 归位。
- **MAC 解析合同**（`design.md` §2，两个入口共用 `sole_registered_mac`）：显式 `mac` 必须已登记；未给 `mac` 时
  **只在恰好一台已登记设备时**才允许兼容默认，多台一律返回 `select a device: several are registered (<MAC 列表>)`，
  既不取列表首项、也不拿"当前选中设备"顶替。UI 的 12 个 Tauri 命令与 MCP 平台工具都走这条规则。
- **实机自检（2026-09-25 21:10，桥 PID 32316 + watchdog 45960）**：
  `platform_status_refresh {}` / `template_activate {}` / `platform_publish_preview {}` → 全部
  `select a device: several are registered (70041DD7A340, 7C4FADB93408)`；
  `platform_status_refresh {"mac":"70041DD7A340"}` → `result=ok`，设备实况 `active=quad`、`battery 45`、
  `commit_seq 405`、`applied_seq 4`；`platform_overview` → 两台都在（书桌屏 profile_count=2、Note4 =1）。
  桥日志显示两台设备**各自**被轮询与 claim（每个 MAC 一条 `/v2/status` 与 `POST /claim status=200`），
  即按 MAC 路由确实在跑，没有串到同一台。
- **测试**：`cargo test -p bridge-core -p bridge-app` → **38 + 84 项全绿**（另有 7/8/6 项测试二进制全绿）；
  `cargo check --workspace --all-targets` exit 0；`git diff --check` 干净。
- **顺带修正的两处**：① `main.rs` 里 `profile_save`/`profile_push` 两处旧工具残留（一个 activity 分支、一处
  `get_mcp_info` 的工具清单串）；② A4 删除 HTML 回退后，`v2_fallback_server` 测试夹具仍要求 `GET /`，
  已同步为 `/status.json` → `/v2/status`（测试意图不变，仍验证"投递前必须经认证 v2 状态"）。

### 未决 / 遗留（不谎报为已完成）
1. **`ui/index.html` 的界面没有被我"肉眼"验收**：我无法操作 Tauri 窗口。已验证的是它依赖的后端契约
   （12 个命令的 `mac` 参数、无 MAC 拒绝语义、真实设备应答）、内联 JS 语法（`node --check` exit 0）、
   以及新 UI 在重编时被嵌入 exe（宏展开期读取 `frontendDir`；注意 Tauri 对 UI 资源做压缩，
   **不能**用 exe 内明文搜索来判断是否嵌入——用同版本对照串验证过这点）。
2. **`data/templates/` 一直是空的、启动日志 `templates: []`**——这是**改动前就有**的状态（09:26 的启动日志即如此），
   不是本次引入：手工拷贝到该目录可成功（属主正常），所以嫌疑是 `ensure_runtime()` 里 `let _ = std::fs::copy(...)`
   把失败吞掉了（`config.seeds` 解析或读取失败都不留痕迹）。影响面仅限文件模板库 `Library`（见 3）。
3. **A5 待决策**：文件模板库 `Library`（`data/templates/*.json`）是否一并删除。它现在只支撑
   `get_status.templates`（UI 未消费）、`preview_template`/`reload_templates`（已无 UI 接线）与 MCP 的
   `template_get/validate/render/save`。删它要把这些改指向 v2 登记表，属能力变更，故未擅动；已记入 `task-phase3.md`。
4. `data/family_profiles` 的 1.54 草稿顺序现在是 `quad,mini`（initial `quad`），与设备当前 active 一致——
   本次还原时是 `mini,quad`，之后由桥的按 MAC 对账写成；两台设备的 Profile 与两族草稿都在，无丢失。

### 收尾现场（已只读核实）
- 进程：**watchdog = PID 5064**（证据：桥日志 `09:28:20Z INFO bridge_app::watchdog: watchdog started (pid 5064)`）；
  **主进程 = 24612**（`netstat -ano`：8765/8766/8767 LISTENING 属主均为它；`artifacts/bridge-app-run.pid`=24612）。
  停桥顺序必须是**先 watchdog 再主进程**。
- 起桥必须用**非受限（提权）**命令（受限沙箱会假失败并回收分离子进程、WebView2 报 `ERROR_BUSY`）。
- 本次**不需要**删 `target/debug`；重建前把 `data/`（`state.json`、`device-token-*.json`、`bridge-app.json`）当运行时数据处理。
- 待核对：`data/templates/` 上次事故后是空的，预期启动时由 `ensure_runtime` 从 `tools/test-bridge/templates/` 拷回；
  `data/profiles.json` **不应**再被创建。

## 桥：契约刷新修复 + ACL 闸门绕过（删构建目录）与运行时数据损失/恢复（2026-09-25 晚）

- **ACL 闸门（第二次遇到）**：`cargo test` 卡在写 `bridge/target/debug/.fingerprint/bridge-render-*/lib-bridge_render` → `os error 5`。实测**沙箱内无法修**：该文件属主是 `喵的问都死\CodexSandboxOffline` 且缺能力 ACE → `Set-Acl` 报 `Attempted to perform an unauthorized operation`（用 `SecurityIdentifier` 对象也一样；字符串 SID 会先报"identity references could not be translated"）。按用户指示走**删除重建**：停桥（watchdog 先）→ 删 `bridge/target/debug`（删除本身也被同一闸门拦住 `incremental/*`、`bridge-core.exe`，需一次提权）→ `cargo test -p bridge-core`（**85 项全绿**）+ `cargo build -p bridge-app`（112 s）→ 重启桥。
- ⚠️ **副作用（已如实记录）**：`<exe>/data/`（= `bridge/target/debug/data`）就是桥的**运行时数据目录**，随构建目录一起被删。丢失：`state.json`（设备登记/Profile/jobs/contexts/plans/家族 Profile）、`templates/`（模板库）、`template-backups/`、`device-token-*.json`（操作 token 缓存）、`bridge-app.json`（含 endpoint token）、`logs/`、`previews/`。**已恢复**：① 配置与 endpoint token（按本会话早先读到的内容写回 `data/bridge-app.json`，`bridge id` 仍是 `8c94`，与设备端 owner 一致）；② 模板库 —— mini/quad 来自 `tools/test-bridge/templates/`、`codex-status-a` 来自仓库夹具 `bridge/crates/core/tests/fixtures/codex-status-a-400x300.json`，**compiled_crc 与删除前逐一相同**（`cd7f5454` / `4cfa3a08` / `15ff5547`）；③ 两台设备用 `platform_device_register_v2` 重新登记（caps 从设备 `/status.json`+`/v2/status` 重新推导）；④ 两个 Profile 用 `profile_save_v2` 重建，**并把绑定全部从测试源 `static1` 改成真实源 `codex`**（1.54 8 条、Note4 7 条，见 state.json 校验）。**永久丢失**：桥侧作业历史（`jobs[]`/`bundle_jobs`，含 11 条孤儿摘要）与旧日志（关键证据已摘录进本文）；操作 token 缓存需下一次 BLE 会合重新协商（发布/数据不需要它，`/claim`、`/update` 需要）。
- **桥侧代码修复（A7 的根因）**：`note_device_status` 里新增：**当设备上报的 `active_template_id` 与当前契约不同、且该模板在 Profile 内时，重建数据契约**（`set_contract` → 新 requirements/triggers，`service.rs:1246+`）。此前只有 context 变化才重建，设备"本地按键切模板"会让桥一直按旧模板发字段集 → 设备恒回 `incomplete`。新增单测 `platform::service::tests::device_side_template_switch_refreshes_the_data_contract`（同一 context、切到 quad 后 `buckets[codex].monthly.remaining` 必须出现在契约里）→ **85 项全绿**。
- **验收（实机，1.54）**：`platform_publish` → **applied / displayed**，job `48b47968` = `succeeded`，设备 `committed_job_id=48b47968`、`active=mini`、`[bundle] install total=78140`；随后 `platform_push_now` 的原始 ACK 是 **`{"op":"data","result":"applied","display_state":"displayed","data_seq":1}`**，设备 `data_seq=1 applied_seq=1 display=displayed renders=9` —— **`incomplete` 消失，显示用的是真实 `codex` 数据**（weekly 49% → remaining 51；5h/monthly 该账号没有 → 走模板 `when.exists` 缺值分支）。Note4 全程正常（`data_seq=applied_seq=65`，未重发）。
- **待做**：按一次 1.54 按键让它**本地切到 quad**，确认桥会跟着把契约刷成 quad（8 远端字段）且数据继续 applied —— 这是该修复的现场确认（单测已覆盖逻辑）。

## 1.54 OTA 到 0.18.23-bw + 模板推送成功（2026-09-25）— A2 阻塞解除（数据面的 `incomplete` 已在上一节修复并验收）

- **OTA（用户授权执行）**：`firmware_ota {rom:'.pio/build/esp32-s3-epaper-154g/firmware.bin', device_ip:'192.168.3.163', device_mac:'70041DD7A340'}` → **`0.17.10-bw → 0.18.23-bw`（1,740,576 B），耗时 54.1 s**。刷前 ROM 已按 SHA256 `91937B18…` 核对；刷后 `slot=ota_1`、`next_slot=ota_0`、`reset=software`、`heap=103824`、设备自报 `[ota] post-OTA light window 300s`。桥的缓存 token 直接可用，未走 BLE 取 token。
- **模板推送成功（这次一次就过）**：`platform_publish {"mac":"70041DD7A340"}` → ACK **`{"op":"bundle","result":"applied","display_state":"displayed","retention":"flash","active_context_id":"f54099f7e1ca7978"}`**，job `2bfc710c` → `succeeded`（`bundle_jobs` 落空=终态），设备 `committed_job_id=2bfc710c`。
  - 设备侧证据：`[bundle] install total=78150 free=1810432 fs_total=1966080 fs_used=155648` → **安装真正落盘**；随后 `[tpl] rendered mini` → `[v2] active=quad ctx=772e2eeabb74e589` → `[v2] local switch -> quad` → `[clk] reserved x=161..195 y=9..20 box=35x12 win=5x60B` → `[tpl] rendered quad (BLE)`；**安装后 `ct_abi` 报错消失**（`v2ActiveLoad` 通过）。
  - 终态：`fw=0.18.23-bw`、`slot=ota_1`、`commit_seq=388`、**`v2_templates=2`**（原来的 3 个含 `full` 已按桥 Profile 收敛为 `mini,quad`）、`active=quad`。
  - **结论：A2 的两个互锁故障（COMMIT 被拒 + `ct_abi`/`data disabled`）都由"0.18.23 的流式安装 + 一次成功安装写入的新编译缓存/新 context"同时解掉**；旧 0.17.10-bw 上的 `oom`/`owner` 两种拒绝不再出现。这也证实了 A1/A2 节的判断（瓶颈在设备固件的 commit 路径）。
- **数据面剩一个新问题（桥侧，非固件）**：安装后第一帧数据 `seq=82` **applied**（BLE），设备本地切到 `quad` 之后每帧都被拒，`platform_push_now` 返回的原始 ACK 给出确切原因：
  `{"op":"data","result":"rejected","display_state":"failed","error":"incomplete","data_seq":83,"active_context_id":"772e2eeabb74e589"}`
  - `incomplete` 出自固件 `v2_runtime.cpp:114-118`：`fields.size() != remoteCount`（remoteCount = 编译模板里 `kind<=9` 的 requirement 数；CRC 检查在它之前 `:110` **已通过**）。即**桥发来的字段条目数与已装模板要求的远端字段数不等**，而不是内容错。
  - 证据链指向"桥的字段契约落后于设备本地切换"：桥只在 **context 变化时**才按设备上报的模板重建契约（`service.rs:1298-1321`，位于 `note_device_status` 的 reconcile 分支内；`refresh_contract` 的调用点只有 `service.rs:661/1741` 与 activate/publish 路径），而设备本地按键切模板会自己轮换 context 并被桥"提前采纳"，此后契约不再刷新 → 桥仍按**上一个模板**（`mini`，2 个远端字段）发字段集，而设备已按 `quad`（8 个远端字段）校验。观察到的时序（mini 时 applied → 切 quad 后 incomplete）与此完全吻合。
  - 另一条独立的配置缺陷（backlog 早有记录、与上面叠加）：1.54 Profile 里 5 个字段仍绑测试源 **`static1`**，而 `static1` 只提供 2 个（`buckets[codex].weekly.usedPercent`、`.weekly.resetsAt`）→ `monthly`/`5h` 的值必然是 `null`（模板走 `when.exists` 缺值分支，显示不出真实用量）。`codex` 源当前有 `weekly.usedPercent=49`、`resetCredits.availableCount=1`，但**这个账号没有 5h / monthly 桶**。
- **未做**：没有改 Profile/绑定、没有改桥代码、没有提交 git；`template_activate` 也没动（设备现在停在 `quad`，是用户按键切过去的）。Note4 全程未受影响（其数据在 15:2x 前正常 applied；之后它按自己的节奏入睡）。

### 待办（按优先级）
1. **把 1.54 Profile 的 5 个 `static1` 绑定改回 `codex`**（`profile_save_v2`），然后**再发一次** `platform_publish`（Profile 绑定属于冻结进 Bundle 的内容）——这样显示才是真实用量而不是静态测试值。属改配置，需用户点头。
2. **桥侧修契约刷新**：设备上报的 active template 变化时（不只是 context 变化时）就重建数据契约，否则"设备本地切模板"会让该设备的数据投递永久 `incomplete`。建议同时在 `note_device_status` 里对 `active_template_id` 做变更检测。属改代码。
3. 复核 `remoteCount` 语义：固件要求"每条远端 requirement 一个条目、缺值发 `v:null`"（`v2_runtime.cpp:128-133` 允许 null、不允许缺条目），桥的 `wire_fields`(`coordinator.rs:949-966`) 确实每 requirement 一条，所以两者一致的前提是**契约里的 requirement 列表 == 已装模板的**——即问题 2 的根。

## 多 env 实测：154g 隔离目录建成 + 1.54 固件重建（2026-09-25）— 隔离有效、ROM 已产出；交替构建仍会触发 framework 重装，且重装需工作区外写权限

- **建目录**（host 侧创建，属主 `喵的问都死\cogic`、继承到沙箱能力 ACE，符合 AGENTS「不要在沙箱令牌下建目录」）：`pwsh tools/pio-target.ps1 -Target 154g -Setup` → 真拷贝 `framework-arduinoespressif32`(69 MB) + `-libs`(2057 MB)，其余 15 个 junction 指向 `C:\Users\cogic\.platformio\packages`。两侧现在都是 2 real + 15 linked，各 ≈2126 MB。
- **构建**：`pio run -d <repo> -e esp32-s3-epaper-154g` → **SUCCESS 102.38 s**，无 framework 重装、无 banner。
- **154g ROM（当前源码，含 0.18.23 全部改动）**：

| 产物 | 大小 | SHA256 |
|---|---|---|
| `.pio/build/esp32-s3-epaper-154g/firmware.bin` | 1,740,576 B | `91937B18FE52B53BF43502A53CFC2B815CFCEEE3633C01F38CB8C4E5B30385CF` |
| `…/firmware.factory.bin` | 1,806,112 B | `FEA9F928BC0C38857C1942F71758C639B2F36038A168DFD89D81A6B48BFF66CF` |

  - 内嵌版本串 **`0.18.23-bw`**（`0.17.10-bw` 已不存在）、`fw_target=codex-status-154g`、`render_target=epd-ssd1681-200x200-1bpp`；镜像头 `e9 07 02 30 …` → byte[3]=`0x30` = **8 MB + 40 MHz** ✓（GD25Q64 要求）。旧的合并期产物（1,744,496 B / `E9046F6B…`）已被本次构建覆盖（历史值仍在 `backlog.md` §0）。
- **交替构建实测（C6 判据 c/d/e）**：154g 之后立刻构建 note4 → 第一次 **13.45 s FAILED**：日志出现 `*** Reinstall Arduino framework ***` + `*** Compile Arduino IDF libs for zectrix-note4-b ***`，随后 `error: Failed to initialize cache at C:\Users\cogic\AppData\Local\uv\cache … os error 5（拒绝访问）` → `Failed to create a proper virtual environment`。第二次（指纹已被上一次写回 note4、`UV_CACHE_DIR` 指向工作区内）→ **SUCCESS 39.10 s、无重装**，note4 ROM **hash 未变** `42AAF00B257908434E86CEA45838DC60FD65FE8FD426D0632E570BD6BDBCF72E`(1,758,032 B) —— 即 OTA 在机的那一版可复现。
  - **结论：隔离防的是"互相破坏"，不是"重装代价"。** `sdkconfig.defaults` 是仓库根唯一生成物，切换目标必然把它改写成对方的 `# TASMOTA__…`，下一次切回就触发 framework 重装（与旧记录一致，只是重装发生在目标自己的包目录里、不再伤到对方）。整轮下来两个包目录都完好（各 2 real + 15 linked 未变），**没有跨目标损坏**。
  - **重装在受限沙箱里做不完**：uv 要写 `%LOCALAPPDATA%\uv\cache`（工作区外）被拒；`UV_CACHE_DIR` 指到工作区内可消除该条，但 `tool-esptoolpy` 的 editable 安装仍会因写 junction 指向的共享包目录（C 盘）报 `Cannot update time stamp of directory 'esptool.egg-info'`——这条是**非致命警告**（esptool 5.4.0 照常工作，两个目标都构建成功）。要完整跑通重装需一次性提权，或把这两个路径纳入工作区。
- **新坑（务必记住）**：`tools/pio-target.ps1` 在**后台作业**里会假失败——脚本本身没问题（用 `$PSScriptRoot` 定位仓库），但它 `& pio run` 时 `pio` 继承到的 cwd 变成 `D:\Documents\project`（与本仓库无关的目录）→ `NotPlatformIOProjectError`；同一 shell 里直接 `pio project config` 却正常。可靠做法：**给 pio 加 `-d <repo>`**，或在前台 shell 里跑脚本。
- **待办**：① 想刷 1.54 就用上面这个 ROM，但 OTA 是独立决定（设备需在 awake/light 窗口；命令形状 `firmware_ota {rom:…, device_ip:192.168.3.163, device_mac:70041DD7A340}`）；注意它**不含** `owner` 校验的改动（见 A1/A2 节），所以能治 `oom`/`ct_abi`、不保证治 `owner`。② C6 判据 c/d/e 已实测（上面），若真要"切换不重装"，得让 `sdkconfig.defaults` 按目标隔离或在构建前写回目标指纹——属改动，需决策。③ 重装路径需提权或把 uv 缓存/esptool 包纳入工作区。

### 交替构建已消除重装（2026-09-25 晚补做 next.md §7 判据 c/d/e）

- **上次的考察已保存，且这轮是接它的"待定"项**：根因/上游依据在 `docs/history/next-2026-09-25.md` §7、§10、§11（含 pioarduino PR #511、issue #532/#533、Meshtastic PR #11834、PlatformIO 四页文档），调查脚本在 `artifacts/hash-forensics/`（`compute_fingerprint.py`/`dump_effective.py`/`pin_down.py`/`why_differ.py`，本地 gitignore）。§7 当时把 c/d/e 留空、并写"隔离方案**待定**：先诊断并隔离项目根 `sdkconfig.defaults` 与已安装 package 状态"；§10 同时禁止"固定/手工编辑 `sdkconfig.defaults` 首行"。
- **这轮实测（推翻 §7 的乐观前提）**：只做包目录隔离**不足以**免重装——判据在仓库根那个共享文件上。实测：154g 之后首次构建 note4 → `*** Reinstall Arduino framework ***` → 且该路径在受限沙箱里**跑不完**（uv 要写 `%LOCALAPPDATA%\uv\cache` → `os error 5`）。
- **解法（已落地 `tools/pio-target.ps1`）**：把 `sdkconfig.defaults` 按目标**整份快照**（存 `.pio-core/sdkconfig.defaults.<target>.snapshot`），构建前若与当前文件不同就整份还原，构建成功后再快照。**还原的是该目标自己生成过的完整文件（内容+指纹自洽），不是伪造首行**——这正是 §10 禁令的本意所在；安全证明：重建出的 154g ROM 与记录值**逐字节相同**（`91937B18…`）。快照过期（platformio.ini 改了）只会失配一次→走正确的"重生成"路径→构建后自动刷新快照。顺手修了脚本的 cwd 依赖（`Set-Location $RepoRoot` + `pio run -d $RepoRoot`，因为嵌套 pwsh/沙箱 broker 会把子进程 cwd 换成别的目录）。
- **实测结果**：priming（154g 首次，含一次重装路径）45.7 s 后——**note4 39.2 s、154g 42.1 s 交替，两侧均无 `*** Reinstall ***`、无 `Compile Arduino IDF libs`**，两侧 ROM 哈希不变。即：**切换代价从"每次重装/重编 IDF 库"降到几十秒增量**。复测（4 连切，逐次统计编译单元数）：`note4 29.5 s / 154g 27.8 s / note4 32.6 s / 154g 29.0 s`，**四次都是 `compiles=0`**、`Reinstall=False`、`IDFlibs=False`，两侧 ROM 哈希逐一稳定（note4 `6B3C386D…`、154g `91937B18…`）——即切换时**一个编译单元都不重编**，耗时全是脚本与 PlatformIO 的 up-to-date 检查（对照"完全不改"的 32.8 s）。priming 仍需一次完整重装，且该路径需要工作区外写权限（本沙箱下需一次性提权）。
- **仍未解决的上游风险**：`next.md` R2（issue #532/#533，HybridCompile 把按内存类型的产物写进共享 `lib/`+`ld/`，本机实测 7 个文件错位）——包目录隔离只能把影响限制在单目标内，不能消除；持续跟踪上游。

### 构建触发条件（2026-09-25 实测，note4 目标）

| 改动 | 重编范围 | 实测 |
|---|---|---|
| 不改动 | **0 个 TU**、不重链接 | 32.8 s（全是脚本/PlatformIO 的检查开销），`compiles=0` |
| 改 1 个 `.cpp`（`src/dev_log.cpp`） | **只重编那 1 个 TU + 重链接** | 46.2 s，`compiles=1`（`dev_log.cpp.o`） |
| 改被广泛 include 的头（`src/template_engine.h`） | **10 个 TU**（`src/` 里所有 include 它的：`main/bundle_store/refresh_policy/template_engine/template_xfer/v2_*`） | 41 s，`compiles=10` |
| 改 `custom_sdkconfig` / `board_build.arduino.memory_type` / `flash_size` 等**配置** | `*** Reinstall Arduino framework ***` + 删**所有** `sdkconfig.<env>` + 删/重下该目标 framework/libs + **重编 IDF 库**（≈"整个项目重编"那一类） | 本轮未重测（上次记录 15 min 量级；本轮 priming 到 15.5 s 就撞沙箱 uv 拒权而失败） |
| 改 `platformio.ini`（哪怕加一行注释） | PlatformIO 用 `.pio/build/project.checksum`（当前 `9c777f73…`）判定工程不匹配 → **删 `.pio/build/<env>` 并全量重编应用**；注释不进 `custom_sdkconfig`，所以**不会**连带触发 framework 重装 | 未测（代价与扰动大）——机制见 `next.md` §10 R7/D3 |
| 删 `.pio/build/<env>`、删包目录、`pio run -t clean` | 全量重编 | §10 明令禁止 |
| **每目标 `sdkconfig.defaults` 快照还原**（切换目标时） | **不触发**任何重编（内容=该目标自己的，指纹匹配） | 实测 note4 39.2 s / 154g 42.1 s，`Reinstall=False`、`IDFlibs=False` |

- **注意（探测的副作用，已如实记录）**：ESP-IDF 会把**编译日期/时间**写进镜像（本机 `firmware.bin` 里能搜到 `Sep 25 2026`、`18:51:24` 等串）→ **任何一次重编都会改变 ROM 哈希**。所以：① 上面探测让 note4 的本地产物变成 `6B3C386DFC59798A…`（18:10:05，1,758,032 B），与记录里的 `42AAF00B…` 不再相同——**设备上跑的仍是 05:02 那次 OTA 的 `42AAF00B…` 版本**，只是本地产物被重编过；② 之前"重建 154g ROM 与记录值逐字节相同"之所以成立，正是那次 `compiles=0`（154g 产物现在仍是 `91937B18…` ✓）；③ 以后要复现某个记录哈希，必须做到"零编译重建"，否则哈希必然变（内容等价、时间戳不同）。

## A1/A2 现场恢复：桥重启 + 清掉 1.54 Bundle 队列（2026-09-25）— **已被当日 OTA 解掉：见顶部「1.54 OTA 到 0.18.23-bw + 模板推送成功」节**（本节保留当时的失败现场与 `oom`/`owner` 归因）

### 现场（实机，15:13–15:35）
- **桥**：`bridge/target/debug/bridge-app.exe`（04:43:36 构建；`git status` 干净，与 HEAD 一致）→ **PID 21848 + watchdog 41784**；`0.0.0.0:8765` HTTP、`127.0.0.1:8766` MCP（GET→405=端点活着）、UDP 8767；`artifacts/bridge-app-run.pid`=21848（旧值 11664 已确认不存在）；日志无 panic，且 `usage changed (rev 1)` / `usage refreshed` 出现（取数源自愈）。
- **起桥的沙箱坑（新，务必照做）**：受限模式（workspace-write）下 `pwsh tools/start-bridge.ps1` **会假失败**：① `Get-NetTCPConnection` 在沙箱内看不到该监听者 → 脚本 10s 判"未出现"并抛错，而桥日志显示端口早已绑好；② 该命令结束时沙箱**把已分离的子进程一并回收**（实测进程随命令结束消失）；③ 沙箱下 WebView2 建 host 失败：`failed to create webview: HRESULT(0x800700AA) ERROR_BUSY`。用**一次性 `danger-full-access`** 重跑同一条命令即成功，且日志里**没有** WebView2 报错。桥必须从提权/非受限命令启动。
- **两台设备**（用户按键唤醒后，`GET /status.json` + ARP 双证；MAC 均与桥登记一致）：

| 设备 | MAC | ip | fw | slot | wake | data_seq/applied_seq | committed_job_id / commit_seq | 其他 |
|---|---|---|---|---|---|---|---|---|
| 1.54" 书桌屏 | `70:04:1D:D7:A3:40` | 192.168.3.163 | **0.17.10-bw** | ota_0 | power-on | 0 / 0 | `9098ec1d` / 381 | heap≈101.6 KB，电量 27%/3540 mV，`epd_busy_fails=0`，v2_templates=3，ctx `2f20033a7e135b52` |
| Note4 | `7C:4F:AD:B9:34:08` | 192.168.3.177 | 0.18.23-note4-b | ota_1 | ext1 | 63 / 63 | `1b4500ad` / 267 | heap≈97.9 KB，电量 86%/4080 mV，`epd_busy_fails=6`，`panel_power_mode=keep` |
- **1.54 IP/MAC 疑点结案**：设备自报 + ARP 均为 `192.168.3.163` / `70:04:1D:D7:A3:40`，与 `state.json` 登记一致 → 无需改 state；旧文档的 `192.168.1.50` / `70:04:1D:AA:BB:CC` **确认作废**。1.54 固件实为 **0.17.10-bw**（此前记的 `0.16.7-bw` 过期）。

### A2 只读复核（file:line）
- ① **Bundle 先于 Data**：`coordinator.rs:365-386` 的 `Delivery::Bundle{..}` 带 `return`，在 Activate(`:390`)、in-flight Data(`:408`)、Data 生成(`:421/433`) 之前。
- ② **取消对 `sending` 有效**：`app/platform.rs:1868-1871` → `job_cancel`(`:662-665`) → `service.rs:895-907` → `coordinator.rs:613-621`，条件是 `!state.is_terminal()`(`model.rs:459-468`，Cancelled/Succeeded/Failed/Unknown 才算终态)；终态会在下一次 persist 被 `bundle_jobs` 过滤掉(`service.rs:281-284`)。**MCP 工具描述 "queued (unstarted)" 不准确**（实测能取消 `sending`）。
- 设备侧真相：`committed_job_id=9098ec1d`（09-23 就 succeeded 的老作业）≠ `83f4324c` → 该 Bundle **从未提交**；对账要求相等(`service.rs:1262-1276`) 所以永不自愈。全程未改设备任何值。

### A2 执行与结果
- **取消**：`platform_publish_cancel {"mac":"70041DD7A340"}` → `{"cancelled": true, "job": {job_id 83f4324c, state "cancelled"}}`；`state.json` 回读：`bundle_jobs` 该 MAC 条目消失，`jobs[]` 中 `83f4324c` → `cancelled`。
- **孤儿 `waiting` 摘要确认**：`jobs[]` 里 11 条 waiting（1.54：`8e685eb8 9807acd8 0bfad6e9 6105361c a44910e2 78972ae8 e8af7792 25a2922b 7e7777e5 91b51cdb`；Note4：`5c6d63fc`）**不是活状态**——`bundle_jobs` 只从 coordinator 的活 `job` 派生(`service.rs:281-284`)，取消后该表为空即证无活作业；`enqueue_bundle` 会替换 `waiting` 作业(`coordinator.rs:576-584`)，而 `refresh_bundle_history` 只在终态转换时改写历史行(`service.rs:318-326`)，故每次替换都留下一行旧摘要。**按要求未逐个取消。**
- **一次干净重发（诊断）**：`platform_publish {"mac":"70041DD7A340"}` → 新作业 `e60830a4`（55,312 B，crc `8cbde6d2`）。`v2_client::install_bundle`(`v2_client.rs:323-414`) 全程：BEGIN 200 → **14 个 CHUNK 全部 200/applied**（offset 0…53248，无 HTTP 错误、无超时、无 `result!=applied`、无 next_offset 不匹配）→ COMMIT 200（42 ms）但 ACK 为：
  `{"op":"bundle","result":"rejected","error":"oom","retention":"flash","display_state":"unchanged","active_context_id":"2f20033a7e135b52"}`
- **与原始卡死同型**：09-24 20:48:33Z–21:19:24Z（本地 04:48–05:19）对 `83f4324c` 也是 BEGIN+14 chunks 全 200 后 COMMIT；此后该作业 `updated_at` 再没动过（停在 04:47:44 = Waiting→Sending 那一刻）→ **原来的"卡在 sending"就是同一种"COMMIT 被拒"**。
- **桥侧为何卡住不报错（独立缺陷）**：`app/platform.rs:720-722` 对非 applied 的 COMMIT ACK 调 `retry_job(..., committed=false)`，而 `retry_bundle_ack`(`coordinator.rs:624-636`) **只在 committed==true 时改状态**，`service.rs:922` 也只在 committed 时刷新历史 → 被拒作业永远停在 `sending`，每个会合窗口重试一次，并因 Bundle 分支优先而**永久顶住该设备的 Data 投递**。
- **设备侧根因候选**（源码引用为当前树 0.18.x；设备跑 0.17.10-bw，提交路径应同源，未逐字节对比）：`bsInstallSource` 先做 flash 余量检查，不足报 `"space"`(`bundle_store.cpp:433-436`)——**本次没有报 space**，所以**不是 8 MB flash 装不下**；随后逐个模板 `tplCompile` 重建编译缓存并 `malloc(tplCtSize()+16)`，该 malloc 失败即 `err="oom"`(`:467-468`；`:452` 注释"约 9 KB 编译记录"、`:461-462` 注释"1.54 上 compiled hex 可能比可用堆还大"）。设备 `/log` 不记录 bundle 安装失败原因，只有 HTTP ACK 里的 `oom`。
- **第二次尝试（用户授权，设备冷启动后 19 s 内发，堆最新鲜）**：新作业 `37217f05`。BEGIN 200 → 14 个 CHUNK 全 200 → COMMIT 200，但这次 ACK 是 **`error:"owner"`（不是 oom）**：
  - `owner` 只可能来自 `v2_bundle_command.cpp:144-158` 的 `strcmp(bodyOwner, owner)`（全仓唯一产出该字符串的位置）：`owner` = **COMMIT 请求体**里的 `bridge_id`，`bodyOwner` = **flash 里已暂存载荷**里的 `bridge_id`（解析不到就退回 `server.arg("bridge_id")`，而桥从不发这个参数 → 空）。
  - 该检查位于 CRC 校验(`:138`)与 `(bridge_id,request_id,nonce)` 匹配(`:110`)**之后**，两关都过了才到它 ⇒ **请求体解析正常**（否则先报 `crc`/`session_or_length`），是**设备侧重读暂存载荷时没拿到 `bridge_id`**。
  - 桥侧无问题：`deliver()` 在安装前先 `bind_pending_bundle_owner`(`platform.rs:685` → `service.rs:844-871`) 强制 `payload.bridge_id == link.bridge_id`，否则直接返回 `failed` 不发包；本次安装确实发生了 ⇒ 两侧值在桥侧是一致的。
  - 旁证：COMMIT 前 0.2 s 有一次 `/v2/status` 6 s 超时（`07:44:31.757`），说明设备当时正被 LittleFS 分片写占住；同一窗口桥的自动 `/claim` 也在跑（`07:44:29.79`）。
- **两次失败的共同点与差异**：都是"分片全过、COMMIT 被拒"；`oom` 发生在 commit 内 `bsInstallSource` 的 11.3 KB 连续分配（`sizeof(CtTemplate)`，`bundle_store.cpp:467-468`），`owner` 发生在其**之前**的载荷校验。两次都无法归因于桥侧——桥每次都把完整载荷送到了设备且设备自校验 CRC 通过。
- **设备固件是关键变量**：1.54 跑 `0.17.10-bw`，早于当前树的 `ca4022c`（0.18.23「stream bundle install from file」，即 `oom` 所在那段的重写）；而 `owner` 检查本身是更早的 `b702aa3`（09-24）引入的共享判定，0.18.23 里**未改**。
- **另一个必须记住的事实**：当前桥 exe（04:43 构建）**一次成功的 Bundle 提交都没有**——Note4 最后一次成功（`1b4500ad`）是 09-24 23:20，早于该构建；此后桥的三次尝试（`83f4324c`/`e60830a4`/`37217f05`）全部失败。所以"桥侧回归"不能靠 Note4 的历史排除，但按上面逐行追，桥侧三处（freeze 用 `ctx.bridge_id`、`bind_pending_bundle_owner` 强制一致、commit 请求用 `link.bridge_id`）取的是同一个值。
- **清理与现状**：第二次尝试后同样自动取消（`37217f05` → `cancelled`），`bundle_jobs` 为空。Bundle 分支释放后桥立刻投递 Data：`/v2/data seq=82` → 设备 200 但 `result="rejected"`、`display_state="failed"`、`error_category="ack_rejected"`、`data_seq=0`（即"队列已解阻塞，但设备自己拒绝数据"）。设备仍为 `committed_job_id=9098ec1d`、`templates=full,mini,quad`、`active=quad`、`data_seq=applied_seq=0`。注意 `commit_seq` 从 381 涨到 383 但**没有**换过 committed job——它会被启动时的 context 轮换 `bsSetActive` 递增，**不能**当"安装成功"的判据。
- **设备第二个独立故障**：1.54 `/log`：`[v2] active load failed: ct_abi`（反复）→ `[v2] cannot rotate unknown-retention context; data disabled`。已装 Bundle（job `9098ec1d`）的**编译缓存 CT_ABI 与当前固件不符**，`v2ActiveLoad()`(`main.cpp:2036-2043`) 失败 → `v2SwitchActive()` 也失败(`main.cpp:5645-5648`) → 固件自己关掉了 data。`bsLoadCompiled` 本有"用槽内保留 source 重建缓存"的兜底(`bundle_store.cpp:568-591`)，但本次连兜底也返回 `ct_abi`。注意该设备每次启动的 context id 都会轮换（`09509c9cf9be57d2` → `471096d2e97a9548` → status 报 `52f71c2a3d8c0d99`）。
- **结论**：1.54 现在有**两个互锁故障**——(a) COMMIT 校验过不去（`oom` 或 `owner`，都在设备侧），(b) 已装 Bundle 的编译缓存 ABI 不可用 → `data disabled`。两者都能被"一次成功的 Bundle 安装"同时解决，但那次安装过不去，且两次尝试给出两种不同的设备侧拒绝 ⇒ **瓶颈在设备固件的 commit 路径（0.17.10-bw），不是桥**。已完成用户授权的第二次尝试后**停手**，未裁剪模板/Profile，未做族发布，未提交 git，未改任何代码。

- **过程瑕疵（如实记录）**：最初两次 `platform_publish_cancel` 因为调用侧把 JSON-RPC 的 `arguments` 丢了（PowerShell 里参数名用了自动变量 `$args` 的坑），落到了**默认设备 Note4** 上，属 no-op（Note4 作业已终态，回 `job: null`）；唯一副作用是清掉 Note4 当时的 `data.in_flight`(`coordinator.rs:620`)。事后核实 Note4 正常：Plan ack 连续 applied，`data_seq=applied_seq=63`。1.54 的取消随后带正确参数重做并达成。

### 待办
1. ~~**首选：把 1.54 刷到已构建好的 `0.18.23-bw`**~~ → **已完成（2026-09-25，见顶部节）**：OTA 54.1 s 成功，随后的 `platform_publish` 一次 applied，job `2bfc710c` succeeded。剩下的是数据面的字段集问题（`incomplete`）与 Profile 的 `static1` 绑定，见顶部节的待办 1/2。
2. **桥侧小硬化（一行级，可让 `owner` 不可能发生）**：设备本来就为"载荷解析不到 `bridge_id`"留了退路 `server.arg("bridge_id")`（`v2_bundle_command.cpp:149`、`main.cpp:4427`），而桥**从不发这个参数**。给 COMMIT 请求带上 `?bridge_id=<id>`（或表单字段）即可让该校验不再依赖那次 55 KB 载荷重解析。属改代码，需用户批准。
3. **桥侧独立缺陷（与上面独立）**：非 applied 的 COMMIT ACK 应落成 `Failed` + `last_error`，或加尝试上限/退避；否则任何设备一次 commit 失败都会永久堵死该设备的 Data 投递（`coordinator.rs:624-636` + `service.rs:922`）。
4. 备选路线：C1（Bundle v3，55.3 KB→约 16.2 KB，同时缩小载荷重解析与 commit 分配压力）/ 缩模板或 Profile / 暂时放弃该设备数据同步。未决前**不要再对 1.54 发 `platform_publish`**。
5. 核实 1.54 上 `bsLoadCompiled` 的 ct_abi 兜底为何不生效（若修好，可不重装 Bundle 就解掉 `data disabled`）。
6. 环境：受限沙箱下 `tools/start-bridge.ps1` 会假失败并回收子进程；起桥需一次性提权（见现场节）。
7. `PROGRESS.md` 已于本轮做了一次下沉（`合并 codex/fake` 与 `桥侧修复：物理唤醒窗口取大` 两节移入 `docs/history/progress-archive-2026-09-25.md`，现约 28 KB）。仍偏大：下次里程碑可继续把`显示修复：时钟局刷窗口宽度算错`与`当前现场与待办`以下的"本轮核查"清单下沉（注意 AGENTS.md 已不再引用 `§包目录隔离`，但「沙箱写入边界」小节与 backlog D13 互指，别断链）。

## 显示修复：时钟局刷窗口宽度算错（Note4 / 比例字体）：2026-09-25 — 已实现、已 OTA、判据命中

- **根因（几何，与残影无关）**：`src/template_engine.cpp` 的 `tplFontClockBox()` 比例字体分支按 **2 个数字 + 冒号** 预留窗口：`long adv = 2L * digitAdv + glyphs[':'].adv;`。而 `device.now` 的字符串是 **"HH:MM" = 4 个数字 + 冒号**。ntthin18 实测（`src/font_noto_ntthin18.h:124-134`）：`'0'..'9'` adv=160（10 px）、`':'` adv=64（4 px）→ 真实宽度 **44 px**，代码只算出 **26 px**。
  - 后果：窗口 `x=69..94`（byte 窗口 4 B = 32 px），blit 上限 `bufW=32`、文本 `xOff=5` → **只能画到第 27 px**；字形边界为 `0:69-78 / 4:79-88 / ::89-92 / 分钟十位:93-102 / 分钟个位:103-112` → 窗口局刷只更新前 ~2.5 个字，**其余像素保持上一次整帧渲染的旧笔画**，视觉上是"新前缀 + 旧后缀"的怪字形（现场被读成 `04:64`）。
  - 整帧渲染不受影响（按画布裁剪，不按 box），所以"全刷正常、只有局刷坏"；这也解释了为什么加 ghost 预算/强制全刷都治不了根。
  - **位置来源**在设备上是编译产物（`v2ActiveLoad()` → `clkComputeRectCt()` 读 `v2Ct.ops[]`），**尺寸**却是运行时重算（`tplFontClockBox`）→ 两处不同源正是缺陷土壤；若模板给时钟加 `rect`+`align:center/right`，同一个错误 textW 还会让**位置**也偏 18 px（当前 `codex-status-a` 无 rect，故只有尺寸错）。
- **改动**：
  - `src/template_engine.cpp`：`2L * digitAdv` → `4L * digitAdv`（窗口 26→46 px，byte 4→7，104→182 B；Note4 `CLK_MAX_BYTES=512` 足够）；新增 `tplFontPropWidth()` 与"上次 blit 被裁掉多少墨水像素"的 `tplFontClockClipped()`。
  - `src/template_engine.h`：上述两个声明的公开声明。
  - `src/main.cpp`：`clockTickWake()` 增加**窗口容纳校验**（`need > avail` 或 `clipped > 0` → 不发这次局刷，改走全刷并记日志），这类几何错误以后会自己暴露；新增 `clkBaselineAfterPanelWrite()`，在**每次成功渲染后**（全刷总是、区域局刷仅当覆盖时钟窗口）重新 `clkCaptureFromFramebuffer()`——此前 light 下只要发生过一次帧渲染，`clkPixels` 就会与驱动 shadow 失配、窗口局刷被拒并**永久退化**为每分钟整帧渲染。
- **构建产物**：`.pio/build/zectrix-note4-b/firmware.bin`，1,758,032 B，SHA256 `42AAF00B257908434E86CEA45838DC60FD65FE8FD426D0632E570BD6BDBCF72E`；`FW_VERSION=0.18.23-note4-b`；镜像头 `byte[3]=0x40`（16 MB/40 MHz）；`sdkconfig.defaults` 首行 `# TASMOTA__9244f2068d3cf08d` ✓。按用户决定**未复制进 `artifacts/`**（`artifacts/` 写入被沙箱拒绝、提权被用户否决），ROM 保留在 `.pio/build/`。
- **烧录**：经桥内建 MCP 先 `power_plan {mode:"light", mac:7C4FADB93408}`（plan 659 排队）让设备在会合窗口升 light，再 `firmware_ota {rom:…, device_ip:192.168.3.177, device_mac:7C4FADB93408}` → **`0.18.21-note4-b → 0.18.23-note4-b`**（1758032 B），槽位 `ota_1`、`wake=power-on`。
- **判据命中（实机）**：`[clk] reserved x=69..114 y=5..30 box=46x26 win=7x182B`（修复前 `x=69..94 box=26x26 win=4x104B`）；整分钟 `[clk] tick build=866us wake=54652us write=709175us total=764790us`，**无** "too narrow"/裁剪告警，`epd_busy_fails=0`、`clk_partials` 递增。
- **未做/后续**：(1) v2 会合路径仍无 ghost 预算（`CLK_GHOST_LIMIT` 只在 legacy pull 与 light tick 的 per-boot 计数上生效）；(2) 消除"位置来自编译产物、尺寸运行时重算"的分叉——把测量宽度写进 `CtOp` 需要同步改 Rust 编译器与 Python 测试桥的哈希，属独立任务。

## 已下沉的历史节（2026-09-25 收纳）

以下两节的全文已移入 `docs/history/progress-archive-2026-09-25.md`（追加在文件末尾）：
- `合并 codex/fake（共享 v2 决策 + 设备模拟器）` —— 含合并后 ROM 表、包目录隔离的早期判据、`git stash` 现场事故；
- `桥侧修复：物理唤醒窗口取大 max(plan, t_boot+300s)` —— 含其"待实机复核"项与沙箱写入边界的原始记录。

其中的**当前**结论仍在本文或 `backlog.md` 里：ROM 现状见本文「多 env 实测」与 `backlog.md` §0；多目标隔离见 backlog C6；`git stash` 禁令与沙箱写入边界见 `AGENTS.md` 与本文末尾「沙箱写入边界」。

## 当前现场与待办（2026-09-25 核查）

| 项 | 值 |
|---|---|
| 1.54" 设备 | MAC `70041DD7A340`，名"书桌屏"，登记 IP `192.168.3.163`，`sync_enabled=true`，Profile `mini,quad` |
| Note4 设备 | MAC `7C4FADB93408`，名"Note4"，IP `192.168.3.177`，`sync_enabled=true`，Profile `codex-status-a` |
| 两设备可达性 | **2026-09-25 15:2x 已按键唤醒并实测可达**（`/status.json` + ARP 双证，MAC 与登记一致）；此前 09-25 05:2x 起曾深睡不可达 |
| 已装固件 | Note4 `0.18.23-note4-b`；**1.54 已于 2026-09-25 OTA 到 `0.18.23-bw`**（slot `ota_1`，ROM `91937B18…`） |
| 桥 | `bridge/target/debug/bridge-app.exe`（09-25 04:43 构建）；**运行中 PID 21848 + watchdog 41784**（15:13:45 起，8765/8766/8767 在听）——起桥须用非受限命令，见 A1/A2 节 |
| 1.54 队列/模板 | **已解决**：`bundle_jobs` 为空；`platform_publish` job `2bfc710c` = `succeeded`，设备 `committed_job_id=2bfc710c`、`v2_templates=2`(`mini,quad`)、`active=quad` 并已渲染。剩余：数据帧被拒 `incomplete`（桥侧契约）+ Profile 的 `static1` 绑定，见顶部节 |
| Note4 正常 | 最新 job `1b4500ad` `succeeded`；09-25 实机 `data_seq=applied_seq=63`（之后按其节奏入睡） |

**待办的唯一事实来源是 `docs/roadmap/backlog.md`**（紧急 A1–A6 / 暂缓 B1–B4 / 长线 C1–C7 / 技术债 D1–D13）。可直接交给新窗口执行的实现工作单在 `docs/roadmap/prompts/`。

本轮核查又关闭/下调了三条，都遵循**证据优先于旧文档**：

- **修正上一版的错误结论**：我曾写「v2 会合路径已覆盖 ghost 预算」并宣判旧记录过时——**那是错的**。已用代码核实：预算判定只在 `deepNetworkCycle()`（`main.cpp:5166/5177`），而它唯一的调用点在 `main.cpp:5697` 的 `else if (deepWakePath)` 分支内，进入条件是 **`!(deepWakePath && v2BundleReady)`**（`main.cpp:5678`）。Note4 有已提交 v2 Bundle → 走 `5678-5693` 分支 → **`deepNetworkCycle()` 从不被调用**，`rtcClkPartials >= CLK_GHOST_LIMIT` 在该设备上永不成立。更早的记录 `PROGRESS.md:37`（"v2 会合路径没有任何 ghost 检查"）**才是对的**，A3 是真需求。
  - 附带确认：深睡薄唤醒 `deepThinWake()`（`main.cpp:5275/5283`）同样无预算检查；且 `epdPartialCount` 是普通 RAM（深睡清零），而薄唤醒在 `main.cpp:5528` 早于 `5531` 的 `epdBegin()` 就返回了 → 深睡路径上**两个预算都不生效**。
  - `refresh_policy.cpp` 里两个 `RGN_CLOCK` 不是同一件事：`:97` 的 `5` 是 `classRank()` 的区域合并优先级，`:108` 的 `90` 才是 `classDefaultBudget()` 的每区域预算；而且它只被走 `epdFlush()` 的局刷消费，**直接写时钟窗口的路径根本不经过它**。`src/refresh_policy.h` 已被固件与宿主共同包含，适合放单一来源宏。
  - 工单见 `docs/roadmap/prompts/A3-note4-clock-ghost-budget.md`。
- 「族发布会把设备 `sync_enabled` 覆盖回 false」（backlog A6）**不成立**：修正本体在 `bridge/crates/app/ui/index.html:541`，引入于 `fc3c464`（2026-09-25 04:04:10），此后该文件未再改动；运行桥 `bridge-app.exe` 构建于 **04:43:36**，晚 39 分钟 → **当前桥已含该修正**，族草稿的 `false` 不会被复制。旧的「未构建」结论来自 `bridge-family-sync-preservation/status.md`（写于构建之前）。
- 「panel-power 改动是否已进 ROM 需确认」（backlog B1）**已确认进了**：`panel_pwr` 与 `note4RestoreFrameBaseline()` 引入于 `e226d8e`，早于 `0.18.23` 的源码提交 `af2b607`，且之后未再改 `src/` → 已 OTA 的 `0.18.23-note4-b` 就含这两项，**B1 只剩实机测量，不需要重编固件**；`artifacts/codex-status-0.18.21-…panel-modes.bin` 是过期候选。
- `bugs.md` 的 BUG-1 / BUG-2 **均已由 v2 实现修复**（ACK CRC 同源、deep 唤醒恢复数据检查点或轮换 context）；已结案并归档为 `docs/history/bugs-2026-09-22.md`（含文件行号证据）。
- `next.md` 的合并/多 env/包隔离工作已完成，已归档为 `docs/history/next-2026-09-25.md`；其 §5.2 的 `git stash` 流程在本机**不成立**，不要再照做。

## 历史归档

本轮把 `PROGRESS.md` 从 89 KB 压到约 17 KB，并归档文档（详见 `docs/README.md`）：

| 归档 | 内容 |
|---|---|
| `docs/history/progress-archive-2026-09-25.md` | 本次下沉：原 `PROGRESS.md` 111 行起的全部历史节（2026-09-24 及更早 + 合并/包隔离过程细节，约 71 KB，逐字节保留） |
| `docs/history/progress-archive-2026-09-23.md` | 2026-09-22 及更早的进度（通用平台 v2 实现、0.13–0.16 各轮实机修复） |
| `docs/history/workflow/<initiative>/` | 已结项专项原件，共 **17 个**（第一批 15 个；`generic-display-platform-design`、`live-template-delivery` 在 ACL 修好后补入） |
| `docs/history/next-2026-09-25.md` | 原 `next.md`（合并/多 env/包隔离，均已完成） |
| `docs/history/bugs-2026-09-22.md` | 原 `bugs.md`（BUG-1/BUG-2 结案） |
| `docs/history/acl-repair-notes.md` | 沙箱 ACL 机制、判据与修复脚本 |
| `docs/roadmap/backlog.md` | **待办的唯一事实来源**（紧急 A / 暂缓 B / 长线 C / 技术债 D） |
| `docs/roadmap/archive-digest-legacy.md` / `-recent.md` | 各专项一页式摘要与"还欠什么" |
| `docs/README.md` | 文档总索引与"该读哪一个" |

### 沙箱写入边界（2026-09-25 查清并已修，结论修正了旧记录）

旧记录把两次归档失败归为"属主 + DACL"两种原因，实测后**真正挡路的是 DACL 里那条沙箱能力 ACE**
（`S-1-4-1018769461-493222538`），不是属主：

- 判据：能写的文件 SDDL 里有 `(A;ID;0x110156;;;S-1-4-1018769461-493222538)`，不能写的没有。
  沙箱用受限令牌，`Authenticated Users`/`Users` 这类宽泛组 ACE **不生效**，写入只认那条能力 SID。
- 能力 ACE 是后加到仓库根的，**可继承 ACE 不回溯**，所以早期创建的文件一直缺它（`project-workflow/` 下曾有 86 个）。
- **补齐 ACE 后，即使属主仍是 `CodexSandboxOffline`，`git mv` 与 `Add-Content` 都成功** —— 所以属主不是闸门。
- 属主是 `CodexSandboxOffline` 但缺 ACE 时，沙箱既改不了属主也改不了 DACL；属主是 `cogic` 而只是缺 ACE 时，**DACL 改得动**（令牌有 `WRITE_DAC`），本次已补 90 个对象。
- **沙箱提权（含已批准的 `danger-full-access`）不会把令牌变成管理员**：实测 `elevated? False`，`SetOwner` 恒报 `Attempted to perform an unauthorized operation`。**改属主只能在管理员终端做**。
- 仓库内仍有 64+ 个对象属主是 `CodexSandboxOffline`（ACE 已补齐，不影响使用）；要清干净见 backlog D13 与 `docs/history/acl-repair-notes.md`。
