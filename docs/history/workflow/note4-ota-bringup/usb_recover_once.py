"""Wait for the exact Note4 USB port and attempt one A-slot flash."""

import hashlib
import os
import subprocess
import sys
import time
from pathlib import Path

from serial.tools import list_ports


ROOT = Path(__file__).resolve().parents[2]
ROM = ROOT / "artifacts/codex-status-0.18.19-note4-a-usbfix.bin"
SHA256 = "1EBEF24CA3AC722C2F43D17E06F6858CDDCCC706372BF6996F2C8CCE607AE35D"
MAC = "7C:4F:AD:B9:34:08"


def main():
    if hashlib.sha256(ROM.read_bytes()).hexdigest().upper() != SHA256:
        raise SystemExit("A ROM hash mismatch; no flash attempted")
    deadline = time.monotonic() + 100
    while time.monotonic() < deadline:
        for port in list_ports.comports():
            if port.vid != 0x303A or port.pid != 0x1001 or port.serial_number != MAC:
                continue
            print(f"Exact Note4 USB {MAC} at {port.device}; one flash attempt", flush=True)
            env = os.environ.copy()
            env["PYTHONIOENCODING"] = "utf-8"
            result = subprocess.run([
                sys.executable, "-m", "esptool", "--chip", "esp32s3",
                "--port", port.device, "--baud", "460800",
                "--before", "default-reset", "--after", "no-reset",
                "write-flash", "0x20000", str(ROM),
            ], check=False, env=env)
            raise SystemExit(result.returncode)
        time.sleep(0.1)
    raise SystemExit("Note4 USB did not enumerate within 100 seconds; no flash attempted")


if __name__ == "__main__":
    main()
