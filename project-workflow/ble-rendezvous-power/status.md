# BLE rendezvous power design — status

Status: design complete; silent-wake implemented (task-3, 0.15.2-bw);
implementation plan task-4..task-9 written (stages 1-6); stage 1 in progress

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
  SHA256 `D20BBFEF10D40C47E64BAFFA5FFF70F81BE90575D29DAD66BD851C78C2CF0FCA`,
  re-verified 2026-09-21; the earlier entry here and in PROGRESS.md was one
  character short),
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
- 2026-09-21: implementation plan written for design §12 stages 1-6:
  - task-4 stage 1: baseline evidence, RTC/heap/linker audit, `readBusy()`
    propagation, GATT `rv:2` routing + `rendezvous_v` capability + `pm/rv2`
    rollback switch.
  - task-5 stage 2: semantic regions, ghost budgets, error propagation, old
    frame trust marks, same-frame clean, local full refresh; photo gate before
    high-ink partials.
  - task-6 stage 3: v2 transactions on the existing GATT table (NOOP,
    BEGIN/CHUNK/COMMIT, CRC/idempotency/ACK, encryption/owner checks,
    bounded radio), explicit test sessions only.
  - task-7 stage 4: clock-first rendezvous, radio hard cutoff, BOOT 300 s
    lease, authenticated `POST /power`, deinit/OTA wind-down.
  - task-8 stage 5: bridge resident listener, persistent revision domain,
    WAKE/RENEW/SLEEP scheduling, owner integration, queue end-to-end.
  - task-9 stage 6: staged enablement, measured power/Windows gates, photo
    acceptance, dual-phase experiment last.
- 2026-09-21: **stage 1 complete** (task-4): baseline evidence archived
  (`artifacts/ble-rendezvous/`, 0.15.2 hash corrected), RTC audit (7 680 B
  region, 6 000 B headroom, 5 000 B frame fits but not chosen), EPD BUSY
  return propagation with `epd_busy_fails`/`epd_trusted`, `rv:2` routing
  stub + `rendezvous_v`/`rv_max` capability + `pm/rv2` rollback switch.
  Firmware `0.15.3-bw` built, OTA'd, live (`rv2=0`, `epd_trusted=true`);
  ROM `artifacts/codex-status-0.15.3-bw.bin` SHA256
  `DC6227B3EC4BA01F4A0FDD55D9AE79B166743CEE2BBF17A04EC1F9AD8B1437B2`;
  preview/template tests green. Deferred: BLE-side regression check needs a
  BOOT click; baseline photos are user-supplied. Stage 2 started.
- 2026-09-21: **stage 2 core implemented and deployed** (task-5):
  `refresh_policy` semantic regions + conservative escalation (high-ink full
  until photos), ghost budgets, clean/full-regeneration paths, `/diag`
  controls and telemetry. Firmware `0.15.5-bw` (ROM
  `artifacts/codex-status-0.15.5-bw.bin`, SHA256
  `078ABFC8180D21BF790CE110E97069EA270C5E3EAADF272BA55101FFD0EC6E9A`).
  On-device checks pass (region dump, clean=full, busy_fail/trust, legacy
  policy off, high-ink escalation). Host policy test
  `bridge/crates/render/tests/policy.rs` added and green. Fixed the reported
  post-OTA stale `Connecting:` page: cold boot now renders cached
  usage/status once the link is up even when the first pull times out.
  Remaining: fixed-rig photo acceptance (user) and the 90-partial local
  rebuild soak.
- 2026-09-21: **0.15.6-bw follow-up** (user report: BOOT click flashed the
  whole panel). Region merging now uses real pixel overlap instead of
  byte-expanded bounds, so the black tiles stay separate from the icons and
  clock; per-region stats mask edge bytes. Device: `[rgn] derived n=13
  whole=0`, `render_mode=deep/light` -> `partial/ok`. Host test extended.
  ROM `artifacts/codex-status-0.15.6-bw.bin` SHA256
  `21404F60965F8214603D2BAED599180AD81431902B2333600FA2D4A7508F2F79`.
