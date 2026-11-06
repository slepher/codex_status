# P2 首批状态（2026-11-06）

## 当前目标与最新增量

**最终非实机验收已完成。** 完整workspace542通过/5默认忽略，其中3项App/Fake集成另行显式通过；最新Bundle/Wi-Fi完成钩子及实际AP01 App宿主ASan/UBSan通过。逐项覆盖、性能目标与实测区别、未执行边界见[完成审计](completion-audit.md)。P2未提交；不部署Bridge、不刷ROM。

- [Bundle/Wi-Fi完成钩子](../../artifacts/protocol-p2/bundle-completion-tests.log)：拒绝/未知/冲突不关窗、成功完成关窗，Bundle提交丢ACK可原事务重放，正式期限不变。
- [显式App集成3项](../../artifacts/protocol-p2/final-opt-in-app-verified.log)：pending activation、单包presence反向认证、本机sync-v1归档通过。
- [AP01实际App报告](../../artifacts/protocol-p2/native-app-report.json)：生产App及兄弟维护适配的ASan/UBSan测试通过；错误帧session拒绝，重新握手boot_id不变。跨平台--app-only入口接受显式Bundle，不依赖旧私有ROM。宿主通过不等于真实ROM ABI验证。
- [AP01共享编译/布局及测试Bundle导出](../../artifacts/protocol-p2/ap01-host-fixture-final.log)：3项通过。

以下为分阶段证据，历史“尚未完成”不代表当前结论。

P2-4显式序号增量（最新）：新Wi-Fi事务ID不透明，request_seq单独持久化并贯穿写入/查询/ACK；WTX2显式保存8B序号，旧WTX1只在磁盘导入解释规范w3_，旧不透明无序号回执保留。Bridge旧未知作业导入保持原ID、状态和高水位；WFS6离线归档保留新字段。源码候选未部署，P2整体仍待最终审计。

- [数字入口/迁移ASan+UBSan](../../artifacts/protocol-p2/sequence-host-tests.log)：真实WTX1选择器迁移、两写边界中断、选择器提交后回执/读回失败、重启原ID查询/退役、旧无序号保留、非法序号类型通过。状态报告实际WTX1/WTX2，读取不冒报迁移。
- [management](../../artifacts/protocol-p2/sequence-management-tests.log)与[setup](../../artifacts/protocol-p2/sequence-setup-tests.log) ASan/UBSan通过；[存储故障矩阵/资源](../../artifacts/protocol-p2/sequence-storage-tests.log)通过。[最终完整工作区](../../artifacts/protocol-p2/sequence-workspace-final.log)：542通过、0失败、5条件忽略；含App132、核心191、模拟器71、历史结算4与离线归档4项。早期失败日志保留，不计为通过。
- [四用途及Wi-Fi HTTP/BLE完成钩子](../../artifacts/protocol-p2/sequence-network-completion-tests.log)：未知/序号冲突不关窗，成功退役关闭临时窗口且正式Light截止不变；已包含在最终模拟器71项中。[共享固件错误帧](../../artifacts/protocol-p2/sequence-protocol-host.log)通过。
- [Note4构建](../../artifacts/protocol-p2/sequence-final-note4-build.log)、[匹配ELF栈92项](../../artifacts/protocol-p2/sequence-final-wifi-stack.log)通过。候选`artifacts/protocol-p2/note4-p2-sequence-final-0.18.108.bin`，1865360B，SHA256 `8b6c0473316cb2cb7ad8d9ebd24c3a14d7c68a7b135cd9ee97b8732d568a7431`；ELF同名前缀，SHA256 `d11eeea253127ca966c3b253ddef7fc14262c383f3b17a70a2cb4c8582f2ff6a`。env zectrix-note4-b，marker 0.18.108-note4；RAM97728B、Flash1811619B。

以下身份收敛是前一候选验证，不代表最新源码全套结果。

P2-4身份收敛增量：Status/BLE摘要/sync-v1回执改用boot_id，传输session仅由48B头核对；Bundle接收内部绑定boot身份，AP01实际App也移除CHUNK明文nonce门禁及输出。Data/Plan关联ID使用已有context/seq/plan_id，查询ID不再拼CRC，内容完整性与业务冲突校验仍保留。此身份阶段尚未迁移w3_/WTX；最新显式序号结果见上文。

