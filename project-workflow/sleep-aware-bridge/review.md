# 休眠设备状态与延后操作：实施复核

日期：2026-09-26。结果：两台目标 ROM 已按用户授权顺序发布并在实机认证状态看到新版本；两台模板发布成功。主路径已验，故障矩阵与精确镜像身份证明仍待做。

## 已核实

已阅读 AGENTS、PROGRESS 最新现场、backlog 对应条目、v2 权威设计相关章节及 power-state 历史身份/占用/延后操作章节。逐处查看了 main.rs 的 runtime 初始化/状态读取/OTA 入队和 flush，platform.rs 的发布入口，PlatformService 持久化及重启恢复，Coordinator 替换/取消，MCP OTA 验证逻辑和固件认证/上传路径。来源位置列在 design.md §1。

Luna 调查的关键判断成立：runtime 缺失是 Bridge bug；完整设备缓存没有持久化；发布已有持久任务；OTA 内存队列和全局 selected_mac flush 有缺口。补充发现：flush 传 mac 而底层解析 device_mac；底层确认只看版本变化，必须与未来精确 ROM 身份能力分开。持久 observed 摘要已经存在，因此设计明确扩充现有模型，而不是另造完整状态系统。

## 合同决策

1. Q1：同类未终态只留一个；再次提交冲突，先取消确定未开始的任务再提交。`Unknown`/传输中不可覆盖；取消不伪装回滚。
2. Q2：每 MAC 可同时排 OTA 与发布各一项，按创建时间串行，同秒 OTA 先；新任务不取消旧任务。Bridge 对 HTTP/BLE 业务写使用每 MAC 锁。
3. Q3：`UPDATE OK` 与预期版本只给分级证据；缺运行镜像摘要时 OTA 留在 `awaiting_confirmation`，不自动重刷。同版本或前版本未知记 `version_seen_unproven`。

## 实施与证据

- 登记设备按需创建 runtime；认证 HTTP 快照与最近尝试分开持久，BLE 联系另记时间，旧公开状态仅作近期补充。`get_device_status` 与 MCP `platform_overview` 读同一 PlatformService 记录。正常深睡显示等待/预计休眠，401/错 MAC 显示阻塞。
- 模板发布冻结后等待认证会合，重启 `Sending → Unknown`，先读 `committed_job_id` 对账。OTA 需嵌入 `codex-status-ota-v1|target|version` marker，冻结 SHA256/大小到平台目录，落盘才返回 queued；MCP 提供按 MAC 查询/取消。上传前重验镜像与目标，设备 `/doUpdate` 再核 `target`。旧的全局内存 OTA flush 已删除。
- 定向验证：core 平台服务 20 项、app 38 项、mcp 2 项通过；UI 内联 JS `node --check`、`git diff --check` 通过。MCP `platform_overview` 返回两台登记设备，`firmware_ota_status` 返回空任务；桥监听 8765/8766，数据保留设备 2、模板 3、族配置 2。
- Note4 `pio run -d <repo> -e zectrix-note4-b` 成功；ROM `0.18.24-note4-b`、1,759,040 B、SHA256 `7B123BA616D3E75E38BFF2D8EBD9F3C945B2FF25CB9BCF4FC2B256C83E3A832D`。本轮获用户明确授权后，1.54 `tools/pio-target.ps1 -Target 154g` 成功；ROM `0.18.24-bw`、1,740,832 B、SHA256 `55B8BC9421CE1CA2DE981982A37318271B25A99DAF866F824B116C9FF1B30D78`。两份二进制均含对应目标/版本 marker；154g 头为 8 MB / 40 MHz。最终桥 exe 33,591,808 B，SHA256 `18099053D269073F4D708C2CE3D4F26EA65051466A0A90547F5B9B78A81C6BEF`；主进程 16692 / watchdog 30368，监听 8765/8766。

## 2026-09-26 实机结果

- 顺序 OTA：Note4 `a0959afd`（MAC `7C4FADB93408`）与 1.54 `584b67f5`（MAC `70041DD7A340`）各尝试上传 1 次、均收到 `UPDATE OK`；随后同 MAC 的认证状态分别报告 `fw=0.18.24-note4-b`、`fw=0.18.24-bw`。上传前旧 ROM 没有认证 `fw`，所以两任务均为 `awaiting_confirmation` / `version_seen_unproven`，没有重复上传，也不声称精确镜像验证。
- 实机发现 OTA 待精确确认会永久阻挡同 MAC 后续发布。现改为仅在上传 ACK + 上传后同 MAC 认证见预期版本时允许其他显式业务继续；OTA 本身仍不能再排或自动重刷。定向 core 测试通过，Bridge 重建后两台模板发布均成功：Note4 `e48f608e`，1.54 `98d2c0b9`，认证状态各自回报同一 `committed_job_id`；1.54 显示 `displayed`。
- 1.54 再排 `776b2f66`，在 `waiting` 时重启 Bridge；重启后任务仍等待，随后自然会合提交为 `succeeded`，认证 `committed_job_id=776b2f66`。运行数据仍有设备 2、模板 3、族配置 2。
- Note4 显式 light 计划 `209` 经 BLE ACK 接受 600 秒；显式 sleep 计划 `210` 收到设备 `applied / remaining=0`。实机发现该 HTTP sleep ACK 未记入 Bridge 的 last_accepted；已修正并重建。最终又以显式 sleep 计划 `214` 实测 HTTP ACK `applied / 0`，`power_view_v2` 同步显示 last_sent/last_accepted 均为 `214`、剩余 0。Note4 的 `display_state` 在认证快照中有 `failed` 与 `displayed` 交替；用户实物观察为正常休眠画面。该短暂渲染报告异常未归因。
- 错误 endpoint token 的只读 `/v2/status` 探针已限时 50 秒尝试 17 次，设备在该窗口未返回 HTTP，故 401 **未获实机结论**；探针没有写业务。随后的 sleep `214` 已 ACK，临时 light 请求已结束。
- 追加验证：core OTA 状态门槛定向测试 1 项、app 38 项、`git diff --check` 通过；最终 Bridge 运行数据和监听核对通过。

## 一致性检查

- 最后成功时间与轮询时间分离；旧 observed.last_acked_at 不伪装成整页采样时间。
- 原子入队与认证执行分离；MAC/ROM 身份不随 UI 选择或磁盘源文件漂移。
- queued 不等于完成；上传 ACK 不等于重启确认；模板 applied 不等于 displayed。
- ACK 不明先对账；取消不回滚；重启不复活旧认证会话或 light 承诺。
- 使用现有 v2 PowerPlan/claim/token 合同；历史 pending/隐式保活文字不覆盖 v2。
- 未新增待办事实来源，未把候选 bridge_first/运行镜像摘要能力写成现有功能。

未验：错 IP/401/409 的端到端故障注入、长期 PowerPlan 截止期、OTA 运行镜像精确摘要，以及 Note4 间歇 `display_state=failed` 的设备日志/画面归因。`task.md` S5 保留这些现场门槛；本次自然会合主路径已验，不把版本观察冒充精确镜像证明。
