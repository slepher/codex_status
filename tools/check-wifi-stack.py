#!/usr/bin/env python3
"""Gate the real Xtensa startup frames that overflowed loopTask after migration."""
import argparse
from pathlib import Path
import re
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("elf", type=Path)
parser.add_argument("--objdump", type=Path, default=Path(__file__).resolve().parents[1] / ".pio-pkgs/note4/toolchain-xtensa-esp-elf/bin/xtensa-esp32s3-elf-objdump")
args = parser.parse_args()
assembly = subprocess.check_output([str(args.objdump), "-d", "-C", str(args.elf)], text=True)
failed = False
for name in ("wifiSchemesBegin()", "readWifiStore(Preferences&, WifiStoreV2&)"):
    match = re.search(r"^[0-9a-f]+ <" + re.escape(name) + r">:\n[^\n]*\bentry\s+a1,\s*(0x[0-9a-f]+|[0-9]+)", assembly, re.MULTILINE)
    if not match:
        print(f"FAIL: missing Xtensa frame for {name}")
        failed = True
        continue
    size = int(match.group(1), 0)
    okay = size <= 1024
    print(f"{'PASS' if okay else 'FAIL'}: {name}: {size} B (limit 1024 B; loopTask 8192 B)")
    failed |= not okay
raise SystemExit(1 if failed else 0)
