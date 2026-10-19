"""Focused offline checks for the 0031 maintenance ROM; no device access."""
from pathlib import Path
import hashlib, importlib.util, json, subprocess

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / 'artifacts/ap01-base0031-20261019'
FW = ROOT.parent / 'cuktech-ap01-firmware'
SRC = FW / 'tooling/firmware-modules'

def run(args):
    result = subprocess.run(list(map(str, args)), capture_output=True, text=True, timeout=60)
    if result.returncode:
        raise RuntimeError(result.stdout[-2000:] + result.stderr[-4000:])
    return result.stdout

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

report = {'passed': False, 'device_accessed': False, 'cases': {}}
for name, files, inputs in (
    ('status', ['test_diagnostic_status_0031.c', 'diagnostic_console.c'], []),
    ('image', ['test_ota_image.c', 'ota_image.c'], [
        OUT/'ap01-base0031-v2-rom.bin', FW/'roms/0031/ap01-1.0.2_0031.bin']),
):
    binary = OUT / ('host-' + name)
    run(['gcc', '-std=c11', '-Wall', '-Wextra', '-Werror', '-O1', '-g',
         '-fsanitize=address,undefined', '-fno-pie', '-no-pie',
         *[SRC/f for f in files], '-o', binary])
    report['cases'][name] = {'output': run([binary, *inputs]), 'binary_sha256': digest(binary)}

spec = importlib.util.spec_from_file_location('builder', FW/'tooling/scripts/build-ota-expanded-0031.py')
builder = importlib.util.module_from_spec(spec)
spec.loader.exec_module(builder)
image = (OUT/'ap01-base0031-v2-rom.bin').read_bytes()
base = (FW/'roms/0031/ap01-1.0.2_0031.bin').read_bytes()
manifest = OUT/'ap01-base0031-v2-rom.manifest.json'
builder.validate_candidate(image, manifest)
assert builder.recovery_markers(image) == [base.find(b'0x5245434f56455259544147'), len(image)-40]
assert image[:256] == base[:256]
assert image[builder.CLOUD_OFFSET:builder.CLOUD_OFFSET+4] == builder.CLOUD_ORIGINAL
start = builder.legacy.GIF_START
assert image[start:start+385834] == base[start:start+385834]
for where in (0, 252, 1024, len(image)-40, len(image)-8, len(image)-4):
    changed = bytearray(image)
    changed[where] ^= 1
    try:
        builder.validate_candidate(changed, manifest)
    except ValueError:
        pass
    else:
        raise AssertionError('altered candidate accepted')
# A complete literal reintroduced into any future module must fail the build gate.
changed = bytearray(image)
changed[builder.TAIL_OFFSET+300:builder.TAIL_OFFSET+324] = b'0x5245434f56455259544147'
try:
    builder.recovery_markers(changed)
except ValueError:
    pass
else:
    raise AssertionError('extra recovery marker accepted')
report.update(passed=True, image_sha256=hashlib.sha256(image).hexdigest(),
              image_bytes=len(image), recovery_marker_offsets=builder.recovery_markers(image),
              gif_sha256=hashlib.sha256(image[start:start+385834]).hexdigest(),
              original_header_and_native_ota_preserved=True,
              source_sha256={p.name: digest(p) for p in SRC.glob('*.c')
                             if p.name in ('ota_0031.c','ota_image.c','diagnostic_console_0031.c',
                                           'test_diagnostic_status_0031.c','test_ota_0031_log.c')})
(OUT/'host-checks.json').write_text(json.dumps(report, indent=2), encoding='utf-8')
print(json.dumps({k: v for k, v in report.items() if k != 'cases'}))
