"""Offline LVGL/RGB565 layout study using recovered original 0041 font assets.

Run under WSL; output directory (including obj/) must be precreated elevated.
ROM Montserrat14 for complete ASCII labels, native large digits, and whole-label
YaHei UI 20px for CJK. Never splice font families inside a label or scale glyphs.
Requires ap01-labels-glyphs.json generated with size=20 and baseline=20 using
generate-ap01-font.ps1 (see the local supplement.ps1 evidence copy).
No device transport, production template edits, or archive modifications.
"""
import argparse
import base64
from concurrent.futures import ThreadPoolExecutor
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import subprocess
import sys
from PIL import Image

parser = argparse.ArgumentParser()
parser.add_argument('--sim', type=Path, required=True)
parser.add_argument('--out', type=Path, required=True)
parser.add_argument('--rom', type=Path, required=True)
args = parser.parse_args()
out = args.out.resolve()
assert out.is_dir() and (out / 'obj').is_dir(), 'Precreate output and obj with escalation'
lvgl = args.sim / 'apps/graphics/lvgl/lvgl'
assets = (args.sim / 'apps/examples/lvgldemo/ap01-assets.h').read_text()
start = assets.index('typedef struct { uint32_t unicode, offset;')
end = assets.index('#define ap01_rom_charge_live_power', start)
fonts = assets[start:end]
fonts = re.sub(r'\.fallback=&\w+', '.fallback=NULL', fonts)
names = re.findall(r'static const lv_font_t (\w+) =', fonts)
inventory = []
for name in names:
    definition = re.search(r'static const lv_font_t ' + name + r' = \{(.*?)\};', fonts, re.S)[1]
    height = int(re.search(r'\.line_height=(\d+)', definition)[1])
    records = re.search(name + r'_glyphs\[\] = \{(.*?)\};', fonts, re.S)[1]
    points = [int(x) for x in re.findall(r'\{(\d+),', records)]
    inventory.append({'name': name, 'height': height, 'points': points})
# Reuse the existing ROM decoder; the complete font was absent from its fixed
# export list, but its raw cmap/bitmap/kerning decoding supports this font.
sys.dont_write_bytecode = True
extractor = Path(__file__).resolve().parents[1].parent / 'cuktech-ap01-firmware/tooling/scripts/rehost-assets.py'
spec = importlib.util.spec_from_file_location('ap01_assets', extractor)
decoder = importlib.util.module_from_spec(spec)
spec.loader.exec_module(decoder)
rom = args.rom.read_bytes()
rom_sha = hashlib.sha256(rom).hexdigest()
assert rom_sha == decoder.SHA256
height, baseline, glyphs, native_pixels, pairs = decoder.native_font(rom, 0xa0262f60, True)
assert (height, baseline) == (16, 3)
assert set(range(32, 127)) <= {row[0] for row in glyphs}
name = 'ap01_rom_montserrat14'
fonts += '\nstatic const ap01_rom_kern_t ' + name + '_kern[] = {' + ','.join(
    '{' + ','.join(map(str, pair)) + '}' for pair in pairs) + '};\n'
fonts += 'static const ap01_rom_glyph_t ' + name + '_glyphs[] = {' + ','.join(
    '{' + ','.join(map(str, row)) + '}' for row in glyphs) + '};\n'
fonts += 'static const uint8_t ' + name + '_pixels[] = {' + ','.join(map(str, native_pixels)) + '};\n'
fonts += f'static const ap01_rom_font_data_t {name}_data = {{{len(glyphs)},{name}_glyphs,{name}_pixels,{len(pairs)},{name}_kern}};\n'
fonts += f'static const lv_font_t {name} = {{.get_glyph_dsc=ap01_rom_font_descriptor,.get_glyph_bitmap=ap01_rom_font_bitmap,.line_height={height},.base_line={baseline},.dsc=&{name}_data}};\n'
names.append(name)
inventory.append({'name': name, 'address': '0xa0262f60', 'height': height,
                  'nominal_pixels': 14, 'points': [row[0] for row in glyphs]})
