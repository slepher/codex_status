# Task 4 — 第二阶段：BT modem sleep + DFS 实测定值

Status: pending（第一/二阶段可独立验收；本任务决定 3s 窗口的能量收益）

## 背景

- 现状 sdkconfig：`# CONFIG_BT_CTRL_MODEM_SLEEP is not set`、`RTC_CLK_SRC_INT_RC`；
- 无省电时窗口均值 30–80mA；开 modem sleep+DFS（无 light sleep）后广播 800ms ≈16.4mA（esp-idf #947 Espressif 实测）；
- light sleep 档（≈4.2mA 广播 / 1.8mA 连接）需要准确的 32k 时钟；本板晶振挂在 PCF85063 上、未证实接到 ESP32 XTAL_32K_P/N，暂按不可用。

## 改动

1. `platformio.ini` custom_sdkconfig 增加：
   - `CONFIG_BT_CTRL_MODEM_SLEEP=y`（名称以当前 IDF 版本 Kconfig 为准，构建后核对生成的 sdkconfig）；
   - 保持 `CONFIG_PM_ENABLE/DFS_INIT_AUTO`；
2. `src/main.cpp:363` min_freq 80MHz（与 task-1 同）；
3. 若固件在 BLE 会话期间持有 PM 锁导致降频不生效，按 IDF 文档确认锁行为，不绕过安全语义；
4. DFS 自身作为独立 A/B 臂：`min_freq` 40 vs 80（同 base env），以 `/pmstats?diag=1`
   的 mode residency（SLEEP/APB_MIN/APB_MAX/CPU_MAX）核对会合窗口内是否真降频、哪些锁
   挡住降频；臂与判定统一见 `task-6-bridge-first-impl.md` §5（A2/A3）。

## 验收与回归

- 实机实测：窗口期平均电流（PPK2/Joulescope 或按 pmstats 口径估算）与广播/连接稳定性；
- **估算口径（无电流仪，2026-09-22 定）**：base 与 btpm 各 ≥30 个 deep 周期，用
  `node tools/estimate-power.mjs` 输出每周期 awake/BLE/render 分段与 mAh/day 区间对照
  （见 task-3 §7）；同表记录连接成功率与 connect/discovery 时长；
- 回归：BLE 连接成功率、`reserved` 计划处理、OTA、Wi-Fi light 会话不受影响；
- 若出现连接不稳（社区已知 40MHz min 会掉线），回退 min=80 并记录；
- 结果决定：3s 窗口保留或回调至 1.5s（能量差 ~10mAh/天）。

## 风险

- BT modem sleep 与 Wi-Fi/BLE 共存、与 DFS 组合存在历史 bug（esp-idf #947/#15891），需整机回归；
- **#15891（2025，open）**：BLE→Wi-Fi→BLE 循环后残留 **~2mA**（coex 资源不释放）；我们“BLE 会合
  + 偶发 Wi-Fi light”会踩到，A/B 必须包含“经历一次 Wi-Fi light 后”的基线，而不只开机态；
- 无 32k 晶振时勿尝试 light sleep + BLE；
- A/B 附加项：连接成功率、connect/GATT discovery 时长、单次 wake `awake_ms`（modem sleep 不应
  让握手变慢）；PC 蓝牙与 2.4G Wi-Fi 共射频时，PC 侧 coex 也会影响命中率，记录 PC 网络形态
  （有线/5GHz/2.4GHz）。

## 2026-09-27 Note4 DFS 80/40 MHz 短测

