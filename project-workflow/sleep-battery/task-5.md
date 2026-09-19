# task-5 — M3 LIVE light sleep + proactive push (firmware 0.11.0 + bridge)

Status: started 2026-09-17 (user escalation: waiting for wake windows is not
acceptable when the bridge is reachable; LIVE + push must land).
Spec: `docs/history/sleep-plan-v4.md` §4.1/§4.3/§4.6/§6 M3; decisions D1–D5, D9.

## Why

M2 DEEP windows mean the device only syncs when it wakes (60/300/900 s) and is
unreachable otherwise, so freshness depends on the window cadence. The spec’s
steady state for a reachable bridge is **LIVE**: Wi-Fi association kept alive
with modem-sleep + automatic light sleep (0.5–2 mA), BLE deinitialized, and the
bridge pushing usage over `POST /usage` on fingerprint change + 5 min heartbeat
(T9: ≤5 s). DEEP remains only for boot/unreachable/exit.

## Build path (first task, decision required by experiment)

1. **pioarduino + `custom_sdkconfig`** (preferred): switch the platform to the
   pioarduino fork (Arduino core 3.x / IDF 5.x), set
   `CONFIG_PM_ENABLE=y`, `CONFIG_FREERTOS_USE_TICKLESS_IDLE=y`,
   `CONFIG_PM_SLP_DISABLE_GPIO` as needed; verify NimBLE/ArduinoJson libs still
   build. Rebuilding framework libs may take tens of minutes.
2. Fallback: Arduino as an ESP-IDF component with the same sources; NimBLE
   Arduino library would need an IDF port or replacement.
3. Do not ship a always-on “LIVE without PM” interim (D3 non-goal).

## Firmware (`0.11.0`)

- Enter LIVE after a window where the Wi-Fi path to the bridge succeeded
  (same BSSID); exit after Wi-Fi lost >3 min, no sync for 10 min, or explicit
  request; minimum dwell 3 min; after returning to DEEP wait ≥5 min before the
  next LIVE attempt (§4.1).
- `WIFI_PS_MAX_MODEM` + PM light sleep; BLE deinit in LIVE; DTIM/listen tuning.
- `POST /usage` with `Authorization: Bearer <endpoint token>`; token maps to
  `activeMac` / endpoint record; apply §2 rules; respond `{"accepted":bool}`.
- Keep fallback polling (15 min) plus template Wi-Fi fetch/OTA/status routes.
- Bridge restart GRACE 10 min; BOOT 2 s pairing window temporarily leaves LIVE.
- Watchdog: Wi-Fi down 3 min or no sync 10 min → DEEP exit sequence.

## Bridge

- Resolve the device (mDNS `codex-status-XXXX.local`, fallback cached IP from
  BLE info) and `POST /usage` on fingerprint change + 5 min heartbeat.
- Only mark/address when the device reports LIVE (or accept connection refusal
  as “stay DEEP”); no BLE window pushes needed while LIVE.
- One-click OTA can then assume the device is reachable in LIVE.

## Acceptance (T9–T11)

- T9 change → ≤5 s display; Wi-Fi off 3 min → back to DEEP.
- T10 average LIVE current ≤2 mA (hardware measurement; milestone fails >2 mA).
- T11 regression: token 401, bond intact, `/status.json`, OTA.

## Immediate state

- bridge-app is running (PID logged in `artifacts/bridge-app-run.pid`), serving
  `idle_template: quad`, quad v4 `d3253df5`.
- Device 0.10.0 is in a 900 s failure backoff; the first window with the bridge
  up restores 300 s and completes M2 T2/T3. M3 flashing needs one such window +
  a token (10 min hold).
