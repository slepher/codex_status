# 超时结果分类与关键日志修复

关联 B6/C8。以 wake-network-log-investigation-20261012.md 的已证缺口为范围；保留现有 RF、HTTP、入网、同步截止，不统一延长。

## Plan / tasks

1. 共用发送结果分类：未发送、写入未知、整帧已写但确认未知。BLE 每个失败分支记录阶段、原因、实际/剩余预算；已发事务继续使用原 ID 查询。
2. network_start 明确未发立即结束；共享预算在 BLE 命令内部执行，避免外层取消丢失发送阶段。HTTP 证明到期明确预算耗尽，设备网络失败原因落入同源状态。
3. 固件保留当前尝试内的具体关联失败，记录启动接受/拒绝、入网开始/IP/HTTP就绪/结束原因；区分同步总预算与无进展。修复空 INFO 初始化噪声。
4. 生产 Fake/持久任务回归覆盖未发与未知边界、同 ID 查询、网络原因及预算分类；宿主固件回归、Note4 构建/匹配 ELF 栈门禁、设备页及原生窄宽截图。

## Status

14:33代码、专项回归及最终候选打包完成。Bridge候选20,817,136 B，SHA256 `58b4a78d6584f2dd5bfe0e47f56a989941f516502b2a498b371dd0c226cdb06b`，`artifacts/bridge-macos/Timeout Reporting Candidate.app`，codesign --verify --deep --strict通过。既有 RF 刷新修复与其他未提交变更保留；本轮未替换运行 Bridge、未烧录设备、未提交。

## Review / boundaries

不修改网络凭据、PowerPlan期限、鉴权、日志持久格式或4KiB容量。日志不包含凭据/token/nonce/owner原文。未授权提交；本轮先完成可审查实现及验证。

## 实现与结果语义

- `core::rendezvous::CommandFailure` 保存发送证明、失败阶段和有限原因码。`NotSent`→`not_sent`；分片写入未知与整帧已写/ACK未知→`result_unknown`。所有 write/ACK 截止分支记录耗时、剩余预算与RF预算来源；INFO刷新记录旧/新预算。日志不输出命令体或底层任意错误原文。
- `DeviceConnection` 在开始写帧时标记未确认，仅整帧写成功才解除；写失败或外层取消后，后续任何业务命令均在发送前拒绝，避免部分帧与下一命令拼接。整帧写后丢ACK仍可只读确认。调用方保留持久事务原ID，已发未知的下一次仅query/result；当前查询未发不能清除旧事务围栏。
- network_start 的共享预算在载体内部执行：明确未发立即结束；分片未知结束并关闭链路；仅整帧已写的ACK未知允许有界HTTP证明。预算耗尽时记录 `http_proof_budget_exhausted` 与启动结果，启动未知仍返回 `result_unknown`，不冒充未发送。
- 任务、connection和MCP/UI使用同一状态：未发、结果未知、网络未找到、预算耗尽有独立提示；会合失败更新已准入任务的等待原因，不再一直显示无原因“已排队”。保持待确认事务query-only；明确未发不消耗入网尝试次数。
- 固件保留本次入网首个明确原因，后续普通断开reason36不覆盖201/认证失败；新尝试及GOT_IP清除旧原因，匹配状态同时报告相应原始reason。fallback失败沿用具体原因，缺证时仅 `connect_timeout`，不推断密码错误。
- 固件关键流：`network_start accepted/rejected`、`association_start`、`associated`、`wifi_ip`、`http_listening`、`ready_report_acked`、失败结束；[net]白名单进入统一诊断环。事件回调仅更新原子计数，日志及摘要由主循环写入；计数说明EPD阻塞时可能合并事件，不伪造中间时刻。会合成功补 WIFI_IP 摘要。报告ACK与Bridge认证HTTP证明分别记账。
- 原90s条件保持不变，结束分别为 `sync_total_budget_exhausted` 和 `sync_no_progress`；认证HTTP状态增加可选 `sync.last_error`（本次启动RAM），新开窗清除。完整原因文本进入统一诊断流；未改记录格式/哈希/期限。BLE初始化先安装INFO provider再创建GATT，避免空MAC预检伪错误。

