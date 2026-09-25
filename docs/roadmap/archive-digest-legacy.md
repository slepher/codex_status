# 旧专项归档摘要（2026-09-09 ~ 2026-09-24）

> 用途：`project-workflow/<name>/` 归档后的检索入口。每节保留：做了什么、结论/关键证据、还欠什么。
> 归档目录：`docs/history/workflow/<name>/`。
> 说明：本摘要只依据各专项自身的 `status.md`/`plan.md`/`design.md`/`results.md` 与 `PROGRESS.md` 标题写就；凡两处口径不一致的，都已注明哪一份更晚、以谁为准。

## codex-quota-display
- **目标**：在没有固件、没有桥的早期仓库里，用 Node 脚本产出确定性的 200×200 单色 Codex 余量预览图（`tools/generate-preview.mjs` → `artifacts/codex-quota-preview.png`）。
- **状态**：已完成（task-1..7 全部通过独立复核；当时仓库尚未 `git init`，故无提交，status 明确记 commit "inapplicable"）。
- **关键结论与证据**：
  - task-7 最终预览确定性哈希 `FE3E2953823C45205E05FCD451531C6DD0F11FBC5C0FEA41F6D7ADDC32DC51AC`，3667 黑 / 36333 白像素（该值是预览图哈希，不是 ROM 哈希）。
  - 四带几何：bands `68..72 / 77..96 / 103..122 / 127..131`，gaps `4/6/4`，中点 99.5，留白 67/67，条 89/98；两个验证层都逐项核对了这套几何。
  - task-7 推翻了早期"保留 status/simulated 与 used/left 标签"的布局：标签被移除，复核确认绘制指令中不再出现这些标签。
  - 布局演化链（各轮复核通过）：task-2 紧凑单色 → task-3 顶部紧凑 + 下部留白 → task-4 补齐 plan/user/5H/WEEK/reset/expiration → task-5 全局 RESET/EXP 与紧凑垂直居中 → task-6 每窗口 NEXT 时间 → task-7 去冗余标签的四带布局。
  - 验证方式：每轮都做"编码自测 + 独立 runner 复核 + review"三层；task-7 三层给出同一哈希与同一套几何/像素统计（review verdict: passed）。
  - 范围边界：本专项只有两个预期产物 `tools/generate-preview.mjs` 与 `artifacts/codex-quota-preview.png`（status 记录无 unexpected paths），不涉及固件、桥或模板协议。
- **未完成 / 遗留**：无（后续预览工具链已由 `tools/generate-quad-preview.mjs` 等接管，属新工作，不是本专项欠账）。
- **归档判定**：`可归档（只读历史）`。

## live-template-delivery
- **目标**：让真实 Codex 数据经 Rust 桥下发到设备，扩大 quad 布局，并做到一次固件能力升级后"换样式只换模板 JSON、不再刷固件"。
- **状态**：已完成（0.7.0-bw 端到端验证通过）；但 status 记录文档改动当时仍未提交（固件/工具提交已完成）。
- **关键结论与证据**：
  - 现场：设备 0.7.0-bw / 192.168.1.50 / 模板 3 个 / active quad `4827fea3` / 运行 ota_0 / reset reason software；ROM `artifacts/codex-status-0.7.0-bw.bin` SHA256 `F8CC10E0CC679537B2A7A3ED233266B36D334A718846B7FA7C9A088A22F4CC5D`。
  - 鉴权证据：`tools/device-auth/request_token.py` 经已绑定 BLE 链路协商 token（TTL 3600s）；`/update?token=` 200、Bearer 200、错 token 401、无 token `/update` 401、`/doUpdate` 401。
  - 提交：task1 `e89b0b1/f5e8e2e/0a30c58`；task2 `62bd15e`，display revision `6c39ead`；后续 `8ffc304`、`812170a`、`2fb5d95`、`2bfa034`（"Gate Wi-Fi operations behind a BLE-negotiated token"）。
  - task3 验收同时覆盖 0.5.0 OTA、BLE quad ACK、JSON-only 演示、username 标签、reset credits 为零；BLE 推送有正向 ACK（`artifacts/ble-0.7.0-final.log`）。
  - 后续用户特性：部分刷新 0.6.0/0.6.1（变化 >12.5% 或每 30 次局刷自适应全刷）、0.6.2 OTA 诊断与延迟重启、0.7.0 BLE token 门控 Wi-Fi 操作 + ArduinoOTA 口令随机化。
  - 显示规则在 plan 里被两次改写：先"缺席 5h 显示 infinity"（Task2 user clarification），后被"最新用户显示修订"取代为"缺席 5h 显示 100 且隐藏 5h 重置行"——以后者为准。
- **未完成 / 遗留**：
  - BOOT 短按切模板未手工验证 —— 已作废（后续 power-state 0.12.0 把 BOOT 语义改为单击=BLE 会话，pmstats task-2 又记录 0.13.2 起 BOOT 2s=只切模板，该遗留已失去对象）。
  - GATT 特征表变更后 Windows 会缓存旧属性，需解除配对再重配（或做 Service Changed）—— 仍有效，且已上升为 AGENTS.md 不变量。
  - 全刷仍每 30 次局刷周期性发生 —— 设计内行为，不是缺陷。
- **归档判定**：`可归档（只读历史）`。

## device-discovery
- **目标**：把设备身份从"IP 即身份"改为 MAC 唯一键 + 可编辑显示名，补齐发现链（属性 IP → UDP → ARP → BLE 兜底）并关掉 mDNS，同时引入显式 `POST /claim` 占用/租期。
- **状态**：已完成（task-1..5；固件 0.13.4-bw 已 OTA 并实测占用全流程）。
- **关键结论与证据**：
  - 用户决策（含修正）：MAC 为唯一键；名字可重名、只作显示主对象；**占用只走显式 `/claim`**，由桥按设备空闲自动 claim；`activate`/BSSID 变化/首个同步都不再转移归属（表中列出每个场景的显示层与占用层差异）。
  - 关 mDNS 的收益依据：`mdns_timer` 100ms → light sleep 期间每秒约 10 次唤醒（约 0.3–0.5mA 估算）；DHCP hostname 保留，桥/面板/MCP/device-auth/测试桥均不依赖 mDNS。
  - 协议要点：`POST /claim?id&name&host&port&lease&force&release`，lease 默认 300s（60–3600），被他人占用且无 force → 409 + 现 owner JSON；`usage`/`template` 永不创建或转移 owner，只刷新匹配 id 的 `last_seen`；`/status.json` 暴露 `owner`（空闲为 null）。
  - BLE 端到端已实测（用户单击 BOOT）：`device_discover via=ble` 复用 bond 读取 info（含 `mac`/`ip`/`http_port`），桥日志出现 `device info: {...}`。
  - 现场：桥为 debug 构建（父 PID 5260 + watchdog），固件 0.13.4-bw（ota_0）；证据散在各 task 文件与 PROGRESS.md。
