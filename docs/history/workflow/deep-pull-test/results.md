# deep-pull 测试结果（2026-09-20）

测试固件：`#ifdef CODEX_DEEPPULL_TEST`（`src/main.cpp`），版本 `0.13.9-dptest`
（第一轮）/ `0.13.9-dptest2`（第二轮加 PM 模式统计），构建期
`PLATFORMIO_BUILD_FLAGS=-DCODEX_DEEPPULL_TEST=1`。设备 192.168.3.163，
bridge-app `:8765/:8766` 在线。每轮 5 个 deep cycle（30s 深睡 + RTC 快连 +
`GET /usage`），全部 `fast=1 code=200`（10/10）。

## 墙钟数据（`boot_ms`=setup 入口 esp_timer，`wifi_ms`=关联+DHCP，
`http_ms`=TCP+GET，`total_ms`=dpCycle 全程）

| 轮次 | cyc | wifi_ms | http_ms | parse_ms | total_ms |
|---|---|---|---|---|---|
| dptest | 0 | 1317 | 267 | 1 | 1637 |
| dptest | 1 | 1305 | 163 | 1 | 1517 |
| dptest | 2 | 1303 | 129 | 1 | 1480 |
| dptest | 3 | 1422 | 385 | 2 | 1855 |
| dptest | 4 | 1305 | 376 | 1 | 1733 |
| dptest2 | 0 | 1471 | 58 | 1 | 1578 |
| dptest2 | 1 | 1354 | 401 | 1 | 1803 |
| dptest2 | 2 | 1351 | 632 | 2 | 2031 |
| dptest2 | 3 | 3259 | 107 | 1 | 3413 |
| dptest2 | 4 | 1355 | 371 | 1 | 1774 |

- 快连（缓存 BSSID/信道，无扫描）关联+DHCP 典型 **1.3–1.5s**；HTTP 反向拉取
  584B envelope **0.06–0.63s**；JSON 解析 ~1ms。
- 一次 cycle 总量 **1.5–2.0s**（唯一异常 3.4s，关联慢）。
- 加 boot + setup（含 EPD 电源/初始化）约 0.9s，**从 boot 到拉取完成 ~2.7s**。

## CPU 时间（dptest2 最后一个 cycle，`Time since boot up: 2 699 874 µs`）

| Mode | 时间(µs) | 占比 |
|---|---|---|
| SLEEP (light sleep) | 592 505 | 21% |
| APB_MIN (40MHz) | 615 936 | 22% |
| APB_MAX (80MHz) | 736 099 | 27% |
| CPU_MAX (240MHz) | 752 920 | 27% |

- **CPU 运行态合计 2.11s（78%）**，light sleep 仅 0.59s：Wi-Fi 关联/DHCP/
  HTTP 期间驱动 PM 锁让 CPU 保持 APB 档，不是空睡。
- 锁统计：wifi `APB_FREQ_MAX` 19 次共 1.11s（41%）；rtos0+1 CPU_FREQ_MAX
  0.85s。
- 之前"大部分等待时间会 light sleep、CPU 0.3–0.7s"的估算偏乐观约 3–4 倍。

## 能耗粗算（供参考）

按模式电流粗估（240MHz≈45mA、80MHz≈22mA、40MHz≈15mA、sleep≈1mA）：
一次 cycle ≈ **0.017 mAh**。1min 间隔 ≈ 24mAh/天（~1mA 平均）；5min 间隔
≈ 5mAh/天。本次测试设备在充电（batt 87→91%，mV 4058→4122），不能用电压
斜率校准，数值仅供参考。

## 结论 / 注意

1. 反向拉取 5 个周期稳定成功，`GET /usage` 方向可行（桥侧接口现成）。
2. 主耗时是 Wi-Fi 关联+DHCP（~1.4s），不是拉取本身；快连参数落 RTC 有效。
3. deep 快连窗口里没有启动 WebServer（不需要），因此桥在 deep 期间无法
   push/OTA；产品化时若要窗口内 OTA，需在窗口内起 server 或先切 light sleep。
4. 设备已回滚原 ROM：`0.13.9-dptest2 -> 0.13.8-bw`（MCP `firmware_ota`，
   slot ota_1，next ota_0；`/status.json` 与桥 push/owner 正常）。

原始日志：`artifacts/deep-pull-log-dptest.txt`、`deep-pull-log-dptest2.txt`。
