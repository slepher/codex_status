// Font asset container (CSFN v1) — the device half of
// `docs/font-asset-format.md`.
//
// Fonts are data: the same container is produced by
// `tools/note4-fonts/rasterize_ttf.py`, served by the bridge font library and
// stored per Profile on the device. The glyph descriptor table in the container
// is laid out exactly like `Note4Glyph`, and the string sections are padded to
// even lengths, so a validated container can be pointed at directly with no
// conversion and no RAM copy of the glyph table.
#pragma once
#include <Arduino.h>
#include <stddef.h>
#include <stdint.h>

#include "font_noto.h"

#define FONT_ASSET_MAGIC        0x4E465343u   // "CSFN"
#define FONT_ASSET_VERSION      1
#define FONT_ASSET_HEADER_BYTES 64
#define FONT_ASSET_GLYPH_COUNT  95            // dense ASCII 0x20..0x7E
#define FONT_ASSET_GLYPH_BYTES  8             // == sizeof(Note4Glyph)
#define FONT_ASSET_MAX_BYTES    (48u * 1024u) // one font asset
#define FONT_ASSET_NAME_MAX     24
#define FONT_ASSET_FAMILY_MAX   32
#define FONT_ASSET_COVERAGE_MAX 16

// pixelFormat values (header byte 25).
#define FONT_PIXEL_BW1   0
#define FONT_PIXEL_GRAY4 1

struct FontAssetInfo {
    char     id[9];        // crc32 of the whole container, lowercase hex
    char     name[FONT_ASSET_NAME_MAX];
    char     family[FONT_ASSET_FAMILY_MAX];
    char     coverage[FONT_ASSET_COVERAGE_MAX];
    uint16_t sizePx;
    uint16_t weight;
    uint8_t  bpp;
    uint8_t  pixelFormat;
    uint8_t  lineHeight;
    uint8_t  baseLine;
    uint16_t maxAdv;       // 1/16 px
    uint32_t blobBytes;
    uint32_t glyphCount;
    uint16_t filledGlyphs;
    uint8_t  hint;
    uint32_t bytes;
    uint32_t payloadCrc;
};

// Validate a container held in RAM. Every inconsistency is reported through
// `err` and rejected: magic/version/header length, declared vs actual length,
// payload CRC, bpp/pixel format, truncated payload, descriptor or blob ranges,
// and odd descriptor-table alignment. `info.id` is computed from the bytes.
bool fontAssetValidate(const uint8_t *bytes, size_t len, FontAssetInfo &info, String &err);

// Engine view over an already validated container. The returned font points into
// `bytes`, so the buffer must stay alive (and unchanged) while it is drawn from.
bool fontAssetView(const uint8_t *bytes, size_t len, const FontAssetInfo &info,
                   Note4PropFont &font);

// Whether this container may be rendered by this firmware build. A 2bpp/gray4
// asset must not be pushed to a 1bpp target and vice versa; the check is explicit
// so the failure is a rejection instead of a wrong-looking page.
bool fontAssetMatchesTarget(const FontAssetInfo &info, String &err);

// `pixelFormat`/`bpp` as the transport words ("bw", "gray4", "1bpp", "2bpp").
const char *fontAssetPixelFormatName(uint8_t pf);
const char *fontAssetBppName(uint8_t bpp);
