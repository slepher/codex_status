# Task 7 — font assets as engine data (fonts tunable without a reflash)

Status: **engine half implemented and host-verified; transport and layout polish
deliberately left for the next step; `pio run` for the firmware is NOT verified
after the last fix (see §5a).** No commit, no device flash, no publish.

Next-step prompt: `prompt-font-assets-transport.md`.

Supersedes the anti-aliasing workstream in
`prompt-font-assets-and-aa.md`: workstream B (2bpp/gray4 anti-aliasing) is
**dropped by user decision** ("停止抗锯齿的支持"). Nothing in this task touches the
200×200 pagination, the legacy channel or the small-display bitmap family.

## 1. Why this is an engine task

The user's revised instruction: *adjust the device-side and bridge-side engine;
fonts and layout can be fine-tuned once the engine is ready.* That only holds if a
font is **data** rather than a compiled-in table — otherwise every weight/size
change needs a firmware build and an OTA. So the deliverable is the path:

```
tools/note4-fonts/rasterize_ttf.py  --CSFN v1 container-->  ---> device font store
        (FreeType monochrome)                             `-->  /fonts/<profile>/<id>.bin
                                              bridge font library (content-addressed, dedup)
```

## 2. Single font registry (the prerequisite)

The font list used to exist in three places (engine, refresh policy, clock fast
path). It is now one table in `src/template_engine.cpp` (`TPL_FONTS`), reached
through `tplFontCount/…/tplFontCellBy*/tplFontClockBox/tplFontDrawClock`
declared in `template_engine.h`:

| Index | Name | Family | Used for |
|---|---|---|---|
| 0–4 | `f8`…`f24` | compiled bitmap | 200×200 templates (unchanged) |
| 5 | `nt16` | LVGL 4bpp crop | existing 400×300 |
| 6 | `nt30` | LVGL 4bpp crop | existing 400×300 |
| 7 | `ntthin18` | **Noto Sans Thin 100 @18** | normal text (provisional size) |
| 8 | `ntreg64` | **Noto Sans Regular 400 @64** | large text (provisional size) |

* Appending keeps `CtOp.font` (a persisted index) stable; entries are never
  reordered.
* `main.cpp`'s clock fast path no longer owns a font table: it asks the registry
  for the "HH:MM" box and blits through `tplFontDrawClock`, which handles both
  families with the same pixel writes. `CLK_MAX_BYTES` is 512 on the 400×300
  target and stays 64 on 200×200, so 200×200 behaviour is untouched.
* `bridge/crates/core/src/template.rs::FONTS` mirrors the same nine names.

## 3. What was broken in the tree and is now fixed

| Item | Was | Now |
|---|---|---|
| `bridge/crates/render/tests/compiled.rs` | called `rgn_build_compiled(&blob)` (1 arg) — tests did not compile | passes the template canvas |
| `rgn_build(template)` (JSON path) | never called `set_panel`, so 400×300 regions were derived against 200×200 geometry | derives against the template's canvas (`rgn_build_size`) |
| `rgnSetPanel` | defined **inside an anonymous namespace** → host FFI could not link it at all | external linkage |
| `RGN_MAX` | 32; the 400×300 A template declares 38 elements → whole-frame fallback | 64 |
| `Rgn::area` | `uint16_t`; overflows above 65 535 px (400×300 has 120 000) | `uint32_t` |
| `bridge-render` CLI | no region evidence entry point | `--regions` (JSON and compiled paths) |
| `bridge/crates/render/build.rs` | tracked only `.cpp`; regenerated font **headers** did not trigger a rebuild, so the preview could silently render old glyphs | all font/engine headers tracked |
| host LittleFS shim | no directories, no `name()/openNextFile()/rmdir()` | directory semantics, so the real font store runs in host tests |

## 4. Font container (`docs/font-asset-format.md`)

CSFN v1: 64-byte header + `name|family|coverage|glyphs[95]|blob`. The descriptor
table is the engine's own `Note4Glyph` layout (8 B/entry) and the string sections
are padded to even lengths, so **the device points straight into the container**:
no conversion and no RAM copy of a glyph table. `font_id = crc32(whole file)`
(content address); `payloadCrc32` covers the payload.

Generated assets (committed): `bridge/assets/fonts/font_ntthin18.bin`
(2268 B, id `18c2e4ed`) and `font_ntreg64.bin` (3315 B, id `4dc3b226`).

## 5. Verification actually run

```
# bridge + device engine, isolated target dir (a bridge-app is running from bridge/target)
cd bridge
$env:CARGO_TARGET_DIR='...\bridge\artifacts\cargo-target-fonts'
cargo test -p bridge-core -p bridge-render -p bridge-mcp      # 110 passed / 0 failed
```

* `crates/render/tests/font_asset.rs` (3 tests) — device parser vs shipped
  containers: exact ids/sizes/coverage/`view=1`, byte-flip → `font_crc`,
  truncation, wrong magic, `gray4` header on a 1bpp target, padding removed.
* `crates/render/tests/font_store.rs` (5 tests) — profile-scoped store:
  second push of the same id is a **no-op** (`AlreadyPresent`, byte count
  unchanged); a torn write leaves the previously stored font byte-exact and no
  visible new font; a mis-named or truncated file is reported as bad rather than
  listed/loaded; pruning keeps exactly the referenced ids; an unbound store and
  Profile switching fail closed.
* `crates/core/src/platform/fonts.rs` (18 tests, written by a subagent) — bridge
  library: fixtures parse with the same ids, corrupted/truncated/oversized/
  mis-padded containers rejected, `add` idempotent, `resolve_name`,
  `diff_inventory` pushes only missing ids, caps enforced.
* `bridge-render --compare-compiled --regions` on the regenerated 400×300
  template: `json vs compiled diff pixels: 0`, round-trip `0`, `regions (json):
  18`, `regions (compiled): 18`.

A real bug was found by these tests: `fontStoreInventory` reported a mis-named
file under its *content* id, so the bridge would have considered an id installed
while `load` rejected it. Inventory now requires name == id.

### 5a. Firmware build — open item

`pio run -e esp32-s3-epaper-154g` failed with three compile errors in
`src/font_store.cpp`: on the device `fs::File::name()` returns `const char *`
while the host shim returned `std::string`, and the code called
`String(f.name().c_str())`. **Both sides are now fixed** — the shim's `name()`
returns `const char *` (matching the device, with an in-object name buffer) and
`font_store.cpp` uses `String(f.name())` — and the five store host tests pass
again. **The firmware itself has not been rebuilt since the fix**, because the
session ended first. First action next time:

```powershell
pio run -e esp32-s3-epaper-154g      # sandbox: needs danger-full-access (see §8)
```

If it then fails `checkprogsize`, the compiled-in font set is the first suspect:
`nt16` + `nt30` (LVGL crops, blobs 1598 + 4051 B) are now superseded by
`ntthin18` + `ntreg64` and would reclaim ~7 KB, and dropping them is safe once no
shipped template references them (`codex-status-a` no longer does).

## 6. Not done (next step, in order)

1. **Verify the firmware build** after the `File::name()` fix (§5a).
2. **Template → asset resolution + ABI**: `CtOp` still selects a font by the
   compiled index. Asset fonts need `CtOp.fontRef -> CtTemplate.fontRefs[] {name,
   id}` with `CT_ABI` bump; at activation every ref must resolve from the
   Profile store or the whole template is rejected (never half-rendered).
   Consequence to accept: an ABI bump invalidates already-persisted compiled
   templates until the bridge republishes.
3. **Transport**: `font_inventory` in `/status.json`, per-font
   BEGIN/CHUNK/COMMIT + device ACK, idempotent repeat push (already guaranteed by
   the store), and the "delete what the new set does not reference" step
   (`fontStorePrune` exists, nothing calls it yet).
4. **Bridge wiring**: `service.rs` publish preview ("will push N fonts, X bytes,
   M already present"), MCP read-only font tools, UI font usage.
5. **Layout polish** for `ntthin18`/`ntreg64` in `codex-status-a`, and the final
   weight/size decision (see §7).
6. `assets` partition (4 MB) for the 16 MB Note4 board: the store currently uses
   LittleFS. Fonts are 2–4 KB each today, so LittleFS is adequate for the interim;
   the `assets` region is the intended home for big/CJK payloads.

## 7. Open decision for the user

The provisional faces are Noto Sans **Thin 100 @18** (normal text) and
**Regular 400 @64** (large text), as instructed. At 1:1 on this panel, Thin @18
gives 1 px stems (same as the previous Light @16), and Regular @64 gives 6–7 px —
both verified by `weight-ab.png`. If the thin body text reads too faint on the
real panel, the registry makes switching to Light 300 a one-line asset change
plus a re-emit — which is exactly the point of this task.

## 8. Environment facts the next session needs

* **Sandbox**: `pio run` needs `danger-full-access` (PlatformIO locks
  `C:\Users\cogic\.platformio\platforms.lock` outside the workspace). Shell
  writes are **denied** under `tools/`, the repo-root `artifacts/`, and
  `project-workflow/.../concepts-400x300/`; the `write`/`edit` file tools can
  still write there. Use `bridge/artifacts/` (gitignored via the `artifacts/`
  pattern) as the isolated `CARGO_TARGET_DIR` — `bridge/target` is locked by a
  running `bridge-app.exe`.
* Do **not** kill a running `pio`/`python` mid-install: interrupting a PlatformIO
  package install corrupts `~/.platformio/packages/<pkg>` (missing
  `package.json` → `MissingPackageManifestError`). It happened here with
  `framework-arduinoespressif32-libs` and was repaired by deleting that directory
  and re-running the build with network access.
* The `esp32-s3-epaper-154g-btpm` env pulls the pioarduino platform and can stall
  on a blocked fetch; build `-e esp32-s3-epaper-154g` explicitly.
* The Noto TTFs live in `tools/note4-fonts/vendor/` (needs a gitignore entry;
  the files were fetched with `danger-full-access` because the sandbox blocks TLS
  egress). `tools/note4-fonts/vendor/py/` holds the vendored `freetype-py` wheel.
