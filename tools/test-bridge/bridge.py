#!/usr/bin/env python3
"""
Codex Status 测试桥接（开发用）
- HTTP :8765/usage  (Bearer token)  -> 设备经 Wi-Fi 主通道拉取
- HTTP :8765/template?id=&hash=     -> 模板（hash 一致返回 304）
- BLE central (bleak): 向设备写入 endpoint+token; 可选推送 usage / 模板
用法:
  python bridge.py                 # 启动 HTTP + 扫描 BLE 并下发 endpoint
  python bridge.py --push-usage    # 额外通过 BLE 推一次 usage（测备选通道）
  python bridge.py --push-template # 额外通过 BLE 推送模板（测模板传输）
  python bridge.py --no-ble        # 只起 HTTP
"""
import argparse
import asyncio
import json
import socket
import sys
import threading
import time
import zlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlparse, parse_qs


_builtin_print = print


def safe_print(*args, **kwargs):
    try:
        _builtin_print(*args, **kwargs)
        return
    except UnicodeEncodeError:
        output = kwargs.get("file", sys.stdout)
        sep = kwargs.get("sep", " ")
        end = kwargs.get("end", "\n")
        encoding = getattr(output, "encoding", None) or "utf-8"
        text = sep.join(str(arg) for arg in args) + end
        encoded = text.encode(encoding, errors="replace")
        buffer = getattr(output, "buffer", None)
        if buffer is not None:
            buffer.write(encoded)
            buffer.flush()
        else:
            output.write(encoded.decode(encoding, errors="replace"))


print = safe_print

try:
    from bleak import BleakScanner, BleakClient
except ImportError:
    BleakScanner = None
    BleakClient = None

CHR_ENDPOINT = "e7f1a002-4b2a-4c9e-9a11-3c0d5e9a0000"
CHR_USAGE = "e7f1a003-4b2a-4c9e-9a11-3c0d5e9a0000"
CHR_STATUS = "e7f1a004-4b2a-4c9e-9a11-3c0d5e9a0000"
CHR_TPL_CTRL = "e7f1a005-4b2a-4c9e-9a11-3c0d5e9a0000"
CHR_TPL_DATA = "e7f1a006-4b2a-4c9e-9a11-3c0d5e9a0000"
CHR_INFO = "e7f1a001-4b2a-4c9e-9a11-3c0d5e9a0000"
JSON_WRITE_LIMIT = 180

ARGS = None
RESET_AT = int(time.time()) + 3 * 3600  # 启动时固定，模拟真实窗口重置时间

TEMPLATES = [
    {
        "schema": 1, "id": "full", "version": 3, "min_fw": "0.3",
        "canvas": {"w": 200, "h": 200},
        "elements": [
            {"type": "rect", "rect": [3, 3, 194, 194], "fill": False, "color": "black"},
            {"type": "text", "bind": "account.plan", "prefix": "CODEX ", "x": 8, "y": 10,
             "font": "f16", "color": "black"},
            {"type": "text", "bind": "bridge.label", "x": 8, "y": 32, "font": "f12",
             "color": "black"},
            {"type": "text", "bind": "device.channel", "x": 156, "y": 12, "font": "f12",
             "color": "red"},
            {"type": "text", "bind": "buckets[codex].weekly.usedPercent", "prefix": "USED ",
             "suffix": "%", "x": 8, "y": 52, "font": "f24", "color": "black"},
            {"type": "bar", "bind": "buckets[codex].weekly.remaining",
             "rect": [8, 84, 184, 14], "fg": "yellow", "bg": "white", "border": True},
            {"type": "text", "bind": "buckets[codex].weekly.resetsAt", "prefix": "RESET ",
             "x": 8, "y": 106, "font": "f12", "color": "black"},
            {"type": "text", "bind": "device.sync_hhmm", "prefix": "SYNC ",
             "x": 8, "y": 126, "font": "f12", "color": "black"},
            {"type": "text", "bind": "resetCredits.availableCount", "prefix": "RC ",
             "x": 118, "y": 126, "font": "f12", "color": "black"},
            {"type": "text", "bind": "device.ip", "prefix": "IP ",
             "x": 8, "y": 146, "font": "f12", "color": "black"},
            {"type": "text", "bind": "server_time", "prefix": "SRV ",
             "x": 8, "y": 166, "font": "f12", "color": "black"},
        ],
    },
    {
        "schema": 1, "id": "mini", "version": 2, "min_fw": "0.3",
        "canvas": {"w": 200, "h": 200},
        "elements": [
            {"type": "text", "bind": "account.plan", "prefix": "PLAN ",
             "x": 8, "y": 8, "font": "f12", "color": "black"},
            {"type": "text", "bind": "buckets[codex].weekly.remaining", "suffix": "%",
             "x": 38, "y": 56, "font": "f24", "color": "black"},
            {"type": "text", "text": "LEFT", "x": 88, "y": 96, "font": "f16",
             "color": "black"},
            {"type": "bar", "bind": "buckets[codex].weekly.remaining",
             "rect": [20, 140, 160, 20], "fg": "black", "bg": "white", "border": True},
            {"type": "text", "bind": "device.sync_hhmm", "prefix": "SYNC ",
             "x": 8, "y": 170, "font": "f12", "color": "black"},
        ],
    },
]


