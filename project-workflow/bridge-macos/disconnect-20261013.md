# Note4 长时间断连调查（2026-10-13）

## Plan / task

用户要求提交后调查历史断连；提交为 `5ce379b`。保持恢复后的设备和 Bridge 现场，先对照两端日志，再用生产处理函数复现。关联待办 B6/C8。

- [x] 核实 HTTP 成功间隔、BLE 会合轨迹和设备诊断归档。
- [x] 复现 Wi-Fi 未初始化时 endpoint 回执身份错误，修正稳定 MAC 来源。
- [x] 运行定向回归并记录部署边界；本轮不刷机或重启 Bridge。

## Status / evidence

中国标准时间 17:02:51.059 至 18:10:36.868 无成功认证 HTTP status，间隔 4065.809 秒。期间 BLE INFO 多次验证 MAC 7C4FADB93408、wake_generation 2443911192，随后约 5 秒失败，未进入 status 命令。17:32 替换 Bridge 位于中断期间，不能解释中断起点。

该区间共有 29 次 BLE 身份确认和 29 次 rendezvous failed。恢复后的只读 `/status.json` 再次核对同一 MAC/.99，`/history` 第 69 醒次为 timer：BLE 连接发生于 2300ms、无 command、无 Wi-Fi IP、9264ms 收尾；第 70 醒次为 button/ext1，6600ms 获得 Wi-Fi IP、10800ms HTTP 有结果。恢复由物理按键触发，不能当作自动定时会合恢复证据。没有串口设备可供无扰监听，本轮未打开串口。

只读运行日志位于用户私有 data/logs/bridge-app.log.2026-10-13。私有诊断批次 629 在 17:02:52 记录 light_exit、Plan 3886 remaining=0；630 在 18:11:13 收集到一系列 BLE rendezvous deadline，随后第 70 醒次 Wi-Fi association/IP 成功。未接串口、未重置设备，未改运行数据。

生产 endpoint ACK 使用 `WiFi.macAddress()`，但 BLE-first 自动唤醒时 STA netif 尚未初始化。已安装 Arduino SDK 的 NetworkInterface::macAddress 在 netif 为空时输出全零 MAC；Bridge 必须严格匹配目标 MAC，忽略该 ACK，5 秒超时，后续启动网络指令不可达。既有 `radio::macText()` 读取初始化时从 eFuse 缓存的 MAC，不依赖 Wi-Fi。需要真实处理函数回归确认，不放松 Bridge 身份核验。

## Review / acceptance

`python3 tools/test-ble-endpoint-identity.py --out artifacts/bridge-macos` 提取实际 `handleBleEndpoint`、`radio::macText`，编译生产 BLE wire codec；仅 peer/storage/Wi-Fi HAL 代替硬件。原代码 Wi-Fi off 时输出全零 MAC，断言失败；替换为 `radio::macText()` 后 8 项通过（off/on × bonded/unbonded × verify/persist），verify 和拒绝路径保持零存储写。`python3 tools/test-ble-first-contact.py`、`git diff --check` 通过。没有更改协议、存储、PowerPlan、身份核验或 NVS。

已安装 SDK 的 `WiFiSTA.cpp:300` 委托 `STA.macAddress()`；`NetworkInterface.cpp:657–674` 在 netif 为空时保留零数组并格式化全零 MAC。`application.cpp` 在 BLE 初始化前调用 `loadHostMac`。BLE `write_endpoint_until` 只有完整 MAC/request/principal 匹配才接受 ACK，不匹配会等到 5 秒截止。故障位于 Wi-Fi 建链之前，不是已发起的 Wi-Fi 关联失败。

历史日志未保存 endpoint ACK 字节，不能声称已现场抓到全零回执；源码/SDK 复现、29 次时序和设备醒次记录共同强力支持此根因。修复仅在源码，未构建新 ROM/未刷机/未重启 Bridge；当前 `.99` 仍可能在下一次冷启动定时会合复发。最终硬件闭环需新 ROM 的自然深睡/定时唤醒，无人为 BOOT 干预验证。安全摘要：`artifacts/bridge-macos/disconnect-20261013-summary.json`。
