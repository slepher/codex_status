#pragma once

#include <stddef.h>
#include <stdint.h>
#include <stdbool.h>

// Packed palette pixels: left pixel in the high nibble. LCD adapters convert
// bounded rows to RGB565; no second full RGB framebuffer is required.
static inline bool ap01TemplateCanvas(int w, int h) { return w == 320 && h == 240; }
static inline size_t templateFrameBytes(int w, int h) {
    return ap01TemplateCanvas(w, h) ? (size_t)((w + 1) / 2) * h : (size_t)((w + 7) / 8) * h;
}
static inline uint16_t ap01Palette565(unsigned index) {
    static const uint16_t palette[] = {0x0000, 0xffff, 0x56f9, 0xfd48, 0x7c51, 0x2144, 0xfaeb,
        0x4208, 0x8410, 0xbdd7, 0x1947, 0x2b8d, 0x4493, 0x5142, 0xa384, 0x10a2};
    return index < sizeof(palette) / sizeof(palette[0]) ? palette[index] : 0;
}
static inline bool ap01ConvertRow(const uint8_t *pixels, size_t length, unsigned row,
                                 uint16_t *out, size_t count) {
    if (!pixels || length != 38400 || row >= 240 || !out || count < 320) return false;
    const uint8_t *src = pixels + row * 160;
    for (unsigned x = 0; x < 320; ++x)
        out[x] = ap01Palette565(x & 1 ? src[x / 2] & 15 : src[x / 2] >> 4);
    return true;
}

// Quantize a coverage blend into the shared LCD palette. Source colors remain
// the seven semantic template colors; extra entries are renderer-only shades.
static inline unsigned ap01Coverage(unsigned fg, unsigned bg, unsigned under,
                                    unsigned fa, unsigned ba, unsigned total) {
    if (fa == total) return fg;
    if (ba == total) return bg;
    if (!fa && !ba) return under;
    unsigned ua=total-fa-ba;
    uint16_t f=ap01Palette565(fg), b=ap01Palette565(bg), u=ap01Palette565(under);
    int r=(((f>>11)&31)*fa+((b>>11)&31)*ba+((u>>11)&31)*ua+total/2)/total;
    int g=(((f>>5)&63)*fa+((b>>5)&63)*ba+((u>>5)&63)*ua+total/2)/total;
    int bl=((f&31)*fa+(b&31)*ba+(u&31)*ua+total/2)/total;
    unsigned best=0; int error=100000;
    for (unsigned i=0;i<16;++i) {
        uint16_t c=ap01Palette565(i);
        int dr=r-((c>>11)&31), dg=g-((c>>5)&63), db=bl-(c&31);
        int d=dr*dr*4+dg*dg+db*db*4;
        if (d<error) {error=d;best=i;}
    }
    return best;
}

static inline unsigned ap01Blend(unsigned fg, unsigned bg, unsigned alpha) {
    return ap01Coverage(fg, bg, bg, alpha, 0, 15);
}
