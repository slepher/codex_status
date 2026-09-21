#!/usr/bin/env python3
"""RTC/heap capacity audit for the ESP32-S3 firmware map.

Parses .pio/build/<env>/firmware.map and reports the RTC slow-memory region
size and the bytes used by .rtc.data/.rtc.bss (RTC_DATA_ATTR), plus the
largest contributors. Used by the ble-rendezvous-power stage-1 audit; re-run
after builds that add RTC state (task-4..task-7).

Usage: python tools/rtc-budget.py [map path]
"""

import re
import sys
from pathlib import Path

DEFAULT_MAP = Path(".pio/build/esp32-s3-epaper-154g/firmware.map")
# S3 RTC slow memory is 8 KiB; the linker reserves the low 0x200.
TOTAL_RTC_SLOW = 0x2000
RESERVED = 0x200


def main() -> int:
    map_path = Path(sys.argv[1]) if len(sys.argv) > 1 else DEFAULT_MAP
    if not map_path.exists():
        print(f"map not found: {map_path} (run pio run first)")
        return 1
    region = None
    start = end = 0
    entries = []
    section_re = re.compile(r"^\s+(\.rtc\.(data|bss)\S*)\s+0x([0-9a-fA-F]+)\s+0x([0-9a-fA-F]+)(?:\s+(.+))?$")
    region_re = re.compile(r"^(rtc_slow_seg)\s+0x([0-9a-fA-F]+)\s+0x([0-9a-fA-F]+)\s+rw")
    for line in map_path.read_text(errors="replace").splitlines():
        m = region_re.match(line)
        if m:
            region = (int(m.group(2), 16), int(m.group(3), 16))
            continue
        m = section_re.match(line)
        if not m:
            continue
        name, addr, size, where = m.group(1), int(m.group(3), 16), int(m.group(4), 16), m.group(5) or ""
        entries.append((name, addr, size, where.strip()))

    if not entries:
        print("no .rtc.data/.rtc.bss entries found")
        return 1
    lo = min(e[1] for e in entries)
    hi = max(e[1] + e[2] for e in entries)
    used = sum(e[2] for e in entries)
    base = region[0] if region else lo
    size = region[1] if region else TOTAL_RTC_SLOW
    free = base + size - hi
    print(f"RTC slow region: start=0x{base:08x} size={size} ({size} B)")
    print(f".rtc.data/.rtc.bss used: {used} B over address range 0x{lo:08x}..0x{hi:08x}")
    print(f"headroom: {free} B ({free / 1024:.2f} KiB)")
    print("largest entries:")
    for name, addr, sz, where in sorted(entries, key=lambda e: -e[2])[:12]:
        print(f"  {sz:6d} B  {name:24s} 0x{addr:08x}  {where[-60:]}")
    print("fit check: 5000 B full old-frame ->",
          "FITS" if free >= 5000 else f"NO (need 5000, have {free})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
