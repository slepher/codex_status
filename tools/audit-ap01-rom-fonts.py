"""Read-only 0041 font inventory: full-file callback references and cmap decoding.

Writes only to a caller-precreated evidence directory; never accesses devices.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import string
import struct

BASE = 0x9ffff000
EXPECTED = '972db4c136c7ed9e24a83c07c1a7fd62040ca018b08ca285216d26b1fee3c6b9'
parser = argparse.ArgumentParser()
parser.add_argument('--rom', type=Path, required=True)
parser.add_argument('--extractor', type=Path, required=True)
parser.add_argument('--out', type=Path, required=True)
parser.add_argument('--specimen', action='store_true', help='Export raw glyph specimens (requires Pillow)')
parser.add_argument('--reference-font', type=Path, help='Compare complete glyph data with LVGL C source')
args = parser.parse_args()
assert args.out.is_dir(), 'Precreate evidence directory with elevation'
rom = args.rom.read_bytes()
assert hashlib.sha256(rom).hexdigest() == EXPECTED

def offset(address, size):
    at = address - BASE
    if not 0 <= at <= len(rom) - size:
        raise ValueError(f'out of ROM: {address:#x}+{size}')
    return at

def unpack(fmt, address):
    return struct.unpack_from(fmt, rom, offset(address, struct.calcsize(fmt)))

def decode_font(address):
    callback, bitmap_callback, release, height, baseline, flags, descriptor, fallback, user = unpack('<9I', address)
    if not 0 < height <= 128 or not 0 <= baseline < height:
        raise ValueError('invalid font metrics')
    bitmap, metrics, cmaps, kern, packed = unpack('<5I', descriptor)
    count, bpp, bitmap_format = packed >> 16 & 0x1ff, packed >> 25 & 15, packed >> 30
    if not 0 < count <= 511 or bpp not in (1, 2, 3, 4, 8):
        raise ValueError('invalid descriptor')
    points, maps = {}, []
    for index in range(count):
        cmap_address = cmaps + index * 20
        start, length, first, unicodes, indexes, items, kind = unpack('<IHHIIHB', cmap_address)
        if not 0 < length or start + length > 0x110000 or kind not in range(4):
            raise ValueError('invalid cmap')
        entries = []
        zero_offsets = []
        if kind in (0, 2):
            for delta in range(length):
                adjustment = rom[offset(indexes + delta, 1)] if kind == 0 else delta
                gid = first + adjustment
                entries.append((start + delta, gid))
                if kind == 0 and adjustment == 0:
                    zero_offsets.append(start + delta)
        else:
            previous = -1
            for position in range(items):
                delta = unpack('<H', unicodes + position * 2)[0]
                if not previous < delta < length:
                    raise ValueError('invalid sparse unicode list')
                previous = delta
                adjustment = unpack('<H', indexes + position * 2)[0] if kind == 1 else position
                entries.append((start + delta, first + adjustment))
        for cp, gid in entries:
            if cp in points:
                raise ValueError('overlapping cmap')
            points[cp] = gid
        maps.append({'address': hex(cmap_address), 'start': start, 'length': length,
                     'glyph_id_start': first, 'unicode_list': hex(unicodes),
                     'glyph_id_offsets': hex(indexes), 'list_length': items, 'type': kind,
                     'zero_offset_codepoints': zero_offsets})
    glyphs = {}
    for gid in set(points.values()) - {0}:
        word, width, rows, x, y = unpack('<IBBbb', metrics + gid * 8)
        if width > 128 or rows > 128:
            raise ValueError('invalid glyph size')
        bitmap_offset = word & 0xfffff
        if bitmap_format == 0:
            offset(bitmap + bitmap_offset, (width * rows * bpp + 7) // 8)
        glyphs[gid] = {'gid': gid, 'bitmap_address': hex(bitmap + bitmap_offset),
                       'advance_16': word >> 20, 'width': width, 'height': rows,
                       'x': x, 'y': y}
    visible = {cp for cp, gid in points.items() if gid in glyphs and
               glyphs[gid]['width'] and glyphs[gid]['height']}
    alphabet = string.ascii_uppercase + string.ascii_lowercase
    return {'address': hex(address), 'file_offset': address - BASE,
            'callback': hex(callback), 'bitmap_callback': hex(bitmap_callback),
            'height': height, 'baseline': baseline, 'descriptor': hex(descriptor),
            'bitmap': hex(bitmap), 'metrics': hex(metrics), 'kern': hex(kern),
            'packed': hex(packed), 'bpp': bpp, 'bitmap_format': bitmap_format,
            'fallback': hex(fallback), 'cmaps': maps, 'mapped_count': len(points),
            'visible_ascii': ''.join(chr(cp) for cp in range(33, 127) if cp in visible),
            'missing_letters': ''.join(c for c in alphabet if ord(c) not in visible),
            'complete_english': all(ord(c) in visible for c in alphabet + string.digits),
            'Y': glyphs.get(points.get(ord('Y'), 0)),
            'points': {str(cp): gid for cp, gid in sorted(points.items())},
            'glyphs': {str(gid): glyph for gid, glyph in sorted(glyphs.items())}}

known = {int(address, 16): name for name, address in re.findall(
    r'\("(ap01_rom_\w+)", (0x[0-9a-f]+),', args.extractor.read_text())}
seed = 0xa02549f4
signature = rom[offset(seed, 8):offset(seed, 8) + 8]
# Bytewise search, rather than alignment filtering, covers unaligned references.
hits, fonts, rejected = [], [], []
position = 0
while True:
    position = rom.find(signature, position)
    if position < 0:
        break
    hits.append(position)
    try:
        font = decode_font(BASE + position)
        font['recovered_name'] = known.get(BASE + position)
        fonts.append(font)
    except (ValueError, struct.error) as error:
        rejected.append({'offset': position, 'reason': str(error)})
    position += 1
assert set(known) <= {int(font['address'], 16) for font in fonts}
report = {'rom': str(args.rom.resolve()), 'sha256': EXPECTED, 'bytes': len(rom),
          'base': hex(BASE), 'scan': 'all bytes; exact pair of verified fmt_txt callbacks',
          'callback_pair': signature.hex(), 'candidate_count': len(hits),
          'known_count': len(known), 'font_count': len(fonts), 'rejected': rejected,
          'fonts': fonts}
if args.reference_font:
    source_bytes = args.reference_font.read_bytes()
    source = source_bytes.decode('utf-8')
    def array(name):
        body = re.search(r'\b' + name + r'\[\]\s*=\s*\{(.*?)\};', source, re.S)[1]
        return re.sub(r'/\*.*?\*/', '', body, flags=re.S)
    reference_bitmap = bytes(int(v, 16) for v in re.findall(r'0x([0-9a-fA-F]+)', array('glyph_bitmap')))
    reference_metrics = [tuple(map(int, row)) for row in re.findall(
        r'\.bitmap_index\s*=\s*(\d+),\s*\.adv_w\s*=\s*(\d+),\s*'
        r'\.box_w\s*=\s*(\d+),\s*\.box_h\s*=\s*(\d+),\s*'
        r'\.ofs_x\s*=\s*(-?\d+),\s*\.ofs_y\s*=\s*(-?\d+)', array('glyph_dsc'))]
    complete = [font for font in fonts if font['complete_english']]
    assert len(complete) == 1
    font = complete[0]
    for field, expected in [('line_height', font['height']), ('base_line', font['baseline']),
                            ('kern_scale', int(font['packed'], 16) & 65535), ('bpp', font['bpp']),
                            ('kern_classes', int(font['packed'], 16) >> 29 & 1),
                            ('bitmap_format', font['bitmap_format'])]:
        assert int(re.search(r'\.' + field + r'\s*=\s*(\d+)', source)[1]) == expected
    bitmap_at = offset(int(font['bitmap'], 16), len(reference_bitmap))
    assert rom[bitmap_at:bitmap_at + len(reference_bitmap)] == reference_bitmap
    for gid, reference in enumerate(reference_metrics):
        word, width, height, x, y = unpack('<IBBbb', int(font['metrics'], 16) + gid * 8)
        assert (word & 0xfffff, word >> 20, width, height, x, y) == reference
    values, left, right, left_count, right_count = unpack('<IIIBB', int(font['kern'], 16))
    kerning = {}
    for name, pointer in [('kern_left_class_mapping', left), ('kern_right_class_mapping', right),
                          ('kern_class_values', values)]:
        data = bytes(int(value) & 255 for value in re.findall(r'-?\d+', array(name)))
        at = offset(pointer, len(data))
        assert rom[at:at + len(data)] == data
        kerning[name] = {'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()}
    reference_cmaps = re.findall(
        r'\.range_start\s*=\s*(\d+),\s*\.range_length\s*=\s*(\d+),\s*\.glyph_id_start\s*=\s*(\d+)',
        array('cmaps'))
    assert [tuple(map(int, row)) for row in reference_cmaps] == [
        (c['start'], c['length'], c['glyph_id_start']) for c in font['cmaps']]
    sparse = b''
    for index, cmap in enumerate(font['cmaps']):
        if cmap['type'] == 3:
            unicode_values = [int(value, 16) for value in re.findall(r'0x([0-9a-fA-F]+)', array('unicode_list_' + str(index)))]
            assert len(unicode_values) == cmap['list_length']
            sparse = struct.pack('<' + 'H' * len(unicode_values), *unicode_values)
            at = offset(int(cmap['unicode_list'], 16), len(sparse))
            assert rom[at:at + len(sparse)] == sparse
    comparison = {'source': str(args.reference_font), 'source_sha256': hashlib.sha256(source_bytes).hexdigest(),
                  'font_address': font['address'], 'descriptor_records_equal': len(reference_metrics),
                  'bitmap_bytes_equal': len(reference_bitmap), 'bitmap_sha256': hashlib.sha256(reference_bitmap).hexdigest(),
                  'cmap_headers_equal': len(reference_cmaps), 'sparse_unicode_entries_equal': len(sparse) // 2,
                  'kerning': kerning, 'kerning_class_counts': [left_count, right_count]}
    report['reference_comparison'] = comparison
    (args.out / 'reference-comparison.json').write_text(json.dumps(comparison, indent=2))
(args.out / 'font-audit.json').write_text(json.dumps(report, ensure_ascii=False, indent=2))
for font in fonts:
    if font['complete_english']:
        print('Complete English:', font['address'], 'line height', font['height'],
              'Y gid', font['Y']['gid'], 'bitmap', font['Y']['bitmap_address'])
print(f'ROM SHA verified; {len(fonts)} fonts, {len(fonts)-len(known)} omitted; rejected {len(rejected)}')

if args.specimen:
    from PIL import Image, ImageDraw
    complete = [font for font in fonts if font['complete_english']]
    assert complete, 'No complete English font found'
    font = complete[0]
    assert font['bpp'] == 4 and font['bitmap_format'] == 0
    atlas = Image.new('RGB', (320, 180), 'black')
    draw = ImageDraw.Draw(atlas)
    draw.text((8, 5), 'ROM ' + font['address'] + ' / original pixels', fill='#aeb8c5')
    def glyph_mask(glyph):
        count = glyph['width'] * glyph['height']
        packed = offset(int(glyph['bitmap_address'], 16), (count + 1) // 2)
        values = bytes(((rom[packed + i // 2] >> 4) if i % 2 == 0 else
                        (rom[packed + i // 2] & 15)) * 17 for i in range(count))
        return Image.frombytes('L', (glyph['width'], glyph['height']), values)
    for row, value in enumerate([string.ascii_uppercase, string.ascii_lowercase,
                                 string.digits + ' !?@#$%&', 'SYNC 14:35   RC 2']):
        x, top = 8, 28 + row * 26
        for cp in map(ord, value):
            glyph = font['glyphs'][str(font['points'][str(cp)])]
            if glyph['width'] and glyph['height']:
                mask = glyph_mask(glyph)
                y = top + font['height'] - font['baseline'] - glyph['height'] - glyph['y']
                assert x + glyph['x'] >= 0 and x + glyph['x'] + glyph['width'] <= 320
                atlas.paste('white', (x + glyph['x'], y), mask)
            x += (glyph['advance_16'] + 8) >> 4
    draw.text((8, 145), 'Native 4bpp glyphs; no replacement font', fill='#aeb8c5')
    atlas.save(args.out / 'complete-english-original.png')
    atlas.resize((960, 540), Image.Resampling.NEAREST).save(args.out / 'complete-english-3x.png')
    y_glyph = font['Y']
    mask = glyph_mask(y_glyph)
    mask.save(args.out / 'Y-mask.png')
    mask.resize((220, 200), Image.Resampling.NEAREST).save(args.out / 'Y-mask-20x.png')
    packed_size = (y_glyph['width'] * y_glyph['height'] * font['bpp'] + 7) // 8
    at = offset(int(y_glyph['bitmap_address'], 16), packed_size)
    (args.out / 'Y-packed.bin').write_bytes(rom[at:at + packed_size])