- 2026-09-21: **0.15.7-bw deployed**: leaving deep through the timer pull now
  sets light mode before rendering and requests `forceCleanRefresh`, so the
  wake frame is one clean full refresh and the Zzz ghost ("looks like still
  asleep") is gone. Device log confirms the pull-path sequence. Known issue
  recorded in task-10: the same wake can draw the frame twice (two full
  flashes) on the timer path; a RAM-flag fix was prototyped and reverted as
  unrequested — apply with the task-10 work.
- 2026-09-21: **task-10 written (documentation only, per user)**: (A) cold
  boot with a cached snapshot renders the template immediately with
  `WIFI OFF` and no `Connecting:` page; (B) Wi-Fi icon blinks ~1 Hz while
  connecting until success (steady) or failure (hidden), with the power-cost
  measurement required before shipping. No code changes made.
- 2026-09-21: OTA field incident recorded in task-8: a queued OTA retry loop
  aborted every upload client-side until `bridge-app` was restarted (device
  and device token were fine; curl uploaded the ROM in 26 s). Stage 5
  actions: pending-OTA visibility, preflight settle, shorter bounded
  upload timeout, push/OTA serialization.
- 2026-09-21: **task-10 implemented and verified** (firmware 0.15.9-bw,
  quad v12, hash `430cc188`; ROM `artifacts/codex-status-0.15.9-bw.bin`,
  1 644 256 B, SHA256
  `47C641DF254449751DA6678F4503AA8276B527FCC3688DFABDA076024F9B8549`):
  - A: cached cold boot renders the template before association
    (`WIFI OFF`, no `Connecting:` page); one link-up partial lights the icon;
    failed connect keeps the template with unchanged retry/deep.
  - B: `device.state` blinks `WIFI CONN`/`WIFI OFF` at 1 Hz in `connectBest`;
    blink ticks stay `partial`/`blink`, bypass the icon ghost budget and the
    30-partial streak, and are bounded to a cold boot / BOOT wake (deep
    retries use `deepFastConnect`, no blink). quad v12 adds the `WIFI CONN`
    icons so only the Wi-Fi cell toggles.
  - Double-frame fix: `wakeBaselineDrawn` stops the timer pull -> light path
    from drawing the wake frame twice (verified in the boot log: one full +
    one partial).
  - Measured via `/diag?blink_test=30`: avg 870 ms/tick, worst 875 ms,
    waveform avg 829 ms; 30/30 partials, budgets untouched. Decision: keep
    1 Hz (worst case <=30 ticks per failed boot ~= 0.4 mAh by the design
    §9 model); `/diag?blink_ms=N` tunes or disables the phase at runtime.
  - Intermediate 0.15.8-bw (ROM
    `artifacts/codex-status-0.15.8-bw.bin`, SHA256
    `FFECE66BF1062926CD18BAD27127F1FFA74CCF129CC3F7572237FBA8B0CD5006`)
    was flashed for the first measurement round, then superseded by the
    `blinkAllowed` baseline guard in 0.15.9.
  - Still user/physical: BOOT-press blink check, no-AP failure run, and the
    photo gate for stage 2 (unrelated to this task).
  - Tests: `pio run`; `node tools/test-quad-preview.mjs` (blink assertions
    added); `cargo test -p bridge-core --test template` (quad v12 hash);
    `cargo test -p bridge-render --test policy`;
    `cargo test --workspace` (isolated target dir); `git diff --check` clean.
- 2026-09-21: **"never sleeps in light" fixed** (firmware 0.15.10-bw + bridge):
  - Cause: the bridge renews the owner lease every 60 s and the device counted
    a renew as activity (`noteActivity("claim")`), so the 600 s local idle
    never fired; the 5-minute heartbeat push did the same. On the bridge side
    the push `mode_str()` used `expects_deep()` (quiet>=600 s && silent>=120 s)
    but the 10 s `/status.json` poll and the renewals counted as contact, so
    every push carried `mode:"light"` — no light->deep path existed.
  - Firmware: renew no longer resets the idle timer (new claim only); the
    `/usage` push resets it only when `usage_rev` changed (missing rev =
    conservative change). Design §6 ("claim/续 owner 不能续 light").
  - Bridge: `activity.rs::light_wanted()` is shared by the pull response and
    the push `mode_str()`; the push loop pushes immediately on a mode change
    (`mode_changed`), so the bridge commands deep after light dwell
    (300 s) + quiet (600 s) without waiting for the 5-minute heartbeat.
  - Verified: with renews and heartbeats flowing, `idle_s` climbed to 600 and
    the device slept on its own; with `/diag?idle_deep_s=3600` (rule out the
    local fallback) the bridge's mode flip still put it to deep ~60 s later.
    Bridge restarted watchdog-first (new PID 6392 + watchdog 42604).
  - ROM `artifacts/codex-status-0.15.10-bw.bin` (1 644 320 B, SHA256
    `79B0412320C08CDB297FB3141FB0B3B7831FEF1BD240B13BDD553E281EDCFCD0`);
    `cargo test --workspace` green, new `push_mode_follows_quiet_not_contact`
    unit test.
