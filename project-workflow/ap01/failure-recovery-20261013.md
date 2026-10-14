# 0031 自定义 App / ROM 失败恢复评估

> 2026-10-14更新：用户明确设备不存在fallback，后续实现不依赖自动回退。下文Boot2条件分支只保留为历史通用研究，不作为AP01可用路径。主方案改为0031上OTA与常规通信/绘图双进程；OTA独立存活及故障范围待实现/实测，仍不保证底座损坏可恢复。

2026-10-13。应用户要求独立子代理只读评估；未访问设备、凭据或刷机。目标为0031变体，0041仅对照。结论：**App 校验失败的退回路径已可离线验证；整个底座 ROM 不能启动时，尚无已验证的不开壳恢复方案，难度高。**

## 按故障层区分

| 状态 | 可用路径与条件 | 难度/证据边界 |
|---|---|---|
| 固定 XIP 包损坏、ABI/入口不符、分配失败 | 本候选拒绝执行 App，原厂 UI 调用继续 | 低；宿主/RV32覆盖，原厂联网仍须实机验证 |
| 同步 App init 拒绝且 stop 正常 | 释放状态并返回原厂调用 | 低；本候选不运行长期 App，没有独立修复服务 |
| Native App 越界、死锁、非法指令 | 格式校验不能隔离；可能拖垮整个系统 | 高；未实现跨重启尝试计数/安全模式，不承诺下一次启动恢复 |
| 底座仍能联网且原厂 OTA 可用 | 用冻结的原厂0031更新容器走原厂回刷 | 中；上游安装记录不代表本机从自定义版回原厂已成功，需同机演练 |
| 新 ROM bootloop | 如果实际 Boot2 启用回退且尚未标记成功，可能切旧槽 | 工程验证高；本机 Boot2、标记成功时机、次数未知 |
| 双槽均坏，Boot2可用 | Boot2 UART ISP；USB IAP依赖编译配置和板级线路 | 高；没有 AP01 专属 boot/reset/UART 接线或 Type-C 下载态证据 |
| Boot2/双分区表坏 | Boot ROM UART、JTAG或外部Flash编程 | 很高；需拆机/电平与安全状态确认、同机全Flash备份 |

本候选不改原厂固件槽或引导数据，但不存在内存隔离。所谓“包失败返回原厂”只涵盖可正常返回的失败，不能涵盖 CPU 异常、死锁、内存踩踏、断电和底座启动失败。当前 App 没有网络维护入口，也不能在运行中独立替换 XIP 文件。

候选2终审已完成，SHA256`61af60e00475f24f4460e60734d7b93a67fb05bb5b756077ccf9d81d4ba7f849`，RV32模拟12/12通过。正常返回错误、init/stop强制失败仍保留原厂对象/寄存器，唯一workspace释放；不替任意App清理外部副作用。已增加一次原厂日志result marker，但sink配置及同机读取未知，首次刷入前必须取得可观测路径。原厂logger实际帧848B，连entry16B至少864B+格式helper/回调嵌套；304B模拟栈不含它，真实UI任务余栈仍须验证。详细[实现验收](app-implementation-20261013.md)，最终离线汇总`artifacts/ap01-research/app0031-20261013/candidate2/verification.json`。没有因此新增独立救砖能力。

## 双槽和 Type-C 不能直接当救砖保障

原厂更新容器不是地址0起的全Flash备份，不含同机 Boot2、真实分区表、校准/身份和eFuse。静态两份7MiB槽和分区表选择证据见[分区复查](knowledge/0031分区与启动入口复查.md)。保留旧槽提高恢复机会，下一次原生 OTA 又可能覆盖它；不当作永久备份。

厂商通用 OTA [写备用槽并读回SHA后切槽](https://github.com/bouffalolab/bouffalo_sdk/blob/b3c904b47ec0f8487c3b640553cee582b1597925/components/fota/ota/ota.c#L433-L477)。Boot2 [回退逻辑由编译开关控制](https://github.com/bouffalolab/bouffalo_sdk/blob/b3c904b47ec0f8487c3b640553cee582b1597925/examples/boot2_isp/main.c#L542-L593)，应用[标记启动成功会清重试状态](https://github.com/bouffalolab/bouffalo_sdk/blob/b3c904b47ec0f8487c3b640553cee582b1597925/components/utils/partition/partition.c#L337-L365)。这些是芯片族代码，未证明 AP01 启用相同策略。

Boot2 [失败等待入口只有特定配置才含 USB IAP](https://github.com/bouffalolab/bouffalo_sdk/blob/b3c904b47ec0f8487c3b640553cee582b1597925/examples/boot2_isp/main.c#L595-L619)。芯片有USB引脚，不证明背后Type-C接通它们。正常连接枚举未发现AP01串口/USB设备，只是负面样本，见[USB调查](usb-firmware-check-20261013.md)；没有演练下载态。芯片族 [bouffalo_isp](https://github.com/bouffalolab/bouffalo_isp/tree/7caf6444f3631a8e887271295f5c6be74f49dfa9)需要产品提供boot/reset控制，不能照搬成AP01操作步骤。

## 后续恢复设计和首刷证据

稳定底座应在 App 启动前原子记录 pending/尝试次数，下次发现未确认启动先进入内置维护界面；恢复界面、网络、认证、最小资源和更新任务不能依赖单份App/Bundle。Native越界仍不能靠包检查解决。确定0031真实 boot-success 调用后再研究延后确认，不能凭通用SDK改槽状态；不写eFuse。本候选尚未实现这些持久恢复能力。

不开壳首刷至少先核对实际型号/版本/MAC，冻结原厂包和摘要，验证同机原厂0031回刷后网络/米家/UI恢复。它仍不覆盖“ROM不联网”。要把底座失败纳入可恢复范围，需要拆机核实Flash/GND/电平/UART/boot/reset等入口，只读识别安全状态，对同机整片Flash读两次比对并演练独立恢复。此处是待办，不是已授权执行或完成的步骤。

首次刷入仍按[方案P0–P5](first-flash-plan-20261013.md)另行验收。若用户后续选择不开壳首刷，需明确接受底座启动失败时可能没有现成恢复步骤；本阶段只构建/测试，不刷机。
