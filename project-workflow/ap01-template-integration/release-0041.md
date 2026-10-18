# AP01 0041 完整 ROM

**后续版本提示：当前待刷选择为[含独立OTA稳定槽的0041 ROM](release-0041-ota-slots.md)。** 本页候选16为此前基础版本，不含直接稳定槽回切；历史镜像和证据保留。

本轮只制作、模拟验收和归档，不访问 AP01，不刷机，不替换或启动生产 Bridge。以下结论取代此前“原生 HTTP/VFS/UI 尚未接线”的阶段记录；物理验收仍归 backlog C9。

最终文件：`artifacts/ap01-platform-integration/device/ap01-0041-codex-status-20261019.bin`，**6,981,976 B**，SHA256 **`6adb31dcaa0a35c77a38c729d298764f0c72ae7e729b6c77a21dd0b7beeb56bc`**。匹配候选16 ELF 473536 B，SHA256 `52909ff9eb07767e06ddf039c5c60faa47c5d26fa1a93b8f31c151b381c1a7c8`。同名 zip 包含 ROM、SHA256、manifest、acceptance、匹配 ELF/map/audit/link/payload 和本说明。

最终原生证据为 `device/native-qemu-31.json`（八模板/四画面）、`32.json`（完整升级）、`33.json`（同NOR新槽恢复）；均为上述同一 ROM。原厂活动 LVGL 缓冲四场景零像素差异，缺5h/RC2、真实0%/RC0、满值均复核。core162/render15/v2_state21、ASan/UBSan、存储8组合通过。配套隔离 Bridge 38736384 B，SHA256 `808adc1e7307d09eaf2ccebcc4e19bf4c7f1e30bd0d66130724d88fd6a2d5546`，未启动/部署。

## 交付内容

以 SHA256 `972db4c136c7ed9e24a83c07c1a7fd62040ca018b08ca285216d26b1fee3c6b9` 的原厂 0041 为唯一基线。完整镜像保留原始启动头、入口、PEC、ROMFS、rcS、全部九个原厂页面和五个 GIF，只替换一处八字节 UI 注册调用，在镜像尾部追加自有 XIP 模块并重新计算恢复尾部长度及 CRC。总长小于原生 7 MiB 固件槽。它不是仅供注入的模块或需要手工拼接的补丁。

原生线程独立提供维护 HTTP `18080`、业务 HTTP `18081` 和 UDP 发现 `8767`。Bridge 使用已知地址、UDP、多网卡邻居补查，认证核对完整 Wi-Fi MAC 后更新地址；常规局域网流程无需输入 IP，不依赖 mDNS。四位双屏确认由原厂 UI 线程绘制，必须新的旋钮按下/释放才能绑定。配对不自动发布，显式 claim 后仍需用户发布完整 Bundle。

业务使用共享 CompactFrame、owner/session、完整 Bundle、持久激活回执和完整 Data 快照。AP01 常在线，不生成/下发 PowerPlan。1–8 模板按顺序接入原厂正反切页，保留所有原厂页面。彩色圆环、alpha4 字体、缺少 5 小时桶时的完整居中布局、独立 RC 和真实零值均保留。中文字体随完整 Bundle 发布，不内置 ROM。

持久文件位于已挂载的 `/resource/codex-status`，不格式化原厂卷。绑定 APB1、Bundle BCS2、维护升级 APF1 均有显式版本、边界及校验；原子替换后读回验证。Bundle 缺失与损坏分别报告 `unconfigured` / `recovery`，损坏时业务认证入口仍可接收完整 Bundle 修复，内置界面继续可用。显示分配失败有界退出并返回失败状态。

## 维护升级

新增维护端点 `/api/firmware/status`、`begin`、`chunk`、`commit`、事务查询和显式 `reboot`。全部要求已有绑定 token，写操作核对 principal、MAC、request_id 和 revision。最多 32 KiB 分片，确切重复可重放，变化的重复明确拒绝；接收超时五分钟。镜像验证使用 0041 模型位置 `0x4f00`、恢复 CRC、入口及完整长度界限。调用原厂 Flash open/write/verify 和 `boardctl(0xff13)` 提交至非活动原生槽；仅响应发送完毕且连接关闭后才允许显式重启。

