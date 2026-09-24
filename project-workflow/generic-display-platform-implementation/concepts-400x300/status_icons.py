"""Reproducible 20x20 monochrome status artwork for the Note4 template."""
import base64
import json
import struct
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parent

# Rasterized from the user-selected Icons8 TV Off (#20203) and battery (#59804).
# The device-facing name of the former is Bridge Off.
# at 20x20. Bluetooth/Wi-Fi are the earlier 20x20 Icons8-inspired artwork.
TV_OFF = """
....................
....................
....................
..#################.
.###################
.##...............##
.##...............##
.##.....##.##.....##
.##.....#####.....##
.##......###......##
.##.....#####.....##
.##.....##.##.....##
.##...............##
.##...............##
.###.............###
..#################.
.......#######......
.......#######......
....................
....................
"""
BATTERY = """
....................
....................
....................
....................
....................
..################..
.##.............##..
.#...............##.
.#..########.....###
.#..########.....###
.#..########.....###
.#..########.....###
.#...............##.
.##..............#..
..################..
....................
....................
....................
....................
....................
"""
BLUETOOTH = "AGAAAHAAAHgAAGwAAGYADGcABmYAA/wAAfgAAPAAAPAAAfgAA/wABmYADGcAAGYAAGwAAHgAAHAAAGAA"
WIFI = "AAAAAAAAAAAAA/wAH/+APAPA8ADwwPAwh/4QDw8AHAOAEACAAfgAA/wAAwwAAAAAAGAAAGAAAAAAAAAA"
WIFI_OFF = "AAAAAAAAAGAAA2wAH2+APGPA8GDwwGAwh24QD28AHGOAEGCAAQgAAwwAAwwAAAAAAGAAAGAAAAAAAAAA"
# User-approved 20x20 sleep artwork with one-pixel horizontal bars.
SLEEP_20 = "AAAAAAAAf8AAAEAAAIAAAR+AAgCABAEAf8IAAAQAAAgAAB+AHwAAAQAAAgAABAAAHwAAAAAAAAAAAAAA"


def rows(art):
    lines = art.strip().splitlines()
    assert len(lines) == 20 and all(len(line) == 20 for line in lines)
    return [[pixel == "#" for pixel in line] for line in lines]


def from_bits(value):
    data = base64.b64decode(value)
    assert len(data) == 60
    return [[bool(data[y * 3 + x // 8] & (0x80 >> (x % 8)))
             for x in range(20)] for y in range(20)]


def bits(pixels):
    data = bytearray(60)
    for y in range(20):
        for x in range(20):
            if pixels[y][x]:
                data[y * 3 + x // 8] |= 0x80 >> (x % 8)
    return base64.b64encode(data).decode()


def halves(pixels):
    result = []
    for x0 in (0, 10):
        data = bytearray(40)
        for y in range(20):
            for x in range(10):
                if pixels[y][x0 + x]:
                    data[y * 2 + x // 8] |= 0x80 >> (x % 8)
        result.append(base64.b64encode(data).decode())
    return result


def line(pixels, x1, y1, x2, y2, thick=1):
    steps = max(abs(x2 - x1), abs(y2 - y1))
    for step in range(steps + 1):
        x = round(x1 + (x2 - x1) * step / steps)
        y = round(y1 + (y2 - y1) * step / steps)
        pixels[y][x] = True
        if thick == 2:
            pixels[y + 1][x] = True


def crossed(pixels):
    """Add a two-pixel diagonal strike over a 20x20 on-state icon."""
    result = [row.copy() for row in pixels]
    for step in range(2, 18):
        x, y = step, 18 - step
        result[y][x] = True
        result[y + 1][x] = True
    return result


def icon_set():
    off = rows(TV_OFF)
    for y in range(7, 12):
        for x in range(7, 14):
            off[y][x] = False
    line(off, 7, 7, 12, 11)
    line(off, 12, 7, 7, 11)
    on = [row.copy() for row in off]
    for y in range(7, 13):
        for x in range(7, 14):
            on[y][x] = False
    line(on, 7, 9, 9, 11, 2)
    line(on, 9, 11, 13, 7, 2)

    # Normalize the visible height inside each 20x20 box. The original
    # Bluetooth fills all 20 rows, while Wi-Fi and Bridge use only 15.
    bluetooth = [[False] * 20 for _ in range(20)]
    original_bluetooth = from_bits(BLUETOOTH)
    for y in range(16):
        bluetooth[y + 2] = original_bluetooth[round(y * 19 / 15)].copy()

    original_battery = rows(BATTERY)
    outline = [[False] * 20 for _ in range(20)]
    for y in range(4, 16):
        outline[y] = original_battery[5 + round((y - 4) * 9 / 11)].copy()
    for y in range(7, 13):
        for x in range(4, 16):
            outline[y][x] = False
    icons = {
        "bluetooth": bluetooth,
        "bluetooth-off": crossed(bluetooth),
        "wifi": from_bits(WIFI),
        "wifi-off": from_bits(WIFI_OFF),
        "bridge-off": off,
        "bridge-on": on,
        "zzz": from_bits(SLEEP_20),
        "battery-outline": outline,
    }
    for level in (0, 25, 50, 75, 100):
        image = [row.copy() for row in outline]
        for y in range(7, 13):
            for x in range(4, 4 + round(level * 12 / 100)):
                image[y][x] = True
        icons[f"battery-{level}"] = image
    return icons


def write_png(path, pixels, scale=1):
    raw = bytearray()
    for y in range(20 * scale):
        raw.append(0)
        for x in range(20 * scale):
            raw.append(0 if pixels[y // scale][x // scale] else 255)

    def chunk(name, payload):
        return (struct.pack(">I", len(payload)) + name + payload
                + struct.pack(">I", zlib.crc32(name + payload) & 0xffffffff))

    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", 20 * scale, 20 * scale, 8, 0, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(raw, 9))
    png += chunk(b"IEND", b"")
    path.write_bytes(png)


if __name__ == "__main__":
    icons = icon_set()
    for name, pixels in icons.items():
        write_png(ROOT / f"{name}-20.png", pixels)
        write_png(ROOT / f"{name}-20-8x.png", pixels, 8)
    (ROOT / "status-icons-bits.json").write_text(
        json.dumps({name: bits(pixels) for name, pixels in icons.items()}, indent=2) + "\n",
        encoding="utf-8",
    )
