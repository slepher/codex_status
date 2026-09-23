#!/usr/bin/env python3
"""Rasterize TrueType/OpenType outlines to the engine's 1bpp proportional font.

`crop_lvgl_font.py` harvests glyphs that were already rendered by the upstream
LVGL converter (4 bpp alpha, thresholded down to 1 bpp).  That path can only
reproduce the sizes and weights the xiaozhi component happened to ship, and the
threshold is applied *after* antialiasing, which thins or breaks strokes.

This tool renders straight from a static hinted TTF with FreeType's *monochrome*
rasterizer (`FT_LOAD_TARGET_MONO`), so grid-fitting/hinting runs on the outline
itself -- the same pipeline `lv_font_conv --bpp 1` uses.  Output is one engine
header: row-padded 1bpp blob + one 6-byte descriptor per ASCII slot.

Desk height and weight come from the caller:

  * usage numbers ("大字体方案")  -> ExtraLight 200, digits + '%' + ':' only
  * every other string ("小字体方案") -> Light 300, full ASCII 0x20-0x7E

Descriptor semantics (identical to `font_noto.h` and `crop_lvgl_font.py`):

    offset    byte offset of the glyph's row-padded 1bpp box in the blob
    adv        pen advance in 1/16 px (FreeType gives 1/64 px)
    box_w/h    ink box size in pixels
    ofs_x      box left edge relative to the pen position
    ofs_y      baseline - box bottom (negative for descenders)

The engine draws at  gx = pen/16 + ofs_x,  gy = baseline - ofs_y - box_h,  and
`baseline = y + lineHeight - baseLine`, which is why the emitted line metrics are
ascent (= lineHeight - baseLine) and descent (= baseLine) rather than FreeType's
raw ascender/descender.

Usage:
  python rasterize_ttf.py report   <ttf> <size> [--hint native|auto|none]
  python rasterize_ttf.py emit     <ttf> <size> <name> [--charset ascii|digits]
                                   [--out <dir>] [--hint ...]
  python rasterize_ttf.py specimen <ttf> <size> <text> <out.png>
                                   [--charset ...] [--hint ...] [--scale N]
"""

import os
import re
import struct
import sys
import zlib

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "vendor", "py"))
import freetype  # noqa: E402  (vendored wheel, tools/note4-fonts/vendor)

ASCII = list(range(0x20, 0x7F))
# Usage figures: tabular digits plus the marks that can legitimately appear in a
# percentage. `?` is the engine's fallback for out-of-table characters, so it is
# carried too: an unexpected character renders as '?' instead of silently
# disappearing.
DIGITS = [ord(c) for c in "0123456789%:.,- ?"]

CHARSETS = {"ascii": ASCII, "digits": DIGITS}

# Hinting modes. `native` uses the font's own TrueType hinting (the static
# `hinted/` Noto builds have it); `auto` forces FreeType's autohinter, which is
# what lv_font_conv turns on by default; `none` disables grid-fitting entirely
# and is kept for A/B evidence only.
HINT_FLAGS = {
    "native": 0,
    "auto": freetype.FT_LOAD_FORCE_AUTOHINT,
    "none": freetype.FT_LOAD_NO_HINTING,
}


