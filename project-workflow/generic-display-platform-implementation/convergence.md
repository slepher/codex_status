# v2 convergence — 2026-09-22

The implementation is not complete. Existing hardware evidence covers the HTTP
path, compiled C++ rendering, and refresh policy; it does not demonstrate BLE
rendezvous, consumption of the Rust compiled artifact, or dirty-window writes.

## Ordered work and acceptance

1. Protocol correctness: validate data CRC and sequence through the shared runtime;
   preserve PowerPlan deadlines on replay; bind bundle transfers to owner, session,
   declared length and CRC; make activation retries context-aware and idempotent.
   Validate with host runtime tests, HTTP client tests, and a firmware build.
2. BLE rendezvous: authenticated status/data/plan over the existing paired GATT
   channel, fragmented transport and correlated ACK, bounded radio windows.
   HTTP delivery must never be reported as BLE delivery. Requires paired-device
   integration evidence before marking complete.
3. One compiled format: shared C++ compiler emits the wire artifact; Bridge stores
   and forwards it. Verify requirement index order and host/device byte parity.
4. Self-contained bundle: retain sources, resources, bindings, target and compiled
   bytes in A/B slots; activation records must not rewrite a committed slot.
   Test interrupted writes, corrupt records, ABI recovery and low-space rejection.
5. Display: derive byte-aligned padded dirty windows from the actual frame diff,
   submit old/new window pixels; retain BUSY failure/full-refresh recovery.
   Host geometry tests plus SSD1681 hardware validation and ghost photographs.
6. UI/target cleanup: four top-level pages, power under device. Note4 driver remains
   blocked on the GPIO, waveform and memory facts listed in PROGRESS.md.

No automatic publish, OTA, deployment, commit, or restart of running services.
Do not mark any stage complete from pure state tests alone.
