# Task 3 — Wi-Fi icon follows the link; immediate Zzz removal on BOOT

Owner: implementation (firmware + quad template; no bridge/protocol change).

Revision of task-2: the 0.15.1 flow kept Zzz until Wi-Fi connected. The user
revised it — on BOOT wake the sleep glyph is removed immediately, the Wi-Fi
icon appears only after the connection succeeds, and going back to deep
restores the sleep glyph and hides the Wi-Fi icon. The silent skip of the
`Connecting:` page stays.

Behavior:

- deep: Zzz shown; no Wi-Fi icon (template `device.state == "DEEP"`).
- BOOT/PWR wake: the first panel update removes Zzz before association; Wi-Fi
  is not up yet, so the Wi-Fi icon stays hidden (`state == "WIFI OFF"`).
- connect success: a second render shows the Wi-Fi icon (`state == "BLE OFF"`
  or `"BLE ON"`); this flush is a partial refresh over the wake baseline.
- idle -> deep: `enterDeep()` draws Zzz and hides the Wi-Fi icon.
- connect failure/timeout on battery: restore the sleep frame (full baseline,
  like `enterDeep`) and return deep on the existing retry cadence.

Changes:

- `tools/test-bridge/templates/quad.json`: v10 -> v11. The Wi-Fi icon at
  (121,6) becomes conditional on `device.state` = `BLE OFF` / `BLE ON`; the
  crossed-Wi-Fi icon (`WIFI OFF`) is removed (absence now means "not
  connected").
- `src/main.cpp`: `wokeFromDeep` (renamed from `keepSleepImage`) drives the
  silent connect, the immediate wake render, the post-connect icon render and
  the failure restore; new `renderSleepGlyph()` helper; FW_VERSION 0.15.2-bw.

Validation:

- `pio run`, `node tools/test-quad-preview.mjs`.
- Device: `/diag?render_mode=deep` + `GET /frame?which=last` => 0 black pixels
  in the Wi-Fi cell; `render_mode=light` => icon present; then a physical BOOT
  press in deep (Zzz removed first, Wi-Fi icon after connect).
