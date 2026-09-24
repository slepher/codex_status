# 唤醒会合诊断记录计划（2026-09-24）

## 目标

两台设备都以**一次物理唤醒为一条记录**。设备醒来时创建记录，在同一条记录内填入 BLE、Wi-Fi、命令与回复的阶段和耗时；再次入睡时收尾。若中途重启，保留最后已确认阶段，标记为未完成。Bridge 用同一设备 MAC、唤醒序号和请求 ID 对照自己的扫描、连接与投递日志，回答“这次通信停在哪一步”。本计划只定义取证，不预判故障根因。

## 记录边界

- 深睡 timer、按键、上电各生成一条 wake record。仅更新时钟的薄唤醒也记录，但标记 `thin`，不虚构通信失败。
- 一次唤醒内 BLE 会合后若获 light 计划并转 Wi-Fi，仍使用**同一条记录**；`transport` 标记实际经过的通道。若 light 持续很久，只记录首次 Bridge 联系及其结果，避免把后续周期推送混入这次唤醒。
- Bridge 的一次扫描可能覆盖两台设备；以目标 MAC 区分设备。已建立认证连接后交换 `wake_seq`；命令已有的 `request_id` 继续用于匹配命令和 ACK。不把扫描候选广播当作已认证身份。未建立连接时只能按 MAC 候选与时间窗口关联，明确标记为“未确认”，不声称精确匹配。

## 设备侧单条记录

固定长二进制结构，目标 `sizeof <= 48 B`，不存字符串、token、完整 MAC 或 JSON。实现时用 `static_assert` 固定大小。字段分组如下：

| 组 | 内容 |
|---|---|
| 身份 | `wake_seq`；设备 MAC 由设备身份提供；收到命令后记 `request_id` 的短校验值，仅作检索提示，最终仍以 Bridge 原 ID 核验 |
| 唤醒 | wake cause、EXT1 GPIO 位图摘要、计划唤醒时刻或 RTC 时间、实际启动时间来源 |
| 阶段 | 本次经过的最远阶段及各关键阶段相对唤醒的毫秒数：无线启动、BLE 广播、连接、命令收到、回复就绪、BLE 关闭、Wi-Fi 有 IP、首个 HTTP 结果；未经过用哨兵值 |
| 结果 | `thin / answered / advertising_start_failed / no_connection / no_command / wifi_no_ip / http_failed / interrupted` 等设备可判定的互斥终态；原始错误码与总清醒时长 |

阶段时间只在状态真实发生处写入。设备能证明“回复已交给本地 GATT 层”，不能自行证明 Bridge 已读到 ACK；该结论要用 Bridge 日志补齐。时间用本次启动的单调毫秒，绝对时间仅作辅助，因为深睡后启动计时器归零且时钟可能尚未校准。每次阶段更新后维护有效性标记/校验，避免把中断写入的半条记录误判为完整结果。

## 存储与读取

- 复用现有 RTC `/history` 环形缓冲，**替换事件逐条记录**，不并存第二个 RTC 日志。首版定为 64 条 × 最多 48 B = 最多 3,072 B；旧环为 120 × 16 B = 1,920 B，净增最多 1,152 B。当前构建的 7,680 B RTC SLOW 链接区中，Note4 占 2,764 B、1.54 占 2,312 B；按上限替换后约占 3,916/3,464 B。实际改动后必须以两种构建的 map 重新核对。
- `/history` 保持 `since=<seq>` 增量读取语义，返回记录版本与统一的一行一唤醒 JSON。`/status.json` 给出最新序号、最早可读序号和覆盖计数，读取不改变休眠计划或 owner。
- 现有 `tools/estimate-power.mjs` 依赖旧 `HIST_WAKE`，实施时同步改为读新版的 `wake_type` 与 `awake_ms`。历史诊断文档注明格式版本；不把旧 ROM 的历史解释成新版格式。
- RTC 断电会清空；本轮不把每个阶段写入 Flash/NVS，以免磨损和干扰会合时序。若实测必须跨断电保存，再单独设计失败摘要持久化。

## Bridge 对照与判读

