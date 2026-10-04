#!/usr/bin/env python3
"""Gate real Xtensa Wi-Fi startup and transaction frames, including local chains."""
import argparse
from pathlib import Path
import re
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("elf", type=Path)
parser.add_argument("--objdump", type=Path, default=Path(__file__).resolve().parents[1] / ".pio-pkgs/note4/toolchain-xtensa-esp-elf/bin/xtensa-esp32s3-elf-objdump")
args = parser.parse_args()
assembly = subprocess.check_output([str(args.objdump), "-d", "-C", str(args.elf)], text=True)
functions = {}
for block in re.split(r"(?=^[0-9a-f]+ <[^\n]+>:\n)", assembly, flags=re.MULTILINE):
    symbol = re.match(r"^[0-9a-f]+ <([^\n]+)>:\n", block)
    if not symbol:
        continue
    frame = re.search(r"\bentry\s+a1,\s*(0x[0-9a-f]+|[0-9]+)", block[:250])
    if frame:
        functions[symbol[1]] = (int(frame[1], 0), set(re.findall(r"\bcall\w*\s+[0-9a-f]+ <([^\n]+)>", block)))

# Old ELF support keeps the .50 overflow regression runnable. v3 may inline
# codec/legacy conversion helpers; their frames are then included in read.
reader = next((name for name in functions if name.startswith("readWifiStore(Preferences&, WifiStore")), None)
required = ["wifiSchemesBegin()", reader]
if reader and "WifiStoreV2" not in reader:
    for prefix in ("saveV2(", "wifiSetupPoll(", "wifiSetupCommand(", "wifiManagementCommand(", "wifiControlCommand(", "wifiManagementWithContext(", "wifiDeviceStatus(", "handleWifiManagement(", "servicePresence(", "storeUpsert(", "storeAuthenticate("):
        required.append(next((name for name in functions if name.startswith(prefix)), None))
failed = False
for name in required:
    if name is None:
        print("FAIL: missing required Wi-Fi Xtensa frame")
        failed = True
        continue
    size = functions[name][0]
    okay = size <= 1024
    print(f"{'PASS' if okay else 'FAIL'}: {name}: {size} B (limit 1024 B; loopTask 8192 B)")
    failed |= not okay

# Scope is explicit: Wi-Fi module + Preferences/NVS frames present in this ELF.
# Indirect platform/library calls and ISR stacks remain outside this subtotal.
def local(name):
    return name.startswith(("wifi", "Wifi", "readWifiStore(", "saveV2(", "save(Wifi", "Preferences::", "nvs_", "nvs::", "store", "load(Endpoints", "save(Endpoints", "wifiDeviceStatus(", "handleWifiManagement(", "servicePresence("))

def chain(name, seen=frozenset()):
    if name in seen or name not in functions or not local(name):
        return 0, []
    own, calls = functions[name]
    children = [chain(child, seen | {name}) for child in calls]
    cost, path = max(children, default=(0, []), key=lambda item: item[0])
    return own + cost, [name] + path

for name in required:
    if name is None:
        continue
    size, path = chain(name)
    okay = size <= 4096
    print(f"{'PASS' if okay else 'FAIL'}: local chain {size} B (limit 4096 B; 4 KiB reserved for callers/platform): " + " -> ".join(path))
    failed |= not okay
raise SystemExit(1 if failed else 0)
