# Fake ROM：共享固件核心与加速协议测试设计

日期：2026-09-22。状态：设计提案，尚未实现。本次只新增本文，不构建、不部署、不操作硬件。

## 1. 推荐结论与范围

推荐在 Cargo workspace 增加 `bridge/crates/device-sim`，产物为 Windows console `device-sim.exe`。Rust 负责进程、localhost socket、虚拟事件调度、测试控制和产物；设备业务执行真实 ROM 的 C++。从 `src/main.cpp` 小步抽出共享 `DeviceKernel`，继续调用现有 `v2_state.h`、`v2_runtime.cpp`、`bundle_store.cpp`、模板引擎与刷新策略。每进程模拟一台设备，多个 EXE 模拟多设备；首版不为现有 C++ 全局状态强行改成多实例。

模拟目标是所有影响协议可观察结果的设备行为：身份/认证/owner、会话、接收队列、分片、数据与计划幂等、Bundle/激活、持久恢复、显示结果、按键、时钟、电量保护、无线可达性、会合和退出。仅模拟 deep/light 名称和几条 ACK 不能达成目标。每种尚未覆盖的行为必须在能力清单中标为 unsupported；不得返回虚构成功。

默认启动为 **1x、独立进程、真实 HTTP、内存 RTC + 文件持久存储、无 GUI**。回归测试显式选择 step/max；Nx 用于人工观察。fake BLE 默认使用另一个 loopback socket，进程内集成测试可以使用同一消息合同的 channel。它不是 Windows 虚拟 peripheral，不应出现在蓝牙配对列表。

不采用：Rust 重写 ROM 状态机；复制一份 main.cpp 后各自演进；通过改短 300/600/60 秒常量“加速”；模拟整套 ESP-IDF/FreeRTOS/SPI 寄存器；为一种硬件或一个调用点创建抽象工厂；在模拟器内启动真实 Codex 采集或访问真实设备。软件模拟不能验证真实无线性能、flash 电气可靠性或墨水屏残影。

## 2. 当前代码证据和缺口

此处以阅读时工作树为准，源码符号比行号稳定；实施前记录 Git revision、工作树 diff 摘要、构建参数。仓库正在并行演进，不把未提交代码当已部署 ROM。

| 证据位置 | 已有能力 | 对模拟器的直接影响 |
|---|---|---|
| `PROGRESS.md` 最新章节 | 顶部记录 400×300 SSD2683 第二 target，0.16.8 仅编译未 OTA；下一现场节为 0.16.7/200×200 实机 | 默认模拟已验证 200×200；400×300 可做像素/协议测试，不能声称验证未知 LUT/引脚 |
| `docs/generic-display-platform-design-v2.md` | 平台目标合同；开头仍写早期“未实现/0.15.10” | 目标与当前行为分列；不照抄旧现场或候选会合时长 |
| `src/v2_state.h` | 无 Arduino 依赖的 Plan/DataSeq/Checkpoint/BundleRx/CRC；显式 `nowMs` | 直接复用；不需要新协议状态机库 |
| `src/v2_runtime.cpp` | 完整字段/index/order/context/CRC/seq 校验、usage 重建；6144B usage 上限 | 原函数编入宿主；Rust 不重新解释 fields 或合成 applied ACK |
| `src/bundle_store.cpp` | `/bundle/a.bin,b.bin,m0.bin,m1.bin`；槽读回 CRC、双 metadata、活动项提交、源保留及 ABI 恢复 | 必须运行相同写入/恢复代码；模拟存储仅提供字节操作、容量和故障 |
| `src/main.cpp` 的 `v2Command`、`applyV2Data/Plan`、`handleV2Bundle*`、`handleV2Activate` | endpoint token、owner、MAC/nonce/request、幂等缓存、分片暂存、显示 ACK 都在主文件 | 这是主要共享提取范围，单独共享 v2_state 仍不够 |
| `serviceV2Ble`、`v2Rendezvous`、`v2RendezvousRender` | BLE queue 长度 2、单消息 ≤8192B；bond/encryption/peer 二次核对；会合后才渲染 | fake BLE 也经过队列与相同入口，不能直接调用 DataSeq 并跳过授权和时序 |
| `setup`、`loop`、`enterDeep`、`sleepToNextEvent`、`deepSleepRaw` | ESP 单调时间、RTC、owner、usage cache、分钟薄唤醒、网络会合、睡眠 glyph、无线关闭交织 | 要提取编排和保留规则，不能把 sleep 仅当一段 await |
| `refresh_policy.cpp` 与 main 的绘制/时钟路径 | 同帧可零刷新，clean/force/trust 在零差分之前；成功才记预算；时钟有独立窗口 | 复用候选帧、策略和成功提交顺序；模拟显示完成与失败 |
| `bridge/crates/render/build.rs`、`shim/`、`src/ffi.cpp` | cc 已编译真实模板、v2_runtime、bundle_store、refresh_policy、GUI_Paint；Arduino String shim 和内存 LittleFS | 可复用编译基础；当前 shim 有全局 files/capacity/writeBudget，非独立进程设备运行时 |
| `render/tests/{compiled,policy,v2_state,bundle_store}.rs` | 像素/区域一致、计划序号、字段 CRC、RTC、分片守卫、撕裂写已有测试 | 保留为快速单元测试，不用端到端替代所有测试 |
| `core/tests/v2_client.rs` | TCP 假端点验证客户端顺序/401/409，手写常量状态和简化 ACK | 属客户端契约夹具，未执行 ROM 鉴权、存储、显示或电源，不能作为 Fake ROM |
| `core/src/v2_client.rs` | 已支持 `127.0.0.1:port`；真实 `/v2/*`，先 status 取 nonce；默认 409 包装为 occupied | HTTP 客户端本身无需另造模拟协议；保留原状态码，防止业务拒绝误分类 |
| `core/src/coordinator.rs` | 多数入口显式 now；仍有 enqueue/终态/plan ACK/summary 直接 `crate::now_secs()` | 加速必须消除这些隐藏时钟依赖 |
| `ble/src/lib.rs::V2Connection`；`app/src/platform.rs::ble_cycle` | connect/status/data/plan/close；真实扫描、重试、分片、长属性读取，app 串行机会 | 提取小的 transport seam，复用命令构造和业务调度 |

