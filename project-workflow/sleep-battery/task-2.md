# task-2 — M2 protocol and template assets (docs/history/sleep-plan-v4.md §6 M2.1–2.2)

Status: complete (2026-09-17). task-3 (firmware) and task-4 (bridge) follow.

Owned: `src/template_engine.*`, `bridge/crates/core/src/{template,envelope,runtime}.rs`,
`bridge/crates/render/src/*`, `bridge/crates/{core,app,mcp}` call sites,
`tools/generate-quad-preview.mjs`, `tools/test-quad-preview.mjs`,
`tools/test-bridge/bridge.py`, `tools/test-bridge/templates/quad.json`,
core tests.

## Protocol (frozen)

- Element `mode`: `idle` | `live` | `any` (default `any`). LIVE render skips
  `idle`; IDLE render skips `live`. Unknown values reject the template.
- Binds resolved only in IDLE rendering (not-exists in LIVE):
  `device.state` -> `IDLE`; `device.offline_mins` (exists with a last-sync time);
  `device.idle_reason` (`boot`/`wifi_lost`/`bridge_lost`/`env_switch`).
- Envelope: `idle_template` (string, bridge-selected; omitted when unset) and
  `active_hold_seconds` (default 600).
- quad: `min_fw 0.10`, `version 4`, bottom-left variants per docs/history/sleep-plan-v4.md §4.4.

## Implemented

- Firmware engine: `TplEnv.idle/offlineMins/idleReason`; `mode` parsing and
  per-mode element filtering; the three idle binds; dry-run validation accepts
  them.
- Rust: `BindSpec::{DeviceState,DeviceOfflineMins,DeviceIdleReason}`,
  `mode` whitelist, `EnvelopeOptions.{idle_template,active_hold_seconds}`,
  `PollerConfig.idle_template` shared `Arc<RwLock<Option<String>>>`.
- Render FFI/CLI: `idle`/`offline_mins`/`idle_reason` env fields
  (`bridge-render --idle --offline-mins N --idle-reason R`); MCP
  `template_render` accepts the same keys.
- Node generator: new binds, `mode` filter, idle previews
  (`quad-preview-idle.png`, `quad-preview-idle-missing-5h.png`).
- Python test bridge: envelope `idle_template`/`active_hold_seconds` with
  `--idle-template` / `--active-hold` args.
- quad.json: LIVE/IDLE variants (5H 158/144, BATT 172/158, state 172, OFF 186,
  SYNC live-only 186), `min_fw 0.10`, `version 4`.

## Verification

- `pio run` SUCCESS; `cargo test --workspace` 17/17 (2 BLE, 8 envelope,
  7 template); `node tools/generate-quad-preview.mjs` + `node
  tools/test-quad-preview.mjs` pass (7 fixtures incl. idle).
- Pixel parity: `bridge-render --diff` = 0 against both `quad-preview.png`
  (live) and `quad-preview-idle.png` (idle) with the C++ engine.

