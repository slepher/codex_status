#!/usr/bin/env python3
"""Build an explicitly synthetic 16MiB AP01 NOR fixture, never a device backup."""
import argparse
import hashlib
import json
from pathlib import Path
import struct
import subprocess
import zlib

IMAGE_SHA256 = '972db4c136c7ed9e24a83c07c1a7fd62040ca018b08ca285216d26b1fee3c6b9'
UNSIGNED_OTA_SHA256 = '1bbe8f45359eb7cdb55f337643f3ce28894999ffb21beab92fb62e995cd4854d'


def validate_running_image(data, original_path=None, ota_manifest=None):
    """Accept only original0041 or its explicitly authorized frozen patch."""
    digest = hashlib.sha256(data).hexdigest()
    identity = dict(original_sha256=IMAGE_SHA256, current_sha256=digest,
                    kind='original0041', patch_manifest=None)
    if ota_manifest is not None:
        if original_path is not None:
            raise ValueError('candidate identity options are mutually exclusive')
        import importlib.util
        spec = importlib.util.spec_from_file_location('ota_builder', Path(__file__).with_name('build-ota-channel-0041.py'))
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        built = module.validate_candidate(data, ota_manifest)
        identity.update(kind='0041-independent-ota-channel', original_path=built['original']['path'],
                        patch_manifest=dict(kind=built['format'], path=str(ota_manifest.resolve()),
                            sha256=hashlib.sha256(ota_manifest.read_bytes()).hexdigest(), build_id=built['build_id']))
        return identity
    if original_path is None:
        if len(data) != 6769704 or digest != IMAGE_SHA256:
            raise ValueError('only the exact original0041 image is accepted without --unsigned-ota-candidate ORIGINAL_IMAGE')
        return identity
    if len(data) != 6769704 or digest != UNSIGNED_OTA_SHA256:
        raise ValueError('unsigned OTA candidate must match its frozen SHA256')
    from patch_0041_unsigned_ota import patch_bytes
    original_path = original_path.resolve()
    expected, validation = patch_bytes(original_path.read_bytes())
    if data != expected:
        raise ValueError('candidate differs from patch_bytes(original) recomputation')
    encoded = json.dumps(validation, sort_keys=True, separators=(',', ':')).encode()
    identity.update(kind='0041-unsigned-ota-candidate', original_path=str(original_path),
                    patch_manifest=dict(kind='recomputed-patch_bytes-validation',
                        sha256=hashlib.sha256(encoded).hexdigest(), validation=validation,
                        tool='patch_0041_unsigned_ota.py',
                        tool_sha256=hashlib.sha256(Path(__file__).with_name('patch_0041_unsigned_ota.py').read_bytes()).hexdigest()))
    return identity


SDK_COMMIT = '41bf338a57be53919d99f6c6715a96100428c97d'
FLASH_SIZE = 0x1000000
MAGIC = 0x54504642


def sha(data):
    return hashlib.sha256(data).hexdigest()


def sdk_crc32(data):
    """Independent bitwise form of fixed SDK bflb_soft_crc32_ex(0,...)."""
    crc = 0xffffffff
    for byte in data:
        crc ^= byte
        for _ in range(8):
            crc = (crc >> 1) ^ (0xedb88320 if crc & 1 else 0)
    return crc ^ 0xffffffff


def partition_table(age):
    # Standard SDK entry: type/device/active/name[9], address[2], maxLen[2],
    # len, age. ROM a0059762 uses name+3, addr+12/+16, maxLen+20/+24,
    # stride36 and activeIndex. Normal/uncompressed SDK config leaves len=0.
    entry = struct.pack('<BBB9s6I', 0, 0, 0, b'FW',
                        0x10000, 0x7b0000, 0x700000, 0x700000, 0, 0)
    header = struct.pack('<IHHI', MAGIC, 2, 1, age)
    table = header + struct.pack('<I', zlib.crc32(header)) + entry
    table += struct.pack('<I', zlib.crc32(entry))
    return table.ljust(0x1000, b'\xff')


