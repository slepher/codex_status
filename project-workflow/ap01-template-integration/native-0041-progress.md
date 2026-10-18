# 0041 原生渲染迁移与 ABI 续作

2026-10-18 20:32 香港时间。用户要求继续制作可刷 ROM；本里程碑仍未生成完整 ROM，不刷机、不操作生产 Bridge，GIF 页面保留。

## 实现与资源

`TemplateRenderer` 与 `PaintCanvas` 使用调用方持有的状态。画布尺寸、字体视图/绑定及裁剪计数属于渲染对象；ESP/宿主自由函数仍转发默认实例，AP01原生构建不生成默认实例。两份静态CtTemplate临时缓冲改为冷路径按需堆分配并检查失败，正常运行继续使用持久编译模板。CT ABI2、字体索引、BindKind序号及canonical CRC `fb18016d`未变。

原生目标显式声明0041/palette4；位图字体描述符只读，ArduinoJson用请求局部分配器，base64端口去掉可写查找表。原生String不依赖std::string/locale。4bpp目标明确接受BW/alpha4、拒绝gray4，与Rust准入一致。

现有GCC8.4、rv32imafc/ilp32f实际构建绑定、配对、渲染器、Paint、字体校验和五份位图字体，共11个对象。拒绝非空data/bss/sdata/sbss/got/TLS/初始化及析构段。合并对象仍为**ET_REL**，有libc/libgcc/new/time导入，**不是可执行ELF或ROM**。281268 B，SHA256 `3c5db68fae133898b17acfcd0c597f0599dd3cca83de5e61332623c17a361595`。

RV32实测PAINT24 B、渲染状态228 B、编译模板11321 B；画面38400 B、字体49028 B另计。渲染器/Paint代码与只读数据分别54287/4464 B，字体校验2747 B，最终链接会改变大小。渲染器逐函数栈最大512 B、Paint560 B，**不是总调用链栈或整机峰值**。JSON/VFS/UI/分配器开销、原厂堆栈余量仍须完整ELF及原生运行核对。

## 验证

- 33项通过：lib15、Bundle2、compiled11、font_asset4、Unicode1；新增不同分辨率两个渲染对象交替编译/绘制，确认不串画布和帧指针。[日志](../../artifacts/ap01-platform-integration/device/render-regression.log)。
- 原生配置与自有String在WSL宿主实际执行；覆盖自拼接、内部源重分配、复制/移动及64位数字边界。双桶/缺5h/零值/满额四组与冻结基准均逐像素差异0，保留RC和缺桶重排。[报告](../../artifacts/ap01-platform-integration/device/native-render-report.json)。这是宿主运行，不是RV32 CPU、NuttX或原ROM执行证明。
- RV32编译参数、源/对象哈希、段、导入和逐函数栈：[native-core-audit.json](../../artifacts/ap01-platform-integration/device/native-core-audit.json)。
- 最初受限环境测试因私有文件写入失败，按操作要求提权隔离运行后通过。原生像素最初162点差异来自SYNC输入23:59与夹具12:40不同，统一输入后为0。

## 0041接线依据

原件SHA256 `972db4c136c7ed9e24a83c07c1a7fd62040ca018b08ca285216d26b1fee3c6b9`前后不变。共享REA generic模型在memset后错误截断；新增离线脚本核对冻结私有模型哈希，在独立Ghidra工程使用`RISCV:LE:32:ap01thead2`及已有返回证明修复脚本，不修改旁仓分析环境或原件。

本版候选：`0xa00a5ed6`图片创建/源/对齐包装（调用`0xa00ed0a8`/`0xa00ed560`）；`0xa00b2f6c`窗口创建；`0xa019d422`动画切换窗口；`0xa00b2ff4`读取active索引；`0xa01ad654`注册事件；`0xa00d8af6`读取key事件；UI主入口`0xa005cc1e`。`0xa0025c24`是**file指针级seek**，不能当fd级lseek调用。[八项原始导出](../../artifacts/ap01-platform-integration/device/0041-ui-abi-candidates.json)。

先前导出另见timer_create`0xa008bc64`、obj_create`0xa00d91c6`、child_count`0xa00dd144`。都是离线ABI候选；UV/启动路径`0xa005a736`仍有bad-instruction/函数体边界限制，不使用推测对象尾部保存私有状态。独立工程、模型指纹及修复记录在`device/ghidra-native-0041/`。后续须核对原始指令/前置字节/调用链，不套0031地址，也不把decompiler文字超出body的部分当作边界证明。

## 交付边界

**没有完整0041 ROM路径或ROM哈希。** 下一步仍归C9：原生VFS、独立维护/业务任务、共享Bundle状态和原厂UI消息/旋钮/显示ACK接线，随后OTA仲裁、完整镜像启动/恢复及总资源检查。合并对象、宿主程序和旧OTA候选均不代替完整功能ROM。
