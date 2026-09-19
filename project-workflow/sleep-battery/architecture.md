# Architecture after the communication change (M3 and beyond)

Owner-facing consolidation, 2026-09-17. Supersedes nothing in `docs/history/sleep-plan-v4.md`;
fills in the cross-component consequences of moving the data path from
“device pulls during deep-sleep windows” to “device is LIVE, bridge pushes”.

## 1. Communication model

| Channel | Purpose | When | Transport |
|---|---|---|---|
| Wi-Fi HTTP pull | template fetch, OTA, status, fallback poll | any awake window / LIVE | device → bridge `GET /usage`, `GET /template`, bridge → device `GET /status.json` |
| Wi-Fi HTTP push | usage updates (primary data path) | LIVE | bridge → device `POST /usage` (Bearer endpoint token) |
| BLE | identity, trust, pairing, window fallback, endpoint refresh | DEEP windows, pairing, IP change | BLE GATT |
| Timer/button | wake source | DEEP only | ext1 (BOOT) / RTC timer |

Steady state with the PC on: device is **LIVE** (Wi-Fi associated, PM light
sleep, BLE off) and receives pushes ≤5 s (T9). DEEP exists only when the bridge
is unreachable; its windows are the recovery path, not the data path.

## 2. Per-component changes

### Firmware 0.11.0

- Custom core with `CONFIG_PM_ENABLE=y` + tickless; BLE controller off in LIVE.
- State machine: DEEP window success (same BSSID, bridge HTTP OK) → LIVE; exit
  on Wi-Fi lost >3 min, no sync 10 min, explicit request; min dwell 3 min;
  ≥5 min DEEP before retrying LIVE.
- `POST /usage` route: Bearer endpoint token → endpoint record → §2 active
  rules → `{"accepted":bool}`.
- Liveness: `WIFI_PS_MAX_MODEM`, DTIM/listen tuning, watchdog, 15 min fallback
  poll, `/status.json` reports `mode`.
- Keep the existing token-held OTA window path for when the device is DEEP.

### Bridge

- Address the device by mDNS (`codex-status-XXXX.local`) with the BLE-info IP
  cache as fallback; stop using the hardcoded `device_ip` for anything but a
  manual override.
- Pusher becomes dual-path: try `POST /usage` first when the device is LIVE or
  reachable; fall back to BLE window scanning only when HTTP has been
  unreachable for the GRACE window (10 min) or the device reports DEEP.
- Push triggers: usage fingerprint change + 5 min heartbeat; explicit profile
  push adds templates + `activate:true`.
- BLE scanning becomes demand-driven (user action, DEEP recovery, IP refresh),
  not a 24/7 background duty; the tray icon/error should reflect the data path
  actually in use.
- Device page: resolve via mDNS, show `mode`/`idle_reason`/last push; stop
  showing “BLE not found” as a hard error while Wi-Fi data is flowing.

### Panel / MCP

- Show device state badge (LIVE/DEEP/IDLE), last push and heartbeat; keep the
  idle-template picker.
- `bridge_status` gains `device_mode`, `last_push`, `next_heartbeat`.
- One-click OTA: needs a token decision (below).

### Templates

- quad `mode` variants and the IDLE screen stay as implemented; idle template
  is pinned and delivered with profile pushes (pull path covers DEEP).
- Preview pipeline stays four-way identical (firmware engine / Rust / Python /
  Node) and pixel-diffed.

## 3. Decision points to close before M3 acceptance

1. **OTA authorization in LIVE.** Current invariant: OTA token is negotiated
   over BLE. In LIVE, BLE is off by design. Options:
   a) accept the endpoint token (delivered over BLE, stored in NVS, already
      used for `/usage`) as OTA authorization — simplest, one-click OTA works,
      keeps the “token from BLE” root of trust, but changes the current 401
      invariant test; b) temporarily leave LIVE (BOOT 2 s) for BLE token
      handoff — no invariant change, but manual and slow; c) bridge keeps the
      last BLE-negotiated token? not possible (RAM-only, expires).
   Recommendation: (a), with the endpoint token verified against the record
   bound to `activeMac`; keep BLE token for DEEP/manual as-is.
2. **Idle vs LIVE naming for `device.state`.** `device.state` renders `IDLE`
   only; if LIVE templates later need a state row, extend to `LIVE` text
   (bind currently idle-only by spec).
3. **active across pushes.** With real push, A-bridge-always-on/B-machine
   blind spot (§2) shows up; keep explicit `activate:true` as the override and
   consider a panel “send to device now” that sets it.

## 4. Risks / costs

- Custom core build (pioarduino `custom_sdkconfig` or Arduino-as-component):
  NimBLE-Arduino 1.4.3 is core-2.x; core 3.x needs NimBLE 2.x API migration.
  Budget a build-migration spike before committing to the platform switch.
- LIVE current target ≤2 mA must be measured on hardware (T10); if unmet, M3
  does not pass and DEEP windows remain the fallback.
- IP drift and mDNS blocked networks: keep BLE-info IP cache and the endpoint
  refresh path.
- PC sleep/shutdown: device exits LIVE after 10 min (GRACE), renders IDLE with
  cached usage, and resumes LIVE on the first window after the bridge returns.

## 5. Execution order

1. Build spike: pioarduino + `custom_sdkconfig CONFIG_PM_ENABLE` compiling the
   current 0.10.0 sources unchanged; record time/size and NimBLE viability.
2. If green: firmware LIVE state machine + `POST /usage` + watchdog (0.11.0).
3. Bridge push path + mDNS addressing + demand-driven BLE.
4. Panel/MCP state surfacing, OTA decision (3.1) implemented.
5. Acceptance T9–T11 (T10 measured with the user), ROM + PROGRESS.

Until step 1–2 pass, M2 stays the shipped behavior; the only interim tweaks are
the manual-sync 30 s scan (done) and correcting misleading bridge error text.
