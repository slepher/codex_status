# 板载独立 RTC 接入（2026-10-04）

用户授权：开始修改，以板载独立 RTC 负责日期时间和离线走时；单调计时器继续负责超时/窗口。基线为已部署 Note4 0.18.55 / 3A552561…642FF。本轮不提交、不替换设备运行镜像。

## 硬件与设计

- Note4：PCF8563、I²C 0x51、SDA47/SCL48/INT5，依据 [厂商硬件表](https://github.com/itopinion/zectrix-note4-epd-demo/blob/main/docs/HARDWARE.md) 与 src/platform_target.h。1.54：PCF85063A、同地址/引脚，依据 [Waveshare 产品文档](https://docs.waveshare.com/ESP32-S3-ePaper-1.54G)。旧 power-state 文档将两种型号混写，本轮纠正。
- 日期寄存器按 [NXP PCF8563](https://www.nxp.com/docs/en/data-sheet/PCF8563.pdf) / [PCF85063A](https://www.nxp.com/docs/en/data-sheet/PCF85063A.pdf) 分 target；不可通过共有 0x51 自动猜型号。RTC 始终保存 UTC，显示沿用现有 TZ。
- 独立 RTC 成功读值优先于内部 RC 深睡差分；冷启动/软件复位也可恢复。无效（STOP/测试模式/OS或VL/非法BCD或日期/不支持的世纪）及传输失败明确拒绝，保留内部 RTC 兜底。有效范围与既有 timeKnown 一致，且限制至 2099 年末。
- Bridge 两个校时入口写回 RTC；NTP 回调在短临界区保存实际UTC与单调锚点，在 loop/深睡前处理 I²C，避免 TCP/IP 回调与业务并发访问。仅可信新校时写 RTC，不将内部 RC 回退值写回污染独立 RTC。
- 有界 I²C 100kHz/25ms 超时、16B TX/RX 工作缓冲；校时 STOP→一次完整7字节日历→读回一致→启动→最终有效性验证。失败不宣称成功；中途失败保持 STOP/本启动未确认屏障，等待下次可信校时修复。
- 每次唤醒读取；常驻 light 每60s读取一次，差≤1s保持系统亚秒相位，差>1s才纠正。不每秒通过 I²C 拉取，不改变 PowerPlan、claim、radio 或唤醒源，不新增 NVS 写入。ESP32 RTC 定时唤醒仍保留，RTC 读数纠正日期时间；RTC INT 唤醒不在本次范围。没有独立供电证据时不承诺完全断电保持。

## 任务与验收顺序

1. 生产小驱动+真实驱动寄存器假总线测试（两种芯片、UTC/TZ、闰日/跨年、失效/传输/写阶段故障）。
2. main.cpp 接入所有校时及启动/light/深睡入口，公开状态显示型号/健康/样本 UTC，保留时钟来源兼容字段。
3. 宿主 ASan/UBSan；仅 Note4 ROM 增量构建、marker/大小/hash、真实 Xtensa 栈与 RAM 证据；Wi-Fi 启动门禁及 diff 检查。
4. 更新 PROGRESS/唯一 backlog。两板实机首次校时→无网多轮深睡→软件重启/24h漂移/功耗及1.54 ROM与部署仍单列未验，不以假总线或编译替代。

状态：源码及软件验收完成，Note4 .56 候选已构建、未部署；证据目录 artifacts/external-rtc。实施与资源证据见 task.md，复核见 review.md，剩余实机验收只维护 backlog 的时钟准度行。

## 授权部署后实机修订（2026-10-04）

用户“更新固件”授权Note4 OTA；.56实际运行hash已验证，但clock_rtc报告buffer_allocation_failed。现行Arduino Wire.cpp拒绝小于32B缓冲，宿主替身漏此约束。先补替身复现red，再改为平台最小32B/方向、修订Note4版本.57、重新构建/栈门禁/OTA/独立运行hash与真实RTC读取验证。修订资源预算增加32B堆请求；1.54不构建或部署。

.57实机已验证运行hash，仍short_read。厂商zectrix_board.cc的BoardI2cForcePowerOn及i2c_device.cc每次事务显式开启GPIO42，而本项目epdBegin/epdThinBegin关闭42；按该供电前提补替身red，再以Note4-only作用域在RTC读/写期间开启并恢复原电平，保持原深睡/空闲电源状态。修订.58须实机读数及跨多次访问走时验证；不以厂商参考代码代替现场。

最终：上述授权部署修订完成，Note4.58运行hash与RTC ready/valid/后续采样确认，已登记release；长期及1.54剩余统一归backlog，软件阶段不再是未部署。

## 用户版本命名修订

用户要求固件只使用note4后缀；统一Note4 FW_VERSION为0.18.59-note4，去掉版本分支中的a/b及功能后缀，模拟设备默认目录同用note4。构建环境/硬件target标识不变，历史版本/ROM记录保留原值。只增量构建现行Note4 B环境，核对marker/ROMhash后沿本次OTA授权更新并独立确认运行镜像/RTC，同步PROGRESS与后续命名规则。

版本命名修订已完成：0.18.59-note4，唯一Note4环境47.57s构建/marker/匹配ELF门禁及RTC/main帧一致检查通过；OTA e082ac3e succeeded/image_verified，下个自然联网窗口独立运行镜像hash14CC1FBA…1FB8DF匹配，RTC ready/valid与WFS3保持正常，latest release已登记。后续只用版本号-note4；完整产物与证据见PROGRESS顶节。本轮未提交。
