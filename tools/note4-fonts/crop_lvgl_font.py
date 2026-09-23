#!/usr/bin/env python3
"""Analyse and crop LVGL `fmt_txt` bitmap fonts (the xiaozhi/Noto component).

The xiaozhi component ships generated `font_*.c` files in LVGL's binary font
format: a `glyph_dsc[]` table (bitmap_index / adv_w / box_w / box_h / ofs_x /
ofs_y), a `cmaps[]` table mapping codepoints to glyph ids, and one packed
`glyph_bitmap[]` blob (1, 2 or 4 bpp).  The Codex Status firmware engine stores
its fonts as fixed-cell 1bpp tables, so reusing these fonts needs a crop:
pick the codepoints we actually draw (ASCII), threshold 4bpp alpha to 1bpp, and
emit one descriptor per glyph with a byte offset into a shared blob.

Usage:
  python crop_lvgl_font.py report  <font.c> [<font.c> ...]
  python crop_lvgl_font.py emit    <font.c> <size-name> [--out <dir>] [--threshold 8]
  python crop_lvgl_font.py specimen <font.c> <text> <out.png> [--threshold 8] [--scale 1]
"""

import os
import re
import struct
import sys
import zlib

ASCII = list(range(0x20, 0x7F))

DSC_RE = re.compile(
    r"\{\.bitmap_index = (\d+), \.adv_w = (\d+), \.box_w = (\d+), "
    r"\.box_h = (\d+), \.ofs_x = (-?\d+), \.ofs_y = (-?\d+)\}"
)
CMAP_RE = re.compile(
    r"\.range_start = (\d+), \.range_length = (\d+), \.glyph_id_start = (\d+),"
    r".*?\.type = LV_FONT_FMT_TXT_CMAP_(\w+)",
    re.S,
)