- 仅改 Note4 固件版本标记和 `esp_pm_configure` 下限 80→40 MHz，构建/OTA 后用正式 Plan 从 deep timer 经 BLE 进入 light 读 `/pmstats?diag=1`；测试后已刷回 `0.18.25-note4-b` 并 ACK 正式 sleep Plan 673。ROM 身份、SHA、原始证据见 `PROGRESS.md` 顶节。
- 80 MHz 生产版在 light boot 142.449s 时，APB_MIN 80MHz 累计 0.100998s；40 MHz 实验版真实 timer BLE 会合后的同次 boot 29.885s 与 116.614s，APB_MIN 40MHz 都是 **0.101291s**。其间多出的约 86.7s 并未在 40MHz 活跃档度过，主要分配给 SLEEP、APB_MAX 80MHz、CPU_MAX 240MHz。PM 统计按 boot 累加，不是逐阶段计数；不能确认 BLE 扫描/广播单独在哪个档，更不能把 SLEEP 行显示的 40M 当成 CPU 在 40MHz 执行。
- 40 MHz 正式 timer BLE 窗口仅 4 次：2 回答、2 未连接；80 MHz 既有历史 24 answered/9 no_connection，时间与样本不同，不能推断连接率差异。没有电流仪，不报告 mAh 节省；未启用 BT modem sleep。
- 2026-09-27 追加真实 deep timer BLE 等待运行点采样：`0.18.26-note4-b-pmw5` 在广播到 GATT 连接的 1,401 ms 内，每轮读取 `getCpuFrequencyMhz()`，141/141 次在任务运行点读到 240 MHz；起点与连接时也读到 240 MHz。**2026-09-28 口径更正**：这会漏掉 `delay(10)` 间隔中的降频驻留，不能代表整段等待的模式时间；原始日志 `artifacts/note4-pmw5-blewake-20260927-234706-log.txt`（ignored）。
- 代码顺序解释了实测：timer wake 的 `v2Rendezvous()` 先于 `startNormalMode()` 内的 `configurePowerManagement()`；PM 模式表也在 BLE 阶段尚不可用。因此此前 40/80 MHz 配置只作用于会合后 light 阶段，不能用于衡量 BLE 等待降频。下一次 A2 应先设计 BLE 阶段启用 PM/锁的安全位置，再验证实际频率、连接稳定性、≥30 周期和电流/能量；不能仅改 `min_freq` 后宣称 BLE 等待节能。20/10 MHz 暂不优先。

## 2026-09-27 用户指定的唤醒后 10 MHz 临时 A/B

- 目标：仅 Note4，在 timer deep wake 的 `v2Rendezvous()` 开始、BLE 控制器启用前配置 DFS（max 240 MHz、min 80 或 10 MHz、自动 light sleep 先关闭），记录 PM 配置结果、广播到 GATT 连接的逐轮 CPU 频率采样、PM 模式驻留、连接/计划 ACK 与窗口耗时。后续正式 light 路径仍按原逻辑配置 240/80 MHz。
- 顺序：先 80 MHz 早配置基线，再 10 MHz 同路径；各做至少一次带正式 Light Plan 的 timer BLE 窗口，观察额外自然窗口与 Bridge 扫描日志。若 10 MHz 配置被拒或连接不能稳定完成，记录失败并立即回退；不把频率采样换算成电量。测试结束恢复生产 `0.18.25-note4-b`、正式 sleep Plan 与工作区 `rf1` 源码/ROM。
- 判断：若 BT 的 `ESP_PM_APB_FREQ_MAX` 锁让 BLE 窗口全在 80/240 MHz，10 MHz 对等待没有节能依据；即使出现 10 MHz，也需测电流与同条件 ≥30 周期后才能定量收益和可靠性。

### A/B 结果

| 下限 | BLE 广播到 GATT 连接 | APB_MIN 增量 | APB_MAX 80 MHz 增量 | CPU_MAX 240 MHz 增量 | 正式 Light Plan |
|---|---:|---:|---:|---:|---|
| 80 MHz | 1,723 ms | 0 | 1,336,940 µs | 385,793 µs | 709 `applied/600s` |
| 10 MHz | 2,043 ms | **0** | 1,588,741 µs | 454,390 µs | 713 `applied/600s` |