已查到的具体时序是：`V2_RENDEZVOUS_WINDOW_MS=3000`，连接成功后截止改为 `connectedAt+6000`，计划接受后 200ms grace；不是整个窗口永远 3 秒。BundleRx 总截止为 BEGIN 后 120000ms、总长度 ≤262144、CHUNK ≤16384；Bridge 默认块为 4096。BLE 只支持 status/data/plan，其他 op 返回 `http_required`。v2 路由实际注册 `/v2/bundle/begin|chunk|commit`，没有单体 `POST /v2/bundle`。

必须保留的“现状与目标差距”测试：当前 BEGIN/COMMIT 未见绑定 `expected_active_context_id` 的校验；部分错误仅回拒绝而不取消 rx；Bundle 未统一预检剩余 light 收尾预算；`v2BootMs` 在 setup 若干初始化后取值而非最早物理唤醒入口；`enterDeep` 仍有 `/deep` 通知及同步绘制，可能越过名义 deadline。当前 checkpoint 只保存 seq/CRC/context，不等于 fields 全量 RTC 保留；owner 重启会对保存的 uptime 做钳制，不能擅自改成跨 deep 的绝对 lease。`ownerAllows` 在无 owner 时返回 true，有匹配 owner 时调用 ownerTouch；v2OwnerOk 当前直接复用它，所以“必须先 claim 才可写”的目标尚不能作为当前实现事实。owner 到期判断还是 `elapsed > lease`，不是其他截止普遍采用的 `>=`。首阶段以 characterization 固定现状，并以单独的期望失败用例表达目标差距。修复时改共享 ROM 路径，再同时更新真实固件与模拟断言；不能在 simulator 中偷偷修好。

## 3. 进程、端口与依赖

建议命令合同（待实现）：

```text
device-sim.exe --listen 127.0.0.1:0 --ble-listen 127.0.0.1:0
  --data-dir <absolute-dir> --clock 1x --seed 42
```

启动 stdout 只输出一条 ready JSON，含 HTTP 与 fake BLE 两个实际端口、虚构设备 MAC、target、clock mode 和产物目录；诊断去 stderr/JSONL。未传 data-dir 时使用 `<exe>/data/device-sim/<instance>`；CI 使用独立临时目录，不能读取生产 bridge data。seed 只决定测试熵，设备 token/endpoint token/control token 分开配置，日志永不输出。只监听 loopback；控制面使用独立随机 bearer token，由测试父进程的受限配置交付，不打印到日志。

