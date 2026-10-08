# Slate 协议参考：可借鉴内容与适用边界

核查日期：2026-10-08。对象：同父目录 `../slate` 当前本地工作树；读取到的 HEAD 为 `45f8078027c0ca937961e064ca602e5ea9e407a8`，不以此保证工作树没有未提交差异。本轮只读 slate，未运行其测试或操作设备。下述为参考建议，不是 codex_status 新增已实现协议。

## 1. 结论与模型差异

**值得借鉴的是协议合同集中定义、错误统一、状态/变化分层，以及通信资源生命周期。Slate 不能直接提供本项目 HTTP/BLE 交汇的实现样板。**

| 项目 | slate | codex_status |
|---|---|---|
| 设备通信方向 | 设备主动 poll 后端，按 manifest 拉取图片/音频 | Bridge 向设备推 Data/Plan/Bundle，设备低功耗会合及管理通信 |
| 显示内容 | 后端渲染的 400×300 packed 图像及音频 | 设备执行 CompiledTemplate，本地渲染当前 fields |
| 所有权 | 服务端用户绑定设备及选择内容组 | Bridge 显式 claim/lease，占用与 PowerPlan 分开 |
| 载体分工 | 内容同步使用 HTTP；小智语音另外使用 MQTT/WebSocket（MQTT 路径还承接 UDP 音频） | 设备业务 HTTP/BLE 适配与下层交汇/换接 |
| 提交 | 内容缓存先暂存、逐帧交换文件 | 完整 A/B Bundle、上下文条件提交、事务回执 |

“Slate MQTT/WebSocket 可换”只证明其**语音子系统**具有共用协议入口，不能推广为设备内容同步已有多载体实现，或已验证先 HTTP ready 后关闭 BLE。

## 2. 最值得借鉴的五项

| 参考内容 | 源码证据 | 对本项目的具体价值 | 边界 |
|---|---|---|---|
| 集中协议 schema 与命名 DTO | [device.ts](../../slate/shared/src/types/device.ts)、[content.ts](../../slate/shared/src/types/content.ts)、[shared README](../../slate/shared/README.md) | 为 Status/Data/Plan/ACK/Wi-Fi 等分别固定字段、必需性、范围与缺失语义；用同一合同校对 HTTP/BLE 投影，减少文档与 wire 漂移 | TS schema 前后端共享，固件仍独立 C++ 解析；不是三端自动生成或验证全部一致的证据。无须引入 zod 到本项目 |
| 一份稳定错误响应和追踪关联 | [ApiErrorEnvelope](../../slate/shared/src/api.ts)、[AppExceptionFilter](../../slate/backend/src/common/nest/filters/app-exception.filter.ts) | 统一有限错误码、可操作说明和关联信息；将入口参数失败、认证、冲突、容量与未知结果逐项对照，不只列成功 ACK | 本项目必须附失败阶段与“是否可能已执行”，不能只复制 error/message；HTTP trace requestId 不等于持久幂等 request_id |
| 多层变化摘要 | [group-etag.ts](../../slate/backend/src/modules/groups/group-etag.ts)、[device schema](../../slate/shared/src/types/device.ts)、[ETag helper](../../slate/backend/src/common/utils/etag.ts) | 分清“结构/安装内容变化”“当前数据变化”“实际显示结果”，避免一次无关变化触发整包传送或重复绘屏；可作为未来 C1/C5 内容摘要设计参考 | 当前 context、data_seq、CRC、commit_seq 已有各自语义；不为对齐命名另造一套etag。内容摘要不是授权、事务回执或OTA运行证明 |
| 根据执行场景选择同步范围 | [sync_poll.cc](../../slate/firmware/main/sync/sync_poll.cc) | SyncBackground/SyncUserActive 明确分开，定时唤醒可以只处理当前内容；很适合本项目“场景规则独立于 wire”的表达方式 | 本项目仍以设备 data_contract 决定字段，不改成后台只拉图片，不绕过完整 Data 原子快照 |
| 一次工作机会内管理 HTTP 资源 | [ApiClient](../../slate/firmware/main/sync/api_client.h)、[sync_service.cc](../../slate/firmware/main/sync/sync_service.cc) | 单一 client 入口、请求串行化、异常连接丢弃、一次 sync 工作结束 ResetConnection；本项目可对照现有机会预算与 BLE close 是否完整收尾 | 借鉴生命周期，不能照搬每请求重新计时；实际 socket/TLS 握手收益需测量，client handle 保留不自动证明 TCP连接复用 |

优先价值在前两项：它们能帮助对齐本项目“任何请求必须明确返回”的合同，并发现 wire 投影差异。后面三项可用于检查现有机制及未来资产方案，不表示应立即修改协议。

## 3. 高层逻辑与载体的另一种参考

小智的 [Protocol](../../slate/firmware/main/xiaozhi/protocol/protocol.h) 提供共用业务动作 `SendStartListening/SendStopListening/SendAbortSpeaking/SendMcpMessage`；MQTT 与 WebSocket 负责底层 Start/Open/Close/SendText/SendAudio。

可借鉴其**责任边界**：高层说“开始某业务”，载体负责发什么和如何建立通道。连接事件与 `audio_channel_ready` 分开，也支持本项目区分本地准备、认证可用及换接完成。

