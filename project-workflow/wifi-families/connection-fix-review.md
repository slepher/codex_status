# C8 / A9 审阅（2026-10-31）

- C8 根因：open_link 的4秒 timeout 丢弃 connect Future，disconnect只清理当前已登记的对象；WinRT connect先创建/连接局部BLEDevice，成功后才安装到shared.device。因此disconnect=ok不能证明被丢弃的原操作已结束。旧代码对所有Elapsed一律隔离，无迟到完成收尾机会。
- 修复：固定总截止不延长；保留原Future并在原预算内等待迟到完成，至少留1秒断开。迟到成功/失败均仍向调用方报告原超时，不继续setup、不换连接重试；只有原操作已结束且disconnect成功才释放NativeLease。挂起、嵌套超时、取消/清理失败仍隔离。未改vendor、扫描/写入隔离和watchdog。
- A9 根因：加载context后未恢复运行时ACK，set_contract强制dirty；来源依次初始化的中间缺失又会留下粘滞dirty。修复后恢复ACK，用当前push指纹与ACK比较；实际合同/context变化明确清空运行态与持久态ACK。
- 未确认Data原先只保留序号高水位，未落盘冻结内容。新增data_in_flight每设备最多一份，发送前随原state原子写入；ACK后清除。重启继续原seq/CRC/payload，最新来源变化仍等待原ACK后推送。
- 资源：仅宿主Rust内存/JSON状态变化，按实际在途设备和字段分配；无固件/NVS/任务栈变化。无在途时字段为空map，在途开销为已有InFlight实际JSON大小；没有历史快照仓库。
- 兼容：旧state缺data_in_flight按空map加载；新state含此键，旧Bridge使用deny_unknown_fields，回退必须同时恢复升级前exe/data备份。当前未部署、未改真实data。
- 范围：源码/合成生产路径能证明生命周期和业务决策，不能证明23:45那次WinRT操作何时完成，也不能替代Windows实机RF/多周期和macOS验收；C8/A9历史其他矩阵不结项。
