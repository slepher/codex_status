# Bridge 审查修复（2026-10-20，B6/C8）

## Plan

1. HTTP事实采用本次认证boot身份；只在持久字段改变时写连接状态；异步激活后合并最新事实。
2. 所有BLE收尾统一先放单设备锁、在有界清理后再放无线电锁。
3. 独立监督服务任务和native隔离，捕获任务退出/panic，沿既有受管重启及退避恢复。
4. 历史清账耗尽的终态任务原样原子归档到platform既有目录，读回核验后腾出活动队列；保持unknown/原身份，提供历史读取和显式原事务清账重试。不把retired/unknown当未提交。
5. 红绿回归、相关Rust/设备页验证；按既有授权部署Mac并验自然会合，固件不变。

## Task / status

- [x] 针对状态/锁/监督/历史容量建立回归（两项旧实现red）
- [x] 最小实现与相关测试
- [x] 构建/部署及同MAC认证HTTP核验（当前HTTP正常，普通BLE按合同抑制）
- [x] 更新交接与验收边界

## Review

不新增C1兼容、不放宽认证/owner/事务围栏，不改BOOT选网/ROM。未知历史不可删证据或重发原写；归档先于状态移除、失败保留队列，重复归档可恢复。归档查询有界、逐文件读取，敏感内容仅本地0600；不把宿主回归当主机休眠同机验收。原工作区交接改动保留，未经要求不提交。

## 实施与验证

- 两项旧实现red：RAM-only联系更新更换state文件；32条清账耗尽unknown使新作业wifi_job_capacity。首批green通过，日志review-fixes-20261020-{red,core}.log。
- http_ready显式接收本次认证响应与采样时间；所有HTTP入口同步更新。连接setter比较实际序列化的持久字段，仅RAM事实变化不写盘，持久失败回滚。激活后只合并最新事实中的退避意图，不覆盖交换期间的boot失效处理。
- close_device_ble供会合及Wi-Fi维护共同使用：释放per-device → bounded disconnect → 释放radio；更新原挂起清理回归，证明HTTP继续、无线电互斥及有界归还。
- supervise_services独立监督run_services JoinHandle与native隔离，工作循环pending时也检测；退出/panic仅记录固定错误分类，沿原受管75退出码/退避先退出旧子进程再拉起，未记录panic payload。单独监控任务退出与卡住后隔离的用例通过；没有实际让用户主机休眠，不能称同机合盖验收通过。
- 清账耗尽终态原样写platform/wifi-history-MAC-ID.json，schema1/UTF8 payload字节数/SHA256/MAC+ID+terminal校验，原子写+读回后才移出活动队列。状态提交失败保留原记录，重复文件由活动记录遮蔽；显式reconcile_job加载原ID只重置只读清账预算，后续proof同步写回归档。不把unknown/retired变成未提交。
- UI/MCP共用Wi-Fi view，最多投影32条归档历史；其他原ID可显式清账；全部证据仍在磁盘。Node设备页通过，WK74场景通过，560/1000网络失败截图已审阅；合成IPC不证明Rust/设备通信。
- 完整回归App119 passed/3既有条件ignored；Core180 passed，另集成测试3+7+7+9 passed/2既有ignored。日志review-fixes-20261020-suite.log。追加跨真实Fake交换的旧副本覆盖回归通过，最终review_三项全绿；初始失败来自Fake缺少有效窗口/owner，已补齐仅该用例的事实，未放宽生产准入。

## 持久与资源边界

本轮固件/NVS/设备RAM/设备栈增量均0，无ROM构建。Bridge归档：空0文件/0 B；实际运行10条Job仅按同格式离线估算，单条796–1501 B，中位1073 B，典型8条约8584 B，32条按样本最大约48032 B；这些是样本估算，不是任意输入上限。每文件读取前检查1 MiB上限，历史投影最多32 Job、目录候选最多33，逐条解析；不在内存加载全部历史。完整allocator/运行峰值RAM未测。磁盘证据不自动淘汰；空间或读回/提交失败明确报错并保留队列，不凭容量需求删除未知证据。持久只读加载state/旧格式不迁移；原回执/namespace/序号围栏保持。

## 部署与同机核验（10:33–10:35）

- 默认旧supervisor66982/child66993已按入口停止；私有备份artifacts/bridge-macos/review-fixes-20261020-backup，167文件/114757259 B逐文件SHA核验，含旧C2 app/data/installation-id。修复版启动前160个data文件逐一仍匹配备份。
- 当前工作区构建47.92s，本地签名包codesign --verify --deep --strict通过；exe78732896 B，SHA256 `850f0cc38752c11dfa5fefe60714b783354fe0f3c820714a5ba81b66a050568c`。无固件构建/刷写。构建日志review-fixes-20261020-build.log，部署清单review-fixes-20261020-deployment.json。
- 10:33:57受管启动supervisor71060/child71064；8765/8766监听，health204。最初沙箱内health请求无法连接，改在具本地网络权限的会话核验；未将该工具权限错误当Bridge故障。
- 10:34多次真实/api/status HTTP200，Plan5846 HTTP200；10:34:31完整认证缓存已推进，同MAC7C4FADB93408、.105-note4、同boot6ff58ad397029f745c2c917313928de6、bridge_reachable=true/error=null。连接requested=false/attempts0/next0/admission_error=null。白名单证据review-fixes-20261020-live-verification.json。
- 当前HTTP健康，automatic_ble=false，未为了验收人为制造第二轮BLE或延长设备电源窗口。设备屏幕未目视核验；真实主机合盖/苏醒与长期RF/跨平台矩阵仍待验，不能凭本轮故障注入宣称全部完成。BOOT旧网回退顺序不在本轮实现范围。
- 生产代码无DEBUG临时日志，未提交。归档保护、HTTP事实、清理锁序和监督修复已部署；B6/C8剩余实体边界继续开放。
