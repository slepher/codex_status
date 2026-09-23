#include "font_asset.h"

#include <string.h>

#include "platform_target.h"
#include "v2_state.h"   // v2Crc32: one CRC32/IEEE implementation for the whole build

static inline uint16_t rd16(const uint8_t *p) {
    return (uint16_t)(p[0] | (p[1] << 8));
}

static inline uint32_t rd32(const uint8_t *p) {
    return (uint32_t)p[0] | ((uint32_t)p[1] << 8) | ((uint32_t)p[2] << 16) |
           ((uint32_t)p[3] << 24);
}

static bool idValid(const char *id) {
    if (!id || strlen(id) != 8) return false;
    for (int i = 0; i < 8; i++) {
        char c = id[i];
        if (!((c >= '0' && c <= '9') || (c >= 'a' && c <= 'f'))) return false;
    }
    return true;
}

// Copy a container string section into a bounded, NUL-terminated buffer. The
// section carries no terminator in the container, so a length that fills the
// buffer is rejected rather than silently truncated.
static bool copySection(const uint8_t *src, uint16_t len, char *dst, size_t cap) {
    if (cap == 0 || len >= cap) return false;
    for (uint16_t i = 0; i < len; i++) {
        uint8_t c = src[i];
        if (c < 0x20 || c > 0x7E) return false;   // printable ASCII only
        dst[i] = (char)c;
    }
    dst[len] = 0;
    return true;
}

bool fontAssetValidate(const uint8_t *bytes, size_t len, FontAssetInfo &info, String &err) {
    memset(&info, 0, sizeof(info));
    if (!bytes) { err = "font_null"; return false; }
    if (len < FONT_ASSET_HEADER_BYTES) { err = "font_short_header"; return false; }
    if (len > FONT_ASSET_MAX_BYTES) { err = "font_too_large"; return false; }
    if (rd32(bytes) != FONT_ASSET_MAGIC) { err = "font_magic"; return false; }
    if (rd16(bytes + 4) != FONT_ASSET_VERSION) { err = "font_version"; return false; }
    if (rd16(bytes + 6) != FONT_ASSET_HEADER_BYTES) { err = "font_header_bytes"; return false; }
    const uint32_t fileBytes = rd32(bytes + 8);
    if (fileBytes != (uint32_t)len) { err = "font_length"; return false; }
    const uint32_t payloadCrc = rd32(bytes + 12);
    if (payloadCrc != v2Crc32(bytes + FONT_ASSET_HEADER_BYTES,
                              len - FONT_ASSET_HEADER_BYTES)) {
        err = "font_crc";
        return false;
    }
    info.blobBytes    = rd32(bytes + 16);
    info.glyphCount   = rd32(bytes + 20);
    info.bpp          = bytes[24];
    info.pixelFormat  = bytes[25];
    info.lineHeight   = bytes[26];
    info.baseLine     = bytes[27];
    info.maxAdv       = rd16(bytes + 28);
    info.sizePx       = rd16(bytes + 30);
    info.weight       = rd16(bytes + 32);
    info.filledGlyphs = rd16(bytes + 40);
    info.hint         = bytes[42];
    info.payloadCrc   = payloadCrc;
    info.bytes        = (uint32_t)len;

    if (info.bpp != 1 && info.bpp != 2) { err = "font_bpp"; return false; }
    if (info.pixelFormat > FONT_PIXEL_GRAY4) { err = "font_pixel_format"; return false; }
    if (info.pixelFormat == FONT_PIXEL_BW1 && info.bpp != 1) { err = "font_bpp_format"; return false; }
    if (info.pixelFormat == FONT_PIXEL_GRAY4 && info.bpp != 2) { err = "font_bpp_format"; return false; }
    if (info.glyphCount != FONT_ASSET_GLYPH_COUNT) { err = "font_glyph_count"; return false; }
    if (info.lineHeight == 0 || info.baseLine >= info.lineHeight) {
        err = "font_line_metrics";
        return false;
    }
    if (info.filledGlyphs > FONT_ASSET_GLYPH_COUNT) { err = "font_filled"; return false; }

    const uint16_t nameLen     = rd16(bytes + 34);
    const uint16_t familyLen   = rd16(bytes + 36);
    const uint16_t coverageLen = rd16(bytes + 38);
    // String sections are padded to an even length so the descriptor table is
    // 2-byte aligned and can be used in place (see docs/font-asset-format.md).
    const uint32_t pad = (uint32_t)(nameLen & 1) + (uint32_t)(familyLen & 1) +
                         (uint32_t)(coverageLen & 1);
    const uint32_t headerAndStrings = FONT_ASSET_HEADER_BYTES + nameLen + familyLen +
                                      coverageLen + pad;
    const uint32_t glyphOff = headerAndStrings;
    if (glyphOff & 1u) { err = "font_align"; return false; }
    const uint32_t glyphBytes = (uint32_t)FONT_ASSET_GLYPH_COUNT * FONT_ASSET_GLYPH_BYTES;
    const uint32_t blobOff = glyphOff + glyphBytes;
    if (blobOff > len || info.blobBytes > len - blobOff) { err = "font_payload_range"; return false; }
    // The declared payload must describe the file exactly: trailing slack would
    // let two different files carry the same declared shape.
    if (blobOff + info.blobBytes != len) { err = "font_payload_length"; return false; }

    const uint8_t *p = bytes + FONT_ASSET_HEADER_BYTES;
    if (!copySection(p, nameLen, info.name, sizeof(info.name))) { err = "font_name"; return false; }
    p += nameLen + (nameLen & 1);
    if (!copySection(p, familyLen, info.family, sizeof(info.family))) { err = "font_family"; return false; }
    p += familyLen + (familyLen & 1);
    if (!copySection(p, coverageLen, info.coverage, sizeof(info.coverage))) { err = "font_coverage"; return false; }
    if (info.name[0] == 0) { err = "font_name_empty"; return false; }
    if (info.coverage[0] == 0) { err = "font_coverage_empty"; return false; }

    // Descriptor sanity: every box must fit inside the blob, and a filled glyph
    // must have a non-zero box. A half-written container must never look valid.
    for (uint32_t i = 0; i < info.glyphCount; i++) {
        const uint8_t *g = bytes + glyphOff + i * FONT_ASSET_GLYPH_BYTES;
        const uint32_t off = rd16(g);
        const uint32_t w = g[4], h = g[5];
        if (w == 0 || h == 0) continue;
        const uint32_t need = (uint32_t)((w + 7) / 8) * h;
        if (w > 127 || h > 127) { err = "font_glyph_box"; return false; }
        if (off > info.blobBytes || need > info.blobBytes - off) {
            err = "font_glyph_range";
            return false;
        }
    }

    snprintf(info.id, sizeof(info.id), "%08x", v2Crc32(bytes, len));
    return true;
}

