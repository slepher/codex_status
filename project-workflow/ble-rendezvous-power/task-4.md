# Task 4 — Stage 1: baseline evidence, capacity audit, driver/rollback groundwork

Owner: implementation (firmware + artifacts; no protocol behavior change).

Inputs: design §12.1, §2 (facts table), `src/EPD_SSD1681.cpp`, `src/main.cpp`
(deep/light paths), `src/ble_bridge.cpp`.

## Deliverables

1. **Baseline evidence (before any behavior change)**
   - Archive the running ROM hash (`0.15.2-bw` bin + SHA256) in `artifacts/`.
   - Capture `GET /status.json`, `GET /history`, `GET /frame?which=last`
     (PBM) from the live device into `artifacts/ble-rendezvous/` with a
     timestamped note; record the minute-cycle and black-block reference
     numbers already in `PROGRESS.md` as the power baseline. Photos stay a
     user-supplied item (no camera automation in this repo).
   - No firmware is flashed for this item; the live device keeps running
     `0.15.2-bw` while the evidence is collected.

2. **RTC/heap/linker capacity audit (design §7 RTC table)**
   - Build the release env and parse `firmware.map` for `.rtc.data`/
     `RTC_DATA_ATTR` symbols; report used bytes, align/slack, the biggest
     consumers (history ring 1440 B, clock window 64+ B) and whether a
     5000 B full old-frame can ever fit; if not, record the fallback
     (clock window only + mandatory full refresh for regions without old
     pixels).
   - Re-check PSRAM availability and heap after `epdBegin()` (5000 B frame +
     5000 B last frame) so stage 2 can size region buffers conservatively.
   - Write the numbers into this task file plus `status.md`; they are inputs
     to the §14 open question.

3. **Driver BUSY propagation (design §8.4)**
   - `readBusy()` returns `bool`; timeouts are not silently swallowed.
   - Public driver calls that run a waveform return `bool` (`Display`,
     `DisplayPart`, `Clear`, window variants); `main.cpp` counts failures
     (`epdBusyFails`, last failure context) and marks the display baseline
     untrusted on failure (the actual recovery policy lands in stage 2).
   - No behavior change on the happy path; `/status.json` gains
     `epd_busy_fails` / `epd_last_ok`.

4. **GATT compatibility routing + rollback switch (design §5.1, §10)**
   - `ble_bridge` gains a control-dispatch hook: a template-control JSON with
     `"rv":2` is routed to a v2 handler stub (NACK `not_ready` for now, never
     treated as an old template op); JSON without `rv` keeps the exact legacy
     path. `bleSetV2Handler()` keeps the legacy handler untouched.
   - Capability advertisement: INFO gains `"rendezvous_v":0` (v2 not
     negotiated) plus `"rv_max":2`; no new control is ever sent unless the
     device advertises negotiation.
   - Rollback switch: compile-time default off (`CODEX_BLE_RV2=0`) plus a
     token-gated runtime gate persisted in NVS `pm/rv2` (`POST /diag?rv2=1|0`;
     default 0). With the gate off, a future v2 path can never run. BOOT-based
     legacy recovery is a stage 6 item; the switch is the stage 1 guarantee.
   - GATT table stays unchanged. Windows attribute-cache cost is explicitly
     recorded here: no new UUIDs, no Service Changed; re-pairing is not
     required and must not be introduced silently.

## Validation

- `pio run` builds; ROM size delta recorded.
- `node tools/test-quad-preview.mjs` and `cargo test -p bridge-core --test
  template` (isolated `CARGO_TARGET_DIR`) unchanged and green.
- On-device (OTA or serial as available): `/status.json` shows the new
  counters; legacy BLE template push still works (`bridge-ble --once` or the
  existing test path) and `rv:2` control gets a NACK instead of corrupting the
  transfer.
- `git diff --check` clean.

## Rollback

Set `pm/rv2=0` (default) or reflash `artifacts/codex-status-0.15.2-bw.bin`;
no GATT/attribute change means no Windows cache cleanup is needed.

