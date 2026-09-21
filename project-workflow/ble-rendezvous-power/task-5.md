# Task 5 — Stage 2: display safety layer (regions, ghost budget, local full refresh)

Owner: implementation (firmware only; no BLE protocol yet).

Inputs: design §8 (all subsections), task-4 capacity numbers,
`src/EPD_SSD1681.*`, `src/main.cpp` (`epdFlush`, `clk*`, `render*`).

## Deliverables

1. **Semantic regions derived from the active template (design §8.1)**
   - Parse `elements[]` once per template load into regions:
     `clock/text/usage_digit/solid_tile/inverted_text`; usage quadrants are
     located from the actual element rect/bounding box (bound `buckets[...]`),
     never hard-coded quad coordinates; fall back to whole-frame when the
     layout cannot be derived confidently.
   - Byte-align and merge intersecting regions (byte-expanded intersection);
     merged regions take the most conservative class. Region table is
     reported via `/status.json` (`regions` count, kinds, rects) for tests.

2. **Region statistics + ghost budget (design §8.1/§8.2)**
   - Per region: old/new black counts, area, changed, W2B, B2W,
     `d/b/s` ratios, background polarity, last full refresh, budget.
   - Decision order exactly per §8.2: trust first, then polarity/black-amount,
     then area and budget; keep the full-frame changed >12.5% fallback.
   - Initial conservative policy: high-ink regions (`max black ≥ 0.35` or
     solid/inverted class) with any change go **full refresh** (high-ink
     partial admission is stage 2b, gated on photos); low-ink text/clock may
     use window partials; clock keeps its own 90 budget and its local full
     rebuild.
   - `clean_requested` must bypass the identical-frame `memcmp` early return
     and force the full waveform (design §8.4); used by the clock budget and
     the post-failure recovery.

3. **Error propagation + trust marks (design §8.4)**
   - A BUSY timeout or failed waveform marks the baseline untrusted and sets
     a maintenance flag; the next required display is a full refresh. The
     trust flag is stored in RTC with the frame CRC and template hash;
     mismatch/anomalous reset/deep-cycle re-init invalidates it.
   - No half-rendered frame may update `lastDisplayedFrame`, the RTC clock
     pixels, or the displayed revision.

4. **Local full refresh from cached data (design §8.4)**
   - When the clock budget (90) or a local maintenance flag is due, rebuild
     the full frame from the cached usage + active template (no network), draw
     the current minute into it, and full-refresh. If no rebuildable snapshot
     exists, stop partial writes and flag the maintenance todo instead of
     flushing an uninitialized image.
   - `epdFlush` gains explicit kind/reason telemetry (`refresh_kind`,
     `refresh_reason`, dirty stats) via `/status.json`.

5. **Photo acceptance (user-gated)**
   - After the conservative path is deployed, the user takes fixed-rig photos
     of a full refresh vs. the previous image; until those pass, the high-ink
     partial path stays disabled. Frame captures (`/frame`) are the software
     oracle: candidate vs. shared-engine render must be pixel-identical, and
     software equivalence never replaces the photo test.

## Validation

- Host tests unchanged; `pio run`; on-device `/diag?render_mode=light|deep` +
  `/frame?which=frame|last` window diff shows the expected escalation for a
  crafted sequence (change one quadrant, then the clock, then a full page).
- A forced BUSY failure injection (`/diag?busy_fail=1`) marks untrusted and
  the next flush is a full refresh.
- Budget counters advance only on successful partials and reset on any full
  refresh; clock and usage budgets do not consume each other.

## Rollback

`/diag?region_policy=off` (token-gated, runtime) restores the previous
`epdFlush` decision path (changed-pixels/30-partial) without reflashing; the
conservative all-full-refresh mode is the safe default if anything regresses.

## Stage 2 results (2026-09-21, firmware 0.15.5-bw)

Implemented:

- `src/refresh_policy.{h,cpp}` (new): semantic regions derived from the
  template (`rect` fill -> solid, white-on-black/`bg:black` text -> inverted,
  `buckets[...]`/`resetCredits` -> usage, `device.now` -> clock, icons/bars/
  lines), byte-expanded merge with most-conservative class, per-region
  changed/W2B/B2W/black counts, ghost budgets (high-ink 4, low-ink 10, clock
  90), and the ordered decision (trust -> clean/force -> identical -> derive ->
  >12.5% area -> polarity -> high-ink guard -> budget -> partial).
