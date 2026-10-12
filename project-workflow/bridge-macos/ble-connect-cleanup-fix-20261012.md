# Bridge建链失败清理修复与重启

用户授权：先修复Bridge问题，然后重启Bridge。关联B6及13:19失联取证；本次只改Bridge，不实施固件/协议整体重设计、不刷ROM。

## Plan / tasks

1. 在生产建链重试与失败清理控制流建立回归：两次connect失败、disconnect永不回调时，必须返回原错误、释放调用者锁并开始下一轮。
2. 复用原CLOSE_BUDGET，为全部BLE断开清理统一有限等待及开始/结束/错误/耗尽日志，不增加连接或业务超时。
3. 运行相关Rust回归；核对并停止默认LaunchAgent及同路径子进程，私有备份data/安装身份与旧App；重建并签名核验当前工作区Bridge。
4. 部署原路径，启动原LaunchAgent，核验健康/MCP/权限/扫描及真实同MAC认证会合；记录实际验收边界。

## Status

15:55:56修复版部署并启动原默认LaunchAgent完成。15:51停止旧supervisor11717/child11724并确认退出，私有备份data/安装身份/原plist与旧App，state备份及启动前hash一致。当前supervisor15210/child15214，8765/8766归属原路径，health204/MCP43工具，双权限allowed。现有工作区含分类修复和其他未提交改动，全部保留；未提交或刷ROM。

## 部署与实机结果

- release构建3m19s；候选 `artifacts/bridge-macos/BLE Cleanup Candidate.app`，部署 `artifacts/bridge-macos/Codex Status Bridge.app`，x86_64 exe20,850,560 B，SHA256 `fc792b2b2016181550b7629b71846c66eba0aa8f69acd0378bbc1f85ef62acf7`。候选/部署哈希一致，严格签名通过；安装身份及原数据保留。
- 私有回滚备份：`~/Library/Application Support/dev.codexstatus.bridge/deployment-backups/ble-cleanup-20261012-155103/`，包含data/installation-id/managed.plist/previous.app；原exe SHA53a5fab4…a9b63。
- 15:56:04自然会合wake906完成同MAC认证；首轮Plan写入耗尽实际RF预算，明确报告WriteUnknown/result_unknown，清理1ms完成。随后scan2/12/22/32持续执行，证明该失败未阻塞后续扫描；不把该首轮写入记为成功。
- 15:58:04第二自然会合wake908认证成功，Plan3726得到applied ACK/600s，清理正常返回，15:58:22整轮rendezvous complete。15:59认证HTTP status result=ok、同MAC/ROM0.18.83/reset8、Plan3726剩余531s，宿主仍连Xiaomi_1EBB。
- 本轮Bridge分类/UI部分随当前工作区部署生效；固件0.18.85仍是未部署候选，新增固件关键日志/原因保留不冒充已在现场运行。首轮RF写入未知和全部长期/迟到原生回调矩阵仍保留后续分析，不以两次联系关闭整个B6/P2。
- 安全现场记录 `artifacts/bridge-macos/ble-cleanup-20261012-live-safe.log`；部署元数据及MCP有限字段在deployment-safe.json。

## 实现与验证

- `bridge/crates/ble/src/lib.rs`提取生产两次connect重试的原控制流，失败统一进入共享disconnect_bounded；其余15处裸清理与公开close也使用现有CLOSE_BUDGET（2秒），close_bounded复用相同真实载体清理。不增大原4秒connect/120ms重试间隔或业务预算，不spawn遗留清理任务；到期取消future并丢弃句柄，保留原connect错误。
- 清理记录开始、成功、明确失败和disconnect_timeout，关联scan_id/实际耗时/预算，不打印底层任意错误或凭据。无界disconnect.await生产调用已清零。
- 新回归直接经过生产connect重试/失败清理控制流，注入两次连接失败与永不返回的disconnect；修复前3秒外部观察截止失败，修复后约2.13秒返回原错误、释放调用者锁并执行下一连接。不经过Fake连接入口，不把单个timeout测试冒充完整调用链覆盖。
- 最终串行Rust269通过/3条件忽略，Node通过、WK60通过，560未知/1000预算截图已审阅。首轮并发有120ms旧恢复用例在snapshot阶段耗尽、未发却断言sent=true；单独与最终串行全套均通过，保留首轮日志作为时序不稳定证据，未修改测试断言或生产期限。
- 证据：artifacts/bridge-macos/ble-cleanup-20261012-{red,tests,tests-final,recovery-recheck,ui,wk,build}.log；部署身份/备份记录为ble-cleanup-20261012-deployment-safe.json。实体扫描恢复结果见上节；故障注入证明有界收尾，不代表所有macOS平台生命周期缺口已经解决。

## Review

最小共享清理修复，不新增后台worker/回退协议，不更改原事务ID/未知结果围栏、鉴权、PowerPlan或设备凭据。新构建可能含已有Bridge分类/UI变更，应在部署记录中明确；0.18.85固件仍未部署。
