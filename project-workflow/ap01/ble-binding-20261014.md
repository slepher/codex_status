# AP01 0031 安全蓝牙绑定接续

> 同日后续用户决策改为[Wi-Fi双屏核对与旋钮确认](screen-binding-design-20261014.md)，首版只防误操作，不再要求SMP；以下原厂指令与扫描结论是旧路线证据，不作为当前实现的前置阻塞。

2026-10-14。用户确认优先实现 **AP01 与 Bridge 的安全蓝牙绑定**，不是修复米家绑定。设备无 fallback；主方案为 OTA 与常规通信/绘图两个独立进程。**已证实0031现有ATT权限检查无条件拒绝加密/认证属性，直接复用原厂GATT不能完成安全绑定**；尚未实现可部署绑定固件或完成实机绑定。

## 已完成与证据

- 解除 `PROGRESS.md` 唯一冲突（更新时间），Git 索引不再存在 unmerged 条目。保留既有 Note4 endpoint MAC 修复、测试和断连调查，未提交。
- 当前 Mac 原无 Windows 私有 AP01 产物；从固定公开上游重新获取0031原件，6,804,520 B，SHA256 `8a721fc8ef25458d415b2460e4a251e0503a82f7743fdff85b12612190e5c1cb`。原件、分析产物留在 `artifacts/ap01-research/ble0031-20261014/`。旁仓库按 `3bd98a8ca4fb35a9f8869121b13961d405b16dff` 稀疏只读克隆，未改上游文件；停止了首次包含185 MB无关输入的全量克隆。
- 新增 `tools/ap01/inspect_ble.py`，复用已有0031校验器/反汇编器。只收集基本块常量引用、原字节指针匹配及指定窗口；校验失败、未知指令/控制流、输出已存在均不能被当成功。精确原件、变异拒绝、截断拒绝、地址跨无关指令和寄存器覆盖/跳转失效测试通过；原有 ROMFS/内置任务等自检通过。
- Capstone 5.0.7：55个相关字符串、135处候选引用，扫描区文件 `[0x1000,0x1b0000)` 中14,407条未解码项。此数量不是函数数；字符串缺失不能证明 SMP 未编译。`ble-block-evidence.json` 为最终引用报告，最初仅相邻指令的 `ble-evidence.json` 保留为历史。
- 本机 LLVM 23.1.3 启用 T-Head 扩展交叉解码，选定调用目标与 Capstone 一致；例如 `0xa01280a6` 的未知指令解为 `th.mula s1,s10,s5`。原 `.incbin` ELF 带数据映射符号，须 strip 后才可作为代码反汇编；早期 `.word` 输出不算解码成功。`original-code-llvm.dis` 地址为代码段偏移，**需加 `0xa0000000`**，不是Flash写地址。窗口之外仍可能混入数据，不能据线性反汇编填完整ABI。

## 本轮定位：可以继续查，不能直接调用

| 位置 | 本轮直接证据 | 尚不能声称 |
|---|---|---|
| `0xa0127d3a..7d52` | 原厂分配32 B服务描述与288 B特征描述；之后按48 B记录初始化6个特征 | 新服务预算已知、可复用其全局内存 |
| `0xa0127f30..7f50` | 把固定 `0x62fc4c34` 传给 `0xa00daa7a`；后者操作属性表/handle和链表，符合底层GATT服务注册行为 | 米家 `mible_gatts_service_init` 是可直接调用的通用动态注册函数 |
| `0xa01280a4..80d6` | 按48 B特征记录、取handle查属性后更新属性数据 | 可以替换原厂FE95表而不影响米家 |
| `0xa0191a50..1ae4` | 日志关联 `mible_gatts_notify_or_indicate`；构造24 B栈上参数，调用 `0xa00dc330`；本层栈64 B | 已确认全部参数、连接加密约束或完整调用峰值 |
| `0xa0127c04..7c28` 等 | 原厂有界回调表扫描/登记，界限4；部分访问由T-Head索引指令实现 | 当前表空余量已知，能安全追加/卸载回调 |

`ble-windows.json`、`ble-registration.json` 与 `manifest.json` 保留原字节、工具版本和摘要。上面的320 B是原厂现有初始化分配，不是新增绑定实现的RAM承诺；当前没有新增设备端持久格式/运行分配，空/单条/上限预算须随真实适配实现给出。

## 已闭环：原厂权限检查不具备可用的加密通路

找到原厂函数 `0xa01a99ec..0xa01a9a34`，72 B，SHA256 `1b4629d8e55c152d4bb86ee4e9bc2476c75cddbaf846bff8c00e0c4984e2e0ea`。实际参数为属性指针和权限mask；读属性偏移18的权限、偏移4/8的读写回调是否存在。随后 `perm & mask & 0x30` 非零固定返回5，`perm & mask & 0x0c` 非零固定返回15；整个函数没有连接状态读取或子调用。

这不是仅凭字符串缺失作结论。原始指令由LLVM解码，并由新增 `tools/ap01/test_ble_security.py` 在Unicorn 2.1.4中执行；10个明确场景及**6144个组合全部通过**。组合覆盖全部256个权限字节、3种实际调用mask、4种读写回调存在性，并切换未使用参数为零/未映射地址；校验真实返回、栈和帧寄存器恢复、属性未被修改。函数、属性和栈之外未映射连接对象，错误解引用会使测试失败。

