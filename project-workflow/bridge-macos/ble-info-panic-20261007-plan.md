# Note4 .68 BLE INFO panic 修复

## 证据与范围

USB只读dump CRC有效18084B，匹配.68 ELF前缀；nimble_host认证完成回调在refreshInfo→infoExtra String::move/free触发重复释放断言。主循环bleSetInfoExtra与BLE回调refreshInfo无锁更新同一String。修复信息更新与INFO/Status特征生命周期；保留BLE认证、claim、既有窗口及GATT表。独立Bridge重试耗尽另归B6，本轮不扩大为桥重连重构。

## 执行与验收

1. 用生产refreshInfo/设置函数与真实Arduino WString的两个线程交错复现同一缓冲重复释放；旧代码必须red。
2. 最小同步保护缓存、provider和INFO/Status特征使用；关闭时先在锁内脱离特征，释放锁后停止host，host停止后再清接收缓存，避免等待host的死锁和清缓存竞态。
3. 宿主交错/并发压力/关闭后更新与重新发布green及ASan/UBSan；检查生产调用点、Wi-Fi栈门禁与实际ELF资源。
4. 只构建Note4 .69，不clean/并行pio。核ROM marker/size/hash、USB精确MAC与真实活动槽，保留旧dump/ROM/分区/NVS。用户授权修复后部署只更新应用，工具复位单记，核运行镜像/重复认证与Wi-Fi链路；若实体未通过如实记录。

## 状态

已实施并部署：旧dump与.68 ELF保全于artifacts/note4-wifi-20261006。

## 根因、实现与验证

- 生产refreshInfo、bleSetInfoExtra、bleSetInfoProvider和真实Arduino WString在两个线程交错，观察到同一缓冲两次进入free；旧无锁路径以exit86明确red。修后ASan/UBSan通过8000次认证/读取/主循环更新。测试同时编入生产bleDeinit，模拟停止host时最终回调必须在另一线程获取锁，确认没有持锁等待host的死锁；脱离特征后的更新不写失效GATT，重新发布可写。
- 使用一个原生std::recursive_mutex保护缓存/provider/fw、INFO与Status特征创建/访问/脱离及peerAddress/peerIdentity字符串复制。不在锁内等待host停止；接收分片String只在host停止后清空。权限、GATT表、窗口/PowerPlan、持久化格式未更改。
- 资源：每种配置条数额外持久化均0B；一个mutex对象4B，实际IDF后端malloc的esp_pthread_mutex_t8B+FreeRTOS队列92B=100B堆载荷（另计分配器开销），初始化一次，无每请求常驻仓库副本。静态RAM117324B与.68一致（链接对齐不可解读为mutex零成本）。.68→.69真实ELF局部帧refreshInfo208、bleBegin64、bleDeinit32、bleNotifyStatusQuiet32B均不增，锁调用链另有pthread库帧，不能把局部帧当整任务峰值。Wi-Fi匹配ELF门禁通过，HTTP局部链2912B，既有BLE dispatcher1136B仍沿原预算。
- 唯一Note4构建64.48s，只编ble_bridge/main，未clean/重装/切其他env。ROM1863488B、SHA25611990BB19076EF5841A827BA5A9616259759B2485AAA26320AA31771F8A92F73；ELF5D8E3E600E368D7B70C27F37BFB5A9451FD3E5CF9BC44F5EE328545A8CFABBA2，证据artifacts/ble-info-panic-20261007。
- USB精确MAC7C4FADB93408/COM5，真实otadata CRC seq17/18/state2选择ota_1@0x610000。仅写应用并Hash verified；写后未恢复应用前，0x8000..0x10000真实分区/NVS/otadata/phy逐字节相同。保留转储/存储与配置。工具进入ROM与hard-reset单列；运行镜像认证SHA/大小/fw/target/MAC/slot完全匹配候选。
- 实体已见绑定认证、claim、600s正式Plan、保存home的activation回执、wd21-la/192.168.3.177、HTTP handoff与BLE释放。为已有attempts3阻挡显式use_current重开一次有限机会，没有编辑运行data/凭据/配对。Bridge源码/exe未修改，计数成功不归零问题仍B6待修。
- Windows监听边界修正：pyserial.serialwin32._reconfigure_port通过SetCommState仍配置DTR/RTS，跳过_update_*不能推广为本机保现场保证；验证期间认证reset_reason11=USB复位，不能冒充新panic。停止串口重开后改用认证HTTP跨60s同boot采样，结果见stability-69.json。旧dump确定为.68 panic，保留完整原件；真实长期RF/电池多轮仍待验。
