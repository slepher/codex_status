#!/usr/bin/env python3
"""Measure ink geometry of a rendered template preview (pure stdlib, no PIL).

Usage:
  python measure-preview.py <png> [x0 x1 y0 y1] [--rows] [--bands x0,x1 ...]

Prints the black-pixel bounding box of the whole image (or of the given
rectangle) and, with --bands, the bounding box of each x-band so icon/text
alignment inside a status bar can be compared numerically.
"""

import os
import struct
import sys
import tempfile
import zlib


def read_png(path):
    data = open(path, "rb").read()
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise SystemExit("not a PNG: " + path)
    pos, idat, plte = 8, b"", None
    w = h = bd = ct = None
    while pos < len(data):
        (ln,) = struct.unpack(">I", data[pos:pos + 4])
        typ = data[pos + 4:pos + 8]
        chunk = data[pos + 8:pos + 8 + ln]
        pos += 12 + ln
        if typ == b"IHDR":
            w, h, bd, ct, _comp, _filt, inter = struct.unpack(">IIBBBBB", chunk)
            if inter:
                raise SystemExit("interlaced PNG unsupported")
        elif typ == b"IDAT":
            idat += chunk
        elif typ == b"PLTE":
            plte = chunk
        elif typ == b"IEND":
            break
    raw = zlib.decompress(idat)
    channels = {0: 1, 2: 3, 3: 1, 4: 2, 6: 4}[ct]
    if bd == 8:
        stride = w * channels
        fbpp = channels
    elif bd == 1:
        stride = (w + 7) // 8
        fbpp = 1
    else:
        raise SystemExit("bit depth %d unsupported" % bd)
    rows, prev, off = [], bytearray(stride), 0
    for _y in range(h):
        ft = raw[off]
        line = bytearray(raw[off + 1:off + 1 + stride])
        off += 1 + stride
        if ft == 1:
            for i in range(fbpp, stride):
                line[i] = (line[i] + line[i - fbpp]) & 0xFF
        elif ft == 2:
            for i in range(stride):
                line[i] = (line[i] + prev[i]) & 0xFF
        elif ft == 3:
            for i in range(stride):
                left = line[i - fbpp] if i >= fbpp else 0
                line[i] = (line[i] + ((left + prev[i]) >> 1)) & 0xFF
        elif ft == 4:
            for i in range(stride):
                a = line[i - fbpp] if i >= fbpp else 0
                b = prev[i]
                c = prev[i - fbpp] if i >= fbpp else 0
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                pr = a if (pa <= pb and pa <= pc) else (b if pb <= pc else c)
                line[i] = (line[i] + pr) & 0xFF
        elif ft != 0:
            raise SystemExit("filter %d unsupported" % ft)
        rows.append(bytes(line))
        prev = line
    return w, h, bd, ct, plte, rows


def to_bits(path):
    w, h, bd, ct, plte, rows = read_png(path)
    bits = []
    for line in rows:
        row = []
        if bd == 1:
            for x in range(w):
                byte = line[x >> 3]
                row.append(1 if not ((byte >> (7 - (x & 7))) & 1) else 0)  # sample 0 = black = ink
        else:
            for x in range(w):
                if ct == 3:
                    idx = line[x]
                    r, g, b = plte[idx * 3:idx * 3 + 3]
                elif ct in (0, 4):
                    r = g = b = line[x * (2 if ct == 4 else 1)]
                else:
                    o = x * (4 if ct == 6 else 3)
                    r, g, b = line[o], line[o + 1], line[o + 2]
                row.append(1 if (r + g + b) / 3 < 128 else 0)
        bits.append(row)
    return w, h, bits


def bbox(bits, x0, x1, y0, y1):
    xs, ys, n = [], [], 0
    for y in range(y0, min(y1, len(bits))):
        row = bits[y]
        for x in range(x0, min(x1, len(row))):
            if row[x]:
                xs.append(x)
                ys.append(y)
                n += 1
    if not n:
        return None
    return min(xs), min(ys), max(xs), max(ys), n