| 原始指令实际执行 | 返回值 |
|---|---|
| 普通读 / 普通写，有对应回调 | 0 / 0 |
| 缺少读 / 写回调 | 2 / 3 |
| 要求加密的读 / 写 | 15 / 15（Insufficient Encryption） |
| 要求认证的读 / 写 | 5 / 5（Authentication） |
| 通知路径的加密 / 认证检查 | 15 / 5 |

真实调用点包括ATT读取 `0xa0101e78`（mask `0x15`）、ATT写入 `0xa0101ee8`（mask `0x2a`），以及GATT通知检查 `0xa00dc2a2`（mask `0x14`）。这说明新增受保护特征会被现有路径拒绝；不能只给属性设置ENCRYPT位或让INFO返回 `peerEncrypted=true`。

芯片厂商 [GATT实现](https://github.com/bouffalolab/bl_iot_sdk/blob/master/components/network/ble/blestack/src/host/gatt.c) 与 [连接实现](https://github.com/bouffalolab/bl_iot_sdk/blob/master/components/network/ble/blestack/src/host/conn.c) 的无SMP分支有相同行为；本地固定Git blob分别为 `653c5e417690adba9f14f1e6cdc57bcc732e0ee2`、`076c37475e44e6fe4648c59e26c98972d53021c7`。**这是未启用SMP的强证据，不等于证明整个ROM不存在任何SMP或控制器加密代码，也不证明公开BL602 SDK与AP01 NuttX/BL618的ABI一致。**

证据为 `security-checker-execution.json`、`security-checker-windows.json`、`security-manifest.json`。下一步的实现前置条件已经从“寻找一个配对函数”改为核实并补齐同一蓝牙host内的SMP、安全事件和密钥持久化：L2CAP固定通道、HCI加密事件、连接结构/生命周期及密码学随机源必须逐一接通。不可通过跳过该函数、删除权限位、明文发token或并行启动第二个host来冒充完成。此处没有生成实验ROM，新增设备端RAM/栈/持久字节均为0；安全协议移植成本尚未知。

## 安全合同与双进程归属

沿用 `bridge/crates/ble/src/lib.rs` 的 `request_device_token → token_link_info → pair_probe → request_token_on_peripheral`，以及MCP按完整Wi-Fi MAC保存设备令牌的入口。发现名称 `CodexStatus-` 和CoreBluetooth UUID只是候选，必须读取完整Wi-Fi MAC；MAC不符立即拒绝，未开物理窗口不发起首次绑定。配对完成后重新读取连接真实加密与bond状态，再通过受保护AUTH请求取得32位十六进制设备token。现有Bridge可以复用客户端流程，但原厂AP01不提供本项目GATT合同，改扫描名称不能让它获得授权。

实现时 INFO 可公开最小身份，AUTH、返回token的STATUS读取/通知及CCC必须核对当前连接。断连清除分片、订阅和临时结果，不能把上一连接的token留给下一连接。米家的应用层认证不作为标准BLE链路加密或持久bond证据。小米[底层API说明](https://miecosystem.github.io/mijia_ble_api/)中的GATTS初始化/通知/回调定义仅作接口对照，不证明此0031构建提供通用实现或SMP。

绑定、密钥、token、身份、会话及OTA认证归稳定常驻部分；业务进程停止不得释放其回调、队列或锁。OTA进程拥有有界接收/查询/安装；业务进程负责日常HTTP和绘图，显示仍经原厂线程。保留米家蓝牙任务和配网。两个执行单元共享原厂系统，不承诺进程内存隔离或设备fallback。

## 尚未闭环的实施入口

1. 原厂ATT安全检查无条件拒绝已由原指令执行确认；标准SMP、安全级别变化、peer identity和持久bond仍未接通。先补齐并验证同一host的安全能力，再实现绑定服务，不能用米家token、明文GATT或伪造布尔字段替代。
2. 新服务属性布局、注册时机、原厂回调共存/卸载与物理旋钮配对窗口仍需真实ABI。现有 `firmware/ap01` ABI1同步候选禁止长期线程/回调，不能直接把新服务塞进同步App。
3. 完成接口核验后才实现常驻绑定服务、原子密钥记录和Bridge错误链回归；再验未授权读取/过期窗口/重启重连/错误MAC/跨连接token隔离/米家共存。最终实机须以设备回读身份及认证状态验收。

待办唯一归 C9，本记录不把候选引用或宿主自检写成设备已绑定。

## 本轮实机只读检查

用户确认设备在当前Mac附近并上电，提供IP `192.168.31.207`。ARP返回 `34:F0:15:57:21:0B`，用户已确认与米家Wi-Fi MAC一致；作为后续连接的预期身份，尚未写入Bridge设备登记。根页面GET在9 ms内连接拒绝，不能据此推导整个网络/UDP不可用。

2026-10-14 10:06:30 CST，CoreBluetooth在poweredOn下扫描12秒，未匹配名称含AP01或广播服务数据含该MAC正/反序的候选。只证明本次过滤条件下没有目标；不证明设备没有蓝牙、没有GATT或无法进入广播。诊断工具没有连接、配对、订阅、读写特征或重置米家绑定。私有证据 `live-advertisements.json` 和 `scan.swift`。没有设备身份/认证回读，不能标记实机绑定成功。

10:30:46 CST，扩大至25秒扫描，同时匹配FE95服务、AP01/CUKTECH名称和确认的MAC正/反序：收到135个唯一外设，目标候选0。只保留候选及总数，不保存其他外设身份；证据 `live-advertisements-expanded.json`、`scan-expanded.swift`。扫描本身正常，仍无可确认的连接目标；未要求用户解除米家绑定或复位。
