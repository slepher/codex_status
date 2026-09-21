# Task 2 — silent wake: keep the sleep image until Wi-Fi connects

Superseded by `task-3.md` for the wake sequence (Zzz is now removed
immediately on BOOT; the Wi-Fi icon follows the link). The silent skip of the
`Connecting:` page remains.

Owner: implementation (firmware only; no bridge, protocol, or GATT change).

Inputs: plan.md "Additional requirement (2026-09-21): silent BOOT wake",
`docs/ble-rendezvous-power-design.md` §3 (diagram, bounded-timeout table) and
§13, `src/main.cpp` wake/connect/render paths.

Behavior:

- Any wake from deep sleep (BOOT/PWR EXT1, or the battery retry-timer wake
  whose panel still shows the sleep image) does not draw the full-screen
  `Connecting: <SSID>` page. The panel keeps the sleep image (Zzz) while Wi-Fi
  associates.
- After `connectBest()` succeeds, render the active template once when the last
  deep entry actually drew the sleep glyph (`rtcDeepGlyph & 1`); the first
  flush after wake is a full refresh, so the Zzz is replaced with a clean
  baseline.
- A failed/timed-out connect leaves the sleep image unchanged and follows the
  existing bounded retry/return-to-deep path.
- Cold boot and explicit setup/diagnostic paths keep the connecting page.

Changes:

- `src/main.cpp`: add `keepSleepImage` (`= woke`); `connectBest(bool
  showProgress)`; render-on-connect in `startNormalMode()`; `FW_VERSION` bump.

Validation:

- `pio run` builds.
- OTA to the device; in deep sleep press BOOT: the panel stays on Zzz until the
  bridge contact clears it, with no `Connecting:` page; `/history` records
  `to-light`, `/status.json` otherwise unchanged.
- Failed-connect case (bridge/Wi-Fi unavailable): Zzz stays and the device
  returns to deep after `WIFI_CONNECT_MS`.