- **未完成 / 遗留**：
  - ARP 邻居表冷路径（真实换网：设备换网 + 桥重启 + 不同二层）未复现，status 明确"留待现场" —— 仍有效（后续 PROGRESS 标题中未见闭环记录）。
  - 自动 `ble=1` 通告触发路径本次未捕获到该广播（UDP 广播偶发丢失）；机制未变、手动路径已验证 —— 仍有效（低风险）。
  - status 中"未提交：本目录、`docs/history/icons-task.md`、PROGRESS.md、`docs/power-state.md`、固件与桥代码（等用户同意）" —— 已作废（工作区此后已推进到 0.16.x/0.18.x 提交线，该待提交状态无意义）。
- **归档判定**：`可归档（只读历史）`。

## deep-pull-test
- **目标**：一次性量测"深睡定时唤醒 → 缓存 BSSID/信道快连 → 反向 `GET /usage` → 回 light sleep"整条链路的墙钟耗时与 CPU 时间，不做产品化。
- **状态**：已完成（2026-09-20 两轮，共 10 个 deep cycle 全部 `fast=1 code=200`）。
- **关键结论与证据**：
  - 测试固件在 `#ifdef CODEX_DEEPPULL_TEST` 下，版本 `0.13.9-dptest` / `0.13.9-dptest2`，构建期 `PLATFORMIO_BUILD_FLAGS=-DCODEX_DEEPPULL_TEST=1`，设备 192.168.3.163，桥 `:8765/:8766` 在线。
  - 快连（缓存 BSSID/信道、不扫描）关联+DHCP 典型 **1.3–1.5s**；HTTP 反向拉取 584B envelope **0.06–0.63s**；JSON 解析 ~1ms；单周期 **1.5–2.0s**（唯一异常 3.4s 是关联慢）；boot→拉取完成约 **2.7s**。
  - CPU 时间（dptest2 最后周期，`Time since boot up: 2 699 874 µs`）：SLEEP 592 505µs/21%、APB_MIN 615 936/22%、APB_MAX 736 099/27%、CPU_MAX 752 920/27% → CPU 运行态合计 2.11s（78%），light sleep 仅 0.59s。
  - 锁统计：wifi `APB_FREQ_MAX` 19 次共 1.11s（41%）；rtos0+1 `CPU_FREQ_MAX` 0.85s。
  - 推翻了旧估算：此前"大部分等待时间会 light sleep、CPU 0.3–0.7s"偏乐观约 **3–4 倍**。
  - 能耗粗算：单周期 ≈ **0.017 mAh**（240/80/40MHz ≈ 45/22/15mA、sleep ≈ 1mA）；1min 间隔 ≈ 24mAh/天、5min ≈ 5mAh/天；测试时设备在充电（batt 87→91%、4058→4122mV），数值仅供参考。
- **未完成 / 遗留**：
  - deep 快连窗口内不启动 WebServer，因此**桥在 deep 期间无法 push/OTA** —— 仍有效（产品化若要在窗口内 OTA，需窗口内起 server 或先切 light sleep；后续 sleep-modes 的 pending 排队正是对该限制的绕行）。
  - 设备已回滚原 ROM `0.13.9-dptest2 → 0.13.8-bw`（回滚 ROM SHA256 `2E5CF9826C94E6551B6C5C284AE248480035C68EC3F664A0D7A4F5D091FF1C5A`），`/status.json` 与桥 push/owner 正常 —— 已闭环。
  - 测试代码全部留在 `#ifdef` 下，默认构建零影响 —— 无需处理。
- **归档判定**：`可归档（只读历史）`。

## pmstats
- **目标**：免 USB 经 HTTP 读设备 PM light-sleep 统计（`GET /pmstats`）+ 桥面板加「功耗」tab；随后调研「~125 次/s 微睡眠」的成因与优化空间。
- **状态**：已完成（task-1 上线 0.13.0-bw；task-2 根因定位并落地修复 0.13.3-bw）。
- **关键结论与证据**：
  - 鉴权与节流决策：`/pmstats` 与 `/log`、`/status.json` 一致**免 token**（只读诊断，无密钥）；服务端 10s 缓存、**不参与 3s 轮询**（读本身会短暂唤醒设备、污染测量）；MCP 工具名 `pm_stats`。
  - task-2 根因（结论）：**不是异常唤醒源，是应用自己的轮询节奏** —— `loop()` 尾部 `delay(5)`（HZ=1000 → 5 个 tick）使每个 loop 迭代进一次自动 light sleep 并睡到下一个 tick，统计上即呈"~5ms 微睡眠、100+ 次/s"。
  - A/B 实测（`POST /diag?loop_delay=N`，60s 窗口）：5ms 默认 → 114–134 次/s、SLEEP 71–80%、CPU_MAX 17–19%；25ms → 45.6 次/s、86%、9%；50ms → 29.5 次/s、90%、7%；收益约 50ms 后饱和，100/200ms 只再省几 %，但 HTTP 往返成倍变慢、BOOT 单击可能落在两次采样之间被漏掉、OTA 吞吐塌 → 默认定 **25ms + 传输时自动 5ms**。
  - 排除项（都写进了 task-2 表格）：`esp_timer` dump 无 ~6ms 周期定时器（最高频是 mdns_timer 100ms=10/s）；wifi 唤醒仅 ~1/s，与 `listen_interval=10` × 100ms beacon 吻合（MAX_MODEM 不看 DTIM）；`FREERTOS_HZ=100` 只会更糟（`delay(5)` → `vTaskDelay(0)`，idle 永不入睡）。
  - ROM：0.13.1-bw `81B39D9E8EE4F4AD5D50808D44E8D9214776A04343253B1FB41EFEDF7F13C485`（`?timers=1`）；0.13.2-bw `D8D1CEF28FEC31A31CF4A4C8B27535B510BB69CC93052E09D8FFD741BAD15482`（sleep diag + `POST /diag`，并按用户要求 BOOT 2s 只切模板、不再自动开 BLE）；0.13.3-bw `7BA1171DB0CB6171EF2F8719A3DBA1F92A2F677AD57735438CC2D481DDAA7731`（空闲默认 25ms）。
- **未完成 / 遗留**：
  - **T10 拔电电池斜率对照（5ms vs 25ms，`battery_mv` 斜率 + `/pmstats`）未做** —— 仍有效。
  - 诊断面（`?timers=1`/`?diag=1`/`POST /diag`）去留未决，status 倾向保留（诊断价值高、成本低）—— 仍有效（低优先）。
  - `IDLE_TIME_BEFORE_SLEEP 3→5/10` 暂缓（已确认与根因无关）—— 仍有效（已判定无关）。
  - 当时未提交（`src/main.cpp`、`platformio.ini`、`docs/power-state.md`、本目录）—— 已作废（工作区已前进到更晚提交线）。
- **归档判定**：`可归档（只读历史）`。