- 两臂 `esp_pm_configure(max=240,min=80/10,light_sleep=false)` 均返回 `ESP_OK`。每臂各一次成功 timer BLE 窗口，模式时间为窗口起止累计差值，不受运行点采样偏差影响。10 MHz 下限在 BLE 等待没有驻留收益；两臂约 78% 为 80 MHz、22% 为 240 MHz。BT modem sleep 未在 Note4 base env 启用，ESP-IDF 的 BT APB 锁与现象一致。
- 10 MHz 测试后 Wi-Fi/light、模板显示正常，未观察到配置失败、重启或本次连接失败；1 次/臂不足以判断 320 ms 连接差或长期连接稳定性。无电流仪，不能量化电池收益。测试 ROM、原始日志、同 MAC 版本确认、生产回退见 `PROGRESS.md` 顶节。实际建议是先评估提前启用 80 MHz DFS；无需继续为当前 BLE 等待追 10 MHz。

## 2026-09-28 Note4 BLE 省电三臂实机计划

- 仅目标 `zectrix-note4-b`、登记 MAC `7C4FADB93408`。单份实验 ROM 编入 `CONFIG_BT_CTRL_MODEM_SLEEP=y`，选 main XTAL 作 BLE 低功耗时钟；在控制器已启用后用 IDF `esp_bt_sleep_disable/enable` 切换 A/B，避免三个不同编译产物带来的系统差异。三臂分别为 A：modem sleep 禁用、DFS 下限 80 MHz；B：modem sleep 启用、下限 80 MHz；C：modem sleep 启用、下限 40 MHz。BLE 会合前先 `esp_pm_configure(light_sleep=false)`，不在本轮启用 BLE light sleep。
- 通过 token 保护的诊断参数保存实验臂；每轮 deep timer 唤醒都回读该设置。在广播开始、连接建立、窗口关闭处记录 PM 模式累计及 `esp_bt_sleep` 返回码；每臂至少 30 个 deep 周期，记录成功/超时、会合时长、awake/BLE 时长、PM 驻留。正式 Wi-Fi/light Plan 与渲染路径保持原功能；每臂观察一次 BLE→Wi-Fi/light。若 B/C 连接或正常模式受损，立即回退 A 并刷回原生产 ROM。
- 使用相同的 3 秒可连接广播窗口、Bridge、模板、网络环境比较。估算先以驻留时间和官方示例电流作为条件区间，再用本项目 `estimate-power.mjs` 的 awake/BLE 分段预测日耗电；官方板级电流不能当作 Note4 实测。最终恢复生产 ROM、原 Profile 和 sleep Plan，记录 ROM SHA、原始日志及实际样本量。

### 三臂实机结果（2026-09-28）

实验 ROM：`artifacts/note4-0.18.29-btpm-fast-test.bin`，`zectrix-note4-b` / `zectrix-note4-400x300`，1,767,616 B，SHA256 `89681437DC39CB684C401B3A9EBC50184089FA32E5E33AB80C4812674F92F402`；OTA job `797fcc7a` 上传 ACK，设备同 MAC 自报 `0.18.29-note4-b-btpm`。精确在机镜像哈希仍不可由设备自证。所有臂在 BLE 前配置 DFS，`esp_pm_configure` 和 `esp_bt_sleep_enable/disable` 均返回 `ESP_OK`，`light_sleep=false`。实际测试把设备周期临时设为 15s，并经历正式 BLE Plan → Wi-Fi light → 下一轮 deep；与生产 60s 周期不同。

**Bridge 节奏修正**：初试仍用同 MAC 成功后 55s 冷却，A 组 33 周期仅 12 次回答，此数据只保留作误配证据，不能用来比较能耗。临时 Bridge 仅对实验版本的 Note4 改成 8s 冷却（其它设备仍 55s），单元测试 3/3 通过。下表是修正后重置设备计数、重新采集的三组，分母是设备自身 `/btpm.json` 的完整 BLE 窗口周期，分子是设备收到有效答复的次数。原始 `/btpm.json`、`/status.json`、`/history`、`/pmstats?diag=1`、`/log` 均存于 ignored `artifacts/btpm-{a,b,c}-bridge8-*`。

