# 0041 启动外部Mock与独立QEMU实例

日期：2026-10-17，Asia/Hong_Kong。用户明确要求启动，并授权Mock必要外部环境。原件/设备/Bridge未改；本轮仍未启动完整NuttX或原厂UI。

最新续作已补RTC/CLIC/UART/SPI/DMA/TE，7套93组检查通过；原件推进到PEC UART2后端入口，原厂UI仍未启动。读[最新续作与阻塞](qemu-rtc-display-20261017.md)。本页以下为旧冻结实例的历史快照，不能代表最新模型停点。

## 旧实例快照

WSL Ubuntu-24.04 **PID11338**，已实际从0041入口执行，当前QMP `status=debug / running=false`。进程、exe及状态已经另一条QMP连接独立核对。PID仅为此刻快照；后续操作先核对`live-mock-session.json`的PID/exe/socket，不凭历史PID停止进程。

暂停原因：`mepc=0xa002a12e / mcause=5 / mtval=0x20009014`，尚未实现MCU_MISC RTC/机器定时器配置。PC已进入零异常向量并被断点捕获；不能直接从PC0继续冒充恢复。下一步补定时器模型后从原件入口重新启动，再处理后续GPIO/CLIC等真实依赖。完整启动仍开放。

会话/独立核对/日志均在`artifacts/ap01-research/qemu-0041-20261017/`：`live-mock-session.json`、`live-mock-session.verify.json`、`live-mock-session.qemu.log`、`live-mock-session.pid`。调试和QMP使用会话JSON登记的`/tmp/ap01-live-*.gdb/.qmp` Unix socket，没有公开TCP端口。

## 已实现并验证的Mock

- OTP：显式2048 B合成fixture，bank0只读加载512 B，两个控制寄存器读取完成状态机；编程、非法触发、未知bank/读取仍拒绝，复位取消pending。不是设备OTP/密钥。
- 合成板型：fixture偏移`0x18`字`0x11800000`，对应QFN56、8MiB Flash、WB4MiB PSRAM。0041的字段提取、字符串表及固定SDK `bl616_ef_ctrl.h`为依据。全零fixture导致“无PSRAM”断言，随后早期panic空指针；保留失败证据，没有给OS伪造TCB跳过断言。
- PSRAM：有限REQ/GNT、配置W1P事务、提交后状态读回、取消和复位；显式合成ID0/ID1为零，CR0读回为控制器状态视图，不声称真实wire编码/总线时序；睡眠/复位等未支持动作拒绝。物理数据服务是已映射4MiB功能RAM。
- Boot2：明确合成的cached JEDEC `0x1740ef`（SDK支持的8MiB配置）；slot320 Flash配置API返回`-1 unsupported`，原厂自身fallback表继续执行。没有Flash读写/OTA成功stub。
- 时钟：slot115只接受XTAL4/PLL3，更新HBN/GLB状态与40MHz XTAL、960MHz WiFiPLL、491.52MHz AudioPLL；slot144在PLL有效后接受selector5，更新mux/divider/gate及MCU320MHz/BCLK80MHz；slot227原生RV32查询HBN XTAL标记，有效时写一个字节。非法请求返回SDK `ERROR=1`，不改已提交状态。没有真实锁相/物理计时保证。

BootROM三个模拟入口及一个unsupported入口均通过明确登记的table槽映射到`0x90000000..`，代码由QEMU原生执行；时钟请求通过仅Mock模式映射的mailbox提交。ROM原件未Patch这些调用，其他未知BootROM槽继续报错。严格模式（不启用Mock开关）仍保留原先拒绝。

**55项合成原生QEMU回归通过**，报告`tests-xtal-getter.json`，涵盖原有模型、OTP/PSRAM/时钟状态与失败链。原件实际推进证据`xtal-getter-run.json`及live日志。虚拟时钟值由独立QMP读取核对，不能当实际MCU频率测量。

## 产物与启动

冻结Mock引擎`artifacts/ap01-research/qemu-0041-20261017/qemu-system-riscv32-mock`，SHA256 `2e77fda96cc801d8a75e470a49f75f72df2e6d77b8d6321da653fe73e8b00673`；它是WSL Linux ELF，原有非Mock冻结引擎未覆盖。

合成fixture `otp-synthetic-ap01.bin`，SHA256 `a4d7631e1ffc7f5dee69950ff2f2fc66cf20d22d143bf07ecf3126c19c376ad6`。只用于模拟，不导入任何设备/账号凭据。原厂BIN SHA256仍为`972db4c136c7ed9e24a83c07c1a7fd62040ca018b08ca285216d26b1fee3c6b9`。

`tools/ap01/start-qemu-ap01.py`启动独立daemon，无工具stdio继承；有限时运行后在边界暂停，保存PID/exe/命令/socket/寄存器，日志16MiB上限、禁core。启动失败时从唯一pidfile恢复并验证PID后清理自己的daemon。成功会话保留供检查，无显示窗口。复现时输出名须不存在：

```bash
cd /mnt/d/Documents/PlatformIO/Projects/codex_status
python3 -B tools/ap01/start-qemu-ap01.py \
  --qemu artifacts/ap01-research/qemu-0041-20261017/qemu-system-riscv32-mock \
  --image artifacts/ap01-research/ap01-1.0.2_0041.bin \
  --otp-fixture artifacts/ap01-research/qemu-0041-20261017/otp-synthetic-ap01.bin \
  --output artifacts/ap01-research/qemu-0041-20261017/live-mock-next.json
```

在提权WSL命令中运行；不需要PIO/串口/设备/Bridge。当前实例已经启动，复现命令会另建实例，不是连接现有实例。

## Review

GPT-6.1 Sol审阅未发现已支持OTP/PSRAM事务的状态污染；指出daemon启动包装器失败后可能遗留PID，已补唯一pidfile恢复/exe/argv核对及有限时退出检查。保留未实现RTC、GPIO/中断、完整Flash/文件系统、无线与显示模型的边界，不以合成身份、寄存器锁存或错误处理路径证明真实硬件成功。
