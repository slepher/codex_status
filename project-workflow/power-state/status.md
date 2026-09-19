# Project Workflow Status

## Initiative
- Artifact directory: project-workflow/power-state
- Repository: codex_status
- Updated: 2026-09-18
- Plan revision: 4 (task-1..4/6/7 done; task-5 hardware steps pending)

## Repository snapshot
- HEAD: bcd8238 (`Firmware 0.12.5 + bridge: single-mode power state machine,
  explicit template push, MCP OTA`)
- Worktree: task-7 changes uncommitted in `src/main.cpp` (+`FW_VERSION
  0.12.6-bw`, `POST /template`), `bridge/crates/mcp/src/lib.rs`
  (`push_templates_http`), `bridge/crates/app/src/main.rs` + `ui/index.html`
  (HTTP profile push, BLE template path removed), docs/workflow updates;
  untracked `project-workflow/power-state/task-7.md`
- Running bridge: new build (task-7), parent PID 41820 + watchdog child;
  :8765/:8766 TCP and :8767 UDP listening

## Progress
- Current task: task-5 (device acceptance, hardware steps only); task-7
  (HTTP template push) is done
- Current phase: firmware 0.12.6-bw OTA'd and quad v7 (`c598adc0`) pushed and
  active over HTTP; host regressions green. Remaining: button/LED/power-state,
  battery slope (T10), field checks
- Exact next action (requires user + hardware): run the task-5 checklist —
  button pass, plugged/unplugged grace, low-battery cutoff, WIFI OFF cadence,
  T9/T10, UDP endpoint update, bridge-loss `OFF N M`, OTA regression

## task-1 result
- `FW_VERSION 0.12.0-bw`; single-mode state machine, USB-SOF plug detect,
  GP3 LED, BOOT 2/15/30 s, BLE session + `bleDeinit`, WIFI OFF cadence,
  low-battery power-off, UDP announce 8767, endpoint self-heal, new
  `/status.json` fields, `pmstats`, engine/store idle removal
- `platformio.ini`: `CONFIG_PM_PROFILING=y` (pmstats needs
  `esp_pm_impl_dump_stats`); ROM archived as above

## task-2 result
- Firmware engine already carried the v0.12 grammar from task-1; this task
  synced the host side: `core/template.rs`, `core/envelope.rs`,
  `runtime.rs`/`core/main.rs`, `render` (`state` replaces `idle`/`idle_reason`
  in `Env`/FFI), `mcp`, `app` (config/main/UI idle removal),
  `tools/test-bridge/bridge.py`, Node preview tools, quad v5 single layout

## task-3/4 result
- task-3: envelope carries `bridge.host/port` (`bridge_core::lan_ip()`);
  `device.rs` parses `mac/state/plugged/wifi_state/retry_stage`; app keeps a
  10 s device-status cache (`get_device_status` reads it), a UDP announce
  listener on :8767 (known MAC only; first contact from the configured
  address; `ble=1` triggers one BLE cycle), a live `device_ip` shared by
  push/BLE/panel, and a fully demand-driven BLE loop (no periodic scan)
- task-4: `PollerConfig.exe` is optional and the poller re-runs CLI discovery
  after every app-server failure (Codex auto-upgrade path move); a missing
  codex at startup no longer blocks HTTP/MCP/BLE; hidden watchdog
  (`--watchdog <pid>`) exits with a clean parent, relaunches abnormal exits,
  gives up after 3 in 5 min (`<data>/logs/watchdog.log`)
- runtime evidence: bridge PID 49768 + watchdog 25644; :8765/:8766/:8767
  listening; envelope OK; watchdog kill/relaunch proven; push to device 200 OK
- ROM: `artifacts/codex-status-0.12.5-bw.bin`
  SHA256 `7F13489935AB7CB371370FA52B4553F7B7E114D8FF59BE13F81D8BAE7C2E5A8D`
  (0.12.5 classifies windows by duration and adds the `monthly` selector
  (`windowMins >= 43200`) so free/go monthly plans can show MONTH while
  paid weekly plans keep WEEK; quad v7 `c598adc0`. 0.12.4 made template
  delivery explicit-push only; 0.12.3 persisted last-sync in NVS and fixed
  the false WIFI OFF; 0.12.2 defined bridge-loss `OFF N M`; 0.12.1 wrapped
  OTA-screen text and removed the pairing overlay)

## task-6 result (bridge OTA)
- `bridge-ble`: `request_device_token()` connects to an advertising device and
  requests the token over the bonded auth characteristic (10 s wait)
- `bridge-mcp`: new tool `firmware_ota {rom, device_ip?}`; token cache
  `<data>/device-token.json` with one automatic re-fetch on HTTP 401; uploads
  multipart to `POST /doUpdate?token=`, then polls `/status.json` for the
  version change (60 s); single-run guard
- app: MCP tool list string updated; `opencode.jsonc` MCP timeout 15 s → 180 s
- evidence: bridge (task-6 build) parent 28804 + watchdog 15764; `tools/list`
  shows `firmware_ota`; bogus path and no-BLE-session calls return actionable
  errors without uploading; **real OTA done**: 0.11.9→0.12.0 (40 s, token
  fetched over BLE), 0.12.0→0.12.1, 0.12.1→0.12.2, 0.12.2→0.12.3,
  0.12.3→0.12.4 and 0.12.4→0.12.5 (cached token, pure HTTP)

## task-7 result (HTTP template push)
- Firmware `0.12.6-bw`: `POST /template` (endpoint-token gated via
  `endpointTokenAuthorized`, raw JSON body, query `id/version/hash/activate`,
  `tplValidateForStorage` CRC/min_fw/dry-run, store + activate + redraw,
  `activeTplId` cache invalidation); 400/401/413/500 error paths
- ROM `artifacts/codex-status-0.12.6-bw.bin` SHA256
  `82B0AF5D52F6A718324FCB9B1DC3E3D41732019D8C6208E3E389C45751FF9E63`;
  OTA 0.12.5→0.12.6 via MCP `firmware_ota` (cached token, pure HTTP)
- `bridge-mcp::push_templates_http(cfg, ids, activate)`: GET `/status.json`,
  skip unchanged hashes, Bearer `cfg.token`, activation target sent last and
  re-sent when unchanged but not active; MCP `profile_push` and the panel's
  `push_profile` both use it; `PendingPush`/BLE template consumption removed,
  BLE loop only serves UDP `ble=1` endpoint/token handshakes
- runtime evidence: bridge parent 41820; `profile_push default` →
  `pushed 1 (quad); skipped 2 unchanged`; device `/status.json` quad
  `c598adc0` active + `/log` `[tpl] http saved id=quad`; second push
  `pushed 0 (skipped 3)`; `config-tbif` activated unchanged mini; error codes
  401/400/413 verified; `cargo test --workspace` 16, python 4/4, node 7

## Notes for acceptance
- OTA is done and reusable: click BOOT to open a BLE session only when the
  token cache is empty; afterwards `firmware_ota` works over plain HTTP
- Remaining acceptance is task-5 hardware/field work only: button/LED/power
  behavior, T10 battery slope, UDP endpoint update, bridge-loss `OFF N M`
