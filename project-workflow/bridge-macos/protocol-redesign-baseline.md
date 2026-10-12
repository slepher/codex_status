# P0 合同与本机基线

2026-10-12；源码基线 `49cec054`。本轮为 P0 基线与 P1/T01 首个增量，不是 P0–P6 全部完成。

## 入口与现行格式

固定帧由 `core/src/compact_frame.rs` 与 `src/v2_command_envelope.cpp` 定义：marker 0xC1，48 B 头，kind/op/result/payload_len/session/request_no/context；HTTP 整帧上限4096 B，BLE8192 B，单属性512 B。request_no不能为0；非空context必须16个十六进制字符；长度精确匹配。Wi-Fi现行能力为wifi_management=3；不能因文件名v2或snapshot schema=2删除现行格式。

| op | 操作 | 主要业务字段/成功证据 |
|---|---|---|
| 1 | status | 认证MAC、完整分页状态及data_contract；分页不能拼接不同read_id |
| 2–5 | data/plan/claim/renew | context、数据序号/计划ID、bridge身份；共享处理器ACK才确认；claim独立显式发生 |
| 6–9 | sync_begin/page/ack/complete | 冻结批次、offset/hash、已落盘ACK；不是持续离线存储保证 |
| 10–13 | bundle_begin/chunk/commit/activate | 单份完整Bundle、长度/CRC/offset、context；提交与激活各自确认 |
| 14–17 | wifi_snapshot/apply/result/result_ack | snapshot_id/offset；request_id/expected_revision/changes；query_id；终态落盘后才能ACK |
| 18 | wifi_command | 当前trial/activate等子操作；network_id/version或试连凭据；accepted不是completed |
| 19 | ota_image | 可选运行镜像只读证明；不是原生OTA上传前置 |
| 20–24 | ready_report/business_query/network_start/sync_config/sync_open | 当前会合/session/wake/计划关联；回报与反向HTTP分别证明可达性 |
| 25–27 | history/wifi_owner/wifi_info | 现行有界读与身份/配置状态；不续正式电源期限 |

以上为操作索引，不重复各操作完整schema；P3删除时仍须逐项核对字段白名单。HTTP注册集中在 `firmware/transport.cpp:1036–1056`；BLE op编码与HTTP共享编号，但当前 `wifi_*` 通配到18尚未收紧。

| 生产来源 | 当前调用路径/责任边界 |
|---|---|
| 自动BLE | app主循环 → rendezvous::ble_cycle → DeviceConnection → 业务服务/共享固件处理器 |
| 周期HTTP/数据投递 | app/platform调度 → core device_client → 固件HTTP分派；旧Pusher后备仍待P3删除 |
| 设备presence/report | Bridge接收器更新观察；反向请求另走device_client，不把响应当工作拉取 |
| UI/MCP Wi-Fi控制 | app/wifi入口 → 平台持久作业 → opportunity_admitted或unified_on_link → 同一wifi_attempt |
| 首次绑定/凭据恢复 | 绑定BLE身份和token → 显式claim/现行配网；旧permit/recover_on_link分支仍存在 |
| Bridge OTA | 镜像冻结/预检/普通上传/认证运行核验；客户端编排不成为设备前置 |
| 独立OTA | GET /update、POST /doUpdate、ArduinoOTA → token/原生Update；无需owner/session/Bridge |

P1/T01 修复位于HTTP与BLE共用的wifi_attempt；无需两个载体分别补条件。Rust HttpStatus(401/403)异常本来已有原写/查询区分，本次补齐JSON错误响应这条路径。

## 持久数据与切换围栏

只读检查本机 `bridge/target/debug/data/platform/state.json`：Note4键7C4FADB93408，一条本地目标、13个已完成activate作业，next_job_seq=273；这些作业均有operation_result，未确认数0、待ACK数0。仅代表读取时的Windows本地文件，不代表macOS、设备NVS或未来发布时状态。环境未设置CODEX_STATUS_DATA。本轮未打印配置内容、密码或token，未写运行数据。

Bridge原子JSON保存完整WifiState/冻结意图/request_id/sent/may_have_committed/confirmation/receipt_ack_pending/next_job_seq；wifi_mutate持久失败恢复内存旧值。设备WFS4为显式长度/数量/CRC/记录，最多12配置、8回执、8高水位；当前解码还支持WFS3和更旧迁移。本轮不变更持久格式或迁移数据。

P1后续必须统一 `(principal, namespace, request_id)`：apply去重与存储校验目前少namespace，而result/trial/activate包含namespace。取消/过期停止新写后，仍需独立query-only确认调度。不能把本机未确认数0当作删除设备回执或跳过转换的依据。

发布前步骤固定：暂停发送 → 私有备份本机实际data与已授权读取的设备状态 → 列出各namespace未知事务、指纹及高水位 → 转换并读回验证最终格式 → 恢复匹配软件。转换器与设备侧终态证明尚未实现；无法建立可执行围栏就阻止新写。此处是准备方案，不宣称已备份全部运行数据或验证迁移。

## 负向与独立OTA验收输入

| 输入 | 最终要求 | 本轮状态 |
|---|---|---|
| 错marker、尾随字节、op>27、request_no=0、非法context | 帧层拒绝 | 既有golden_frame_and_boundaries纳入core回归 |
| wifi_management=2/缺字段、wifi_authorize/permit、wifi_migrate、旧裸JSON | 最终无执行/翻译/降级 | 仍有旧成功路径；P3负向矩阵待实施，不宣称已拒绝 |
| 原trial/activate已发后result返回storage/session/claim/busy/unauthorized错误 | 保留原Unknown/ID，不ACK、不重写 | 本轮新增共享服务回归 |
| result错误响应带outcome=rejected | 不能据此宣称原事务被拒绝 | 本轮新增回归；ok=true的拒绝回执仍可确认 |
| GET /update、multipart POST /doUpdate，正确token且无owner/session | 原生上传可用 | 源码入口核对；未上传实机 |
| 无文件/非multipart/缺错token/错target/中止后重试 | 401/明确失败/资源回收，不能UPDATE OK | P4待复现及窄修；当前完成handler只看全局otaUploadDenied/Update.hasError |

旧路径具体删除位置沿用[设计§9](protocol-redesign-design.md#9-旧逻辑删除清单)。现有C++测试的legacy/permit正向用例本轮仅用于建立旧基线；P3必须删除或改为拒绝测试，不能把它们作为最终协议支持承诺。

## active_protected基线

原宿主测试在已有另一bond配置时，删除active保存行仍期待active_protected。MSVC宿主执行复现该断言失败；按现行合同修夹具后全程序通过。新增独立唯一配置拒删用例，并校验有备用配置时删除不启动Wi-Fi、不改active版本/凭据，重载后凭据仍可取。没有改变生产固件语义。