一个 HTTP listener 同时承载 ROM 路由和 `/sim/*` 控制面；命名空间是必要的边界，不再为控制面增加第二个 HTTP 端口。deep 时 `/sim/*` 始终可用，但 `/v2/*`、`/claim` 等设备路由不得正常应答：适配器应按场景关闭请求或制造受控黑洞，不用自造 HTTP 503 假装真实无线不可达。同端口模型无法复现 TCP connect 阶段的 connection-refused；这一层仍属真实网络/实机测试，不应为它增加常驻管理端口。fake BLE socket 仅传测试链路事件，不暴露为 ROM HTTP 路由。

```text
Bridge application service / coordinator
      ├── 原 v2_client ── localhost HTTP ──┐
      └── V2Connection seam ── fake BLE ──┤
                                          v
                         Rust I/O adapter → 单一事件队列
                                              v
                          C ABI → C++ DeviceKernel
                                   ├── v2_runtime / v2_state
                                   ├── bundle_store / owner_store
                                   └── template_engine / refresh_policy / GUI_Paint
                                              v
                              ports → ESP32 或宿主实现
```

共享核心不依赖 Rust、Tauri、Tokio、WebServer、NimBLE、ESP-IDF。允许继续使用已能宿主编译的 Arduino String/ArduinoJson，不在本项目中顺便改写所有字符串/JSON。不要 `#include main.cpp` 后提供数百个假的 SDK 函数。`bridge-render` 保留预览公共 API；首阶段可由它暴露内部 kernel FFI，device-sim 依赖现有编译产物。只有确实发生重复链接或构建配置分歧，才把 C++ 编译部分搬到一个共享 build helper/低层 sys crate，不先建多层 crate。

## 4. DeviceKernel 的边界与状态

先抽函数再收拢状态；预计 `src/device_kernel.{h,cpp}` 管生命周期和命令，`src/device_ports.h` 定义必要宿主接点。不引入通用 event bus、反射命令注册表或面向每个函数的 interface。已有 bundle_store 全局状态首版继续使用，每进程一个 kernel；所有 FFI 调用必须由同一个线程串行执行。宿主 render 现有全局 painting 状态也不能并行重入。

核心持有：当前 Profile/CtTemplate/context、DataSeq/fields/usage、PlanState/provisional/safety deadline、session nonce、BundleRx 和暂存路径、activate/bundle 去重结果、owner 状态、mode/wake/recovery 标志、会合截止/连接时刻/ACK grace、无线/PM/OTA 锁逻辑、候选帧/最后成功帧/基线可信度、区域预算和时钟窗口。RTC/NVS 格式与持久记录仍归原存储模块；不要把整个 C++ 对象按内存 memcpy 保存。

核心入口为少量有界事件：Boot(reset/wake cause)、Request(HTTP/BLE 元数据与原始字节)、Button(level/change)、RadioConnected/Disconnected、TimerDue、DisplayDone、StorageFailure/PowerCut。事件附逻辑时间及递增序号。输出效果只有 Reply、RadioAction、DisplaySubmit、Sleep/PowerOff、诊断；存储可先同步调用端口，直到有真实需要才拆异步存储完成事件。HTTP 解析器只解包 method/path/query/header/body，业务 JSON、错误顺序、鉴权、ACK 字段由共享 C++ 处理。

同一时刻，先处理已到期的安全截止，再按入队序号处理外部事件；有效窗口为 `[start, deadline)`。必须在 ROM 与 simulator 共用该顺序；迁移之前现有 loop 的实际顺序用边界用例保留并报告差异。处理请求时冻结输入字节，不能异步引用 socket buffer。一次 tick 跑到静止点/产生待完成效果，不忙循环；返还最近内部截止给 scheduler。

显示和数据提交分开：BLE Data 在会合中应用并回 pending；关闭 radio 后执行一次合并后的数据+时钟绘制。模拟器也通过 `DisplayDone(success/failure)` 更新状态、预算、旧帧。重复 seq 不得再次触发波形；显示失败不回滚已应用数据，后续恢复必须走 ROM 已有/明确新增的恢复路径。HTTP 当前同步显示语义在提取阶段保持；要改成异步 ACK 必须作为协议行为变更单独评审。

## 5. 最少 ports

