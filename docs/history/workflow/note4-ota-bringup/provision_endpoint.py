"""Provision the exact Note4 Bridge endpoint over its bonded BLE GATT link.

The token is read from an ignored runtime file and never printed or logged.
"""

import argparse
import asyncio
import json
from pathlib import Path

from bleak import BleakClient, BleakScanner


NAME = "CodexStatus-B93408"
MAC = "7C:4F:AD:B9:34:08"
CHR_INFO = "e7f1a001-4b2a-4c9e-9a11-3c0d5e9a0000"
CHR_ENDPOINT = "e7f1a002-4b2a-4c9e-9a11-3c0d5e9a0000"
CHR_STATUS = "e7f1a004-4b2a-4c9e-9a11-3c0d5e9a0000"


async def provision(token_path: Path, host: str, port: int) -> None:
    token = token_path.read_text(encoding="utf-8").strip()
    if len(token) != 64 or any(c not in "0123456789abcdefABCDEF" for c in token):
        raise ValueError("endpoint token file must contain 64 hex digits")
    device = await BleakScanner.find_device_by_filter(
        lambda d, _: d.name == NAME, timeout=20
    )
    if device is None:
        raise RuntimeError("exact Note4 BLE advertisement not found")
    loop = asyncio.get_running_loop()
    ack = loop.create_future()

    def on_status(_characteristic, data: bytearray) -> None:
        try:
            reply = json.loads(bytes(data))
        except (UnicodeDecodeError, ValueError):
            return
        if reply.get("ack") == "endpoint" and not ack.done():
            ack.set_result(reply)

    async with BleakClient(device, timeout=30) as client:
        info = json.loads(bytes(await client.read_gatt_char(CHR_INFO)))
        if (info.get("mac") != MAC or info.get("peerBonded") is not True
                or info.get("peerEncrypted") is not True):
            raise RuntimeError("Note4 identity or bonded/encrypted BLE link mismatch")
        await client.start_notify(CHR_STATUS, on_status)
        payload = json.dumps(
            {"schema": 1, "host": host, "port": port, "token": token},
            separators=(",", ":"),
        ).encode("utf-8")
        for start in range(0, len(payload), 180):
            await client.write_gatt_char(
                CHR_ENDPOINT, payload[start:start + 180], response=True
            )
        reply = await asyncio.wait_for(ack, timeout=12)
        await client.stop_notify(CHR_STATUS)
    if reply.get("ok") is not True:
        raise RuntimeError("Note4 rejected endpoint configuration")
    print(f"Note4 endpoint provisioned over bonded BLE: {host}:{port}")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("token_file", type=Path)
    parser.add_argument("--host", required=True)
    parser.add_argument("--port", type=int, default=8765)
    args = parser.parse_args()
    asyncio.run(provision(args.token_file, args.host, args.port))


if __name__ == "__main__":
    main()
