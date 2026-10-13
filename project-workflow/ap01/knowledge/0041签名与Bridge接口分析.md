# 0041 签名、首次安装与 Bridge 接口分析

> 2026-10-13部署目标核对：本篇保留当轮0041主线研究结果；实际构建/刷入目标为支持自定义App的0031变体，已有准备见[索引](../preparation-index-20261013.md)。0041仅供对照，不增加0041移植任务。

2026-10-13。0041为主线，0031只作对照。此次只读分析官方文件，没有访问账号、扫描设备、调用设备RPC、生成修改ROM或刷机。设备仍为0031是用户报告，未在此次实机回读。

## 结论与证据层次

0041确实新增应用OTA未签名拒绝分支，而且保留后续证书链、包签名、CRC及启动槽切换路径。当前仍运行0031的设备，沿已公开实测的0031原厂OTA进入修改版，是比先升级0041再研究绕过更有依据的首次安装路径。0041的静态分析尚未证明可用的原厂安装绕过；修改待安装镜像自己的拒绝分支，不能使当前运行的原厂0041接受它。

新版仍有NuttX内置应用表、米家网络任务、原厂界面任务、littlefs资源区和登记到通用堆的4 MiB外部内存，支持保留系统/驱动、增加Bridge设备应用的研究方向。没有证明现成局域网HTTP服务、可直接使用的Bridge GATT或动态可执行文件加载器。此报告提供入口和预算约束，不承诺这些地址已成为稳定SDK ABI。

证据分为：原文件/CRC/结构解析；Capstone候选定位；GNU独立反汇编；Ghidra真实函数反编译与交叉引用；公开芯片SDK条件。字符串命中、通用SDK和不完整伪代码各自保留限制。原始输出均在Git忽略的`artifacts/ap01-research/0041-20261013/`，研究工具在本仓库，第三方克隆只读。

## 镜像身份与地址

| 项目 | 0041主线 | 0031对照 |
| --- | --- | --- |
| 文件长度 | 6,769,704 B | 6,804,520 B |
| SHA256 | 972db4c136c7ed9e24a83c07c1a7fd62040ca018b08ca285216d26b1fee3c6b9 | 8a721fc8ef25458d415b2460e4a251e0503a82f7743fdff85b12612190e5c1cb |
| MD5 | 3c2d962be82c73860daff903178d9b9e | 469a7329d496f81ae8625c4d76ccf56d |
| 头CRC32，存储值与复算一致 | 0x436ccbb5 | 0x3cd13b95 |
| 40 B恢复记录起点 | 文件0x674c00 | 文件0x67d400 |
| 恢复CRC，存储值与复算一致 | 0x47fb9315 | 0x7ffd605b |
| startup GP | 0x62fc34c0 | 0x62fc34c0 |

代码和flash常量使用`VA = 文件偏移 + 0x9ffff000`，入口文件0x1000对应0xa0000000。这不是烧录偏移，也不适用于拷贝到RAM后的全部对象。0041比0031小34,816 B；相对7 MiB槽的文件尺寸差额570,328 B不能直接当可追加代码量。

来源：[最终候选定位](../../../artifacts/ap01-research/0041-20261013/refined/comparison.json)、[启动结构清单](../../../artifacts/ap01-research/0041-20261013/refined/inventory.json)。候选扫描止于文件0x1be7a0/0x1ba700，不是全ROM控制流。未解码项包括内嵌数据，不能把其数量称为厂商指令数量。GCC后缀共享让`mcu_signed_file+4`成为`signed_file`的实际地址，工具已处理内部字符串起点；只查零终止字符串起点会漏掉该引用。

## REA、Ghidra与分析质量

实际安装REA 6.4.0。`inspect_firmware_regions`因Windows/Linux Binwalk前置返回不可用；`open_binary`对原厂裸BIN拒绝格式。实际安装的`GhidraProvider.js`只接纳x86/x86_64/arm/arm64/mips，Windows另外限制PE。因此搬同版REA到WSL不能解决目标支持。

Ghidra 12.1.4自身已安装RISC-V语言。直接Windows headless以`BinaryLoader`、`RISCV:LE:32:default:gcc`、base 0x9ffff000成功导入，两版分别保存在本仓库私有项目中。`Ap01Export.java`执行种子反汇编、自动分析、函数调用/入引用查询和真实decompiler导出。不是REA反编译，没有伪造REA Evidence ID。WSL已有Java，但Ghidra launcher实际未接受该版本；此次有效Ghidra结果来自Windows现有JDK21.0.10。

Provider通常提供会话、查询和证据组织；同一引擎同配置的单次反编译不因接入方式变好。但仅按skill临时扫描，容易漏掉调用链和交叉引用，整次调查不能自动声称等效REA。此次补做了实际Ghidra分析与GNU核对，仍不是所有REA工作流能力的替代。