def validate_table(table):
    """Original a0058554 validator contract, using independent SDK CRC."""
    if len(table) < 20:
        raise ValueError('truncated partition table')
    magic, version, count, age, header_crc = struct.unpack_from('<IHHII', table)
    if magic != MAGIC or count > 16 or len(table) < 20 + count * 36:
        raise ValueError('invalid partition header/count/length')
    if sdk_crc32(table[:12]) != header_crc:
        raise ValueError('partition header CRC mismatch')
    entries = table[16:16 + count * 36]
    entries_crc, = struct.unpack_from('<I', table, 16 + count * 36)
    if sdk_crc32(entries) != entries_crc:
        raise ValueError('partition entries CRC mismatch')
    return dict(version=version, count=count, age=age,
                header_crc32=f'{header_crc:08x}', entries_crc32=f'{entries_crc:08x}')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--image', required=True, type=Path)
    parser.add_argument('--ota-channel-manifest', type=Path,
                        help='reconstruct an independently built bounded OTA-channel image')
    parser.add_argument('--unsigned-ota-candidate', type=Path, metavar='ORIGINAL_IMAGE',
                        help='explicitly allow frozen candidate after patch_bytes recheck against this original')
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--manifest', type=Path)
    parser.add_argument('--sdk', type=Path, default=Path(__file__).resolve().parents[3] / 'bouffalo_sdk')
    args = parser.parse_args()
    image_path, output = args.image.resolve(), args.output.resolve()
    manifest_path = (args.manifest or output.with_suffix('.manifest.json')).resolve()
    if output == manifest_path or output.exists() or manifest_path.exists():
        parser.error('fixture and manifest must be distinct new files; never overwrite')
    if not output.parent.is_dir() or not manifest_path.parent.is_dir():
        parser.error('output directories must already exist')
    original = image_path.read_bytes()
    try:
        image_identity = validate_running_image(original, args.unsigned_ota_candidate, args.ota_channel_manifest)
    except (ValueError, OSError) as exc:
        parser.error(str(exc))
    if len(original) > 0x700000:
        parser.error('original exceeds explicitly modeled FW0 extent')
    sdk = args.sdk.resolve()
    revision = subprocess.run(['git', '-C', str(sdk), 'rev-parse', 'HEAD'],
                              check=True, capture_output=True, text=True,
                              timeout=10).stdout.strip()
    if revision != SDK_COMMIT:
        parser.error('SDK HEAD must match pinned commit ' + SDK_COMMIT)
    sdk_crc = sdk / 'drivers/lhal/src/bflb_common.c'
    sdk_config = sdk / 'bsp/board/bl616dk/config/partition_cfg_4M.toml'
    crc_source, config_source = sdk_crc.read_bytes(), sdk_config.read_bytes()
    if b'0xEDB88320' not in crc_source or b'version = 2' not in config_source:
        parser.error('fixed SDK CRC/config evidence unavailable or incompatible')
    # SDK polynomial check independently agrees with standard library vector.
    if sdk_crc32(b'123456789') != 0xcbf43926:
        raise AssertionError('SDK CRC self-check failed')
    tables = [partition_table(1), partition_table(0)]
    validation = [validate_table(table) for table in tables]
    # Both protected ranges must reject corruption; no implementation-only test.
    for offset in (8, 16):
        damaged = bytearray(tables[0])
        damaged[offset] ^= 1
        try:
            validate_table(damaged)
        except ValueError:
            pass
        else:
            raise AssertionError('partition corruption was accepted')
    flash = bytearray(b'\xff' * FLASH_SIZE)
    flash[0xe000:0xf000], flash[0xf000:0x10000] = tables
    flash[0x10000:0x10000 + len(original)] = original
    assert len(flash) == FLASH_SIZE
    assert flash[0x11000:0x11004] == original[0x1000:0x1004]
    assert flash[0x710000:0x720000] == b'\xff' * 0x10000
    assert flash[0xeb0000:0xff0000] == b'\xff' * 0x140000
    assert flash[0x7b0000:0xeb0000] == b'\xff' * 0x700000
    manifest = dict(
        synthetic_fixture=True, device_backup=False, installable_image=False,
        original_rom_modified=False, boot2_written=False,
        original=dict(path=image_identity.get("original_path", str(image_path)), size=len(original), sha256=IMAGE_SHA256),
        running_image=dict(path=str(image_path), size=len(original), **image_identity),
        fixture=dict(path=str(output), size=FLASH_SIZE, sha256=sha(flash), erased_byte=255),
        sdk=dict(expected_commit=SDK_COMMIT, actual_commit=revision, crc_source=str(sdk_crc),
                 crc_source_sha256=sha(crc_source), partition_config=str(sdk_config),
                 partition_config_sha256=sha(config_source)),
        map=[dict(name='partition_table0', start=0xe000, size=0x1000, **validation[0]),
             dict(name='partition_table1', start=0xf000, size=0x1000, **validation[1]),
             dict(name='FW0', start=0x10000, size=0x700000, active=True,
                  populated_bytes=len(original), physical_entry=0x11000),
             dict(name='data_hardcoded_by_ROM', start=0x710000, size=0x10000,
                  contents='erased; no filesystem or credentials'),
             dict(name='FW1', start=0x7b0000, size=0x700000, active=False,
                  contents='erased'),
             dict(name='resource_hardcoded_by_ROM', start=0xeb0000, size=0x140000,
                  end_exclusive=0xff0000,
                  contents='erased; mounted/autoformatted before /data')],
        assumptions=[
            'Explicit 16MiB synthetic NOR; virtual JEDEC1840ef must be selected by host fixture mode.',
            'Legacy JEDEC1740ef and synthetic OTP are independent model assumptions; generator changes neither.',
            'No physical Boot2 backup: bytes0..dfff remain FF; PEC functionality is supplied separately.',
            'Partition version2 follows fixed SDK config; table0 age1/table1 age0 deliberately selects table0.',
            'Only required FW table entry exists, type0/device0/active0; normal image len field0 follows SDK config.',
            'Data extent is ROM hardcoded; no unknown AP01 DATA name/type or other partition entries invented.',
            'All unpopulated areas including FW1/resource/crash/OTA remain erased FF, not captured device content.',
            'Original board init hardcodes capacity16MiB and /data710000..720000; this path has no JEDEC-size branch.',
            'Original partition init mounts /resourceEB0000..FF0000 before caller mounts /data; first eraseEB0000 is resource autoformat.',
            'Selected running 0041 BIN begins at physical10000, its entry at physical11000 maps to XIPa0000000.',
            'Littlefs autoformat may program the synthetic data/resource areas only through the host NOR model.'
        ],
        validator_evidence=dict(original_va='0xa0058554..0xa00585e0',
                                original_window_sha256=sha(original[0x59554:0x595e0]),
                                header_crc_range=[0, 12], count_offset=6,
                                entries_offset=16, entry_stride=36,
                                entries_crc_offset='16 + count*36',
                                independent_bitwise_sdk_crc=True,
                                header_and_entry_corruption_rejected=True),
        storage_layout_evidence=dict(
            data_setup_va='0xa0055b94..0xa0055bb6',
            data_setup_window_sha256=sha(original[0x56b94:0x56bb6]),
            data_start_blocks=0x7100, data_block_count=0x100, mtd_block_bytes=256,
            capacity_written=0x1000000,
            resource_setup_va='0xa0059afa..0xa0059b4e',
            resource_setup_window_sha256=sha(original[0x5aafa:0x5ab4e]),
            resource_start_formula='0xeb0000 / geometry.blocksize',
            resource_count_formula='0x140000 / geometry.blocksize',
            mount_order=['partition_init /resource autoformat', 'board_init /data autoformat'],
            jedec_conditional_observed=False))
    # Exclusive creation also protects against races after initial existence checks.
    with output.open('xb') as stream:
        stream.write(flash)
    reread = output.read_bytes()
    if len(reread) != FLASH_SIZE or sha(reread) != manifest['fixture']['sha256']:
        raise RuntimeError('fixture readback verification failed')
    with manifest_path.open('x', encoding='utf-8') as stream:
        json.dump(manifest, stream, indent=2)
        stream.write('\n')
    print(json.dumps(dict(fixture=str(output), manifest=str(manifest_path),
                         size=FLASH_SIZE, sha256=manifest['fixture']['sha256'],
                         partition_tables=validation), indent=2))


if __name__ == '__main__':
    main()
