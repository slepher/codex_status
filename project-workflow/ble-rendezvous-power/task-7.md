# Task 7 — Stage 4: firmware rendezvous, clock-first, light lease, `POST /power`

Owner: implementation (firmware; bridge stays on the legacy pull path as the
default until stage 5).

Inputs: design §3 (state machine, timeout table), §5 (WAKE_LIGHT),
§6 (lease rules), §10 (compat), task-6 protocol, `src/main.cpp` deep paths.

## Deliverables

1. **Deep rendezvous cycle (v2 mode only, behind `pm/rv2`)**
   - RTC minute wake order: restore clock, thin clock window write first (a
     BUSY panel cannot swallow the advertising window), then BLE init
     (2 s) + advertise 1.5 s (adaptive rule off by default, §4) + bonded
     encrypted connect (3 s) + one transaction (8 s) with the 15 s total
     radio hard cutoff.
   - No command / NOOP / timeout: radio off, deinit, back to deep on the
     regular minute anchor; missed ticks are not replayed. No Wi-Fi is ever
     started by the rendezvous.
   - Direct small usage update commits to RTC-cached state, refreshes the
     screen under the stage-2 safety policy, ACKs, deinits, deep.
   - Radio-off cleanup has a bounded 2 s watchdog path that forces a direct
     deep entry (RTC marker) instead of a reset loop.

2. **Light lease (design §6)**
   - `WAKE_LIGHT(lease_s)`: accepted only in rendezvous; granted 30–600 s,
     `wake_ack(granted_s, lease_id)`; device then deinits BLE, fast-connects
     Wi-Fi (15 s cap, counted against the lease), switches to light.
   - BOOT/PWR deep wake defaults to a 300 s lease; the physical BOOT path
     keeps the 0.15.2 silent-wake visuals (no `Connecting:` page, Zzz clears
     first, Wi-Fi icon after connect, sleep frame restored on failure).
   - Deadline is `monotonic_now + granted_s`, never wall clock; only
     `RENEW_LIGHT` extends it; repeated request ids return the original
     deadline; state reads/claim/data/transfer do not renew.
   - Expiry: reject new BEGIN/OTA, finish in-flight work within
     `min(own deadline, lease_deadline + grace)`, abort at the hard boundary,
     release PM locks, deep.

3. **`POST /power` (authenticated HTTP control while in light)**
   - Bearer device token + hostId/owner check + `lease_id` match;
     `RENEW_LIGHT`, `SLEEP_AFTER_ACK`, `SYNC_REVISION_EPOCH` (new epoch
     registration); ACK first, then bounded drain. `wake_ack` never claims
     the AP is up.
   - Endpoint is added to `/diag`-style token gating; no token rule is
     loosened anywhere (`/update`, `/doUpdate`, ArduinoOTA, `POST /claim`
     unchanged).

4. **Deinit/OTA wind-down checks (`pm_cleanup_fail` telemetry)**
   - Every exit path (no command, NACK storm, timeout, lease expiry, OTA
     abort, low battery) releases the BLE controller and its PM lock before
     deep; failures are counted and force the next wake to skip radio and go
     straight to deep.
   - OTA in a lease honors 20 s stall / 180 s total and falls back to the old
     slot; low battery aborts first.

## Validation

- On-device, `pm/rv2=1`, explicit sessions: NOOP-only cycle, 512 B update
  (screen only after COMMIT), WAKE_LIGHT 60 s then expiry, `RENEW_LIGHT`
  extension, repeated RENEW id (no extension), `SLEEP_AFTER_ACK`, OTA inside
  a lease, forced BLE init/deinit failures.
- `/history` gains the rendezvous events from §11; each deep return is
  verified by `hist` and `/status.json` (`mode=deep`, `next_rendezvous_in_s`).
- Legacy path regression: `pm/rv2=0` keeps the 0.15.2 behavior byte-for-byte
  at the HTTP/BLE interface.

## Rollback

`pm/rv2=0` returns to the legacy 60 s Wi-Fi pull; BOOT recovery (stage 6)
will also force legacy without HTTP. No GATT change.
