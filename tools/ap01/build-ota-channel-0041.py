#!/usr/bin/env python3
"""Build a bounded 0041 OTA-channel simulation candidate; never flash a device."""
import argparse
import base64
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import secrets
import struct
import subprocess
import sys
import zlib

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
BASE = 0x9ffff000
OFFSET = 0x1c7000
LIMIT = 0x1cba6c
GIF_START, GIF_END = 0x1c5a6c, 0x223d96
BASE_SHA = '1bbe8f45359eb7cdb55f337643f3ce28894999ffb21beab92fb62e995cd4854d'
GIF_SHA = '5e656788665f7f0022b0b5500d569a8aae07f4a46b080d033b3cacfa4abc940d'
spec = importlib.util.spec_from_file_location('unsigned_patch', Path(__file__).with_name('patch_0041_unsigned_ota.py'))
patch = importlib.util.module_from_spec(spec)
spec.loader.exec_module(patch)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def read_base(original, candidate):
    source = original.read_bytes()
    expected, _ = patch.patch_bytes(source)
    data = candidate.read_bytes()
    if data != expected or sha(data) != BASE_SHA:
        raise ValueError('requires the exact frozen unsigned-OTA candidate and matching original')
    if sha(data[GIF_START:GIF_END]) != GIF_SHA:
        raise ValueError('pet GIF extent identity mismatch')
    return data


def assemble(base, payload, gif):
    if len(payload) <= 256 or OFFSET + len(payload) > LIMIT:
        raise ValueError('payload exceeds the bounded first OTA code extent')
    if len(gif) > OFFSET - GIF_START or gif[:6] not in (b'GIF87a', b'GIF89a') or gif[-1:] != b';':
        raise ValueError('invalid bounded placeholder GIF')
    if struct.unpack_from('<HH', gif, 6) != (320, 240):
        raise ValueError('placeholder GIF must preserve original dimensions')
    entry, = struct.unpack_from('<I', payload, 24)
    if not BASE + OFFSET + 256 <= entry < BASE + OFFSET + len(payload) or entry & 1:
        raise ValueError('builtin entry outside injected code')
    out = bytearray(base)
    # Keep the GIF valid and reduce its descriptor length before reusing bytes.
    out[GIF_START:GIF_START + len(gif)] = gif
    struct.pack_into('<I', out, 0x1c5a5c, len(gif))
    out[OFFSET:OFFSET + len(payload)] = payload
    struct.pack_into('<I', out, 0x1be990, entry)
    # Preserve all original applications; add the independent channel in rcS.
    rcs = b'set +e\nifup wlan0\nmiio_client &\nmible_mesh_common_bt &\nlvgl_ui_main &\nsetlogmask &\nexit\n'
    if len(rcs) > 97:
        raise ValueError('rcS exceeds original ROMFS extent')
    out[0x1bef04:0x1bef04 + 97] = rcs.ljust(97, b'\n')
    struct.pack_into('>I', out, 0x1bee30, 0)
    words = struct.unpack('>120I', out[0x1bee24:0x1bf004])
    struct.pack_into('>I', out, 0x1bee30, (-sum(words)) & 0xffffffff)
    # Exclusive local OTA: reject the original cloud OTA dispatcher as busy.
    # c.li a0,-16; c.jr ra. No frame has been established at function entry.
    out[0x1168f6:0x1168fa] = bytes.fromhex('41558280')
    struct.pack_into('<I', out, len(out) - 4, patch.recovery_crc(out))
    patch.image_metadata(bytes(out), require_identity=False)
    allowed = [(GIF_START, GIF_START + len(gif)), (0x1c5a5c, 0x1c5a60),
               (OFFSET, OFFSET + len(payload)), (0x1be990, 0x1be994),
               (0x1bef04, 0x1bef65), (0x1bee30, 0x1bee34),
               (0x1168f6, 0x1168fa), (len(out) - 4, len(out))]
    for i, (before, after) in enumerate(zip(base, out)):
        if before != after and not any(a <= i < b for a, b in allowed):
            raise ValueError(f'unexpected modification at {i:#x}')
    return bytes(out), allowed


