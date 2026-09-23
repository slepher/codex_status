#!/usr/bin/env python3
"""Compare the two 400x300 font cuts pixel by pixel.

Answers the two questions that decide whether the font plan is safe:

  1. does the ink of the usage figures and the '%' marks keep the baseline the
     approved layout was measured against, and
  2. exactly which pixels changed between the LVGL 4bpp crop and the FreeType
     monochrome rasterization.

Usage:
  python compare-fontplan.py <baseline.png> <candidate.png> [--json]
"""
import json
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)),
                                "..", "concepts-400x300"))
import importlib.util
_spec = importlib.util.spec_from_file_location(
    "measure_preview",
    os.path.join(os.path.dirname(os.path.abspath(__file__)), "..",
                 "concepts-400x300", "measure-preview.py"))
measure = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(measure)

# Regions of interest, from make-template.py: the three usage figures, the three
# '%' marks and the status-bar clock block.
ROI = {
    "usage-5h": (16, 147, 120, 160),
    "usage-week": (217, 348, 120, 160),
    "usage-week-pro": (105, 236, 120, 160),
    "percent-5h": (126, 146, 120, 160),
    "percent-week": (327, 347, 120, 160),
    "statusbar": (0, 200, 0, 36),
    "footer": (0, 399, 262, 299),
}


def ink_rows(bits, x0, x1, y0, y1):
    rows = [y for y in range(y0, y1 + 1)
            if any(bits[y][x] for x in range(x0, x1 + 1))]
    return (rows[0], rows[-1]) if rows else (None, None)


def main():
    if len(sys.argv) < 3:
        raise SystemExit(__doc__)
    bw, bh, base = measure.to_bits(sys.argv[1])
    cw, ch, cand = measure.to_bits(sys.argv[2])
    if (bw, bh) != (cw, ch):
        raise SystemExit("canvas mismatch: %dx%d vs %dx%d" % (bw, bh, cw, ch))
    out = {"regions": {}, "diff": {}}
    for name, (x0, x1, y0, y1) in ROI.items():
        b0, b1 = ink_rows(base, x0, x1, y0, y1)
        c0, c1 = ink_rows(cand, x0, x1, y0, y1)
        out["regions"][name] = {
            "baseline_rows": [b0, b1],
            "candidate_rows": [c0, c1],
            "baseline_centre": None if b0 is None else (b0 + b1) / 2,
            "candidate_centre": None if c0 is None else (c0 + c1) / 2,
        }
    diff = 0
    for y in range(300):
        for x in range(400):
            if base[y][x] != cand[y][x]:
                diff += 1
    out["diff"]["changed_pixels"] = diff
    out["diff"]["total_pixels"] = 400 * 300
    if "--json" in sys.argv:
        print(json.dumps(out, indent=2))
        return
    print("%-14s %-14s %-14s %s" % ("region", "baseline rows", "candidate rows",
                                    "centre shift"))
    for name, r in out["regions"].items():
        shift = (None if r["baseline_centre"] is None or r["candidate_centre"] is None
                 else r["candidate_centre"] - r["baseline_centre"])
        print("%-14s %-14s %-14s %s"
              % (name, r["baseline_rows"], r["candidate_rows"],
                 "n/a" if shift is None else "%+.1f px" % shift))
    print("\nchanged pixels: %d / %d (%.2f%%)"
          % (diff, 400 * 300, diff * 100.0 / (400 * 300)))


if __name__ == "__main__":
    main()
