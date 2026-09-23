# Prompt：Note4 USB 接入后的实机评估

你是 Codex Status 仓库的实机评估代理。用户已将 ZecTrix Note4（4.2 英寸、400×300 黑白、SSD2683、ESP32-S3）通过 USB 接入电脑。请在本机完成**可安全读取的实机评估**，形成下一步 bring-up 的事实清单和证据；不要把旧 1.54 英寸设备的板级参数套给 Note4。

先读 `AGENTS.md`、`PROGRESS.md` 最新节、`project-workflow/next-execution-plan-2026-09-23.md`、`project-workflow/generic-display-platform-implementation/status.md`，再查看 `src/platform_target.h`、`src/EPD_SSD2683.{h,cpp}`、`src/epd_target.h`、`platformio.ini`、`partitions.csv`。当前目标头文件有缺失事实的 `#error`，当前 `platformio.ini` **没有 Note4 env**；驱动只是骨架，尚无 Note4 实机验收。现有 1.54 英寸设备运行 0.17.9-bw，桥可能在后台运行，不要影响它。

## 执行步骤

1. 记录接入前后 Windows 串口/USB 设备变化，确定真正对应 Note4 的端口、VID/PID、设备名称与序列号；不要默认 COM3/COM4。若有多个候选，结合拔插变化判定。记录 Note4 的板号、屏幕/排线标识、供电方式和可见按键；无法从 USB 得到的事实明确记为未知。
2. 先用只读方式查看芯片信息、Flash ID/容量、MAC、启动日志和现有分区表。确认 esptool 实际参数与容量后，将**全片出厂镜像及分区表备份**到 `artifacts/`，记录 SHA256、字节数和读取命令；备份可能含凭据，不打印内容、不放进仓库或日志。若芯片、端口或容量无法可靠确认，不猜长度读取或写入。
3. 观察现有出厂固件：USB 复位/启动是否稳定，串口启动原因、PSRAM 检测、屏幕方向/全刷或局刷行为、BUSY 超时、三个按键、休眠/唤醒。只记录实际能观察的项目；不要把旧板的 40 MHz Flash 修复、引脚、电池和分区设置直接套用。
4. 从仓库资料、随板文件或厂商资料逐项核实 EPD SCK/MOSI/CS/DC/RST/BUSY/3V3_EN 与 PGUP/PGDN/ENTER 的 GPIO，SSD2683 的 gate/方向/数据入口/border/温度曲线及全刷、局刷 LUT，Flash/PSRAM 型号与容量。给每个结论标明证据来源；USB 枚举和 `flash_id` 本身不能证明 GPIO 或波形。
5. 对照当前源码列出：哪些事实已确认、哪些仍缺失；Note4 独立 PlatformIO env、分区与帧缓存、驱动时序、按键/唤醒、桥端 target 注册分别需要什么改动。可完成不依赖未知参数的只读分析和方案文档。

## 写入与交付边界

- 本次 USB 评估先不执行 `erase_flash`、`write_flash`、`pio run -t upload`、OTA、改分区或覆盖出厂固件。若完整 bring-up 需要刷机，先提交具体 ROM/env、分区、备份与恢复路径，以及会改变的设备状态，供用户确认后再执行。不要只因为设备可枚举就移除 `#error` 或开启 `TARGET_PARTIAL`。
- 不停止或重启非本任务构建目录的桥/设备服务；不暴露 Wi-Fi 密码、设备 token 或备份内容。保留用户现有未提交文件。
- 原始串口、芯片/分区读取摘要和照片索引存 `artifacts/`；将确认的板级事实、未知项、风险与下一步最小刷机方案写入 `project-workflow/generic-display-platform-implementation/status.md`，到达实机里程碑再更新 `PROGRESS.md`。

最终给出一张表：**项目、实测值、证据、是否足以进入 Note4 ROM 构建**；另列首次刷机前仍需的最少事实与恢复步骤。明确说明本轮是否改了设备状态。
