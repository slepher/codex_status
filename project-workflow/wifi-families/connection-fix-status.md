# C8 / A9 修复状态

更新时间：2026-10-31 00:47，Asia/Hong_Kong。

源码修复完成，Windows已于00:44按用户授权重建部署，未提交；Mac原生仍待验，未改设备ROM。

- C8：保留超时原生操作，固定总预算内收尾后再断开；迟到完成仍返回原超时，不触发第二次connect。确认收尾+disconnect成功才解除占用，持续挂起/取消/嵌套超时/关闭失败仍隔离。
- A9：恢复context的已确认push/full指纹；当前来源就绪后与ACK比较，同值/同缺失/同质量不重复push、不授600秒Light。真实变化继续push；合同/context/设备数据丢失使基线失效。未确认Data新增持久冻结快照，重启复用原seq/CRC/内容。
- 周期全量及其未知重试沿用临时传输规则，不授正式Light；full_sync_deadline仍只由ACK推进。

验证：

- 红：收紧原remaining重启测试后2项失败（同值重启返回light_data）；C8迟到完成测试失败于disconnect raced the pending native operation。
- 绿：`cargo test -p bridge-core -p bridge-ble -p bridge-app --lib --bins`：Core186、BLE32、App129，共347通过；App3条件忽略（外部Fake/report接口），0失败。
- 覆盖：BLE迟到成功/失败、后续会合、持续挂起、嵌套超时及取消；HTTP/BLE同值重开、push/pull覆盖、缺失/质量变化、原在途周期/真实push恢复、合同变化后再次重启、设备同context丢Data补发、纯诊断路径。
- `git diff --check`通过；既有btleplug lifetime及App测试nonce warning未扩大处理。
- 日志：[测试输出](../../artifacts/wifi-families/connection-fix-tests-20261031.log)。根因/兼容/资源：[审阅](connection-fix-review.md)。

验收边界：仅本事件两项源码缺陷修复，不关闭C8/A9历史剩余项。Windows已捕获一次真实超时settled=true且后续自然会合成功；多周期/长期功耗及macOS原生仍待验，不能保证所有原生挂起均无需恢复重启。

部署注意：新state增加data_in_flight，旧版deny_unknown_fields不接受；部署前冻结exe和data备份，回退须配套恢复。旧state可被新版读取，旧版已丢失的在途payload不能凭空重建。

## 2026-10-31 Windows部署与首轮实机验证

- 00:43核对默认主PID47744/watchdog16368、8765/8766、同exe任务；依watchdog→任务→核实残留顺序停桥。旧exe及346文件data完整备份并逐文件SHA256核对，构建期间data无变化。
- 当前工作区HEAD6f61814+dirty，在bridge/隔离target-note4-sync运行cargo build -p bridge-app，36.40s成功。新39268864 B，SHA256 cead2571f17c3010a97d6154d2464386977f1813c62551201584705d5c5d75f1；旧5f5eae51d3cdc122c00b4118c27a688e1fec33d006a931a311f29f0dc8f8bcbb。
- 00:44:28由提权tools/start-bridge.ps1/计划任务启动，主PID36028/watchdog17964；任务Running、health204/MCP44。3设备身份/Profile/同步许可、Wi-Fi库及4个Bridge配置/凭据文件校验保持一致。
- 00:45:08 scan13真实建链超时，00:45:16原操作settled=true、disconnect ok；未隔离重启。00:46:12后续scan31认证同MAC7C4FADB93408/wake7345成功；00:46:13 delivery=idle/needs_http=false，Plan9028 ACK remaining0，随后正常关闭并完成会合。部署前后context不变、Data保持1336/CRC88b1e361，未重复推同内容、未授600秒Light；新state的data_in_flight字段已落盘。
- 验证末同主PID36028仍运行/health204。此为一轮真实超时恢复与后续会合样本，长期稳定性仍归C8/A9；未为测试断网、强制唤醒或改电源策略。

私有证据与配套回退备份：artifacts/wifi-families/connection-fix-deploy-20261031-0043/（旧exe、完整data、manifest、deployment.json、build.log、verification.json、note4-verification.json、runtime.log）。回退必须配套恢复旧exe/data，不可只替换旧exe读取新state。