| 端口 | 共享核心决定 | ESP32 适配器 | 宿主适配器 |
|---|---|---|---|
| Clock + Scheduler | deadline、剩余时间、最近下一事件、分钟/会合选择 | esp_timer/millis、RTC 时间锚点、timer wake | 64 位虚拟总时间、每 boot uptime、确定性事件堆 |
| Transport/Radio | 哪种状态可接收、窗口关闭、队列限额、认证和 ACK | WebServer/NimBLE/FreeRTOS 入队、Wi-Fi 建连和断开 | 真实 HTTP listener、fake BLE socket、受控建连完成 |
| Storage | 原 A/B/owner/usage 写入顺序、校验、恢复选择 | LittleFS/Preferences/RTC | 同名逻辑文件与 KV、字节级故障、RTC image |
| Display | render/diff/window/全局刷选择、预算、基线可信度 | SSD1681/SSD2683 的寄存器、BUSY、plane、供电 | 保存 submit 描述与帧；在指定时刻返回 BUSY 成败 |
| Platform | 按键语义、低电/USB策略、reset 后状态、随机 context/nonce 的使用 | GPIO/ADC/USB/reset/熵/锁操作 | 注入按键、电量、USB、reset cause；带 seed 的熵源 |

Clock/Scheduler 合并是因为核心仅需 `now + next_deadline`，无须再建定时器服务；日志作为 effects，不另建日志服务。Storage 首阶段通过扩展现有 LittleFS shim 和小型 Preferences 接点承载；在同一源码能跑之前不为了整洁替换 `bs*` API。只有硬件调用留在适配器，owner/session/commit 决定不允许留在 Rust。

Display 同时记录 `candidate` 与 `displayed`；后一份只在成功完成后更新。失败时实际面板可能是未知中间图像，sim 只能记录 uncertainty，不能声称旧帧仍物理准确。驱动窗口对齐合同可以共享纯函数；SPI 指令顺序和真实 waveform 留在驱动测试/实机。

## 6. 虚拟时间的严格合同

三个时域必须命名区分：`sim_time_us`（跨 reboot 的单调实验时间）、`boot_uptime_us`（本次设备启动起算，匹配 ESP 定时器/millis）、`wall_epoch_s`（用于屏幕时间/源观测，可校时跳变）。OS socket 的真实 elapsed time 单独计量。校时不移动 light/会合/事务截止；deep timer 由 sim_time 调度，但唤醒后 uptime 从零，RTC 仅按真实保留规则恢复。保留 32 位 millis 回绕视图，测试约 49.7 天边界，不能用宿主 64 位无回绕掩盖 ROM 算术问题。

| 模式 | 精确定义 |
|---|---|
| `1x` | 未暂停时虚拟单调时间随 OS monotonic 以 1:1 推进；事件按 deadline 运行。状态读取不额外推进时间 |
| `Nx` | `virtual_delta = N × wall_delta`，N>0；常量和消息中的秒数不变。调度仍逐个处理跨过的 deadline，不一次跳过中间事件 |
| `step` | 虚拟时间冻结；外部已到达请求在当前时刻执行至静止点；仅 `/sim/advance` 推进指定时长或到下一个事件。一次请求不暗中推进几分钟 |
| `max` | 在参与者全部就绪时跳至最近事件，执行该时刻全部事件到静止点，反复运行到明确的 `until_us`/事件上限/断言完成/等待外部输入；没有无界自动快进 |

`advance(delta_us)` 处理 `(now, target]` 全部中间事件，返回实际停点和 `stopped_reason`。`next_event=true` 与 delta 二选一；有零时刻循环时达到 max_events 即失败，输出最后事件轨迹，不能卡死或悄悄丢事件。切换倍率先结算旧模式，再设新锚点，时间不倒退。事件按 `(due_us, priority, sequence)` 排序，seed 和输入时序固定则结果可重放。

**max/step 与真实 socket 的协调不能靠 sleep 猜测。** 场景 runner 是实验时间唯一驱动者，设备与 Bridge 测试驱动报告 `idle(next_deadline)` 或 `in_flight(operation_id)`。只有所有参与者处理完当前时间、没有未登记的请求/响应，才允许跳到下一 deadline。每个真实 I/O 开始前登记屏障，响应已被 Bridge 消费后释放；在途时 max 暂停自动跃迁，以免设备在 Windows 尚未调度客户端前睡掉。模拟链路延迟由登记好的完成事件推进；注入丢 ACK 时由场景安排逻辑超时完成。屏障等待仍有有限 wall-clock watchdog，触发为 harness/I/O failure，不能伪装协议 timeout。

使用未改造的生产 Bridge EXE 时默认只开放 1x/Nx；Nx 是压力观察、不是确定性证明，必须记录 wall I/O 是否赶不上虚拟窗口。无协作 Bridge 时 max 只用于独立 ROM 场景，不能宣称完成 Bridge+ROM 加速集成。不要在开启 max 的情况下用“等待 50ms 没网络包”判断全系统空闲。