## sleep-battery
- **目标**：解决 1.54" B/W 面板的电池续航 —— DEEP（深睡窗口，µA）与 LIVE（Wi-Fi keep-alive，0.5–2mA）双模式，由桥可达性驱动；权威规格是 `docs/history/sleep-plan-v4.md`。
- **状态**：部分完成。M1（0.9.0）/M2（0.10.0）完成；M3（task-7：pioarduino 自定义核心 + PM light sleep）在 status 时点仍卡在构建 spike（Arduino 库重编未完成）；M3 的实际落地由后续 power-state 专项基线固件 **0.11.9-bw（"PM light sleep + T9 已过"）** 接续。
- **关键结论与证据**：
  - status 快照 HEAD `de5c09d`；最新边界：stock env（0.10.3，可回滚）能从改动后的树干净编译链接（`pio run -e esp32-s3-epaper-154g` SUCCESS，337s，证明共享源码仍兼容 NimBLE 1.4.3）；设备仍跑 0.10.3-bw（ota_1）；回滚 ROM `artifacts/codex-status-0.10.3-bw.bin` SHA256 `C0AABDC117D88D8FB073306C27F0BAD5DB7967600A1ACFAF019B7EFE5780B61D`。
  - pm env 只能从**无空格 cwd** 构建：仓库路径含空格会被 pioarduino `custom_sdkconfig` 拒（`espidf.py:2629`）；需 junction 且让 pio 的真实 cwd 落在 junction 上（shell `workdir`/`Set-Location` 会解析成真实路径而仍失败）；subst 盘符也不行（`os.path.basename("P:\\")` 为空）。
  - `pio run -v` 会在 GBK 控制台挂住输出读取线程（UnicodeEncodeError），pm env 尤其不能加 `-v`。
  - SCons 4.11.1 按需装到 `~/.platformio/packages/tool-scons/scons-local-4.11.1`；短暂缺失会报 `No module named 'SCons.Tool.FortranCommon'`，重跑即好。
  - 当时设备 0.10.3-bw 在电池上约 30% / 3.59V 且下降，PM light sleep 是缓解项，因此要求尽早落地 0.11.0-bw。
- **未完成 / 遗留**：
  - 上述"路径含空格 / junction / 勿用 `-v` / SCons"构建约束 —— **已作废**（仓库已改名为无空格的 `codex_status`，AGENTS.md 已注明可直接在仓库目录构建）；但 AGENTS.md 目前仍引用本目录 `task-7.md §7` 作为踩坑出处，归档时需同步改引用。
  - M3 的 T9/T10/T11 硬件验收在 status 时点未跑（task-7 的下一步是重跑 pm 构建 → flash/OTA → T10/T9/T11）—— 本专项内未闭环；T9 与 PM light sleep 由 power-state 基线（0.11.9-bw）接续，T10 电池斜率在 power-state/pmstats 里仍列为待办。
  - 约束"不擅自停运行中的桥（bridge-app PID 43768）与设备服务"—— 属通用不变量，仍有效。
  - status 的"未提交"清单（`platformio.ini` pm env、`.gitignore` pioarduino 残留、`src/ble_bridge.cpp` NimBLE 1.x/2.x 双构建、`src/main.cpp`）—— 已作废（工作区已前进）。
- **归档判定**：`可归档（只读历史）`，但归档时需同步更新 AGENTS.md 对 `sleep-battery/task-7.md §7` 的引用。

## power-state
- **目标**：落地 v0.12 单模式常开电源状态机（空闲自动 light sleep、不主动 deep，仅三条 deep 路径），并同步桥侧：去待机模板、设备状态缓存、UDP 通告、按需 BLE、watchdog、桥 OTA 与 HTTP 模板推送。
- **状态**：部分完成。task-1/2/3/4/6/7 完成；**task-5（整机硬件/现场验收）未做**，status 明确"Remaining acceptance is task-5 hardware/field work only"。
- **关键结论与证据**：
  - task-1（FW 0.12.0-bw）：单模式状态机 AP / BLE ON / IDLE(BLE OFF) / WIFI OFF / 低电；`usb_serial_jtag_is_connected()` 插电判定、GP3 LED、BOOT 单击/2s/15s/30s、BLE 会话 120s + `bleDeinit`、WIFI OFF 节奏、低电 5% 断电、UDP 通告 8767、endpoint 自愈；`platformio.ini` 加 `CONFIG_PM_PROFILING=y`（pmstats 需要 `esp_pm_impl_dump_stats`）。
  - task-3/4：envelope 带 `bridge.host/port`；设备状态 10s 缓存 + UDP `:8767` 监听（仅已知 MAC，`ble=1` 触发一次 BLE cycle）+ 完全按需 BLE（无周期扫描）；watchdog `--watchdog <pid>`（父 exit 0 → 退出；异常退出重拉；5min 内 3 次则放弃并写日志）；运行证据 PID 49768 + watchdog 25644，kill/relaunch 已证。
  - task-6（桥 OTA）：MCP `firmware_ota {rom, device_ip?}`，token 缓存 `<data>/device-token.json`（401 自动重取一次），上传 multipart `POST /doUpdate?token=` 后轮询 `/status.json` 60s；实测连续 OTA 0.11.9→0.12.0→0.12.1→0.12.2→0.12.3→0.12.4→0.12.5，首次经 BLE 取 token、其后纯 HTTP。
  - task-7（HTTP 模板推送，FW 0.12.6-bw）：新增 `POST /template`（`endpointTokenAuthorized` 门控、raw JSON body、query `id/version/hash/activate`、`tplValidateForStorage` CRC/min_fw/dry-run、store+activate+redraw、`activeTplId` 缓存失效，400/401/413/500 路径齐全）；`bridge-mcp::push_templates_http` 被 `profile_push` 与面板共用，`PendingPush`/BLE 模板消费删除，BLE 循环只服务 UDP `ble=1` 端点/token 握手；实测 `profile_push default` → `pushed 1 (quad); skipped 2 unchanged`，二次 `pushed 0 (skipped 3)`。
  - ROM：0.12.5-bw `7F13489935AB7CB371370FA52B4553F7B7E114D8FF59BE13F81D8BAE7C2E5A8D`（0.12.5 按窗口时长分类并加 `monthly` 选择器 `windowMins >= 43200`；0.12.4 起模板投递改为仅显式推送）；0.12.6-bw `82B0AF5D52F6A718324FCB9B1DC3E3D41732019D8C6208E3E389C45751FF9E63`。
  - 回归：`cargo test --workspace` 16、python 4/4、node 7/7；quad v7 hash `c598adc0`。
- **未完成 / 遗留（task-5 逐项，status 原文列出，全部仍有效）**：
  - 按键（BOOT 单击/2s/15s/30s）与 GP3 LED 行为；
  - 插电/拔电宽限（grace）行为；
  - 低电断电（5%）；
  - WIFI OFF 深睡节奏；
  - T10 电池斜率测量；
  - T9 push 延迟；
  - UDP 换 IP 后 endpoint 更新实测；
  - 桥失联显示 `OFF N M`；
  - OTA 回归；
  - 三端模板哈希一致。
  - 附注：后续 0.15.x/0.16.x 有大量实机验证，但本专项内没有把 task-5 逐项闭环的记录，故按"专项内未闭环"保留；不得据此声称 task-5 已完成。
  - status 里"task-7 changes uncommitted"（`src/main.cpp`、`bridge/crates/mcp`、`bridge/crates/app`+`ui/index.html`）—— 已作废（工作区已前进）。
