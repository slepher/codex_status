# 周期 activate 调度修复

用户报告仍每约3分钟activate，随后明确要求先验证新协议，若仍触发就在新框架修正，并直接部署。运行Bridge38B7CE11/.69旧协议ROM；主工作树含c97fd87精简协议及固件模块化。旧协议隔离worktree仅留作对照，不部署。

步骤：新协议下验证无HTTP工作、BLE小数据、真实HTTP工作收尾及后续联系；闲置连接状态不能独立授予Light，显式请求/真实HTTP工作/既发事务保留网络权限。补齐同一Fake网络报告闭环：真实非loopback私有接口、生产Bridge接收器及认证回读，不放宽源IP校验；Fake使用同源C++ Wi-Fi/Rendezvous和共享信封，Rust网络适配不冒充Arduino HTTPClient/RF。通过回归与匹配ELF栈门禁后备份data/exe/ROM，协调更新默认Bridge和Note4新协议ROM，核对MAC/运行hash，并观察至少两个原3分钟周期。无提交、不直接改生产state。

状态：复现、修正、同源验证、协调部署与运行hash验收完成（2026-10-08 22:23）；用户随后授权提交本次部署源码及必要的模块拆分依赖。

新协议负对照仍申请Light，确认为陈旧连接状态的调度实现bug。正向四轮同源Fake闭环无HTTP探测/新activate/RF增加；独立report→生产接收器→认证回读通过。另修正owner_id事实读取、独立SyncOpen与Sleep确认及OTA image参数从URL移到业务载荷，未放宽权限或事务合同。

Note4 .71 ROM DE7897DD…E6D31/1,898,240 B与认证运行镜像完全一致，OTA e70d4281单次上传/succeeded/image_verified；默认Bridge最终62E6C492…59C76C4，PID41500，计划任务启动，data/exe/配置已私有备份保留。只更新Note4；书桌屏ROM不动，paused_device_macs排除其自动HTTP/BLE，登记/Profile/凭据保留，恢复时移除此MAC并重启Bridge。

22:13确认收尾至22:23约596s，新增activate=0、Wi-Fi next_job_seq=168；书桌屏暂停后背景联系=0。实机有正常数据工作续发正式PowerPlan，因此不把该样本称为多轮纯空闲BLE测试。app75/core129/HTTP6/Fake57及3项显式同源联测、其他模块回归和匹配ELF栈门禁通过；实际RF/电池/掉电/动态RAM及macOS长期矩阵继续归backlog A9/B6。证据及完整ROM/ELF/Bridge SHA见PROGRESS最新节与artifacts/wifi-new-protocol-observation-20261008.json。
