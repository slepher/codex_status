# Task 5 — Refresh and anti-ghosting (M5)

Bridge only decides whether to send data; the device decides whether to refresh
from real old/new framebuffers.

Implemented in `refresh_policy` + `epdFlush` and extended here:

- full candidate framebuffer vs last successfully displayed real framebuffer;
- zero refresh when pixels are identical (unless a maintenance clean full
  refresh is due — memcmp must not suppress it);
- dirty rects from actual changed pixels, expanded/cropped by the driver's X/Y
  alignment, edges filled from real old/new pixels (never white);
- overlapping windows merge; previous/new planes prepared correctly;
- previous framebuffer and budgets update only after BUSY success; BUSY failure
  marks the baseline untrusted; power loss/abnormal reset/layout change/CRC
  failure forces a safe full refresh;
- black tile + inverted digits: only the changed strokes and necessary aligned
  area, never the whole black background; dirty area is tracked separately from
  the stable semantic-region ghost risk;
- counters: partial count, cumulative changed area, black→white erasure, time
  since full refresh, temperature range, region polarity, baseline trust;
- clock refresh uses its own budget; clean full refresh never needs the network.

SSD1681 thresholds are hardware-verified; any new panel keeps the driver
interface and conservative defaults and stays `blocked_by_hardware_arrival`.