- **归档判定**：`可归档（只读历史）`（遗留是现场验收清单，已在摘要中逐条保留）。

## sleep-modes
- **目标**：v0.14 deep/light 双模式 + 时钟区域直写 + 定时反向拉取 + 离线重试；桥侧 `usage_rev`/`mode`/`next_contact_s`/`pending` 决策与排队；0.15.0 追加"时钟/时区随 PC"、"深睡切换历史"与 P2 端到端。
- **状态**：部分完成。主体已实现并多轮实机验证；P0 两处根因与 P1 图标问题均已修复并验证；长测与收尾未闭环。
- **关键结论与证据**：
  - 根因 #1（电池深睡后失联=唤醒即断电）：deep 期间 GPIO6/17 被 `gpio_hold_en`+`gpio_deep_sleep_hold_en` 冻结，唤醒后数字域复位输出寄存器=0，而 `releaseWakeHolds()` **先放 hold 再设电平** → GPIO17（BAT_Control，HIGH 锁存开机）被瞬间驱动 LOW → 锁存释放断电；0.14.3 实测 `nvs_stage_boot=90`（"准备深睡"之后连 thin 的 2 都没写）证明是掉电而非软件卡死；0.14.4 改为先 `pinMode+digitalWrite(17,HIGH)/(6,LOW)` 再 `gpio_hold_dis`/`gpio_deep_sleep_hold_dis`。验证：13:19:42 `POST /deep` → 13:20 thin 唤醒（码 2/30/3）→ 13:21:08 桥收到 `device pull: requested_next=60 rev=0 -> mode=deep next=900`，断电不再出现。
  - 根因 #2（pull 成功后卡死）：0.14.5 细码现场读回 `nvs_stage_boot=43` → 卡在渲染分支的 `epdBegin(false)`；机制是 setup 已调用过一次 `epdBegin(!woke)`，`DEV_Module_Init()` 二次 `SPI.beginTransaction()` 取非递归 `paramLock`，而本项目从不 `endTransaction()` → 同任务自锁、无界阻塞。修复：0.14.6/0.14.7 用静态标志让 `DEV_Module_Init()` 每 boot 只初始化一次、渲染分支仅在 `!frame`(OOM) 时重试、缓冲改为指针为空才分配（同时消掉重复初始化泄漏 2×5000B）。验证：0.14.6 电池、0.14.7 插电 + `deep_usb` 均通过，复位后读 `deep={clock_wakes:13, net_windows:1, net_fails:0, last_code:200}`。
  - P1（进 deep 后 Zzz 不显示/与 BT 残影叠加）：用 `frame_capture=1` 存睡前帧、`GET /frame?which=saved` 取回，比对模板图标区 **0/256 像素差**、`deep.glyph=5`，证明渲染确实执行且帧内容正确；定位为**局刷基线漂移**（`epdThinBegin` 的面板电源脉冲重置 SSD1681 控制器，而 `lastDisplayedFrame` 仍按旧内容算差异）。0.14.13 在 `enterDeep()` 渲染前 `epdPartialReady=false` 走 0xC7 全刷，用户现场确认 Zzz 显示、时钟每分钟更新。
  - 0.15.0：pull/push 用响应生成时刻覆盖 `server_time`（实测与墙钟差 **0s**）+ `tz_offset_min=480`；固件 `applyTzOffsetMin()`（POSIX 反向符号 +480 → `UTC-8:00`，±14h，变化才写 NVS `pm/tz`），OTA 往返后仍保持；深睡切换历史用 RTC 环 `HIST_CAP=120` × 12B（`epoch/ev/stage/batt/aux`，事件码 1 boot/2 enter-deep/3 thin/4 net-ok/5 net-fail/6 to-light），`GET /history` + `/status.json` 的 `hist_count`/`hist_head`。
  - 用户纠正推翻了一处原实现：900s 只属于设备连不上 Wi‑Fi/桥的失败退避，**桥可达应恒为 60s**（`activity.rs` deep 分支也回 60，Python 测试桥与单测同步，`docs/power-state.md` §13.3/§13.6/§13.8 修订）—— 即推翻 §8.4 里"静默→deep `next=900`"的旧行为。
  - ROM：0.14.13-bw `F6D9C2F085FF32A09A8A36DC66531CEAE46225F951EB1EB7803BAA0E58E05FCF`；0.15.0-bw（1 629 552B）`FCB9CF7560B367D1B9004B2FB424098FE5C9F56DAB1B27A8FF6F6658932771FA`；另有 0.14.3 `14A8EC17…`、0.14.4 `CA263411…`、0.14.5 `C3D1E484…`、0.14.6 `693CE5BB…`、0.14.7 `B963E166…`、0.14.8 `F9797CC6…`、0.14.12 `DD10BC1D…`；回滚 ROM 0.13.8-bw `2E5CF982…1C5A`。
  - OTA 逻辑缺陷（0.14.12 + 桥）：原 `/doUpdate` 忽略 `UPLOAD_FILE_ABORTED`，客户端中断后 `Update` 永久处于 already running、锁不释放、界面卡在 OTA 画面；现统一 `otaUploadCleanup()` + 20s 停滞看门狗 + `POST /diag?ota_abort=1`；桥侧上传前探测超时 2s→10s+重试一次、检查响应体 `UPDATE FAILED`、上传前先 `ota_abort=1` 清残留、队列 flush 失败退避 60s→2m→4m→…≤30m。
- **未完成 / 遗留**：
  - **长测 ≥2h**（60s 接触节奏下的电量斜率、时钟准度、残影、`/history` 逐分钟 thin 条目）—— 仍有效（后续只有 2026-09-24「1.54 时钟局刷交替问题…待实机验收」等条目，未见本专项的闭环结论）。
  - 手动唤醒现场验证（BOOT→light、再击→BLE）与 BLE 打断 —— **部分仍有效**：BOOT 唤醒路径已由 ble-rendezvous-power 现场验证（history `enter-deep(60)` → `boot aux=3`(EXT1) → `to-light aux=1`）；"BLE 打断"未见闭环。
  - 提交前整理：诊断设施（`frame_capture`、`nvs_stage`、`device_*` 调试工具）建议保留但默认关；审阅 `DEV_Module_Init` 幂等与新增全刷的影响；`/history` 暴露面审阅 —— 仍有效。
  - 非 bug 观察（需记住）：浅睡→深睡后一分钟内设备会按计划 pull，若桥的安静期不足 `QUIET_DEEP_S=600` 会回 `light` → 设备回在线、Zzz 被 light UI 覆盖；调试时用 `device_mode deep` 固定。
  - status 记录"未提交（等用户确认）"—— 已作废（工作区已前进）。
