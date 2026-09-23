# Generic display platform v2 — status

## Font assets as engine data — 2026-09-23 (task 7)

Full write-up, evidence and remaining work: `task-7-font-assets.md`. Font
container contract: `docs/font-asset-format.md`.

User decision recorded: **anti-aliasing / 2bpp-gray4 is dropped** ("停止抗锯齿的支持");
the workstream is now "make fonts data so weight/size can be tuned without a
reflash", with normal text = Noto Sans Thin @18 (provisional) and large text =
Noto Sans Regular @64 (provisional).

Implemented and host-verified (no flash, no publish, no commit):

- **One font registry** in `src/template_engine.cpp` (`TPL_FONTS`, 9 entries,
  append-only because `CtOp.font` is persisted). `main.cpp`'s clock fast path no
  longer keeps its own table; `refresh_policy` already used the shared cell
  helpers. `bridge/crates/core/src/template.rs::FONTS` mirrors it.
- **New faces** from `tools/note4-fonts/rasterize_ttf.py` (FreeType *monochrome*,
  `FT_LOAD_TARGET_MONO`, static hinted Noto TTFs): `ntthin18` (Thin 100 @18,
  ASCII, blob 1412 B) and `ntreg64` (Regular 400 @64, tabular digits, blob
  2457 B). Both confirmed tabular (0–9 share one advance), so digits do not
  jitter. The TTFs are vendored under `tools/note4-fonts/vendor/` (gitignored).
- **CSFN v1 container** emitted by the same tool as the header (`bridge/assets/fonts/`,
  `font_ntthin18.bin` id `18c2e4ed`, `font_ntreg64.bin` id `4dc3b226`); device
  parser `src/font_asset.{h,cpp}`; profile-scoped store `src/font_store.{h,cpp}`
  (atomic tmp→verify→rename, CRC re-check, name==id, caps, prune);
  bridge library `bridge/crates/core/src/platform/fonts.rs` (content-addressed,
  dedup, per-Profile plan, inventory diff, caps).
- **Fixed a genuinely broken tree**: the host FFI could not link
  `rgnSetPanel` (anonymous namespace), `compiled.rs` called the old 1-arg
  `rgn_build_compiled`, `rgn_build` derived 400×300 regions with 200×200
  geometry, `RGN_MAX` 32 < the template's 38 elements, `Rgn::area` overflowed
  `uint16_t` on a 120 000-pixel panel, `build.rs` did not track font headers, and
  the host LittleFS shim had no directories.

Verification: `cargo test -p bridge-core -p bridge-render -p bridge-mcp`
(isolated target dir) **110 passed / 0 failed**, including 3 device-parser +
5 device-store host tests and 18 bridge-library tests; `bridge-render
--compare-compiled --regions` on the regenerated 400×300 template reports
`json vs compiled diff pixels: 0`, round-trip `0`, and 18 regions on both paths.
A test-found bug: the store's inventory listed a mis-named file under its content
id; it now requires name == id.

Not done (next): template→asset resolution (`CtOp.fontRef` + `CT_ABI` bump),
`font_inventory` + BEGIN/CHUNK/COMMIT/ACK transport, bridge publish preview /
MCP / UI, and the final layout + weight decision.

**Open blocker to verify first:** `pio run -e esp32-s3-epaper-154g` failed on three
compile errors in `src/font_store.cpp` — on the device `fs::File::name()` returns
`const char *`, the host shim returned `std::string`, and the code called
`String(f.name().c_str())`. Both sides are fixed (shim `name()` now returns
`const char *`; the source uses `String(f.name())`) and the five store host tests
pass again, but **the firmware has not been rebuilt since**. Handover prompt with
the full next-step plan: `prompt-font-assets-transport.md`.

## Note4 A template host delivery — 2026-09-23 (rev 2)