| 臂 | 控制器 / DFS 下限 | 回答 / 周期 | 平均 BLE 窗口 | APB_MIN | APB_MAX | CPU_MAX | SLEEP |
|---|---|---:|---:|---:|---:|---:|---:|
| A | modem sleep 关 / 80 MHz | 33/34 (97.1%) | 2.848s | 1.0% | 72.1% | 26.9% | 0 |
| B | modem sleep 开 / 80 MHz | 32/32 (100%) | 2.950s | 24.9% | 47.6% | 27.6% | 0 |
| C | modem sleep 开 / 40 MHz | 30/34 (88.2%) | 2.858s | 38.2% | 30.0% | 31.8% | 0 |

APB_MIN 在 B 是 80 MHz、在 C 是 40 MHz。按每周期计，A/B/C 的 APB_MIN 分别为 0.028/0.733/1.092s。A→B 说明 BT modem sleep 释放 APB 锁让 80 MHz 最低档有实际驻留；B→C 又让 40 MHz 档占 38.2%。SLEEP 为 0 是预期：这轮关闭自动 light sleep，不能借此证明 BLE light sleep 的收益或稳定性。C 组有 4 次未答，A 有 1 次，B 无；各组约 30 次，不足以把差异归因于 40 MHz，但现有证据不支持直接把 C 设成生产默认。窗口耗时三组相近，未见 B 明显变慢。

**C 组 4 次未答的逐轮归因**：设备 `/history` 中 seq 143、144、158 是 `no_connection`：3s 可连接广播窗口到期，设备未看到连接。Bridge 日志对 143、144 对应的时段确实发现了 Note4 广播，随后多次 `connect_failed`；158 的 3s 窗口内没有 Note4 广播命中记录（Bridge 同时服务另一台设备），不能断言是设备 40 MHz 失效。seq 163 是 `no_command`：设备约 1.6s 看到 BLE 链路，但连接后额外 6s 内未收到有效命令；Bridge 在同一时段多次发现 Note4 广播，`connect` 阶段反复失败，其中一次耗时约 4.0s。可判定的是“3 次连接截止 + 1 次指令截止”，更深层原因是 Windows GATT 建链/调度、扫描相位、并发设备或 40 MHz 影响，当前日志无法区分。延长到 5s 可连接窗口和 9s 已连接命令窗口是合理的**下一轮诊断臂**；须保持 Bridge 8s 节奏、同条件再采 ≥30 周期，并以设备回答率和额外 BLE 开机时间共同评判，不能直接改生产值。成功轮在收到 ACK 后约 200ms 关闭，不会被新上限强制拖长。