Bridge 必须注入的时间包括：coordinator 的 now（含 `note_plan_ack`、job 时间戳、summary 的隐藏 now）；`core/src/platform/service.rs` 的 ACK/full_sync 计时（实施时核对实际路径）；源轮询/观测/过期时间；`app/src/platform.rs` status/plan/cycle 时间；`app/src/main.rs` 的 owner 60s 续约、扫描/机会/重试/缓存节流；`ble/src/lib.rs` 的 stamp_clock、request ID 生成、逻辑 ACK/连接预算。request ID 改用注入计数/熵，不依赖真实 SystemTime 纳秒才能唯一。真实 btleplug connect/read 和 TcpStream read/write timeout 继续有 wall-clock 上限；fake BLE 的协议等待使用逻辑 clock。首阶段让测试 runner 直接调用 coordinator/application service 的一轮工作；最终把生产循环的“等下一轮”接到同一 clock，避免复制一套 Bridge 行为。

## 7. HTTP 与 fake BLE 接入

设备面保持真实方法、状态码和路径：`GET /v2/status`、`POST /v2/data|plan|activate`、`POST /v2/bundle/begin|chunk|commit`，以及配套 `/claim`、`/status.json`、必要恢复/诊断路由。鉴权分别复用 endpoint token 与设备操作 token 的真实规则；同 owner 续约不改电源 deadline。状态/恢复读取不建立 owner、不发布模板。unknown path 返回与固件一致的未实现响应。

接收 body 有硬上限、有限 header/body wall timeout，慢 socket 不占 kernel 线程。完整命令入队后串行执行；停无线时中止待提交请求，已提交但未返回 ACK 的结果仍保留。测试覆盖“提交成功 → response 丢失”，而不是只在提交前丢包。路径 query 必须原样传给共享 handler；BEGIN/COMMIT 的 bridge_id 在 body，CHUNK 使用 request_id/session_nonce/offset query，不能为对齐最初文档改线协议。

fake BLE seam 位于 `V2Connection` 对应用服务提供的 connect/command/close 边界，采用小 enum 或必要的一个 trait，真实 Peripheral 与 SimSocket 两实现。命令加 protocol/rv/request_id/nonce/token/MAC/server_time、分片、ACK request 匹配的代码应共享。接收侧复用 `ble_bridge.cpp` 的可提取有界 JSON 重组及 main 的认证队列；第一阶段若只传完整 JSON，能力明确标注“不覆盖分片”，不能把该阶段当 BLE 验收完成。

socket 合同建议 length-prefixed JSON + 有界 byte payload，消息仅含 `scan/connect/disconnect/write_fragment/read_status` 和 `advertising/connected/value/error`，携带实验 peer/bond/encrypted/MTU 元数据。这些元数据只能由受保护模拟适配器设置，不能从业务 body 自证已绑定。状态读取模拟长属性完整读取与旧 ACK；notify 可丢、截断，不得把 Write Response 当应用成功。窗口截止、连接后窗口、两条队列溢出和断连前入队/断连后处理的 peer 检查都照共享核心执行。

首次配置用测试专用 provision fixture 注入虚构 MAC、独立 token、endpoint/bond 记录，再按真实 `/claim` 建 owner；提供 empty/unconfigured fixture 测试无 Bundle 的 legacy 分支。default fixture 不复用现场 MAC/凭据，且 simulator target 标志不得被真实 OTA 选择器当硬件设备。UDP/ARP 发现首期不模拟，Bridge 显式指定 loopback endpoint；身份不一致拒绝仍要测试。

## 8. 控制面合同

所有变更返回 `event_id, sim_time_us, boot_id`，命令入同一队列。可选 `at_us` 必须 ≥当前时间，用于准确复现边界；重复控制请求可使用 control request ID 去重。控制 token 和业务 token 不可互换。控制面只能影响虚构实例。