supplement = json.loads((out / 'ap01-labels-glyphs.json').read_text(encoding='utf-8-sig'))
rows, pixels = [], bytearray()
# Keep this face intact. Coverage selection happens for each whole label in C.
for glyph in supplement:
    offset = len(pixels)
    packed = base64.b64decode(glyph['bits'])
    stride = (glyph['width'] + 1) // 2
    for y in range(glyph['height']):
        for x in range(glyph['width']):
            value = packed[y * stride + x // 2]
            pixels.append(((value >> 4) if x % 2 == 0 else (value & 15)) * 17)
    rows.append((glyph['cp'], offset, (glyph['advance'] + 8) // 16,
                 glyph['width'], glyph['height'], glyph['left'], glyph['offset_y']))
fonts += '\nstatic const ap01_rom_glyph_t supplement_glyphs[] = {' + ','.join(
    '{' + ','.join(map(str, row)) + '}' for row in rows) + '};\n'
fonts += 'static const uint8_t supplement_pixels[] = {' + ','.join(map(str, pixels)) + '};\n'
fonts += f'static const ap01_rom_font_data_t supplement_data = {{{len(rows)},supplement_glyphs,supplement_pixels,0,NULL}};\n'
fonts += 'static const lv_font_t supplement_font = {.get_glyph_dsc=ap01_rom_font_descriptor,.get_glyph_bitmap=ap01_rom_font_bitmap,.line_height=24,.base_line=4,.dsc=&supplement_data};\n'
(out / 'fonts.h').write_text('#include "lvgl.h"\n#include <string.h>\n' + fonts +
    '\nstatic const lv_font_t *original_fonts[] = {' + ','.join('&' + n for n in names) + '};\n')
(out / 'font-inventory.json').write_text(json.dumps(inventory, ensure_ascii=False, indent=2))
print(f'{len(inventory)} recovered ROM fonts; complete Montserrat14 and CJK face loaded', flush=True)
flags = ['-O1', '-DLV_CONF_SKIP=1', '-DLV_COLOR_DEPTH=16', '-DLV_USE_OS=0', '-I' + str(lvgl), '-I' + str(out)]
sources = sorted(lvgl.joinpath('src').rglob('*.c'))
objects = [out / 'obj' / (str(s.relative_to(lvgl)).replace('/', '_') + '.o') for s in sources]

def compile_one(pair):
    source, obj = pair
    if not obj.exists() or obj.stat().st_mtime < source.stat().st_mtime:
        result = subprocess.run(['gcc', *flags, '-c', str(source), '-o', str(obj)], capture_output=True, text=True)
        if result.returncode:
            raise RuntimeError(result.stderr)

with ThreadPoolExecutor(max_workers=6) as pool:
    list(pool.map(compile_one, zip(sources, objects)))
source = Path(__file__).with_suffix('.c')
subprocess.run(['gcc', *flags, str(source), *map(str, objects), '-lm', '-o', str(out / 'preview')], check=True)
result = subprocess.run(['stdbuf', '-oL', str(out / 'preview')], cwd=out, capture_output=True, text=True)
(out / 'validation.log').write_text(result.stdout + result.stderr)
print(result.stdout, flush=True)
if result.returncode:
    raise RuntimeError(result.stderr)
report = {'scope': 'ROM Montserrat14 native pixels for ASCII labels; intact YaHei UI20 for CJK labels; original large quota digits; LVGL 9.2.1 host software draw; RGB565; not physical ROM LVGL execution',
          'rom_sha256': rom_sha, 'ascii_font_address': '0xa0262f60',
          'ascii_nominal_pixels': 14, 'ascii_line_height': 16,
          'assets_sha256': hashlib.sha256(assets.encode()).hexdigest(), 'scenes': []}
for scene in ['dual', 'weekly', 'monthly', 'long-account']:
    raw = (out / (scene + '.rgb565')).read_bytes()
    assert len(raw) == 320 * 240 * 2
    image = Image.frombytes('RGB', (320, 240), raw, 'raw', 'BGR;16')
    image.save(out / (scene + '-montserrat14.png'))
    image.resize((960, 720), Image.Resampling.NEAREST).save(out / (scene + '-montserrat14-3x.png'))
    report['scenes'].append({'name': scene, 'sha256': hashlib.sha256(raw).hexdigest()})
(out / 'report-montserrat14.json').write_text(json.dumps(report, indent=2))
