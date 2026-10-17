# 0041 原件启动与独立OTA模拟闭环

## 结果与验收

用户要求的离线顺序已完成：原厂0041在必要Mock中持续执行→unsigned候选上注入独立OTA模块→A安装B→同一NOR虚拟复启B→B回刷A→同一NOR复启A后通道仍可用。没有访问/刷写实机或启停Bridge；验证阶段未提交，后续按用户要求将源码、测试和交接记录提交，私有artifact不入库。

报告均在`artifacts/ap01-research/qemu-0041-20261017/`。最终QEMU10.1.0引擎SHA256 `7246b77aa123e3b785d8c1093b4ee2bac19a3fb1b489bc0d1035963d87e18b9d`；v12 shim SHA256 `6752a1a51f973b2f917e779e12edd5a6419cccacde2ff69dfcc2d23e5434e886`。

| 验收 | 证据 |
|---|---|
| 原件持续执行 | `ui-empty-scan-continuous-125.json`：50宿主秒/8628虚拟ms，无guest异常，NuttX/MiIO/LVGL原生初始化与定时回调；25次扫描均经历pending/done/results。LCD仍为开机Logo，完整主界面显示尚未验。 |
| HTTP及错误响应 | `ota-channel-health-122.json`：RV32代码实际返回200健康、401错token、500截断EOF。 |
| A→B完整安装 | `ota-channel-upload-127.json`：6769704 B、HTTP200 done/ok、原厂ff13真实返回0、payload ff04；guest6045ms，无提前环境复位。 |
| 独立提交核对 | `ota-channel-boot-selection-128.json`：F000表header age2、entry age1、active1；B逐字节匹配，旧A/Boot2/旧表/保留区不变。 |
| B启动后OTA | `ota-channel-nextboot-health-129.json`：直接复用127 NOR，FW1/XIP7B0000运行B，200/401/EOF通过，无重造NOR。 |
| B→A再次安装 | `ota-channel-upload-130.json`：完整A、HTTP200、原厂ff13=0及payload ff04；guest6054ms，原生UI回调持续。 |
| 第二次独立核对 | `ota-channel-boot-selection-131.json`：E000表header age3、entry age2、active0；A完整，旧B保护通过。 |
| A再次启动后OTA | `ota-channel-nextboot-health-132.json`：复用130 NOR，FW0运行A，200/401/EOF通过，无reset/fatal。 |
| 更新中断保护 | `ota-interrupted-123-verification.json`：原件扫描失败引起复位中断部分真实上传；活动A、两份表、Boot2不变，非活动B首字节仍无效。不是物理掉电测试。 |

最终引擎native回归：`tests-dma-final-126.json`6组、`tests-pwm-final-126.json`3组、`tests-ota-flash-final-126.json`三profile均通过。HTTP核心`tests-ota-channel-01.json`为ASan/UBSan 42项。扫描模型单元13项、启动验证器20项含真实127与19种负向通过；这些单元不冒充硬件测试。所有测试进程及owned GDB/QMP socket已清理。

## 实现与产物

原件6769704 B SHA256 `972db4c136c7ed9e24a83c07c1a7fd62040ca018b08ca285216d26b1fee3c6b9`保持不变。基线为已授权unsigned候选SHA256 `1bbe8f45359eb7cdb55f337643f3ce28894999ffb21beab92fb62e995cd4854d`。

| 模拟候选 | SHA256 |
|---|---|
| `ota-channel-a-004.bin`，6769704 B | `8570f4cd0ffe631802be16da3b11d27683fcfe6d5c0c9b24f4e360d3dca539a6` |
| `ota-channel-b-005.bin`，6769704 B | `4f71fd731f45c9c45cc1d010e8d2326cbfdfd1bc136df8f8f6073f2dd03ed806` |
| `ota-channel-linked-v5.elf` | `ffe20065a5fc13eea5c3cf47563cac692a676cbda9ed2fdc4c2128674f151d03` |
| 127安装后16MiB NOR | `fc299b4371e502f6d50a47f4aec97ec118cc79db620153b2e91b90a8f9bae397` |
| 130回刷后16MiB NOR | `b05cd412efa7d3c0a1863af058761a1de305b6750d83a3a07cf67bb8c27b0a40` |