bool fontAssetView(const uint8_t *bytes, size_t len, const FontAssetInfo &info,
                   Note4PropFont &font) {
    if (!bytes || len < FONT_ASSET_HEADER_BYTES) return false;
    const uint16_t nameLen     = rd16(bytes + 34);
    const uint16_t familyLen   = rd16(bytes + 36);
    const uint16_t coverageLen = rd16(bytes + 38);
    const uint32_t pad = (uint32_t)(nameLen & 1) + (uint32_t)(familyLen & 1) +
                         (uint32_t)(coverageLen & 1);
    const uint32_t glyphOff = FONT_ASSET_HEADER_BYTES + nameLen + familyLen +
                              coverageLen + pad;
    const uint32_t blobOff = glyphOff + (uint32_t)FONT_ASSET_GLYPH_COUNT * FONT_ASSET_GLYPH_BYTES;
    if (glyphOff & 1u) return false;
    if (blobOff + info.blobBytes > len) return false;
    font.blob       = bytes + blobOff;
    font.glyphs     = (const Note4Glyph *)(const void *)(bytes + glyphOff);
    font.lineHeight = info.lineHeight;
    font.baseLine   = info.baseLine;
    font.maxAdv     = info.maxAdv;
    return true;
}

bool fontAssetMatchesTarget(const FontAssetInfo &info, String &err) {
    if (strcmp(TARGET_PIXEL_FORMAT, "1bpp") == 0 && info.pixelFormat != FONT_PIXEL_BW1) {
        err = "font_pixel_format_mismatch";
        return false;
    }
    if (strcmp(TARGET_PIXEL_FORMAT, "2bpp") == 0 && info.pixelFormat != FONT_PIXEL_GRAY4) {
        err = "font_pixel_format_mismatch";
        return false;
    }
    return true;
}

const char *fontAssetPixelFormatName(uint8_t pf) {
    switch (pf) {
        case FONT_PIXEL_BW1:   return "bw";
        case FONT_PIXEL_GRAY4: return "gray4";
        default:               return "?";
    }
}

const char *fontAssetBppName(uint8_t bpp) {
    switch (bpp) {
        case 1: return "1bpp";
        case 2: return "2bpp";
        default: return "?";
    }
}