class Face:
    def __init__(self, path, size, hint="native"):
        self.path = path
        self.size = size
        self.hint = hint
        if hint not in HINT_FLAGS:
            raise SystemExit("unknown hint mode %r" % hint)
        self.face = freetype.Face(path)
        self.face.set_pixel_sizes(0, size)
        # Ascent/descent of the hinted face at this pixel size (26.6 fixed).
        self.ascent = -(-self.face.size.ascender // 64)      # ceil
        self.descent = -(-(-self.face.size.descender) // 64)  # ceil of |descender|
        self.line_height = self.ascent + self.descent
        self.base_line = self.descent

    def load(self, ch):
        flags = freetype.FT_LOAD_RENDER | freetype.FT_LOAD_TARGET_MONO | HINT_FLAGS[self.hint]
        self.face.load_char(chr(ch), flags)
        return self.face.glyph

    def glyph(self, ch):
        """Return (rows, box_w, box_h, ofs_x, ofs_y, adv_1_16).

        rows: list of box_h lists of 0/1, MSB-first within each source byte.
        """
        g = self.load(ch)
        bm = g.bitmap
        w, h = bm.width, bm.rows
        pitch = bm.pitch
        buf = bm.buffer
        rows = []
        for y in range(h):
            row = buf[y * pitch:(y + 1) * pitch]
            rows.append([(row[x >> 3] >> (7 - (x & 7))) & 1 for x in range(w)])
        adv64 = g.advance.x
        if adv64 % 4:
            raise SystemExit("glyph U+%04X advance %d/64 is not a whole 1/16 px"
                             % (ch, adv64))
        ofs_y = g.bitmap_top - h
        return rows, w, h, g.bitmap_left, ofs_y, adv64 // 4

    def pack(self, ch):
        rows, w, h, ofs_x, ofs_y, adv = self.glyph(ch)
        stride = (w + 7) // 8
        blob = bytearray(stride * h)
        for y, row in enumerate(rows):
            for x, ink in enumerate(row):
                if ink:
                    blob[y * stride + x // 8] |= 0x80 >> (x % 8)
        return bytes(blob), w, h, ofs_x, ofs_y, adv

    def digits_are_tabular(self):
        """True when every digit shares one advance (no `tnum` shaping needed)."""
        advs = {ch: self.load(ch).advance.x for ch in range(ord("0"), ord("9") + 1)}
        return len(set(advs.values())) == 1, advs


def write_png(path, width, height, pixels):
    """pixels: list of rows, each a list of 0 (ink) / 255 (white)."""
    raw = b"".join(b"\x00" + bytes(row) for row in pixels)

    def chunk(tag, data):
        return (struct.pack(">I", len(data)) + tag + data
                + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF))

    png = (b"\x89PNG\r\n\x1a\n"
           + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 0, 0, 0, 0))
           + chunk(b"IDAT", zlib.compress(raw))
           + chunk(b"IEND", b""))
    with open(path, "wb") as fh:
        fh.write(png)


