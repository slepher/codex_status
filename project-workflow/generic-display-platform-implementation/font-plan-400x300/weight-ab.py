#!/usr/bin/env python3
"""Weight A/B strip for the 400x300 font plan.

Renders the real strings at their real pixel sizes with FreeType's monochrome
rasterizer -- exactly what the engine receives -- so the weight choice can be
made on 1:1 pixels instead of a smoothed preview:

  rows 1-4  usage figures  "94  25  100%"              at 30 px
            (Thin 100 / ExtraLight 200 / Light 300 / Regular 400)
  rows 5-6  body strings   "Remaining 5H WEEK RESET
                            SYNC 23:59 CODEX"          at 16 px
            (Light 300 / Regular 400)

Every row prints the widest stem and the ink pixel count of its sample, so
"which weight survives 1bpp at this size" is a number, not an impression.

Usage:
  python weight-ab.py <out.png> [--zoom N]
"""
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
VENDOR = os.path.abspath(os.path.join(HERE, "..", "..", "..", "tools", "note4-fonts", "vendor"))
sys.path.insert(0, os.path.dirname(VENDOR))
import rasterize_ttf as rt  # noqa: E402

USAGE_TEXT = "94  25  100%"
BODY_TEXT = "Remaining 5H WEEK RESET SYNC 23:59 CODEX"

ROWS = [
    ("Thin 100 @30", "NotoSans-Thin.ttf", 30, USAGE_TEXT),
    ("ExtraLight 200 @30", "NotoSans-ExtraLight.ttf", 30, USAGE_TEXT),
    ("Light 300 @30", "NotoSans-Light.ttf", 30, USAGE_TEXT),
    ("Regular 400 @30", "NotoSans-Regular.ttf", 30, USAGE_TEXT),
    (None, None, None, None),
    ("Light 300 @16", "NotoSans-Light.ttf", 16, BODY_TEXT),
    ("Regular 400 @16", "NotoSans-Regular.ttf", 16, BODY_TEXT),
]


def stem(rows):
    """Longest horizontal ink run in a glyph: its vertical stroke width."""
    best = 0
    for row in rows:
        run = 0
        for v in row:
            run = run + 1 if v else 0
            best = max(best, run)
    return best


def median_stem(f, ch):
    """Median over rows of the longest run: the stroke width of a curved glyph.

    The plain maximum belongs to horizontal bars (the crossbar of '4', the waist
    of '%'), so digits with curves are measured row by row instead.
    """
    rows, _w, _h, _ox, _oy, _adv = f.glyph(ord(ch))
    runs = []
    for row in rows:
        run = best = 0
        for v in row:
            run = run + 1 if v else 0
            best = max(best, run)
        if best:
            runs.append(best)
    return sorted(runs)[len(runs) // 2] if runs else 0


def ink(rows):
    return sum(sum(r) for r in rows)


def draw(canvas, x, y, rows):
    h, w = len(canvas), len(canvas[0])
    for dy, row in enumerate(rows):
        for dx, v in enumerate(row):
            if v and 0 <= y + dy < h and 0 <= x + dx < w:
                canvas[y + dy][x + dx] = 0


def main():
    out = sys.argv[1]
    zoom = rt.opt(sys.argv[2:], "--zoom", 1, int)
    label_face = rt.Face(os.path.join(VENDOR, "NotoSans-Light.ttf"), 12, "native")

    pad, label_w, width = 6, 118, 560
    blocks = []
    y = pad
    for title, ttf, size, text in ROWS:
        if title is None:
            blocks.append((None, None, y, 10, None))
            y += 10 + pad
            continue
        f = rt.Face(os.path.join(VENDOR, ttf), size, "native")
        _tw, th, _placed = rt.layout(f, text)
        blocks.append((title, f, y, th, text))
        y += th + pad
    height = y + pad

    canvas = [[255] * width for _ in range(height)]
    print("%-20s %-9s %-9s %-9s %-9s %s"
          % ("face @ size", "stem '1'", "stem '8'", "stem 'H'", "ink", "sample"))
    for title, f, y, th, text in blocks:
        if title is None:
            continue
        _lw, lh, lplaced = rt.layout(label_face, title)
        for gx, gy, grows in lplaced:
            draw(canvas, pad + gx, y + max(0, (th - lh) // 2) + gy, grows)
        _tw, _th, placed = rt.layout(f, text)
        total = sum(ink(grows) for _gx, _gy, grows in placed)
        for gx, gy, grows in placed:
            draw(canvas, label_w + gx, y + gy, grows)
        one = f.glyph(ord("1"))[0]
        eight = f.glyph(ord("8"))[0]
        cap = f.glyph(ord("H"))[0]
        print("%-20s %-9s %-9s %-9s %-9s %s"
              % (title,
                 "%d px" % max((max((r for r in row), default=0) for row in one), default=0),
                 "%d px" % median_stem(f, "8"),
                 "%d px" % median_stem(f, "H"),
                 "%d px" % total, repr(text)[1:-1]))

    if zoom > 1:
        canvas = [[v for v in row for _ in range(zoom)] for row in canvas for _ in range(zoom)]
        width *= zoom
        height *= zoom
    rt.write_png(out, width, height, canvas)
    print("wrote %s (%dx%d)" % (out, width, height))


if __name__ == "__main__":
    main()
