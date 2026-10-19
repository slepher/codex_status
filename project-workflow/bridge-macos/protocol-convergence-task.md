# 实施切片记录

任务来源为 backlog B6/C8；本文件只记录 [计划](protocol-convergence-plan.md) 的执行拆分，不独立增添需求。

- R1a：共享已知 MAC 执行许可入口，覆盖 authorize/run/recover_save；自动会合连接后只try_lock，忙则有界关闭以消除反向等待；竞争、期限与取消回归。
- R1b：自动会合发现与连接拆分，按候选设备取得许可；双设备公平性、MAC复核回归。
- R2：原事务有限查询/ACK、持久预算与显式收尾重试；合并HTTP执行后与定期ACK路径，生产Fake验证取消/过期/预算耗尽与容量释放。
- R3–R5：依次按计划推进，每片先记录具体合同与测试再改；四端 wire 门禁不可跳过。

完成情况和当前下一步以 [status](protocol-convergence-status.md) 为准。
