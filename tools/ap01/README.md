# AP01 研究与离线构建工具

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