Bridge `bridge/target/debug/bridge-app.exe` was rebuilt and started; local MCP `http://127.0.0.1:8766/mcp` responded. The chosen 400×300 A layout is saved as `codex-status-a` / `epd-ssd2683-400x300-1bpp` through `template_save_v2` (`saved=true`, `published=false`, `source_crc=2b523381`, `compiled_crc=41c31abd`, template `version=1`). Sources, sample inputs, MCP PNGs, measurement tool and instructions: `concepts-400x300/`. The right status group has 4 px icon gaps and a 4 px icon-to-percentage gap; support text uses `f16` and the main numbers use the same bundled bitmap-font series at `f24`/2×. User-selected Icons8 TV Off (#20203) was renamed Bridge Off, with a matching Bridge On; battery #59804 supplied the outline for five 20px states. The template fills that outline from `device.battery`; C++ numeric bind handling was extended for this local value. MCP previews checked 0/25/50/75/100%.

Rev 2 (same day, after user feedback that the fill and text were not centred): status-bar text and the battery fill were re-aligned from pixel measurements (`concepts-400x300/measure-preview.py`, stdlib-only PNG reader with a `--selftest` round trip), and the main numbers' `%` marks were moved to the digits' ink centre (y 149 → 127; measured centres both 132.5) on user request. Measured cause of the status-bar drift: `GUI_Paint.cpp` `Paint_DrawPoint()` writes 1×1 points at `Xpoint-1, Ypoint-1`, so every primitive drawn through lines/rects (separators, bar fill, non-region text) lands 1 px left/up, while icons drawn with `Paint_SetPixel` do not; `Paint_DrawRectangle(..., DRAW_FILL_FULL)` also fills one row less than declared. Both are vendored behaviours that affect firmware and host identically, so the shared engine was left untouched (200×200 behaviour preserved) and the 400×300 template compensates instead: date ink centre 14.0 → 18.0, time 13.5 → 17.5, battery percentage 15.5 → 17.5 against the icon centre line 17.5/18.0; battery fill changed from an off-centre 5-row block (rows 14..18 of the 10-row shell interior) to a symmetric 6-row block (15..20), widened 12 → 14 px so 100% reaches the interior's right edge. A footer `OFF <n>M` row (existing `device.offline_mins` local contract, firmware exposes it after `BRIDGE_LOST_MIN=6`) was added opposite `SYNC <hh:mm>` so the offline/stale state is explicit instead of reusing stale numbers. Eleven previews were rendered and verified as 400×300 with exactly two colours: normal, weekly-only (no 5h, RC=0, no label), bucket 0, bucket 100, offline/no-data, over-long plan/label (clipped per region, no overlap with RC), five battery levels, and the status-bar zoom.

Host work (rev 1 + rev 2): Rust canvas validation and CTP1 envelope now accept the 400×300 target, the shared C++ render harness sets canvas per job, PNG output follows the canvas, `device.date` supplies the local MM/DD field, and MCP v2 validation routes through the tray application. Rev 2 also closed two 400×300 holes in the host verification path: the compiled-template render entry point allocated a 200×200 frame buffer regardless of canvas (`render_compiled_bits_size` added) and `png_to_bits` only decoded 200×200 references (canvas-aware variant added); the `bridge-render` CLI gained `--compare-compiled` (JSON path vs compiled path vs serialize/deserialize round trip on the template's own canvas) and a canvas-aware `--diff`. Existing 200×200 behaviour remains covered by `cargo test -p bridge-core -p bridge-render -p bridge-mcp` (83 passed / 0 failed, isolated `CARGO_TARGET_DIR`) and by `--compare-compiled` on quad/mini/full. MCP `template_render` produced the normal and boundary previews, each checked at 400×300 with exactly two colors. Four 20×20 status glyphs are visual placeholders; independent live Bluetooth/Wi-Fi/host status binds are not yet defined. No Profile change, publish, OTA, or Note4 device verification occurred.

Open items: the bucket-missing presentation is a per-template property, confirmed by the user — `AGENTS.md` now describes both variants (200×200 quad keeps static `100` + hidden reset; the 400×300 A variant hides the 5h block and promotes weekly) instead of stating one global rule. The delivered template lives in the workflow folder plus the bridge v2 store; the legacy seed library (`tools/test-bridge/templates/`) deliberately does not carry the 400×300 variant, so a permanent 400×300 parity test still needs a canonical in-repo fixture location.

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

## Second target (ZecTrix Note4 / 4.2" 400×300 SSD2683) — hardware arrived, bring-up pending

2026-09-23: user confirmed the Note4 hardware has arrived. The arrival blocker is cleared;
the board facts below are still unverified in the repository, and no Note4 ROM has been
flashed or physically verified. Execution order is recorded in
`../next-execution-plan-2026-09-23.md`.

Confirmed facts (user): 4.2" B/W **400×300**, controller **SSD2683**; buttons =
side PGUP/PGDN, front ENTER (nets `KEY_PGUP`/`KEY_ESP32_EN`/`KEY_ENTER`).

Done and compile/host-verified (no OTA):
- target geometry: `platform_target.h` (`TARGET_WIDTH/HEIGHT/ROW_BYTES/FB_BYTES`),
  `template_engine.tplSetCanvas`, `refresh_policy.rgnSetPanel`; 200×200 host
  pixel/region parity tests stay green.
- driver alias layer `src/epd_target.h` (`EPD_TGT_*`); `main.cpp` converted.
- `src/EPD_SSD2683.{h,cpp}` skeleton (400×300/1bpp, windows, planes, BUSY
  propagation, partial-window entry points), gated on `CODEX_TARGET_NOTE4`.
- `platform_target.h` contains explicit `#error`s for missing hardware facts;
  current `platformio.ini` has no Note4 env yet, so an independent ROM env must
  be added during bring-up (no guessing).
- `artifacts/codex-status-0.16.8-bw.bin` (geometry refactor, not deployed).

Hardware facts to confirm before enabling the Note4 build (exact list):
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

## Note4 USB evaluation — 2026-09-23 (supersedes the earlier missing-facts list above)

No Note4 firmware was flashed and no persistent device state was written. The board
was already connected when evaluation began, so a before/after unplug comparison
could not be made. Current enumeration gives an Espressif USB-Serial/JTAG port,
COM5 (VID:PID `303A:1001`, PnP `USB JTAG/serial debug unit`, USB serial/MAC
`7C:4F:AD:B9:34:08`). `esptool v5.3.0` connected repeatedly and reported ESP32-S3
QFN56 rev 0.2, 40 MHz crystal, native USB-Serial/JTAG, MAC `7c:4f:ad:b9:34:08`,
16 MB detected flash (JEDEC manufacturer/device `46/4018`) and 8 MB embedded PSRAM.
Flash reads caused transient resets/download-mode entry; the successful partition read
requested a hard reset afterward. COM5 remained enumerated. No credentials or flash
contents are included in this document.

Sanitized command/measurement summary: `artifacts/note4-usb-evaluation.txt`.

| Item | Confirmed value | Evidence / readiness |
|---|---|---|
| USB endpoint | COM5, VID:PID `303A:1001`; Espressif USB-Serial/JTAG; USB serial `7C:4F:AD:B9:34:08` | `pio device list`, `pnputil`, esptool; identifies the connected ESP32-S3, but no unplug delta was captured |
| Chip | ESP32-S3 QFN56 rev 0.2, 40 MHz crystal, MAC `7c:4f:ad:b9:34:08` | `esptool chip-id` / `read-mac`; repeated connection succeeded |
| Flash / PSRAM | 16 MB detected, raw flash ID `46/4018` (exact flash vendor part unknown); 8 MB embedded PSRAM reported | `esptool flash-id` and chip connect; official DevKit docs specify N16R8 |
| Device partition table | `nvs` 0x9000/0x4000; `otadata` 0xD000/0x2000; `phy_init` 0xF000/0x1000; `ota_0` 0x20000/0x5F0000; `ota_1` 0x610000/0x5F0000; `assets` 0xC00000/0x400000 | Read-only dump [note4-partition-table-0x8000.bin](/D:/Documents/PlatformIO/Projects/codex_status/artifacts/note4-partition-table-0x8000.bin), 4096 bytes, SHA256 `A82133FA4CD77C180D65FA75CA3B5C27BCEBFB8CC4D419362838852E995BA9E5` |
| Panel / board | User-confirmed 4.2-inch B/W 400×300, SSD2683; physical PCB revision and cable label not seen | No panel activity or physical photo captured |
| EPD GPIO (reference) | Power 6, BUSY 8, RST 9, DC 10, CS 11, SCK 12, MOSI/SDA 13 | [Official NOTE4 DevKit V1.0 guide](https://wiki.zectrix.com/en/software/note4-development-guide); unit revision still must be visually matched |
| Buttons (reference) | UP/PGUP 39; DOWN/POWER/PGDN 18; front BOOT/CONFIRM/ENTER 0; active low | Same official guide; physical button response not tested |
| EPD controller source | Public ZECTRIX SSD2683 driver and waveform definitions are available | [Reference driver](https://github.com/itopinion/zectrix-note4-epd-demo/tree/main/components/zectrix_epd), including `private_include/ssd2683_waveform.h`; exact-source adaptation and physical validation remain |

The complete 16 MB pre-flash backup of the device's current flash contents is saved as
`artifacts/note4-preflash-full-flash-16m.bin` (16,777,216 bytes; SHA256
`366dea39643855fd5250d15bb8f23da3b363eca1705a0068b9ab5a598e01d110`). Stub reads
repeatedly stopped at several ranges; 16 KiB ROM-only reads recovered the affected
ranges while keeping the device in download mode. The image length and hash were
verified after completion. No flash write, erase, OTA, or partition change was done.
No UART boot log was saved, and no screen orientation/refresh, BUSY timing, button
response, sleep/wake, board silkscreen, display-cable label, or power-source inspection
was performed. The esptool chip identification does not stand in for those observations.

Font check: the 4 MiB `assets` partition at `0xC00000` contains only 349 non-erased
bytes; neither LittleFS nor SPIFFS could mount it as a file system, and no standalone
TTF/OTF/BDF font file was identified. The active app contains `LvglBuiltInFont` type
strings, so its glyphs appear to be compiled into the firmware rather than stored as
font files. The upstream XiaoZhi NOTE4 board config names Noto Sans Basic 30_4 and
Material Symbols 30_4 ([upstream CMakeLists](https://github.com/78/xiaozhi-esp32/blob/main/main/CMakeLists.txt));
recovering a portable font from this ROM would require reverse-engineering LVGL's
compiled glyph tables. Instead, the matching generated sources were downloaded from
the upstream `78/xiaozhi-fonts` v2.0.0 component into
`artifacts/note4-fonts-xiaozhi-2.0.0/`. Noto Basic 30_4 C source is 1,464,339 B and
Material Symbols 30_4 C source is 152,876 B (1,617,215 B combined source text; compiled
flash size must be measured in the target build). The matching 30_4 common CBIN file is
2,609,092 B, or 62.2% of the 4 MiB assets partition, leaving 1,585,212 B gross. This
fits as a standalone asset; additional assets and filesystem overhead must share the
remaining capacity. No font was extracted from the ROM itself.

The official reference docs unblock pin and memory planning, subject to confirming
this unit matches DevKit PCB V1.0. The official driver/waveform source is available;
adapt its complete initialization, temperature handling, orientation, and waveform
selection instead of adopting the existing skeleton's unverified register values.
The current root `partitions.csv` is an 8 MB/1.54-inch layout and must not be reused:
the device table reserves 16 MB with two 0x5F0000 OTA slots and a 4 MB assets region.
`platformio.ini` still has no Note4 env; `TARGET_PARTIAL` stays disabled until exact
panel full refresh, BUSY behavior, and orientation are proven.

Next steps: visually confirm PCB revision/cable labels and USB/battery power, capture
an indexed photo, then create a separate env/partition layout from the official
reference and audit framebuffer/shadow memory against 8 MB PSRAM. Keep the verified
pre-flash image unchanged as the recovery source for any later authorized flash work.
Before first flash, present the exact env, ROM hash, partition, verified backup, and
recovery procedure for authorization. Bridge-side 400×300 target registration,
canvas-aware validation/preview, and template variant routing remain separate work.

## Other

- `cargo fmt --check` on the whole workspace: pre-existing drift in untouched files.
