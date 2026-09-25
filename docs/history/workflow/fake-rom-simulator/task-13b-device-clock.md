# Task 13b — 设备模拟器逻辑时钟

状态：宿主验证完成，已提交 `7aa6d38`。基线 `a8578a6`。

## 决策

先为模拟设备建立可控时间入口，再让 owner、PowerPlan、Bundle 截止使用它。Bridge 每目标时钟属于 Stage E，本切片不修改 Bridge。设备时钟与未来 Bridge 时钟可分别配置，允许同步或失步；本任务不得假设二者必然共用时钟。

- 一个进程拥有一个 `SimClock`，以宿主 `Instant` 为锚。保存已结算的非负逻辑毫秒、当前倍率 `rate_ppm`（1x = 1,000,000；0 = 暂停；上限 1,000,000,000 即 1000x）、锚定的宿主瞬时、启动 wall epoch 和可变 wall 偏移。虚拟增量为宿主 monotonic 增量乘 `rate_ppm / 1,000,000`，使用足够宽的整数与饱和边界；改变倍率先按旧倍率结算，逻辑时间永不倒退。
- 本进程冷启动的逻辑 monotonic 与 uptime 均从零开始，默认 wall epoch 取启动时宿主时间；可选 `--epoch-ms <非负整数>` 固定基线。wall = 基线 + 逻辑增量 + 偏移，可前后跳；校时或改倍率不改已经结算的逻辑截止。跨进程持久 monotonic、RTC 和 PowerPlan 恢复留待持久层任务，当前诊断中标明 `clock_persistence: unsupported`。
- `GET /sim/time` 和 `POST /sim/time` 只接受 control bearer，设备业务 token 不可调用。GET 返回 `monotonic_ms`、`uptime_ms`、`wall_ms`、`rate_ppm`、`wall_offset_ms`。POST 严格解析以下三种 JSON（未知 op/字段或越界值返回 400，不改变时钟）：`{"op":"rate","rate_ppm":N}`；`{"op":"step","delta_ms":N}`；`{"op":"wall","offset_ms":N}`。step 只在暂停时允许、delta 非负且单次最多 86,400,000 ms；rate 允许 0..1,000,000,000；wall offset 为有符号整数，wall 结果必须可表达为非负整数。成功返回新快照。
- 所有时钟操作由同一进程锁串行。`/v2/status` 与 `/sim/state` 从该时钟一次取样，不能再直接读 `Instant::elapsed()`。未来业务命令进入共享状态锁后也使用同一次 `nowMs`；这一切片不引入尚未实现的业务行为。`/sim/state` 把 `clock-control` 从 unsupported 移除，并给出时钟摘要；ready capability 加 `clock_control`。不提供 `max`：它需要完整事件队列与在途 I/O 屏障，单纯跳到最大值会错过截止。
- 仍仅监听 loopback，不提供 `/status.json`，不接生产 Bridge/设备，不写数据目录，不修改固件/共享 C++。严格保持 Stage C 状态 builder 路径。

## 验收

- `cargo test -p device-sim` 以真实 loopback 进程验证：默认 1x 单调、暂停后 host sleep 不推进、step 精确推进、变倍率连续且不回退、wall 正负偏移不改变 monotonic、非法命令原子拒绝、控制 token 隔离、两个进程不同倍率互不影响。测试不依赖精确宿主调度毫秒，只对暂停/step/偏移作精确断言。
- 现有 bootstrap 测试与 `cargo test -p bridge-render` 不退化，`git diff --check` 通过。Luna 只改 `bridge/crates/device-sim/` 及其直接 Cargo 依赖，不能改 Bridge、`src/`、其他 worktree、生产服务或设备；不提交。主代理复核后更新进度并提交。

## 验证记录

`device-sim` 9 项集成测试通过；`bridge-render` 全套通过（含 19 项 `v2_state`）；`git diff --check` 通过。`/sim/state` 使用嵌套 `clock` 快照，非法时钟命令统一返回 `{"error":"invalid_clock_command"}`。这两处原设计未限定 JSON 形状，主代理审查后采用此实现。
