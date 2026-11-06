# P2 实施计划（2026-11-06）

P1已提交3cf9061。用户要求继续P2，明确不部署Bridge、不刷ROM。

1. 先完成P2-3：Wi-Fi业务入口直接接收48B操作码，移除HTTP/BLE入口把op重新塞回JSON及Bridge剥离op的胶水；共享宿主/Fake同步，业务哈希和持久事务ID不变。
2. 清理P2-4中Bundle CHUNK冗余session_nonce参数/头；传输session、防重放和原业务request/offset继续校验。
3. P2-2旧history下线须同步替换Bridge现有尾部拉取为sync-v1，保持日志确认与RF期限，不能只删路由。
4. P2-1统一开窗须保留当前临时Data/诊断窗口、重复ID不续期与正式PowerPlan期限。
5. P2-4的w3_解析承载WTX1高水位，不能直接删除；须先设计显式业务sequence并验证旧持久回执/未知事务恢复，再退役前缀。48B信封无业务CRC，不能替代跨重启业务幂等。

按可验证里程碑落地；不将已完成的小项冒称P2全部完成。每次涉及固件的最终候选运行Note4构建/匹配ELF栈，源码通过不代替实机验收。

## P2-1 当前落地合同

- 退役op24(sync_open)，保留空位；独立即时联网只用op22 network_start，reason必填wifi_setup/data_sync/diag_sync/bundle。正式计划伴随开网仍由Plan.network负责。
- 同一network_start请求一次完成临时窗口准入和回报路由冻结；Bridge先完成必要的BLE配置，再发送这一个指令，避免窗口先开/路由后补的两阶段未知状态。
- request_id是原联网业务操作ID；同ID同载荷只确认，不更新opened_at；同ID改变purpose/Data序号/回报路由明拒；已关闭同ID不能重新开窗。
- 临时窗口沿用既有90s硬截止/90s无进展预算，独立于正式Light；data_sync不依赖诊断enabled。diag_sync仍要求诊断启用且存在due/pending，不以诊断开关阻止Data。
- Data ACK、诊断complete、Bundle commit、Wi-Fi事务result_ack各结束其对应临时业务；若正式Light仍有效，只结束临时许可。失败/超时关闭临时窗口但不撤销正式Plan。
- 设备自动在正式Light结束时排空诊断是既有计划生命周期行为，保留，不另引入BLE指令或延长正式期限。

## P2-4 身份收敛实施

传输会话仅从48B帧头获取；Status/BLE摘要与sync-v1回执只暴露boot_id，用于跨载体同次启动验证。Bundle接收缓存绑定boot身份，BLE重连不使合法业务失效；仍校验帧session、owner、业务ID及offset。AP01去除CHUNK明文nonce检查与摘要字段，原生维护ABI不改。Data/Plan correlation采用既有业务context/seq/plan_id，读取仅用操作名配合帧request_no，不再生成CRC字符串ID。WTX/w3_迁移独立进行，先保留现有高水位直到显式序号完整替代并通过故障回归。

## 显式序号与持久迁移合同

新 Wi-Fi apply/trial/activate 使用 request_seq:u64>0；result/result_ack/setup_status/setup_cancel 携带原序号。request_id 不透明且永久冻结，传输重连不生成新业务身份。同 ID 不同序号拒绝。WTX2 在原回执后追加8B；WFS4配置及WCM1选择器不变，离线归档升WFS6以保留该字段。WTX1/旧WFS磁盘导入与Bridge缺字段作业导入才允许解释规范w3_；无序号旧回执保持0、不能退役，新写0拒绝。旧高水位不得降低。

迁移复用准备槽/读回/WCM1原子提交，成功后清旧槽。测试覆盖事务准备和选择器写失败、选择器已提交但回执/读回失败、重启原ID查询与退役；旧兼容读取删除作为后续单独验收，不随本次清理。
