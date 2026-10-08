# 周期 activate 调度修复

2026-10-09 修复部署完成：Note4 .72 ROM095BAD00…45865 / 1,901,760 B与认证运行镜像一致，默认Bridge B770BA43…CA4B0由计划任务运行；完整Status/整体请求期限、独立Wi-Fi与Bridge错误、无Data网络变化重绘及失败有界重试已实施。同源大Status端到端与超预算拒绝通过，Fake58/app75/core132/HTTP7及3项显式生产联测通过，匹配ELF栈门禁通过（Status976 B/分页helper128 B，未扩大限制）。原b0事务正常completed，正式Bridge刷新成功、运行镜像记录已按完整发布长度更新。data/exe已备份，书桌屏暂停与登记保留，无提交。设备驱动确认可信帧基线，仍有局刷回退；目视/长期RF功耗/动态峰值及macOS原生UI矩阵继续在backlog，不因本次上线全部结项。现场、资源和证据见PROGRESS顶节。

2026-10-09 用户授权修复并部署：完整Status使用单一认证快照的有界分页，保持4096B帧上限，不裁剪字段合同；同源Fake覆盖超帧、错读ID/过期/跨principal与恢复。Bridge请求期限包含锁/开会话/全部分页，错误载荷明确失败；Wi-Fi事实与Bridge验证独立，网络状态变化单独绘图且失败有界重试。先回归、匹配Note4 ELF栈/资源门禁，再备份当前data/exe/ROM并仅部署Note4与默认Bridge，查询既发原事务、不自动换ID重发；核对运行镜像与真实Status、独立Wi-Fi图标链路。书桌屏仍暂停，不提交。

2026-10-09 追加现场调查：用户报告断开图标。保存只读Bridge/事务现场，不把图标当认证连接失败证明；00:05 ready报告后HTTP200却identity_mismatch，原b0事务未确认；400ms HTTP探测实际4625ms，源码发现会话Mutex等待不计入请求期限。先补有界锁等待及失败Status识别回归，再按新协议既有授权更新Bridge、查询原事务并采集真实错误载荷；不重发未知activate、不换配置、不清NVS。00:33:57真实认证帧已证实reply_limit（HTTP200/信封result3）；00:35精简Status成功且HTTP ready。USB原生监听前后boot不变，当前IP/RSSI正常，同时存在EPD局刷失败/全刷回退。core131及HTTP6回归通过；Bridge锁/错误处理修正尚未部署，固件完整Status超帧尚待修正，结论及证据见PROGRESS最新节。

用户报告仍每约3分钟activate，随后明确要求先验证新协议，若仍触发就在新框架修正，并直接部署。运行Bridge38B7CE11/.69旧协议ROM；主工作树含c97fd87精简协议及固件模块化。旧协议隔离worktree仅留作对照，不部署。

步骤：新协议下验证无HTTP工作、BLE小数据、真实HTTP工作收尾及后续联系；闲置连接状态不能独立授予Light，显式请求/真实HTTP工作/既发事务保留网络权限。补齐同一Fake网络报告闭环：真实非loopback私有接口、生产Bridge接收器及认证回读，不放宽源IP校验；Fake使用同源C++ Wi-Fi/Rendezvous和共享信封，Rust网络适配不冒充Arduino HTTPClient/RF。通过回归与匹配ELF栈门禁后备份data/exe/ROM，协调更新默认Bridge和Note4新协议ROM，核对MAC/运行hash，并观察至少两个原3分钟周期。无提交、不直接改生产state。

状态：复现、修正、同源验证、协调部署与运行hash验收完成（2026-10-08 22:23）；用户随后授权提交本次部署源码及必要的模块拆分依赖。

新协议负对照仍申请Light，确认为陈旧连接状态的调度实现bug。正向四轮同源Fake闭环无HTTP探测/新activate/RF增加；独立report→生产接收器→认证回读通过。另修正owner_id事实读取、独立SyncOpen与Sleep确认及OTA image参数从URL移到业务载荷，未放宽权限或事务合同。

Note4 .71 ROM DE7897DD…E6D31/1,898,240 B与认证运行镜像完全一致，OTA e70d4281单次上传/succeeded/image_verified；默认Bridge最终62E6C492…59C76C4，PID41500，计划任务启动，data/exe/配置已私有备份保留。只更新Note4；书桌屏ROM不动，paused_device_macs排除其自动HTTP/BLE，登记/Profile/凭据保留，恢复时移除此MAC并重启Bridge。

22:13确认收尾至22:23约596s，新增activate=0、Wi-Fi next_job_seq=168；书桌屏暂停后背景联系=0。实机有正常数据工作续发正式PowerPlan，因此不把该样本称为多轮纯空闲BLE测试。app75/core129/HTTP6/Fake57及3项显式同源联测、其他模块回归和匹配ELF栈门禁通过；实际RF/电池/掉电/动态RAM及macOS长期矩阵继续归backlog A9/B6。证据及完整ROM/ELF/Bridge SHA见PROGRESS最新节与artifacts/wifi-new-protocol-observation-20261008.json。


2026-10-09 用户补充状态合同：Wi-Fi连接状态与Bridge业务状态独立。Wi-Fi以设备驱动关联/IP事实及实际关网为准，Bridge认证失败、reply_limit、协议/回读失败、结果未知只影响Bridge层，不得写成Wi-Fi断开或据此自动换配置/重启关联。无网络时Bridge不可用，但仅Bridge失败不能反推Wi-Fi失败；电源窗口到期导致实际关网仍单独据驱动更新。固件Wi-Fi图标env.state经templateStateText/deviceStateText取wifiUp，pollWifi由WiFi.status更新；env.offlineMins属于Bridge失联标记，不能混称Wi-Fi图标。当前Bridge wifi.connection.state将bridge_unreachable塞入连接流程状态，是需要分层修正的模型问题；不因此断言固件已被HTTP错误直接断网。
