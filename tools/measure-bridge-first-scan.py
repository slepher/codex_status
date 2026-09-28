"""Measure Note4 BLE diagnostic scan hits while the test PC publisher runs.

Requires an already authorized light window and separate test publisher.
The operation token is read from the Bridge cache and never printed.
"""

import json
import time
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TOKEN_PATH = ROOT / "bridge/target/debug/data/device-token-7C4FADB93408.json"
OUT = ROOT / "artifacts/window-normal-note4.jsonl"
MAC = "7C:4F:AD:B9:34:08"


def main():
    cached = json.loads(TOKEN_PATH.read_text(encoding="utf-8"))
    assert cached["device_mac"].replace(":", "").upper() == MAC.replace(":", "")
    token = cached["token"]
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    status = json.load(opener.open("http://192.168.3.177/status.json", timeout=5))
    assert status["mac"].upper() == MAC and status["fw"] == "0.18.25-note4-b"
    assert status["power"]["mode"] == "light"
    rows = []
    for seconds in (1, 2):
        for index in range(12):
            start = time.monotonic()
            request = urllib.request.Request(
                f"http://192.168.3.177/diag?blescan={seconds}&company=65535",
                data=b"", method="POST",
                headers={"Authorization": f"Bearer {token}"},
            )
            result = json.load(opener.open(request, timeout=seconds + 10))
            rows.append({"scan_s": seconds, "index": index,
                         "elapsed_ms": round((time.monotonic() - start) * 1000),
                         "matched": result.get("matched"),
                         "first_match_ms": result.get("first_match_ms"),
                         "ok": result.get("ok")})
    OUT.write_text("".join(json.dumps(row) + "\n" for row in rows), encoding="utf-8")
    for seconds in (1, 2):
        group = [row for row in rows if row["scan_s"] == seconds]
        print(f"{seconds}s scan: {sum(row['matched'] > 0 for row in group)}/{len(group)} hit")
    print(f"evidence={OUT}")


if __name__ == "__main__":
    main()