Bridge 继续使用现有阶段日志，补充已认证会合的 `wake_seq`、完整 `request_id` 和结果。一次失败输出一行汇总：`device_mac, wake_seq, wake_cause, device_last_stage, bridge_last_stage, stage_durations, result`。判读只按双方最后**确证**节点：

| 设备记录 | Bridge 记录 | 可确定的断点 |
|---|---|---|
| 已广播 | 无对应候选 | 广播到扫描之间；需空口证据再分 RF 与 Windows 扫描 |
| 已连接 | 已连接但未见命令 | GATT 服务/写入路径 |
| 已收到命令并提交回复 | 等 ACK 超时 | 设备 GATT 回复到 Bridge 读取之间 |
| Wi-Fi 已获 IP | HTTP 超时 | IP 之后的 TCP/HTTP 路径 |

## 实施顺序与验收

1. 固件：两种 target 共用 record 格式；以现有 `/history` 替换式实现。验证断电清空、深睡保留、中途复位标 `interrupted`、环覆盖和单调阶段顺序。两种 PlatformIO 构建及 map 容量都通过。
2. Bridge 与取证工具：认证后关联 `wake_seq`/`request_id`，更新功耗脚本；同一唤醒跨 BLE→Wi-Fi 在导出中仍只有一行。扫描未认证的关联必须显示不确定性。
3. 实机：两台设备各采集至少 30 次 timer 会合、一次按键唤醒与一次故意关闭 Bridge 的超时；抽查设备记录和 Bridge 阶段日志可重建完整时间线。先不启用高频串口、JTAG 或抓包；只有断点仍落在空口/系统栈之间时，再短时增加相应抓包。

本计划不包含 OTA、刷机或修改运行中的 Bridge；实施和部署另行记录 ROM、SHA256 与实测结果。

## 实施决策（2026-09-24）

- 分成固件记录与 Bridge 对照两项实现，共用同一字段合同；固件代码待当前 1.54 Bundle OOM 修复结束后接手 `src/main.cpp`，避免并发编辑。
- RTC 断电后 `wake_seq` 会重置，因此每次 RTC 全新初始化还生成 `wake_generation`；深睡保留它。确定关联键为已认证 MAC + `wake_generation` + `wake_seq`。`request_id` 保留完整值在 Bridge 日志中，设备只存短校验值。
- `/history` 保持增量读取和数组外形，每行含 `format:2`、`wake_generation`、`seq`、`wake_type`、`awake_ms`、`result` 与阶段/耗时。`/status.json` 和 BLE INFO、v2 ACK 同步暴露当前 `wake_generation` 与 `wake_seq`。没有代际字段的旧 ROM 仅可做不完整关联。
- 新版 ROM 统一使用数字版本 `0.18.22`，设备后缀分别为 `-note4-b` 和 `-bw`；两台都要构建并核对 RTC map、SHA256。此次任务先交付 ROM 文件与 Bridge 代码，实机三十次会合验收在安装后执行并记录；不把构建通过写成实机通过。

## Bridge 端持久化补充决策（2026-09-24）

- Bridge 仅在认证 MAC 后读取设备历史。每台设备第一次连接立即启动一轮同步；一轮完整成功后至少间隔 15 分钟才启动下一轮。上轮未取完时，下次会合只续取未落盘的序号；已保存记录不重复传。
- 正常 `status`、数据与 PowerPlan 优先。PowerPlan ACK 后用剩余 BLE 时间分页请求 `history(since, limit)`，同步错误只影响诊断游标，不使原本成功的会合失败。旧 ROM 不支持时跳过并记一次能力状态。
- 设备 BLE 返回已收尾记录，也返回 CRC 有效的当前唤醒快照（`complete:false`）。Bridge 按 MAC + `wake_generation` + `seq` 把记录写入 PC 的 `data/platform` JSONL；同一醒次后续快照如有变化可追加修订，最终入睡时以 `complete:true` 保存终版。只有终版推进已完成游标；重启从文件恢复游标和最近快照。RTC 环覆盖产生的序号缺口单独记录，不制造缺失的阶段。
- 这能保存已成功交付给 Bridge 的记录。设备在尚未连接 Bridge 前断电，或 RTC 环已覆盖而 Bridge 从未读到时，丢失部分无法恢复；Bridge 报告相应缺口。
