"""Package verified ROM Latin/numeric glyphs and whole-face CJK for Bundle v3.

No device access. Output directory must already exist; proprietary pixels stay
in local artifacts. The primary numeric asset is retained for 0..100 fit.
"""
import argparse
import base64
import hashlib
import importlib.util
import json
from pathlib import Path
import struct
import sys
import types
import zlib

sys.dont_write_bytecode = True
repo = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser()
parser.add_argument('--rom', type=Path, required=True)
parser.add_argument('--labels', type=Path, required=True)
parser.add_argument('--digits', type=Path, required=True)
parser.add_argument('--out', type=Path, required=True)
args = parser.parse_args()
assert args.out.is_dir(), 'Precreate output directory with elevation'

def module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value

decoder = module('ap01_rom_decoder', repo.parent / 'cuktech-ap01-firmware/tooling/scripts/rehost-assets.py')
sys.modules.setdefault('freetype', types.SimpleNamespace(FT_LOAD_FORCE_AUTOHINT=1, FT_LOAD_NO_HINTING=2))
packer = module('ap01_csfn', repo / 'tools/note4-fonts/rasterize_ttf.py')
rom = args.rom.read_bytes()
assert hashlib.sha256(rom).hexdigest() == decoder.SHA256
report = {'rom_sha256': decoder.SHA256, 'fonts': []}

def emit(name, family, height, baseline, size, records):
    bitmap, metrics, points = bytearray(), bytearray(), []
    for cp, advance, width, rows, left, y, alpha in records:
        assert len(alpha) == width * rows
        points.append(cp)
        metrics += struct.pack('<HHBBbb', len(bitmap), int(advance), width, rows, left, y)
        for row in range(rows):
            for x in range(0, width, 2):
                hi = alpha[row * width + x] // 17
                lo = alpha[row * width + x + 1] // 17 if x + 1 < width else 0
                bitmap.append((hi << 4) | lo)
    face = types.SimpleNamespace(hint='native', line_height=height, base_line=baseline)
    asset = bytearray(packer.pack_container(name, family, 'unicode', face, size,
                                          bitmap, metrics, len(points), points))
    struct.pack_into('<H', asset, 4, 3)
    asset[24:26] = bytes([4, 2])
    assert len(asset) <= 48 * 1024
    path = args.out / (name + '.csfn')
    path.write_bytes(asset)
    report['fonts'].append({'name': name, 'family': family, 'bytes': len(asset),
                            'id': f'{zlib.crc32(asset):08x}', 'line_height': height,
                            'size_px': size, 'sha256': hashlib.sha256(asset).hexdigest()})

def native(name, address, family, size):
    height, baseline, glyphs, pixels = decoder.native_font(rom, address)
    by_cp = {row[0]: row for row in glyphs}
    records = []
    for cp in range(32, 127):
        if cp in by_cp:
            _, offset, advance, width, rows, left, y = by_cp[cp]
            alpha = pixels[offset:offset + width * rows]
        else:
            advance, width, rows, left, y, alpha = 0, 0, 0, 0, 0, b''
        records.append((cp, advance * 16, width, rows, left, y, alpha))
    if name == 'nt16':
        assert set(range(32, 127)) <= set(by_cp)
    else:
        assert set(map(ord, '0123456789')) <= set(by_cp)
    emit(name, family, height, baseline, size, records)

native('nt16', 0xa0262f60, 'LVGL-Montserrat14', 14)
native('nt30', 0xa0254a84, 'AP01-ROM-Numerals38', 38)
records = []
for glyph in json.loads(args.labels.read_text(encoding='utf-8-sig')):
    width, rows = glyph['width'], glyph['height']
    packed = base64.b64decode(glyph['bits'])
    alpha = bytearray()
    for y in range(rows):
        for x in range(width):
            value = packed[y * ((width + 1) // 2) + x // 2]
            alpha.append(((value >> 4) if x % 2 == 0 else (value & 15)) * 17)
    records.append((glyph['cp'], glyph['advance'], width, rows, glyph['left'], glyph['offset_y'], alpha))
emit('ntthin18', 'Microsoft-YaHei-UI', 24, 4, 20, records)
digits = args.digits.read_bytes()
assert digits[:4] == b'CSFN' and digits[24:26] == bytes([4, 2])
(args.out / 'ntreg64.csfn').write_bytes(digits)
report['fonts'].append({'name': 'ntreg64', 'family': 'retained-Bahnschrift48',
                        'bytes': len(digits), 'id': f'{zlib.crc32(digits):08x}',
                        'sha256': hashlib.sha256(digits).hexdigest()})
(args.out / 'font-report.json').write_text(json.dumps(report, indent=2))
print(json.dumps(report, indent=2))
