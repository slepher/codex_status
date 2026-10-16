# REA RISC-V 裸 BIN 接入计划

日期：2026-10-16。待办归 [backlog C9](../../docs/roadmap/backlog.md)。状态：**计划已记录，未实施、未安装、未运行真实Ghidra验证**。

## 目标与授权边界

让REA通过现有Ghidra provider分析调用者明确指定的RV32/RV64裸BIN，复用ARM的BinaryLoader、会话、查询、Evidence和清理路径。AP01完整镜像还需要加载基址与代码入口分离。此次授权只记录计划；实现位置是同父目录`/Users/slepher/project/rea`，本仓库保留计划和验收结论。

不实现CPU自动识别、反编译器、厂商指令语义、仿真或多段固件重建；不重启Bridge、不访问设备、不改ROM/分区/NVS。此前“不额外逆向AP01”的约束继续有效；将来可对已有镜像做工具接入核验，不因此扩大逆向范围。源码克隆不等于已安装REA运行环境。

## 已核对基线与依据

- 本地源码：`/Users/slepher/project/rea`，浅克隆，`main`提交`d72d856f58d4747c5eb4307426e80d20934e017c`。
- 发行标签：`rea-agents-6.5.0`，提交`3b79af28e80785afce9f5998d8f03289402a1847`。下列裸BIN枚举、语言映射、RISC-V准入及Java准备脚本在两份基线一致。
- [#2063](https://github.com/morluto/rea/issues/2063) / [PR #2151](https://github.com/morluto/rea/pull/2151)：已实现的RISC-V ELF接入，不包含裸BIN。
- [#4246](https://github.com/morluto/rea/issues/4246)：开放设计讨论，提出调用者选择Ghidra language/compiler及独立base address；截至本次搜索未确认对应实现。实施前重新核对，优先复用已合入或可用补丁。
- [PR #1868](https://github.com/morluto/rea/pull/1868)：原始裸BIN接入；[#4245](https://github.com/morluto/rea/issues/4245)的架构推测不属于本计划。

| REA源码 | 实际行为与增量 |
|---|---|
| `src/domain/rawBinary.ts` | 裸BIN只接纳x86/x86_64/arm/arm64/6502；新增RISC-V准入、显式位宽及对应范围/对齐检查 |
| `src/domain/binaryTargetTypes.ts` | raw-binary变体排除RISC-V；需表达不带ELF元数据的RISC-V目标 |
| `src/ghidra/GhidraRiscvProfile.ts`、`GhidraPspProfile.ts` | RISC-V现被要求具有ELF元数据；明确区分raw与ELF，保留现有ELF准入 |
| `src/ghidra/GhidraRawBinaryLanguage.ts` | ARM只映射现成language/compiler；补`RISCV:LE:32:default`、`RISCV:LE:64:default`及`gcc` |
| `src/ghidra/GhidraAnalysisProfile.ts`、`GhidraProviderClient.ts` | 绑定配置、Evidence及缓存身份；raw当前拒绝环境变量language override，不能靠设置变量绕过 |
| `src/ghidra/GhidraLauncher.ts` | 通用传递BinaryLoader/baseAddr/processor/cspec，无需重写加载器 |
| `bridge/ghidra/ReaGhidraPrepareRaw.java` | 要求文件偏移0就是入口，再调用Ghidra反汇编/建函数；基址与入口分离时同步改此校验 |
| `src/ghidra/GhidraLoadImageValues.ts` | 独立核对文件哈希、语言、映射和入口；分离参数后须分别校验 |

## 范围与接口原则

首版以固定的小端RV32/RV64路径达到现有ARM裸BIN的支持程度。沿用`riscv`架构族，显式传递32/64位；具体字段名在实现时按现有合同统一，不从地址大小或文件后缀猜位宽。CLI、MCP、目标类型、输出schema及profile使用同一份解释。

若#4246已有可复用通用language接入，先评估其真实验证与目标适用性，再决定复用；本计划不以该讨论获批准或实现为前提，不自行扩展到所有Ghidra语言。

基址与入口分离采用向后兼容原则：新增可选`base_address`，省略时保留当前“base=entry”的行为；提供时文件偏移0映射到base，代码入口独立且须位于有文件支持的映射内。映射范围按位宽检查，入口按选定RISC-V模型检查对齐，压缩指令入口不能套用ARM的4字节限制。BinaryLoader报告的Program image base与实际文件映射起点分开记录。

固定`gcc` compiler spec是明确的分析约定，不证明裸BIN原始ABI。记录所选模型及ABI未知限制；不伪造ELF头、e_flags或double-float声明。soft/single-float的精确参数恢复与厂商扩展指令完整性单列限制，不借此宣称全部RISC-V固件正确反编译。

首轮真实验收以Linux/macOS支持的Ghidra路径为准。Windows当前连ARM裸BIN也拒绝，平台准入另列后续范围，不以POSIX测试宣称Windows支持。

## 实施顺序与交付

1. **核对环境与上游**：检查REA工作区、AGENTS/贡献规则、#4246及相关PR；按仓库锁定工具链准备环境。记录REA/Ghidra/JDK版本与提交，构建自动创建目录按本项目操作要求预先提权创建；不改全局MCP配置。
2. **标准裸BIN接入**：补类型/准入、RV32/RV64映射、profile、输出与错误合同；查询、反编译、交叉引用、进程生命周期直接复用。同步修正所有假定`architecture=riscv`必有`target.riscv`的消费者。
3. **基址/入口分离**：更新输入、目标/profile身份、launcher参数、Java准备脚本、独立加载校验及快照重放。可独立评审，不与通用CPU自动识别绑定。
4. **验证与交接**：先用公开源码生成RV32/RV64小样本；相关单元/合同检查通过后跑真实CLI/MCP。AP01仅核验已知映射与已有函数。按REA贡献规范完成所需检查，回填版本、命令、证据及剩余限制。未经要求不提交、不发布PR、不安装替换现有工具。

交付预期：REA源码改动及必要回归、公开可再生样本、真实CLI/MCP报告、使用参数与限制说明。本仓库只登记计划/结果；私有原始输出按`artifacts/`规则保存，原厂BIN不入库。

## 验收矩阵

| 场景 | 判据 |
|---|---|
| RV32、RV64标准裸BIN | CLI/MCP均能打开，回读正确language/compiler与位宽；已知指令、直接调用、全局引用及函数伪代码与样本预期一致 |
| 非零base、entry不等于base | 文件偏移/内存地址往返一致；入口在指定偏移建立，绝对引用指向链接地址，不把头部当入口 |
| 参数与边界失败 | 缺位宽、非法位宽/地址、空文件、溢出、无效对齐、入口越界，返回可定位错误；不换架构重试或伪报成功 |
| profile/缓存 | 改位宽、language、compiler、base或entry均不能复用不匹配快照；等价地址拼写仍可复用相同语义 |
| 源文件与清理 | 前后SHA256不变；取消、失败、关闭正确清理自有进程/临时项目，失败保留可诊断信息 |
| 既有功能 | ARM裸BIN行为及RISC-V ELF拒绝/接纳范围保持一致；原ARM/x86等相关真实验证按改动影响运行 |
| AP01完整镜像 | 已有文件offset0映射到`0x9ffff000`，offset`0x1000`入口为`0xa0000000`；核对既有函数地址/引用，不以未解释custom指令宣称完整质量 |

AP01映射依据：[已有0041分析](knowledge/0041签名与Bridge接口分析.md)。头部/入口分离的公开样本先通过，再使用私有镜像；不靠裁剪或伪造ELF代替完整BIN验收。

## 时间预算、状态与评审

标准RV32/RV64接入以**约1人日**为首轮实现和针对性验证预算；假设Ghidra/JDK已可用、复用现有流程，不包含上游等待、环境安装和厂商指令研究。AP01完整镜像的基址/入口分离独立记录工时；同日能否闭环由真实样本结果决定，不把估算作为验收证据。

当前已完成：克隆、6.5.0/当前源码定向对照、上游issue搜索和本计划。尚无代码改动、测试或工具部署证据；下一步由backlog C9承接，不把计划写成已支持。

计划评审结论：接入不需要重写Ghidra处理器；主要风险是现有RISC-V类型与ELF元数据耦合，以及base/entry合一的合同。实施复核须确认两者已贯通CLI/MCP/Evidence/快照，且未扩大为未知ABI或Windows/厂商指令支持承诺。
