# BOOT 网络与图标缺口修复

## Plan
修复调查R1–R6：Bridge失败与Wi-Fi关联分离，BOOT已有Light预算不被失败取消；presence回应与反向HTTP证明分离；图标只读Wi-Fi阶段，Bridge明确不可达立即显示OFF；boot_id与可轮换会话nonce分离；有限诊断保留按键/无线/图标事件。保留全部鉴权、重放围栏、固定期限和正常Sleep。运行Bridge/Fake不停止，不刷固件、不发布模板、不改生产data。

## Tasks
- 主agent实现最小共享路径修正，准备真实模板布局的未发布候选与协议说明。
- 增加真实策略/模板/诊断回归，覆盖BOOT到期边界、BLE开关不改变Wi-Fi图标、明确失败与待验证、session变化。
- Luna执行定向回归及隔离构建/匹配ELF检查；主agent独立审查差异与证据。

## Status
实现、Luna测试、主agent审查完成；未部署。既有绿灯候选包含在当前工作区，和本轮网络改动分开记账。

- Luna：Rust render 63项（13单元+50集成）全通过；BOOT/Wi-Fi恢复与元数据两组MSVC宿主检查、Python模板编码2项、固件模块检查通过。匹配最终ELF64项栈门禁通过；`git diff --check`通过。证据：`artifacts/boot-network-fix-20261011/results.md`及同目录日志。
- Note4最终增量构建确认重编wifi_setup.cpp，消除最后一次编辑与编译重叠的不确定性。候选 `artifacts/note4-0.18.83-boot-network-candidate.bin`，1,837,456 B，SHA256 `CA2C573985784D51AC5420755DA938DC0C7D48D8F68C0E6B0E4886835E87B1CE`；同名ELF，SHA256 `65A54745C1516C3357939EB074845C5B33AFE582FB130E84364898FB8C1DC8ED`。marker为`codex-status-ota-v1|zectrix-note4-400x300|0.18.83-note4`，仅本地候选，未递增版本，不等于当前已部署.83镜像。
- RAM118,916 B、Flash1,785,607 B；相对本轮前绿灯候选RAM净+40 B、Flash+600 B、BIN+624 B。新增固定boot ID 33 B、事件交接/已读记录各4 B，无新增NVS、任务、条目仓库或动态分配；空/单次/连续/上限复用同一状态。诊断仍用原有限环，多次断开合并为count+最后reason；动态堆/射频/EPD实测未验。
- 真实Bridge PID75952、Fake PID34328及8765/8766/18783监听已只读核对，未停止。生产data/模板未改；设备运行镜像保持原部署。

## Review
主agent已完成调用链审查：

- R1：`networkFailed`共同入口仅在真实Wi-Fi故障/无IP时断开；Bridge故障保留关联和HTTP服务，不改任何deadline。检查了presence、ready报告收尾、fallback、pollWifi及最终enterDeep：失败报告轮不递归重试，原电源策略仍负责最终关网。
- R2：实际v4与候选v5的所有非Wi-Fi元素完全相同。Wi-Fi区域仅绑定wifi_state/wifi_blink，BLE手动图标独立；新整张模板区域像素回归覆盖手动ON→OFF但自动BLE仍ON→实际BLE OFF。主动深睡阶段改idle，避免把主动关网画成失败OFF。后续用户要求取消device.state兼容，已在device-fields-only-20261011.md专项中删除；旧字段模板直接拒绝，不自动重写。
- R3/R4：明确网络错误立即使Bridge显示OFF；匹配principal的202/accepted算Bridge方向可达，缓存反向证明false不再误触发断网。这个回应不授予owner、投递或修改权限，业务仍按原认证/ACK判断。恢复后按既有联系新鲜度显示，不伪造Data联系时间。
- R5：所有固件status/presence/report的boot_id统一调用单份本次启动固定ID；BLE连接变化仍清空并重建session_nonce，原请求号/摘要重放围栏保留。HTTP会话失效仍需按原协议重开，稳定boot_id不等于取消会话失效。
- R6：保留BOOT、BLE会话、Wi-Fi链路、网络图标转换日志；不记录每一连接blink相位，防止挤满有限诊断环。驱动回调用4字节原子值交给主循环记录reason/count，GOT_IP不清该诊断值；主agent发现并修正了“从可被主循环清零的旧变量取reason”的竞态。多次事件可合并，只保证数量及最后原因，不冒充完整逐事件轨迹。

测试执行交Luna，初次recovery宿主编译缺initializer_list头，主agent补齐后已重跑通过。主agent复核原始日志纠正首次报告计数：Rust实际63项，不是68项。宿主模型不替代实体BOOT、RF和EPD验收，稳定boot_id跨实机BLE重连尚未实测。

模板候选 `artifacts/note4-template-network-fix-candidate.json`（codex-status-a v5，CRC eb628a93）须显式发布后才影响现有设备，未发布不声称现场修复。当前Fake进程也是此前基线，不因本轮源码修改自动更新。