A/B同一v5代码、不同build_id，用于证明更新连续性，不声称已实现新业务。含manifest的注入模块6132 B，无可写静态段。端口常驻server24 B、活跃connection1716 B，合计1740 B堆，不含原件socket/flash内部；子线程分配8192 B，C自身静态栈链约2880 B，132观测API处栈下降下界2864 B，**不是完整高水位**。父builtin栈保留。

`ota_channel.c`执行有界HTTP，`ota_0041.c/.ld`绝对绑定原件API。GET `/health`；POST `/firmware`要求64hex Bearer、完整Content-Length及X-ROM-CRC32；guest请求总期限120s。复用原件非活动槽写入/首字节无效标记/CRC读回恢复/ff13提交。借setlogmask builtin与rcS启动；父任务原生poll休眠保活，子线程服务。v5修复了入口return后子线程未被调度的问题。原云miIO.ota禁用以排除并发；pet0换有效空白GIF，原GIF范围复用给模块，范围/ROM身份均受builder清单重建检查。认证key仅ignored私有artifact与镜像，不打印/入库。旧A002/B003未解决生命周期，只留历史。

## Mock边界与审阅

网络是**仅payload调用**的宿主loopback socket API provider，原HTTP解析、线程、安装代码由RV32执行；没有空口Wi-Fi/关联。BLE走原件无HCI错误，精确ADC查询NULL，未伪造温度。扫描失败确会触发原件4000ms重启，现以公开API有限20ms空扫描事务替代：先pending、再done、空结果ENOENT，原caller容忍；不是忽略复位。GPIO3高选择非工厂模式，GPIO24高为按钮松开。DMA/PWM为有界功能模型，无物理波形等价。[硬件细节](qemu-rtc-display-20261017.md)。

`verify-qemu-ota-boot.py`独立检查表CRC、header/entry age严格+1且不溢出、active切换、候选全部字节及保护区。原件双表选择为unsigned较大header age、同age选E000。ff13保留entry.length+28，只修改active与age；本normal/uncompressed合成profile原值0，因此严格检查仍0，镜像6769704 B另行校验。没有真实Boot2备份，按该规则保存NOR再启动是**显式虚拟复启**，不是执行恢复出的Boot2。

审阅发现并修正了测试误判风险：健康模式出现任何reset不得PASS；最终UART出现异常或输入文件改变必须清PASS；分区长度按原件保留0合同核验，不能误要求等于镜像字节数。

当前结果不证明物理Wi-Fi、031首次安装、真实Boot2回滚/掉电恢复或永不死机。OTA线程仍共享内核/地址空间，不能隔离未来Patch的内存破坏或关中断死循环。健康查询目前仅协议版本，生产使用前仍需完善提交结果查询/身份与认证管理；未知提交会围栏，不可盲重发。**manifest仍flashable=false，全部候选仅模拟，未部署。**

## 复现

使用已有目录、WSL，见各运行JSON的完整command。`build-ota-channel-0041.sh <新名称>`构建模块，builder在冻结unsigned基线上生成候选/清单。运行`run-qemu-ota-channel.py`，提供`--qemu --image --manifest --flash-fixture --rom-shim --profile --output`；profile用125，完整上传另给`--upload-image --upload-manifest --auth-key-file --timeout 180`。180是宿主调试预算，不改guest120s期限。

每次安装后先运行`verify-qemu-ota-boot.py --before <原NOR> --after <快照NOR> --image <上传镜像> --manifest <清单> --output <新报告>`，通过后使用**同一快照NOR**与其`--active-slot 0|1`复启；不得重造空NOR冒充更新后启动。密钥不放命令行值。当前全部有限验证实例已退出，旧`start-qemu-ap01.py`仅历史启动边界，不代表本闭环。