| 接口 | 输入/输出与限制 |
|---|---|
| `POST /sim/wake` | `reason=timer|button`；只在 deep 接受唤醒，awake 返回明确 no-op；button 对应物理唤醒语义，timer 不授予 BOOT 300s |
| `POST /sim/advance` | step/max 的 `delta_us` 或 `next_event`，可附 max_events/until；输出处理数、实际时刻、等待原因；1x/Nx 先显式切 step，不隐式改模式 |
| `POST /sim/clock` | 切 1x/Nx/max/step、倍率及运行上界；可单独注入 epoch/tz 校时，不能重置单调时间 |
| `POST /sim/reboot` | `kind=software|watchdog|power_cut`；另用 `/sim/wake` 唤醒 deep；返回 reset cause/存储保留摘要。power_cut 后为 powered_off，显式 power-on（同接口 kind=cold_boot） |
| `POST /sim/input` | battery_pct/voltage、USB、按键 level/持续时间、link 状态；按键走 ROM debounce/hold 语义，不直接篡改 active |
| `POST /sim/fault` | 注册/移除有限次故障：匹配 op/phase/path/write ordinal，动作 drop/delay/disconnect/short_write/corrupt/read_fail/busy_timeout/power_cut；回 fault_id；不接受任意文件路径或脚本 |
| `GET /sim/state` | 只读完整诊断：mode、deadline、RTC有效性、owner摘要、context/job/seq/plan、rx offset、锁、frame CRC、预算、队列、已注册故障；不推进时间、不输出 token/原始业务秘密 |
| `GET /sim/events?after=N&limit=M` | 有界 JSON 事件环，含 next_cursor/dropped_before；分页上限，不能无界下载 |
| `GET /sim/frame.pbm?kind=displayed|candidate` | P4 PBM，原 target 尺寸；默认最后成功帧。无成功帧回 404；header 附 frame_id/逻辑时间/trusted，PBM 黑白位序按格式转换 |

同一控制请求不能夹带真实协议写入，也不能直接赋值 `seq/plan_id/committed` 造成功状态；需要这些状态时由合法业务消息建立。只有明确的 corruption fault 可以破坏已存在字节。

## 9. 持久化、重启和断电

实例目录保存 flash 逻辑文件、NVS KV、启动配置、manifest 和有界 trace；RTC 默认在进程内单独区域。可导出 RTC 证据，但普通重新启动 EXE 视为冷启动，不能默认恢复 RTC image。测试需要跨进程恢复 RTC 时用显式 resume 参数并在报告标记。

| 事件 | RAM/session/plan | RTC | flash/NVS | 显示 |
|---|---|---|---|---|
| 正常 deep → timer/button | 重建易失运行态，重新握手 nonce；不复活旧 light 承诺 | 按真实 checkpoint/时钟/窗口范围保留 | 保留，运行原 bsBegin/ownerBegin | 可见图保留，基线是否可信按 ROM 路径 |
| software/watchdog reset | 清易失态和 RX/cache；旧 session 失效 | 只按已验证 reset cause 合同使用，默认不得宣称数据有效 | 保留已完成字节 | 标记基线不可信，走真实恢复 |
| power_cut → cold_boot | 丢弃 | 丢弃 | 保留切点之前实际落下的字节，可撕裂 | 可见帧仅为最后成功观测；物理状态未知 |

宿主存储保留真实 A/B 格式与 metadata CRC，不能以保存一个“已提交 Bundle.json”替代。现有 LittleFS writeBudget 只让写函数返回短写，随后程序还会清理；这是 **I/O 错误测试，不等于突然断电**。新增断电切点必须立即停止 kernel 执行、不执行失败清理和析构所触发的提交，然后销毁易失态，带原始存储 image 重新 boot。可在端口返回专门的不可恢复 abort 标志并逐层退出；若采用异常中断，只在宿主适配层启用且不得改变生产 error path。关键验收覆盖 header、每模板 blob、源 payload、读回、metadata 写入及读回、运行态切换、ACK 发送各边界。

不要假设宿主文件 close 与 ESP flash 掉电等价。测试模型明确“哪些字节已 durable”，运行 trace 中记录 flush/切点；Windows crash durability、LittleFS 真正 metadata 恢复仍需实机。容量、文件创建失败、读坏数据、NVS 单项写失败、RTC checksum/context 不匹配均可注入。故障只允许在实例逻辑路径集合内，不操作任意宿主文件。

## 10. 产物与可诊断性

stdout ready + stderr 摘要；`events.jsonl` 记录 schema_version、run/boot/event ID、sim/uptime/wall 时间、输入类型、状态前后摘要、deadline、request/job/context/seq/plan ID、业务结果、refresh reason/window/预算、frame CRC、radio/锁和故障命中。认证令牌、Wi-Fi 密码、数据源凭据和完整敏感字段必须脱敏。manifest 记录源码标识、target、ABI、seed、模式、输入场景哈希及模拟覆盖范围。

