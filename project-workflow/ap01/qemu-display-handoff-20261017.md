# AP01 QEMU 显示续作交接

## 接手先读 / 当前任务

从根`PROGRESS.md`进入本文件，再查backlog C9。用户最新要求看到0041实际界面：**下一步打通QEMU原厂主界面显示**，同时保留已通过的独立OTA闭环；不以宿主重建SIM替代原厂ROM画面。设备目前0031，无备份/USB救砖途径，不再询问备份，不刷实机、不访问串口/云端、不改Bridge。

已完成：原件进入NuttX、MiIO、LVGL初始化及原生定时回调；125连续50宿主秒/8628虚拟ms无guest异常。A004→B005真实HTTP/原厂CRC/ff13提交→同NOR虚拟复启B→回刷A→复启A后OTA均通过。[闭环报告、哈希、限制](qemu-ota-final-20261017.md)。这不是完整显示验收，也不等于物理Wi-Fi或Boot2验证。

## 精确停点与下一项排查

最新原件证据目录`artifacts/ap01-research/qemu-0041-20261017/`，报告`ui-empty-scan-continuous-125.json`，UART同名`.json.uart.log`，显示`.json.ppm`/`ui-empty-scan-continuous-125.png`。目前捕获仍为CUKTECH Logo，`lcd_state.pixel_writes=153600`，不是主界面；没有已证明的首个异常可追。原件SHA/输入NOR文件未变。

已知原生`lvgl_ui_main`入口`0xa005cc1e`；日志有`lv_ap01_display_process, finish`、主题/动画构造及后续每秒回调。fbdev日志：起址`0xa8000020`、总长307200、stride640、320×240×RGB565，双屏页。**排查建议，尚非已定位根因**：先比对该RAM两页随guest运行是否变化；若变化，沿LVGL flush→NuttX fbdev update/vsync→原件LCD工作任务→SPI/DMA/TE确认刷新在哪里停止；若不变，查LVGL刷新/动画与任务等待。不能因为回调打印就宣称界面已显示，也不要直接填屏幕或跳过原厂流程伪造通过。

模型入口`tools/ap01/qemu-ap01.c`，显示相关`qemu-ap01-spi.h`（含LCD）、`qemu-ap01-dma.h`、`qemu-ap01-uart.h`；计时/中断在main、timer/mac/machw头。`run-qemu-ap01.py:capture_display`读取QMP `/machine` 的`lcd-state`并screendump；可同步查`flash-state`等。native显示回归`test-qemu-lcd.py`、DMA `test-qemu-dma.py`、CLIC `test-qemu-clic.py`。按实际改动选测，不例行重跑全部。

## 现有环境 / 复现原件

Windows仓库`D:/Documents/PlatformIO/Projects/codex_status`；WSL发行版`Ubuntu-24.04`，仓库挂载`/mnt/d/Documents/PlatformIO/Projects/codex_status`。QEMU源`/home/slepher/qemu-ap01-0041/source`，官方v10.1.0 commit `f8b2f64e2336a28bf0d50b6ef8a7d8c013e9bcf3`，现有build同根`build`。最终引擎SHA256 `7246b77aa123e3b785d8c1093b4ee2bac19a3fb1b489bc0d1035963d87e18b9d`。工具链在旁仓`cuktech-ap01-firmware-tools/tools/docker/toolchain/bin/riscv64-unknown-elf-*`，只读复用；SDK旁仓commit `41bf338a57be53919d99f6c6715a96100428c97d`，部分克隆，不要广泛git grep触发lazy fetch。

所有验证实例已退出，没有可继续操作的实时窗口/OTA监听。`start-qemu-ap01.py`是历史早期暂停实例，不能用于展示本轮主界面。先用已构建引擎复现，不必重编；WSL命令按仓库规则提权执行，新输出使用未存在的文件名。已有artifact目录可直接写，新目录必须提权预建。

以下在WSL仓库根执行，将`display-resume-133.json`替换为尚不存在的名字：

