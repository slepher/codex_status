# pmstats 专项状态

- 2026-09-19：task-1 完成（固件 0.13.0-bw 已 OTA、bridge/MCP/面板已部署并
  实测）；随后进入 **task-2 微睡眠调研**（进行中，交接新窗口，见 task-2.md）。
- 桥：父 PID 31712 + watchdog 9844（target/debug 新构建），:8765/:8766 监听。
- 设备：0.13.0-bw（ota_1）、BLE OFF、pm_light_sleep=true、电量 90%（墙上充电，
  `plugged=0` 属正常——纯充电器不被 USB-SOF 识别）。
- 未提交：所有改动等用户同意；下个固件改动记得 bump `0.13.1-bw`。
