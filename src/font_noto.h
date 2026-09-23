#pragma once

// Proportional (Noto Sans) font family for the large-display templates.
//
// These tables are cropped from the xiaozhi/Noto LVGL fonts by
// `tools/note4-fonts/crop_lvgl_font.py`: ASCII 0x20-0x7E only, 4 bpp source
// thresholded to the engine's 1 bpp format, one shared blob plus per-glyph
// metrics.  They are a *separate family* from the small-display bitmap fonts
// (f8..f24 in fonts.h): templates pick whichever family they need, and one
// family's glyphs never affect the other's rendering.
//
// Glyph metrics mirror LVGL: `adv` is the pen advance in 1/16 px, the glyph box
// is drawn with its top-left at (pen/16 + ox, baseline - oy - h).
#include <stdint.h>

typedef struct {
    uint16_t off;   // byte offset into the font blob
    uint16_t adv;   // pen advance, 1/16 px
    uint8_t  w;     // box width in pixels
    uint8_t  h;     // box height in pixels
    int8_t   ox;    // box x offset from the pen position
    int8_t   oy;    // box y offset above the baseline
} Note4Glyph;

typedef struct {
    const uint8_t    *blob;
    const Note4Glyph *glyphs;      // 95 entries, indexed by (c - 0x20)
    uint8_t           lineHeight;
    uint8_t           baseLine;
    uint16_t          maxAdv;      // widest advance, 1/16 px (region estimates)
} Note4PropFont;

#include "font_noto_nt16.h"
#include "font_noto_nt30.h"
#include "font_noto_ntthin18.h"
#include "font_noto_ntreg64.h"
#if defined(CODEX_TARGET_NOTE4) || defined(CODEX_RENDER_NOTE4_FONTS)
#include "font_noto_ntreg96.h"
#endif

static const Note4PropFont note4_nt16 = {
    font_nt16_blob, font_nt16_glyphs, FONT_NT16_LINE_HEIGHT, FONT_NT16_BASE_LINE,
    FONT_NT16_MAX_ADV};
static const Note4PropFont note4_nt30 = {
    font_nt30_blob, font_nt30_glyphs, FONT_NT30_LINE_HEIGHT, FONT_NT30_BASE_LINE,
    FONT_NT30_MAX_ADV};
// Font plan (provisional sizes; the layout and the exact cuts are tuned once the
// asset path can deliver fonts without a firmware rebuild):
//   normal text -> Noto Sans Thin 100 @18 px, full ASCII
//   large text  -> Noto Sans Regular 400 @64 px, tabular digits
// Both come from tools/note4-fonts/rasterize_ttf.py (FreeType monochrome).
static const Note4PropFont note4_ntthin18 = {
    font_ntthin18_blob, font_ntthin18_glyphs, FONT_NTTHIN18_LINE_HEIGHT,
    FONT_NTTHIN18_BASE_LINE, FONT_NTTHIN18_MAX_ADV};
static const Note4PropFont note4_ntreg64 = {
    font_ntreg64_blob, font_ntreg64_glyphs, FONT_NTREG64_LINE_HEIGHT,
    FONT_NTREG64_BASE_LINE, FONT_NTREG64_MAX_ADV};
#if defined(CODEX_TARGET_NOTE4) || defined(CODEX_RENDER_NOTE4_FONTS)
static const Note4PropFont note4_ntreg96 = {
    font_ntreg96_blob, font_ntreg96_glyphs, FONT_NTREG96_LINE_HEIGHT,
    FONT_NTREG96_BASE_LINE, FONT_NTREG96_MAX_ADV};
#endif