def tpl_canon(t: dict) -> bytes:
    return json.dumps(t, separators=(",", ":"), sort_keys=True).encode()


TPL_BYTES = {t["id"]: tpl_canon(t) for t in TEMPLATES}
TPL_HASH = {t["id"]: f"{zlib.crc32(TPL_BYTES[t['id']]) & 0xffffffff:08x}" for t in TEMPLATES}
TPL_BY_ID = {t["id"]: t for t in TEMPLATES}


def lan_ip() -> str:
    s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    try:
        s.connect(("8.8.8.8", 80))
        return s.getsockname()[0]
    finally:
        s.close()


def bridge_label() -> str:
    name = socket.gethostname()
    cleaned = "".join(c if (" " <= c <= "~") else "?" for c in name)[:16]
    return cleaned or "bridge"


def fragment_payload(data: bytes, limit: int = JSON_WRITE_LIMIT) -> list[bytes]:
    if limit <= 0:
        raise ValueError("fragment limit must be positive")
    return [data[i:i + limit] for i in range(0, len(data), limit)]


def negotiated_write_limit(client, default: int = JSON_WRITE_LIMIT) -> int:
    mtu = getattr(client, "mtu_size", None)
    if isinstance(mtu, int) and mtu > 3:
        return min(default, mtu - 3)
    return default


async def write_fragmented(client, uuid: str, data: bytes, limit: int | None = None):
    limit = negotiated_write_limit(client) if limit is None else limit
    for chunk in fragment_payload(data, limit):
        await client.write_gatt_char(uuid, chunk, response=True)


