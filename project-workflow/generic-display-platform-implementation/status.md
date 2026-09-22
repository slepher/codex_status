# Generic display platform v2 — status

Status: implemented in the working tree (no commit). M0–M6 code/host-test scope
complete; on-device verification is blocked by the device being offline.

## Implementation map

| Stage | Where |
|---|---|
| M0 baseline | `bridge/crates/core/tests/legacy.rs`, existing template/envelope/policy tests, Python/Node hashes |
| M1 bridge domain | `bridge/crates/core/{compile.rs,datasource.rs,coordinator.rs}`, `platform/{model,service,store}.rs` |
| M2 compiled template | firmware `template_engine.{h,cpp}`; host parity `bridge/crates/render/tests/compiled.rs` |
| M3 bundle/context/seq | firmware `bundle_store.{h,cpp}`, `v2_state.h` (`V2DataSeq`); host `tests/v2_state.rs` |
| M4 rendezvous/PowerPlan | firmware `v2_state.h` (`V2PlanState`), main.cpp endpoints + loop guard; bridge `v2_client.rs`, coordinator plan state |
| M5 refresh | `refresh_policy` `rgnBuildCt`, `epdFlush` (unchanged safety layer) + host policy tests |
| M6 targets/UI/MCP | `src/platform_target.h`, platformio gray4 env, `bridge/crates/app/src/platform.rs`, UI four pages, MCP tools |
| M7 persistence/migration | `platform/store.rs`, `PersistedState` (contexts/plans/next_seq), `migrate_legacy_profile`, `recovery_import` |

## Tests run

| Command | Result |
|---|---|
| `cargo test --workspace` (isolated `CARGO_TARGET_DIR=artifacts/cargo-target-v2`) | 81 passed / 0 failed |
| `cargo build -p bridge-app -p bridge-mcp` | ok |
| `pio run` (both envs) | ok; ROM sizes recorded |
| `node tools/test-quad-preview.mjs` | 7/7 |
| Python canonical hashes | `c1a2faaf` / `e6ba459e` / `430cc188` match Rust+firmware |
| `git diff --check` | clean |
| `cargo fmt --check` | pre-existing violations in untouched files; new/modified files formatted |

## Hardware verification — DONE on the 200×200 SSD1681 device (2026-09-22)

Device MAC `70:04:1D:D7:A3:40` / 192.168.3.163; final firmware 0.16.7-bw
(`artifacts/codex-status-0.16.7-bw.bin`, SHA256 `687A611B…250CB`).

| Check | Result |
|---|---|
| OTA 0.15.10 → 0.16.x (40 MHz) | ok (6 rounds, each fixing a hardware-found bug) |
| Legacy path after v2 firmware | `[v2] no committed bundle`, quad renders, rgn n=13 |
| Bundle BEGIN/CHUNK/COMMIT install | applied; `v2_bundle=true`, 3 templates, `commit_seq=2` |
| A/B second bundle + new context | applied; `commit_seq` increments, new context |
| Data push | `data_seq` monotonic → `displayed`; field CRC matches the bridge |
| Black-tile inverted digit partial | `partial/ok dirty=37`; repeated changes → `full/clean` |
| Zero refresh when idle | `refresh_kind=none`, `frame == last` |
| Formal PowerPlan | ids 1/4/6/7; countdown monotonic across reads and one push |
| Stale plan id | rejected `stale_plan` |
| BOOT provisional | `wake=ext1`, `prov_rem=276`; bridge kept window (`granted=267`) then extended to 600 |
| BOOT with bridge unreachable | radio closed at ≈t_boot+293 s, then deep (no formal plan) |
| Deep → timer wake | same `active_context_id`; queued push delivered on first rendezvous |
| A→B→A remote activate | three distinct contexts |
| OTA wrong target | HTTP 401 + `[ota] rejected: target … != …` |
| Aborted install + deep reboot | committed bundle/job intact |
| Bridge restart recovery | counters persist; seq 9 gap accepted; new plan issued |
| PM | 79 % light sleep, 2822 sleeps, no leaked OTA/USB locks |

Remaining: fixed-rig photo for ghost quantification (user deferred);
bridge BLE rendezvous transport (device still uses the legacy Wi-Fi deep pull —
the v2 protocol/HTTP/PowerPlan semantics are implemented, the BLE beacon
transport is not).

## Second target (ZecTrix Note4 / 4.2" 400×300 SSD2683) — software in progress

Confirmed facts (user): 4.2" B/W **400×300**, controller **SSD2683**; buttons =
side PGUP/PGDN, front ENTER (nets `KEY_PGUP`/`KEY_ESP32_EN`/`KEY_ENTER`).

Done and compile/host-verified (no OTA):
- target geometry: `platform_target.h` (`TARGET_WIDTH/HEIGHT/ROW_BYTES/FB_BYTES`),
  `template_engine.tplSetCanvas`, `refresh_policy.rgnSetPanel`; 200×200 host
  pixel/region parity tests stay green.
- driver alias layer `src/epd_target.h` (`EPD_TGT_*`); `main.cpp` converted.
- `src/EPD_SSD2683.{h,cpp}` skeleton (400×300/1bpp, windows, planes, BUSY
  propagation, partial-window entry points), gated on `CODEX_TARGET_NOTE4`.
- second ROM env prepared but not in the default build; all missing hardware
  facts are explicit `#error`s (no guessing).
- `artifacts/codex-status-0.16.8-bw.bin` (geometry refactor, not deployed).

Blocked on hardware facts (exact list):
1. ESP32-S3 EPD GPIO map (`EPD_SCK/MOSI/CS/DC/RST/BUSY/EPD3V3_EN`).
2. `KEY_PGUP/KEY_PGDN/KEY_ENTER` GPIO numbers.
3. SSD2683 timing (gates, data entry/orientation, border, temperature curve)
   and both waveform LUTs → `ssd2683_luts.h`.
4. Board flash/PSRAM part and capacities (partition/A-B frame budget audit).

Then: wire the NOTE4 pin map, enable the env, `TARGET_PARTIAL` only after
waveform/BUSY evidence, register the 400×300 render target on the bridge
(registry + canvas-aware validation/preview + template variant path), and run
the physical checklist. Everything else in v2 is implemented and verified on
the current 200×200 device (see the table above).

## Other

- `cargo fmt --check` on the whole workspace: pre-existing drift in untouched files.
