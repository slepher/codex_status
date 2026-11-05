# Note4 USB 现场检查（2026-11-05）

用户请求检查已通过 USB 连接、未在本机 Bridge 登记或蓝牙配对的 Note4。本次仅诊断，未配对、改网、claim、发布、刷机或重启 Bridge。

## 现场结论

14:17–14:19 Asia/Shanghai，IOUSB 枚举 VID 0x303A / PID 0x1001、USB JTAG_serial debug unit，serial 7C:4F:AD:B9:34:08，串口 /dev/cu.usbmodem101，与既有 Note4 身份相符。本机运行 data/platform/state.json 仅登记 AP01，未登记 Note4。

串口自报 fw=0.18.107-note4、slot=ota_1；与交接卡 .106 不一致，本轮未取得运行镜像哈希，不能将本地 .106 ELF 当作匹配产物。设备能响应 status/pmstats/log/diag/batt。Wi-Fi state=off、ip=-，日志多次 attempt failed: network_not_found，随后 BLE report next、advertising as CodexStatus-B93408，并在 rendezvous deadline 后关闭；存在 sync_total_budget_exhausted。直接故障为网络接入失败且本机尚无绑定通道，不证明密码错误或具体缺失 SSID。照片的 53% / 11-04 14:35 为旧显示；USB本身不会建立 Bridge 业务同步。

heap=138052、epd_writes=42、busy_fails=0、trusted=1；电池实时64%/3880mV/plugged=1。仅本轮响应正常，不证明历史从未崩溃或物理面板全部健康。

## 监听与证据边界

原生 os.open/read/close，不发送 DTR/RTS modem ioctl，不配置 termios、不清接收缓冲。14:17:30–14:17:50 首次只读监听；14:18:19–28发送只读 status/pmstats/log；14:18:43–49发送只读 diag/batt/pmstats。读写打开在沙箱被拒后通过提权执行。未使用 esptool 或主动复位。

两次 PM uptime=282.889s→307.922s，与采样时间间隔相符，期间未捕获 ROM reset/panic 文本。boot_id/reset_reason 未获得，首次打开前 uptime 未知，不能据此给首次打开前后复位保证。串口存在截断/交错，日志不是完整历史。

私有原始证据：artifacts/bridge-macos/note4-usb-20261105-{passive,cli,diag}.log。

## 下一步

通过本机 Bridge 首次配对 CodexStatus-B93408，核对同 MAC 后读取设备网络配置，再选择当前可用的 2.4GHz 网络；认证状态与真实数据 ACK 后验收恢复。尚未执行配对或写配置。关联既有 B6/C8，不新增完成结论。
