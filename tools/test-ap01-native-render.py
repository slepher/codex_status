"""Exercise the native (non-std::string) renderer port in WSL, not on a device.
The caller must run elevated: g++ creates temporary compilation directories.
"""
from pathlib import Path
import hashlib
import json
import subprocess

repo = Path(__file__).resolve().parents[1]
out = repo / 'artifacts/ap01-platform-integration/device'
assert out.is_dir(), 'Precreate the output directory with elevation'

def linux(path):
    path = path.resolve()
    return '/mnt/' + path.drive[0].lower() + path.as_posix()[2:]

sources = ['template_engine.cpp', 'GUI_Paint.cpp', 'font_asset.cpp']
sources += ['font' + str(n) + '.cpp' for n in (8, 12, 16, 20, 24)]
binary = out / 'native-render-smoke'
report_path = out / 'native-render-report.json'
report_path.write_text(json.dumps({'passed': False, 'stage': 'building', 'flashable': False}), encoding='utf-8')
command = ['wsl', '-d', 'Ubuntu-24.04', '--', 'g++', '-std=c++17', '-O2',
           '-DCODEX_AP01_NATIVE=1', '-DCODEX_RENDER_NOTE4_FONTS=1',
           '-DARDUINOJSON_ENABLE_ARDUINO_STRING=1', '-DARDUINOJSON_ENABLE_STD_STRING=0',
           '-DARDUINOJSON_ENABLE_STD_STREAM=0', '-Wno-deprecated-declarations']
for directory in ['ports/ap01', 'bridge/crates/render/shim', 'src',
                  '.pio/libdeps/esp32-s3-epaper-154g/ArduinoJson/src']:
    command += ['-I', linux(repo / directory)]
command += [linux(repo / 'ports/ap01/render_smoke.cpp'), linux(repo.parent / 'cuktech-ap01-firmware/tooling/firmware-modules/native_calendar_0041.cpp')]
command += [linux(repo / 'src' / source) for source in sources]
command += ['-o', linux(binary)]
subprocess.run(command, check=True)

fixture = repo / 'artifacts/ap01-template-integration'
report = {'scope': 'native source/ABI surface on Linux host; not RV32 execution or ROM',
          'flashable': False, 'hardware_verified': False,
          'executable_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(), 'scenes': []}
for scene in ('both-windows', 'weekly-only', 'zero', 'full'):
    frame = out / ('native-' + scene + '.palette4')
    subprocess.run(['wsl', '-d', 'Ubuntu-24.04', '--', 'env', 'TZ=Asia/Shanghai', linux(binary),
                    linux(repo / 'tools/test-bridge/templates/codex-status-ap01.json'),
                    linux(fixture / (scene + '.usage.json')), linux(frame),
                    *[linux(fixture / ('ap01-' + font + '.csfn')) for font in ('labels', 'digits', 'medium')]],
                   check=True)
    expected = (fixture / (scene + '.palette4')).read_bytes()
    actual = frame.read_bytes()
    assert len(actual) == len(expected) == 38400
    different = sum((a >> 4 != b >> 4) + ((a & 15) != (b & 15)) for a, b in zip(actual, expected))
    report['scenes'].append({'scene': scene, 'different_pixels': different,
                            'frame_sha256': hashlib.sha256(actual).hexdigest()})
report['passed'] = all(scene['different_pixels'] == 0 for scene in report['scenes'])
report_path.write_text(json.dumps(report, indent=2), encoding='utf-8')
print(json.dumps(report))
assert report['passed'], 'Native port differs from the frozen shared-render fixture'