def layout(f, text, scale=1):
    """Place a string exactly like the engine's drawPropText.

    Returns (width, height, list of (x, y, rows)).
    """
    pen = 0            # 1/16 px
    placed = []
    for ch in text:
        cp = ord(ch)
        if cp < 0x20 or cp > 0x7E:
            cp = ord("?")
        rows, w, h, ofs_x, ofs_y, adv = f.glyph(cp)
        baseline = f.line_height - f.base_line
        gx = (pen // 16) + ofs_x * scale
        gy = baseline - ofs_y * scale - h * scale
        placed.append((gx, gy, rows))
        pen += adv * scale
    return (pen + 15) // 16, f.line_height * scale, placed


def render_text(f, text, scale=1, pad=2):
    tw, th, placed = layout(f, text, scale)
    width, height = tw + pad * 2, th + pad * 2
    buf = [[255] * width for _ in range(height)]
    for gx, gy, rows in placed:
        for y, row in enumerate(rows):
            for x, ink in enumerate(row):
                if not ink:
                    continue
                for sy in range(scale):
                    for sx in range(scale):
                        px = pad + gx + x * scale + sx
                        py = pad + gy + y * scale + sy
                        if 0 <= px < width and 0 <= py < height:
                            buf[py][px] = 0
    return width, height, buf


def report(ttf, size, hint):
    f = Face(ttf, size, hint)
    tabular, advs = f.digits_are_tabular()
    print("face          %s" % os.path.basename(ttf))
    print("pixel size    %d px   hint=%s" % (size, hint))
    print("ascender      %d px (lineHeight %d, baseLine %d)"
          % (f.ascent, f.line_height, f.base_line))
    print("descender     %d px" % f.descent)
    print("tabular 0-9   %s (%d/64 px per digit)"
          % ("yes" if tabular else "NO - needs OpenType tnum", advs[ord('0')]))
    print()
    print("%-6s %5s %5s %5s %5s %6s  %s" % ("char", "box_w", "box_h", "ofs_x",
                                            "ofs_y", "adv/16", "stem run lengths"))
    for ch in "018:%.?Hg":
        rows, w, h, ofs_x, ofs_y, adv = f.glyph(ord(ch))
        runs = []
        for row in rows:
            run = best = 0
            for v in row:
                run = run + 1 if v else 0
                best = max(best, run)
            runs.append(best)
        vertical = [r for r in runs if r]
        print("%-6s %5d %5d %5d %5d %6d  %s"
              % (repr(ch)[1:-1], w, h, ofs_x, ofs_y, adv,
                 "max %d, median %d px" % (max(vertical),
                                           sorted(vertical)[len(vertical) // 2])
                 if vertical else "-"))


def emit(ttf, size, name, charset, out_dir, hint, asset_out=None):
    """Write the engine header *and* the canonical font asset container.

    The header is what the firmware compiles in today; the ``.bin`` is the same
    font as a transportable asset (see ``pack_container``), so the bridge can
    deliver it and the device can load it without a firmware rebuild. Both come
    from one rasterization, which is what keeps the compiled-in and the pushed
    copy byte-identical.
    """
    f = Face(ttf, size, hint)
    cps = CHARSETS[charset]
    blob = bytearray()
    glyphs = bytearray()
    rows_out = []
    present = []
    for cp in ASCII:
        if cp not in cps:
            rows_out.append((0, 0, 0, 0, 0, 0))
            glyphs += struct.pack("<HHBBbb", 0, 0, 0, 0, 0, 0)
            continue
        packed, w, h, ofs_x, ofs_y, adv = f.pack(cp)
        rows_out.append((len(blob), adv, w, h, ofs_x, ofs_y))
        glyphs += struct.pack("<HHBBbb", len(blob), adv, w, h, ofs_x, ofs_y)
        blob += packed
        present.append(cp)
    os.makedirs(out_dir, exist_ok=True)
    upper = name.upper()
    header = os.path.join(out_dir, "font_noto_%s.h" % name)
    with open(header, "w", encoding="utf-8") as fh:
        fh.write("// Generated by tools/note4-fonts/rasterize_ttf.py\n")
        fh.write("//   source   : %s\n" % os.path.basename(ttf))
        fh.write("//   raster   : FreeType monochrome (1 bpp), %d px, hint=%s\n"
                 % (size, hint))
        fh.write("//   charset  : %s (%d of 95 ASCII slots filled)\n"
                 % (charset, len(present)))
        fh.write("//   sha256   : %s\n" % file_sha256(ttf))
        fh.write("// Note4Glyph: {blob offset, advance (1/16 px), box_w, box_h, ofs_x, ofs_y}\n")
        fh.write("#pragma once\n#include <stdint.h>\n\n")
        fh.write("#define FONT_%s_LINE_HEIGHT %d\n" % (upper, f.line_height))
        fh.write("#define FONT_%s_BASE_LINE %d\n" % (upper, f.base_line))
        fh.write("#define FONT_%s_BLOB_BYTES %d\n" % (upper, len(blob)))
        filled = [r[1] for r in rows_out if r[2]]
        fh.write("#define FONT_%s_MAX_ADV %d\n\n" % (upper, max(filled) if filled else 0))
        fh.write("static const uint8_t font_%s_blob[%d] = {\n" % (name, len(blob)))
        for i in range(0, len(blob), 16):
            fh.write("    " + " ".join("0x%02X," % b for b in blob[i:i + 16]) + "\n")
        fh.write("};\n\n")
        fh.write("static const Note4Glyph font_%s_glyphs[%d] = {\n" % (name, len(ASCII)))
        for cp, r in zip(ASCII, rows_out):
            fh.write("    {%5d, %4d, %3d, %3d, %4d, %4d}, /* 0x%02X %s */\n"
                     % (r[0], r[1], r[2], r[3], r[4], r[5], cp,
                        repr(chr(cp))[1:-1] if cp != 0x20 else "space"))
        fh.write("};\n")
    print("wrote %s (%d slots, %d filled, blob %d bytes, lineHeight %d, baseLine %d)"
          % (header, len(ASCII), len(present), len(blob), f.line_height, f.base_line))

    container = pack_container(name, os.path.basename(ttf), charset, f, size,
                               bytes(blob), bytes(glyphs), len(present))
    asset_dir = asset_out or out_dir
    os.makedirs(asset_dir, exist_ok=True)
    asset = os.path.join(asset_dir, "font_%s.bin" % name)
    with open(asset, "wb") as fh:
        fh.write(container)
    print("wrote %s (%d bytes, font_id %08x)"
          % (asset, len(container), crc32(container)))


def crc32(data):
    return zlib.crc32(data) & 0xFFFFFFFF


CONTAINER_MAGIC = b"CSFN"
CONTAINER_HEADER = 64
PIXEL_FORMAT_BW1 = 0
# One descriptor is exactly the engine's `Note4Glyph` (u16 off, u16 adv, u8 w,
# u8 h, i8 ox, i8 oy = 8 bytes), so a loaded container can be used in place
# without any conversion step.
GLYPH_BYTES = 8


def pack_container(name, family, coverage, face, size_px, blob, glyphs, filled):
    """Canonical font asset container (see docs/font-asset-format.md).

    Layout, little-endian, fixed 64-byte header then the payload:

      0  u32 magic "CSFN"      20 u32 glyphCount (dense ASCII-95)
      4  u16 version = 1       24 u8  bpp (1)
      6  u16 headerBytes = 64  25 u8  pixelFormat (0 = b/w 1bpp)
      8  u32 fileBytes         26 u8  lineHeight
     12  u32 payloadCrc32      27 u8  baseLine
     16  u32 blobBytes         28 u16 maxAdv (1/16 px)
                               30 u16 sizePx
                               32 u16 weight   34 u16 nameLen
                               36 u16 familyLen  38 u16 coverageLen
                               40 u16 filledGlyphs  42 u8 hint
                               43 u8 reserved  44..63 u32 reserved[5]

    Payload = name | family | coverage | glyphs | blob, where `glyphs` is 95
    descriptors of exactly the engine's `Note4Glyph` layout (8 bytes each:
    u16 off, u16 adv, u8 w, u8 h, i8 ox, i8 oy), so a loaded container is usable
    in place with no conversion.

    Each string section is padded to an even length with zero bytes (the padding
    is NOT counted in its length field): the device points at the descriptor table
    directly, so it has to start on a 2-byte boundary.

    `payloadCrc32` covers the payload, so the container is self-checking;
    `font_id` is the CRC32 of the whole container, so any change to a glyph or a
    metric changes the identity. Both are plain CRC32, matching the repo's other
    integrity fields.
    """
    weight = 400
    match = re.search(r"(Thin|ExtraLight|Light|Regular|Medium|SemiBold|Bold|Black)",
                      family)
    if match:
        weight = {"Thin": 100, "ExtraLight": 200, "Light": 300, "Regular": 400,
                  "Medium": 500, "SemiBold": 600, "Bold": 700, "Black": 900}[match.group(1)]
    hint_code = {"native": 0, "auto": 1, "none": 2}[face.hint]
    name_b = name.encode("ascii")
    family_b = family.encode("ascii")
    coverage_b = coverage.encode("ascii")

    def even(b):
        """Pad a string section to an even length.

        The glyph table holds `uint16_t` fields and the device points straight at
        it, so it must be 2-byte aligned. The padding is not counted in the
        section's length field, so a reader can always recover the string.
        """
        return b + (b"\x00" if len(b) % 2 else b"")

    payload = (even(name_b) + even(family_b) + even(coverage_b) + glyphs + blob)
    header = bytearray(CONTAINER_HEADER)
    struct.pack_into("<4sHHIIII", header, 0, CONTAINER_MAGIC, 1, CONTAINER_HEADER,
                     CONTAINER_HEADER + len(payload), crc32(payload),
                     len(blob), len(ASCII))
    header[24] = 1                  # bpp
    header[25] = PIXEL_FORMAT_BW1   # pixel format
    header[26] = face.line_height
    header[27] = face.base_line
    struct.pack_into("<HHH", header, 28, max_adv(glyphs), size_px, weight)
    struct.pack_into("<HHHH", header, 34, len(name_b), len(family_b),
                     len(coverage_b), filled)
    header[42] = hint_code
    return bytes(header) + payload


def max_adv(glyphs):
    """Widest advance in the descriptor table (stride matches Note4Glyph)."""
    best = 0
    for i in range(0, len(glyphs), GLYPH_BYTES):
        _off, adv, w, _h, _ox, _oy = struct.unpack_from("<HHBBbb", glyphs, i)
        if w and adv > best:
            best = adv
    return best


def specimen(ttf, size, text, out_png, charset, hint, scale):
    f = Face(ttf, size, hint)
    width, height, buf = render_text(f, text.replace("\\n", "\n"), scale)
    if scale > 1:
        buf = [[v for v in row for _ in range(scale)] for row in buf for _ in range(scale)]
        width *= scale
        height *= scale
    write_png(out_png, width, height, buf)
    print("wrote %s (%dx%d, %d px face, scale %d)"
          % (out_png, width, height, size, scale))


def file_sha256(path):
    import hashlib
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        for block in iter(lambda: fh.read(65536), b""):
            h.update(block)
    return h.hexdigest().upper()


def opt(args, flag, default=None, cast=str):
    return cast(args[args.index(flag) + 1]) if flag in args else default


def main():
    if len(sys.argv) < 4:
        raise SystemExit(__doc__)
    cmd = sys.argv[1]
    ttf, size = sys.argv[2], int(sys.argv[3])
    rest = sys.argv[4:]
    hint = opt(rest, "--hint", "native")
    if cmd == "report":
        report(ttf, size, hint)
        return
    charset = opt(rest, "--charset", "ascii")
    if charset not in CHARSETS:
        raise SystemExit("unknown charset %r" % charset)
    if cmd == "emit":
        name = rest[0]
        emit(ttf, size, name, charset, opt(rest, "--out", "."), hint,
             opt(rest, "--asset-out", None))
    elif cmd == "specimen":
        specimen(ttf, size, rest[0], rest[1], charset, hint,
                 opt(rest, "--scale", 1, int))
    else:
        raise SystemExit(__doc__)


if __name__ == "__main__":
    main()