业务在固件事务期间明确拒绝，维护健康保持可读。已发出的不确定提交标为 unknown，禁止换 ID 重发；重启后用正在运行的 build_id 确认原回执。接收中断、验证失败、保存失败及格式损坏均有负面测试。传统 OTA 保留，按此前约定人工顺序使用两个通道，未增加跨通道共享锁。

**0031 旧独立维护接收器使用不同模型偏移 `0x4d48`，不能承诺它直接接受本 0041 镜像。** 此次只验证新 0041 接收器的完整更新及新槽恢复。实际首刷方式和设备身份须在另行授权刷机时核对；没有操作旧设备来验证跨版本通道。

## 资源与成本

0041 `/resource` 为 `0x140000`（1.25 MiB）；原生 `/data` 为 `0x710000` 起的 64 KiB，不能混算。五个 GIF 共 2,153,808 B，位于 ROM/XIP，删除不能直接增加 `/resource`，因此全部保留。模拟实装八模板完整 raw Bundle 为 293,743 B，含编译缓存 384,575 B；保留旧内容的更新准入模型约 827,039 B。物理设备已有文件、LittleFS 块放大和真实 free 未测；运行时按 statfs 实数准入，容量不足明确拒绝。原生仅通告 raw/compression=0，不冒充支持 CSZ1。

RV32 `ilp32f` 实际 sizeof：Runtime 2072 B（已含 Business 1768 B 和 Store 700 B，不能重复相加），Application 576 B，Paint 24 B，Renderer 228 B，单份 CtTemplate 11,321 B。模板安装逐条分配一份编译对象，不将八份对象压入栈或常驻空数组。全部 30 个输入对象通过无可写静态段、TLS、隐式构造和 GOT 检查；最终 ELF 无未解析导入。

绑定持久字节：空 16、单条典型 121 / 上限 281、八条典型 856 / 上限 2136；提交堆上限 6816 B（不含分配器开销）。维护服务常驻 68 B，事务按需 496 B；APF1 单文件最多 1036 B、临时与最终共 2072 B。HTTP 小缓冲 6145 B，维护分片额外最多 32768 B；整份 6.98 MB ROM 流式写入，不复制进 RAM。写 Flash 使用 1024 B 临时栈块。

每幅 packed 图 38400 B，带原厂 LVGL 调色板的图 38464 B；替换时旧/新图片与新 packed 帧合计 115328 B，三份随 Bundle 字体共 49028 B。模板解析 JSON 有 128 KiB 上限、过滤器 8 KiB；字体单条解析 80 KiB，元数据 8 KiB。分配按实际使用增长并检查失败；这些是各项成本，不将互斥阶段简单相加充当测得的峰值。原厂双 RGB565 帧缓冲额外 307200 B 属既有基线。

自有业务/维护/发现任务分别 32768 / 16384 / 8192 B，原厂 UI 65536 B。匹配最终 RV32 `.su` 最大自有单帧 1056 B（Flash write）、Bundle install 1040 B、配对 UI 832 B；`.su` 不是整链高水位。模拟报告记录 socket 观测点 SP 差值，它也是下界，不冒充物理峰值。完整安装、更新、八模板遍历及重启运行无异常；物理堆/栈高水位仍待实机验收。Note4 的共享代码另外用匹配 Xtensa ELF 执行 Wi-Fi 栈闸门。

## 验收边界

最终 ROM、匹配 ELF、审计、链接图、逐像素报告、宿主 ASan/UBSan、Bridge 回归及模拟事务证据由 `tools/accept-ap01-rom.py` 校验后登记到 ROM 同名 manifest / acceptance / sha256 文件。`flashable=true` 表示镜像格式及原生模拟流程验收通过，不表示已上机；`hardware_verified=false` 保留。

QEMU 执行完整 RV32 内核、原厂 VFS/LittleFS、原厂 LVGL 与自有应用。MAC、loopback socket、LVGL 输入、fbdev 就绪、Boot2 和硬件外设明确使用模型。像素来自原厂 LVGL RGB565 帧缓冲，四场景当前显示缓冲与共享渲染结果逐像素一致；LCD/SPI 模型仍显示 Logo，不能作为物理屏验证。原生 OTA 写入完整非活动槽、保护区对比以及从同一 NOR 的新槽恢复另外验证，不把宿主 fake Flash 结果当作原厂 Flash ABI 证据。

生产 Bridge、Note4 和 AP01 均未部署此次候选。macOS WKWebView 因当前 Windows 环境缺少 swiftc 仍未验，不纳入本轮 ROM 模拟结论。