def make_usage() -> dict:
    now = int(time.time())
    used = (now // 30) % 100 if ARGS.dynamic else ARGS.percent
    return {
        "schema": 1,
        "server_time": now,
        "next_sync_seconds": ARGS.interval,
        "bridge": {"label": bridge_label(), "hostId": "test01"},
        "account": {"plan": "prolite"},
        "buckets": [
            {
                "id": "codex",
                "name": None,
                "windows": [
                    {
                        "kind": "weekly",
                        "usedPercent": used,
                        "resetsAt": RESET_AT,
                        "windowMins": 10080,
                    }
                ],
                "credits": {"balance": "0", "hasCredits": False, "unlimited": False},
            }
        ],
        "resetCredits": {"availableCount": 1, "nextExpiresAt": now + 7 * 86400},
        "templates": {tid: {"version": t["version"], "hash": TPL_HASH[tid]}
                      for tid, t in TPL_BY_ID.items()},
        "active_hold_seconds": ARGS.active_hold,
    }


class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        u = urlparse(self.path)
        qs = parse_qs(u.query)
        if self.headers.get("Authorization", "") != f"Bearer {ARGS.token}":
            self.send_response(401)
            self.end_headers()
            self.wfile.write(b"unauthorized")
            print("[http] 401 unauthorized")
            return
        if u.path == "/usage":
            body = json.dumps(make_usage()).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            print(f"[http] 200 /usage -> {json.loads(body)['buckets'][0]['windows'][0]['usedPercent']}%")
            return
        if u.path == "/template":
            tid = qs.get("id", ["full"])[0]
            thash = qs.get("hash", [""])[0]
            if tid not in TPL_BYTES:
                self.send_response(404)
                self.end_headers()
                return
            if thash and thash == TPL_HASH[tid]:
                self.send_response(304)
                self.end_headers()
                print(f"[http] 304 /template?id={tid} (hash unchanged)")
                return
            body = TPL_BYTES[tid]
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            print(f"[http] 200 /template?id={tid} hash={TPL_HASH[tid]} ({len(body)}B)")
            return
        self.send_response(404)
        self.end_headers()

    def log_message(self, fmt, *a):
        pass


def serve_http():
    srv = ThreadingHTTPServer(("0.0.0.0", ARGS.port), Handler)
    print(f"[http] listening on http://{lan_ip()}:{ARGS.port} "
          f"(token={ARGS.token}, templates={list(TPL_BYTES)})")
    srv.serve_forever()


async def push_template(client, tid: str):
    data = TPL_BYTES[tid]
    crc = zlib.crc32(data) & 0xffffffff
    ctrl = {"op": "begin", "id": tid, "version": TPL_BY_ID[tid]["version"],
            "hash": f"{crc:08x}", "len": len(data), "crc": crc}
    await write_fragmented(client, CHR_TPL_CTRL, json.dumps(ctrl).encode())
    print(f"[ble] template begin id={tid} len={len(data)} crc={crc:08x}")
    await asyncio.sleep(0.3)
    off = 0
    while off < len(data):
        chunk = data[off:off + max(1, negotiated_write_limit(client) - 2)]
        payload = bytes([off & 0xFF, (off >> 8) & 0xFF]) + chunk
        await client.write_gatt_char(CHR_TPL_DATA, payload, response=True)
        off += len(chunk)
        await asyncio.sleep(0.03)
    await write_fragmented(client, CHR_TPL_CTRL, b'{"op":"end"}')
    print(f"[ble] template end id={tid} ({off}B sent)")
    await asyncio.sleep(0.5)
    act = json.dumps({"op": "activate", "id": tid}).encode()
    await write_fragmented(client, CHR_TPL_CTRL, act)
    print(f"[ble] template activate {tid}")


async def ble_push():
    if BleakScanner is None:
        print("[ble] bleak 未安装，跳过 BLE（pip install bleak）")
        return
    print("[ble] scanning for CodexStatus-* (30s)...")
    dev = await BleakScanner.find_device_by_filter(
        lambda d, ad: (d.name or "").startswith("CodexStatus"), timeout=30
    )
    if not dev:
        print("[ble] device not found")
        return
    print(f"[ble] connecting {dev.name} ({dev.address})")
    async with BleakClient(dev, timeout=30) as client:
        info_raw = await client.read_gatt_char(CHR_INFO)
        try:
            info = json.loads(bytes(info_raw).decode("utf-8"))
        except (UnicodeDecodeError, json.JSONDecodeError) as e:
            raise RuntimeError(f"device info is malformed: {e}") from e
        if not isinstance(info, dict) or info.get("peerBonded") is not True:
            raise RuntimeError(
                "device is not bonded; pair manually in Windows Bluetooth settings: "
                "hold BOOT for 2 seconds to open the 120 second pairing window, "
                "connect CodexStatus, then retry"
            )
        print(f"[ble] device info: {info}")
        try:
            await client.start_notify(
                CHR_STATUS, lambda _, data: print("[ble] status:", data.decode(errors="replace")))
        except Exception as e:
            print("[ble] notify subscribe failed:", e)
        ep = {"schema": 1, "host": lan_ip(), "port": ARGS.port, "token": ARGS.token}
        await write_fragmented(client, CHR_ENDPOINT, json.dumps(ep).encode())
        print(f"[ble] endpoint written: {ep}")
        if ARGS.push_usage:
            await write_fragmented(client, CHR_USAGE, json.dumps(make_usage()).encode())
            print("[ble] usage pushed over BLE")
            await asyncio.sleep(1)
        if ARGS.push_template:
            await push_template(client, ARGS.template)
        await asyncio.sleep(1)


def main():
    global ARGS
    p = argparse.ArgumentParser()
    p.add_argument("--token", default="test-token-123")
    p.add_argument("--port", type=int, default=8765)
    p.add_argument("--percent", type=int, default=91)
    p.add_argument("--dynamic", action="store_true", help="percent 随时间变化")
    p.add_argument("--interval", type=int, default=60)
    p.add_argument("--push-usage", action="store_true")
    p.add_argument("--push-template", action="store_true")
    p.add_argument("--template", default="full", choices=list(TPL_BY_ID))
    p.add_argument("--active-hold", type=int, default=600,
                   help="envelope active_hold_seconds")
    p.add_argument("--no-ble", action="store_true")
    ARGS = p.parse_args()

    threading.Thread(target=serve_http, daemon=True).start()
    if not ARGS.no_ble:
        try:
            asyncio.run(ble_push())
        except Exception as e:
            print(f"[ble] failed: {e}; HTTP mock remains available")
    print("[main] running; Ctrl+C to stop")
    try:
        while True:
            time.sleep(3600)
    except KeyboardInterrupt:
        pass


if __name__ == "__main__":
    main()
