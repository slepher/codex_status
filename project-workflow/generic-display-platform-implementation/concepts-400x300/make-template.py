"""Build the chosen 400x300 layout with four 20px status pictograms.

Font plan (user decision, sizes provisional until the engine can deliver fonts
without a firmware rebuild):

  * normal text -> ``ntthin18``: Noto Sans Thin 100 @18 px, full ASCII.
  * large text  -> ``ntreg64``: Noto Sans Regular 400 @64 px, tabular digits.

Both are rasterized from the static hinted Noto TTFs with FreeType's monochrome
renderer (``tools/note4-fonts/rasterize_ttf.py``), so grid-fitting runs on the
outline instead of thresholding an already antialiased 4bpp crop.

Coordinates are derived from the faces' own line metrics rather than measured by
hand yet -- layout polish is the follow-up step:

  ascent = lineHeight - baseLine   (ntthin18: 26-6 = 20, ntreg64: 88-19 = 69)
  ink top for a capital/digit      = y + ascent - box_h
                                   (ntthin18 caps: y+7, ntreg64 digits: y+22)

The large-text regions are sized to exactly one line height (88 px) so the
64 px digits are never clipped by the region, and their baselines stay clear of
the RESET block and the rule at y=221.

Re-measure after any change: ``measure-preview.py`` reads rendered PNGs pixel by
pixel, ``compare-fontplan.py`` diffs two cuts, and ``bridge-render --regions``
re-checks the region derivation.
"""
import json
from pathlib import Path
from status_icons import halves, icon_set

ROOT = Path(__file__).resolve().parent

SMALL = "ntthin18"
BIG = "ntreg64"

# ntthin18: keep a capital's ink top at `t` -> y = t - 7.
THIN_INK_BIAS = 7
# ntreg64: region y lands the digit ink at [y+22, y+69]; one line height tall.
BIG_LINE = 88
BIG_INK_BIAS = 22


def text(x, y, font, *, value=None, bind=None, prefix=None, suffix=None, region=None, align=None, scale=None, when=None, time_format=None):
    e = {"type": "text", "x": x, "y": y, "font": font, "color": "black"}
    for k, v in {"text": value, "bind": bind, "prefix": prefix, "suffix": suffix,
                 "region": region, "align": align, "scale": scale, "when": when,
                 "time_format": time_format}.items():
        if v is not None:
            e[k] = v
    return e


def big(x, region_w, bind, when):
    """Large-text element: region is exactly one 64 px line, top-aligned ink."""
    return text(x, 88, BIG, bind=bind, region=[x, 88, region_w, BIG_LINE],
                align="center", when=when)


def rule(x1, y1, x2, y2, when=None):
    e = {"type": "line", "x1": x1, "y1": y1, "x2": x2, "y2": y2, "color": "black"}
    if when is not None:
        e["when"] = when
    return e


present = {"bind": "buckets[codex].5h.remaining", "exists": True}
absent = {"bind": "buckets[codex].5h.remaining", "exists": False}
weekly = "buckets[codex].weekly.remaining"

elements = [
    text(12, 5, SMALL, bind="device.date"),
    text(75, 5, SMALL, bind="device.now"),
]
icons = icon_set()
for x, name in zip((248, 272, 296, 320), ("bluetooth", "wifi", "bridge-on", "battery-outline")):
    for offset, bits in zip((0, 10), halves(icons[name])):
        elements.append({"type": "icon", "x": x + offset, "y": 8, "w": 10, "h": 20,
                         "bits": bits, "color": "black"})
elements.append({"type": "bar", "bind": "device.battery", "rect": [324, 16, 14, 7],
                 "max": 100, "fg": "black", "bg": "none", "border": False})
elements.extend([
    text(344, 5, SMALL, bind="device.battery", suffix="%", region=[344, 5, 44, 20]),
    rule(12, 36, 388, 36),
    text(16, 44, SMALL, value="Remaining"),
    rule(199, 56, 199, 211, present),
    text(16, 66, SMALL, value="5H", when=present),
    big(16, 132, "buckets[codex].5h.remaining", present),
    text(132, 120, SMALL, value="%", when=present),
    text(18, 177, SMALL, value="RESET", when=present),
    text(18, 200, SMALL, bind="buckets[codex].5h.resetsAt",
         region=[18, 201, 170, 18], when=present, time_format="hhmm"),
    text(219, 66, SMALL, value="WEEK", when=present),
    big(217, 132, weekly, present),
    text(333, 120, SMALL, value="%", when=present),
    text(219, 177, SMALL, value="RESET", when=present),
    text(219, 200, SMALL, bind="buckets[codex].weekly.resetsAt",
         region=[219, 201, 168, 18], when=present),
    text(105, 66, SMALL, value="WEEK", when=absent),
    big(105, 132, weekly, absent),
    text(221, 120, SMALL, value="%", when=absent),
    text(105, 177, SMALL, value="RESET", when=absent),
    text(105, 200, SMALL, bind="buckets[codex].weekly.resetsAt",
         region=[105, 201, 168, 18], when=absent),
    rule(12, 221, 388, 221),
    text(16, 233, SMALL, bind="account.plan", region=[16, 232, 83, 20]),
    text(104, 233, SMALL, bind="bridge.label", region=[104, 232, 193, 20],
         when={"bind": "bridge.label", "exists": True}),
    text(326, 233, SMALL, bind="resetCredits.availableCount", prefix="RC ",
         region=[314, 232, 73, 20], align="right",
         when={"bind": "resetCredits.availableCount", "exists": True}),
    rule(12, 269, 388, 269),
    text(16, 270, SMALL, bind="device.sync_hhmm", prefix="SYNC ",
         when={"bind": "device.offline_mins", "exists": False}),
    text(16, 270, SMALL, bind="device.offline_mins", prefix="OFF ", suffix="M",
         when={"bind": "device.offline_mins", "exists": True}),
    text(333, 270, SMALL, value="CODEX"),
])

template = {"schema": 1, "id": "codex-status-a", "version": 1,
            "render_target": "epd-ssd2683-400x300-1bpp",
            "canvas": {"w": 400, "h": 300}, "elements": elements}
path = ROOT / "codex-status-a-400x300.json"
path.write_text(json.dumps(template, ensure_ascii=True, indent=2) + "\n", encoding="utf-8")
print(path)
