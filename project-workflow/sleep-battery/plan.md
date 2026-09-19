# Sleep & Battery

Authoritative spec: `docs/history/sleep-plan-v4.md` (v4, decision-closed). This initiative tracks its
execution. No scope changes are allowed without a spec revision.

## Goal and baseline

Battery endurance for the 1.54" B/W panel: DEEP (deep sleep windows, µA) and
LIVE (Wi-Fi keep-alive, 0.5–2 mA) modes, driven by bridge reachability.
Baseline HEAD `941ad3e`, firmware 0.8.0-bw, all three implementations green.
Device at 192.168.1.50 / SSID `home-wifi`, BLE `CodexStatus-AABBCC`, USB unplugged.

## Constraints (from docs/history/sleep-plan-v4.md)

- Protocol changes are three-way (firmware engine / `bridge/crates/core/template.rs` /
  Python test bridge) plus `tools/generate-quad-preview.mjs`; unknown `type/font/bind`
  rejects the whole template.
- Preview must remain pixel-identical (`crates/render` shares the C++ engine).
- Token gate (401) and BLE bond semantics are untouched.
- Push is explicit (per profile), save never pushes.
- Running bridges are user services: never stop them without asking.
- No secrets in the repo or logs.

## Milestones

### M1 — low-risk fixes and infrastructure (firmware 0.9.0)

task-1. Panel sleeps after every refresh and is re-initialized before drawing;
BLE advertises only while a window is active and stops before sleep; `mode
auto|deep|live` CLI; no minute-sync re-render in deep mode; BLE info carries
device IP/HTTP port (refreshed when the IP changes); bridge tray autostart.
mDNS needs no firmware change: `ArduinoOTA.begin()` already starts it
(`_mdnsEnabled` default true); acceptance only re-verifies `.local` resolution.

DoD: `pio run`; `epd_writes` stable in no-change windows; `codex-status-XXXX.local`
resolves (regression); `/status.json` intact; cargo tests green.

### M2 — DEEP dual mode + IDLE template (firmware 0.10.0 + bridge + templates)

task-2. Protocol: element `mode`, idle binds (`device.state`,
`device.offline_mins`, `device.idle_reason`), envelope `idle_template` /
`active_hold_seconds`, `min_fw 0.10`. Quad idle variants. Firmware sleep
decoupling, RTC state, scan/last-used/BSSID selection, persisted usage cache,
IDLE render + built-in fallback screen, AP policy D10, failure/backoff/accelerated
windows. Bridge scan cadence, idle template selection + pin, panel picker, MCP
fields, profile delivery. Scenarios §4.8.

DoD: `pio run`, isolated `cargo test --workspace`, `node
tools/generate-quad-preview.mjs`, `node tools/test-quad-preview.mjs`,
`git diff --check`; T1–T8; ROM archived + PROGRESS.md.

### M3 — LIVE light sleep (firmware 0.11.0 + bridge)

task-3. Custom Arduino core (PM + tickless, BLE controller off), LIVE state
machine, `POST /usage` with token + active rules, watchdog, DTIM tuning,
fallback polling; bridge mDNS addressing + push on fingerprint change / 5 min
heartbeat. DoD: T9–T11, average LIVE current ≤2 mA, ROM archived + PROGRESS.md.

## Evidence and recovery

- ROMs/artifacts under `artifacts/` (gitignored).
- `PROGRESS.md` updated at each milestone with device field state and hashes.
- Hardware acceptance needs the user for OTA/power; do not stop running services.