- **归档判定**：`可归档（只读历史）`；未闭环的长测与收尾条目须随摘要保留。

## generic-display-platform-design
- **目标**：产出一份实现无关、基于仓库事实的通用显示平台架构（多设备/多显示/模板驱动/多数据源/低功耗），交付 `docs/generic-display-platform-design-v2.md`。
- **状态**：已完成（设计定稿；实现未启动 —— 由 `generic-display-platform-implementation` 接续）。
- **关键结论与证据**：
  - 2026-09-21 独立 Astra 上下文产出 v1（先落 docs 根，后移入 `docs/history/generic-display-platform-design-v1.md`）；主评审确认覆盖四页产品模型、仅激活项参与编译的要求、观察到的设备导入与去重、不可变部署、多目标 ROM、类型化快照、原子存储、电源集成、迁移与测试。
  - 2026-09-22 产品决策简化模板管理：每个 template ID + render target **只保留当前最新内容**；Profile 只引用 ID；发布冻结一个临时 ReleaseArtifact 仅供队列一致性，**不是可选版本**；设备模板恢复/导入是边角流程。
  - 同日 v2 定稿并取代 v1 的冲突点：8 项 Profile、CompiledTemplate、单一 active context、完整 A/B Bundle、单一 PublishJob、Bridge 拥有的 push/pull 调度与 PowerPlan、设备拥有的有界执行/帧缓冲刷新。
  - v1 移入 `docs/history/`，所有活跃架构引用改指 v2；v2 是唯一现行平台架构，v1 仅作决策历史。
  - 校验方式：`git diff --no-index --check -- NUL <file>` 无空白错误（exit 1 属预期，因新文件与 NUL 不同）；本任务**只改文档**，未动源码/运行时/设备/部署/既有设计文档。
  - plan 的范围纪律：必须区分"当前事实 / 目标决策 / 未验证的硬件假设"，避免大爆炸式重写，并明确列出延后（deferred）范围；当时的 BLE 会合设计只作参考材料、不是更广架构的权威。
  - 迁移约束：既有行为与安全不变量（token 401、claim/lease、save≠push）在迁移期间必须继续可用；文档需覆盖产品/领域模型、固件/ROM 架构、Bridge 架构、模板编译与数据需求流、设备与显示 target、不可变部署、传输/电源集成、四页 UI、MCP 对等、从当前仓库迁移、验证与显式延后范围。
- **未完成 / 遗留**：无（"取得产品批准后拆分 M0 兼容夹具与 M1 application-service/frozen-queue"已由 implementation 专项的 M0–M7 承担）。
- **归档判定**：`可归档（只读历史）`。注意 `docs/generic-display-platform-design-v2.md` 是现行权威设计，它不在本目录内，归档不影响它。

## generic-display-platform-implementation
- **目标**：把 v2 设计落地为可用的通用多设备墨水屏平台（Codex 只是一个 DataSource；Bridge 拥有数据源/Profile/Bundle 冻结/PowerPlan；设备执行有界命令、编译模板与帧缓冲差分）。
- **状态**：部分完成。M0–M7 代码与宿主测试完成，且已在 200×200 SSD1681 实机验证（0.16.7-bw）；Note4 第二 target 由 note4-ota-bringup 接续；**字体资产 task-7 未完成**。
- **关键结论与证据**：
  - 实机（MAC `70:04:1D:D7:A3:40` / 192.168.3.163，固件 0.16.7-bw，`artifacts/codex-status-0.16.7-bw.bin` SHA256 `687A611B…250CB`）：OTA 0.15.10→0.16.x 共 6 轮各修一个硬件 bug；legacy 路径 `[v2] no committed bundle` 仍正常渲染 quad、`rgn n=13`；Bundle BEGIN/CHUNK/COMMIT 安装后 `v2_bundle=true`、3 模板、`commit_seq=2`；A/B 第二 bundle 使 seq 递增并产生新 context；data push `data_seq` 单调且字段 CRC 与桥一致；黑块反色数字局刷 `partial/ok dirty=37`、重复变化转 `full/clean`；空闲 `refresh_kind=none` 且 `frame == last`；正式 PowerPlan ids 1/4/6/7 且倒计时单调；陈旧 plan id 被拒 `stale_plan`；BOOT provisional `prov_rem=276`、桥先给 `granted=267` 再延到 600；桥不可达时约 t_boot+293s 关无线后进 deep；deep→timer 唤醒保持同一 `active_context_id`；A→B→A 远端激活产生三个不同 context；OTA 错 target 返回 401 + `[ota] rejected`；PM 79% light sleep / 2822 sleeps 且无泄漏 OTA/USB 锁。
  - 宿主测试：`cargo test --workspace`（隔离 `CARGO_TARGET_DIR=artifacts/cargo-target-v2`）**81 passed / 0 failed**；Python canonical hashes `c1a2faaf`/`e6ba459e`/`430cc188` 与 Rust+固件一致；`node tools/test-quad-preview.mjs` 7/7；`git diff --check` clean。
  - 400×300 Note4 模板 rev2 已保存（`codex-status-a` / target `epd-ssd2683-400x300-1bpp`，`source_crc=2b523381`、`compiled_crc=41c31abd`，`saved=true`、`published=false`）；测得 `GUI_Paint` 的 `Paint_DrawPoint()` 在 `Xpoint-1, Ypoint-1` 落笔（线/矩形类基元整体左上偏 1px，用 `Paint_SetPixel` 的图标不偏），且 `DRAW_FILL_FULL` 少填一行；这是 vendored 行为、固件与宿主一致，故共享引擎不动，由 400×300 模板补偿 —— 并明确推翻了"驱动/引擎有 bug 需改引擎"的直觉结论。
  - 字体引擎化（task-7 前半，宿主已验证，未 flash/未发布/未提交）：单一字体注册表 `TPL_FONTS`（9 项，**只能追加**，因 `CtOp.font` 是持久化索引）；新增 `ntthin18`（Noto Sans Thin 100@18，ASCII，blob 1412B，id `18c2e4ed`）与 `ntreg64`（Regular 400@64，表格数字，blob 2457B，id `4dc3b226`）；CSFN v1 容器 + 设备解析 `font_asset.*` + Profile 范围字体库 `font_store.*`（原子 tmp→verify→rename、CRC 复检、name==id、容量与 prune）；宿主测试 **110 passed / 0 failed**，`--compare-compiled --regions` 报 `json vs compiled diff pixels: 0`、round-trip 0、18 区域。用户决定**停止抗锯齿/2bpp-gray4 工作流**。
  - 顺带修好树内真实破损（否则宿主/固件路径根本不成立）：`rgnSetPanel` 在匿名命名空间导致宿主 FFI 无法链接、`compiled.rs` 调用旧 1 参 `rgn_build_compiled`、`rgn_build` 用 200×200 几何推导 400×300 区域、`RGN_MAX=32` < 模板 38 元素、`Rgn::area` 的 `uint16_t` 在 120 000 像素面板上溢出、`build.rs` 未跟踪字体头文件（预览会静默用旧字形）、宿主 LittleFS shim 不支持目录。
