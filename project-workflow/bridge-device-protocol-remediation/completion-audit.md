# 全文非实机完成审计

2026-11-06。范围：`docs/bridge-device-protocol-remediation-design.md` 的 P0/P1/P2 实现、离线软件验证、Note4构建/静态栈和资源说明。未部署Bridge、未刷ROM、未访问设备。P0/P1已提交，P2未提交。

## 逐项证据

最终完整工作区：[sequence-workspace-final.log](../../artifacts/protocol-p2/sequence-workspace-final.log)，542通过、0失败、5默认忽略。其后仅追加/更新测试夹具与宿主测试入口，生产源码未变；下列增量均重新执行通过。

| 要求 | 当前实现及直接证据 |
|---|---|
| P0-1 已认证会话缓存、懒打开、并发读不冲掉写、失效后有界恢复 | device_client的认证MAC/boot缓存及每地址会话锁；完整回归 `status_data_plan_activate_round_trip`、`concurrent_status_and_writes_share_the_authenticated_session`、`status_after_reboot_adopts_new_authenticated_session_before_writing`、`p0_session_refresh_failure_preserves_original_write_outcome`、`p0_activation_handshake_and_write_share_one_budget`。未知写保留原事务，不自动换ID。 |
| P0-2 >512B只读原帧重放、写回执预算 | transport的只读分类/相同摘要回读及有界写ACK；`fixed_frame_http_rejects_malformed_identity_and_retired_requests`覆盖超过512B状态、原号重复和冲突；共享RequestFence回归、BLE 511/512/513边界与独立Wi-Fi最大错误帧通过。op25已在P2退役，不再列为有效只读指令。 |
| P0-3 错误帧、4槽、代次隔离 | [共享生产代码宿主检查](../../artifacts/protocol-p2/sequence-protocol-host.log)执行超时、8193B溢出、损坏头、未认证、4帧入队及第5帧拒绝、既有队列保存；radio使用xQueueCreate(4, sizeof(pointer))。连接代次只在连接/断开/安全变化处推进，帧失败仅清接收状态；无需另建无消费者的rxFrameEpoch。 |
| P0-3 Notify与恢复 | BLE订阅状态通知，兜底150→300→500ms；NativeRadio失败计数/3秒冷却，下次打开使用新Manager/Adapter。完整回归 `cooldown_expires_and_repeated_failures_allow_fresh_adapter_retry`、取消/failed stop、native open与cleanup失败、Fake远端断开重连通过。真实驱动与空口频率属于实机项。 |
| P1-1 UTF-8分页、移除字符串Hex/Base64 | `complete_large_status_roundtrips_shared_pager_and_production_client`逐字节拼接大Status；`wifi_current_projection_freezes_pages_and_owner_manages_all_origins_atomically`覆盖UTF-8/转义及修订过期；[setup ASan/UBSan](../../artifacts/protocol-p2/sequence-setup-tests.log)、[management ASan/UBSan](../../artifacts/protocol-p2/sequence-management-tests.log)覆盖12条最大转义配置；BLE线格式6项通过。二进制诊断records_b64不是退役的字符串标识编码。 |
| P1-2 单包presence、主循环无网络I/O | 固件presenceTask独立执行HTTP，连接/读写250ms；servicePresence只投递/回收工作项，迟到结果检查原身份/期限。ReportSession、session/report端点已删除。最新[显式本机集成](../../artifacts/protocol-p2/final-opt-in-app-verified.log)3项通过，含生产presence处理器→独立Fake认证回读→完成排队作业→空闲不重复激活。UDP发现保留，202不是业务成功。 |
| P1-3 头切片、5秒连接池 | device_client只解析HTTP头，body作为帧字节；每地址最多1条空闲连接、5秒/runtime归属/Connection:close检查。`persistent_http_reuses_one_connection_and_honors_server_close`、`complete_ack_does_not_wait_for_socket_close`、`lost_ack_on_reused_connection_drops_socket_without_replaying_write`及跨runtime用例通过。Note4上游WebServer主动close，未宣称该板已有keep-alive收益。 |
| P2-1 即时开窗统一、四用途、完成与失败 | op22要求wifi_setup/data_sync/diag_sync/bundle，op24空位拒绝；正式Plan.network保留。同ID不续期，修改载荷拒绝，关闭ID不重开。完整模拟器71项覆盖首次无IP、Data不受诊断开关阻挡、deadline独立、诊断原批次恢复。[最新完成钩子专项](../../artifacts/protocol-p2/bundle-completion-tests.log)通过：Wi-Fi HTTP/BLE未知/冲突不关窗、退役关窗；Bundle拒绝不关窗、提交丢ACK仍关窗、原事务重放不重复提交、正式期限不变。 |
| P2-2 history统一sync-v1 | GET /history、op25、Plan.history_sync_ms及旧App消费者删除；`current_device_protocol_rejects_retired_routes_and_ble_markers`、`wake_summaries_use_sync_v1_pages_and_durable_completion_only`通过，后者生成8条真实模拟唤醒摘要、分页确认并证明下一批不重复。额外显式Bridge archive测试通过。 |
| P2-3 数字Wi-Fi分发 | HTTP/BLE以uint16_t opcode调用共享处理器；线上无重新注入JSON op。客户端 `wifi_control_uses_frame_operation_without_json_discriminator`与[数字入口ASan/UBSan](../../artifacts/protocol-p2/sequence-host-tests.log)通过，旧字段/未知操作拒绝，无持久化或RF副作用。 |
| P2-4 两层身份、跨重启回执 | 帧session/request_no仅管传输；JSON boot_id稳定。Data/Plan用原业务ID，CHUNK不再传明文nonce。显式Wi-Fi request_seq贯穿写/查询/ACK/取消，WTX2每回执8B；WTX1/Bridge旧磁盘作业只在导入解释规范w3_。核心磁盘迁移及历史结算4项、真实WTX1选择器写前/提交后故障宿主回归通过；无序号旧回执不能退役，新写0拒绝。旧格式兼容读取不在本次删除，后续删除须证明存量已迁移。 |
| AP01关联变更 | [实际App宿主ASan/UBSan报告](../../artifacts/protocol-p2/native-app-report.json)通过完整分块/提交重放、激活丢重命名ACK、重启恢复、维护适配、页面恢复/显示失败；错误session拒绝、重新握手boot_id不变且无session_nonce输出。使用兄弟仓库维护实现，无复制；边界检查通过。宿主运行不证明ROM ABI/真实VFS/UI。 |
| §5构建/资源 | [最终Note4构建](../../artifacts/protocol-p2/sequence-final-note4-build.log)、[匹配ELF 92项栈](../../artifacts/protocol-p2/sequence-final-wifi-stack.log)、[候选哈希](../../artifacts/protocol-p2/sequence-final-candidate.txt)通过。持久/host峰值空、单条、典型、上限见[存储测试](../../artifacts/protocol-p2/sequence-storage-tests.log)和[p2-review](p2-review.md)。PROGRESS记录最终候选，未把源码版本号当部署证据。 |

