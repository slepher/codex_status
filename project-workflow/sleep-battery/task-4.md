# task-4 — M2 bridge (scan cadence, idle template, panel, MCP)

Status: implemented 2026-09-17; live acceptance pending.
Owned: `bridge/crates/ble/src/lib.rs`, `bridge/crates/core/src/{runtime,envelope}.rs`,
`bridge/crates/app/src/{main,config}.rs`, `bridge/crates/app/ui/index.html`,
`bridge/crates/mcp/src/lib.rs`.

## Implemented

- BLE cadence (`sleep.md §4.6`): 20 s rounds with a 5 s scan window; 12
  consecutive misses → 60 s rounds; after a successful cycle scanning pauses
  until the usage/template/idle fingerprint changes or the 5 min heartbeat
  expires (`ble_fingerprint`).
- `BleConfig.scan_timeout_ms` (5 s tray loop, 30 s one-shot tools).
- Explicit profile pushes also deliver the idle template (`profile_push` in the
  panel command path and MCP); the idle template is pinned on the device.
- Runtime idle template: `AppCtx.idle_template: Arc<RwLock<Option<String>>>`
  shared with the poller (envelope), BLE push and MCP; `set_idle_template`
  validates against the library, persists to `data/bridge-app.json` and wakes
  the BLE loop; `get_idle_template` for the panel.
- Panel: “待机模板（IDLE）” selector in the 配置 tab (loads with templates,
  saves on change, shows uninstalled idle ids).
- MCP: `McpConfig.idle_template`; `profile_push` appends the idle template.
- app-server polling stays at 60 s.
- Tray autostart (M1) unchanged.

## Remaining acceptance

- Start bridge-app, verify the device pulls within a window (T2), `epd_writes`
  stable across no-change windows (T3), idle template visible after two failed
  windows (T7), and the pushed quad v4 hash matches repo/Rust/Node (T8).
- Runtime `<exe>/data/templates/quad.json` is the old copy; seed updates do not
  overwrite it (by design). Update via panel save or copy before acceptance.
