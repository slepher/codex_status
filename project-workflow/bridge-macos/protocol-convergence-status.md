# 协议收敛状态

2026-10-19，R1–R5源码完成；R3–R5已提交9e748eb；Windows Bridge与Note4随后已部署，15分钟短时观察完成，见[部署记录](protocol-convergence-rollout.md)。用户要求的 `codex/fix-wifi-switch` 已合入master（93bb386），worktree已移除，分支保留。该分支的Windows部署是旧协议切网修复，不代表本轮C2已经上线。待办归B6/C8，C9继续开放。

## 已完成与删除清单

| 切片 | 最终实现 | 同时删除的机制 |
|---|---|---|
| R1/R2 | 设备→BLE有界准入；停止执行的原事务有限查询/ACK，先持久确定证据再退役 | 连接后才发现忙设备的路径、HTTP独立收尾循环 |
| R3 | AcceptedPending/Unknown分离；trial/activate复用持久Job与Transport；ConnectionFacts持久策略与RAM观察/联系分离，UI/MCP共享投影 | 全局OPERATIONS、SCHEMES、第二套trial/activate执行路径、Connection的HTTP状态改写、注册前无用scheme_get；INPUTS移入AppCtx |
| R4 | RendezvousContext持有本轮观察、冻结准入、Plan、sync及结果证据；仍复用Wi-Fi核心 | 13个私有JSON键、返回大型元组、未读sync错误字段、无用Plan副本 |
| R5 HTTP | Tokio TCP/读写和session/status异步锁；原截止贯穿等待/连接/发送/读取；取消释放socket与设备许可 | 同步HTTP、轮询锁等待、blocking_device/业务blocking封装、worker许可跟踪及同步wifi_attempt/wifi_settle适配器 |
| R5 wire | C2唯一格式，35个显式opcode；载体准入后CommandContext校验业务身份；Note4/1.54/Fake/AP01同改 | C1接收、wifi_*通配编号、wifi_save别名、wifi_command包装、JSON op/command/session_nonce及重复nonce校验；旧串口wifi-json写入及wifiManagementRequest |

这是实际替换旧路径，不以改名/压缩排版或删除校验计算简化成果。测试用同步进程夹具只用于已有Fake测试，生产没有同步HTTP兼容入口。原生BLE及系统Wi-Fi查询的隔离线程仍按平台需要保留。

## 合同、迁移与回滚

- 正常接受不写错误；只有明确接受/完成证据允许network_start。未知提交继续冻结原principal/namespace/request_id、sent事实、冲突围栏和有限查询/ACK预算，不换ID重发。
- connection字段显式schema=2；仅保存目标、意图、退避与准入错误。旧Connection单向读取迁移，丢弃旧online/HTTP证明及瞬时RF错误，保留认证/安全阻断；未知schema拒绝。RAM观察与联系按boot和360秒新鲜度投影，新设备失败不能被较早HTTP成功覆盖。重启后不得恢复虚假在线状态。
- 部署前冻结exe/data；旧Bridge不能直接回读新schema/Job，回滚需恢复配套备份。迁移读取旧磁盘数据不是旧wire兼容。
- 帧op/session替代JSON重复字段；device_mac、principal、业务request_id、Data seq、Plan id、业务context与设备回执高水位保留。HTTP Bearer、BLE绑定/加密/token、claim operation_token及owner规则不放宽。
- 状态响应保留连接代次，二进制Bundle chunk保留接收代次关联；它们不再是JSON命令的重复会话验证。HTTP/BLE旧JSON字段明确拒绝。无旧协议回退；升级失配时仅可使用用户已授权的受鉴权原生OTA。

## 验证

完整命令（bridge目录）：`cargo test -p bridge-core -p bridge-render -p device-sim -p bridge-app -p bridge-ble -p bridge-mcp --lib --bins --tests`。

491通过、0失败、5条件忽略：App114、BLE29、Core174及集成26、MCP3、Render及集成72、Fake bootstrap69与历史收尾4。忽略为App三项条件联测及AP01 live/storage，不计通过。日志：[最终回归](../../artifacts/bridge-macos/protocol-convergence-final-regression.log)。合并后保留window_closed现场初态，夹具补齐相同boot_id以符合新投影的启动隔离；另复验连接迁移和该现场用例：[合并后回归](../../artifacts/bridge-macos/protocol-convergence-post-merge.log)。

边界覆盖：接受/未知、原ID跨重启收尾和8回执/32Job容量、过期/取消/身份隔离；重启丢弃在线证明、跨boot/新鲜度；慢分页原截止、零写/部分写/完整帧丢ACK、真实TCP取消关闭socket与释放许可；35个Rust/C++操作逐项对拍、旧C1/别名/旧字段拒绝、MAC/principal/凭据与原重放围栏。并行App测试的模拟主机环境已纳入SIM_ENV隔离。合并后App完整复验114通过/0失败/3条件忽略，见 `protocol-convergence-app-post-merge.log`。合并后迁移3项及现场切网1项通过；过程中旧夹具缺boot_id引发的失败及修复均保留在日志中。

