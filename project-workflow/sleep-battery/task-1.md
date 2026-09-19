# task-1 — M1 low-risk fixes and infrastructure (firmware 0.9.0)

Spec: `docs/history/sleep-plan-v4.md` §6 M1 (v4, revised 2026-09-17: mDNS is provided by
`ArduinoOTA.begin()`; no firmware `MDNS.begin()` is added). Baseline `941ad3e`,
`pio run` green.

## Owned files

- `src/main.cpp`, `src/EPD_SSD1681.cpp/.h`, `src/ble_bridge.cpp/.h`
- `bridge/crates/app/src/main.rs` (+ `autostart.rs`), `Cargo.toml`
- `bridge/crates/core/tests/template.rs`, `tools/test-bridge/templates/quad.json`
  (stale-test repair, see below)
- `project-workflow/sleep-battery/*`

## Changes

1. Panel sleep: `epdFlush()` puts the panel into deep sleep mode 1 after every
   refresh; `EPD_SSD1681_WakePartial()` (init + re-seed 0x26 from the last
   displayed frame) runs before the next partial, and `EPD_SSD1681_Init()` wakes
   for a full refresh. RAM-retention mode 1 keeps the previous-RAM baseline.
2. BLE windowed advertising: `bleAdvertiseStart/Stop`; `enterDeepSleep()` stops
   advertising before sleeping; advertising starts on boot.
3. `mode auto|deep|live` serial CLI, persisted in NVS `cfg/mode`; `deep`/`live`
   both enable sleep windows until M3 (`live` = deep). `auto` keeps the legacy
   `cfg/batt` gate.
4. Render debounce: minute/battery re-render tick is skipped when sleep windows
   are enabled; framebuffer memcmp still suppresses identical writes.
5. BLE `info` carries `ip` / `http_port`, refreshed when the device IP changes
   (`updateInfoExtra()` on the 10 s tick). No `MDNS.begin()` (ArduinoOTA already
   starts mDNS); `codex-status-XXXX.local` verified resolving.
6. Bridge tray: `开机自启` writes/removes `HKCU\...\Run` (`CodexStatusBridge`),
   check state derives from `autostart::matches_current_exe()`; first BLE cycle
   remains immediate on start.
7. Stale-test repair (pre-existing at baseline): commit 0545945 dropped
   `"version"` from `quad.json`, failing `hashes_match_python_test_bridge`.
   Restored `"version": 3` (content did change in 0545945) and updated the
   assertion.

## Out of scope

- No GATT table change (Windows cache re-pair).
- No mandatory window sleep / IDLE template / RTC state (M2).
- No protocol JSON changes.

## Verification (2026-09-17)

- `pio run`: SUCCESS (RAM 62500 / Flash 1306373).
- `cargo check -p bridge-app`, `cargo build -p bridge-app`: clean, no warnings.
- `cargo test --workspace`: 15/15 pass (2 BLE + 7 envelope + 6 template).
- `node tools/generate-quad-preview.mjs` + `node tools/test-quad-preview.mjs`:
  5 fixtures pass.
- Device OTA (token via `tools/device-auth/request_token.py`): `UPDATE OK`,
  `/status.json` shows `fw 0.9.0-bw`, slot ota_1, templates/bonds/NVS preserved.
- `/log`: `mode=auto`, no mDNS error lines; mDNS `codex-status-AABBCC.local`
  resolves (ArduinoOTA-provided).
- BLE `info` read: `"ip":"192.168.1.50","http_port":80`, peerBonded/Encrypted.
- Test bridge running: device fetched `channel: WIFI`, rendered quad;
  `epd_writes` stayed 4 across a 15 s unchanged-data sample (`epd_partial=true`).
  DEEP-window no-growth (T3) is deferred to M2, where the window sleep exists.
- ROM archived `artifacts/codex-status-0.9.0-bw.bin` SHA256
  `04D9662E31F1A976646B02CA19F812B18ADC99889F9172013F72512ADB432174`.

`git diff --check` reports CRLF-at-EOL warnings for the C++ files only; this is
the repo baseline (same warnings on commit 24ecd96) and not introduced by M1.
