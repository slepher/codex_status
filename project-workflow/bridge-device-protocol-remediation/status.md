# P0 状态

[计划](plan.md) · [任务](task.md) · [审阅](review.md)

2026-11-02：P0 源码及软件验收完成，已提交、未部署、未刷机。

## 已完成

- P0-1：认证身份摘要/会话复用、首次 compact 握手；并发 status 不重开活动会话；设备重启后 status 可接纳新认证会话。确定 session 拒绝有界刷新；写 unknown 保留原业务事务。
- P0-2：同号同摘要的大只读响应可重执行，异摘要拒绝；写回执 512 B 守卫、原 HTTP 状态码回放；sync_begin 超限时仅裁重复诊断摘要。AP01 App 同步回执策略。
- P0-3：错误帧、主动残片超时检查、四槽轻量队列、帧错误不增加连接代次；Notify 唤醒与 150/300/500ms 兜底；3 秒冷却、连续失败 degraded、每轮重建 Manager/Adapter；watchdog 不再因 BLE 超时退出整个进程。

## 验证

- `cargo test --workspace --locked`：522 passed / 0 failed / 5 ignored（既有条件测试）。系统代理绕过仅用于 loopback Fake；[日志](../../artifacts/protocol-p0/workspace-tests.log)。
- 最后补充设备重启会话用例后，core/ble 全回归：245 passed / 0 failed / 2 ignored，含并发 status+data+plan、身份缓存、重启会话、大只读重执行、同号冲突及冷却/连续失败；[日志](../../artifacts/protocol-p0/core-ble-final.log)。上述计数有重叠，不能相加当独立测试数。
- `tools/test-protocol-p0.py`：生产 BLE 重组函数的超长/残片超时/损坏头、四槽队列满、未认证拒绝、合法帧保留及操作分类通过；宿主模型不等同 GATT/RF。
- `tools/test-nimble-host-stop.py --out artifacts/protocol-p0`：2000 轮通过。构建脚本识别新版 NimBLE 上游已经修复的定时器清理实现，未知实现仍拒绝。
- AP01 App application.cpp 宿主对象编译、`tools/test-ap01-firmware-boundary.py` 通过；未修改兄弟固件仓库。
- Note4 `pio run -e zectrix-note4-b` 成功；匹配 ELF 的 `check-wifi-stack.py --no-online-wifi-migration --no-legacy-management-entry` **88 项通过**。[构建](../../artifacts/protocol-p0/firmware-build.log)、[栈](../../artifacts/protocol-p0/wifi-stack.log)。
- `serviceV2Ble` 栈帧 960 B / 1024 B；所查局部调用链 2784 B / 4096 B。`v2HttpBegin` 栈帧 416 B。局部静态检查不包含所有间接/平台调用，不代表实机任务高水位。
- `git diff --check` 通过；既有 UI/托盘/配对工作区修改保留。

## 候选与工具链

仅候选，**版本字符串与现场相同，必须以哈希区分，不可认作已部署**：

- env `zectrix-note4-b`；marker `codex-status-ota-v1|zectrix-note4-400x300|0.18.106-note4`。
- ROM：`artifacts/protocol-p0/note4-p0-0.18.106.bin`，1,872,736 B；SHA256 `1b93bf34f4139c21d322fb0a8a3c93d9333503fb056663c4771f8a07bfaeb864`。
- 匹配 ELF：`artifacts/protocol-p0/note4-p0-0.18.106.elf`；复制后已逐文件 SHA256 核对。
- 静态 RAM 97,856 B / 327,680 B；Flash 程序 1,818,671 B / 6,225,920 B。
- 本机首次 ESP32 构建：项目隔离 Python 3.12.16 / PlatformIO 6.2.0；平台 55.3.312，Arduino 3.3.12 / IDF 5.5.5、ArduinoJson 7.4.3。嵌套安装的 6.2.0.post1 被平台版本约束拒绝，隔离 penv 改正式 6.2.0 后通过；未伪造 SDK 指纹。此工具链区别于历史部署，ROM 不作部署授权。

## 剩余边界

真实 GATT Notify、Windows/macOS 驱动超时后回收、四并发动态 RAM 峰值/任务栈高水位、RF/掉电及长期恢复需实机验收；关联 backlog A9/B6/C8 不结项。P1/P2 未实施。资源估算、设计差异与审阅见 [review](review.md)。
