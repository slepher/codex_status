#!/usr/bin/env python3
"""Compose the font-plan evidence images.

Produces, from the two rendered pages:
  * a 2-up full-page comparison (LVGL 4bpp crop | FreeType monochrome), and
  * a 2x zoom of the usage block and the status bar, where the weight change is
    actually visible.

Both are pure 1bpp output from the shared firmware engine, so they are the same
pixels the panel receives -- no viewer interpolation is involved at 1x.

Usage:
  python compose-fontplan.py <baseline.png> <candidate.png> <out-prefix>
"""
import importlib.util
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
VENDOR = os.path.abspath(os.path.join(HERE, "..", "..", "..", "tools", "note4-fonts", "vendor"))
sys.path.insert(0, os.path.dirname(VENDOR))
import rasterize_ttf as rt  # noqa: E402


def load(path):
    spec = importlib.util.spec_from_file_location(
        "measure_preview",
        os.path.join(HERE, "..", "concepts-400x300", "measure-preview.py"))
    measure = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(measure)
    w, h, bits = measure.to_bits(path)
    return w, h, bits


def blit(canvas, x, y, bits, zoom=1):
    for dy, row in enumerate(bits):
        for dx, v in enumerate(row):
            if not v:
                continue
            for zy in range(zoom):
                for zx in range(zoom):
                    px, py = x + dx * zoom + zx, y + dy * zoom + zy
                    if 0 <= py < len(canvas) and 0 <= px < len(canvas[0]):
                        canvas[py][px] = 0


def crop(bits, x0, y0, x1, y1):
    return [row[x0:x1] for row in bits[y0:y1]]


def main():
    base_path, cand_path, prefix = sys.argv[1], sys.argv[2], sys.argv[3]
    # Labels default to the input file names so the image can never claim a font
    # cut that is not the one it was rendered from.
    label_a = sys.argv[4] if len(sys.argv) > 4 else os.path.basename(base_path)
    label_b = sys.argv[5] if len(sys.argv) > 5 else os.path.basename(cand_path)
    bw, bh, base = load(base_path)
    cw, ch, cand = load(cand_path)
    assert (bw, bh) == (cw, ch) == (400, 300)

    label = rt.Face(os.path.join(VENDOR, "NotoSans-Light.ttf"), 14, "native")

    def draw_label(canvas, x, y, text):
        for gx, gy, rows in rt.layout(label, text)[2]:
            blit(canvas, x + gx, y + gy, rows)

    # 1) full-page 2-up with a separating rule
    gap = 24
    width, height = 400 * 2 + gap, 300 + 30
    canvas = [[255] * width for _ in range(height)]
    blit(canvas, 0, 26, base)
    blit(canvas, 400 + gap, 26, cand)
    for y in range(height):
        canvas[y][400 + gap // 2] = 0
    draw_label(canvas, 0, 4, "A: " + label_a)
    draw_label(canvas, 400 + gap, 4, "B: " + label_b)
    out = "%s-2up.png" % prefix
    rt.write_png(out, width, height, canvas)
    print("wrote %s (%dx%d)" % (out, width, height))

    # 2) zoom: usage block + status bar, 2x
    zoom = 2
    regions = [("usage 5h", 0, 100, 200, 170), ("status bar", 200, 0, 400, 36)]
    rows = []
    for title, x0, y0, x1, y1 in regions:
        rows.append((title, x0, y0, x1, y1))
    width = 400 * 2 + gap
    height = 0
    for _t, x0, y0, x1, y1 in rows:
        height += (y1 - y0) * zoom + 26
    canvas = [[255] * width for _ in range(height)]
    y = 0
    for title, x0, y0, x1, y1 in rows:
        draw_label(canvas, 0, y + 4, "A " + title)
        draw_label(canvas, 400 + gap, y + 4, "B " + title)
        blit(canvas, 0, y + 22, crop(base, x0, y0, x1, y1), zoom)
        blit(canvas, 400 + gap, y + 22, crop(cand, x0, y0, x1, y1), zoom)
        y += (y1 - y0) * zoom + 26
    for yy in range(height):
        canvas[yy][400 + gap // 2] = 0
    out = "%s-zoom.png" % prefix
    rt.write_png(out, width, height, canvas)
    print("wrote %s (%dx%d)" % (out, width, height))


if __name__ == "__main__":
    main()