不建议直接复制整个虚基类、回调/队列体系到设备显示业务：本项目已有共享纯决策函数、HTTP/GATT adapter、RendezvousProtocol 和应用服务。只要这些职责清楚，不需要为了两个载体再叠一层通用框架。Slate 的 Protocol 命名也同时含业务动作与通道生命周期，不能照它的文件边界替代本项目已确定的“业务合同 / 下层执行 / wire”三分层。

## 4. 条件读取与缓存：借鉴规则，保持现行合同

Slate 使用 `ETag/If-None-Match/304`；响应标记 `Cache-Control: private, must-revalidate`。固件通过 manifest 与单资源摘要减少下载，未预期的图片/音频304在暂存流程中被视为不能完成该更新。

有用的规则是：**只有已有可用缓存且身份/版本仍匹配时，未变化响应才能复用缓存**。304不返回新内容，不能将缺失本地缓存当完成。对应本项目可校对 BLE 复用 data_contract 的 context 约束、已确认数据指纹与完整快照有效性。

但不恢复本项目已删除的旧 GET usage/template/304 业务路径。未来资产增量读取是否采用 HTTP ETag，由 C1/C5 定稿和设备能力协商决定，不能因 slate 已实现就新增未经支持的设备端点。

## 5. 不宜照搬的四处

### 5.1 注册/占用语义

Slate [registerOrReset](../../slate/backend/src/modules/devices/device-firmware.service.ts) 允许同 MAC 无鉴权注册重置归属并轮换 secret；源码明确记录这是可用性取舍及其 MAC 可伪造风险。固件连续401会触发 secret 失效处理，[app.cc](../../slate/firmware/main/app/app.cc) 清 secret 并重启恢复注册。

本项目必须保持 device operation token、绑定认证与显式 claim/lease，401明确停写/修复身份。不能借鉴成“收到若干401就自动清凭据、重配对或转移 owner”，MAC主键不是授权凭证。

### 5.2 错误信息返回 bool

Slate 后端错误结构清楚，但固件 `ApiClient` 多数接口返回 bool，状态/错误主要由日志和少量 callback 描述。`DoRequest` 内部读取错误 envelope 不代表上层取得结构化业务错误；不能作为本项目全链路失败返回的完整样板。

本项目仍需能区分 not dispatched、明确 rejected、accepted、completed 与 unknown，并保留阶段和恢复入口。Slate 语音队列也存在 not-ready 丢消息等分支，不复制为本项目的请求完成合同。

### 5.3 整组暂存不等于整组原子提交

[SyncManifestAndFrames / CommitStagedFrames](../../slate/firmware/main/sync/sync_manifest.cc) 先完整下载后逐帧 CommitFrame；[CommitSwaps](../../slate/firmware/main/storage/cache/cache_staging.cc) 对一帧多个文件做备份/交换并在该次交换成功后删备份。[CacheWriter::Rollback](../../slate/firmware/main/storage/cache/cache_frames.cc) 清理暂存，不恢复已成功提交的前面帧。

因此“下载失败不提前更新”有源码支持，但本轮不能将其宣称为“后续帧提交失败或掉电时整组必定保持旧版本”。README 的整组描述应结合这个实现边界。可借鉴先暂存再可见的思想，**不替换本项目完整 A/B Bundle 原子提交及掉电恢复合同**。

### 5.4 内容拉取不等于可靠写事务

Slate group next/prev 是相对动作，当前读到的请求结构没有本项目这类持久 transaction ID + 回执围栏。不能复制为丢响应后自动重试的模板激活模式；本项目以目标 template/context 和原事务证据防止重复副作用。

资源下载有响应大小上限和 Content-Length 完整性检查可供参考，但其 `vector` 整体缓存及按音频上限统一预算不能直接用于本项目小任务栈/低RAM链路。依本项目使用量及窗口预算流式处理。

## 6. 与当前整理的对应

| 当前文档 | Slate 带来的参考检查点 |
|---|---|
| [高层与wire](bridge-device-protocol.md) | 每操作有明确字段合同；错误与业务成功判据；trace ID、事务ID和内容摘要分别命名 |
| [下层场景](bridge-device-communication-scenarios.md) | 按前台/后台/恢复场景选择工作范围；连接存在与业务通道ready分开；没有通信需求不下载/启通道 |
| [实现对照](bridge-device-communication-implementation.md) | client/通道收尾、错误有没有传播到调用者、缓存提交有没有真实恢复证据，不能只信README概括 |
| [历史对账](bridge-device-protocol-history-audit.md) | 参考ETag思想不恢复旧业务通道；manifest增量仍是未来方案，不能推定当前能力 |

本文不新增实施待办，也不修改既有协议。若后续采纳，实施任务仍汇入 [backlog](roadmap/backlog.md)，并按本项目具体失败场景与资源证据验收。

## 7. 证据范围

已读 slate AGENTS、shared/firmware/backend README、共享schema、设备 poll/controller/service、HTTP client、ETag计算与响应、错误filter及既有测试源码、同步场景/暂存、语音 Protocol/MQTT/WebSocket相关边界。

只读检查未建立 slate 部署版本或实机状态，未重新运行其测试，不将测试文件存在表述为本轮测试通过。没有修改 slate 文件、启动服务或读取私有运行data。