| 目标 | 实际验证 | 未执行 |
|---|---|---|
| Note4 | 0.18.105-note4构建/匹配ELF栈、直接OTA完整运行哈希证明、15分钟观察及自然BLE交汇，见rollout | 实体屏幕目视、长期RF/Flash掉电完整矩阵 |
| 1.54 | 0.18.37-bw构建、版本marker、匹配ELF Wi-Fi栈门禁 | 实机刷写及RF |
| Fake | 真实共享C++核心、HTTP/BLE、bootstrap69+收尾4 | 不代替原生BLE/RF |
| AP01 | RV32编译链接、完整0041候选打包、真实application宿主ASan/UBSan通过 | 本轮未跑恢复VFS/UI ABI执行/实机，manifest仍flashable=false |

Note4/1.54栈命令为 `python tools/check-wifi-stack.py <matching.elf> --no-online-wifi-migration --no-legacy-management-entry`，对应源码已无这两个入口。证据 `artifacts/bridge-macos/protocol-convergence-{note4,154g}-{build,stack}.log`。AP01证据同目录 `protocol-convergence-ap01-{build,package,host}.log` 与 `artifacts/ap01-platform-integration/device/native-app-report.json`、候选manifest。

## 构建产物（Note4随后已部署，1.54/AP01未部署）

| 目标/路径 | 字节 | SHA256 |
|---|---:|---|
| Note4 `.pio/build/zectrix-note4-b/firmware.bin` | 1850016 | `5433769a050a7fe717f52d3d1b5c0c1f76e66a2ce12b5e4cce1880eee7059608` |
| 1.54 `.pio/build/esp32-s3-epaper-154g/firmware.bin` | 1831776 | `b76365e408f338be39a393c64dcf143da5c4506ff2c9da2b4cc72e2f2397e7a4` |
| AP01 `artifacts/ap01-platform-integration/device/ap01-native-0041-c2-candidate.bin` | 6994312 | `c2305894caf41af7e30501d5a3c39a8458ff0f0fae179d1b27573e8463b53221` |

AP01 build_id `f0392fd4d9e2e592e77f2b6e01928283`；匹配ELF490252 B，SHA256 `c73f074f589e5c0cae9a203608a60f50ae7182632713b7c0c65d16f9a1aabad0`。完整产物清单见 `artifacts/bridge-macos/protocol-convergence-artifacts.json`。重新编译会改变ROM哈希。

## 资源与边界

本轮不改WFS4/WTX1/WCM1/PPL1/Bundle持久格式；空/单/典型/上限设备持久增量均0，不删除未知回执。CommandContext在ILP32为两指针8 B，载体持有认证principal；不复制绑定仓库。HTTP入口旧字段拒绝使用有界临时JSON解析，与后续handler解析分时释放；不能把固定48 B帧头当解析堆预算。

Note4静态RAM96300 B、Flash1796235 B；1.54静态RAM81216 B、Flash1778848 B。匹配ELF serviceV2Ble栈帧960 B（门限1024），Wi-Fi setup576 B，BLE→Wi-Fi存储本地链2784 B（门限4096）；链不包含平台/间接调用/ISR，不声称运行峰值。C2帧上限仍HTTP4096/BLE8192、单属性512 B，无增大接收缓冲。

Bridge每设备至多一个ConnectionFacts，观察/联系在RAM；紧凑JSON connection值：空null4 B、单条156 B、典型带退避184 B、上限500 B（32字节SSID最坏JSON转义、32字节ID、64字节错误、整数上限），不含字段名。无按配置容量预留空数组；实际字符串按需分配。宿主allocator和运行堆峰值未实测，取消测试证明生命周期结束不留HTTP worker。

AP01现行预算未变：绑定空16 B/单典型121 B/8条典型856 B/单最大281 B/8条最大2136 B，提交堆上限6816 B；应用事务488 B、chunk堆32768 B、HTTP头/小体6145 B、持久记录1036 B、pending+record2072 B、native scratch栈1024 B。预算不含allocator/完整UI调用链；详见native-core-audit.json及 `protocol-convergence-resources.json`。

源码验收阶段没有启停Bridge、访问真实data或刷机；用户随后授权的Windows/Note4部署独立见rollout，未发布Bundle。未改UI文件，不把合成IPC/宿主结果当产品实机验收；B6/C8长期RF、Flash、原生BLE取消及C9独立更新/实机闸门继续按backlog管理。
