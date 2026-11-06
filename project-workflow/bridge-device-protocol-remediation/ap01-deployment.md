# 协议整改配套部署（2026-11-06）

## Plan
用户明确授权 AP01、Note4 和现有 macOS Bridge 配套更新。先备份与身份核验，设备更新后切换 Bridge，再验证运行身份、镜像与数据回执。AP01 只更新独立 App，不刷基础或救援 ROM。

## Task
- [x] 原运行配置/两台登记 MAC 核对；Bridge 应用和完整运行数据归档，115 项可读
- [x] AP01 .109 双基址一致、零未解析导入、集合资源校验
- [x] Note4 .109 构建、marker/大小/哈希及匹配 ELF 栈检查
- [x] 原事务升级、同 MAC 认证运行验证
- [x] 新 Bridge 签名校验、托管启动、health 204
- [x] 两设备 Data applied，状态 displayed

## Status
14:28（Asia/Shanghai）配套部署完成。

- AP01 34F01557210B：0.18.109-ap01，集合3 succeeded，runtime healthy/fault0，cleanup_pending=false；boot10898439199890799177。原事务 c4bf352cf83449dc98dd7051bb84241b。包205400 B SHA256 `708e9f382c92aa773a1067abe99efa146e72ba845ce47cca4090be2ce1554776`；运行App摘要 `703dcde304f42bf5d885933048e4f4ffe8f45822404d6e94bc7896176e949ffb`。ROM build仍677bc30246d7ae8f7cd73698fe4086d6、运行槽1；本次没有ROM槽写入，槽0救援基座未操作。
- Note4 7C4FADB93408：0.18.109-note4，原OTA dc9a95b1 单次上传 succeeded/image_verified。ROM `artifacts/ap01-protocol-deploy-20261106/note4-0.18.109.bin`，1865360 B SHA256 `82bd57deb431fb383a492decd152fc8958b28c25e0e90771df2b4776b1131673`，认证运行镜像逐项匹配。匹配ELF SHA256 `196a34f4bd65b80a7e23a342d790216532cd2a2ee75d16280b8b7501d7b98275`。
- Bridge：原 `artifacts/bridge-macos/Codex Status Bridge.app` 已替换，81787296 B SHA256 `582c711141e12b49d78ed201970f92faf819c124389604f3aee432bf29528e5c`，保留原data和CODEX_STATUS_CODEX，LaunchAgent托管启动。旧实例退出后复制并校验签名。
- Note4 BLE Plan/Data seq63 applied、sync-v1 complete；AP01 HTTP Data seq229 applied；随后两设备状态均displayed，原模板codex-status-a保留。

## Review
AP01首次安装查询遇到不含MAC的维护忙响应，CLI报identity mismatch；未换事务/未重发安装，之后只查询原事务确认succeeded及新boot/集合摘要。备份 `artifacts/ap01-protocol-deploy-20261106/bridge-before.tar.gz` SHA256 `2f270df89819db94911f7b5228b00a51c51cfc6543e6c619883db6ebe70d85c5`。证据均位于同目录evidence，含最终事务、认证版本、槽状态、Bridge回执、两设备显示状态。

验收限软件认证/回执及设备上报显示状态；未目视确认AP01原厂界面/旋钮，也未重复执行紧急恢复或长期功耗测试。Note4运行状态采样epd_busy_fails=1，不能凭该累计值归因为本次升级。未提交本轮改动。

## 14:50 Note4 Bridge off/on 调查
用户反馈部署后短暂off再on。托管Bridge PID77869仍running、never exited，非再次重启。设备sync-v1归档2026/2027（wake3）证明：11029ms bridge_off=0，430797ms、490819ms、550850ms、610775ms均wifi connected且bridge_off=1；第一次变化距Plan/Data约420秒。14:36:59 Plan9424/Data65 applied；14:47:18 sync-v1仍完成；14:47:24 Plan9425 HTTP超时；14:48:29 BLE claim applied，Plan9426 ACK丢失但后续BLE观测已接受9426（只认原事务观测，不重发）。
源码根因：display.cpp currentBridgeOfflineMinutes使用rtcLastSyncEpoch，config.h BRIDGE_LOST_MIN=6且整分钟>6才off；radio::markSynced仅Data/Plan路径调用，认证status/claim/诊断及成功presence未刷新该显示时间。省略重复Plan/Data后，正常通信不能维持显示新鲜度，约7分钟即误报off；后续Plan更新使其回on。不得以此时间刷新PowerPlan期限。独立存在HTTP Plan超时、sleep Plan ACK丢失，不能把它们当成430797ms开始误报的原因。此次只调查，未修改固件、未重启或重刷。
证据：私有运行data/platform/diagnostics/7C4FADB93408/2026-*.json、2027-*.json；本次evidence/note4-off-on-{connection,power,sync_diagnostics}.json；Bridge 2026-11-06日志。最终on的屏幕变化由用户观察，缓存原Plan确认支持恢复解释，未拿到相应新UI日志。

## Note4 在线误报修复计划
Plan：将显示用联系时间与Data/Plan同步时间分开；只有当前owner的认证HTTP/BLE联系或有效presence刷新。明确失败标志由之后的有效联系清除，不修改RF调度、同步数据或PowerPlan期限。
Task：实现、7分钟无Data/Plan回归、失败恢复/真实失联回归、Note4构建/栈校验、OTA认证核验。
Status：进行中；不需要更新AP01或Bridge。
Review：保持原事务和现有配置；新增RTC状态按实际大小记账，不新增NVS键。

结果：修复并部署0.18.110-note4，原OTA任务6e7be972单次上传，succeeded/image_verified；认证同MAC运行镜像SHA256匹配。AP01和Bridge未重启。

实现：BridgeContactState占8B RTC，无堆分配或NVS新增写入。HTTP完成帧/会话校验后、BLE完成绑定凭据及MAC校验后、成功且未过期的presence均经过当前owner检查后刷新。不同principal或无owner不刷新；显示失败状态独立于RF调度和错误报告；不修改Data同步时间或PowerPlan期限。冷启动清状态，深睡保留。

验证：生产联系函数通过owner隔离、30分钟无Data/Plan但持续联系、7分钟真实失联、失败恢复及同步时钟不变回归；metadata及200/400屏绘制通过。原显示测试脚本仅支持WSL，本机使用其生成的同份C++经clang编译运行通过。Note4 RAM97728B、Flash1811887B，匹配ELF栈检查通过。软件回归覆盖超时窗口，尚未完成修复后的整段7分钟实机持续观察。

ROM：`artifacts/ap01-protocol-deploy-20261106/note4-0.18.110.bin`，1865664B，SHA256 `60a2430dc4092de6c52f8a681843ac4b3773972de25f71abef1ec80818fdc474`。匹配ELF SHA256 `7d8cccc4c4303fc705c96ea6423c6f2af815738fb59d3e99f12c321175a8a3ef`。证据在同目录evidence/note4-110-*；未提交。
