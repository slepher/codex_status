"""One authenticated Note4 ROM OTA with exact device/slot preflight.

The operation token is obtained over the bonded encrypted BLE link and kept
only in memory. The upload is sent once; a lost HTTP ACK is resolved by reading
the post-reboot device state, never by sending the image again.
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

from ota_verify import MAC, NAME, TARGET, CHR_INFO, CHR_STATUS, CHR_AUTH
from bleak import BleakClient, BleakScanner


HTTP = urllib.request.build_opener(urllib.request.ProxyHandler({}))


def status(ip):
    with HTTP.open(f"http://{ip}/status.json", timeout=15) as response:
        doc = json.load(response)
    return {key: doc.get(key) for key in (
        "fw", "mac", "fw_target", "render_target", "slot", "next_slot",
        "v2_bundle", "commit_seq", "committed_job_id", "active_template_id", "psram_free",
        "epd_busy_fails", "ip")}


async def bonded_token():
    device = await BleakScanner.find_device_by_filter(
        lambda d, _: d.name == NAME, timeout=20)
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
    boundary = "note4-auth-ota"
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
    except Exception:
        return None, "transport closed; checking reboot"


def main():
    parser = argparse.ArgumentParser()
    for option in ("ip", "rom", "sha256", "from-fw", "to-fw", "from-slot",
                   "to-slot", "evidence"):
        parser.add_argument("--" + option, required=True)
    args = parser.parse_args()
    rom_path = Path(args.rom)
    evidence_path = Path(args.evidence)
    rom = rom_path.read_bytes()
    digest = hashlib.sha256(rom).hexdigest().upper()
    if digest != args.sha256.upper():
        raise SystemExit("ROM SHA256 mismatch; upload not attempted")
    if not (1024 < len(rom) <= 0x5F0000) or args.to_fw.encode() not in rom:
        raise SystemExit("ROM size/version guard failed; upload not attempted")
    before = status(args.ip)
    if (before["mac"] != MAC or before["fw_target"] != TARGET
            or before["fw"] != args.from_fw or before["slot"] != args.from_slot
            or before["next_slot"] != args.to_slot
            or before["v2_bundle"] is not True
            or before["active_template_id"] != "codex-status-a"):
        raise SystemExit("device identity/slot/template preflight failed; upload not attempted")
    evidence = {"rom": str(rom_path), "sha256": digest, "rom_bytes": len(rom),
                "before": before, "bonded_auth": False, "upload_count": 0}
    token = asyncio.run(bonded_token())
    evidence["bonded_auth"] = True
    print("Exact Note4 identity, slot, installed template, ROM hash and bonded BLE auth passed.")
    evidence["upload_count"] = 1
    code, reply = upload_once(args.ip, token, rom_path, rom)
    evidence["post_http_status"] = code
    evidence["post_reply"] = reply
    print(f"OTA POST: HTTP {code}, {reply}")
    if code not in (None, 200) or "UPDATE FAILED" in reply:
        evidence_path.write_text(json.dumps(evidence, indent=2), encoding="utf-8")
        raise SystemExit("OTA rejected; no second upload attempted")
    deadline = time.monotonic() + 150
    after = None
    while time.monotonic() < deadline:
        try:
            candidate = status(args.ip)
            if candidate["fw"] == args.to_fw:
                after = candidate
                break
        except (urllib.error.URLError, TimeoutError, ValueError):
            pass
        time.sleep(2)
    evidence["after"] = after
    evidence_path.write_text(json.dumps(evidence, indent=2), encoding="utf-8")
    if (after is None or after["mac"] != MAC or after["slot"] != args.to_slot
            or after["v2_bundle"] is not True
            or after["active_template_id"] != "codex-status-a"
            or after["committed_job_id"] != before["committed_job_id"]
            or after["commit_seq"] < before["commit_seq"]):
        raise SystemExit("new slot/template persistence not confirmed; do not retry OTA")
    print(f"Verified {args.to_fw} from {args.to_slot}; template/job retained.")


if __name__ == "__main__":
    main()