每次成功 DisplayDone 可按选项导出编号 PBM；失败保留 candidate 与最后成功图和 uncertainty 标志。PBM 原始像素与 `bridge-render` 同输入对拍；sim frame 不加桌面 UI 边框，不依赖 GUI 才能检查。失败包包括控制输入、事件尾部、可脱敏存储 image、两帧及首次断言差异。trace 默认有大小上限，CI 失败再保留详细证据。

## 11. 测试矩阵

| 类别 | 必测场景 | 核心断言 |
|---|---|---|
| 认证/owner | 错 endpoint token、错设备 token、空闲 claim、他人 owner、续约/到期、MAC/nonce/peer 错误 | 原 401/409/业务拒绝；无隐式 claim；读取和续约不移动 light |
| Data | 正常、同 seq 同内容、同 seq 异内容、旧 seq、跳号、字段缺/多/乱序/CRC/本地字段、旧 context | 拒绝不改确认基线；重放零重复波形；完整快照原子替换 |
| 模板/激活 | 1/8 项循环、A→B→A、请求重放、接收中按键、target/ABI/未知 type/font/bind | context 不复用；不得裁为3；非法包不半安装；现有差距单列 |
| Bundle | 分片大小边界、重复相同/冲突/跳洞、过期、owner切换、低空间、编译失败 | offset/总限/CRC正确；只能完整旧包或新包；成功存储后才 flash ACK |
| 断电 | 每写操作前后及关键字节撕裂、提交后 ACK前断连、激活 metadata 损坏 | 使用真实 bsBegin 恢复；无新旧拼接；unknown 可查询收敛 |
| Power | BOOT 299999/300000ms、timer wake、重复/旧/冲突计划、限额钳制、无计划600s、校时前后 | deadline 不重置；当前缺陷以显式失败暴露；不能把进入收尾等同无线已关闭 |
| Rendezvous | 无 Bridge、迟连接、连接后6s、ACK后200ms、连续漏窗口、radio off后显示 | 每个窗口有界；薄时钟唤醒不意外联网；BLE Data pending 后可观察 displayed/failed |
| fake BLE | 180B分片边界、UTF-8字节、MTU、8192B上限、队列满、断连换peer、旧/截断ACK | 绑定/加密二次校验；Write Response不确认数据；超时不续期 |
| 显示 | 同像素、89↔90/99↔00、清影同帧、时钟与数据合并、BUSY失败、跨deep/冷启动 | 与真实引擎逐像素一致；成功才更新旧帧/预算；窗口外不变；照片质量不由此证明 |
| Bridge | push/pull真值表、ACK丢失且源继续变、full_sync到期、桥重启、保存/claim无发布 | ACK只确认在途指纹；新快照后续投递；deadline仅成功ACK更新；任务冻结 |
| 环境/退出 | Wi-Fi失败/恢复、低电、USB、按键长按、OTA锁/超时、配网退出 | 有界关闭无线和锁；未模拟的硬件动作明确unsupported，绝不虚报升级成功 |
| 时间/隔离 | 1x/Nx/step/max同一脚本、时钟跳变、millis回绕、两实例不同owner/target | 归一化事件/帧相同；无deadline被跳过；状态/存储不串实例 |

测试分三层：原快速 C++/Rust 单元测试；step/max kernel 场景；真实 localhost HTTP + 协作 Bridge 集成。保留现有 v2_client 极小 server 作为 HTTP 编码单元夹具，同时新增 against-simulator 测试，避免客户端和模拟器共享同一错误假设。

## 12. 分阶段迁移与退出门槛

每阶段开始前在 `project-workflow/fake-rom-simulator/` 落 plan/task/status/review，完成后由主任务更新 PROGRESS。本设计不创建这些文件或修改现场记录。

