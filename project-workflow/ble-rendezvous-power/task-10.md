# Task 10 — boot UX: cached-template cold start + Wi-Fi icon blink (documentation only)

Status: requirements captured 2026-09-21 (user). **Documentation only — no
implementation in this window.** All code work is deferred to a new window.

Inputs: user requirements (2026-09-21), design §3 (wake/bounded timeouts),
§4 (rendezvous visual state), §9 (power budget), task-2/task-3 (silent wake),
current cold-boot path in `src/main.cpp` (`setup` / `startNormalMode` /
`connectBest`).

## Requirement A — cold boot with cached data enters the template immediately

- After the boot sequence completes, if a usage cache exists (NVS `ucache`,
  loaded into `lastUsage` in `setup()`), the device must render the active
  template immediately instead of waiting for Wi-Fi association. The
  full-screen `Connecting: <SSID>` page must not be shown in that case.
- While the link is down: `device.state = "WIFI OFF"` (Wi-Fi icon hidden),
  and the bridge/offline condition conveys the disconnected state per the
  template (quad v11 already has the offline/`WIFI OFF` icon variants).
- After the connection succeeds: one update (partial over the wake baseline)
  lights the Wi-Fi icon and applies the fresh pull/push as today.
- If the connection fails/times out: the template stays visible with
  `WIFI OFF`; the existing bounded retry/backoff and deterministic deep return
  are unchanged. No `Connecting:` page on this path either.
- If there is **no** cache: the current fallback (status/connecting screen)
  stays; implementation decides the exact fallback (status screen preferred).
- Relationship to existing work: 0.15.5 already renders cached content once
  the link is up. This requirement moves that render **before** association
  and drops the connecting page for the cached case. Silent wake
  (task-2/task-3) already does this for BOOT/PWR deep wakes; this extends the
  same rule to cold boot.
- Acceptance:
  - Cold boot with cache -> template visible within one refresh after boot,
    no `Connecting:` page; state `WIFI OFF` until connect.
  - Connect success -> exactly one partial update showing the Wi-Fi icon.
  - Connect failure -> template remains with `WIFI OFF`; retry cadence and
    bounded deep return unchanged.
  - Cold boot without cache -> documented fallback, still no stuck page.

## Requirement B — Wi-Fi icon blinks (~1 Hz) while connecting

- During a connection attempt (cold boot and BOOT wake), the Wi-Fi icon
  alternates visible/hidden at roughly 1 Hz until either:
  - the connection succeeds -> icon steady on; or
  - the attempt fails/times out (`WIFI_CONNECT_MS`), the link-loss policy
    fires, config mode starts, or the device enters deep -> icon hidden.
- The blink must stop immediately at any of those exits and must not extend
  the connect timeout or the wake/radio budget (design §3/§13 stay binding).
- Implementation notes for the new window (no code written here):
  - Template-driven is preferred: a new `device.state` value (e.g.
    `WIFI CONN`) or a dedicated bind, with quad v12 conditions. `when`
    currently supports only a single condition, so two icons in the same cell
    (or a new state) is the likely route.
  - Any template-protocol change requires the three-end sync (firmware engine
    / Rust canonical JSON / Python test bridge) plus the shared hash tests
    (`node tools/test-quad-preview.mjs`,
    `cargo test -p bridge-core --test template`).
  - Blink frames must be partial/window-level; no full refresh per tick.
  - **Power is the open question**: each partial waveform costs ~0.3-0.8 s of
    panel activity; a 10-30 s connect at 1 Hz is 10-30 partials on a
    battery-powered device. Measure the per-tick cost and decide the period
    (1 s vs 2 s) and/or a `plugged`-only variant before shipping; do not ship
    an unmeasured blink. This is an explicit design §9 item.
- Acceptance:
  - Toggle at 1 ±0.2 Hz during association; steady on at connect; hidden at
    failure/deep.
  - `refresh_kind=partial` for blink frames; no full refresh while blinking.
  - Connect timeout, retry cadence, and bounded deep return unchanged.

## Known issue to fix together with Requirement B (2026-09-21, 0.15.7 verification)

- The 0.15.7 deep-wake path can draw the wake frame **twice** on the timer
  pull -> light transition: `deepNetworkCycle` draws the clean light frame
  (leavingDeep), then `startNormalMode` draws the same frame again with
  `forceCleanRefresh`, producing two full flashes. A RAM flag
  (`wakeBaselineDrawn`) fix was prototyped during verification but reverted
  as unrequested. Apply it when this path is next touched (Requirement A/B
  work), then verify exactly one full refresh per wake plus one partial for
  the Wi-Fi icon.
- Verified working from the same 0.15.7 change: the Zzz is now erased by a
  clean full refresh when leaving deep through the pull path
  (`deepNetworkCycle` sets light mode before rendering and requests
  `forceCleanRefresh`), fixing the "Zzz ghost looks like still asleep" report.
