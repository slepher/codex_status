# pmstats 专项状态

- 2026-09-19：task-1 完成（固件 0.13.0-bw 已 OTA、bridge/MCP/面板已部署并
  实测）；**task-2 根因定位完成**——微睡眠 = `loop()` 的 `delay(5)` 轮询节奏，
  非异常唤醒源（详见 task-2.md 表格：delay 5→50ms 时入睡 134→29.5 次/s、
  SLEEP 80→90%）。修复已落地：0.13.3 空闲 25ms + 传输时自动 5ms。
- 设备：0.13.3-bw（ota_0，next ota_1）、BLE OFF、`pm_light_sleep=true`、
  电量 81%（墙上充电，`plugged=0` 属正常）；空闲 `loop_delay=25ms`（有 TCP
  客户端/OTA 时自动 5ms）。
- 固件：0.13.1（timer dump）/ 0.13.2（sleep diag + `POST /diag` + BOOT 2s
  不再开 BLE）/ 0.13.3（空闲默认 25ms）均已 OTA；ROM SHA256 见 task-2.md /
  PROGRESS.md。
- 桥：父 PID 31712 + watchdog 9844（target/debug 新构建），:8765/:8766 监听。
- 未提交：`src/main.cpp`、`platformio.ini`、`docs/power-state.md`、本目录文档。
