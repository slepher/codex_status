# 接手任务：基于0031制作并模拟验证方案B ROM

你接手的是 `D:\Documents\PlatformIO\Projects\codex_status`。请遵循根目录AGENTS.md，先读PROGRESS.md，但注意：**本prompt中的用户最新指令已将当前目标从0041界面显示切换为0031方案B的离线模拟验证。** 不要接着排查0041的Logo显示，也不要只回答可行性或提供计划。

## 用户目标与授权

用户希望保留0031原厂系统、驱动、米家配网和Wi-Fi后台，通过“编译独立模块＋原厂符号绝对绑定＋二进制Patch”加入认证HTTP OTA通道：电脑向设备的18080端口推送完整ROM，不必每次使用小米云端指令和电脑HTTPS拉取服务器。

用户最新明确要求：“先为我模拟验证”。应持续推进到具体候选与实际运行证据：

1. 0031原件在必要Mock下启动并持续执行；不要求所有外设完整工作。
2. 构建**0031专用**HTTP OTA注入ROM候选，记录全部修改范围、来源与校验。
3. 在QEMU内执行候选代码，验证健康、认证、错误处理和真实原厂安装链。
4. A候选上传B→原厂写入/CRC/提交→保存NOR→核验后虚拟复启B→OTA仍可用；再B→A回刷并复启检查，证明通道连续性。

这是离线模拟任务。**不刷实机、不访问设备/串口/小米云端、不读取账号或token缓存、不启停Bridge，不为模拟开公网/局域网固件服务器。** 测试传输只允许宿主loopback。设备当前0031来自用户报告，没有备份或USB恢复途径；不要再次询问备份。

“永久固定底座槽”“开机预切回旧槽”“10秒健康确认”“旋钮强制恢复”都尚未实现或验证，不能把现有OTA通道说成这些能力。此轮先验证0031独立HTTP整ROM更新；如需槽恢复设计，单独列验收，不擅自宣称已完成。普通双槽OTA会轮流覆盖非活动槽，备用槽不等于永久救援底座。

## 当前状态，避免从错误结论开始

- 最近相关提交：`4e04564 Add AP01 QEMU OTA validation and display handoff`。
- **只有0041做过整ROM QEMU OTA往返验证，0031尚未做本轮验证、尚未生成方案B候选。** 不得把0041报告改名或替换哈希当0031证据。
- 上一位agent只完成了资料核对，并按规则提权创建本地证据目录 `artifacts/ap01-research/qemu-0031-20261017/`；目录当前为空。没有改0031适配代码、启动0031 QEMU或读取凭据。
- 工作区有其它既有未提交的SIM/逆向文档和代码，不回退、不清理、不混入本任务。没有已授权的新提交动作，除非用户后续要求，否则只完成工作、更新交接，不自动commit/push。
- 0041有限验证实例均已退出，无可沿用的实时窗口或OTA监听。旧`start-qemu-ap01.py`仅早期暂停实例，不能拿来冒充当前完整启动。

## 精确输入与环境

0031只读原件：

```text
D:/Documents/PlatformIO/Projects/cuktech-ap01-firmware-tools/artifacts/firmware/original/ap01-1.0.2_0031.bin
字节数：6804520
SHA256：8a721fc8ef25458d415b2460e4a251e0503a82f7743fdff85b12612190e5c1cb
```

先亲自核验身份；原件不修改。候选、合成NOR、报告用独立新名称，拒绝覆盖旧证据。它是应用更新容器，不是整片Flash备份，分析VA/文件偏移不是实机烧录地址。