已有第三方[GhidraMCP](https://github.com/LaurieWired/GhidraMCP)，属于独立MCP接入；REA的[IDA接入](https://github.com/morluto/rea/blob/main/docs/ida-provider.md)可复用`ida-pro-mcp`，但需要符合目标与许可条件的IDA。未安装、未验证它们处理此AP01的效果。修复REA需处理裸镜像识别、架构/profile、导入地址/入口与回归，而不只是增加白名单。

实际质量限制：0041启动函数0xa0029b1e在0xa0029c20遇坏指令；tag解析0xa00703ce在0xa0070896截断；若干custom opcode被输出成无语义的`custom0()`。函数参数类型和间接调用也可能不完整。相关段落不能以伪代码外观流畅认定已解释，需厂商指令语义/专用工具链继续核对。接入另一个MCP并不会自动补上处理器语义。

## 0041签名拒绝与0031区别

0041参数解析入口0xa006f5ce；0031对应入口0xa0068320，均有0xb0 B直接栈帧。两版均读取`app_url`、`signed_file`、`cert_verify`、`original_length`、`partial_enable`及`app_force`。

0041的关键路径由GNU和Ghidra交叉确认：

```text
0xa006f7ec/7f0  调0xa01976d4，解析signed_file到s2+0x0c
0xa006f7f4      lbu a5, 12(s2)
0xa006f7fc      bnez a5, 0xa006f8aa   // 非零才继续
0xa006f802      li s3, -10
0xa006f804      日志等级不允许打印时，直接到0xa006f70e清理
0xa006f81a      引用0xa025b410未签名拒绝日志
0xa006f824      j 0xa006f70e
0xa006f70e      清理JSON；非零结果不进入成功路径，最终返回-10
```

即使日志被关闭，拒绝仍然发生。`app_force`解析位于此检查之后，不能据它声称能越过unsigned拒绝。`cert_verify=none/optional`也不等于固件签名不检查：该字段和包签名是不同层次。

0031同一位置直接继续解析cert_verify，再仅在signed_file非零时要求original_length；false没有新版这一立即拒绝。不能由局部分支证明0031任意包均可安装，但它与公开修改版采用signed_file=false的安装记录相符。

证据：[0041 Ghidra参数解析](../../../artifacts/ap01-research/0041-20261013/0041-ghidra-pass1.txt)、[0031对照](../../../artifacts/ap01-research/0041-20261013/0031-ghidra-parser-control.txt)、[GNU完整窗口](../../../artifacts/ap01-research/0041-20261013/0041-ota-parser-gnu.txt)。

## 后续验签、CRC与切槽

| 0041入口 | 已见路径 | 限制 |
| --- | --- | --- |
| 0xa00703ce | 读取固件tag、版本/model/pdid/算法字段；安装入口处理返回-13为downgrade error | 中途坏指令；不完整证明所有版本比较、SHA覆盖范围及边界 |
| 0xa0074b94 | 由总长度减original_length得到扩展区长度，读入扩展区；寻找END CERTIFICATE，分隔证书及签名，调0xa0199e66 | custom0影响字节读取/游标语义；未验证自造包或运行结果 |
| 0xa0199e66 | 内置米家根证书、解析包证书、逐证书验证；组织名检查允许xiaomi或匹配产品标识；调用0xa00786a0验证签名 | 根CA算法不等于每张叶证书算法；不完整数据映射保留 |
| 0xa00786a0 → 0xa01972fa | 公钥上下文分派到函数表+0x10；调用点传摘要类型6与32 B摘要 | 与mbedTLS风格pk_verify相符；未独立命名所有算法实现/恢复动态叶证书 |
| 0xa0074d30 | tag验证→signed标志决定是否调用签名验证→0xa00b51d8→board控制0xff13→清理/后续完成 | 不是该函数开始才第一次写Flash；下载写入路径尚未完整追踪 |
| 0xa00b51d8 | 流式128 B读取，比较尾4 B CRC；相关标记首字节翻转/恢复 | 这是CRC/标记恢复，不应误叫完整ROM烧录函数 |
| 0xa0055a14的0xff13分支 | 取ro.boot.update_slot，更新分区项/计数，选择0xe000或0xf000表记录写入路径 | custom0和RAM表字段仍待细化；未证明断电恢复/Boot2重试 |
| 0xa0074ea2 | 名为ota_mcu的独立验证/写magic路径，使用不同结构字段与偏移8 | 不能将MCU/PogoPin路径当作主应用安装绕过 |

证据：[验证与安装调用](../../../artifacts/ap01-research/0041-20261013/0041-ghidra-pass2.txt)、[证书/CRC/分区](../../../artifacts/ap01-research/0041-20261013/0041-ghidra-pass3.txt)、[分派与网络](../../../artifacts/ap01-research/0041-20261013/0041-ghidra-pass5.txt)。伪代码中的FUN名称仅代表地址，表中语义是按调用/参数/日志综合判断。

根证书文件偏移0x259ba4..0x259dd6，代码直接引用。OpenSSL解析为CA:TRUE、O=Mijia Root/C=CN、P-256公开密钥、证书签名ecdsa-with-SHA256。[公开证书](../../../artifacts/ap01-research/0041-20261013/mijia-root-public.pem)、[独立解析](../../../artifacts/ap01-research/0041-20261013/mijia-root-openssl.txt)。

所以当前证据是设备拥有验签公钥/信任根，不能推导固件签名私钥位于设备。固件签名通常由发布方完成；设备身份密钥即使存在也不等于发布方签名私钥。无法由该公开密钥直接反推出私钥；实际研究价值在实现、信任配置与可用安装入口，不在暴力破解标准椭圆曲线。未证明此0041实现存在可利用缺陷。

当前下载的BIN以40 B恢复记录结束，尚未单独取得并验证该OTA调用所需的证书/签名外包装。将signed_file改成true、补一个original_length或重算CRC，都不构成合法签名。应用OTA验签也不能替代Boot ROM/Boot2/eFuse安全启动调查；该文件没有同机Boot2和eFuse现场。

## 首次烧入修改ROM的评定

| 入口 | 当前评定 | 必须满足/尚缺证据 |
| --- | --- | --- |
| 当前0031的原厂OTA安装修改版 | 最有依据的候选：作者实测+本地旧版false分支继续；本轮未刷机 | 核实实际0031、目标身份、已有修改版包格式与完整同机备份/恢复方法；不能直接套旧版补丁到0041 |
| 先升级0041，再用原有unsigned OTA | 不可作为现成方案；已确认被-10拒绝 | 需另证合法签名、不同安装入口或实际可用缺陷 |
| 在0041修改包内取消拒绝/验签 | 可作为已取得执行/写入能力之后的改造，不能解决第一次安装 | 原厂运行镜像不会执行尚未安装的补丁 |
| signed_file=true配自签证书/CRC | 未成立 | 证书链受内置米家根约束，SHA/CRC与数字签名作用不同 |
| UART/USB ROM ISP | 芯片族有能力，AP01板级可行性未知 | Boot引脚/按键与数据线路、实际枚举、Boot2、eFuse/安全限制、镜像烧录格式与偏移 |
| 已有NSH/应用/资源入口直接部署新程序 | 可研究，尚无首次安装路径证明 | 控制台是否外露、命令权限、动态loader及持久启动能力未证实；GIF资源加载不自动等于可执行加载 |

公开[厂商Boot2示例](https://github.com/bouffalolab/bouffalo_sdk/blob/b3c904b47ec0f8487c3b640553cee582b1597925/examples/boot2_isp/main.c)有读取eFuse、选择分区以及条件UART/USB loader逻辑；[ISP工具](https://github.com/bouffalolab/bouffalo_isp/blob/7caf6444f3631a8e887271295f5c6be74f49dfa9/README.md)列出BL616/BL618。这些只能证明芯片/通用代码具备路线，不证明AP01后置Type-C接出下载态或其安全配置放行。该段来自独立只读资料核对，不是从应用BIN反编译出来的Boot2。

## 新版资源和应用入口

0041启动ROMFS位于文件0x1bee24，480 B，rcS正文文件0x1bef04、97 B；卷与rcS头校验复算均为零。脚本仍为ifup wlan0后后台运行miio_client、mible_mesh_common_bt和lvgl_ui_main，再退出NSH。

内置表文件0x1be934起，8条16 B记录及零终止项：

| 应用 | 0041入口 | 优先级 | 配置栈 |
| --- | --- | ---: | ---: |
| lvgl_ui_main | 0xa005cc1e | 100 | 65536 B |
| mible_mesh_common_bt | 0xa0062d28 | 100 | 6144 B |
| miio_client | 0xa0067296 | 100 | 6144 B |
| nsh | 0xa0032230 | 100 | 4096 B |
| renew | 0xa005a62a | 100 | 4096 B |
| setlogmask | 0xa0058f5e | 100 | 4096 B |
| sh | 0xa00323f6 | 100 | 4096 B |
| wapi | 0xa0077024 | 100 | 4096 B |

入口相比0031并非统一加固定偏移；旧result_loader链接地址、全局对象和UI挂钩必须逐项迁移，不能改一个ROM版本字符串复用。三项后台应用配置栈仍合计77,824 B，这不是已测峰值，不能据某个入口的小栈帧贸然缩栈。

0041分区初始化0xa0059762实际读取E000/F000两份596 B表，校验并比较age，然后要求fw起点10000或7b0000且长度700000。它创建fw1/fw2各7 MiB、ota 512 KiB、crash 48 KiB、reserve 64 KiB、resource 0xeb0000起0x140000=1.25 MiB，并将/resource挂载littlefs。资源区容量不等于空闲量；512 KiB ota节点也不能容纳本次完整BIN。原厂固件槽机制保留，本项目应用/Bundle仍用单份覆盖方案。

0xa0029640..0xa0029688加载同一通用堆对象0x62fc6aec，分别增加内部区域及0xa8000000..0xa8400000共4 MiB；区域登记调用0xa002e9c2。[GNU参数核对](../../../artifacts/ap01-research/0041-20261013/0041-heap-region-gnu.txt)。这是0041自己的机器码，不沿用0031地址。窗口起点不证明整个函数边界；pass2的该候选函数出现unaff_s0，不能当完整启动伪代码。登记4 MiB不是已测空闲4 MiB，仍缺运行时最小空闲量、连续块、显示DMA/对齐要求。

## Bridge可复用接口与设计取舍

0041网络初始化0xa0066296由miio_client入口调用。0xa0066558..655e以(2,1,0)调0xa00268c4创建socket；0xa00666e6..6704构造`02 00 d4 32 7f 00 00 01`并调0xa003f446，即127.0.0.1:54322的内部绑定。0xa0066748/674c调用监听封装0xa01a18ce。该内部通道不直接对电脑开放，也尚未证明使用HTTP；不宜直接改成全网卡暴露。

当前候选可复用底座：内核socket/bind/listen/read/write、任务与定时器、littlefs文件读写、原厂显示与输入、保留米家配网后已经建立的Wi-Fi。仅网络绑定路径和表内入口达到本轮参数/调用证据；其他完整函数ABI、超时、线程/锁要求仍需逐项确认。未定位能直接注册业务路由的原厂LAN HTTP server，不能把HTTP客户端/通用listen代码当服务端。

对接当前本仓库合同应增加独立设备服务：认证POST /api/status、/claim、/api/data、/api/plan、/api/activate与Bundle begin/chunk/commit，明确MAC/principal/owner/context/事务及真实ACK；CompiledTemplate遵循compiler ABI2和同源渲染。米家绑定不自动授予Bridge权限。当前token只经绑定加密BLE取得，AP01要实现符合合同的配对/GATT，不能用原厂账号token替代或放宽HTTP鉴权。

AP01按用户要求设计为Wi-Fi常驻、业务BLE只配对；原厂蓝牙任务还承担米家功能，不能据此删除整个栈。当前DeviceCapabilities含max_light_s等字段，但没有单独always_on字段；需要在后续实现中明确能力和PowerPlan适配，不能照搬ESP32睡眠/NVS代码。AP01 hardware_verified保持false直到实际验收。

预算沿既有[设计](../design.md)方法重算：320x240单帧RGB565 153,600 B；两张单色帧合计19,200 B；16行RGB565暂存10,240 B。4 MiB堆登记让内存方向更有希望，但未取得真实余量，不能承诺接入成功。Bundle容量由/resource实时空闲、文件系统开销、压缩解码长度和安装峰值共同决定，不固定成1.25 MiB。下载期间应用继续，传完校验后进入内置界面，再覆盖单份应用/Bundle；不加项目自管A/B、不占备用固件槽作Bundle空间。

当前Bridge字段依据`bridge/crates/core/src/platform/model.rs`、`device_client.rs`、`compile.rs`；本轮未修改Bridge协议或AP01驱动。可重用的是平台合同与渲染逻辑，需重新实现NuttX设备适配层，不能把ESP32镜像直接烧到BL618。

## 验收、残余问题与下一步

已完成两版固定SHA、头CRC/恢复CRC、地址与GP复算；最终对照工具自检通过，包括旧版变异拒绝、PC/跳转、ROMFS/应用表与新版后缀共享引用。Ghidra导入、OTA parser/验证/证书/分区/网络函数导出成功；GNU核对unsigned拒绝和4 MiB参数；OpenSSL独立解析公开根证书。工具不产生ROM。所有Ghidra进程已退出，原件未写入。

此次三个问题的状态：应用层新增签名限制已回答；首次进入路径已作条件评定，硬件ISP/安全启动仍未知；Bridge资源与入口已有新版证据，但端到端ABI和实机预算仍部分回答。不存在已验证0041安装绕过，也没有伪装可部署的新ROM。

后续优先项：补T-Head指令语义并复核被截断tag/版本/摘要链；恢复下载时写Flash及切槽事务；在不升级0031前提下核对本机身份、可恢复备份和下载态线路/eFuse；实机读取资源占用和堆余量后，才决定首次修改包及Bridge应用大小。凭据继续保存在被Git忽略的位置，此次未读取。
