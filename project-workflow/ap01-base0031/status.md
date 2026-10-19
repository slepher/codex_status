# 0031基础ROM修复与部署

**2026-10-20 已实机部署。** 用户明确要求刷入，并纠正基础ROM建立前不能用旧版本固定槽位保护阻断；采用标准OTA保留当前可用原厂0031槽1，向非活动槽0安装。唯一miIO.ota获ok，一次完整6822344 B下载后自动重启；没有派发恢复、重试或第二次OTA。认证build53574b55382d55a514ef5b7767db66d9、BootROM实际映射0x11000/运行槽0、物理分区表age12/active0，OTAidle。用户确认显示/GIF与旋钮切页正常。临时HTTPS已关闭，Bridge未启停，未提交Git。

已部署ROM：`artifacts/ap01-base0031-20261019/ap01-base0031-v2-rom.bin`，6822344 B，SHA256 `d843aae4e097c31ce06a315c62e1a66f4fb3bf9299db289b6f31562d0ef2231b`。匹配ELF `base0031-v2.elf`，SHA256 `1de09d9c2b94df5097e2baf31bb85510aee80596ec40da697f4e41f0f28ff43c`。manifest已标simulator_verified/flashable/physical_device_verified=true，实机范围明确列出；独立OTA整包再传输仍仅模拟通过。只有维护/OTA/诊断，无业务应用、模板或C2。

## 修复

校验器按编码常量逐字节比较恢复标记，不再将完整连续标记编入模块。正式构建强制检查整个BIN恰好两处，第二处必须为末尾40 B恢复记录；校验长度、头CRC和完整ROM CRC仍保留。原版归档未修改。

独立OTA与诊断物理读取共享非阻塞Flash访问围栏，先获得围栏才开写入；STATUS忙时从RAM返回运行映射/分区表并明确physical_reads=busy、物理读取rc=-16，不把不可读冒称镜像损坏；FLASH明确ERR busy，不排队占满请求任务；LOGS继续RAM快照。读围栏在网络发送前释放，写入失败关闭后释放，可继续救援；已提交/未知结果仍保留事务写入围栏。原厂OTA与独立OTA仍需顺序使用，此围栏不冒称跨两种OTA的全局锁。

## 验证

- ASan/UBSan：52项请求故障、15组诊断/日志/客户端回归通过；实际STATUS格式器64次忙响应无物理Flash读取、FLASH忙拒绝、解除占用后恢复读取，失败开写释放围栏通过。
- 镜像正反向完整性、额外标记构建拒绝通过。原厂256 B启动头、385834 B GIF及描述符、PEC和传统OTA入口保持；构建器逐字节限制原件修改只在原有维护启动钩子/rcS/ROMFS校验三处。
- 原生NuttX TCP（无宿主socket替换）：OTA占用时13次STATUS、FLASH忙、后续LOGS、并发写拒绝通过；中断未完成上传后仍能完整上传正式ROM。`native-upload-verification.json`确认生产SHA目标槽逐字节匹配，原运行槽、Boot2、保留区不变，ff13返回0、仅提交非活动槽1。
- 直接执行独立核对的0031原厂恢复函数0xa0063f00：旧三标记包返回-1且分区表/两槽不变；新两标记正式ROM返回0、分区age1→2/槽0→1，两ROM槽字节不变。结果见native-recovery-good-v4/extra-marker-v4及acceptance.json。
- 分别使用OTA提交和原厂恢复提交的同一NOR快照，从槽1启动正式ROM，均通过；原厂UI启动，无guest exception/意外reset。此为模型中的槽位选择与启动，不是物理Boot2验收。
- 按最终正式源码重编BIN和ELF逐字节可复现。源码快照release-source.zip、release-provenance.json、resources.json及acceptance.json同产物目录。

## 成本与边界

RV32 server8264→8272 B，connection保持1384 B；空/1/2/4请求的RAM增量均8 B，无持久格式/日志容量增量。STATUS静态帧3616→3648 B，启动帧80 B不变，请求栈16 KiB不变；真实库调用链高水位、LCD、掉电和Flash墙钟时延未实机验证。

原始native安全窗口报告在预期软复位处结束，因此passed=false；没有改写这些记录。独立验收以真实CRC读回、提交返回、NOR前后比较和新槽启动判断。宿主socket代理失败、初版恢复探针因未继承任务环境导致的空指针故障、载荷编码再次产生标记而被构建拒绝的试验均保留；修正探针后复验，未放宽ROM校验。模拟器明确建模无线/Boot2/GPIO/Flash，不声称物理验收。

## 源码位置

0031真实源码在旁仓库`../cuktech-ap01-firmware/tooling/firmware-modules/`及`tooling/scripts/`，本轮在已有修改上增量修复，未回退旧工作。当前仓库tools/test-ap01-base0031.py、prepare-ap01-base0031.py、verify-ap01-base0031.py保存复现入口。


## 实机安装与只读核验

证据：同产物目录install-task.json、install-https-events.jsonl、install-final-verification.json、installed-readonly-verification.json、installed-boot-log.jsonl。5次STATUS为47/47/94/62/63 ms，认证构建ID和实际槽位不变；FLASH读取不含凭据的manifest前64 B及恢复尾，均匹配候选；保留槽1原版头256 B及恢复尾匹配原厂0031。未读manifest凭据区域，未导出完整活动槽/PEC，不能冒称物理整ROM SHA逐字节验证。没有为测试再刷写槽1或触发原厂恢复；本轮实际证明原厂正常OTA安装与新维护接口可用，独立OTA占用期间的诊断、恢复和掉电矩阵仍保留模拟/硬件边界。
