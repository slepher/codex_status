# C9 实施交接

## 2026-10-11 部署执行

用户最新指令授权 Bridge 部署和 Note4 刷机，解除此前仅打包等待限制；诊断进程已交接并停止设备访问。执行顺序：认证核对 Note4 MAC/现有配置；隔离构建 Bridge；停止匹配 watchdog/计划任务后备份旧 exe 与完整 data；替换并由标准计划任务启动；以唯一 request_id 排队 .80 OTA；核验运行镜像完整哈希、身份、配置和压缩能力。保留原生 OTA 双槽，不发布新模板，不改另一设备。原故障根因及 BOOT/GATT 后续事项仍未解决。

部署前认证：Note4 7C4FADB93408 / .78 / ota_1，configured=true，codex-status-a；证据 artifacts/c9-deploy-before.private.json。新 Bridge 构建成功，SHA256 BD641F1E5ADC0870063B329F77F6734C22F4E22F8AB418185C081850CBE7C2FB；日志 artifacts/c9-bridge-deploy-build.log。

16:27:53部署完成：Bridge PID63064，计划任务和8765/8766正常。旧exe/data备份 artifacts/bridge-before-c9-20261011.exe、bridge-before-c9-20261011-data.zip（260项、ZIP CRC通过）。Note4 OTA 0972b435，唯一请求c9-note4-0080-20261011；16:25:39入队、16:25:48开始、16:26:06已收上传确认、16:27:53完整镜像认证一致，attempt_count=1。运行.80/ota_0，USB continuous/light，bundle_compression=1、原文上限4194304；旧codex-status-a已迁移，Data441 displayed。未发布新模板；压缩安装实机峰值和掉电矩阵仍待验。证据 artifacts/c9-ota-status.private.json、c9-deploy-after.private.json。以下“未部署”为此前候选阶段记录。

2026-10-11 16:10（香港时间）。Note4 单份 Bundle 与统一分块 zlib 已完成候选构建。下载时继续运行旧内容，校验后才停止并覆盖，使用内置恢复界面；原生固件 OTA 双槽保留。用户要求烧录排队到故障解决后，本轮未部署、操作设备、重启 Bridge 或提交。并行诊断记录显示工具复位后 .78 已恢复响应，但原故障根因尚未确认，见[诊断记录](../bridge-macos/note4-stall-20261011.md)。

候选 **0.18.80-note4 / zectrix-note4-b**：[BIN](../../artifacts/note4-0.18.80-zlib-blocks.bin)、[匹配 ELF](../../artifacts/note4-0.18.80-zlib-blocks.elf)。BIN **1,920,688 B**，SHA256 `4CCD39B2C751C44DD6C89527742592656B3495F9700A4121D98BFDEB6C0A7B83`。Flash section 1,868,291 B，静态 RAM 119,148 B；相对 .79 BIN +3,616 B，静态RAM不变。版本和目标 marker已核对。.79旧候选及.78诊断ELF保留；.pio/build现在对应.80，不能用于解释.78现场。

实现：CSZ1 / zlib-blocks-v1 为独立16KiB zlib块，Note4复用芯片ROM miniz，下载和驻留都保留压缩正文，按块解析/编译/加载字体；物理包与原文处理分别限额。Bridge仅在设备声明能力且压缩获益时冻结压缩字节，旧任务/旧设备继续原JSON，重试不重新编码。canonical JSON/hash及原生OTA格式不变。协议与所有权见[设计](design.md)。

验证：最终core/render **217通过**，HTTP模拟器 **62通过**，app **80通过/3条件忽略**，合计359通过。覆盖损坏/超限包保留旧内容、跨块/重启字体、八模板压缩安装、丢ACK跨重启回执及能力变化后冻结字节。初次新增测试辅助整包JSON解析失败，改字段过滤后复测通过。匹配ELF Wi-Fi栈52项及固件模块检查通过。日志：artifacts/c9-compression-final-tests.log、c9-compression-http-tests.log、c9-compression-tests.log（含app结果及已修正的首次辅助测试失败）、c9-compression-store-tests.log、c9-compression-note4-build.log、c9-compression-wifi-stack.log、c9-compression-modules.log；候选资源明细c9-compression-candidate.json。

资源：生产编码器Note4字体Bundle样本10608→3134B（减70.5%，编译缓存另计，不代表任意包）。目标编译器确认miniz状态10992B；单解码器含输入/输出及实际块索引，4MiB原文上限时44916B堆内存，另计分配器/解析/编译/字体/网络。直接帧：BundlePayload32–64B、bsInstall992B、loadFonts480B；不是全调用链或实机峰值保证。

待验仅在backlog C9管理：真实掉电、堆峰值、功耗和显示矩阵仍未完成；后续运行版本与故障调查以[PROGRESS](../../PROGRESS.md)及对应专项为准，以上.80记录是历史部署证据。AP01按相同CSZ1约定设计，0031原厂DEFLATE已有表及代码引用，可调用接口和应用加载仍待实机；尚未构建AP01 Bridge接入ROM。2026-10-12文档同步见[复核记录](ap01-doc-sync-20261012.md)。