- **未完成 / 遗留**：
  - **task-7 字体资产剩余（仍有效）**：① 修完 `fs::File::name()` 的 `const char*` 问题后**固件尚未重编**（下一步是 `pio run -e esp32-s3-epaper-154g`；若 `checkprogsize` 失败，首选怀疑内置字体集，`nt16`+`nt30` 可由 `ntthin18`+`ntreg64` 取代并回收约 7KB）；② 模板→资产解析 `CtOp.fontRef` + `CT_ABI` bump（ABI bump 会使已持久化的编译模板失效直到桥重新发布，且解析失败须整份拒绝、不得半渲染）；③ 传输（`/status.json` 的 `font_inventory`、逐字体 BEGIN/CHUNK/COMMIT+ACK、幂等重复推送、"删除新集合不再引用的字体"步骤 —— `fontStorePrune` 已存在但尚无调用者）；④ 桥侧 wiring（`service.rs` 发布预览"将推送 N 个字体、X 字节、M 个已存在"、MCP 只读字体工具、UI 字体用量）；⑤ `codex-status-a` 的布局打磨与最终字重/字号决策（Thin@18 在真机上是 1px 笔画，可能偏淡）；⑥ 16MB Note4 板的 4MB `assets` 分区（目前仍用 LittleFS，字体 2–4KB 够用，assets 是未来大/CJK 载荷的家）。
  - **BLE 会合传输未实现（仍有效）**：v2 的协议/HTTP/PowerPlan 语义已实现，但设备仍走 legacy Wi-Fi deep pull；`convergence.md` 明确把 BLE rendezvous 列为未完成，PROGRESS 2026-09-24 标题亦为「唤醒会合诊断记录 —— 计划已定，未实施」。
  - 固定机位残影照片量化（用户推迟）与 stage 2 的 90 次局刷本地 rebuild soak —— 仍有效。
  - 400×300 变体的 canonical 仓库内 fixture 位置未定（交付物在 workflow 目录 + bridge v2 store，legacy seed 库 `tools/test-bridge/templates/` 故意不带该变体）→ 永久 400×300 parity 测试仍缺载体 —— 仍有效。
  - 四个 20×20 状态图标是视觉占位；独立的 live 蓝牙/Wi-Fi/host 状态绑定尚未定义 —— 仍有效。
  - `convergence.md` 的 6 条收敛工作（协议正确性、BLE 会合、单一编译格式、自包含 bundle、显示脏窗口、UI/target 清理）逐条仍有效；其约束"不得仅凭纯状态测试判定完成"仍适用。
  - 4 个 20×20 之外的另一处文档口径差异（**须注明谁更新**）：status.md 的实机验证表停在 **0.16.7-bw**，而 plan.md 约束段已写"当前 1.54 版本保持 **0.17.10-bw**"并规定 Note4 与 1.54 使用同一数字版本、仅以文件名与 `FW_VERSION` 后缀（`note4-b`/`bw`）区分 —— **以 plan.md 为准（更晚）**。
  - Note4 硬件事实清单在 status.md 内前后两处口径不同：前一段列为"启用 Note4 构建前必须确认的 4 类事实"，后一段「Note4 USB evaluation」声明取代前者并给出官方 GPIO 映射、实测分区表与 16MB/8MB 容量 —— **以后者为准**；但"本机 PCB 是否与官方 DevKit V1.0 一致"仍未实物核对，`TARGET_PARTIAL` 也仍需等到全刷/BUSY/方向被证明后才可启用。
- **归档判定**：`仍需保留在工作区（AGENTS.md 指明它是当前实现工作区；task-7 字体资产未闭环、BLE 会合传输未实现）`。

## ble-rendezvous-power
- **目标**：用短 BLE 会合取代 deep 模式每分钟 Wi-Fi pull（桥可选 NOOP / 直接 BLE 投递 / 租借 Wi-Fi light 会话），并定义大黑块的安全局刷策略；另加"静默唤醒"需求（任何唤醒都不得显示整屏 `Connecting:` 页）。
- **状态**：部分完成。设计定案（`docs/ble-rendezvous-power-design.md`）；task-2/3（静默唤醒，0.15.1/0.15.2）、task-4/5（stage 1/2，0.15.3/0.15.5/0.15.6/0.15.7）、task-10（0.15.9）已实现并部署；"never sleeps in light"修复（0.15.10）；**stage 3–6（task-6..task-9）未实现**。
- **关键结论与证据**：
  - 静默唤醒 0.15.2-bw（ROM `D20BBFEF10D40C47E64BAFFA5FFF70F81BE90575D29DAD66BD851C78C2CF0FCA`；status 注明此前条目与 PROGRESS 的记录"少一个字符"、已修正，以修正值为准）：BOOT 后立即去掉 Zzz、Wi-Fi 图标仅在连上后显示、回 deep 再隐藏；quad v11（hash `93199731`）让 Wi-Fi 图标条件于 `device.state` 并去掉斜杠 Wi-Fi 图标；现场 BOOT 通过（history `enter-deep(60)` → `boot aux=3`(EXT1) → `to-light aux=1`，`epd_writes=3`，`deep.glyph=5`）。0.15.1-bw ROM SHA256 `5F1F81A6941229476A194BBED9B09FBCBAD2F9294E59E853BB71E94FCB3E2D52`。
  - stage 1（0.15.3-bw，`DC6227B3EC4BA01F4A0FDD55D9AE79B166743CEE2BBF17A04EC1F9AD8B1437B2`）：基线证据归档 `artifacts/ble-rendezvous/`；RTC 审计（7680B 区、6000B 余量、5000B 帧放得下但未选）；EPD BUSY 返回传播（`epd_busy_fails`/`epd_trusted`）；`rv:2` 路由桩 + `rendezvous_v`/`rv_max` 能力 + `pm/rv2` 回滚开关，实机 `rv2=0`、`epd_trusted=true`。
  - stage 2（0.15.5-bw `078ABFC8180D21BF790CE110E97069EA270C5E3EAADF272BA55101FFD0EC6E9A`；0.15.6-bw `21404F60965F8214603D2BAED599180AD81431902B2333600FA2D4A7508F2F79`）：`refresh_policy` 语义区域 + 保守升级（照片门通过前 high-ink 一律全刷）、ghost 预算、clean/全再生路径、`/diag` 控制与遥测，宿主 `render/tests/policy.rs`；0.15.6 把区域合并从"字节扩展边界"改为**真实像素重叠**，修掉用户报告的"BOOT 点击刷整屏"（`[rgn] derived n=13 whole=0`）；同时修掉 OTA 后冷启动残留 `Connecting:` 页（链路起来后即渲染缓存 usage/status）。
  - 0.15.7-bw：定时 pull 离开 deep 时先置 light 再渲染并请求 `forceCleanRefresh`，消除"看起来还在睡"的 Zzz 残影；同时记录已知问题：同一唤醒在 timer 路径可能画两次帧（两次全闪），RAM-flag 修复被原型验证后按"非请求改动"回滚，留待 task-10 一并处理。
  - task-10（0.15.9-bw，1 644 256B，`47C641DF254449751DA6678F4503AA8276B527FCC3688DFABDA076024F9B8549`；中间版 0.15.8-bw `FFECE66BF1062926CD18BAD27127F1FFA74CCF129CC3F7572237FBA8B0CD5006`）：A 冷启动有缓存快照即渲染模板（`WIFI OFF`、无 `Connecting:` 页）；B 连接中 `device.state` 以 1Hz 交替 `WIFI CONN`/`WIFI OFF`，blink tick 走 `partial/blink`、绕过图标 ghost 预算与 30 次局刷 streak、仅限冷启动/BOOT 唤醒（deep 重试走 `deepFastConnect` 不闪）；quad v12（hash `430cc188`）加入 `WIFI CONN` 图标；`wakeBaselineDrawn` 修掉 timer pull→light 的重复画帧。测量（`/diag?blink_test=30`）：平均 870ms/tick、最差 875ms、波形平均 829ms、30/30 partial、预算未动 → 决定保留 1Hz（失败启动最差 30 tick ≈ 0.4mAh，按设计 §9 模型）。
  - "never sleeps in light"修复（0.15.10-bw，1 644 320B，`79B0412320C08CDB297FB3141FB0B3B7831FEF1BD240B13BDD553E281EDCFCD0`）：桥每 60s 续 owner lease 与 5 分钟心跳被设备计为活动 → 600s 本地空闲永不触发；桥侧 push 的 `mode_str()` 用 `expects_deep()`，但 10s `/status.json` 轮询与续约都算接触，于是每次推送都带 `mode:"light"`，light→deep 路径根本不存在。修法：续约不再重置设备 idle 计时（仅新 claim 重置）、`/usage` 推送仅在 `usage_rev` 变化时重置；桥侧 `activity.rs::light_wanted()` 由 pull 响应与 push `mode_str()` 共用，mode 变化立即推送、不必等心跳。落地依据是设计 §6"claim/续 owner 不能续 light"。
  - task-8 记录 OTA 现场事故：排队的 OTA 重试循环会把客户端上传全部中止，直到重启 `bridge-app` 才恢复（设备与 token 都正常，curl 26s 即传完 ROM）→ stage 5 需补 pending-OTA 可见性、preflight settle、更短的有界上传超时、push/OTA 串行化。