- Windows仓库：`D:/Documents/PlatformIO/Projects/codex_status`。
- WSL发行版：`Ubuntu-24.04`；仓库映射 `/mnt/d/Documents/PlatformIO/Projects/codex_status`。
- QEMU源：`/home/slepher/qemu-ap01-0041/source`，官方v10.1.0固定commit `f8b2f64e2336a28bf0d50b6ef8a7d8c013e9bcf3`。
- 现有构建：`/home/slepher/qemu-ap01-0041/build/qemu-system-riscv32`；最近0041引擎SHA256 `7246b77aa123e3b785d8c1093b4ee2bac19a3fb1b489bc0d1035963d87e18b9d`。改模型后重新记录哈希，不把旧报告套到新引擎。
- 交叉工具链：旁仓 `cuktech-ap01-firmware-tools/tools/docker/toolchain/bin/riscv64-unknown-elf-*`；只读复用。
- Bouffalo SDK旁仓：`D:/Documents/PlatformIO/Projects/bouffalo_sdk`，commit `41bf338a57be53919d99f6c6715a96100428c97d`；是部分克隆，不要广泛git grep触发大量lazy fetch。
- 当前已有0031证据目录可直接使用；任何新目录必须按AGENTS提权预建。WSL操作使用适当提权工具权限。不得为了测试例行clean或破坏已有QEMU源码/构建。

## 定向阅读入口

不要通读全部历史，只读与本轮有关的部分：

1. `project-workflow/ap01/qemu-ota-final-20261017.md`：0041闭环、报告序列、哈希及模拟边界。
2. `project-workflow/ap01/qemu-display-handoff-20261017.md`：现有模型环境、0041完整运行命令与已知坑；本轮复用环境，不续作其显示目标。
3. `project-workflow/ap01/app-implementation-20261013.md`：0031已有XIP诊断模块、空间/ABI及仅离线验收边界。
4. `project-workflow/ap01/first-flash-plan-20261013.md`：0031更新格式/恢复边界，重点不是执行其中实机P0/P2。
5. `project-workflow/ap01/knowledge/0031网络监听与HTTP复查.md`：0031socket/bind/listen证据；“未找到LAN HTTP服务”不能说成已穷尽证明没有。
6. `docs/agent-operations.md`的构建与工具节；待办唯一来源`docs/roadmap/backlog.md` C9。

已有0031最小XIP诊断候选在 `artifacts/ap01-research/app0031-20261013/candidate2/`，不是HTTP OTA底座。`tools/ap01/build_app_rom.py`提供空间/固定基线参考：原GIF偏移`0x1f80ac`、385834 B，既有优化释放60934 B、载荷区域偏移`0x2475d0`。必须重新核验使用的具体布局与允许范围，不能把这些数字当作任意可写空洞。

## 需要复用和适配的代码

### 通用核心与0041端口

- `firmware/ap01/ota_channel.c/.h`：已验证的有限HTTP核心，尽量复用，不重写协议。
- `firmware/ap01/ota_0041.c/.ld`：仅作端口参考，不能直接改名套0031地址。
- `tools/ap01/build-ota-channel-0041.py/.sh`：可复用构建/限定Patch/清单重建模式，0031要单独核实镜像身份、尺寸、manifest、启动钩子和CRC。
- `tools/ap01/run-qemu-ota-channel.py`：payload范围限定的loopback socket API提供器，HTTP/线程/原厂安装逻辑仍由RV32执行。
- `tools/ap01/verify-qemu-ota-boot.py`、`test-ota-boot-verifier.py`：独立核对真实NOR提交及负向案例。

0041模块复用malloc/free、socket/bind/listen/accept/recv/send/close、poll、setsockopt、errno、线程封装、monotonic time、原厂inactive-slot open/write/CRC/close、board-app提交与复位。**所有0031地址、参数布局、返回值、常量和线程生命周期必须有本版本证据。** 特别核对poll结构、timeval、float ABI、gp、小数据段、栈预算、写入函数会修改首字节缓冲等。

已知0031的内部socket/bind/listen线索在上述知识文档，尚不能视为完整导入表。现有0041功能不等于0031ABI已证实。

### QEMU与明确的版本写死位置

- `tools/ap01/qemu-ap01.c`、`qemu-ap01-*.h`、`qemu-ap01-cpu.patch`、`apply-qemu-ap01-cpu.py`。
- `build-qemu-ap01.sh`、`build-qemu-bootrom-shim.sh`、`qemu-bootrom-shim.c/.ld`。
- `run-qemu-ap01.py`、`build-qemu-flash-fixture.py`、`qemu_scan_mock.py`。

已确认：`run-qemu-ap01.py`和fixture builder当前默认严格接受0041哈希/尺寸；应增加明确0031准入，保留0041默认与负向检查，不能放宽成任意ROM。

