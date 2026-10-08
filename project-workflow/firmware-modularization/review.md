# 固件 main 胶水层：边界与验收

日期：2026-10-08。基线为本轮开始时已包含精简协议候选的 6,421 行 `src/main.cpp`，不是运行设备的旧镜像。仅源码重构与未部署候选构建；不改变 Bridge、设备配置、协议合同或生产运行现场。

## 最终归属

| 文件 | 职责与状态 |
|---|---|
| `src/main.cpp`（21 行） | `FW_VERSION`、OTA marker、12,288 B loop 栈 hook、Arduino setup/loop 转交；无业务状态或硬件执行 |
| `src/firmware/application.cpp` | 物理唤醒分类、初始化顺序、单一 cooperative loop 顺序与分支；只通过模块限定名调用/访问集成状态 |
| `display.cpp` | 帧缓冲、面板基线、波形/刷新结果、时钟窗口、模板绘制、会合后的单次绘制 |
| `platform.cpp` | 已安装 active/CompiledTemplate/context、Data 应用、Plan 执行、模板切换、Bundle 事务；复用既有 `v2_*_command` 决策和存储 |
| `transport.cpp` | WebServer、认证、操作 token、48 B 编解码、请求/重放状态、BLE mailbox、分发、响应和 OTA |
| `radio.cpp` | BLE/Wi-Fi 启停、身份与发现、会合硬件适配、独立 HTTP ready report、配置恢复入口 |
| `power.cpp` | PM locks、BOOT 恢复/睡眠准入、GPIO hold、深睡、按键/USB/电池与 LED |
| `clock.cpp` | 时区、系统/外部 RTC、单调时间、SNTP 回调到 loop 的同步交接 |
| `diagnostics.cpp` | wake trace、状态/历史/PM/帧投影、HTTP/串口诊断控制；控制动作调用所属模块 |
| `sync.cpp` | 诊断环与 log sink、冻结批次、同步窗口、批次 ACK/complete |

每个模块内部头 `*_internal.h` 仅是固件集成边界，不是稳定对外 SDK：只导出按模块归属的 `State`/`Retained` 对象及实际跨模块调用的函数。167 个原先跨职责字段进入这些类型；80 个仅在本模块使用的变量声明保持 TU 私有。没有单一巨大 runtime context、逐字段 extern 列表、include `.cpp`/`.inc`、接口工厂、任务注册表或新增任务/线程。`dependencies.h` 只集中设备平台包含文件，不保存状态。

初始化、状态投影和诊断控制仍有明确的内部状态访问；本轮保留既有协作调度与线程边界，不能将按职责分文件描述成所有服务已可独立运行。HTTP/BLE 共用回复上下文仍归 transport 所有，不在本轮改为多请求并发。NimBLE 回调→有界 mailbox→loopTask 与 SNTP 回调→受锁待处理值→loopTask 的边界保留。

## 主路径与迁移核对

- 启动：Arduino `setup` → `application::initialize` → 原顺序的唤醒分类/时钟薄唤醒/恢复/认证/PM/正式 BLE 联系。
- 运行：Arduino `loop` → `application::tick` → 原顺序的时钟、OTA、串口/HTTP、BLE 分发、Wi-Fi poll、会合/普通模式与电源收尾。
- Data：transport 认证/分发 → platform 既有决策与应用 → display 即时或会合后绘制 → transport 原 ACK；应用与显示状态不合并。
- 状态构造多一条有意的 `buildPlatformStatus` 栈边界：第一版 Xtensa handler 1,040 B 超出 1,024 B 门限 16 B；分离后 handler 880 B、builder 224 B。字段构造保持原顺序，临时 snapshot 字符串在投影后更早释放，因此 `heap_free` 仍是采样时值，而非承诺与旧实现诊断数值逐字相同。

