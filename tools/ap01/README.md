# AP01 研究与离线构建工具

## 0041 QEMU执行与Patch注入

当前目标与验收见[独立OTA闭环验收](../../project-workflow/ap01/qemu-ota-final-20261017.md)，硬件细节见[模型证据](../../project-workflow/ap01/qemu-rtc-display-20261017.md)。原件已进NuttX、MiIO及LVGL原生初始化/定时回调；`ui-empty-scan-continuous-125.json`在50秒宿主观察期推进到虚拟8628ms，无guest异常。LCD捕获仍是原厂开机Logo，不能称完整主界面显示已验。显式`--icount-shift 2 --icount-sleep on`是合成时间，不表示E907精确计时；`start-qemu-ap01.py`历史RTC实例不代表最新模型。

`run-qemu-mock.sh <新报告.json>`先编译当前模型，再有限时执行精确0041。必要功能显式启用：`--rom-shim <bin> --clic-mock --pec-uart-mock --flash-fixture <bin>`；CPU还须开启`x-ap01-clic=on,x-ap01-p=on`及原有T-Head扩展。`--breakpoint 0x62fc2500 --pass-interrupts 1000 --pass-ecalls 1000`允许原厂trap/handler/mret实际执行，其他异常仍停止。报告记录原件/引擎/shim/fixture哈希、UART、异常和QMP显示/Flash状态；截图与`pixel_writes`须结合执行入口判读，不将合成测试画面算作原厂UI。

连续观察可用`--observe-seconds 50 --timeout 60`并省略trap-handler breakpoint/pass参数。正常输入配置`--gpio3-input-high --gpio24-input-high`显式选择非工厂模式、松开旋钮按键；BLE/ADC采用真实不可用路径`--ble-unavailable --adc-unavailable`。扫描必须用`--scan-empty-mock`：20ms pending→done→空列表ENOENT；`--scan-unavailable`会使原件安排4秒延迟重启，保留它仅用于失败取证。无线连接没有被模拟。

`build-ota-channel-0041.sh <新名称>`构建RV32独立HTTP模块，`build-ota-channel-0041.py`重建固定unsigned候选上的限定Patch及清单；`run-qemu-ota-channel.py`用payload范围限定的loopback socket提供器执行实际HTTP模块。`verify-qemu-ota-boot.py`检查真实提交后的NOR表CRC/age/活动槽、候选逐字节匹配及旧槽保护，再供显式虚拟复启；它不是恢复出的Boot2。候选均为simulation-only，不能据此宣称031首次安装或实机恢复能力。

`build-qemu-bootrom-shim.sh <新前缀>`编译只供模拟器的RV32 API绑定；`build-qemu-flash-fixture.py`构造16MiB合成NOR及SDK格式分区表。默认只有`/data [710000,720000)`和`/resource [EB0000,FF0000)`可修改进程内NOR；显式`ota-flash-mock=on`额外允许非活动FW槽及非活动分区表，活动槽/Boot2/保留区仍拒绝。`ota-fw1-active=on`仅供已核验提交后的FW1复启。磁盘输入fixture始终只读；不是实机备份或可安装Flash镜像。各`test-qemu-*.py`为独立合成用例，不代替整ROM验收。

入口：[显示续作交接](../../project-workflow/ap01/qemu-display-handoff-20261017.md)。`build-qemu-ap01.sh`构建固定QEMU10.1.0及`qemu-ap01.c`；`run-qemu-ap01.py`执行精确0041，`test-qemu-ap01.py`检查模型，`probe-qemu-injection.py`保留早期注入对照。已进入原生任务并完成OTA模拟往返，实际LCD主界面仍未显示。须在WSL运行，构建新目录按AGENTS提权创建；不刷实机。

## 维护核心离线构建

`build_maintenance.py`只读核对现有0031-opt/opt-setting及清单，复用原校验器检查允许修改范围和尾部CRC，编译本仓库维护核心，执行宿主检查与同一套RV32模拟测试，输出栈/尺寸/源码与产物哈希。只生成静态库和核心ELF，**没有设备任务/HTTP/存储/安装/UI适配，不生成完整ROM**；不联网、不访问设备、不修改旁仓库、不创建目录。使用本机已有LLVM23和Rust所带链接器，以及既有Unicorn2.1.4；不用原Linux构建工具在macOS上冒充成功。

```sh
python3 -B tools/ap01/build_maintenance.py --output <已提权预建的AP01产物目录> --require-rom
```

`--require-rom`在核心验证通过后仍返回2并记录`rom_ready=false`，表示设备接口缺失；去掉该参数仅检查核心构建。已有同名最终报告拒绝覆盖。现有固件没有完整原厂源码，本轮遵循“不额外逆向”，缺失接口见[实施记录](../../project-workflow/ap01/maintenance-build-20261015.md)。

## 0031 原厂OTA与独立维护定位

