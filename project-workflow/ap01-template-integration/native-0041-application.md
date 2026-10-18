# 0041 原生应用接线与完整候选

**当前最新为2026-10-19 01:36 候选22：独立OTA稳定槽已接入并模拟通过。** 见[含稳定槽的新ROM](release-0041-ota-slots.md)；下方候选16作为此前基础版本保留，不含直接稳定槽回切。全程未刷机。

**2026-10-19 最终状态：候选16完整模拟验收通过并生成正式待刷ROM。** 见[交付、维护协议、资源和边界](release-0041.md)及[状态](status.md)。原生31/32/33覆盖八模板安装/四场景、全镜像维护升级/保护区、同NOR新槽启动及持久回执恢复。正式产物6981976 B，SHA256 `6adb31dcaa0a35c77a38c729d298764f0c72ae7e729b6c77a21dd0b7beeb56bc`；未刷机。下方候选01/12和待验描述为历史阶段。

2026-10-18，C9。继续制作完整0041 ROM；不刷设备，不访问串口/凭据，不启动或替换生产Bridge。原件始终只读。

## 实现

- BundleStore、画布和渲染状态由调用方持有。ESP/宿主保留默认入口；原生端口没有全局LittleFS、默认渲染器、ArduinoJson分配器单例或隐式初始化段。
- 0041 VFS绑定使用原件inode/LittleFS/fd/fsync/statfs/mkdir/rename，根目录`/resource/codex-status`，不格式化。写后sync、原子rename后读回。原生compression=0，只收完整raw Bundle，保留全部GIF。
- 激活回执与活动模板/context同一BCS2原子元数据保存；Bundle使用持久receipt。提交读回失败查原事务，未确认返回unknown并围栏。APB1绑定保存也读回；只有UI绘制后的新按下/释放能确认。
- 复用CompactFrame及共享Data/Bundle/activate核心；MAC/principal/token/session/owner逐层校验。会话按实际principal分配；围栏覆盖chunk查询参数和操作token。不接受PowerPlan。
- HTTP上限2048 B头/4096 B请求及绝对期限，拒绝重复长度、TE、Expect、NUL和歧义查询。维护18080、业务18081、UDP8767为独立任务；维护不取得业务渲染锁。只有pairing/info允许公开读取。
- 原生UI只hook主界面注册回调的一处8 B调用。先执行原厂条件导航，在首尾环回加入全部模板；数据不抢焦点，覆盖层/初始化优先。堆状态经callback/timer user_data传递，不借未知RAM或对象尾部。原子队列送帧到UI线程，失效原厂图像缓存后再释放旧帧。

## 验证与候选

- [原生端口宿主ASan/UBSan](../../artifacts/ap01-platform-integration/device/native-app-report.json)：四位码/旧释放拒绝、完整122792 B双模板Bundle分片/重放、丢失rename回执读回、激活重启/原请求重放、独立维护、UDP及HTTP解析通过。不是0041 ABI执行证明。
- 共享回归33项通过：`device/native-shared-regression.log`。28个RV32对象无可写静态/构造/TLS/GOT段；完整XIP ELF无未解析符号，libgcc/libm用匹配rv32imafc/ilp32f多库。[对象审计](../../artifacts/ap01-platform-integration/device/native-core-audit.json)、[链接](../../artifacts/ap01-platform-integration/device/native-link-report.json)。源码变更后需重编更新指纹。
- 完整候选01：`artifacts/ap01-platform-integration/device/ap01-native-0041-candidate-01.bin`，**6961976 B**，SHA256 **5240924aabb8d91ddd076f2cb04b0542af5b565b4babfb678e948d68ed1b4b20**。固定原件SHA/调用前置字节/完整重建/7MiB槽容量/尾记录长度与CRC通过；所有GIF、ROMFS、rcS、Boot头及非hook原字节保留。**flashable=false，尚未验收为可刷ROM。**
- 候选01真实QEMU执行到`0xa005d180`注入，并原生创建32768 B栈业务线程tid29；50宿主秒无guest异常。服务未开：GDB证明原厂Wi-Fi MAC getter返回全0，ROM正确拒绝。`native-qemu-01/02.json`早期passed只指旧runner安全观察，其`native_service_probe_passed=false`，不算功能通过。
- 固定最新QEMU **a549e66f1f7680e30a7d491733ff2b95ff2de5a9f972dd70b7fa306b1c50da18**（旁仓0031 P4/P5后续模型），不是旧0041的7246b77a引擎。复制到私有证据。后续显式`--model-wifi-mac` fixture继续验证，不放宽ROM检查；只用loopback，不证明空口Wi-Fi。

## 23:22 原生执行与像素里程碑

- 候选12：6964808 B，SHA256 `2e5bd39ff9778c871cc202f55a5023a45e333846774693b98616ed47282f274d`，清单在`device/ap01-native-0041-candidate-12.manifest.json`。仍为发布验收候选，不是已部署ROM。
- `native-qemu-23/24.json`：实际RV32线程/VFS、三端口、401、UDP、真实原厂绘制后的旋钮确认、绑定保存、122792 B双模板Bundle及Data通过。23专门核对同context/seq1，避免22早期连续切页后缺省帧被误记为Data显示。24为四场景，seq1/2/3/4逐个等待实际绘制。
- 四场景`native-qemu-24.{codex,weekly-only,zero,full}.pixels.json`：原生I4经过原厂LVGL解码后的RGB565帧缓冲与源帧逐像素差异0。缺5h居中、RC2保留、零值0%及RC0、满圆环已目视。23同源宿主在相同UTC/原生环境下CRC一致`d7b9ef9f`、像素差异0；早期131像素差异仅来自宿主误设UTC+8的重置时间文字，不是抗锯齿误差。
- 模拟条件明确：固定合成Wi-Fi MAC；原厂LVGL真实事件派发`a00d831a`；仅在原厂失效回调已等待fbdev就绪时，经原厂`a00806d0`继续刷新。libuv原本删除刷新定时器，模型未送达POLLOUT，不能称硬件已验证。LCD/SPI仍为Logo；交付截图是实际原厂LVGL帧缓冲，未伪造ACK或填屏。
- 修复了原厂裁剪字体缺字/自回退循环、面板挂载层级、真正对象删除`a00ded6c`、UI退出旧Bundle确认、原厂缓存失效及短绑定锁竞争。保留早期失败证据；`a00fa082`是错误的旧模拟事件入口，`a00d7806`仅类析构，均已纠正。
- 最新原生宿主ASan/UBSan通过；共享回归54通过。隔离Bridge已重建，38732800 B，SHA256 `24bfc2ff77525d9d08663f112508e233bddd58ac705564b685dcd4d303f88138`，未启停生产实例。
- 总资源尚待收口：Runtime2016 B（含Business1760/BundleStore700，不能重复相加）、App536 B；单个函数最大栈1040 B不等于调用链。源新增重复熵重启context保护及调用方持有的本地时间结果，尚待新候选。
- OTA范围按旁仓最新`tooling/firmware-modules/README.md`及`knowledge/0031-tail-physical-success-20261018.md`：保留传统入口，人工顺序使用两通道；跨通道共享锁不作部署前提。0031已证槽尾执行不能推广为0041实机已验，0041原始头/跨度/PEC保持不动。

## 开放验收

原生重启恢复、1–8页循环/上限、维护升级适配及未知结果查询、总栈/堆和最终候选验收仍需完成。物理BSP/LCD链为模型限制，不拿宿主NuttX替代ROM UI。验收后才改变flashable标记；未执行实机写入。