- [核心/BLE/渲染](../../artifacts/protocol-p2/identity-core-tests.log)：331通过、2条件忽略；[App](../../artifacts/protocol-p2/identity-app-sim-tests.log)：132通过、3条件忽略；同日志中模拟器首轮4个负例误替换失败，已恢复旧nonce拒绝断言，[完整重跑](../../artifacts/protocol-p2/identity-bootstrap-tests-final.log)71通过。[客户端明确业务ID断言](../../artifacts/protocol-p2/identity-client-tests.log)15通过。
- 两个遗留宿主夹具修正为当前wifi_apply、真实open元数据、控制字符拒绝及直接UTF-8页拼接；[management通过](../../artifacts/protocol-p2/identity-wifi-management.log)。最大转义12条配置暴露jsonChunk按转义后超额直接扣原字节而丢弃整个候选的真实缺陷，现限制单次缩减幅度，仍有界且保持UTF-8边界；[setup ASan/UBSan通过](../../artifacts/protocol-p2/identity-wifi-setup-asan.log)，最大JSON回执434B。[BLE wire 6项](../../artifacts/protocol-p2/identity-ble-wire-tests.log)、[生产大Status逐字节回读1项](../../artifacts/protocol-p2/identity-paging-tests.log)通过。
- AP01边界检查通过，修改的App/native_smoke宿主语法编译通过；QEMU脚本语法通过，未运行QEMU/原生App完整集成，不以语法编译替代运行验收。
- [最新Note4构建](../../artifacts/protocol-p2/identity-firmware-build.log)及[匹配栈检查](../../artifacts/protocol-p2/identity-wifi-stack.log)：候选`artifacts/protocol-p2/note4-p2-identity-0.18.108.bin`，1864272B，SHA256 `7b42f983607b86906e18c15767b26e76dafc9b3a7ed228dcfd4a8467d1d31cce`；ELF同名前缀，SHA256 `d5795a9f828a8acc3620f04f002b33ea1e51f9cb8c583f720cf3274aa079121a`。env zectrix-note4-b，marker 0.18.108-note4，RAM97728B、Flash1810531B；无本轮NVS格式变更。未部署、未刷写。

用户目标为完成整改设计全文的非实机内容；目标仍active，逐项证据见[完成审计](completion-audit.md)。P2-1验收收尾、P2-4剩余部分与最终全文回归仍未完成。

P2-1源码已统一为op22 network_start，op24退役并保留空位。reason覆盖wifi_setup/data_sync/diag_sync/bundle；先完成BLE配置再冻结回报路由并准入临时窗口，同ID重放不续期、改内容冲突、关闭原ID不能重开。Data不依赖诊断开关，正式Plan期限不变；Bundle commit/Wi-Fi result_ack按对应用途结束临时窗口。仍需最终全文审计和业务完成钩子的专项故障覆盖，不宣称P2全部完成。

- [模拟器主套件](../../artifacts/protocol-p2/network-bootstrap-tests-final.log)：70通过，含关闭诊断后的Data完整闭环、正式/临时期限独立、丢ACK及诊断冻结批次恢复。
- [App主套件](../../artifacts/protocol-p2/network-app-tests-final.log)：131通过、3忽略、1旧RF夹具失败；该夹具原先依赖已删除sync_open的900ms延迟，现使用明确过期的1ms预算，[定向重跑](../../artifacts/protocol-p2/network-read-tests.log)1通过。生产源码相同；最终仍须完整重跑，不把失败日志说成全绿。
- [四用途/原ID关闭围栏](../../artifacts/protocol-p2/network-purposes-tests.log)1通过；[宿主错误帧和24/25退役](../../artifacts/protocol-p2/network-frame-tests.log)。
- [Note4构建](../../artifacts/protocol-p2/network-firmware-build.log)与[匹配ELF栈92项](../../artifacts/protocol-p2/network-wifi-stack.log)通过。最新仅候选`artifacts/protocol-p2/note4-p2-network-0.18.108.bin`，1864192B，SHA256 `3f194e2631d5545d87b7aba31c43a88664a71f6945c0b444b697891d45a8b784`；匹配ELF SHA256 `b1f57395f74af8593302387e9181be5fbaf8861ffba77a6db649665578a6dcbc`。env zectrix-note4-b，marker 0.18.108-note4；RAM97728B、Flash1810455B。本增量无NVS格式变更，沿用已有有界窗口状态；动态RAM/栈实机未测。

P2-2现已移除固件GET /history、BLE op25、history_sync_ms及Bridge旧wake_history消费者/备用存储模块；op25保留空位并由帧校验拒绝，不重编号。唤醒摘要原本就在SYNC_WAKE_SUMMARY日志内，统一经sync-v1传输与确认；诊断关闭时不再绕过配置做额外BLE历史拉取。旧磁盘日志文件未删除。

