#!/usr/bin/env python3
"""Validate committed synthetic NOR before virtual reboot; does not execute Boot2."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import struct
import sys

sys.dont_write_bytecode = True


def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


fixture = module('ap01_fixture', 'build-qemu-flash-fixture.py')


def select(flash):
    if len(flash) != 0x1000000:
        raise ValueError('expected16MiB synthetic NOR')
    tables = []
    for address in (0xe000, 0xf000):
        raw = flash[address:address + 0x1000]
        try:
            info = fixture.validate_table(raw)
        except ValueError:
            continue
        # Strict fixture restrictions beyond the original CRC validator.
        if info['version'] != 2 or info['count'] != 1:
            raise ValueError('unexpected fixture partition profile')
        kind, device, active, name, start0, start1, cap0, cap1, length, age = struct.unpack_from('<BBB9s6I', raw, 16)
        if (kind, device, name, start0, start1, cap0, cap1) != (0, 0, b'FW' + bytes(7), 0x10000, 0x7b0000, 0x700000, 0x700000) or active not in (0, 1):
            raise ValueError('unexpected FW entry layout')
        tables.append(dict(address=address, active_slot=active,
                           image_offset=(start0, start1)[active], image_length=length,
                           entry_age=age, **info))
    if not tables:
        raise ValueError('no CRC-valid partition table')
    # Original0041 a00597c8..97d0: unsigned greater age wins; tie selects E000.
    chosen = max(tables, key=lambda t: (t['age'], -t['address']))
    return chosen, tables


def main():
    p = argparse.ArgumentParser(description=__doc__)
    for name in ('before', 'after', 'image', 'manifest', 'output'):
        p.add_argument('--' + name, type=Path, required=True)
    a = p.parse_args()
    if a.output.exists() or not a.output.parent.is_dir():
        p.error('output must be new in an existing directory')
    old, new, image = a.before.read_bytes(), a.after.read_bytes(), a.image.read_bytes()
    identity = fixture.validate_running_image(image, ota_manifest=a.manifest)
    previous, _ = select(old)
    current, tables = select(new)
    # Original ff13 copies the36-byte FW entry, mutating only activeIndex+2
    # and entry.age+32 (a00571ac..71f2). length+28 is preserved, not image size.
    # This synthetic normal/uncompressed profile deliberately starts at0.
    if previous['image_length'] != 0 or current['image_length'] != 0:
        raise ValueError('commit changed the preserved zero-length FW profile')
    if previous['entry_age'] == 0xffffffff or current['entry_age'] != previous['entry_age'] + 1:
        raise ValueError('commit did not advance FW entry age exactly once')
    if previous['age'] == 0xffffffff or current['age'] != previous['age'] + 1:
        raise ValueError('commit did not advance partition age exactly once')
    if current['active_slot'] == previous['active_slot'] or current['address'] == previous['address']:
        raise ValueError('commit did not switch FW and partition-table slot')
    offset = current['image_offset']
    if new[offset:offset + len(image)] != image:
        raise ValueError('selected NOR image differs from authenticated upload')
    protected = [(0, 0xe000), (previous['address'], previous['address'] + 0x1000),
                 (previous['image_offset'], previous['image_offset'] + 0x700000),
                 (0x720000, 0x7b0000), (0xff0000, 0x1000000)]
    if any(old[lo:hi] != new[lo:hi] for lo, hi in protected):
        raise ValueError('protected NOR region changed')
    result = dict(passed=True, physical_boot2_executed=False, physical_device_accessed=False,
                  selection_rule='original0041 unsigned age; equal age selects E000',
                  before=previous, selected=current, valid_tables=tables,
                  image=identity, nor_sha256=hashlib.sha256(new).hexdigest())
    with a.output.open('x', encoding='utf-8') as stream:
        json.dump(result, stream, indent=2)
        stream.write('\n')
    print(json.dumps(dict(passed=True, active_slot=current['active_slot'], output=str(a.output))))


if __name__ == '__main__':
    main()