class Font:
    def __init__(self, path):
        self.path = path
        src = open(path, encoding="utf-8", errors="replace").read()
        self.line_height = int(re.search(r"\.line_height = (\d+)", src).group(1))
        self.base_line = int(re.search(r"\.base_line = (\d+)", src).group(1))
        self.bpp = int(re.search(r"^\s*\*\s*Bpp:\s*(\d+)", src, re.M).group(1))
        self.size = int(re.search(r"^\s*\*\s*Size:\s*(\d+)\s*px", src, re.M).group(1))
        self.glyphs = [tuple(int(v) for v in m) for m in DSC_RE.findall(src)]
        body = src.split("glyph_bitmap[] = {", 1)[1].split("};", 1)[0]
        # The generator writes short values as `0x0`, so accept one or two digits.
        self.bitmap = bytes(int(b, 16) for b in re.findall(r"0x([0-9a-fA-F]{1,2})", body))
        self.cmaps = []
        for start, length, gid, kind in CMAP_RE.findall(src):
            self.cmaps.append((int(start), int(length), int(gid), kind))
        self.dense = [c for c in self.cmaps if c[3] == "FORMAT0_TINY"]

    def glyph_id(self, cp):
        for start, length, gid, _kind in self.dense:
            if start <= cp < start + length:
                return gid + (cp - start)
        return None

    def dsc(self, gid):
        bitmap_index, adv_w, box_w, box_h, ofs_x, ofs_y = self.glyphs[gid]
        return dict(bitmap_index=bitmap_index, adv_w=adv_w, box_w=box_w,
                    box_h=box_h, ofs_x=ofs_x, ofs_y=ofs_y)

    def rows(self, gid):
        """Return box_h rows of box_w alpha values (0..15 or 0..1).

        LVGL's converter packs each glyph's bitmap continuously (no per-row byte
        padding), so the byte length of a glyph is ceil(box_w * box_h * bpp / 8).
        """
        d = self.dsc(gid)
        w, h = d["box_w"], d["box_h"]
        if w == 0 or h == 0:
            return []
        off = d["bitmap_index"]
        total = (w * h * self.bpp + 7) // 8
        data = self.bitmap[off:off + total]
        if self.bpp == 4:
            vals = []
            for b in data:
                vals += [b >> 4, b & 0x0F]
        elif self.bpp == 2:
            vals = []
            for b in data:
                vals += [(b >> 6) & 3, (b >> 4) & 3, (b >> 2) & 3, b & 3]
        elif self.bpp == 1:
            vals = [(b >> (7 - (x % 8))) & 1
                    for b in data for x in range(8)]
        else:
            raise SystemExit("bpp %d unsupported" % self.bpp)
        vals = vals[:w * h]
        return [vals[y * w:(y + 1) * w] for y in range(h)]

    def glyph_bytes(self, gid):
        d = self.dsc(gid)
        return (d["box_w"] * d["box_h"] * self.bpp + 7) // 8

    def check(self):
        """Verify the packed-length model against every consecutive descriptor."""
        bad = []
        for i in range(1, len(self.glyphs) - 1):
            _idx, _adv, w, h, _ox, _oy = self.glyphs[i]
            if w == 0 or h == 0:
                continue
            delta = self.glyphs[i + 1][0] - self.glyphs[i][0]
            want = (w * h * self.bpp + 7) // 8
            if delta != want:
                bad.append((i, w, h, delta, want))
        return bad

    def pack_1bpp(self, gid, threshold):
        rows = self.rows(gid)
        d = self.dsc(gid)
        w, h = d["box_w"], d["box_h"]
        stride = (w + 7) // 8
        blob = bytearray(stride * h)
        # A 1 bpp source only has alpha 0/1, so any non-zero value is ink.
        thr = 1 if self.bpp == 1 else threshold
        for y, row in enumerate(rows):
            for x, a in enumerate(row):
                if a >= thr:
                    blob[y * stride + x // 8] |= 0x80 >> (x % 8)
        return bytes(blob), stride


def report(paths):
    print("%-34s %5s %4s %6s %9s %11s %11s %9s" % (
        "font", "size", "bpp", "glyphs", "bitmap B", "ASCII blob", "ASCII 1bpp", "model"))
    for p in paths:
        f = Font(p)
        ascii_ids = [g for g in (f.glyph_id(cp) for cp in ASCII) if g is not None]
        src_bytes = sum(f.glyph_bytes(g) for g in ascii_ids)
        cropped = desc = 0
        for g in ascii_ids:
            cropped += len(f.pack_1bpp(g, 8)[0])
            desc += 6
        print("%-34s %5d %4d %6d %9d %11d %9d (+%d desc) %s" % (
            os.path.basename(p), f.size, f.bpp, len(f.glyphs), len(f.bitmap),
            src_bytes, cropped, desc, "ok" if not f.check() else "MISMATCH"))


def emit(path, name, out_dir, threshold):
    """Write an engine-ready header: 1bpp blob + per-glyph metrics for ASCII."""
    f = Font(path)
    blob = bytearray()
    rows = []
    for cp in ASCII:
        gid = f.glyph_id(cp)
        if gid is None:
            rows.append((0, 0, 0, 0, 0, 0))
            continue
        packed, _stride = f.pack_1bpp(gid, threshold)
        d = f.dsc(gid)
        rows.append((len(blob), d["adv_w"], d["box_w"], d["box_h"],
                     d["ofs_x"], d["ofs_y"]))
        blob += packed
    os.makedirs(out_dir, exist_ok=True)
    upper = name.upper()
    header = os.path.join(out_dir, "font_noto_%s.h" % name)
    with open(header, "w", encoding="utf-8") as fh:
        fh.write("// Generated by tools/note4-fonts/crop_lvgl_font.py from %s\n"
                 % os.path.basename(path))
        fh.write("// LVGL %d px / %d bpp -> ASCII 0x20-0x7E, 1 bpp, threshold %d\n"
                 % (f.size, f.bpp, 1 if f.bpp == 1 else threshold))
        fh.write("// Note4Glyph: {blob offset, advance (1/16 px), box_w, box_h, ofs_x, ofs_y}\n")
        fh.write("#pragma once\n#include <stdint.h>\n\n")
        fh.write("#define FONT_%s_LINE_HEIGHT %d\n" % (upper, f.line_height))
        fh.write("#define FONT_%s_BASE_LINE %d\n" % (upper, f.base_line))
        fh.write("#define FONT_%s_BLOB_BYTES %d\n" % (upper, len(blob)))
        # Widest advance in 1/16 px: a conservative per-character cell width for
        # callers that only need an upper bound (refresh-region derivation).
        fh.write("#define FONT_%s_MAX_ADV %d\n\n" % (upper, max(r[1] for r in rows)))
        fh.write("static const uint8_t font_%s_blob[%d] = {\n" % (name, len(blob)))
        for i in range(0, len(blob), 16):
            fh.write("    " + " ".join("0x%02X," % b for b in blob[i:i + 16]) + "\n")
        fh.write("};\n\n")
        fh.write("static const Note4Glyph font_%s_glyphs[%d] = {\n" % (name, len(ASCII)))
        for cp, r in zip(ASCII, rows):
            fh.write("    {%5d, %4d, %3d, %3d, %4d, %4d}, /* 0x%02X %s */\n"
                     % (r[0], r[1], r[2], r[3], r[4], r[5], cp,
                        repr(chr(cp))[1:-1] if cp != 0x20 else "space"))
        fh.write("};\n")
    print("wrote %s (%d glyphs, blob %d bytes)" % (header, len(ASCII), len(blob)))


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


def specimen(path, text, out_png, threshold, scale):
    f = Font(path)
    line = f.line_height
    pen = 0
    placed = []
    for ch in text:
        gid = f.glyph_id(ord(ch))
        if gid is None:
            continue
        d = f.dsc(gid)
        rows = f.rows(gid)
        baseline = line - f.base_line
        x = pen + d["ofs_x"]
        y = baseline - d["ofs_y"] - d["box_h"]
        placed.append((x, y, rows))
        pen += d["adv_w"] / 16.0
    width = int(pen) + 4
    height = line + 4
    buf = [[255] * width for _ in range(height)]
    thr = 1 if f.bpp == 1 else threshold
    for x0, y0, rows in placed:
        for y, row in enumerate(rows):
            for x, a in enumerate(row):
                px, py = int(x0) + x, int(y0) + y
                if 0 <= px < width and 0 <= py < height and a >= thr:
                    buf[py][px] = 0
    if scale > 1:
        buf = [[v for v in row for _ in range(scale)] for row in buf for _ in range(scale)]
        width *= scale
        height *= scale
    write_png(out_png, width, height, buf)
    print("wrote %s (%dx%d) from %s" % (out_png, width, height, os.path.basename(path)))


def main():
    if len(sys.argv) < 3:
        raise SystemExit(__doc__)
    cmd = sys.argv[1]
    if cmd == "report":
        report(sys.argv[2:])
        return
    path = sys.argv[2]
    rest = sys.argv[3:]
    if cmd == "emit":
        name = rest[0]
        out_dir = rest[rest.index("--out") + 1] if "--out" in rest else "."
        threshold = int(rest[rest.index("--threshold") + 1]) if "--threshold" in rest else 8
        emit(path, name, out_dir, threshold)
    elif cmd == "specimen":
        text, out_png = rest[0], rest[1]
        threshold = int(rest[rest.index("--threshold") + 1]) if "--threshold" in rest else 8
        scale = int(rest[rest.index("--scale") + 1]) if "--scale" in rest else 1
        specimen(path, text, out_png, threshold, scale)
    else:
        raise SystemExit(__doc__)


if __name__ == "__main__":
    main()
