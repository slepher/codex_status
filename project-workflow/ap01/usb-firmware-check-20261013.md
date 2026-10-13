# AP01 USB 固件检查（2026-10-13）

检查同父目录 `cuktech-ap01-firmware-tools/artifacts/firmware/original/ap01-1.0.2_0031.bin`（6,804,520 B，SHA256 8a721fc8ef25458d415b2460e4a251e0503a82f7743fdff85b12612190e5c1cb），并对照本项目0041原件（SHA256 972db4c136c7ed9e24a83c07c1a7fd62040ca018b08ca285216d26b1fee3c6b9）。

两份镜像含 UsbIfaceMng/UsbIfaceOta 内容，路径均为 PogoPinMng；已有线性机器码引用扫描确认0031代码 0xa00ab848 引用管理模块路径，0xa00affa2 引用OTA模块路径。0031 0xa00afcd0 起读取与 UartCommInit 共用的RAM对象 0x62fca780，0xa00afcea 调用读取入口；读取错误记录同时引用 UartCommReadBytes 和 PogoPinMng/UartComm.c。这是底座/充电接口管理与UART收发的直接静态线索，不证明后置Type-C存在电脑USB数据接口。

全镜像可打印ASCII字符串复查：usbdev、usbhost、usbd_、usbh_、cdc_acm/CDC ACM、ttyACM、ttyUSB、CherryUSB、TinyUSB、bflb_usb、usb_dc、usb_dp/usb_dm、USB descriptor 等模式未命中。USB MUX、usb20有命中，单独不足以认定设备端USB协议栈。未定位到可验证的USB描述符注册、端点收发或USB下载模式入口；未完成全固件控制流和所有硬件寄存器访问证明，因此不能断言代码绝对不存在。

原厂更新包不包含完整Boot ROM/同机Boot2/eFuse现场，不能从应用镜像排除芯片下载态USB能力，也不能推导AP01按键进入方法或板级D+/D−接线。

本轮REA inspect_firmware_regions返回 unsupported_provider：该集成要求Linux与调用者提供的Binwalk。本轮有效证据来自普通Python字节扫描、已有精确哈希匹配的机器码引用扫描、Capstone局部反汇编；不是REA反编译证据。没有打开设备或写入固件。

证据：`../../artifacts/ap01-research/usb-firmware-scan-20261013.json`；历史机器码引用：`../../artifacts/ap01-research/0041-20261013/comparison.json`。
