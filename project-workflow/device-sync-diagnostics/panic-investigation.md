# 2026-09-29 双设备 panic 调查

1. 保存并用对应 ROM ELF 解码实机 core dump；核对 Note4 reset、同步批次、已完成批次与书桌屏差异，区分协议拒绝和任务栈/硬件异常。
2. 沿共享固件的 HTTP 状态、同步传输、LittleFS 完成路径查找栈峰值；对比 Fake ROM 编译入口与实际 FreeRTOS `loopTask` 栈约束，指出测试漏项。
3. 做最小根因修复与可执行回归；仅在串口采集、ROM 身份与回退路径就绪后做实机验证。每个里程碑更新 `PROGRESS.md`；唯一待办在 `docs/roadmap/backlog.md`。

现场初证：书桌屏旧 panic core dump 已从 COM4/MAC `70041DD7A340` 的 coredump 分区读出；匹配 `0.18.32-bw-sync1` ROM SHA `108E82C8…` 的 ELF 解码显示 `loopTask` 8 KiB 栈用 8048 字节、余 128 字节，`DebugException` 且回溯损坏。Note4 `0.18.33-note4-b-sync1` 也自报 `reset=panic`；其诊断批次 4 已完成、批次 5 的 3725 字节仅 ACK 1024，故目前不能把两台都归因于 `sync_complete`。

2026-09-29 修复候选：`syncProtocolRun` 的 1024+1369 字节分页缓冲、`handleSyncImage` 的 4096 字节块从栈改为堆，主循环栈从 8192 增至 12288 字节。对应 1.54 ELF 的函数栈帧分别从 3072→800 和 4512→448 字节；两个目标增量构建成功。`cargo test -p bridge-render` 通过；显式运行默认忽略的 `sync_v1_bridge_archive_against_note4_fake_rom` 通过。Fake ROM 用宿主线程栈，未模拟 ESP 的 8192 字节 `loopTask`，因此协议测试通过并不排除设备栈溢出。Note4 的 `partitions_note4.csv` 没有 coredump 分区，需 USB 串口实时抓取下一次 panic；当前只能将同因列为高可信推断。