**电量估算口径**：PM 表只给频率模式时间，不给 RF 实际占空或板级电流。以生产假设每分钟一次会合（1,440 次/天），窗口平均电流每下降 1 mA，每日约少 `2.9s × 1440 / 3600 ≈ 1.16 mAh`。B 相对 A 多出约 0.705s/周期的 APB_MIN 驻留；若该被释放锁的区间实际少 10 mA，则对应约 2.82 mAh/天，**10 mA 是情景假设，不是测量值**。C 的 40 MHz APB_MIN 为 1.092s/周期；若这一整段原本在 80 MHz 且与 Espressif 芯片 modem-sleep WAITI 条件相同，官方 40/80 MHz 典型差额 8.8–17.3 mA 对应约 3.84–7.56 mAh/天。这个数未计 C 更高的 CPU_MAX 比例、额外漏会合、面板/Wi-Fi/PSRAM 电流，不能视作 Note4 净节电。官方来源：[ESP32-S3 芯片数据手册 §5.6](https://documentation.espressif.com/esp32_s3_datasheet_en.pdf)、[ESP-IDF 电源管理](https://docs.espressif.com/projects/esp-idf/en/release-v5.4/esp32s3/api-reference/system/power_management.html)。项目 `estimate-power.mjs` 对所有臂固定使用同一 BLE 平均电流区间，故不能据其输出区分 BMS 开关，本轮没有伪报该工具的日耗电差值。

**取舍**：B 是下一轮上电流仪测量和长时连接回归的候选；当前生产 ROM 与 Bridge 均已回退，不发布 B/C。C 需先复测连接成功率、最好交错 A/B/C 顺序，排除无线环境和扫描相位；电流仪还应覆盖 BLE→Wi-Fi/light 后的残余电流及整周期，而不是只看 PM 档。回退记录见 `PROGRESS.md` 顶节。

### C 组延长窗口对照计划（2026-09-28）

- 用户要求实机测试 C 组延长等待。保持 Note4 登记 MAC、modem sleep 开、DFS 下限 40 MHz、main XTAL、15s 测试周期、相同 Bridge 8s 同 MAC 冷却与 30–60ms 可连接广播间隔；只把未连接截止从 3s 改为 5s、已连接等命令截止从 6s 改为 9s。使用独立版本 marker 与单份 Note4 测试 ROM，不触及 1.54 目标。
- 先检查默认 Bridge 现场并启动，构建实验 ROM/隔离 Bridge EXE、核验目标/大小/SHA/marker 后按 MAC OTA。设备通过正式 light Plan 进入 Wi-Fi，清零诊断计数后正式 sleep，收集至少 30 个真实 deep timer 窗口。记录设备 `answered/no_connection/no_command`、窗口时长、PM 档、Bridge 原始连接错误与 GATT 成功；对照原 C 30/34、2.858s、APB_MIN 38.2%。
- 如果 5/9 秒仍出现建链失败，要区分“广告未命中”和“已命中但 Windows `connect` 失败”；延长截止不是建链错误的根因修复。若失败变少，按新增 BLE 开机时间评估电量代价。测试后顺序 OTA 回退 `0.18.25-note4-b`、正式 sleep Plan ACK、恢复生产 Bridge EXE 和工作区原 RF1 源码/Note4 ROM，并更新 `PROGRESS.md` 与 backlog。精确在机 ROM 哈希仍需设备能力才可证明。

### C 组 5s/9s 实机结果（2026-09-28）

- 测试 ROM `artifacts/note4-0.18.30-btpm-c59-test.bin`，`zectrix-note4-b` / `zectrix-note4-400x300`，1,764,368 B，SHA256 `4E70D593D0CE871E1817D69B4C278A6F2284E59078F230B2D3EB8AF36A1BE06F`；同 MAC `7C4FADB93408` OTA job `b43cbe2b` 上传 ACK、预期版本 `0.18.30-note4-b-btpm-c59` 已观察。Bridge 仅对该实验版本使用 8s 冷却，隔离构建测试 3/3 通过。设备在正式 sleep Plan 803 ACK 后清零计数，按 15s 计划采样；结束时 light Plan 813 ACK 后读取设备证据。
- `/c59.json`：34 个完整 BLE 窗口、34 次收到有效答复，窗口累计 77.422s，平均 **2.277s/次**。RTC 累计 APB_MIN(40 MHz) 27.135s、APB_MAX(80 MHz) 25.600s、CPU_MAX(240 MHz) 24.693s，分别占 35.0%/33.1%/31.9%；自动 light sleep 仍为 0。`/history` 保存了读取前的 33 个完整 timer BLE 周期，全部 `answered`，平均总清醒 3.664s，最长 7.000s。两次连接建立于广播后约 4.3s/4.2s，证明 5s 上限在本轮确实覆盖了超过旧 3s 截止的成功轮；未见连接后等命令超过旧 6s 上限的轮次，因此 9s 上限收益尚未单独验证。Bridge 在本轮记录若干 `error=Not connected` 的短暂 `connect_failed` 后继续重试；错误本身不是设备已收到 GATT 命令的证据。
- 同口径旧 C 为 30/34、平均窗口 2.858s、APB_MIN 38.2%。本轮 34/34 且**平均窗口反而短 0.581s**，因为成功轮收到命令后立刻收尾，5s/9s 是截止上限而非固定等待时长。按每分钟一次会合，窗口差值对应 `0.581 × 1440 / 3600 = 0.232 mAh/天` 每 1mA 窗口平均电流差的情景系数，不能当成实测节电；本轮 40 MHz 驻留约 0.798s/周期，旧 C 约 1.092s/周期，模式驻留比例下降。样本各仅 34 轮，未交错随机测试，无板级电流仪；无线环境、Windows 扫描/连接调度和两轮偶发长连接均影响结果，**不能由 30/34 → 34/34 断言新参数带来稳定性提升，也不能推出净耗电下降**。建议将 5s/9s 留作下一轮长时对照候选，不直接设成生产默认。
- 原始证据：ignored `artifacts/c59-final.json`、`c59-final-history.json`、`c59-final-status.json`、`c59-final-pmstats.txt`、`c59-final-log.txt`、`c59-bridge-full-window.log`。测试后的回退 OTA job `0465223c`：`0.18.25-note4-b` 上传 ACK、`confirmation=version_observed`；正式 sleep Plan 816 `applied/0s`。生产 Bridge EXE 从 SHA256 `492D618EAB0309F26643DEACADE6738729A0F1BEBEEE8D70FD7B693AA6752A56` 的备份恢复，工作区四份源文件按测试前哈希恢复；最终 Note4 ROM 重建及 Bridge 运行核验见 `PROGRESS.md`。

### B 方案正式参数实施计划（2026-09-28）

- 用户选定 B：Note4-B 的 BLE controller modem sleep 打开、main XTAL 低功耗时钟，deep timer 会合前先配置 `max=240/min=80 MHz, light_sleep=false`，控制器初始化后启用 BT sleep；未连接广播截止从 3s 改为 **4s**，已连接等命令保持 **6s**。成功 ACK 后仍按现有 200ms grace 提前关闭。普通 light 会话继续走原来的 `configurePowerManagement()`；1.54 目标不变。
- 修改仅限 Note4-B 目标配置与 `v2Rendezvous` 的生产路径，保留当前工作区未提交的 RF1 诊断代码；用独立版本 marker 区分候选 ROM。先构建唯一 Note4 目标，核验 SDKconfig、marker、大小和 SHA、`git diff --check`。B 的 3s/6s 已有 32/32 周期样本，但 4s/6s 组合及长时耗电未测；不得把原 B 的回答率或 PM 驻留直接标成新版本实测。
- 若进入设备发布，先核对登记 MAC/target/现用版本并保留 `0.18.25-note4-b` 回退 ROM；发布后用 `/history`、`/pmstats`、Bridge 会合日志确认真实 timer BLE 成功率和 80MHz 驻留，再决定保留或回退。工作区 RF1 仍是未合并诊断，不把含 RF1 的本地 ROM误记为干净生产镜像。

**范围更新（用户追加）**：1.54 B/W 设备也采用同一 B 参数：BT modem sleep、main XTAL、BLE 会合前 240/80 MHz DFS 且关闭自动 light sleep，未连接 4s、连接后 6s。两份 ROM 从不含 RF1 诊断的独立工作树顺序构建，分别核对 target、版本、sdkconfig、大小及 SHA256；1.54 的实机成功率和耗电尚无 B 方案样本，不能套用 Note4 的 32/32。此前“1.54 目标不变”和“仅限 Note4-B”的表述已被本次范围更新取代。

**发布与初验**：两份干净 ROM 均已顺序构建并按登记 MAC OTA，上传 ACK 与同 MAC 版本观察完成。Note4 新固件在正式 sleep 后记录多次完整 BLE 会合；1.54 在正式 sleep 后首次 timer BLE 会合成功，随后设备日志确认 `esp_pm_configure(240/80,no light)` 与 `esp_bt_sleep_enable()` 均为 `ESP_OK`，复核后恢复 sleep。具体 ROM 大小/SHA、job、Plan 与证据见 `PROGRESS.md` 顶节。未做 4s/6s 长时成功率或电池端电流积分；先前 B 的 32/32 是 Note4 3s/6s、15s 诊断周期，不作新版可靠率。