已确认：`qemu-ap01.c`的PEC mock检查**0041固定文件偏移`0x66f000`的`PEC:`**，并将API表overlay在**VA`0xa066e000`**。0031必须重新定位/核对PEC调用合同与表，不可假定同位置。PEC提供器是显式替代，不是已解密原厂受保护代码。

其它必须查的版本依赖：boot trap地址、BLE enable入口、ADC设备lookup入口、扫描start/status/results入口、guest tick地址、builtin/rcS/ROMFS偏移、OTA dispatcher、分区表RAM位置、flash布局。`qemu_scan_mock.py`目前也写死0041三API和tick地址，不能原样给0031使用。

优先小范围版本profile/明确参数复用，避免复制整套模型或搭建泛用框架。按真实0031执行轨迹补缺口，不伪造ready、MGMR完成或吞掉异常。

## 0041已知坑，可复用经验但要核对0031

- 原厂scan启动失败在0041会安排4000ms重启，不能将“返回ENODEV”误当稳定降级。0041正常profile用20ms有限空扫描事务：pending→done→空ENOENT，原caller容忍。0031行为要独立查。
- 0041父builtin main返回时，创建成功的子线程未获得运行；v5用原生poll(NULL,0,1000)休眠保活，子线程8192 B。不要重引入这个问题。
- payload独立OTA必须和原厂OTA写入互斥，不能把原厂busy查询当原子锁；0041候选禁用了云OTA dispatcher。0031要明确保留/排他的策略及后果。
- 固件校验、原厂CRC、NOR写入、分区提交不能固定返回成功。只允许非活动槽和非活动分区表写，保护活动镜像、Boot2、保留区。
- 保存NOR后按CRC/age/active核验，再以**同一NOR**虚拟复启；禁止重造fixture冒充更新后启动。真实Boot2缺失就如实标虚拟选择。
- HTTP成功＋错误凭据＋截断回复不能独自判PASS；还要排除异常、非OTA来源复位、输入文件被改变。记录ff13真实返回和ff04来源，不能把环境重启当OTA重启。
- 当前0041模块只接受固定本版本完整镜像；0031大小是6804520，不能继续使用6769704，更不能套用户历史示例中的固定original_length。
- key/密码/token/原始敏感响应不打印、不入库；认证参数从私有文件读取。HTTP直推仍须认证，不能为了跑通放开鉴权。

## 工作流程、测试和交付

先在`project-workflow/ap01/`写0031专项plan/task/status/review，再实施。复用现有分析/REA与反汇编证据，核对原件；缺资料时先使用本机已有工具，不随意下载安装或操作旁仓。

分阶段记录原件启动、端口映射、候选构建、实际HTTP、安装/复启链。每阶段记录真实命令、引擎/原件/候选/shim/NOR哈希、错误PC/MMIO/失败阶段，未知明确写未知。启动模拟可设置较长的有界宿主调试预算，保持固件请求有限期限；工具调用及时yield，持续向用户说明关键进展。

对修改运行适当测试：HTTP核心故障/边界、0031镜像准入/允许Patch重建、原厂ABI及实际RV32执行、NOR保护/完整性/中断、提交与复启。先使必要链路成立；全外设/米家协议恢复、0041UI、Bridge业务都不是本轮前置。

成功交付应包含：

- 0031专用候选A/B、清单、链接图和资源预算，均为模拟候选、非部署，原件保持不变。
- 0031原件与候选实际运行证据；认证HTTP和至少一次安装→真实NOR→独立核对→虚拟复启后通道继续可用，尽量完成A→B→A。
- 对未模拟硬件、物理Wi-Fi、首次0031安装、Boot2/掉电/回滚及固定底座缺口的准确边界。
- 更新PROGRESS.md为0031本轮最新交接，关联backlog C9和专项报告；不要继续让进度卡把0041界面当当前目标。

若碰到缺少必要材料的真实阻碍，继续完成不受阻部分，留下具体错误、已有尝试、可复现步骤和剩余条件。不能把仅编译成功、纯宿主单元测试或一张SIM截图当作0031整ROM OTA模拟通过，也不要因一次失败就停在泛泛建议。