一次性对照 `artifacts/verify-firmware-moves.py` 检查 241 个迁移生产函数体：剔除归属限定、版本/镜像元数据传递、HTML 版本字符串等价构造和上述有界快照辅助函数后一致，0 项失败。它是源码迁移证据，不是 RF/RTOS 或全入口行为证明。完整映射为 `artifacts/firmware-modularization-map.json`（函数名→模块→迁移前行号）；既有文档中的 `main.cpp` 行号属于迁移前核查时点。

## 资源证据

| 成本 | 迁移前 | 最终候选 | 差值 |
|---|---:|---:|---:|
| 静态 RAM（PlatformIO data+bss） | 117,804 B | 117,860 B | +56 B |
| `.rtc.data` | 5,016 B | 5,080 B | +64 B |
| `.rtc.force_slow` / `.rtc.force_fast` | 32 / 4 B | 32 / 4 B | 0 |
| 应用 Flash | 1,833,063 B | 1,846,899 B | +13,836 B |
| BLE dispatcher 单帧 | 976 B | 960 B | -16 B |
| 最长已检查 Wi-Fi 本地链 | 3,440 B | 3,424 B | -16 B |

常规状态单实例归属、RTC 状态单实例保留，均不完整复制到任务栈。不增加仓库条目、凭据、回执或持久格式：空仓库/单条/典型/满容量情况下本次新增持久字节均为 0；结构上没有新增堆工作区。上述静态成本不随仓库使用量扩张，主要来自模块状态分组对齐及两个元数据指针；跨 TU 调用减少部分内联，Flash 增加约 13.5 KiB。实际 RF/RTOS 动态峰值未实机测量。

匹配 ELF 所有既有 Wi-Fi 单帧（≤1,024 B）/已列本地链（≤4,096 B）门禁通过；namespace 与 Xtensa `$isra$` clone 名称归一化，不提高阈值。局部链不包含间接调用、平台完整栈或 ISR。入口 `setup/loop` 各 32 B、`initialize` 224 B、`tick` 240 B；二进制栈 hook 实际返回 `3 << 12 = 12,288` B，沿用原 hook，旧门禁输出中硬编码 8,192 B 的说明已删。

各保留对象在 `.rtc.data`，本次检查未发现 firmware 静态初始化函数加载 RTC 地址；不将此检查替代真实 deep 保留/唤醒验收。RTC 内部地址布局变更仅属于候选程序内部，不把它当 NVS 存储格式，也不承诺跨不同镜像的 RTC 布局兼容。

最终 ROM/hash 见 `PROGRESS.md` 顶节。资源、入口反汇编、marker 和 hash：`artifacts/firmware-modularization-resources-20261008.json`。

## 验证与边界

- `python tools/test-firmware-modules.py`：入口胶水、按模块存储/RTC 所有权、日志边界通过。
- `python tools/test-ble-first-contact.py`：正式联系门、初始化及 loop 顺序、claim token/session/no-force 约束通过；已改为读取实际所属模块。
- `cargo test -p bridge-render --manifest-path bridge/Cargo.toml --target-dir bridge/target-note4-deploy`：9 个单元与 44 个集成共 53 项通过，含拒绝/冲突/重放与显示结果的现有共享逻辑。该宿主仍不编译完整设备 application/radio 循环。
- WSL `CXX=g++ python3 tools/test-external-clock.py`：迁移后的真实 clock 函数、实际外部 RTC driver，ASan/UBSan 冷恢复、相位、NTP 回调/I2C 隔离、竞态+30s延迟通过；`--pending-only` 负对照仍在目标断言失败。
- 唯一目标 `pwsh -File tools/pio-target.ps1 -Target note4` 构建成功；保留缓存，无 framework 重装、未构建其他 env、未上传。新增源码清单首次触发既有 checksum 下的应用/Arduino 库重编，未做 clean。
- 最终匹配 ELF 栈门禁、marker/大小/hash、源码对照及 diff/新增文件空白检查通过。

未执行设备读写、串口、OTA、Bridge 重启或提交；本轮不验收精简协议尚缺的同一 Fake 网络报告闭环，不宣称新模块调度的硬件 RF/睡眠/长期电池验收通过。相关既有待办仍归 backlog A9/B6/C4，不新增平行账本。