## 验证 / review

- Rust最终回归目标core/ble/app（含集成）：268 passed、3条件忽略、0 failed。生产Fake专测未发送/ACK未知/写未知/成功四种启动结果；未发及未知在200ms级预算内结束，不等待HTTP；分片未知后status也不能发送。生产持久任务差分确认发送标记、原ID和下一次query；已发原事务不会因后续查询未发而失去围栏。
- 实际生产Wi-Fi回调处理器宿主回归：201→36保留network_not_found/201、认证失败保留、新尝试与IP重置；关键日志筛选与两类同步截止通过。纯生命周期边界及millis回绕通过。命令见下面；无设备/NVS写入。
- `node tools/test-device-page.mjs`通过；WKWebView60场景通过，包括四类失败各560/1000窗口。已审阅560-result_unknown、1000-budget截图：文字/操作可见、无溢出。合成IPC不代替真实设备通信。
- Note4 `zectrix-note4-b` / `0.18.85-note4`构建通过，匹配ELF Wi-Fi栈64项通过；ROM1,851,104 B，SHA256 `8c4126cfec33c18073c11639c8d5c90ddd5c2071eb73b6119c997cbcba8fecad`。marker唯一且为`zectrix-note4-400x300`，env与marker不是同一个字符串，不混用。匹配ELF SHA256 `2f7d365345e1899eb37603aa4a37ca4c8b3ff265dd8f81491523195915e911ec`。
- RAM静态120164/327680 B，链接Flash1799207 B；新独立静态符号为2B首次原因、两项4B事件计数、两项4B已消费计数，共18B，另sync State一个4B指针（聚合对齐未分离）。无RTC/NVS格式或持久字节变化、无新大数组/任务栈；JSON增加一个有界状态字段，实机峰值heap/功耗未采样。新增关键文字同样占4KiB环，不解决离线两小时保留，C8继续开放。
- 旧 `test-wifi-management.cpp` 的 `active_protected`断言失败；git HEAD原测试+原wifi_setup.cpp的对照也在同一断言失败（ArduinoJson7.0.4；当前7.4.3亦失败）。不是本轮引入，不把整套宿主管理测试写为通过。新生产事件回归单独通过；旧断言/合同核对另列backlog。
- 首次pio未设隔离packages目录而开始下载共享框架，核对后停止该唯一进程，改用既有`.pio-pkgs/note4`/`.pio-core`完成构建；未切目标、未改sdkconfig指纹或清缓存。首次宿主命令补齐Arduino String与syncBase64依赖后执行；这些命令配置失败不冒称生产回归失败。

关键证据在`artifacts/bridge-macos/timeout-fix-20261012-{rust-final,node,wk,network,stack,pio,host-baseline,release-final}.log`；ROM/ELF/map及candidate-safe.json同目录。旧宿主对照仅为证据，不进入产品。

专项C++检查（已有输出目录）：
```sh
clang++ -std=c++17 -DARDUINOJSON_ENABLE_ARDUINO_STRING=1 \
  -include bridge/crates/render/shim/Arduino.h -I bridge/crates/render/shim \
  -I artifacts/bridge-macos/deps/ArduinoJson-7.0.4/src \
  tools/test-network-diagnostics.cpp src/ble_wire.cpp src/v2_sync.cpp -lz \
  -o artifacts/bridge-macos/timeout-fix-network-diagnostics
artifacts/bridge-macos/timeout-fix-network-diagnostics
```

所有超时常量保持原值；数据、模板canonical哈希与帧/持久格式不变。现场仍需部署后真实RF/网络故障矩阵；候选不能写为已生效。
