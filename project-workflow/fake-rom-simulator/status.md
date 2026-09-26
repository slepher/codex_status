# Fake ROM 修订记录

## 2026-09-26：实施增量（未越过 D1/D2/D3 门槛）

同源 C++ Bundle/Data/Activate 业务与 A/B store 已连到 device-sim HTTP，154g 和 Note4 目标有不同身份与编译画布；实例目录文件持久、独占锁与跨 boot nonce 已接。有限 OTA 的 2–3 预置版本、完整上传校验、默认延迟切换与显式 override 来源区分已接。`/sim/storage` 可以注入短写。新增同源电源判定、timer BLE-only 失联、按键 provisional、boot 相对 owner/Plan 时间、按键 8 项循环和 display candidate/成功帧/失败注入。24 h 虚拟时间与 1,442 次本地 wake 已测试；这只是设备单侧逐事件推进。隔离真实 Bridge 已完成双 Fake MAC 的冻结发布/重启后延后交付与一次 OTA 正常路径；全部故障矩阵、fake BLE/协作时钟、全切点断电/电量/USB 仍未完成，阶段状态以 backlog C2 为准。

隔离 target 测试：`cargo test -p device-sim --test bootstrap` 23 项、`bridge-core --lib` 88 项、`bridge-app` 38 项，`bridge-render` 全部通过。真实 `v2_client` 覆盖 1/8 项与 Data/Activate、重启与短写；OTA 定向覆盖错误输入、有效上传、丢 ACK、override。联测发现 Bridge 的 `sending` 未在发送前落盘，现已修并用不依赖其它写盘动作的重启回归测试验证；设备提交后隔离 Bridge 后台认证先对账成功，尚未捕获提交后 `Sending→Unknown` 的进程重启时序。Note4 固件为共享电源判定重建且核对 ROM，未发布/刷写；未操作实机、未停生产桥。

## 2026-09-26：Astra 完成全部剩余方案修订

用户现指定完成 D/E/F，取消 G；仅为 D/E/F 无法覆盖的硬件特有错误补 case 文档，不实现或实测。已修订当前设计、C2 的执行顺序和活动计划，保留历史 A/B/B2/C、D13a–d 原始记录。当前实现已有同源 HTTP/OTA 切片与隔离 Bridge 联测，尚未完成 D/E/F 出口。

核对的源码包括 device-sim、render FFI/build、共享 v2 命令、main 电源入口、owner_store，以及 sleep-aware-bridge 的当前 app/core 状态与 OTA 路径。Bridge 新 OTA/快照引入的直接 wall 时钟需纳入 E。D 阶段已捕获并修复 Bundle 发送前状态未持久化缺陷，M07 提交后 Bridge 重启的 `sending→unknown→succeeded` 已隔离复现；继续补 D 的代表性场景，再接 E 的全链时钟/fake BLE 与 F 联合验收。

2026-09-26 后续实施：Fake 设备实验时钟已用实例文件跨进程恢复，boot uptime 与实验单调时间分开；控制面帧数据与两族同源预览逐字节一致。进程级重启回归及 bootstrap 25 项通过。Bridge 逐 MAC 时钟、fake BLE 和协作 step/max 仍未完成。硬件边界 case 见 `hardware-cases.md`，仅文档。

本轮未改代码、运行测试二进制、构建、接触设备或启动/停止 Bridge。验证与限制见 review.md；实际待办和阶段状态只从 backlog C2 读取。