- `main.cpp`: `epdFlush` now uses the policy (legacy rule behind
  `rgnPolicyOn`), `forceCleanRefresh` bypasses the identical-frame skip,
  budgets are accounted only after a successful waveform, and failure marks
  the baseline untrusted (stage 1 plumbing). Telemetry: `/status.json`
  `refresh_kind/reason/dirty_pixels/refresh_decisions/rgn/rgn_whole/rgn_policy`;
  `/diag?rgn=1` region dump, `/diag?policy=off|on`, `/diag?clean=1`,
  `/diag?busy_fail=1` injection.
- Host test coverage: `bridge/crates/render/tests/policy.rs` compiles the same
  C++ policy (build.rs now includes `refresh_policy.cpp`) and asserts
  none/clean/trust/force/low-ink-partial/high-ink-full/area-full/budget-reset.
- Cold-boot fix (user report: after an OTA reboot the panel stayed on
  `Connecting:` when the first bridge pull timed out with `http -11`):
  `startNormalMode` now calls `renderCurrent()` once the link is up on a cold
  boot, so the cached usage/template (or the status screen) replaces the
  connecting page even when the pull fails. An identical frame costs no write.

On-device verification (0.15.5-bw, token-gated diag):

- `[rgn] derived n=5 whole=0`; `/diag?rgn=1` lists the merged top high-ink
  region (0..24, 4..100), the bottom-right high-ink region, and three
  low-ink rows; a low-ink row shows `partials=1 cumS=13 budget=9`.
- `clean=1` on an identical frame -> full/clean (`epd_writes` +1).
- `busy_fail=1` -> `epd_trusted=false`, `epd_busy_fails=1`; next render is
  full/trust and restores `epd_trusted=true`.
- `policy=off` + `render_mode=deep` -> partial/legacy; `policy=on` +
  `render_mode=light` -> full/high_ink (same pixels, conservative escalation).
- `node tools/test-quad-preview.mjs` 7/7, `cargo test -p bridge-core --test
  template` 8/8, `cargo test -p bridge-render --test policy` 1/1 (all in an
  isolated `CARGO_TARGET_DIR`), `git diff --check` clean.

Still open (stage 2 exit criteria):

- Fixed-rig photo acceptance of the conservative full-refresh reference; high
  ink partials stay disabled until it passes (and demote to always-full on
  failure).
- Local full rebuild at the clock budget is wired through
  `forceCleanRefresh` + `renderCurrent()` from cached usage; a long-run
  observation of the 90-partial boundary is pending (needs the device to sit
  in deep for >1.5 h).
- ROM `artifacts/codex-status-0.15.5-bw.bin` (1 641 872 B, SHA256
  `078ABFC8180D21BF790CE110E97069EA270C5E3EAADF272BA55101FFD0EC6E9A`).

## Follow-up fix 0.15.6-bw (2026-09-21): pixel-overlap merge

User report: a single BOOT click (BLE icon toggle) flashed the whole panel.
Root cause: regions were merged on **byte-expanded** bounds, which glued the
status icons (x=101..156) into the top black tile (x=4..100) via shared byte
column 12, so any icon change hit the high-ink full-refresh gate. Regions now
merge on **real pixel overlap** only (`px0..px1/py0..py1`); byte columns remain
only as window-write metadata. Per-region statistics use the semantic pixel
rect with edge-byte masks, so neighboring pixels never leak into counts.

On-device (0.15.6-bw): `[rgn] derived n=13 whole=0`; the two black tiles,
three icon cells, the clock cell and the right/bottom text rows are separate
regions. `/diag?render_mode=deep` and `=light` (the same icon change a BOOT
click makes) both report `partial/ok` (`dirty` 143/101) instead of
`full/high_ink`. Host test extended with the icon-change-must-stay-partial
case. ROM `artifacts/codex-status-0.15.6-bw.bin` (1 642 192 B, SHA256
`21404F60965F8214603D2BAED599180AD81431902B2333600FA2D4A7508F2F79`).
