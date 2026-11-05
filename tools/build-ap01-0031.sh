#!/usr/bin/env bash
# Integration only: all ROM patching/validation belongs to the firmware repository.
set -euo pipefail
firmware=$(realpath "${1:?Firmware checkout required}")
out=$(realpath "${2:?New output directory required}")
mkdir -p "$out/objects"
scripts="$firmware/tooling/scripts"
modules="$firmware/tooling/firmware-modules"
python3 -B "$scripts/test-access-menu-0031.py"
cc -std=c11 -Wall -Wextra -Werror -fsanitize=address,undefined -fno-pie -no-pie \
  "$modules/test_access_menu_0031.c" "$modules/device_code_0031.c" \
  "$modules/ota_channel.c" -o "$out/objects/menu-check"
"$out/objects/menu-check"
bash "$scripts/build-ota-channel-0031.sh" ap01-base "$out/objects" 0xa067d000 ap01-http/1 release
python3 -B - "$firmware" "$out" <<'PY'
import hashlib, importlib.util, json, pathlib, struct, subprocess, sys
firmware, out = map(pathlib.Path, sys.argv[1:])
builder = firmware / 'tooling/scripts/build-ota-expanded-0031.py'
rom = out / 'ap01-base-0031.bin'
subprocess.run([sys.executable, '-B', str(builder), '--original', str(firmware / 'roms/0031/ap01-1.0.2_0031.bin'),
    '--payload', str(out / 'objects/ap01-base.bin'), '--elf', str(out / 'objects/ap01-base.elf'),
    '--output', str(rom), '--mode', 'http', '--label', 'ap01-base-0031'], check=True)
spec = importlib.util.spec_from_file_location('builder', builder)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
manifest = rom.with_suffix('.manifest.json')
data = rom.read_bytes()
module.validate_candidate(data, manifest)
m = json.loads(manifest.read_text())
payload = rom.with_suffix('.payload.bin').read_bytes()
assert struct.unpack_from('<I', payload, 8)[0] == 2
assert payload[64:192] == bytes(128), 'ROM contains obsolete credential area'
assert m['credential_source'] == 'device-menu-8digit' and m['per_device_credential']
assert m['independent_ota_enabled'] and not m['cloud_ota_dispatch_modified']
assert m['mode'] == 'http' and m['original_gif_preserved'] and m['native_recovery_compatible']
original = (firmware / 'roms/0031/ap01-1.0.2_0031.bin').read_bytes()
gif_start, gif_desc = module.legacy.GIF_START, module.legacy.GIF_DESC
gif_size = struct.unpack_from('<I', original, gif_desc)[0]
assert data[gif_desc:gif_desc+4] == original[gif_desc:gif_desc+4]
assert data[gif_start:gif_start+gif_size] == original[gif_start:gif_start+gif_size], 'Original cat GIF changed'
assert not m['flashable'] and not m['physical_device_verified']
for offset in (0, 0xbe8da, 0xbe9e6, len(data)-1):
    bad = bytearray(data); bad[offset] ^= 1
    try: module.validate_candidate(bytes(bad), manifest)
    except ValueError: pass
    else: raise AssertionError(f'Corruption accepted at {offset:#x}')
provenance = dict(firmware_commit=subprocess.check_output(['git', '-C', str(firmware), 'rev-parse', 'HEAD'], text=True).strip(),
    toolchain=subprocess.check_output(['riscv64-unknown-elf-gcc', '--version'], text=True).splitlines()[0],
    original_sha256=m['original']['sha256'], rom_sha256=hashlib.sha256(data).hexdigest(),
    bytes=len(data), credential_source=m['credential_source'], business_app=False,
    layout='http', original_cat_gif_preserved=True, hardware_verified=False, corruption_cases=4)
(out / 'ap01-0031-provenance.json').write_text(json.dumps(provenance, indent=2)+'\n')
with (out / 'ap01-0031-SHA256SUMS').open('x') as f:
    for p in sorted(out.iterdir()):
        if p.is_file() and p.name != 'ap01-0031-SHA256SUMS':
            f.write(hashlib.sha256(p.read_bytes()).hexdigest()+'  '+p.name+'\n')
print(json.dumps(provenance))
PY