- **未完成 / 遗留**：
  - **stage 3–6 全部未实现（仍有效）**：task-6 stage 3 = 在既有 GATT 表上做 v2 事务（NOOP/BEGIN/CHUNK/COMMIT、CRC/幂等/ACK、加密与 owner 校验、有界 radio，仅显式测试会话）；task-7 stage 4 = 时钟优先会合 + radio 硬切断 + BOOT 300s 租约 + 鉴权 `POST /power` + deinit/OTA 收尾；task-8 stage 5 = 桥常驻监听 + 持久 revision 域 + WAKE/RENEW/SLEEP 调度 + owner 集成 + 队列端到端；task-9 stage 6 = 分阶段启用 + 实测功耗/Windows 门 + 照片验收 + 双阶段实验最后做。
  - 失败路径（AP/桥不可达 → 恢复 Zzz 帧并回 deep，在受限超时内）未测 —— 仍有效（是 plan 的验收条款之一）。
  - stage 2 的固定机位照片验收（需用户）与 90 次局刷本地 rebuild soak —— 仍有效。
  - stage 1 遗留：BLE 侧回归检查需要一次 BOOT 点击；基线照片由用户提供 —— 仍有效。
  - task-10 遗留：BOOT 按压 blink 检查、无 AP 失败运行 —— 仍有效（与 stage 2 照片门无关）。
  - task-8 的 5 项 OTA 改进（pending-OTA 可见性、preflight settle、有界上传超时、push/OTA 串行化等）—— 仍有效。
  - status 中"Bridge debug deep 已清回 auto"之类运行态设置 —— 已闭环，无需处理。
- **归档判定**：`可归档（只读历史）`；但 `docs/ble-rendezvous-power-design.md` 仍被 v2 实现计划列为"仍然适用的底层证据"，且 stage 3–6 与失败路径测试未完成，须随本摘要保留。

