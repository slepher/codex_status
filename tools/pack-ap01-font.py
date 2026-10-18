"""Pack local Windows rasterized labels using the existing canonical CSFN writer."""
import base64
import importlib.util
import json
from pathlib import Path
import struct
import sys
import types

sys.modules.setdefault('freetype', types.SimpleNamespace(FT_LOAD_FORCE_AUTOHINT=1, FT_LOAD_NO_HINTING=2))
spec = importlib.util.spec_from_file_location('font_packer', Path(__file__).parent / 'note4-fonts/rasterize_ttf.py')
packer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(packer)
glyphs, bitmap, points = bytearray(), bytearray(), []
for glyph in json.loads(Path(sys.argv[1]).read_text(encoding='utf-8')):
    points.append(glyph['cp'])
    glyphs += struct.pack('<HHBBbb', len(bitmap), *(int(glyph[key]) for key in ('advance', 'width', 'height', 'left', 'offset_y')))
    bitmap += base64.b64decode(glyph['bits'])
slot, size = sys.argv[3], int(sys.argv[4])
face = types.SimpleNamespace(hint='native', line_height=60 if slot == 'ntreg64' else 38 if slot == 'nt30' else 18, base_line=12 if slot == 'ntreg64' else 8 if slot == 'nt30' else 4)
asset = packer.pack_container(slot, 'Bahnschrift' if slot in ('ntreg64','nt30') else 'Microsoft-YaHei-UI', 'unicode', face, size, bitmap, glyphs, len(points), points)
asset = bytearray(asset)
struct.pack_into('<H', asset, 4, 3) # v3: same Unicode table, row-padded alpha4 masks
asset[24:26] = bytes([4, 2])
if len(asset) > 48*1024: raise ValueError('alpha4 font exceeds 48 KiB')
Path(sys.argv[2]).write_bytes(asset)
print(f'{len(points)} glyphs, {len(asset)} bytes; local Bundle font asset')
