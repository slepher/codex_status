# Font asset format (CSFN v1)

Fonts are **assets, not firmware**: the engine renders a font loaded from the
device font store exactly like one compiled into the image, so a font's weight,
size or charset can be changed by pushing a new asset instead of reflashing.

This document is the contract between three implementations that must agree
byte for byte:

| Side | Code |
|---|---|
| Generator | `tools/note4-fonts/rasterize_ttf.py` (FreeType monochrome) |
| Bridge (Rust) | `bridge/crates/core/src/platform/fonts.rs` |
| Device (C++) | `src/font_asset.{h,cpp}` |

## Container

Little-endian, fixed 64-byte header followed by the payload.

| Offset | Type | Field | Notes |
|---|---|---|---|
| 0 | u32 | `magic` | `"CSFN"` (0x4E465343) |
| 4 | u16 | `version` | 1 |
| 6 | u16 | `headerBytes` | 64 |
| 8 | u32 | `fileBytes` | total container length, header included |
| 12 | u32 | `payloadCrc32` | CRC32 over `[headerBytes, fileBytes)` |
| 16 | u32 | `blobBytes` | row-padded 1bpp glyph blob length |
| 20 | u32 | `glyphCount` | 95 (dense ASCII `0x20..0x7E`) |
| 24 | u8 | `bpp` | 1 |
| 25 | u8 | `pixelFormat` | 0 = `bw` (1bpp), 1 = `gray4` (2bpp, reserved) |
| 26 | u8 | `lineHeight` | px; one line of text |
| 27 | u8 | `baseLine` | px; descent below the baseline |
| 28 | u16 | `maxAdv` | widest advance, 1/16 px (region estimation) |
| 30 | u16 | `sizePx` | nominal pixel size the face was rasterized at |
| 32 | u16 | `weight` | 100–900 |
| 34 | u16 | `nameLen` | |
| 36 | u16 | `familyLen` | |
| 38 | u16 | `coverageLen` | |
| 40 | u16 | `filledGlyphs` | how many of the 95 slots carry ink |
| 42 | u8 | `hint` | 0 native, 1 auto (FreeType autohinter), 2 none |
| 43 | u8 | reserved | 0 |
| 44 | u32[5] | reserved | 0 |

Payload, in order:

1. `name[nameLen]` — the template-visible font name, ASCII, e.g. `ntthin18`
2. `family[familyLen]` — source file name, e.g. `NotoSans-Thin.ttf`
3. `coverage[coverageLen]` — `ascii` (95 slots) or `digits` (subset)
4. `glyphs[95]` — 8 bytes each, **exactly the engine's `Note4Glyph`**:
   `u16 off, u16 adv, u8 boxW, u8 boxH, i8 ofsX, i8 ofsY`
5. `blob[blobBytes]` — glyph bitmaps, row-padded to whole bytes, MSB first,
   bit set = ink

**Alignment:** each string section is padded with zero bytes to an even length,
and that padding is *not* counted in its length field. The three padded sections
therefore start the descriptor table on a 2-byte boundary, which is what lets the
device point `Note4Glyph *` straight into the container. A reader must pad when
computing offsets and must reject a container whose descriptor table would start
on an odd offset.

Descriptors are stored in the engine's own layout so a validated container can be
pointed at directly — no conversion, no RAM copy of a glyph table.

## Identity

* `font_id = crc32(whole container)` — content address, 8 lowercase hex digits.
  Any change to a glyph, a metric or a header field changes the id, so a font is
  immutable and an update is always a new id.
* `payloadCrc32` is the in-file integrity check the device verifies after
  receiving a font and again when loading it from storage.

Both are plain CRC32, matching the template/bundle envelope.

## Storage on the device

* Path: `fonts/<profile_id>/<font_id>.bin` — fonts belong to a Profile; there is
  deliberately **no cross-Profile sharing** on the device (the same font used by
  two Profiles exists twice).
* Writes are atomic: `.../<font_id>.tmp` → verify CRC → rename. A torn or
  half-written file is never visible under its final name.
* Fonts are write-once: an existing `font_id` is never rewritten, and a
  duplicate push is a no-op that transfers zero bytes.
* Replacing a Profile's font set deletes files in that Profile's directory that
  the new set does not reference.

## Bridge-side library

The bridge keeps one content-addressed library (deduplicated, one blob can serve
many Profiles). A Profile explicitly binds each asset font name to one `font_id`.
The proposed Note4 publish planner compares the complete manifest's object IDs
with the current/rollback committed manifest references from authenticated
status. It does not use a separate font-directory inventory as the publish
contract. Only missing objects would be sent after protocol confirmation.

## Rendering semantics (unchanged from the compiled-in tables)

```
baseline      = y + lineHeight - baseLine
glyph origin  = (pen/16 + ofsX, baseline - ofsY - boxH)
pen          += adv (1/16 px)
```

`tplFontClockBox` reserves the clock window from the widest digit advance, and
the refresh policy bounds a text region with `maxAdv` / `lineHeight`; both are
container fields, so a pushed font works with the existing region derivation.

## Implementation status

| Piece | Where | State |
|---|---|---|
| Generator (`emit`) | `tools/note4-fonts/rasterize_ttf.py` | done; writes the engine header **and** the `.bin` |
| Device parser | `src/font_asset.{h,cpp}` | done; host-tested against the shipped containers |
| Device store | `src/font_store.{h,cpp}` | done; atomic write, CRC re-verify, inventory, prune, caps |
| Bridge library | `bridge/crates/core/src/platform/fonts.rs` | CSFN import, content-addressed dedup, explicit per-Profile version choice; Bridge no longer applies the old 8/48 KiB product caps |
| Template → asset resolution | `CtOp.fontRef` + `CT_ABI` bump | **not implemented** — templates still select fonts by compiled index |
| Versioned manifest/object transfer | firmware HTTP + bridge | **not implemented**; draft in `project-workflow/note4-bridge-publish/protocol.md` awaits joint confirmation |
| `assets` partition placement | 16 MB Note4 board | **not implemented** (LittleFS interim; assets are 2–4 KB today) |

Current firmware still limits one CSFN asset to 48 KiB and one Profile to eight
font files. These are interim implementation limits, not the Note4 product
contract. The versioned protocol requires a device parser/streaming audit and
advertised object/space limits before Bridge transport may be enabled. Existing
firmware cannot accept a Bridge-imported larger CSFN container.

Also not yet a library source: `crop_lvgl_font.py` (the `nt16`/`nt30` LVGL crops)
has no container emitter, so those two exist only as compiled-in tables.
