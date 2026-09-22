# Task 6 — Targets, UI pages, MCP, migration (M6/M7)

## Target abstraction

- `DeviceCapabilities`: `firmware_target`, `render_target`, width/height, pixel
  format, colors/gray, `compiler_abi`, `max_templates`, `max_bundle_bytes`,
  `max_fields`, `max_snapshot_bytes`, retention, BLE/Wi-Fi capability, partial
  refresh capability, safe max PowerPlan.
- Bridge pre-checks and the device validates independently: a mismatched target
  is rejected on both sides, including OTA (`firmware_target` + board revision +
  partition + image length/CRC).
- Different render targets use independent template variants (key =
  `template_id + render_target`); no auto scaling, no auto color downgrade;
  Profile references only the template id and the target device selects its
  matching variant.

Targets:

| target | state |
|---|---|
| `epd-ssd1681-200x200-1bpp` (154G, 200×200 B/W) | implemented, hardware-verified |
| `epd-200x200-2bpp-gray4` | software target: independent ROM env, variant path, OTA target reject, host tests; panel unverified |
| new second hardware | `blocked_by_hardware_arrival`: exact pins/panel/controller missing; config placeholder only, no guessed constants |

## UI (four pages, one application service)

1. Templates: latest per `template_id+target`, applicability, validate, preview,
   save, which profiles reference it, no revision/history/rollback, save≠publish.
2. Devices: MAC key, name, capabilities, owner/claim, Profile 1–8 order, active,
   bindings, explicit publish, sync toggle, PublishJob state, device-vs-expected
   differences, recovery read. Power submenu: current/provisional plan, remaining,
   rendezvous period, battery limit, explicit light/sleep debug.
3. Data: DataSource config, Codex + Static, field types, push/pull bindings,
   latest SourceSnapshot, validity/error/quality, probe, using devices, per-device
   full-sync threshold.
4. MCP: tools/connection state, permissions, results, same service; save never
   implicitly publishes.

Explicit states shown: saved-not-published, waiting for rendezvous, data pending,
applied-but-display-failed, waiting next rendezvous, target incompatible, device
owned elsewhere, new hardware not hardware-verified.

MCP tools must cover the same service operations, keep existing tools working
(legacy adapter where needed), never bypass token/owner, and never auto-publish
on save.

## Persistence and migration

- Runtime data stays under `<exe>/data/`; atomic file replacement; Profile,
  DataSource, device state and task summaries recover; frozen PublishJob content;
  active context; confirmed fingerprints and data_seq; PowerPlan query state;
  bounded, redacted logs; the minute clock never writes flash.
- Migration: legacy template library → latest-version model; legacy ≤3 profiles
  upgrade to 1–8 without silent truncation; legacy devices keep legacy capability
  with an explicit UI limitation note; new protocol is negotiated, never forced;
  a device that misses rendezvous must not silently fall back to periodic Wi-Fi;
  recovery-imported templates/profiles start with sync off; all migration paths
  have tests.
