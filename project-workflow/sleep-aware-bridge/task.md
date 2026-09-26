# 休眠设备状态与延后操作：实施任务

状态：S1–S4 桥侧与 Note4 源码已实施，S5 自动化/重启核查完成、实机会合仍待验，2026-09-26。总待办与优先级以 `docs/roadmap/backlog.md` 为准；本文件只展开该项的执行步骤。合同见 [design.md](design.md)。

## 分步任务

- [x] S1：已登记设备补 runtime；持久完整 HTTP 认证快照、BLE 联系时间与最近尝试；UI/MCP 本地读取并区分正常休眠/阻塞。旧 ROM 不提供 `fw` 的认证字段，须等新 ROM 上机才有该值。
- [x] S2：Q1/Q2 已定；OTA 镜像冻结/校验、每 MAC 持久 job、request_id 幂等、查询与取消；移除错误字符串猜睡眠的内存队列，统一 mac/device_mac。
- [x] S3：每 MAC 锁与认证会合、PowerPlan/claim/窗口门槛、副本重验后交付；全局 OTA flush 已删除；发布重启 Sending 进 Unknown 待认证对账。
- [x] S4：Q3 已定；上传 ACK/版本观察分离，缺精确镜像身份时持续待确认且不盲重刷。认证状态增加 `fw`；精确 `image_verified` 不可从现有 ROM/整文件 SHA256 推出。
- [ ] S5：core/app/mcp 定向测试、UI 脚本、两份目标 ROM 构建、Bridge 重建与监听/数据核查已完成。2026-09-26 两台真实设备各一次自然会合 OTA 收到上传 ACK，随后同 MAC 认证见预期新版本；两台模板发布成功，1.54 另验证 waiting job 跨 Bridge 重启后自然交付。显式 Note4 light/sleep PowerPlan ACK 已观察。剩余：精确运行镜像证明、错 IP/401/409 端到端注入、长期电源截止期；Note4 间歇 `display_state=failed` 待日志/实物复核。见 review.md。

## 最小自动化验收矩阵

| 场景 | 输入/故障注入 | 必须观察到 |
|---|---|---|
| 休眠与 runtime 缺口 | 平台有两台设备，registry 空；重复 UI/MCP 读取 | 返回登记身份和缓存/无记录提示，不报 no runtime record，不自动联网 |
| 最近成功 vs 尝试 | 成功 t1，超时 t2、401 t3 | 值/observed_at 仍为 t1；尝试/阻塞原因独立 |
| 局部认证数据 | BLE ACK 只带电源字段 | 只更新对应字段采样时间，旧固件/电量时间不变 |
| 重启且持续睡眠 | 两台快照和 queued 任务写盘后终止进程 | 重启保留 MAC/job/hash/时间；可达性不继承在线 |
| 两设备交错 | A=`70041DD7A340`，B=`7C4FADB93408`；选择切到 B，连续 100 次 B 会合 | A 任务不投 B；B 按自己任务执行；各自认证失败互不污染 |
| 镜像冻结 | 入队后修改/删除源文件；另一次损坏冻结副本 | 源变化不影响任务；副本损坏拒绝上传，给本地错误 |
| 落盘失败/崩溃点 | 副本落盘前后、任务写入前后、传输开始前后断进程 | 无虚假 accepted，无半文件引用；未知传输先对账；活跃副本不被清理 |
| 参数/身份/权限 | 无 MAC 多设备、mac/device_mac、错目标、IP换到其他 MAC、401、409、yielded | 明确拒绝或阻塞，0 次错误设备业务写，0 次强占 |
| 发布 ACK 丢失 | 设备已提交同 job，Bridge 未收到 ACK | 认证读取 committed_job_id 后成功，不重建任务、不重复激活 |
| OTA 上传后断链 | UPDATE OK 丢失/重启延迟>60s/新 IP/同版本 | awaiting_confirmation，0 次盲目重刷；按 Q3 报确认等级 |
| 排队/替换/取消 | queued、transferring、unknown 各状态操作 | 按定案 Q1/Q2；不能覆盖 unknown，不能把已提交称作取消成功 |
| 电源不变量 | 读取、claim续约、传输失败、窗口不足、timer wake | 无正式新计划则 deadline 增量=0；timer 不获 BOOT 300s |
| 数据/模板边界 | save、pull-only、ACK 前源继续变化、8 项 Profile | save 零发布；pull-only 不续期；仅 ACK 当包被确认；8项完整保留 |
| 升级旧 state | 无新增快照/OTA字段，已有设备、Profile、Bundle、seq | 默认读取不丢原记录；时间未知如实显示；不恢复不存在的内存任务 |

## 验证顺序与现场规则

先跑变更涉及的 core/app/mcp 定向单测、集成假设备测试和 UI 内联 JS 检查，再 `git diff --check`。新测试/构建目录按 AGENTS 提权预建，不清理现有 target/debug/data。无需为文档变更运行构建。

实机 A/B 使用登记记录读出当前 IP，不照抄历史地址；两台都应覆盖「睡眠入队 → 桥重启 → 自然会合 → 正确目标确认」。本次用户明确授权 Note4 与 1.54 两份 ROM 顺序发布；1.54 只用 `tools/pio-target.ps1 -Target 154g` 构建标准 B/W 目标，两个 pio 进程不得并行。发布前分别记录版本、大小、SHA256、marker 与目标 MAC；真实 OTA、发布和重启按本次授权执行。

每格记录 job_id、MAC、阶段时间、认证/ACK/确认摘要、deadline 前后值及工具输出，不记录 token。失败必须保留阶段和可复现输入；通过不等于功耗长期基线已完成。
