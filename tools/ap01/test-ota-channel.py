#!/usr/bin/env python3
"""Build and run standalone OTA protocol fault tests with ASan/UBSan."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--cc', default='gcc')
    args = parser.parse_args()
    output = args.output.resolve()
    binary = output.with_suffix('.host')
    if output.exists() or binary.exists() or not output.parent.is_dir():
        parser.error('new report/binary names in an existing directory required')
    root = Path(__file__).resolve().parents[2] / 'firmware' / 'ap01'
    sources = [root / 'ota_channel.c', root / 'test_ota_channel.c']
    flags = ['-std=c11', '-Wall', '-Wextra', '-Werror', '-Wpedantic', '-O1', '-g',
             '-fsanitize=address,undefined', '-fno-omit-frame-pointer', '-fno-pie', '-no-pie']
    report = dict(device_accessed=False, complete_rom_verified=False,
                  sanitizer_flags=flags, source_sha256={p.name: hashlib.sha256(p.read_bytes()).hexdigest()
                      for p in sources + [root / 'ota_channel.h']})
    try:
        built = subprocess.run([args.cc, *flags, *map(str, sources), '-o', str(binary)],
                               capture_output=True, text=True, timeout=30)
        if built.returncode:
            raise RuntimeError('compile: ' + built.stderr[-4000:])
        env = dict(os.environ, ASAN_OPTIONS='detect_leaks=1:halt_on_error=1',
                   UBSAN_OPTIONS='halt_on_error=1:print_stacktrace=1')
        run = subprocess.run([str(binary)], capture_output=True, text=True,
                             timeout=20, env=env)
        if run.returncode or run.stderr:
            raise RuntimeError('fault suite: ' + run.stderr[-4000:])
        report.update(json.loads(run.stdout), binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest())
    except Exception as exc:
        report.update(passed=False, error=f'{type(exc).__name__}: {exc}')
    with output.open('x') as stream:
        json.dump(report, stream, indent=2)
        stream.write('\n')
    print(json.dumps(report))
    return 0 if report['passed'] else 1


if __name__ == '__main__':
    sys.exit(main())
