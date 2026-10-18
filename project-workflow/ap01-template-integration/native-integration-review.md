# 原生接线审计

**0041最新里程碑**见[native-0041-progress.md](native-0041-progress.md)：调用方持有渲染/画布状态，11个RV32对象只读段/初始化段检查、四组原生端口宿主像素一致和33项回归通过。取得0041页面/事件小函数依据；原生HTTP/VFS/Bundle/UI与完整ROM仍未完成。下文是0031历史审计，其地址和旧存储判断不能代替0041结论；0041/resource调查见[storage-http.md](storage-http.md)。

## 0031历史基线（已被0041目标取代）

**用户已纠正：新业务ROM基于0041制作。** 本文0031地址、UV对象布局、分区假设及现有OTA实现不允许直接用于0041。便携绑定核心的RV32资源测量可以复用方法，版本相关的HTTP/VFS/UI绑定须使用0041证据重新核对。

2026-10-18；本轮只读原件、交叉编译便携核心，没有生成待刷 ROM，也没有刷机。

## 可运行范围

Bridge 的实际 CompactFrame 客户端，经 loopback HTTP/UDP 到 `device-sim`，执行共享 C++ 绑定、Bundle 存储、激活、data_contract 与模板渲染；隔离 NuttX/LVGL UI 经私有 HOSTFS 端口接收帧和配对描述，回传真实按键、显示及激活回执。这是进程间模拟端口，不是产品图像推送协议，也不是 0031 原件执行证据。

`tools/check-ap01-native.py` 用现有 ESP RISC-V GCC 8.4、`rv32imafc/ilp32f` 编译 `ap01_binding.cpp` / `ap01_pairing.cpp`。ArduinoJson 默认 allocator 产生 4 B 可写静态对象，已改为请求局部分配器；当前两个对象的可写静态段为 0。输出对象不含完整应用入口，没有被伪装成 ROM。

## 0031 证据

基线原件 SHA256 `8a721fc8ef25458d415b2460e4a251e0503a82f7743fdff85b12612190e5c1cb`，base `0x9ffff000`、entry `0xa0000000`。本轮 REA/Ghidra 只读证据在 `artifacts/ap01-platform-integration/device/rea-native-ui.json`，9 条记录。

- `0xa0073dee` 的函数体引用 `lv_timer_handler`，`0xa00742a8` 中作为 libuv 定时回调注册。
- `0xa00742a8` 对 0x188 B UV context 分配、初始化与 timer/input/framebuffer 注册的静态调用链已检查；`0x62fcaab0` 为该路径写入的 timer context 指针。不能直接将它当成本项目 context，或擅用对象尾部未分配内存。
- `0xa007d51e` 的函数体引用 `lv_timer_create`，有 callback/period/user_data 线索。尚未建立本项目 UI 消息投递、绘制结束 ACK 与释放按钮的原生调用合同。
- 原件包含 `src/display/lv_display.c`、`src/widgets/image/lv_image.c` 等路径；不足以确定精确 LVGL 版本、结构体和所有 ABI。原有导航及第三方页面注册载荷可作线索，须继续对照本版函数与补丁前置字节。

REA 静态分析不证明调度、硬件、厂商指令或浮点 ABI；本轮未修改这些原厂指令。

## 资源与剩余接线

RV32 绑定状态本体 440 B、实际每条记录 265 B、最多 8 条。APB1 空表 16 B；单条典型 121 B / 上限 281 B；8 条典型 856 B / 上限 2136 B。替换提交时旧记录、新记录、序列化缓冲及状态合计上限 6816 B，不含分配器、JSON、UI、Bundle。逐函数 `.su`、未解析运行时导入和对象哈希见 `device/native-core-audit.json`；不把逐函数最大值当作调用链总栈。

单帧 38400 B，LVGL 含调色板 38464 B，原子换帧峰 76928 B，加按需 4096 B 图像缓存。三份字体总计 49028 B。双模板集成的已安装内容文件 **145505 B**，是宿主模拟文件系统上的实测值，不能推导为原机可用空间。原件分区资料中的 data 64 KiB 不足以直接容纳该未压缩文件；原机实际 VFS、空闲容量、压缩与缓存布局仍须核实，不能宣称可安装。

完整待刷镜像仍需：统一原生 HTTP 服务/认证 owner 与 OTA 优先级；原生 VFS 原子写入及读回；共享渲染器/Bundle 静态状态转入有明确所有权的 RAM context；UI 线程投递、原有切页和实际显示回执；匹配 ELF 的栈/堆预算；原厂保护区、启动、OTA 和恢复检查。待办统一归 C9，不另建完成承诺。

当前没有新 ROM 路径或 SHA256；`hardware_verified=false`，禁止以宿主/SIM 验收代替原生镜像接线完成。