## Stage 1 results (2026-09-21)

Baseline evidence (`artifacts/ble-rendezvous/`, device 192.168.3.163,
live `0.15.2-bw`):

- `baseline-status-20260921-132321.json` (light, BLE OFF, `epd_streak=8`,
  heap 147024, `clk_partials=5`, `usage_rev=22`, owner lease 300).
- `baseline-history-20260921-132321.json` (12 records, boot/thin/net-ok).
- `baseline-last-20260921-132321.pbm`: 17 399 black px; TL hero block 7 538,
  BR hero block 7 463, clock cell 65, Wi-Fi cell 62, Zzz cell 0.
- `baseline-log-20260921-132321.txt`.
- ROM re-hash: `artifacts/codex-status-0.15.2-bw.bin` is
  `D20BBFEF10D40C47E64BAFFA5FFF70F81BE90575D29DAD66BD851C78C2CF0FCA`
  (the hash previously recorded in PROGRESS.md/status.md was one character
  short; both files corrected).
- Baseline minute/black-block power numbers remain those in PROGRESS.md
  (clock ~0.012–0.013 mAh, network ~0.03–0.04 mAh per event; photos still a
  user-supplied item).

Capacity audit (`tools/rtc-budget.py`, new):

- RTC slow region 7 680 B at 0x50000200; `.rtc.data`/`.rtc.bss` used
  1 668 B after stage 1; headroom **6 000 B**. A 5 000 B full old frame fits
  with ~1 000 B left for region metadata, but the history ring (1 440 B) is
  the largest consumer if more space is ever needed. Frame buffers (2 ×
  5 000 B) are heap/PSRAM, not RTC; device free heap 147 KB.
- Decision for stage 2: keep the RTC clock window + add a compact
  region/ghost table; do **not** move the full frame into RTC yet (fits but
  leaves no headroom for stage 4 rendezvous state). Regions without an RTC
  old-pixel copy escalate to full refresh.

Driver BUSY propagation: all `EPD_SSD1681_*` waveform calls now return `bool`;
`main.cpp` counts `rtcEpdBusyFails`, marks `epdBaselineTrusted=false` on
timeout, never updates `lastDisplayedFrame` after an unknown waveform, and
escalates the next flush to a full refresh. New `/status.json` fields:
`epd_busy_fails`, `epd_trusted` (plus the HTML status page). Clock-window
failures set `rtcClkPartials=CLK_GHOST_LIMIT` so the next network window
rebuilds the full frame.

GATT routing + rollback switch: template-control JSON with `"rv":2` is routed
to a stub that NACKs `not_ready` (legacy path untouched); INFO carries
`rendezvous_v` (0/2) and `rv_max:2` (visible in `/status.json` too); the
token-gated `POST /diag?rv2=1|0` persists the gate in NVS `pm/rv2`.

Deployment/validation:

- `pio run` clean (Flash 1 599 448 B, RAM 20.5 %; only the pre-existing
  NimBLE `service->start()` deprecation warning).
- `node tools/test-quad-preview.mjs` 7/7; `cargo test -p bridge-core --test
  template` 8/8 (isolated `CARGO_TARGET_DIR`).
- OTA `0.15.2-bw -> 0.15.3-bw` via MCP `firmware_ota`; live check:
  `fw=0.15.3-bw`, `rv2=0`, `rv_max=2`, `epd_busy_fails=0`,
  `epd_trusted=true`, light mode, battery 72 %.
- `/diag?rv2=1` then `?rv2=0` round-trip verified over HTTP with the device
  token (never printed); `/status.json` followed.
- ROM: `artifacts/codex-status-0.15.3-bw.bin` (1 632 336 B, SHA256
  `DC6227B3EC4BA01F4A0FDD55D9AE79B166743CEE2BBF17A04EC1F9AD8B1437B2`).

Open items carried to stage 2:

- On-device BLE checks (legacy template push still working; `rv:2` NACK) need
  a BOOT click for the BLE session; deferred to the next user interaction.
- The stage-1 baseline photo set (fixed rig) is still user-supplied.
