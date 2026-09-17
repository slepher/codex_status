#!/usr/bin/env python3
"""Request a Wi-Fi operation token from the device over the bonded BLE link.

The device only accepts Wi-Fi operations (firmware OTA /update, /doUpdate and
ArduinoOTA) with a token issued here. The token is bound to the encrypted
BLE link and persisted in NVS: it survives reboots and deep sleep until it is
rotated ({"cmd":"token","rotate":true}) or the device is factory reset.
"""
import argparse
import asyncio
import json

try:
    from bleak import BleakClient, BleakScanner
except ImportError:
    print("bleak is required: pip install bleak")
    raise SystemExit(2)

CHR_INFO = "e7f1a001-4b2a-4c9e-9a11-3c0d5e9a0000"
CHR_STATUS = "e7f1a004-4b2a-4c9e-9a11-3c0d5e9a0000"
CHR_AUTH = "e7f1a007-4b2a-4c9e-9a11-3c0d5e9a0000"


async def request_token(scan_timeout: float, reply_timeout: float) -> int:
    dev = await BleakScanner.find_device_by_filter(
        lambda d, ad: (d.name or "").startswith("CodexStatus-"), timeout=scan_timeout
    )
    if not dev:
        print("device not found")
        return 1
    print(f"connecting {dev.name} ({dev.address})")

    loop = asyncio.get_running_loop()
    token_future = loop.create_future()

    def on_status(_char, data: bytearray) -> None:
        text = bytes(data).decode("utf-8", errors="replace")
        print(f"status: {text}")
        try:
            msg = json.loads(text)
        except json.JSONDecodeError:
            return
        if isinstance(msg, dict) and msg.get("ack") == "auth" and not token_future.done():
            token_future.set_result(msg)

    async with BleakClient(dev, timeout=30) as client:
        info = json.loads(bytes(await client.read_gatt_char(CHR_INFO)).decode("utf-8"))
        if info.get("peerBonded") is not True or info.get("peerEncrypted") is not True:
            print("device link is not bonded+encrypted; hold BOOT for 2s to open the "
                  "pairing window and pair in Windows Bluetooth settings first")
            return 1
        await client.start_notify(CHR_STATUS, on_status)
        await client.write_gatt_char(CHR_AUTH, json.dumps({"cmd": "token"}).encode(), response=True)
        try:
            msg = await asyncio.wait_for(token_future, timeout=reply_timeout)
        except asyncio.TimeoutError:
            print("no auth reply from device")
            return 1
        await client.stop_notify(CHR_STATUS)

    if not msg.get("ok") or not msg.get("token"):
        print("device refused the token request")
        return 1
    print()
    print(f"TOKEN={msg['token']} (persisted in NVS; survives reboots and deep sleep)")
    print(f'usage: curl.exe --noproxy "*" -H "Authorization: Bearer {msg["token"]}" http://<device-ip>/update')
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description="Negotiate a Wi-Fi operation token over BLE")
    parser.add_argument("--scan-timeout", type=float, default=30.0)
    parser.add_argument("--reply-timeout", type=float, default=15.0)
    args = parser.parse_args()
    return asyncio.run(request_token(args.scan_timeout, args.reply_timeout))


if __name__ == "__main__":
    raise SystemExit(main())