def selftest():
    """Encode a known 1bpp PNG in memory and confirm the decoder round-trips it."""
    w, h = 8, 4
    # PNG 1bpp grayscale samples: 0 = black (ink), 1 = white.
    rows = [
        [0, 0, 0, 0, 0, 0, 0, 0],
        [1, 1, 1, 1, 1, 1, 1, 1],
        [1, 1, 1, 0, 1, 1, 1, 1],  # black at x=3
        [1, 1, 1, 1, 1, 1, 1, 0],  # black at x=7
    ]

    def chunk(tag, payload):
        return (struct.pack(">I", len(payload)) + tag + payload
                + struct.pack(">I", zlib.crc32(tag + payload) & 0xFFFFFFFF))

    raw = b"".join(b"\x00" + bytes([sum((1 << (7 - i)) if s else 0 for i, s in enumerate(r))]) for r in rows)
    png = (b"\x89PNG\r\n\x1a\n"
           + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 1, 0, 0, 0, 0))
           + chunk(b"IDAT", zlib.compress(raw))
           + chunk(b"IEND", b""))
    tmp = os.path.join(tempfile.gettempdir(), "_measure_selftest.png")
    with open(tmp, "wb") as fh:
        fh.write(png)
    try:
        ww, hh, bits = to_bits(tmp)
        ink = {(x, y) for y in range(hh) for x in range(ww) if bits[y][x]}
        want = {(x, 0) for x in range(w)} | {(3, 2), (7, 3)}
        ok = (ww, hh) == (w, h) and ink == want
        print("selftest: %s (decoded ink %s)" % ("PASS" if ok else "FAIL", sorted(ink)))
        return 0 if ok else 1
    finally:
        os.remove(tmp)


def main():
    if "--selftest" in sys.argv[1:]:
        raise SystemExit(selftest())
    args = sys.argv[1:]
    if not args:
        raise SystemExit(__doc__)
    path = args[0]
    rest, bands, art, rowspec = [], [], None, None
    i = 1
    while i < len(args):
        a = args[i]
        if a == "--bands":
            i += 1
            while i < len(args) and not args[i].startswith("--"):
                lo, hi = args[i].split(",")
                bands.append((int(lo), int(hi)))
                i += 1
        elif a == "--art":
            art = tuple(int(v) for v in args[i + 1].split(","))
            i += 2
        elif a == "--rows":
            rowspec = tuple(int(v) for v in args[i + 1].split(","))
            i += 2
        else:
            rest.append(int(a))
            i += 1
    rect = rest[:4] or None
    w, h, bits = to_bits(path)
    print("%s: %dx%d" % (path, w, h))
    if rect:
        x0, x1, y0, y1 = rect
    else:
        x0, x1, y0, y1 = 0, w, 0, h
    whole = bbox(bits, x0, x1, y0, y1)
    print("region x[%d,%d) y[%d,%d) ink bbox: %s" % (x0, x1, y0, y1, whole))
    if whole:
        print("  ink center: x=%.1f y=%.1f" % ((whole[0] + whole[2]) / 2, (whole[1] + whole[3]) / 2))
    for a, b in bands:
        r = bbox(bits, a, b, y0, y1)
        if not r:
            print("  band x[%d,%d): empty" % (a, b))
        else:
            print("  band x[%3d,%3d): bbox x[%d,%d] y[%d,%d] h=%d w=%d px=%d center y=%.1f"
                  % (a, b, r[0], r[2], r[1], r[3], r[3] - r[1] + 1, r[2] - r[0] + 1, r[4],
                     (r[1] + r[3]) / 2))
    if art:
        a, b, c, d = art
        print("art x[%d,%d) y[%d,%d) (#=ink):" % (a, b, c, d))
        for y in range(c, min(d, h)):
            print("  y=%3d |%s|" % (y, "".join("#" if bits[y][x] else "." for x in range(a, min(b, w)))))
    if rowspec:
        a, b = rowspec
        print("row profile for x[%d,%d):" % (a, b))
        for y in range(y0, min(y1, h)):
            n = sum(bits[y][a:b])
            if n:
                print("  y=%3d ink=%d" % (y, n))


if __name__ == "__main__":
    main()