`inspect_ota.py`复用0031精确校验/基本块分析，只读提取原厂OTA弹窗、点号定时回调、原生更新I/O等7个窗口及候选引用；不创建目录、不联网、不构建或刷机。输出含原字节与哈希，不定义可调用ABI或完整函数边界。

```sh
PYTHONPATH=artifacts/ap01-research/ble0031-20261014/deps python3 -B tools/ap01/inspect_ota.py \
  --image artifacts/ap01-research/ble0031-20261014/ap01-1.0.2_0031.bin \
  --output <既有目录中的新报告.json> --self-test
```

输出已存在则拒绝，自检覆盖真实timer call目标、UI引用及变异/截断拒绝。[维护设计/证据边界](../../project-workflow/ap01/independent-maintenance-20261015.md)不代表已实现常驻服务。

## 0031 最小XIP App候选

`build_app_rom.py`读取精确原厂0031和已验证的优化GIF，使用只读克隆中现有WSL RV32 GCC工具链编译本仓库`firmware/ap01`，输出完整更新容器、App包、ELF/map/dis/su及源码/成品哈希清单。不联网、读凭据或访问设备，不创建目录，不覆盖既有产物；先按AGENTS提权预建输出目录。WSL需提权工具调用，普通沙箱会E_ACCESSDENIED。GIF逐帧无损验证用WSL现有Pillow。

```powershell
& 'C:\Python314\python.exe' -B tools/ap01/build_app_rom.py --output <本仓库artifacts/ap01-research中预建的新目录>
& 'C:\Users\cogic\.platformio\penv\Scripts\python.exe' -B tools/ap01/test_app_rom.py --directory <该构建目录> --output <新的报告.json> --deps artifacts/ap01-research/app0031-20261013/deps
```

模拟器依赖隔离目录内Unicorn2.1.4，实际执行候选RV32指令；stub原厂对象创建/分配/释放/日志。`verify_gif.py`仅比对资源帧。候选不是完整Bridge或文件App OTA；[源码/ABI](../../firmware/ap01/README.md)、[验收和限制](../../project-workflow/ap01/app-implementation-20261013.md)。本轮不刷机。

本目录保存 codex_status 的分析脚本；结论与依赖见 [研究入口](../../project-workflow/ap01/README.md)。第三方克隆保持只读，不需要在其中安装或放入本项目脚本。

## 下载 0041 原厂固件与复用登录

`download_official.py` 复用只读克隆的扫码/云查询模块。按用户最新授权，扫码后立即把可复用凭据保存为本仓库 `artifacts/ap01-research/xiaomi-account.dpapi` 并读回校验；采用Windows当前用户DPAPI，缓存不入库、不输出内容。后续默认读取缓存直接登录，`--login` 可强制重新扫码。既不读写 env，也不列举设备或调用控制接口。固定目标型号和 0041 版本；若最新接口返回其他版本，停止下载。分别限制登录与固件 HTTPS 目的地，下载会话不带账号 Cookie；错误原文隐藏，二维码退出时删除，缓存继续保留。审阅与状态见 [下载专项](../../project-workflow/ap01/firmware-download-20261013.md)。

2026-10-13实际0041下载/保存后读回已通过，新进程复用登录查询成功；实际缓存受 `.gitignore` 的 `artifacts/` 规则覆盖且未跟踪。已有同名BIN/记录时工具仍拒绝覆盖，不将登录复用等同于重复下载。

当前默认 Python 没有 requests；已验证现有 PlatformIO Python 的 requests 2.34.2，无需安装依赖。在 PowerShell 执行：

```powershell
& 'C:\Users\cogic\.platformio\penv\Scripts\python.exe' -B 'D:\Documents\PlatformIO\Projects\codex_status\tools\ap01\download_official.py' --self-test
& 'C:\Users\cogic\.platformio\penv\Scripts\python.exe' -B 'D:\Documents\PlatformIO\Projects\codex_status\tools\ap01\download_official.py'
```

首次用官方米家 App 扫描 `artifacts/ap01-research/xiaomi-login-qr.png`。二维码有效期至多五分钟；凭据保存发生在查询/下载之前。控制台最终输出路径/长度/MD5/SHA256，BIN 与去除 URL/凭据的 JSON 保存到既有 `artifacts/ap01-research/`。工具拒绝覆盖已有文件；下载失败五分钟内输入 `retry` 可重查/下载，最多三次尝试；取消/超时/进程退出后也可再次运行同一命令复用缓存。实际官方固件域名 `iot-ota-cdn.io.mi.com` 已按精确主机加入白名单。此工具不发送设备升级。

## 静态分析器

`__main__.py` 使用现有 Capstone 5.0.7 及同父目录 `cuktech-ap01-firmware-tools/core/firmware_image` 校验模块，读取该克隆的 `artifacts/firmware/original/ap01-1.0.2_0031.bin` 和原始载荷源码。输入镜像需匹配工具内固定的 SHA256、长度与校验；不会联网或访问设备。

