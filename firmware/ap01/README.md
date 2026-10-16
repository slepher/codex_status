# AP01 0031 native XIP App

## 独立维护核心（2026-10-15）

`maintenance.h/.c`是可单独编译的维护事务核心：完整身份认证回调、流式接收、包预检、停业务前保存状态、共用排他安装、未知结果查询、显式AMT1恢复记录和内置文案；`test_maintenance.c`在宿主和RV32执行同一组故障检查。核心自身不依赖业务执行单元，但设备端独立任务/HTTP、持久binding、原厂共用排他、文件安装及UI适配尚不存在，**不是已运行的设备服务或维护界面，不是可刷ROM**。现有旁仓库只有补丁源码和原厂二进制，不是原厂完整工程；按用户要求不额外逆向。

构建用 `tools/ap01/build_maintenance.py`，详细结果及资源见[维护构建记录](../../project-workflow/ap01/maintenance-build-20261015.md)。下面同步ABI诊断候选保持其原有范围，不用于启动维护服务。

本目录是本项目的可交叉编译源码，不改第三方克隆。首版为**同步 ABI 诊断候选**：原厂创建对象 → 校验固化的 APX1 包 → 初始化 App 状态 → init/step/stop → 释放状态 → 返回原厂对象。它不实现完整 Bridge、任意文件加载、App-only OTA 或持久安全启动。

`entry.S` 包装原厂 `0xa00b2732` 的6字节调用，保留 `s0–s11`、SP、GP及原厂返回值；原厂对象为空时跳过 App。代码固定在原厂 GIF 无损压缩后腾出的 XIP 区，不从通用堆执行，不改 Boot2、分区、原厂网络/更新函数或按键回调。接入点位于原厂 UI 创建路径，并不证明每次开机都会触发；实机须验证实际调用时机。

`app_abi.h` 定义 ABI1：`init/step/stop(api, state, state_bytes, ...)` 和带长度/版本的服务表。API仅诊断回报，调用者提供分配/释放。禁止线程、保留API/栈指针或异步回调；返回后App不继续运行。通用`ap01_runtime_stop`失败留状态供重试，调用者须保持runtime/API/context；本ROM固定同步App结束后不重试，错误返回也释放其唯一workspace，不推广为任意App外部资源恢复。

APX1 为112字节显式小端头：magic/format/header/total/ABI/flags、固定XIP基址、text偏移/长度、三入口相对text偏移、data偏移/长度、BSS长度、16字节状态对齐、payload CRC32、header CRC32、0031原件SHA256和两个零保留字。CRC使用IEEE格式；header CRC计算时自身字段清零。格式见实现中的字段读取顺序，磁盘不写 native struct。校验全部长度/整数溢出/入口/间隙/状态预算后才分配；data复制、BSS清零，raw malloc指针独立保留用于free。**CRC和基线SHA仅检查损坏与兼容性，不验证发布者或代码安全。**

链接脚本拒绝隐含 `.data/.bss/.sdata/.sbss`；模块可写数据只能放包的状态初始化区。`base.c`提供freestanding memcpy/memset。包中的原件SHA是构建期base ID声明比较，并非计算当前运行ROM摘要。包CRC/base ID不提供发布者认证。

每次hook正常返回后，经0031原厂日志`0xa007cfa2`发一次`APX1 diag0031 result=%u (0=init-step-stop-ok)`；0成功，其他值为ABI错误枚举。实际Ghidra确认5个固定参数+varargs，日志sink可为空。同机日志开启/读取通道尚未核实，因此marker存在不等于实机可观测已验。原厂logger帧848 B，连entry16 B至少864 B，再加格式helper/回调栈；模拟304 B排除了logger。首刷前须核实日志回读和UI任务余栈。

构建/测试命令和产物见 [工具说明](../../tools/ap01/README.md)、[实现验收](../../project-workflow/ap01/app-implementation-20261013.md)。所有硬件启动、网络保留、看门狗、Flash/XIP映射和更新接受情况仍待刷机后验证；本阶段用户明确要求不刷机。
