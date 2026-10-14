# Local patch: btleplug 0.11.8

Source: crates.io btleplug 0.11.8, as pinned in the original Cargo.lock.
Original crate checksum: `c9a11621cb2c8c024e444734292482b1ad86fb50ded066cf46252e46643c8748`.
Upstream: https://github.com/deviceplug/btleplug
Upstream source and licenses are retained; registry metadata, CI metadata and
the dependency's own lockfile are omitted. Trailing whitespace in the upstream
license/README and Android wrapper metadata is normalized for repository checks. The workspace patch applies on all
platforms, but source changes affect only the CoreBluetooth backend.

`src/corebluetooth/internal.rs` routes two handlers through the shared
`src/connection_lifecycle.rs` (exported by `src/lib.rs`):

- `disconnect_peripheral`: a UUID absent from the native thread's peripheral
  map completes with `Ok`. A prior remote disconnect removes that entry; no
  second native cancellation/callback is expected.
- `is_connected`: a missing UUID completes with `Disconnected`.

`corebluetooth/mod.rs` imports the same original reply-future implementation
through that shared module. Fake ROM's Bridge peripheral proxy calls these exact
handlers too; it does not maintain a separate copy of the fix.

Existing peripherals still use the original native cancellation and callback.
No timeout is translated into success. Bridge retains quarantine for genuinely
unknown native operations and unfinished writes.

Regression: `python3 tools/test-corebluetooth-close.py --out <existing-dir>`
extracts the actual handlers and imports the shared lifecycle/reply-future code and replays the remote
removal race with stub native objects. Before this patch two tests remain
Pending; with it all four pass. Full Rust tests/build check the real backend.

Evidence and deployment: `project-workflow/bridge-macos/ble-close-fix-20261014.md`
at the repository root. Retire this local patch only after an upstream version
passes the same regression and real BLE remote-close/reconnect checks.

Fake ROM regression: from `bridge/`, run
`cargo test -p device-sim --test bootstrap fake_ble_device_remote_close_reconnect_and_unknown_quarantine`.
It enables `CODEX_STATUS_SIM_BLE_LIFECYCLE=1`, uses an independently controlled
BLE device and StaticJson source, and exercises the production NativeLease
policy on an isolated simulated radio. Removing the missing-peripheral reply
reproduces quarantine on the second connection. See
`project-workflow/bridge-macos/fake-ble-lifecycle-20261014.md` for boundaries.

2026-10-14 pending-connect repair: terminal disconnect completes the connect waiting for discovery; absent-peripheral connect rejects explicitly; connection-failure callback tolerates a previously completed request. Shared `connection_lifecycle::complete/connect` are used by the native thread and Fake replay. Unknown native operations remain quarantined.