## note4-ota-bringup
- **目标**：为 Note4（ESP32-S3 / 4.2" 400×300 SSD2683）建立独立 PlatformIO 环境、16MB 分区布局、USB 首刷与一次受鉴权 A/B OTA，并只在本任务内改固件（不动 Bridge 源码与其 workflow）。
- **状态**：主体已完成（A/B OTA、USB 保持唤醒、PSRAM 与流式 Bundle 均已跑通并双通道验证）；现场验收项未闭环。
- **关键结论与证据**：
  - 设备身份：USB serial/MAC `7C:4F:AD:B9:34:08`，ESP32-S3 QFN56 rev 0.2，16MB flash（JEDEC `46/4018`）、8MB 嵌入式 PSRAM，USB VID:PID `303A:1001`（USB-Serial/JTAG）；分区 nvs `0x9000/0x4000`、otadata `0xD000/0x2000`、ota_0 `0x20000/0x5F0000`、ota_1 `0x610000/0x5F0000`、LittleFS storage `0xC00000/0x400000`；分区 bin SHA256 `3309265E5627F2B83D2152A1DB8BC851972B0F54983FE3E60A12C090D8B86F28`；40MHz bootloader `80F92A58A2C05EC25DF91BD838D977081FAA4438FFB27384BA6DF91CB937F0FB`；工厂整片备份 `366DEA39643855FD5250D15BB8F23DA3B363ECA1705A0068B9AB5A598E01D110`（16 777 216B）。
  - 0.18.19 A/B 验证：A 从 `ota_0`（USB 刷 `0x20000`，**只擦 otadata** `0xD000/0x2000`，NVS/B/storage 保留）、B 从 `ota_1`（**一次**受鉴权 multipart `/doUpdate`，HTTP 200 `UPDATE OK`，evidence 记 `upload_count=1`）。ROM：A `1EBEF24CA3AC722C2F43D17E06F6858CDDCCC706372BF6996F2C8CCE607AE35D`；B `E01BED2D64761910AFBF1624B0AE5DEF36CAECC90D36877D20B0D5557623936D`。B 通过 610s USB stay-awake 检查（`plugged=true`、`mode=light`、正式计划剩余 0、post-OTA hold 0、零 EPD BUSY 失败）。
  - OTA 客户端曾误报失败：postcheck 要求 `commit_seq` 恒为 41，重启后变 42（固件在易失 applied-data 基线未知时轮换 active context，`bsSetActive` 递增 slot commit seq）；Bundle job 与模板都已持久化 → 客户端改为"同一 committed job + 序列非递减"，原始证据与错误保留。
  - USB 掉线根因 #1（固件）：v2 正式计划到期**直接调 `enterDeep`**，绕过了 `idleDeepDue` 已有的 PC USB keep-awake 检查；现共享 deep 入口在 `plugged && !rtcDeepOnUsb` 时拒绝睡眠，v2 到期块在该条件下跳过转换与重复日志；`pollPlug` 需 USB SOF 缺失 10s 才判拔线（该板无 VBUS/CHG sense GPIO，USB 以 PC-host SOF 为准，见 `docs/power-state.md`）。
  - 掉线真相（推翻"设备已死"）：0.18.18-note4-b 显示首个 Bundle 后从 Wi-Fi 与 USB 消失，但 72s BLE 扫描找到 `CodexStatus-B93408`，`power_view_v2` 显示 20:00:28 与 20:01:37 两次会合成功（正式 sleep 计划 26/27）→ MCU 在按 60s 深睡会合循环，不是永久死机；旧镜像只是不提供稳定 HTTP OTA 窗口。
  - 首次 Bundle 安装失败的原因（被后续修正）：27 086B 冻结 Bundle 的七个分片全部 ACK（`ok=1`）、COMMIT 内容 CRC 通过，却被固件以 `rejected/owner` 拒绝 —— 冻结 Bundle **缺顶层 `bridge_id`** 而 COMMIT 带 owner `8c94`，`commit_seq` 保持 0；payload 字段与持久重封由并发 Bridge 任务修，本任务未做 owner 旁路。传输侧排除结论：4096B CHUNK 曾在 offset 8192/12288 报 Windows 10054、1024B 诊断在 14336 停滞，但 LittleFS 以 1KiB 块写/删 32KiB（含 16KiB 边界）每次 0–16ms 成功 → 排除"固定 1024B HTTP 上限"与"16KiB LittleFS 边界失败"；改成 Note4-only raw callback 把 CHUNK 直接流入 LittleFS（用解析器 1436B 缓冲），桥侧补 `X-Request-Id`/`X-Session-Nonce`/`X-Offset` 并保留 4096B 分片，配对后整条 chunk+CRC 路径通过。
  - PSRAM：诊断暴露 `heap_max_alloc=15348`、`psram_free=0`，尽管 esptool 报告 8MiB 嵌入式 PSRAM —— Note4 env 继承了**无 PSRAM 的 S3 板定义**；加 `board_build.psram_type=opi` 与 `BOARD_HAS_PSRAM=1`（保留 16MiB DIO/40MHz flash）后 `psram_free=8,351,272`、内 heap 59,232（中间 ROM `artifacts/codex-status-0.18.18-note4-b-psram.bin` SHA256 `7C6D4147FE4F01A5FB4782E3589C730C7B1DE2FF25ECDABEBD751513018CD40A`）。
  - 早期 NVS/LittleFS 故障结论：80MHz bootloader/app 下出现 `0x110B`（`ESP_ERR_NVS_INVALID_STATE`）与 LittleFS 挂载失败；换 **40MHz bootloader + app** 后转为 `[tpl] store: 0 template(s)` 与 `[bundle] fs ready`，随后 Wi-Fi 与 auth token 正常保存/重载 → 观察到的原因是本机 80MHz bootloader/flash 配置，**不是已证实的 NVS 数据损坏**（诊断擦除过的工厂 NVS 元数据仍有备份）。
  - Bridge 侧（本任务未改 Bridge 源码）：运行时改绑到隔离数据 `artifacts/note4-bridge-data/`，本地 Profile `template_ids=[codex-status-a]`、`initial_active_id=codex-status-a`、`sync_enabled=false`，**未发布**；旧 1.54 绑定备份在 `artifacts/bridge-before-note4-bind-20260923/`（`bridge-app.json` `57AE1435C12ACCD45C8BC697093EAE5822C2D11E0B1A4A25E4753B00643206AC`、`device-record.json` `E4DAA83072F0DE64450554C3365074318941BB4E9BFF926D0DA329267C15AFBF`，不含 token/Wi-Fi 口令）。当时二进制把 Note4 注册成 `width=200,height=200`（实际 400×300）且保存时忽略 Profile 的 `render_target`，导致发布受阻；处理方式是"不得改持久化 capability 绕过"。
  - 恢复流程（已文档化）：进入 ROM download 模式后用官方的"按住正面圆 BOOT、点按侧边 RESET、松开 BOOT"序列，先核对 VID/PID 与 MAC；回退到已知良好 A 就写 A 到 `0x20000` 并**只擦** otadata；整片工厂恢复则写 16MiB 备份到 `0x0`（会覆盖新的 Wi-Fi/BLE 状态）。**两条路径都未执行**。0.18.13 A/B ROM：`57CDB610334908606EE07D6315CEBC45C388771A7D08BF839C4D3C4350737473` / `0D5ADA4C3D559B98B6B2139A2EA5346B6EA0678D6976C8B73B0CF315D64510E7`（各 1 723 696B）。
- **未完成 / 遗留**：
  - **像素级正常 UI 文字验收未通过** —— 仍有效：现场照片仍偏软、无法做像素级判读；status 的 completion check 明列该项未打勾（后续 PROGRESS 2026-09-24「Note4 400×300 图标与状态栏校正 —— 模板源码完成，待发布实机」与 2026-09-25「显示修复：时钟局刷窗口宽度算错 —— 已实现、已 OTA、判据命中」说明显示链路在改进，但本专项目录内没有像素级验收结论）。
  - **按键/唤醒与更宽温度范围未测**（适配器使用全刷 + 室温回退）—— 仍有效（后续 PROGRESS 2026-09-24「Note4 ENTER 唤醒与 1.54 推送只读排查 —— 进行中」）。
  - **两条恢复路径（B→已知良好 A、整片工厂恢复）文档化但未执行** —— 仍有效。
  - 早期 A/面板证据只到"full-black/half-black 可见、零 BUSY 失败"；相机照片的 180° 方向问题被用户澄清为"相机把设备拿反了（按键在屏下方）"，因此不存在驱动旋转 bug，临时 180° 修正已在 build/flash 前回退 —— 已闭环。
  - "未发布 / 设备报零安装模板 / Bridge capability 契约不一致阻塞发布" —— **已作废**：被后续 PROGRESS 2026-09-23「Note4 首次 Bridge Profile 实机发布 —— 模板已安装并显示」取代（本任务不得编辑 Bridge 源码）。
  - 本目录含 5 个可复用脚本：`ota_verify.py`、`ota_upgrade.py`、`provision_endpoint.py`、`usb_recover_once.py`、`capture_serial.py` —— 归档后若需复用，须从 `docs/history/workflow/note4-ota-bringup/` 取（提醒项，非缺陷）。
  - 2026-09-23 时点**无 Git 提交**（按规则未经用户要求不提交）—— 已作废（工作区已前进到 09-24/09-25 的提交线）。
- **归档判定**：`可归档（只读历史）`；恢复流程与 5 个脚本须在归档目录保持可达。
