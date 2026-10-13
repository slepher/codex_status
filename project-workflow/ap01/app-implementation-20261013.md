# 0031 自定义 App 变体：实现、构建与验收

2026-10-13。按用户授权继续实现加载器/运行ABI并构建候选ROM，**不刷机**；不访问账号/设备，不重启Bridge，不提交。基线/已有准备见[索引](preparation-index-20261013.md)，执行待办归C9。

## 计划

1. 固定0031输入及原件指纹，复用已有XIP载荷区域/原厂调用包装，核对真实构建工具与补丁接入点。
2. 实现最小版本化C运行ABI、受限包校验、App上下文初始化、调用与退出/失败清理；代码与数据内存分开，不能把一般堆当作已证明的可执行内存。
3. 生成底座与示例App、链接图、原厂接入补丁和完整BFNP候选；原长度/型号/版本/头及恢复结构保持，逐字节限定修改区间。
4. 宿主边界/分配失败/生命周期测试，RV32编译/ABI及实际产物离线核验；用可用模拟执行补验证寄存器/调用与原厂继续路径。构建和测试记录真实阶段、限制及SHA。
5. 记录首次刷入候选、残余实机验收项；独立子代理审查失败恢复难度。

## 任务与状态

- [x] 查回准备与只读构建/ABI发现；现有工具链支持RV32 C，交叉C++前端缺失，宿主gcc/g++可用。
- [x] 固定XIP代码模型、同步ABI1/包加载器与原厂调用包装；长期运行/文件装载仍未实现。
- [x] 最小App完整ROM候选、构建清单与资源/哈希。
- [x] 宿主边界/故障/生命周期、实际RV32候选和镜像门禁测试。
- [x] 独立恢复评估及候选2代码/产物终审，见[恢复方案](failure-recovery-20261013.md)。

## 审阅边界

已有证据支持固定地址XIP载荷，未证明从/resource将任意RV32代码加载到堆执行。若采用XIP初版，须明确这与独立文件App-only更新的区别；不将GIF或尺寸对象冒充执行验证。候选构建可完成，硬件是否接受/启动必须留待用户后续授权刷机。包/模块失败应返回原厂路径；原生双槽不等于已验证自动回退。

## 实现与构建结果

源码在[firmware/ap01](../../firmware/ap01/README.md)，构建器/模拟执行器在[工具说明](../../tools/ap01/README.md)。APX1显式112B头、同步ABI1 init/step/stop、版本/长度服务表、data/BSS初始化、16B对齐及raw指针free已实现。App禁止异步保留栈上API；通用runtime允许stop失败留状态重试，首版固定App的stop正常返回，未实现长期App宿主和跨重启安全模式。

原厂接入VA `0xa00b2732`，文件偏移通过VA−`0x9ffff000`计算为`0x0b3732`，原6B为`5285eff02079`。包装`lv_obj_create(0xa00c1ec6)`，返回对象保留，NULL时跳过App。跳板VA`0xa001b0ac`，底座VA`0xa02465d0`，包VA`0xa024c000`，App text在`0xa024c070`。该原厂UI创建是否每次启动触发须实机验证，不能宣称是完整boot hook。

最终候选2：`artifacts/ap01-research/app0031-20261013/candidate2/ap01-1.0.2_0031-app-xip.bin`，**6,804,520 B**；MD5 `15c0c8b1ae5b17024d30dbed38ab178a`；SHA256 **`61af60e00475f24f4460e60734d7b93a67fb05bb5b756077ccf9d81d4ba7f849`**。未刷入，版本保持0031。`manifest.json`记录构建时原件/工具/源码指纹和修改区间，ELF/map/dis/sections/su已保存，读回SHA一致。初版candidate与失败中间产物保留，不是最终刷入候选。

底座text 2,144 B；App text180 B，包300 B（头112+text180+data8；BSS8不落盘）。payload含地址间填充23,388 B，占60,934 B空间，余37,546 B；完整Bridge容量另测。State16 B、malloc31 B，无可写全局段/重定位。模拟自定义链栈峰304 B（entry16+boot208+validate80），**排除原厂logger**；原厂log函数实际prologue `addi sp,sp,-0x350`，连entry至少864 B，加其format/helper/callback嵌套栈未知，必须查UI任务余栈。App结束额外heap0、峰31 B；模板/NVS/网络/持久数据均0。

每次正常返回经原厂logger记录`APX1 diag0031 result=%u (0=init-step-stop-ok)`，结果0证明本次init/step/stop和回报全部成功，非0区分拒绝/失败。依据`0031-log-analysis.txt`实际Ghidra原件导出，vararg在a5，缓冲256/512B；若sink未注册会返回。**日志配置及同机读取通道未验证，首次刷入前仍需确认可观测路径**，不能用原厂画面不变证明App执行。同步宿主在init-stop/stop错误后强制释放唯一workspace，通用runtime的重试合同不变。包SHA只比较base ID声明，构建器验证原件SHA，不计算运行ROM摘要。

## 已执行测试

- WSL gcc C99 `-Wall -Wextra -Werror -pedantic -fsanitize=address,undefined`编译执行`app_loader.c + test_app_loader.c`，输出`AP01 loader tests passed`；覆盖截断/CRC/ABI/SHA/边界/溢出/入口/预算、分配失败、对齐/data/BSS、零状态、重入、init/step/stop错误及重试释放。
- 现有RV32 GCC16.1.0/全套binutils构建，ELF32 little-endian RISC-V/RVC softfloat、无未解引用/重定位；链接断言拒绝隐含可写段/空间越界。
- WSL Pillow逐帧RGBA、尺寸、时长与loop对比，GIF385834 B→324886 B结果一致；尾14B对齐间隙保留。
- 固定原厂身份/长度/头/40B恢复结构、尾CRC、允许区间外逐字节一致、成品读回通过；区间外破坏即便重算CRC仍拒绝。
- Unicorn2.1.4实际执行候选hook/跳板/加载器/App/真实report及emit_result，**12/12通过**。成功init/step/stop各1次、回报49/8/1、结果0；CRC/ABI/SHA/入口/初始化数据拒绝、malloc失败、NULL对象以及init/stop/init+stop故障注入都保留原厂继续路径。堆填0xA5验证BSS清零，31B分配恰好释放；SP/GP/s0–s11/原厂对象保留。成功22,053条指令，模拟峰栈304 B；日志stub验证固定5参数、字符串和result。证据`candidate2/emulator-report.json`匹配最终SHA。

模拟器stub原厂lv_obj_create/malloc/free/log，不模拟Boot2、custom扩展、Flash缓存、RTOS/UI/网络/WDT；通过不代表本机安装或启动。独立终审未发现固定地址/原厂返回值/GIF边界/同步释放的代码级阻断，保留日志读取、真实余栈及恢复门禁。**完整Bridge App、认证、文件App OTA、持久安全启动仍属C9未完成。**P0/P2–P5实机未执行，不将本候选称作稳定恢复底座。