def validate_candidate(data, manifest_path):
    """Reconstruct every changed byte; a manifest alone cannot bypass identity."""
    m = json.loads(Path(manifest_path).read_text(encoding='utf-8'))
    if m.get('format') != 'ap01-ota-channel-0041-v1':
        raise ValueError('unsupported OTA build manifest')
    base = read_base(Path(m['original']['path']), Path(m['base']['path']))
    payload = Path(m['payload']['path']).read_bytes()
    gif = base64.b64decode(m['placeholder_gif']['base64'], validate=True)
    if sha(payload) != m['payload']['sha256'] or sha(gif) != m['placeholder_gif']['sha256']:
        raise ValueError('payload/GIF identity mismatch')
    header = bytearray(payload[:256])
    stored, = struct.unpack_from('<I', header, 32)
    struct.pack_into('<I', header, 32, 0)
    if payload[:8] != b'AP01OTA1' or zlib.crc32(header) != stored:
        raise ValueError('OTA manifest CRC mismatch')
    if struct.unpack_from('<I', payload, 20)[0] != len(payload) or zlib.crc32(payload[256:]) != struct.unpack_from('<I', payload, 28)[0]:
        raise ValueError('OTA code CRC/length mismatch')
    expected, _ = assemble(base, payload, gif)
    if data != expected or sha(data) != m['image']['sha256']:
        raise ValueError('image does not equal the bounded reconstructed OTA build')
    return m


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--original', type=Path, required=True)
    p.add_argument('--base', type=Path, required=True)
    p.add_argument('--payload', type=Path, required=True, help='zero-manifest linked binary')
    p.add_argument('--elf', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    p.add_argument('--auth-key-file', type=Path, help='reuse an existing64hex key without logging it')
    p.add_argument('--label', default='ota1')
    a = p.parse_args()
    output = a.output.resolve()
    paths = [output, output.with_suffix('.manifest.json'), output.with_suffix('.payload.bin')]
    if not a.auth_key_file:
        paths.append(output.with_suffix('.auth-key'))
    if not output.parent.is_dir() or any(x.exists() for x in paths):
        p.error('all outputs must be new files in an existing directory')
    base = read_base(a.original.resolve(), a.base.resolve())
    payload = bytearray(a.payload.read_bytes())
    if payload[:256] != bytes(256):
        p.error('linked input must contain exactly the reserved zero manifest')
    tool = ROOT.parent / 'cuktech-ap01-firmware-tools/tools/docker/toolchain/bin/riscv64-unknown-elf-nm'
    listing = subprocess.run([str(tool), '-n', str(a.elf)], check=True, capture_output=True, text=True, timeout=10).stdout
    symbols = {line.split()[2]: int(line.split()[0], 16) for line in listing.splitlines() if len(line.split()) == 3 and line.split()[0] != 'U'}
    entry = symbols['ota_0041_main']
    if symbols['ota_0041_manifest'] != BASE + OFFSET:
        p.error('ELF manifest mapping mismatch')
    key = a.auth_key_file.read_text(encoding='ascii').strip() if a.auth_key_file else secrets.token_hex(32)
    if len(key) != 64 or any(c not in '0123456789abcdefABCDEF' for c in key):
        p.error('auth key must contain exactly64 hex characters')
    m = bytearray(256)
    m[:8] = b'AP01OTA1'
    struct.pack_into('<7I', m, 8, 1, len(base), OFFSET, len(payload), entry, zlib.crc32(payload[256:]), 0)
    m[36:52] = hashlib.sha256(payload[256:] + a.label.encode('utf-8')).digest()[:16]
    struct.pack_into('<3I', m, 52, 8192, 18080, 0)
    m[64:129] = key.encode('ascii') + b'\0'
    struct.pack_into('<I', m, 32, zlib.crc32(m))
    payload[:256] = m
    from PIL import Image
    stream = io.BytesIO()
    Image.new('P', (320, 240), 0).save(stream, format='GIF', optimize=True)
    gif = stream.getvalue()
    decoded = Image.open(io.BytesIO(gif)); decoded.load()
    if decoded.size != (320, 240) or any(decoded.tobytes()):
        raise ValueError('independent placeholder GIF decode mismatch')
    data, allowed = assemble(base, payload, gif)
    manifest = dict(format='ap01-ota-channel-0041-v1', simulation_candidate=True,
                    flashable=False, physical_device_verified=False,
                    original=dict(path=str(a.original.resolve()), sha256=patch.IMAGE_SHA256),
                    base=dict(path=str(a.base.resolve()), sha256=BASE_SHA),
                    image=dict(path=str(output), bytes=len(data), sha256=sha(data)),
                    payload=dict(path=str(paths[2]), bytes=len(payload), sha256=sha(payload),
                                 entry=hex(entry), elf=str(a.elf.resolve()), elf_sha256=sha(a.elf.read_bytes())),
                    build_id=bytes(m[36:52]).hex(), auth_key_sha256=sha(key.encode('ascii')),
                    placeholder_gif=dict(bytes=len(gif), sha256=sha(gif), base64=base64.b64encode(gif).decode('ascii')),
                    allowed_ranges=[dict(start=x, end_exclusive=y) for x, y in allowed],
                    assumptions=['original setlogmask builtin replaced; original UI retained with blank pet0 GIF',
                                 'cloud miIO.ota returns busy to exclude concurrent firmware installation',
                                 'fixed0041 format/size; future image must retain a valid OTA manifest and startup hook',
                                 'no Boot2 or physical Wi-Fi equivalence claim; no physical flash performed'])
    for path, content in ((paths[2], bytes(payload)), (output, data)):
        with path.open('xb') as f: f.write(content)
    with paths[1].open('x', encoding='utf-8') as f: json.dump(manifest, f, indent=2); f.write('\n')
    if not a.auth_key_file:
        with paths[3].open('x', encoding='ascii') as f: f.write(key + '\n')
        paths[3].chmod(0o600)
    validate_candidate(data, paths[1])
    print(json.dumps(dict(image=str(output), bytes=len(data), sha256=sha(data),
                          build_id=manifest['build_id'], manifest=str(paths[1]), flashable=False)))


if __name__ == '__main__':
    main()
