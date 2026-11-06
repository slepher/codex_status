# P2 首批审阅（2026-11-06）

最终非实机审阅完成，逐项结论见[completion-audit](completion-audit.md)。最新Bundle拒绝/提交丢ACK/原事务重放窗口专项通过；实际AP01 App与兄弟维护适配在本机ASan/UBSan运行通过，额外验证错误session拒绝与boot稳定。完整workspace542通过/5默认忽略，其中3项相关集成已显式通过。P2不自动提交，未部署或刷写。下文保留各阶段资源与发现记录。

本批完成P2-3及P2-4的Bundle CHUNK重复nonce清理；不是P2整体完成。P1已提交3cf9061，本批不部署Bridge、不刷ROM。

## 协议与恢复

- Wi-Fi入口、管理与setup处理器直接使用uint16_t操作码；HTTP/BLE真实入口不再注入op。Bridge在构帧前选定操作码，不再解析/重新序列化一次JSON。未知指令/旧op字段发送前返回NotSent/http_prepare；不重试写事务。
- trial/activate业务摘要仍先写原有字符串op再写相同字段，避免旧持久回执同ID被误判冲突。principal、namespace、request_id、WTX1高水位不变。测试夹具的名称适配只在host代码中，设备没有双协议入口。
- CHUNK不再发送X-Session-Nonce或URL session_nonce。固件先由v2HttpRoute验证token、传输session、request_no、context，再检查原Bundle事务、当前boot nonce、offset/重放内容及owner；未移除跨重启边界。Fake/AP01 Fake同步。真实AP01 App原先无此扩展头依赖。
- 48B信封没有业务CRC，单会话request_no不能取代跨连接/跨重启的业务摘要及高水位。剩余w3_退役须先引入显式业务序号并验证旧回执/未知事务恢复，不能删除解析后降低防重放。

## 资源

去掉Wi-Fi常规分发中的完整JsonDocument克隆和字符串op转换，新增持久字节为0；存储格式、记录容量不变。宿主存储夹具原始逻辑字节（配置+事务+选择器）：空89B、单条355B、典型两条535B、12配置/8回执5334B，合并上界6430B；NVS准备峰值分别113/622/982/10538B，上界12828B。宿主clone/save峰值增量分别1011/3576/5013/46646B，上界52184B；这不是设备动态RAM实测。

Note4静态RAM97800B，Flash程序1814511B，比P1减少1424B。匹配ELF：wifiControlCommand栈320B、局部调用链1712B；wifiSetupCommand栈512B、局部链1296B；HTTP管理局部链2192B。92项检查通过；运行任务高水位/RF/功耗仍须实机，未在本轮执行。

## 测试审阅

首次workspace回归发现HTTP控制适配错误限制了apply/result等管理指令，并使发送前错误丢失CommandFailure；已恢复统一操作码表和NotSent分类，最终工作区覆盖记录见status。Core原进度测试仍直接调用已移除的JSON op路径，服务端accept因此等待；只终止该测试子进程，改成显式opcode调用后继续剩余套件。App/BLE在相同生产源码下已通过，不重复运行。

独立tools/test-wifi-management.cpp与test-wifi-setup.cpp存在P1基线即可复现的陈旧夹具：前者默认HTTP页足够容纳数据却断言还有下一页；为比较后续行为，双方仅将过期夹具设为BLE页，随后同样在metadata_key/open匹配断言失败。后者仍断言含控制字符的BLE文本合法，与P1拒绝控制字符合同冲突。它们不计为通过。新numeric专用宿主回归、完整存储故障回归及workspace Fake链路承担本批验证；原日志保存在artifacts/protocol-p2。

## P2-2增量审阅

WakeTrace原先已通过SYNC_WAKE_SUMMARY写入同一RTC诊断日志，删除旧JSON投影/历史消费者不会删除日志记录。sync-v1继续冻结快照、逐页摘要确认、持久receipt与原batch恢复；新增测试从真实唤醒周期读取8条摘要，确认后下一批无重复摘要。删除旧history备用Bridge存储代码，用户磁盘历史文件未触碰。移除history_sync_ms及两个RAM字段，不改变正式Light截止；旧字段由共享Plan解析器拒绝。常驻RAM比首批减少72B，Flash减少3980B；新NVS字节为0。

## network_start增量审阅

- op24/25均保留空位并拒绝；op22一次冻结用途、Data seq与回报路由。相同request_id不续期，冲突和关闭原ID明拒；正式PowerPlan截止独立。
- Bridge先读/配置再network_start，未知ACK依原ID认证HTTP proof；未确认片段写仍关闭链路。未登记回报地址不阻挡已许可BLE只读，但不能开网。
- 模拟器主套件70通过，四用途/关闭围栏定向1通过。App主套件131通过/3忽略/1旧RF夹具失败；修正夹具为明确1ms过期后定向1通过，未把失败主日志冒称全绿。删掉旧sync_open后读取更早完成，原700ms负例并未真实过期。
- 首轮其他夹具失败已修复：accepted替代旧applied；窗口到期后HTTP关闭，应通过控制面核对深睡；可达正式Light不必额外network_start。四用途的diag前置用真实Sleep→唤醒→Light生成light_enter，不放宽无诊断待办拒绝。
- Note4构建/匹配ELF92栈/宿主错误帧均通过。新候选与资源见p2-status；未部署/刷写。
- 后续收尾：Bundle/Wi-Fi用途完成钩子的专项故障测试与首次无IP场景审计；P2-4显式业务序号、剩余nonce职责、遗留独立host夹具、最终全套。当前不标记全文或P2完成。

## 显式Wi-Fi序号增量审阅

业务request_id保持原始不透明值，request_seq不从线上ID解析。写入、原事务查询、ACK、setup状态/取消均核对原序号。新写0拒绝；旧无序号回执只可查询/重放、不可退役。WTX1和Bridge缺字段磁盘作业导入是唯一前缀解释处，既有显式字段权威。旧读取兼容不能在未证明存量迁移完成前删除。

WTX2每回执追加8B，配置WFS4/WCM1结构不变；最大8回执增加64B持久/host heap。空/单条/典型2条/12配置8回执逻辑持久字节89/363/551/5398B（原89/355/535/5334），合并上界6494B。NVS准备峰值113/638/1014/10666B，上界12956B；host clone/save峰值1011/3608/5077/46902B，上界52440B。新增仅随实际回执数量分配，不预留空容量；宿主receipt160B（原152），非板上动态测量。Note4静态RAM97728B不变，Flash1811527B；匹配ELF92项通过。

已补真实WTX1+WCM1 fixture，在事务准备/选择器写入前失败保留原记录，选择器提交后回执或读回失败允许重启读取新记录；原ID/序号/高水位始终不变。首次读取报告WTX1，成功迁移报告WTX2。错误格式不猜测，无序号旧终态不可ACK删除。

## Wi-Fi完成钩子故障审阅

HTTP result_ack原先只检查ok，导致unknown也结束临时窗口；BLE入口则缺少结束钩子。现在两入口仅在ok且outcome=retired时结束wifi_setup窗口。模拟器原HTTP包装路由名为command，已改按实际业务opcode对应名称检查，避免Fake漏触发成功路径。四用途定向回归增加HTTP/BLE未知回执、序号冲突、成功退役和正式deadline不变断言，1项通过。最终Note4候选Flash1811619B，92项栈检查通过，见最新status。
