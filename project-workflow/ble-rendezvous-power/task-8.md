# Task 8 — Stage 5: bridge resident listener, persistent revision, WAKE/RENEW/SLEEP

Owner: implementation (bridge crates; device contract fixed by task-6/7).

Inputs: design §4 (advertising/Windows discovery), §5.2/§5.3 (revision
tuple), §6 (bridge decision order), §7 (owner), §10 (compat),
`bridge/crates/ble`, `bridge/crates/core/activity.rs`, `envelope.rs`.

## Deliverables

1. **Resident listener (design §4)**
   - One adapter/event subscription for the process lifetime; passive scan
     where the backend supports it (record the actual API/scan mode when it
     does not — no unmeasured "passive" claim). Match on service/name, stop
     the scan on a match, connect, complete the transaction, then resume
     waiting. Jittered reties, at most one connection per rendezvous cycle.
   - Multi-device scheduling per MAC; the full WiFi MAC from INFO is the key
     (BLE address/name are not). No per-minute adapter rebuilds.

2. **Persistent revision domain + queue (design §5.2/§5.3)**
   - `(bridge_id, epoch, counter)` persisted in the data dir with an atomic
     write; restore after restart; new epoch only via the authenticated
     `POST /power` `SYNC_REVISION_EPOCH` path; counter overflow rolls to a
     new epoch (never wraps).
   - Usage merge: latest revision wins; template/OTA intent and cancellation
     state preserved; saving a template still does not push it.

3. **Decision + scheduling (design §6)**
   - Order: other owner -> no write; same revision and no clock sync needed ->
     no connection; small usage -> direct BLE atomic update; template/OTA/
     oversized usage/continuous interaction -> `WAKE_LIGHT`; after entering
     light verify identity, claim if needed, then run the authorized queue.
   - `RENEW_LIGHT` only with real in-flight work (e.g. remaining < 60 s), and
     `SLEEP_AFTER_ACK` when done; no unconditional background heartbeat.
   - Owner: `POST /claim` remains the only create/transfer path; a
     newly-bound/expired bridge first takes an authorized `WAKE_LIGHT` +
     HTTP claim, then uses BLE rendezvous. Owner mismatch answers
     `owner_conflict` (HTTP 409 semantics).
   - Time sync: `NOOP` carries `server_time`/`tz_offset_min`; a ~15 min
     (5–60 configurable, measured) cadence, immediately after a TZ/DST change.

4. **Diagnostics + MCP/panel**
   - Bridge logs discovery latency, opportunity sequence, ACK latency,
     retry/queue age; "expected deep / missed rendezvous / confirmed failure"
     are distinct states and a sleeping device's HTTP push failure is not a
     push alarm.
   - MCP `status`/`get`/`device_*` gain the v2 fields; explicit template/OTA
     queues are exercised end-to-end.

## Validation

- `cargo test --workspace` in an isolated `CARGO_TARGET_DIR`; unit tests for
  the revision persistence/epoch switch, decision order, renewal rules, and
  owner gating.
- Integration (device + tray app): NOOP-only, direct 512/2048 B usage, WAKE
  from BOOT, renewal under load, sleep-after-ack, OTA while light, bridge
  restart mid-cycle, PC sleep/wake, weak RSSI, two bridges competing.
- Legacy interop: a v1 device against the v2 bridge stays on the 60 s Wi-Fi
  pull; a v2 device against a v1 bridge keeps the legacy path.

## Rollback

Per-device `legacy_wifi_pull` config is the default until stage 6 passes;
the listener can be disabled without stopping the HTTP/tray functions.

## Field incident (2026-09-21): queued OTA retry aborts

- Symptom: an OTA queued while the device was deep was retried after wake,
  but every queued attempt aborted on the device right after
  `[ota] upload start` (`[ota] abort (aborted)`), so the bridge backed off
  60 s -> 2 m -> 4 m and the panel briefly flashed the OTA screen each time.
  The direct MCP `firmware_ota` call then appeared hung (the device had
  stopped reading while the client was still sending 1.6 MB, so reqwest
  blocked until its 120 s timeout, plus the 60 s version wait).
- The device was healthy: a direct curl multipart upload of the same ROM
  completed in 26 s (`UPDATE OK`), and after restarting `bridge-app` a bridge
  upload of a dummy 1.6 MB image completed normally (device answered
  `UPDATE FAILED` as expected). The long-running bridge process was the
  variable; the abort was client-side premature disconnects, not a device
  fault.
- Bridge log evidence (`bridge/target/debug/data/logs/bridge-app.log.2026-09-21`):
  repeated `doUpdate transport error: error sending request for url
  (http://<ip>/doUpdate?token=<redacted>)`, then `queued OTA attempt failed;
  backing off`. Note the log line includes the token in the URL query — see
  the security item below.
- Stage 5 actions: expose `pending_ota`/ROM path and attempt state in
  `bridge_status`; add a short settle after the `/diag?ota_abort=1` preflight
  before the POST; make `post_firmware` report bytes sent and fail inside a
  shorter bound so a stalled upload is visible instead of looking hung;
  serialize queued pushes against an in-flight OTA.
- **Security follow-up**: redact `token=` (and any credential query) from URLs
  in bridge logs/errors. Separately, `project-workflow/sleep-modes/prompt.md`
  (tracked, committed in e22d952) contains a plaintext device operation token;
  the token is still accepted by the device. Recommend rotating the device
  token over the bonded BLE link at the next BOOT session and purging the
  token from any new/edited prompt files.
- Workaround used: restart `bridge-app` (clears the in-memory queue) and
  upload the ROM directly; device token rules unchanged throughout.