从 codex_status 根目录执行：

```powershell
$env:PYTHONDONTWRITEBYTECODE = '1'
$env:PYTHONIOENCODING = 'utf-8'
python -B tools/ap01/__main__.py --self-test
```

输出分析结果时加 `--output <本仓库中已存在的空目录>`。按仓库规则先提权创建目录；工具拒绝覆盖已有产物。目录应位于 `artifacts/ap01-research/`，不要写回第三方克隆。禁用字节码缓存可避免导入校验模块时向上游写入缓存。

## 0041/0031版本对照与Ghidra

`compare_roms.py`固定0041/0031身份并验证头CRC与恢复记录；生成候选字符串/地址引用和窗口，不是完整CFG。处理GCC字符串后缀共享，未知解码项可能是数据。`--inventory-only`提取并校验两版启动ROMFS、rcS与8项内置应用表；`--self-test`核对两版及既有0031解析器。需要现有Capstone，不联网、不读取登录缓存、不产生ROM。

```powershell
$env:PYTHONIOENCODING = 'utf-8'
python -B tools/ap01/compare_roms.py --output <已经提权预建的本仓库证据目录> --self-test
python -B tools/ap01/compare_roms.py --output <同一目录>
python -B tools/ap01/compare_roms.py --output <同一目录> --inventory-only
```

输出拒绝覆盖已有同名文件。0041/0031有效结果见`artifacts/ap01-research/0041-20261013/refined/`。

`Ap01Export.java`给直接Ghidra headless使用：先以`BinaryLoader`、`RISCV:LE:32:default`、base `0x9ffff000`导入匹配SHA的原件；处理数据库时传`-noanalysis -scriptPath <tools/ap01绝对路径> -postScript Ap01Export.java <新的输出文件> <明确入口地址> ...`。脚本给GP设置实际启动值，反汇编入口、分析变化，再导出真实函数调用/交叉引用/伪代码。数据库、编译产物及目录创建须按项目规则提权。Ghidra脚本失败有时仍给进程exit0，必须核对日志无SCRIPT ERROR、报告实际存在、语言/SHA正确及坏指令/custom0限制。

当前REA6.4.0 Ghidra provider不接纳裸RV32，本脚本输出不能标成REA Evidence。已安装Windows Ghidra12.1.4/JDK21.0.10运行通过；处理器扩展仍有未知语义，不能仅靠自动伪代码填写ABI。有效导出和独立GNU/OpenSSL核对见[0041报告](../../project-workflow/ap01/knowledge/0041签名与Bridge接口分析.md)。

## 历史资源探针

`resource_budget.py` 保留用于解释 2026-10-11 的尺寸报告，仅调整迁移后的路径。它依赖旧版 Bundle 存储结构和第三方工具链，不适配当前单份格式，不能用作当前源码的容量验证；本次不运行其编译流程。更新资源测量前须先调整探针并记录新源哈希。旧报告和对象在 `artifacts/ap01-research/raw/bridge-budget-20261011/`。

## 0031 蓝牙绑定接口定位

`inspect_ble.py`复用本目录0031校验器/解码器，依赖固定上游只读克隆的`core/firmware_image`与Capstone 5.0.7。支持显式原件路径，不要求Windows产物已复制到Mac。只输出候选引用及指定窗口，不生成ROM、不连接/配对设备；先按AGENTS提权预建输出目录，输出文件必须不存在。

```sh
PYTHONPATH=artifacts/ap01-research/ble0031-20261014/deps python3 -B tools/ap01/inspect_ble.py \
  --image artifacts/ap01-research/ble0031-20261014/ap01-1.0.2_0031.bin \
  --output <既有目录中的新报告.json> --self-test \
  --window 0xa0127bd0:0xa01282d0 --window 0xa0191a50:0xa0191ae6
```

未知指令和分支会清除基本块常量推导，避免跨调用臆测寄存器值；仍是有限线性扫描，不证明函数边界、ABI、SMP或密钥持久化。2026-10-14定位/LLVM交叉核对和实机边界见[绑定专项](../../project-workflow/ap01/ble-binding-20261014.md)。

`test_ble_security.py`额外依赖隔离目录内Unicorn 2.1.4，执行精确0031原件中72字节的ATT权限检查函数，不生成替代实现。10个明确场景和6144个组合验证普通读写、缺回调、加密/认证拒绝及返回时的栈/属性约束；仅证明这个真实函数的行为，不替代无线实测或证明整个ROM没有SMP。

```sh
PYTHONPATH=artifacts/ap01-research/ble0031-20261014/deps python3 -B tools/ap01/test_ble_security.py \
  --image artifacts/ap01-research/ble0031-20261014/ap01-1.0.2_0031.bin \
  --output <既有目录中的新报告.json>
```
