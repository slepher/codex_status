# P1 状态

2026-11-06：P0修正已提交 `3315c05`；P1已提交 `3cf9061`，未部署、未刷机；后续推进见[P2状态](p2-status.md)。

## 实现

- P1-1：HTTP Status/Wi-Fi快照及BLE摘要共享UTF-8原文分页，按JSON转义后帧预算裁切；BLE紧凑标识直接传字符串。无旧Hex/Base64字符串兼容路径，二进制诊断编码保留。
- P1-2：删除ReportSession和session/report两段上报，统一认证presence与反向Status事实确认；保留来源IP、登记端口、MAC、boot、owner校验。固件主循环只提交/回收单个工作项，HTTP由独立按需任务执行，迟到结果核对原会合和期限。
- P1-3：HTTP只解析响应头；每地址最多一个5秒空闲连接，同runtime内复用。取消/错误丢弃、不重发未知写，服务端close重新连接；重复Content-Length与Transfer-Encoding拒绝。当前Note4 WebServer仍主动close，不宣称实机连接复用收益。

## 验证

- [HTTP客户端](../../artifacts/protocol-p1/http-tests.log)：14通过，含连接复用、close、丢ACK不重发和跨runtime回收，以及P0会话/预算回归。
- [BLE线格式](../../artifacts/protocol-p1/ble-wire-tests.log)：6通过，覆盖最大标识、511/512/513字节边界、控制字符拒绝和超限unknown。
- [presence本机完整闭环](../../artifacts/protocol-p1/presence-integration.log)：1通过；真实生产处理器+独立Fake进程，登记临时端口，不操作实机。
- [多页快照修订冲突](../../artifacts/protocol-p1/pages-tests.log)：1通过。AP01职责边界与P0宿主错误帧/四槽回归通过。
- `cargo test --workspace --locked -- --test-threads=1`：**542通过、0失败、5条件忽略**，含doc-tests；[完整日志](../../artifacts/protocol-p1/workspace-tests.log)。另显式运行上述presence集成1项通过；其他定向计数与全套重叠，不相加。`git diff --check`通过。

## Note4候选与资源

- env `zectrix-note4-b`；marker `codex-status-ota-v1|zectrix-note4-400x300|0.18.108-note4`。
- ROM `artifacts/protocol-p1/note4-p1-0.18.108.bin`：1870064B，SHA256 `ba637efe91fed8a4d40440156b723c6d54447ebf84e3971f0bb98bb824c49914`。
- ELF `artifacts/protocol-p1/note4-p1-0.18.108.elf`：SHA256 `2be7496190078306b6947888d71045aa165d9fbc6d1ca08e6c1c65fdbbc86c14`。
- [构建](../../artifacts/protocol-p1/firmware-build.log)通过；静态RAM97800B、Flash程序1815935B。[匹配ELF栈](../../artifacts/protocol-p1/wifi-stack.log)92项通过；presenceTask464B/局部链1104B，servicePresence576B/局部链1248B，serviceV2Ble960B/局部链2784B。
- 新按需任务栈6144B，单个动态工作项/HTTP缓冲按实际长度分配；NVS新增0。详细生命周期与预算局限见[审阅](p1-review.md)。

## 边界

软件验收不替代真实GATT/RF、原生驱动、网络失败期间按键/屏幕响应、任务高水位和功耗验收；A9/B6/C8不结项。候选与现场同版本名，以哈希区分。发布时须配套更新Bridge及设备；本次未启停已运行Bridge。P2后续状态单独登记。

## 本轮现场只读核对

运行进程21799/21804指向 `artifacts/bridge-macos/Codex Status Bridge.app/Contents/MacOS/bridge-app`，磁盘文件修改日期2026-11-05，81457936B，SHA256 `fc93068b4c8629e88bd984cadaf77703ec485678f6ba1ba07d32f3721eb9814d`。本轮P1测试使用 `/tmp/codex-ap01-bridge-v3-target`，没有替换上述应用。此磁盘快照与交接卡旧Mac产物记录不同，不据此推断历史部署经过；本轮未认证读取设备ROM。
