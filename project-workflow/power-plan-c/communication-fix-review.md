# 审阅
2026-10-31

- owner：缺字段不等于null；仅同非空boot_id保留观察，读取不更新owner_observed_at。显式null/他人占用生效；过期/时钟回拨转为受鉴权claim，不把旧owner无限当真。保留60秒续租节奏。
- Plan：共享HTTP send_plan出口抑制；必须已有同ID成功ACK、在线且30秒内认证缓存、明确boot_id、mode/period/granted全部一致、非provisional、Light剩余>0。新计划/未知ACK/状态缺字段/变更/过期仍发送。跳过返回sent=false、confirmation=authenticated_status，不伪造新ACK。
- HTTP ACK检查收紧到result=applied且原plan_id、剩余字段存在；无效ACK不登记为accepted，避免后续误抑制。
- 真实HTTP Fake验证相同计划零请求、新计划到达/api/plan；其他边界由缓存回归覆盖。
- 未更改ROM、600秒策略、BLE联系和分钟显示；不恢复旧业务状态协议、不增加依赖/持久格式。
- 红：missing_owner回归在原实现失败；绿：App132通过/3原条件忽略，日志artifacts/note4-communication-fix-tests.log。原btleplug lifetime与unused nonce警告保留。
- 实际功耗比未测量，历史观测2.55:1仅用于场景排序。