| 阶段 | 预计涉及真实文件/新增文件 | 完成条件 |
|---|---|---|
| S0：锁定证据 | `render/tests/*`、`core/tests/v2_client.rs`、新增协议 fixtures/coverage 清单 | 冻结当前路由、ACK、认证错误顺序、时序和目标差距；不改行为 |
| S1：同源命令入口 | `src/main.cpp` 的 v2 handlers；新增 `src/device_kernel.{h,cpp}`、`device_ports.h`；`render/build.rs/src/ffi.cpp` | HTTP/BLE 仍走原适配器，转调同一 C++ handler；宿主可执行 status/data/plan/bundle/activate；原测试不退化 |
| S2：EXE/存储/HTTP | `bridge/Cargo.toml`；新增 `crates/device-sim/{Cargo.toml,src/*}`；`render/shim/LittleFS.h`；`owner_store.*` 的时间/KV接点 | 1x真实listener、/sim控制、独立flash/RTC、合法claim、冷/deep重启、PBM；端到端数据由ROM产生 |
| S3：电源/显示编排 | main 的 setup/loop/rendezvous/enterDeep/sleepToNextEvent、时钟窗口/显示成功路径；新增/完善 kernel ports | 等待循环改 deadline事件；固件适配器仍用实际硬件；step完整运行300s/会合/显示失败；保留legacy入口 |
| S4：Bridge与BLE时钟 | `ble/src/lib.rs`、`src/ble_bridge.cpp` 重组接点；`core/src/coordinator.rs`、`core/src/platform/service.rs`、`app/src/platform.rs/main.rs` | command构造共享，fake BLE不触碰OS蓝牙；隐藏now消除；协作屏障与max可重放 |
| S5：故障矩阵/恢复 | device-sim tests、`render/tests/bundle_store.rs`、共享 storage/owner/usage恢复接点 | 真正硬断电与短写分开；每个持久边界及ACK丢失回归；边界缺陷在共享ROM单独修复 |
| S6：完整覆盖与实机比对 | 覆盖清单、fixture/trace、需补共享的claim/OTA锁/按键/legacy路径 | 所有协议可观察路径可测试或明确保留实机；真实ROM与同源sim成对验证关键场景 |

不以主文件总行数作为进度指标。每次只迁移一个完整纵向路径，例如先 Data 的验证→更新→显示→ACK，再迁 Plan；旧路径转调新函数后删重复实现。不能一次移走约5200行再用未验证 shim 填满编译。S1/S2 可交付有用的部分模拟器，但只有 S6 覆盖清单达标才称“完整 Fake ROM”。

## 13. 验收与实机保留项

验收必须同时满足：

1. Windows EXE 无 GUI 可启动/退出、HTTP 与 fake BLE 两端口无冲突、进程结束可恢复持久包，失败退出码非零；所有实例资源独立，测试不访问现场地址、蓝牙适配器或真实 Codex。
2. ROM 与 sim 编译同一份业务源；Rust 中不存在 owner/seq/plan/Bundle 提交/刷新选择的第二套实现；构建记录列出共享源清单。
3. 原 render、coordinator、v2_client 测试继续通过；真实 v2_client 对 localhost 执行完整发布、激活、Data/PowerPlan/恢复；8项与legacy≤3行为分别明确。
4. step 能精确断言每个边界前/当点/后；max 在有界场景内跨至少24小时虚拟时间及1000次wake，无真实逐周期等待；报告实际运行时间，不预设硬件无关的固定加速比。相同seed/输入事件得到相同归一化trace/帧CRC。
5. 展示无延长的300s/600s截止、60s会合、120s传输超时及ACK屏障；所有退出路径释放逻辑无线/PM/OTA锁；不得把真实socket timeout除以倍率。
6. A/B每个关键切点断电恢复只选完整包，提交后丢ACK可收敛；RTC失效/冷启动不复用旧数据上下文；BUSY失败不推进成功帧基线。
7. 目标合同尚未满足的场景以显式失败/已知缺口记录，不能全局skip后宣称完成；阶段报告区分模拟覆盖、同源单元覆盖和实机证据。

保留实机 smoke：Windows scan命中/首连重试/服务发现、配对与加密、GATT缓存/MTU/长属性/通知、连接时长与无线共存；真实AP重连/DHCP/UDP/ARP；RTC真实保留和唤醒延迟；USB/按键去抖/ADC/低电断电；FreeRTOS并发、堆峰值/8KB栈/看门狗；LittleFS/NVS物理断电和分区空间；OTA token/target检查与双槽启动/回滚；SSD1681/SSD2683的BUSY、plane、对齐、温度/LUT、固定曝光残影照片及24h电量。sim能证明协议和软件决定，不能把虚拟radio时间换算为已验证功耗。

主要风险是抽出 main 编排时改变执行顺序、用过于友好的宿主存储掩盖硬断电、只加速设备却遗漏 Bridge 时钟、全局C++状态并发重入，以及 max 越过真实I/O。以上分别由小步characterization、字节切点、全链时钟清单、单进程单设备单线程和参与者屏障控制；无需引入整机仿真框架。
