# Note4 休眠与临时同步窗口边界校验（2026-10-11）

关联：backlog A9、B6/C8。范围：只读实机/运行记录、源码校验、现有测试及预算复现；未改生产代码、未重启Bridge、未刷ROM。

## 用户确认的边界

自动全量同步可以在正常联系/唤醒机会进入**临时 Light 传输状态**，不生成、不续期正式 Light PowerPlan。临时窗口独立计时，正常完成立即收尾；正式 Light 到期不截断已合法准入的同步。临时窗口有硬截止，不能因读取、进度或重试无限延期。身份/owner/低电/安全边界仍有效。

## 实机结论

Note4 7C4FADB93408 / .82 / ota_0，19:24–19:25（香港时间）未插PC USB、BLE关闭、OTA未进行、手动/BOOT窗口为0。PM SLEEP占比68%→71%，所以CPU light sleep实际存在。当前正式计划3530 light/600s，剩余416→327s，重复同ID不续期。认证缓存reset_reason=8、timer wake；累计33个clock wakes/34个net windows，不能称设备长期从未深睡。

Bridge日志18:21–18:45、18:53–19:17连续接受0秒Sleep计划。19:21:07联合Plan/Data seq459 ACK却授予600s正式Light。Profile full_sync_s=3600；上一数据ACK是18:17:39 seq458，其后无usage changed事件直至本轮。周期全量刷新创建in_flight，`service.rs:2688`将任何in_flight视为正式Light工作，`:2716`授予MAX_LIGHT_S=600。这是周期数据被当作活跃事件的直接实现原因；同期diagnostics_due=true是另一项独立诊断义务，不能混称同一个同步协议。

## 传输截止期缺口

1. `app/rendezvous.rs:593`：HTTP传输预算在正式Light remaining>0时只取remaining-1；只有remaining=0才取sync.remaining_ms-1s。提取当前生产Budget及这个分支编译复现：Light剩2s、临时同步剩80s，得到约1s；Light已到期、临时同步剩80s，反而约79s。3s传输在前一种情况下不能完成。预算一旦冻结不会在Light到期后自动切换。
2. `app/rendezvous.rs:398`：HTTP已可达的路径用所发计划light_duration_s限制预算；Sleep duration=0经max(1)限制为1s，可能截断合法临时同步。
3. `firmware/radio.cpp:538,745`及`transport.cpp:769,948`：正式Light仍有效时优先取其deadline，未合并更长的临时sync deadline。RendezvousProtocol::admitHttp只缩短已准入deadline，正式Light到期后切换分支不会自动恢复被缩短的网络准备期限。此项为源码已确认的准入/ready报告风险，未做实机临界时刻故障注入，不能宣称本轮观察到设备中途断网。

已有保护：`application.cpp:456,476`在syncOpenThisWake内暂缓正式Light到期的deep决策，按独立90s总窗口或complete后200ms退出；正式Light到期时也能开启最终诊断窗口。`matching_http`允许Light已到期且WIFI_SYNC_ONCE仍有效的同session/wake/plan HTTP。同步归档保存batch/offset/prefix，未完成可原批续传。这些保护不能弥补上述准入和预算截断。

## 校验与证据

- 当前源码 `cargo test -p bridge-app platform::rendezvous::tests --target-dir target-note4-deploy`：2 passed。它验证到期后的身份/窗口接受条件，不验证跨到期分页传输。
- `cargo test -p bridge-app sync_diagnostics::tests --target-dir target-note4-deploy`：4 passed / 1 ignored（完整Fake ROM联测需指定模拟器exe，本轮未运行）。确认断点归档、丢回执及owner隔离；不代替临界时刻实机验收。
- 预算复现：[源码](../../artifacts/sync-window-budget-repro-20261011.rs)，命令 `rustc artifacts/sync-window-budget-repro-20261011.rs -o artifacts/sync-window-budget-repro-20261011.exe` 后运行；断言通过，确认当前缺陷。
- 无凭据实机快照：[证据](../../artifacts/note4-sleep-investigation-20261011.json)。原Bridge日志保留在运行data/logs，不复制token或私人诊断原文。

结论：临时同步窗口机制已存在，但当前实现**不完整满足**用户边界。后续修正必须同时区分周期同步与真实push电源意图，并统一Bridge/设备的临时窗口准入和剩余预算；不能只删600s授予。完整验收至少覆盖Sleep+周期同步、Light剩1–2s跨到期、多页慢传输、完成即退出、硬截止、丢ACK原批续传及owner/身份拒绝。未修改或部署修复。

后续：用户追加要求后已完成[源码/协议候选修正](sync-window-fix-20261011.md)，上述调查结论描述修正前行为；现场部署仍为.82/原Bridge。
