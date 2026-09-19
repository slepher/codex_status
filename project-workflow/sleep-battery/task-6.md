# task-6 — 0.10.3 hardening: ext1 wake, PWR power, persistent token; bridge demand-driven BLE

Status: implemented 2026-09-17; hardware acceptance passed for the wake paths,
battery PWR power-off and bridge deployment still pending.
Spec: `docs/history/icons-task.md` next steps 1–2 (follow-up to `task-5.md` M3 Plan B).

## Why

M3 Plan B shipped 0.10.2 with three known gaps from the prompt handover:

1. BOOT ext1 deep-sleep wake did not work (the plan asked for RTC pull
   hardening and PWR in the wake mask).
2. PWR (GPIO18) soft power-off was unimplemented; the live PWR button did
   nothing.
3. The OTA token was RAM-only, so every reboot/deep sleep invalidated it and
   the token-gated OTA route needed a fresh BLE handshake.
4. The bridge scanned BLE every 20 s even while `POST /usage` was healthy and
   reported "ble: device CodexStatus-* not found" as a hard error, which is
   expected while the device is LIVE (BLE off by design).

## Firmware 0.10.3

- `armWakeSources()`: RTC pull-up/down for GPIO0 + GPIO18 plus
  `esp_sleep_enable_ext1_wakeup((1<<0)|(1<<18), ANY_LOW)` and the timer; all
  four sleep sites (window end, LIVE exit, /sleep, AP idle) go through
  `deepSleepFor()`.
- PWR long press (3 s) drops the VBAT latch (GPIO17 LOW). The hold is not
  gated on USB presence (`HWCDC::isPlugged()` proved unreliable after a cable
  unplug: it stayed true and the hold was silently ignored); if USB/charger
  power keeps the board alive, the hold degrades to a clean restart after 3 s.
  One action per hold via a latch flag. PWR is ignored for the first 5 s after
  boot so a power-on long press is not immediately read as a shutdown.
- `GET /status.json` gains `pwr` (GPIO18 level) for remote button debugging.
- Token: auto-issued at first boot, persisted in NVS (`auth/token`), loaded at
  boot; BLE `{"cmd":"token"}` returns the current token, optional
  `{"cmd":"token","rotate":true}` rotates it. Factory reset clears it. Still
  only disclosed over the bonded BLE link (never HTTP/serial).
- `/status.json` gains `wake` (`power-on`/`ext1`/`timer`/…); boot log prints
  `wake=N(name)`.
- Debug CLI over USB: `sleep [sec]`, `pair` (opens the BLE window from LIVE).

## Bridge

- `last_push_ok_at` tracks successful `POST /usage`; while fresh (< 360 s) the
  BLE loop skips scanning; on a fresh bridge run it waits up to 8 s for the
  first push before scanning. A single push timeout must not flip the path:
  only two consecutive failed attempts clear the healthy timestamp (the device
  WebServer occasionally misses a request). The push-check cadence is 3 s so
  envelope changes reach the device inside the T9 ≤5 s budget.
- BLE cycle failures no longer set `last_error` while the HTTP path is healthy
  (debug log only); HTTP success clears stale `ble:` errors and updates
  `last_sync`.
- Push fingerprint excludes `server_time` and the rolling `resetsAt` of unused
  windows (app-server reports `now + window` every poll while `usedPercent` is
  0), so steady-state pushes are heartbeat-only instead of per-minute; EPD
  partial refreshes drop from ~1/min to ~1/5min.
- Tray "OK/stale" threshold now covers the 5 min heartbeat.
- Startup: `tools/start-bridge.ps1` launches the tray detached with
  `UseShellExecute=true` + hidden window + file logs and returns immediately
  (no inherited stdio handles, so callers never block).

## Evidence (2026-09-17)

- `pio run` OK; flash via USB (COM4) then serial ring shows
  `v0.10.3-bw ... wake=0(power-on)`, `[auth] token initialized in NVS`.
- Timer: `POST /sleep?sec=60` with the BLE token → `reset=deep-sleep
  wake=4(timer)`, device back LIVE.
- BOOT: short press during 900 s sleep → `reset=deep-sleep wake=3(ext1)`.
- PWR: short press during 180 s sleep → `reset=deep-sleep wake=3(ext1)`.
- PWR long press (battery, USB unplugged): `pwr=0` seen over `/status.json`,
  device offline 4 s later and stayed off; next long press cold-boots
  (`reset=power-on`, `[pm] RTC state initialized` = RTC domain really lost
  power). Button semantics: long press ≥3 s powers off; in the off state only a
  long press powers back on; a short press does nothing in either state.
- PWR is readable on USB too (hold detected); the hold is ignored/restarted as
  configured.
- Battery trend on LIVE (stock-core Plan B): 75 %/3976 mV → 45 %/3708 mV in
  ~3 h (~10 %/h, tens of mA) — the ≤2 mA T10 target needs the M3 PM light
  sleep (custom core), not modem sleep alone. A 5 min background sampler writes
  `artifacts/battery-log.csv` for the LIVE upper bound and the overnight T1
  check.
- Token persistence: same token accepted for `/sleep` after two deep-sleep
  cycles and after a reflash (`[auth] token loaded from NVS`).
- T9 transport: direct `POST /usage` with the endpoint token returned
  `{"accepted":true}` in 0.95 s wall time with `epd_writes` already
  incremented; bridge push-check cadence tightened to 3 s (observed envelope
  refresh → push in 2.0 s).
- Regression: `POST /doUpdate` without a token → 401.
- Bridge: `cargo check --workspace --offline` clean.

## Pending

- M2/M3 acceptance: T10 LIVE current, T1 DEEP overnight, T7 IDLE, T8
  three-way consistency on the device (T9 transport verified).
- `PROGRESS.md` + commit for 0.10.3; ROM archived as
  `artifacts/codex-status-0.10.3-bw.bin`, SHA256
  `C0AABDC117D88D8FB073306C27F0BAD5DB7967600A1ACFAF019B7EFE5780B61D`.
