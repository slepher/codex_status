# Task 6 — Stage 3: BLE transaction protocol v2 (explicit test sessions only)

Owner: implementation (firmware first, then a test tool; bridge default
transport unchanged until stage 5).

Inputs: design §5 (all subsections), §7 security, task-4 routing/switch,
`src/template_xfer.*` (legacy path to keep), `src/ble_bridge.*`,
`bridge/crates/ble`.

## Deliverables

1. **Capability negotiation + session (design §5.1/§5.2)**
   - INFO exposes `rendezvous_v`, max control size, supported types, radio
     budget once v2 is negotiated for the session; the session nonce is
     returned only after the bonded+encrypted status handshake, never in the
     advertisement, never in INFO.
   - `rv:2` control JSON is dispatched by the task-4 router; `request_id`
     uniqueness enforced per session. All commands require the encrypted,
     bonded link; status read/notify is gated server-side so a later client
     cannot read a previous session's ACK or secret.

2. **V2 transactions on the existing characteristics**
   - `NOOP` (optional `server_time`/`tz_offset_min`, `noop_ack`; no usage
     revision change, no lease renewal).
   - `UPDATE_BEGIN` (`revision`, `length`, `crc32`, `type`; `begin_ack(
     next_offset)`): identity/owner/type/size/budget checks, staging
     allocation, `type=usage` only in the first release; `type=template|
     firmware` answers `needs_wifi`; over-budget answers `nack` up front.
   - `UPDATE_CHUNK` binary framing `0xB2, request_id u32LE, offset u32LE,
     data`; strict contiguous offsets, `MTU-3` per write, duplicate identical
     chunks return the original `next_offset`, holes/overlap-conflict/out-of-
     range NACK and cancel.
   - `UPDATE_COMMIT`: verify length + IEEE CRC32 + JSON schema + identity +
     owner + revision, build the candidate usage/frame, then swap active
     state; failure clears staging and changes nothing.
   - ACK/NACK via `0xB3`-framed status notifications (magic/message_id/index/
     count, ≤`MTU-3`, reassembly cap 512 B / 1 s) plus the read-back path.
   - Idempotency: one commit per revision; duplicate tuple is an idempotent
     ACK with no redraw and no lease extension; `stale_revision` /
     `revision_conflict` per §5.3; new epoch switching is **not** accepted
     from an unauthenticated BEGIN (reserved for the authenticated
     `POST /power` registration in stage 4).
   - `applied_revision` vs `displayed_revision` stored separately; display
     pending/failed never claims success and never rolls the data back.

3. **Radio/session timeouts**
   - Per-transaction 8 s, 2 s no-progress, total radio-on 15 s from
     initialization; ACK drain 500 ms. All exits deinit BLE and return to the
     caller's sleep path deterministically; BLE callbacks only enqueue into a
     bounded staging buffer, never touch Wi-Fi or the panel.

4. **Explicit test session (no default mode change)**
   - With `pm/rv2=1` the device accepts v2 on an explicit BLE session (BOOT
     click / diagnostic tool); with the switch off it answers `rv:2` control
     with `not_ready` and legacy behavior is untouched.
   - A test tool (extend `bridge-ble` with an `--rv2` mode or an equivalent
     script) can: subscribe status, read the nonce, send NOOP, push a 512 B
     and a 2048 B usage snapshot, re-send the same revision (expect idempotent
     ACK), inject CRC/offset errors (expect bounded NACK), and confirm the
     panel refresh only happens after COMMIT.

## Validation

- `pio run`; legacy BLE template push regression on-device (rv2 off).
- `cargo test --workspace` / `cargo test -p bridge-core --test template`
  (isolated target dir) for any Rust-side validation changes.
- Device test log records: negotiation, BEGIN/COMMIT results, radio-on ms,
  deinit, NACK reasons; `/status.json` exposes the v2 telemetry fields from
  design §11.
- MTU-23 boundary test (chunks of 11 B data) and malformed-JSON tests.

## Rollback

`pm/rv2=0` (default), legacy branch untouched; a v2 test session cannot
change the default wireless mode or owner state.
