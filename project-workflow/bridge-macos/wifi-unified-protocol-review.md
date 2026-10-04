# 统一设备在线与HTTP/BLE控制协议：Astra审核

日期：2026-10-04。按用户要求仅做一次独立Astra审核，model=gpt-6-astra，reasoning_effort=medium；审核者只读，主代理根据结论修订设计。未改运行代码/设备，未刷机或提交。

审核输入：wifi-unified-protocol-design.md，SHA256 `71f46c18a80dd44540db696e9580a6f22d5820e346d4d1d1b330d6972fcda2a8`。下列原稿行号只用于追溯输入版本，修订后位置以章节为准。

结论：方向可行，建议先修订再实现。4类确定性修订，1项BOOT救援权限政策已由用户决定。

| 发现 | 原稿/源码依据 | 处理 |
|---|---|---|
| P1 同一BLE peer不能证明具体命名实例principal | 原稿125–127；main.cpp4677/2368，bridge_store.h4 | 第3节增加peer+该principal token双重证明；generation只防旧消息；旧namespace不自动共享给同peer的第二实例 |
| P1 激活完成回执/活动版本缺少原子边界，trial重启被误判中断 | 原稿163/167–169；wifi_scheme.h108，wifi_setup.cpp336–348 | 第4.1节定义pending/accepted/committed原子表、先预留回执容量；active与完成回执同提交；RAM trial无终态证据返回未知 |
| P1 异步任务仍可能被BLE断连取消，截止后恢复旧网又开RF | 原稿136/167；wifi_setup.cpp290–308 | 第4.2节以principal/request归属，断连仅影响投递；撤销/owner/cancel/deadline独立终止；恢复也受原期限 |
| P2 8身份关联与3 namespace/12配置容量混淆 | 原稿127/201；wifi_scheme.h95，bridge_store.cpp5 | 第7节分别保留8关联、3 namespace×4/总12、8回执上限；第4 namespace首次apply明拒，登记不先消耗配置容量 |
| P1 BOOT例外与统一owner政策冲突 | 原稿126/129；main.cpp4698/4707/4791/4773，wifi_setup.cpp217–218/383/464–479 | 已先弹出问题；用户明确选择严格权限，第3.1节已冻结，无claim/他人owner不能保存、试连或启用 |

现行BOOT核对：恢复窗允许已绑定主机即使无endpoint/他人owner也保存本bond，RAM试连只需物理窗且会切断旧Wi-Fi；正式activate要求endpoint/owner允许，legacy迁移还要求物理窗。不能把现行代码行为当作对通用owner规则的新授权，也不能泛化为HTTP/跨namespace旁路。

用户已答复：统一严格权限。BOOT允许合法绑定/身份修复，保存/试连/启用均先取得本机claim，他人占用时拒绝；不隐式转移owner、不扩展其他namespace、不把USB常驻等同物理授权。

主代理已直接修订第3/4/7/8节并补反例矩阵；BOOT条款已按用户答复修订。这里只完成设计修订及差异检查，不宣称实现、软件回归或实机验收完成。
