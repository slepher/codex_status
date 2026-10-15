# 超时与取消专项审查（2026-10-15，B6/C8）

## 范围与状态

用户要求检查其他模块。定向审查Bridge App会合/配网/历史/token/claim、BLE命令与native生命周期、core HTTP与Codex RPC、MCP OTA、UI及固件主要网络期限；未通读无关历史或AP01专项。本轮未修生产代码、未重新部署/重启、未访问真实设备。此前10:21修复部署保持。

## 已注入确认：四条相同取消链

临时测试使用现有App Fake HTTP分片入口、生产DeviceConnection和实际App函数，合成凭据、loopback、CODEX_STATUS_DATA指向tempdir；不操作运行data。每个case先建立真实command session，注入写入迟到/挂起，运行原调用，再尝试status并断开。全部观察到not_sent/previous_write_unknown；生产close_bounded仍因write_uncertain保留NativeLease隔离（该收尾因果由源码确认，四case没有额外开启native模拟租约）。临时fixture和测试在finally精确移除，测试源码与输出留本地证据，不向仓库留下通过即表示故障存在的“回归测试”。

| 优先级 | 位置 | 触发与结果 | 证据 |
| --- | --- | --- | --- |
| P1 | app/src/wifi.rs:434 automatic_on_link | 外层剩余预算包住wifi_match/owner/恢复/activate链，内部command仍按5s/RF计时；wifi_match或owner错误又被吞为Ok。尾预算小于在途写时可取消写、返回wifi_window_timeout，下一命令禁止。 | 300ms预算，302ms后wifi_window_timeout，下一status previous_write_unknown。 |
| P1 | app/src/wifi.rs:680/705/731；rendezvous.rs:370 | snapshot及wifi_status虽然业务只读，GATT载体仍要分片写请求。外层分页/状态读取timeout不能保证各页命令在同一期限结束；note_link同样受外层timeout取消。 | 实际read_on_link_budget→unified_on_link→BleTransport snapshot，300ms预算/305ms返回，job waiting_rendezvous/window_budget_exhausted，下一status previous_write_unknown。其余同类调用点静态确认，未逐个注入。 |
| P1 | app/src/platform.rs:2065 cache_device_token_if_missing | 本机缺operation token时，外层1s包住默认最多5s的request_device_token；超时只记日志并继续endpoint/会合。control_write_until的未知旗不会因外层取消清除。当前已缓存token不走此分支，首次/重建缓存才触发。 | token响应延迟1400ms，真实cache函数1002ms返回，下一status previous_write_unknown。 |
| P2 | app/src/rendezvous.rs:434；platform.rs:1976 sync_wake_history | 外层500ms包住内部2s分页循环，循环又包默认5s的history命令；两层都可能在写中取消，且500ms结果被忽略。正常sync_enabled=true跳过此旧历史分支；disabled/legacy且有wake_generation时仍有风险。 | 使用生产history函数及与调用点一致的500ms外层，501ms后取消、下一status previous_write_unknown。 |

证据：artifacts/bridge-macos/timeout-audit-probe-20261015.log、timeout-audit-probe-20261015.rs.txt。四case在一项观测测试中通过，只证明故障模式可重现，不代表缺陷已修复。

修复方向：将phase绝对Budget贯穿token/history/分页/自动恢复，逐个命令在carrier内取最早期限，页循环不重置预算；取消副作用操作必须返回原始NotSent/WriteUnknown/AckUnknown并在调用者停止本轮。不能统一清write_uncertain/直接解除隔离，也不能因“读业务”而忽略GATT请求写入。

## 相关预算缺口（静态或既有现场证据）

1. **P2 HTTP就绪读取过短**：rendezvous.rs:669每次最多3s调用完整device_client::status；core/device_client.rs:437–505的此函数要锁状态读取、请求并取完全部分页，使用同一总deadline。400ms快速探测（rendezvous.rs:164）、wifi.rs http_control_read中的2s full status也有同样的快探测/完整读取成本不匹配。10:23真实现场日志多次/api/status单次HTTP200耗时约1–3s，但完整proof最终http_proof_budget_exhausted，后续独立完整MCP读取才确认timer/HTTP ready。不能把单页200当完整成功。此处是误判就绪/重复读取，未发现其会设置BLE未知写旗；与四条写取消分开。应按需求使用足够的认证状态投影/compact，或在真实总窗口内完成分页，避免每次短限重启全读。
2. **P2 HTTP claim外层期限与worker不一致**：rendezvous.rs:684–690最多2s包住post_claim_for_mac。main.rs:1516起先有2s identity preflight/可能额外2s认证回退，再启动最多5s的spawn_blocking claim。取消async等待不会停止正在执行的socket worker；可返回claim_unconfirmed后设备才收到claim。当前不冒充成功，且此操作本身可重复续租；未动态复现并发后果，不列作已确认设备失联根因。需要把原期限传入preflight和claim，并保留迟到结果/原操作确认，而非只取消JoinHandle等待。
3. **P2 Codex RPC并非完整20s期限**：core/codex.rs:251–264先await write_line，再给rx套RPC_TIMEOUT=20s；stdin锁、write_all与flush都不在期限内。app-server停读/管道背压可卡在写阶段，20s不会开始计时。pending发送者在写错误/timeout分支未主动remove；正常response/EOF会清理，runtime遇RPC错误会杀进程重建，所以不据此宣称生产内存无限泄漏。此处仅静态确认，未对真实app-server造阻塞；应统一锁/写/应答期限并清理pending，半条JSON写入未知时退役该子进程流，不能复用半帧stdin。
4. **P2 恢复/功耗GET只有每次I/O超时**：core/device.rs:25–36给connect/read/write分别设置相同timeout，read_to_string没有绝对总截止。慢速持续返回且每次间隔小于read timeout可使完整调用远超预算；用于status.json/pmstats的有限预算并非严格保证。与已采用write_until/read剩余deadline的业务device_client不同。本项静态确认，未模拟真实设备慢流；应采用绝对deadline及有界响应长度。

## 已检查的保护与边界

- 业务HTTP device_client已有绝对deadline、锁等待预算、分片发送状态和结构化结果；App DeliveryLock::spawn_blocking把permit移入worker，已有cancelled_http_waiter_keeps_device_exclusive_until_socket_deadline回归。deliver外围timeout并不等于释放仍可写的设备锁，不能当同类确定漏洞报告。
- BLE scan/connect/disconnect超时配套cleanup/NativeLease未知隔离；短scan不是完整业务命令，不能一律延长。
- MCP OTA120s上传错误以结果未知/后续运行核验处理；401才允许重新取token重传，本轮未找到与700ms同型的新增证据。固件同步HTTP与窗口/串口/EPD等待不是Rust future取消模型，本轮未据关键词误列相同缺陷；未做全固件时序验收。
- UI本轮未发现同型业务请求Promise.race/AbortController短取消；不因此宣称界面全部超时路径已测。

## Review / 下一步

优先修配网自动恢复和分页，再修首次token及旧history，补各真实调用点慢写/丢ACK/剩余期限/下一会合回归；HTTP proof和claim期限独立验收；RPC和只读恢复GET总期限作为另一层传输收口。B6/C8保持开放。所有行号以本轮工作区为准；本轮仅审查，未改部署。
