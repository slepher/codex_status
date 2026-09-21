# Task 10 — boot UX: cached-template cold start + Wi-Fi icon blink

Status: **implemented and on-device verified** (firmware 0.15.9-bw, quad v12,
2026-09-21). Requirements captured 2026-09-21 (user). Implementation notes,
measurements, and remaining user/physical checks are in the last section.

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

## Implementation (2026-09-21, firmware 0.15.8/0.15.9-bw + quad v12)

### Requirement A — cached cold boot enters the template at once

- `startNormalMode`: with `!wokeFromDeep && lastUsage.length() > 0` the cached
  template is rendered **before** `connectBest`; the connect page is suppressed
  (`connectBest(!wokeFromDeep && !cachedColdBoot)`). Link down means the
  template draws `WIFI OFF`; no `Connecting:` page.
- After connect, the existing link-up `renderCurrent()` is the single partial
  that lights the Wi-Fi icon (the duplicate cold-boot render in 0.15.5-0.15.7
  was removed).
- Failed connect: the template stays with `WIFI OFF`; the existing bounded
  retry/backoff and deep return are unchanged (no sleep-glyph repaint on this
  path, per the requirement).
- No cache: unchanged fallback (boot status/connecting screen stays).

### Requirement B — Wi-Fi icon blinks (~1 Hz) while connecting

- Firmware adds a template-visible state pair: while a boot connect attempt is
  running, `device.state` alternates `WIFI CONN` (blink on) / `WIFI OFF`
  (blink off) every `WIFI_BLINK_MS` (default 1000 ms). `blinkAllowed` requires a
  rendered template plus a trusted, partial-ready baseline, so the
  Connecting/status page is never touched and an untrusted baseline can never
  turn a tick into a full flash.
- `connectBest` drives the blink inside its wait loop; on failure it re-renders
  once to settle on `WIFI OFF` (icon hidden). On success the link-up render
  shows the steady icon. Deep timer retries use `deepFastConnect` and never
  blink, so the blink is bounded to one cold boot / BOOT wake.
- `epdFlush` treats blink ticks specially: only an exhausted ghost budget
  (`RFNR_BUDGET`) is downgraded to a partial; the tick is logged as
  `refresh_kind=partial`, `refresh_reason=blink`, does not consume the region
  ghost budget and does not count against the 30-partial legacy streak.
- quad v12 adds the `WIFI CONN` icons: x=121 Wi-Fi icon (same bits as
  BLE ON/OFF) and x=141 crossed-link overlay (same bits as `WIFI OFF`), so the
  crossed overlay stays steady and only the Wi-Fi cell changes per tick.
  Rust canonical hash `430cc188`, version 12 (`bridge/crates/core/tests/template.rs`,
  `node tools/test-quad-preview.mjs`).

### Double-frame fix

- `wakeBaselineDrawn` is set in `deepNetworkCycle` when `leavingDeep` renders
  the clean light frame; `startNormalMode` skips its own clean wake render then.
  Verified on the timer pull -> light path: exactly one full (Zzz removal) plus
  one partial (Wi-Fi icon), instead of two full flashes.

### Measurement (device 192.168.3.163, battery, light)

- `/diag?blink_test=N` (token-gated) runs N blink ticks on the live template
  with the real policy and reports the per-tick cost; `/status.json` adds
  `refresh_ms`, `blink_ms`, `blink_on`, `blink_ticks`, `wifi_conn`;
  `/diag?blink_ms=N` tunes the phase (0 disables) at runtime.
- Measured 0.15.9 + quad v12: n=30 -> `avg_ms=870`, `worst_ms=875`,
  `refresh_avg_ms=829` (panel wake/reset + partial waveform). Per-tick cost is
  stable and below the 1000 ms phase; all ticks were partials
  (`blink_ticks` +30, `epd_busy_fails=0`, `epd_trusted=true`, region budgets
  unchanged: x=121 budget 9 from the real link-up partial only).
- Decision: keep the 1 Hz default. Worst case is one failed cold boot =
  <= 30 ticks ~= 0.4 mAh at the design's ~0.013 mAh per clock-window event
  (design §9 model; no current meter was available, so this is panel-activity
  accounting, not a coulomb count). `blink_ms` remains available for field
  tuning; a plugged-only variant was not needed because the blink never runs on
  deep retries.
- On-device boot evidence (0.15.9 + v12, cold boot after OTA): log shows
  `[tpl] rendered quad (-)` **before** `[wifi] slot...` (Requirement A, no
  connecting page), one blink render mid-association (`blink_ticks=1`), then
  `[wifi] connected` and the link-up partial. `[rgn] derived n=13 whole=0`
  (v12's two extra elements stay merged into the icon cells; `RGN_MAX=32`
  still covers 31 pre-merge elements).

### Remaining checks

- Physical BOOT-press verification with the new template (blink phases, 1 Hz
  cadence, steady icon on connect) and a no-AP failure run to confirm the icon
  is hidden and the retry/deep behavior is unchanged.
- Photo/visual sign-off of the blink frames is not covered by the pixel tests.
- `blink_ms` default stays 1000 ms in the shipped ROM (`artifacts/codex-status-0.15.9-bw.bin`).
