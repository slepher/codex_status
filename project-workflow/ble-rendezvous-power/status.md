# BLE rendezvous power design — status

Status: design complete; silent-wake implementation revised by task-3
(immediate Zzz removal, link-driven Wi-Fi icon); v2 protocol not started

- 2026-09-21: scope and acceptance criteria established.
- 2026-09-21: Astra subagent produced
  `docs/ble-rendezvous-power-design.md`; primary review confirmed bounded state
  exits, atomic/idempotent BLE updates, explicit light leases, unchanged
  claim/token invariants, and a separate high-ink refresh budget.
- 2026-09-21: user added the silent-wake requirement; synced into plan.md and
  design §3/§13, implemented as task-2 (firmware only).
- 2026-09-21: task-2 built (`pio run`) and OTA'd to the device:
  `0.15.0-bw -> 0.15.1-bw`, ROM `artifacts/codex-status-0.15.1-bw.bin`
  SHA256 `5F1F81A6941229476A194BBED9B09FBCBAD2F9294E59E853BB71E94FCB3E2D52`.
- 2026-09-21: physical BOOT-press verification passed (history `boot aux=3`
  (EXT1) -> `to-light aux=1`, `deep.glyph=5`, `epd_writes=1`).
- 2026-09-21: user revised the flow (task-3): Zzz is removed immediately on
  BOOT, Wi-Fi icon only after connect, hidden again in deep. quad v11 makes the
  Wi-Fi icon conditional on `device.state` (BLE ON/OFF) and drops the
  crossed-Wi-Fi icon; firmware 0.15.2-bw renders at wake and after connect and
  restores the sleep frame on failed battery connect.
- 2026-09-21: task-3 built and deployed: OTA
  `0.15.1-bw -> 0.15.2-bw` (ROM `artifacts/codex-status-0.15.2-bw.bin`,
  SHA256 `D20BBFEF10D0C47E64BAFFA5FFF70F81BE90575D29DAD66BD851C78C2CF0FCA`),
  quad v11 pushed (`profile_push default`, hash `93199731`). Device check:
  `render_mode=light` => 62 black px in the Wi-Fi cell; `render_mode=deep` =>
  0 black px there and 39 in the Zzz cell. `pio run`,
  `node tools/test-quad-preview.mjs`, and `cargo test -p bridge-core --test
  template` (updated to quad v11/93199731) pass.
- 2026-09-21: physical BOOT-press check of the revised sequence passed (user
  confirmed): history `enter-deep(60)` -> `boot aux=3` (EXT1) -> `to-light
  aux=1`, `epd_writes=3` after the wake boot, `deep.glyph=5`. Bridge debug deep
  cleared back to auto.
- Validation: `git diff --check` passed. Bridge code, GATT table, and
  claim/token rules unchanged; only the quad seed template + firmware changed.
- Next: failure path (AP/bridge unreachable -> restore Zzz and return deep)
  still untested; then measure BLE discovery/transaction energy and black-tile
  image quality, and split implementation into display safety, protocol v2,
  power states, bridge listener, and staged rollout.
