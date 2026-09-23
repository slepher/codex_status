"""One-shot authenticated Note4 A -> B OTA verification.

The operation token lives only in process memory. Evidence excludes the token,
the authenticated URL, and any device secret. No upload retry is performed.
"""

import argparse
import asyncio
import hashlib
import json
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

from bleak import BleakClient, BleakScanner


NAME = "CodexStatus-B93408"
MAC = "7C:4F:AD:B9:34:08"
TARGET = "zectrix-note4-400x300"
CHR_INFO = "e7f1a001-4b2a-4c9e-9a11-3c0d5e9a0000"
CHR_STATUS = "e7f1a004-4b2a-4c9e-9a11-3c0d5e9a0000"
CHR_AUTH = "e7f1a007-4b2a-4c9e-9a11-3c0d5e9a0000"
HTTP = urllib.request.build_opener(urllib.request.ProxyHandler({}))


def public_status(ip):
    with HTTP.open(f"http://{ip}/status.json", timeout=30) as response:
        doc = json.load(response)
    return {key: doc.get(key) for key in (
        "fw", "fw_target", "render_target", "mac", "slot", "next_slot",
        "epd_writes", "epd_busy_fails", "ip")}


async def bonded_token():
    device = await BleakScanner.find_device_by_filter(
        lambda d, _: d.name == NAME, timeout=15)
    if device is None:
        raise RuntimeError("exact Note4 BLE advertisement not found")
    loop = asyncio.get_running_loop()
    answer = loop.create_future()

    def on_status(_characteristic, data):
        try:
            doc = json.loads(bytes(data))
        except (ValueError, UnicodeDecodeError):
            return
        if doc.get("ack") == "auth" and not answer.done():
            answer.set_result(doc)

    async with BleakClient(device, timeout=30) as client:
        info = json.loads(bytes(await client.read_gatt_char(CHR_INFO)))
        if (info.get("mac") != MAC or info.get("peerBonded") is not True
                or info.get("peerEncrypted") is not True):
            raise RuntimeError("BLE identity or encrypted bond mismatch")
        await client.start_notify(CHR_STATUS, on_status)
        await client.write_gatt_char(CHR_AUTH, b'{"cmd":"token"}', response=True)
        doc = await asyncio.wait_for(answer, timeout=12)
        await client.stop_notify(CHR_STATUS)
    token = doc.get("token") if doc.get("ok") is True else None
    if not isinstance(token, str) or len(token) != 32:
        raise RuntimeError("bonded Note4 did not issue a valid operation token")
    return token


def upload_once(ip, token, rom_path, rom):
    boundary = "note4-ota-01813"
    prefix = (f"--{boundary}\r\n"
              f'Content-Disposition: form-data; name="firmware"; filename="{rom_path.name}"\r\n'
              "Content-Type: application/octet-stream\r\n\r\n").encode()
    body = prefix + rom + f"\r\n--{boundary}--\r\n".encode()
    query = urllib.parse.urlencode({"target": TARGET, "token": token})
    request = urllib.request.Request(
        f"http://{ip}/doUpdate?{query}", data=body, method="POST",
        headers={"Content-Type": f"multipart/form-data; boundary={boundary}"})
    try:
        with HTTP.open(request, timeout=120) as response:
            return response.status, response.read(100).decode("ascii", "replace")
    except urllib.error.HTTPError as error:
        return error.code, "HTTP error"
    except (urllib.error.URLError, TimeoutError, ConnectionError):
        # Existing firmware may reboot before its HTTP response arrives.
        return None, "transport closed; checking reboot"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--ip", required=True)
    parser.add_argument("--rom", type=Path, required=True)
    parser.add_argument("--sha256", required=True)
    parser.add_argument("--evidence", type=Path, required=True)
    args = parser.parse_args()
    evidence = {"rom": str(args.rom), "sha256": args.sha256.upper()}
    rom = args.rom.read_bytes()
    if hashlib.sha256(rom).hexdigest().upper() != args.sha256.upper():
        raise SystemExit("ROM SHA256 mismatch; upload not attempted")
    if not (1024 < len(rom) <= 0x5F0000) or b"0.18.13-note4-b" not in rom:
        raise SystemExit("ROM size/version guard failed; upload not attempted")
    evidence["rom_bytes"] = len(rom)
    before = public_status(args.ip)
    evidence["before"] = before
    if (before["mac"] != MAC or before["fw_target"] != TARGET
            or before["fw"] != "0.18.13-note4-a"
            or before["slot"] != "ota_0" or before["next_slot"] != "ota_1"):
        raise SystemExit("device identity/slot preflight failed; upload not attempted")
    token = asyncio.run(bonded_token())
    evidence["bonded_auth"] = True
    print("Exact Note4 identity, ota_0 preflight, ROM hash and bonded BLE auth passed.")

    # Exactly one authenticated POST; never retry a potentially partial upload.
    status, reply = upload_once(args.ip, token, args.rom, rom)
    evidence["post_http_status"] = status
    evidence["post_reply"] = reply
    print(f"OTA POST result: HTTP {status}, {reply}")
    if status is not None and status != 200:
        args.evidence.write_text(json.dumps(evidence, indent=2), encoding="utf-8")
        raise SystemExit("OTA POST rejected")
    if "UPDATE FAILED" in reply:
        args.evidence.write_text(json.dumps(evidence, indent=2), encoding="utf-8")
        raise SystemExit("device reported UPDATE FAILED")

    deadline = time.monotonic() + 100
    after = None
    while time.monotonic() < deadline:
        try:
            candidate = public_status(args.ip)
            if candidate["fw"] == "0.18.13-note4-b":
                after = candidate
                break
        except (urllib.error.URLError, TimeoutError, ValueError):
            pass
        time.sleep(2)
    evidence["after"] = after
    args.evidence.write_text(json.dumps(evidence, indent=2), encoding="utf-8")
    if after is None or after["mac"] != MAC or after["slot"] != "ota_1":
        raise SystemExit("B boot/ota_1 not confirmed; inspect USB serial")
    print("B boot confirmed over HTTP: ota_1, MAC and target match.")


if __name__ == "__main__":
    main()
