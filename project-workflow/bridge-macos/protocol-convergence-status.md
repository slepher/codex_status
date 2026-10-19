# 收敛状态

2026-10-19 18:16（Asia/Shanghai），基线4868c8c。R1发现/连接拆分与R2历史事务收尾已实现，主回归完成，Fake夹具修复已定向通过，未部署。用户要求快速收尾后提交，R3–R5不在本次继续扩展。

R1：已知MAC按设备→BLE获取许可；自动会合先过滤忙设备，扫描结束释放radio，再取得候选设备与radio许可后连接，INFO复核完整MAC。发现不创建会话，单轮准入共享25秒截止；忙A不影响B连接。HTTP worker保持原许可直至实际结束。

R2：停止执行的原事务只用冻结principal/namespace/request_id查询与ACK，不能恢复apply/activate。先持久化确定证据，再ACK；历史结果不覆盖当前缓存。收尾每60秒最多3次，预算跨重启保留，reconcile_job只重置收尾预算，不强制唤醒。未知/身份不符不ACK；收到ACK才释放回执/Job容量。HTTP正常执行后的旧ACK循环已删除，与定期收尾共用入口；HTTP/BLE共用core收尾逻辑。

Sol子代理新增Fake专项4/4通过，覆盖真实提交后取消/过期/确认预算耗尽、设备ACK已生效但响应丢失、双重启、身份隔离、8回执及32Job容量。日志artifacts/bridge-macos/protocol-convergence-fake-settlement.log。扩展Fake Wi-Fi回归初次9通过/1失败：夹具把4139字节状态的status_read分页封套误当完整状态，plan的MAC/session为null而被正确拒绝。改用生产device_client::status组装后原失败项1/1通过；最终使用明确的authenticated_status夹具，保留原始request语义；Note4关联15/15通过，日志protocol-convergence-fake-note4.log。证据protocol-convergence-fake-status-{diagnosis,fixed}.log。主core/app/ble/mcp回归345通过/5条件忽略（App117/BLE29/Core170/AP01模板3/device_client7/envelope7/template9/MCP3）；忽略项为3项App条件测试、AP01 live/storage各1，未冒充执行。日志protocol-convergence-r2-regression.log。

本次只有Bridge内部与本地Job持久数据扩展，未改变设备wire。Note4/1.54共用现有result/result_ack；Fake执行生产固件路径；AP01应用仍在ports/ap01，不新增ESP Wi-Fi管理能力。后续wire变更仍须四端同步、无旧版兼容。没有构建/刷写ROM、启停Bridge或访问真实运行data；实机RF/掉电/原生BLE未验。

新增settlement只在收尾发生时落盘。紧凑JSON增量：空0、成功单条70 B、典型未知单条93 B/8条744 B、以25字符收尾错误及64字符结果错误计算的样例单条186 B/32条5952 B。宿主内存为每Job一个Option及实际错误字符串，未新增整仓库或设备NVS/栈对象；设备资源增量0。部署前冻结备份，写入新Job字段后不得直接用旧Bridge读取，回滚需恢复冻结备份；不引入wire兼容层。

生产Rust目前净增约180行，内嵌测试净增216行，另新增192行Fake测试（不含后续夹具修复）。这是补齐正确性能力与局部收敛，不宣称总代码减少。R3投影、R4私有JSON上下文、R5异步HTTP/wire仍待完成。AP01应用可改，代码当前随整ROM发布，独立更新缺口归C9。