```bash
python3 -B tools/ap01/run-qemu-ap01.py \
  --qemu /home/slepher/qemu-ap01-0041/build/qemu-system-riscv32 \
  --image artifacts/ap01-research/ap01-1.0.2_0041.bin \
  --output artifacts/ap01-research/qemu-0041-20261017/display-resume-133.json \
  --mhint-latch --mtvt-latch --cache-csr-latches --boot-mmio --efuse-shadow \
  --otp-fixture artifacts/ap01-research/qemu-0041-20261017/otp-synthetic-ap01.bin \
  --psram-mock --boot2-mock --clic-mock --pec-uart-mock \
  --rom-shim artifacts/ap01-research/qemu-0041-20261017/qemu-bootrom-shim-v12.bin \
  --flash-fixture artifacts/ap01-research/qemu-0041-20261017/flash-synthetic-0041-v3.bin \
  --rf-calibration-mock --mac-empty-mock --trng-mock \
  --gpio3-input-high --gpio24-input-high --ble-unavailable --adc-unavailable --scan-empty-mock \
  --cpu rv32,x-ap01-clic=on,x-ap01-p=on,xtheadba=on,xtheadbb=on,xtheadbs=on,xtheadcmo=on,xtheadcondmov=on,xtheadmac=on,xtheadmemidx=on,xtheadmempair=on,xtheadsync=on \
  --run-to-trap --icount-shift 2 --icount-sleep on --observe-seconds 50 --timeout 60
```

调试首个trap可改为`--breakpoint 0x62fc2500 --pass-interrupts 10000 --pass-ecalls 1000`并移除observe参数；它会显著降低速度。原件XIP基址`0x9ffff000`，入口`0xa0000000`，文件offset=VA-base。不要只复制证据JSON的底层QEMU command而遗漏runner的API提供器，否则会重新进入未模拟外设路径。

需要改模型后才用`bash tools/ap01/build-qemu-ap01.sh /home/slepher/qemu-ap01-0041/source /home/slepher/qemu-ap01-0041/build`；它会安全应用/升级CPU补丁，不能用git reset覆盖已有模型。构建后记录新引擎hash；既有报告对应旧引擎，不能混称最终同一引擎。

## 不要重复踩的坑 / OTA保护

- `--scan-unavailable`会触发原件4000ms延迟复位，正常profile必须`--scan-empty-mock`：20ms pending→done→空ENOENT，非固定成功；没有无线关联。BLE走无HCI错误，ADC仅精确名称返回NULL；其余查询原生。
- `icount sleep=on`与GPIO3非工厂、GPIO24松开是显式输入假设；不更改宿主时间，不制造旋钮事件。BootROM/PEC是显式绑定，不是已解密受保护PEC。
- OTA入口父builtin必须原生poll休眠保活；返回会让已创建的子线程未获得运行，v5已修。保留8192 B子栈、单连接期限/认证/CRC/未知提交围栏与云OTA排他。
- 当前可复用A004/B005及127/130 NOR和各manifest均在ignored本地artifact；不提交ROM/Flash/密钥/私有日志。key仅以文件路径传入，不打印内容。A/B是同一模块不同build_id的模拟候选，manifest.flashable=false。
- 新修改如涉及安装，按127→128→129→130→131→132报告命令复现闭环；复启必须复用上一轮真实NOR，禁止重造fixture。没有真实Boot2，选择后重启明确标虚拟。

## 提交边界

本次提交包含QEMU/OTA源代码、相应测试、模型/验收/本交接文档。工作区还留有其他SIM/逆向专项的既有未提交内容，不能回退或混入本轮；根进度旧全文已保存在`docs/history/progress-archive-2026-10-17-ap01-qemu.md`。详细日志/ROM/截图在本机ignored artifacts，换机器需自行迁移授权文件，git clone不会自动具备这些私有输入。

交接提交前再次通过HTTP核心ASan/UBSan 42项、真实127快照启动验证器20项、暂存Python语法及git diff --check；本地报告`tests-ota-channel-commit-20261017.json`与`tests-ota-boot-verifier-commit-20261017.json`。没有为文档提交重编QEMU或ROM，已记录引擎/镜像指纹未变。