- [App回归](../../artifacts/protocol-p2/history-app-tests.log)：132通过/3忽略；旧history模块4项测试随模块删除，不将测试数减少当作失败。
- [8条唤醒摘要分页与下一批不重复](../../artifacts/protocol-p2/history-sync-tests.log)：1通过；真实Sleep→BLE下发Light→HTTP sync-v1→期限到期第二批链路。
- [旧路由/旧Plan字段拒绝](../../artifacts/protocol-p2/history-retired-tests.log)：1通过；修正原测试读取分页封装当完整Status的夹具缺陷，使用认证拼接后状态。
- [Rust/C++操作码一致性](../../artifacts/protocol-p2/history-opcode-tests.log)：5通过；[生产BLE错误帧宿主](../../artifacts/protocol-p2/history-frame-tests.log)通过。
- [Note4构建](../../artifacts/protocol-p2/history-firmware-build.log)、[匹配ELF栈92项](../../artifacts/protocol-p2/history-wifi-stack.log)通过。仅候选 `artifacts/protocol-p2/note4-p2-history-0.18.108.bin`，1864272B，SHA256 `e9e9f2dc889b35f49d95916b85b5544b2f9d74397184a5cd153c551a9f23b0a7`；ELF同名前缀，SHA256 `acb30e2ac23358bdadf388166f1209b8dd802bc858fd6f7bb8018045daaa70d3`。env zectrix-note4-b；marker 0.18.108-note4；RAM97728B、Flash1810531B，无NVS格式变更。

以下是此前首批验证记录，非本次增量全套结果；最终工作区全套待其余P2完成后重跑。


P1已提交 `3cf9061`（Streamline protocol encoding and presence reporting）。P2首批修改尚未提交；未部署/重启Bridge，未刷ROM、未访问实机。

## 已实现

- P2-3：HTTP/BLE Wi-Fi控制入口、management/setup共享处理器直接消费数字opcode；Bridge不再向JSON塞入op再剥离。持久事务摘要、原ID、principal/namespace及高水位保留。
- P2-4部分：Bundle CHUNK删除URL session_nonce与X-Session-Nonce；继续验证已认证传输会话和原Bundle业务事务/boot/offset。
- 同步Fake、共享C++宿主及客户端测试；发送前拒绝保留NotSent/http_prepare结构化反馈。

## 验证

- 工作区最终覆盖合计**543通过、0失败、5条件忽略**：相同生产源码下[App/BLE 169通过/3忽略](../../artifacts/protocol-p2/workspace-tests-final.log)，修正旧Core测试入口后[其余套件374通过/2忽略](../../artifacts/protocol-p2/workspace-tests-remaining.log)，后者退出码0。含HTTP客户端15项、模拟器69项；不把首轮失败或重复运行计入。执行分段和原因见[审阅](p2-review.md)。
- [数字opcode宿主](../../artifacts/protocol-p2/wifi-control-op-tests.log)通过：直接分发、旧字段拒绝、持久重放/冲突/退休、读分页、principal拒绝。
- [存储故障与预算](../../artifacts/protocol-p2/wifi-storage-tests.log)通过：41处实际分配故障、提交后报错查询原结果、空/单/典型/上限、损坏拒绝与WTX1高水位。
- [AP01职责边界](../../artifacts/protocol-p2/ap01-boundary.log)通过。[Note4构建](../../artifacts/protocol-p2/firmware-build.log)与[匹配ELF栈92项](../../artifacts/protocol-p2/wifi-stack.log)通过。
- `git diff --check`通过。
- 旧management/setup独立脚本未通过，P1对照可复现；不冒称这些脚本通过，细节见审阅。

## 仅候选

- env zectrix-note4-b；marker codex-status-ota-v1|zectrix-note4-400x300|0.18.108-note4。
- `artifacts/protocol-p2/note4-p2-0.18.108.bin`：1868576B，SHA256 `5534a9621bcf19bc499ede9aeb9d8987a006957f5760134c1f1c9bf1c16224c6`。
- 匹配ELF `artifacts/protocol-p2/note4-p2-0.18.108.elf`，SHA256 `1192af81833fc9e829a460e26029bf3a4ae9a442f310f3a7e0564ba0a2128be1`。
- RAM97800B、Flash程序1814511B；资源分析见审阅。版本名与部署版相同，不据版本名声称已升级。

## 未完成边界

P2-1统一即时network_start已实现并完成主要回归，最终审计仍开放；P2-4剩余nonce/CRC请求ID/w3_显式业务序号仍开放。P2-2已完成上述增量验证。A9/B6/C8继续开放；实机验收待后续明确部署阶段。

## 独立宿主复现

在仓库根目录、预建artifacts/protocol-p2后运行：

```sh
c++ -std=c++17 -DARDUINOJSON_ENABLE_ARDUINO_STRING=1 -Ibridge/crates/render/shim -Isrc -I.pio/libdeps/zectrix-note4-b/ArduinoJson/src tools/test-wifi-control-op.cpp src/ble_wire.cpp -lz -o artifacts/protocol-p2/test-wifi-control-op
artifacts/protocol-p2/test-wifi-control-op
```

存储故障/预算程序使用相同参数，将test-wifi-control-op替换为test-wifi-storage。macOS宿主SHA来自CommonCrypto；本机通过不代表嵌入式动态分配实测。