## 实现差异与验收边界

原设计中的辅助函数/状态结构示意不作为重复实现要求：P2移除了P0草案里的session_nonce缓存，接收缓冲复位无需额外无消费者epoch；每次恢复重建adapter，无需保留已失效对象到第三次失败。上述行为由故障用例验证。

3600/320B是原文页上限，转义后须满足4048/400B载荷预算，不保证每个7KB JSON恰好2页。约4KB临时堆、常驻RAM净减少、90%空口降幅、50–75%延迟降幅和loop<5ms均为原设计目标，不能当实测结论：当前静态RAM97728B/Flash1811619B，Presence最多一个动态6144B任务栈；WTX2最多增加64B持久回执。实际分配和生命周期说明已记录，未伪造性能收益。

全套默认忽略5项中，3项App/Fake集成已显式通过。另2项是既有AP01外部模拟器配对场景和私有中文字库分区容量实验，属于原专项而非本次协议整改要求；其相关App协议/持久行为已由实际App宿主用例覆盖，不宣称那两项已运行。

未执行且不属于本目标：部署/刷写、真实设备/RF与Win/mac原生驱动交付、板上动态栈/堆高水位、电流、主循环实际时延、AP01恢复ROM ABI和同机救援。A9/B6/C8现场事项不因此结项。

结论：文档要求的非实机实现与验证完成；P2保持未提交，部署边界不变。
