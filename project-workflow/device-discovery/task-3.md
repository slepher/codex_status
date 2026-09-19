# task-3 — 固件 0.13.4-bw：关 mDNS + BLE info 带 mac

状态：done（2026-09-19，已 OTA 实测）

## 改动（`src/main.cpp`）

1. `ArduinoOTA.setMdnsEnabled(false)`（`begin()` 前）+ 删除
   `MDNS.addService("http","tcp",80)`；`WiFi.setHostname()`（DHCP option 12）
   保留，hostname 仍为 `codex-status-<MAC 后缀>`。
2. `updateInfoExtra()` 增 `"mac":"<WiFi MAC>"`（与 `/status.json.mac` 同源）。
3. ROM 归档 `artifacts/codex-status-0.13.4-bw.bin`（1603200 bytes）
   SHA256 `97DE8F3FB50E9469B47C10C09BDEC7B11E3E28E4EC2F528FD37D085FBEBD1B48`。

## 验收证据

- `pio run` OK（RAM 20.5%、Flash 50.0%）；MCP `firmware_ota` 实测
  `0.13.3-bw -> 0.13.4-bw`（1603200 bytes，约 32 s），槽位 ota_1 → 重启后
  ota_0。
- mDNS 关闭：`GET /pmstats?diag=1` 任务表无 `mdns` 任务、`?timers=1` 无
  `mdns_timer`；`ipconfig /flushdns` 后
  `Resolve-DnsName codex-status-AABBCC.local` 失败（首次查询命中旧 mDNS 缓存，
  清理后确认失效）。
- `/status.json` 正常（fw/uptime/owner 等）、HTTP/MCP 推送与 `/log` 正常。
- BLE info `mac`：用户单击 BOOT 后 `device_discover via=ble` 实测 info JSON
  `{"fw":"0.13.4-bw","mac":"70:04:1D:AA:BB:CC","ip":"192.168.1.50",
  "http_port":80,...}`（bond 复用，无重配）。
- `git diff --check` 干净。
