"""Check the real host renderer's battery pixels and write a nearest-neighbor preview.
Usage: python3 tools/test-note4-battery-icon.py <bridge-render> <existing-output-dir>
"""
import json
from pathlib import Path
import struct
import subprocess
import sys
import zlib

ROOT = Path(__file__).resolve().parents[1]
TEMPLATE = ROOT / 'bridge/crates/core/tests/fixtures/codex-status-a-400x300.json'


def pixels(path):
    data = path.read_bytes()
    assert struct.unpack('>IIBBBBB', data[16:29]) == (400, 300, 8, 0, 0, 0, 0)
    compressed = b''
    pos = 8
    while pos < len(data):
        size = struct.unpack('>I', data[pos:pos + 4])[0]
        if data[pos + 4:pos + 8] == b'IDAT':
            compressed += data[pos + 8:pos + 8 + size]
        pos += size + 12
    raw = zlib.decompress(compressed)
    rows = []
    for y in range(300):
        kind = raw[y * 401]
        row = bytearray(raw[y * 401 + 1:(y + 1) * 401])
        assert kind in range(5)
        for x in range(400):
            a = row[x - 1] if x else 0
            b = rows[y - 1][x] if y else 0
            c = rows[y - 1][x - 1] if y and x else 0
            p = a + b - c
            distances = [abs(p - v) for v in (a, b, c)]
            paeth = (a, b, c)[distances.index(min(distances))]
            row[x] = (row[x] + (0, a, b, (a + b) // 2, paeth)[kind]) & 255
        rows.append(row)
    return rows


def render(template, level, output):
    subprocess.run([str(Path(sys.argv[1]).resolve()), '--template', str(template),
                    '--battery', str(level), '--usage', str(output.parent / 'note4-battery-usage.json'),
                    '--out', str(output), '--compare-compiled'], check=True)
    return pixels(output)


def chunk(kind, payload):
    return struct.pack('>I', len(payload)) + kind + payload + struct.pack('>I', zlib.crc32(kind + payload))


out = Path(sys.argv[2]).resolve()
assert out.is_dir(), 'Output directory must already exist'
(out / 'note4-battery-usage.json').write_text(json.dumps({
    'schema': 1, 'server_time': 1700000000, 'bridge': {'label': 'preview', 'hostId': 'preview'},
    'account': {'plan': 'plus'}, 'buckets': [], 'resetCredits': {'availableCount': 2}
}))
frames = {}
for level in (0, 50, 99, 100):
    frame = render(TEMPLATE, level, out / f'note4-battery-{level}.png')
    frames[level] = frame
    # Entire one-pixel moat, including corners, must stay white.
    for y in range(14, 22):
        for x in range(322, 337):
            if x in (322, 336) or y in (14, 21):
                assert frame[y][x] == 255, (level, x, y, 'gap lost')
    if level == 0:
        assert all(frame[y][x] == 255 for y in range(15, 21) for x in range(323, 336))
    if level == 100:
        assert all(frame[y][x] == 0 for y in range(15, 21) for x in range(323, 336))
# Reproduce the old bug using only its original bar geometry.
old = json.loads(TEMPLATE.read_text())
next(e for e in old['elements'] if e.get('type') == 'bar' and e.get('bind') == 'device.battery')['rect'] = [324, 16, 14, 7]
before = out / 'note4-battery-before.json'
before.write_text(json.dumps(old))
old_frame = render(before, 100, out / 'note4-battery-before-100.png')
assert old_frame[17][336] == old_frame[17][337] == 0
# Left: old 100%; center: fixed 99%; right: fixed 100%. 8x, no smoothing.
frames_to_show = [old_frame, frames[99], frames[100]]
width, height = 3 * 80 * 8, 28 * 8
raw = bytearray()
for y in range(height):
    raw.append(0)
    for frame in frames_to_show:
        for x in range(80 * 8):
            raw.append(frame[4 + y // 8][316 + x // 8])
png = b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', width, height, 8, 0, 0, 0, 0))
png += chunk(b'IDAT', zlib.compress(raw)) + chunk(b'IEND', b'')
(out / 'note4-battery-comparison.png').write_bytes(png)
print('PASS: 0/50/99/100% moat; full fill; old contact reproduced; shared-engine parity')
