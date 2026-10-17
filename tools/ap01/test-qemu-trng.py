#!/usr/bin/env python3
"""Native synthetic TRNG fixture regression; no original ROM or physical entropy."""
import argparse
from contextlib import contextmanager
import hashlib
import importlib.util
import json
from pathlib import Path
import struct
import subprocess
import sys
import time
import uuid

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location('probe', Path(__file__).with_name('run-qemu-ap01.py'))
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)
CTRL = 0x20004200


def expect(ok, message):
    if not ok:
        raise AssertionError(message)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--qemu', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    qemu, output = args.qemu.resolve(), args.output.resolve()
    if output.exists() or not output.parent.is_dir() or not Path('/tmp').is_dir():
        parser.error('requires Linux /tmp and a new output in an existing directory')
    token = 'ap01-trng-' + uuid.uuid4().hex
    image = output.parent / (token + '.bin')
    report = dict(synthetic_fixture=True, original_rom_accessed=False,
                  full_rom_boot_verified=False, physical_entropy_verified=False,
                  qemu_sha256=hashlib.sha256(qemu.read_bytes()).hexdigest(), checks=[])
    with output.open('x', encoding='utf-8') as result:
        try:
            with image.open('xb') as file:
                file.write(bytes(4096) + b'\x13\x00\x00\x00')

            @contextmanager
            def machine(enabled):
                suffix = '-on' if enabled else '-off'
                gdb, qp = (Path('/tmp') / (token + suffix + ext) for ext in ('.gdb', '.qmp'))
                command = [str(qemu), '-M', 'ap01,boot-mmio=on,boot2-mock=on' +
                           (',trng-mock=on' if enabled else ''), '-cpu', 'rv32',
                           '-kernel', str(image), '-S', '-display', 'none', '-serial', 'none',
                           '-monitor', 'none', '-icount', 'shift=0,align=off,sleep=off',
                           '-gdb', f'unix:{gdb},server=on,wait=off',
                           '-qmp', f'unix:{qp},server=on,wait=off']
                report.setdefault('commands', []).append(command)
                process = gs = qs = qm = None
                with (output.parent / (token + suffix + '.log')).open('x') as log:
                    try:
                        deadline = time.monotonic() + 30
                        process = subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=log,
                                                   stderr=log, preexec_fn=probe.child_limits)
                        gs = probe.connect(gdb, process, deadline)
                        qs = probe.connect(qp, process, deadline)
                        rsp, qm = probe.RSP(gs, deadline), probe.QMP(qs, deadline)
                        rsp.request('?')
                        yield rsp, qm
                    finally:
                        if qm is not None:
                            qm.file.close()
                        for sock in (gs, qs):
                            if sock is not None:
                                sock.close()
                        if process is not None:
                            if process.poll() is None:
                                process.terminate()
                                try:
                                    process.wait(timeout=2)
                                except subprocess.TimeoutExpired:
                                    process.kill(); process.wait(timeout=2)
                            for path in (gdb, qp):
                                if path.exists():
                                    path.unlink()

            def reject(rsp, request):
                try:
                    rsp.request(request)
                except RuntimeError as exc:
                    expect(str(exc).startswith('RSP '), 'expected target rejection')
                else:
                    raise AssertionError('unexpectedly accepted: ' + request)

            with machine(False) as (rsp, qm):
                reject(rsp, f'm{CTRL:x},4')
                report['checks'].append(dict(name='explicit default-off gate', passed=True))

            with machine(True) as (rsp, qm):
                def read(address=CTRL):
                    return int.from_bytes(bytes.fromhex(rsp.request(f'm{address:x},4')), 'little')

                def write(value, address=CTRL):
                    expect(rsp.request(f'M{address:x},4:' + value.to_bytes(4, 'little').hex()) == 'OK', 'write')

                def block():
                    return bytes.fromhex(rsp.request(f'm{CTRL + 8:x},20'))

                def native(words, value=0x806):
                    start = 0x62fc4400
                    code = struct.pack('<' + 'I' * len(words), *words)
                    expect(rsp.request(f'M{start:x},{len(code):x}:' + code.hex()) == 'OK', 'guest code')
                    for reg, val in ((10, CTRL), (11, value), (32, start)):
                        expect(rsp.request(f'P{reg:x}=' + val.to_bytes(4, 'little').hex()) == 'OK', 'set register')
                    end = start + len(code)
                    expect(rsp.request(f'Z0,{end:x},4') == 'OK', 'breakpoint')
                    try:
                        expect(rsp.request('c').startswith(('S', 'T')), 'native stop')
                        raw = bytes.fromhex(rsp.request('g'))
                        expect(int.from_bytes(raw[128:132], 'little') == end, 'native code trapped')
                        return struct.unpack_from('<2I', raw, 12 * 4)
                    finally:
                        rsp.request(f'z0,{end:x},4')

                # 2000 iterations =4001 guest instructions, advancing4us.
                delay = [0x7d000293, 0xfff28293, 0xfe029ee3]

                def check(name, fn):
                    fn()
                    report['checks'].append(dict(name=name, passed=True))

                def initial():
                    expect(read() == 0x800 and block() == bytes(32), 'reset fixture')
                check('initial masked control and cleared output', initial)

                first = None

                def sample():
                    nonlocal first
                    write(0x804)
                    before, after = native([0x00b52023, 0x00052603] + delay + [0x00052683])
                    expect(before & 1 and not before & 0x100, 'native immediate busy')
                    expect(not after & 1 and after & 0x100, 'native finite completion')
                    first = block()
                    expect(first != bytes(32), 'nonzero deterministic fixture')
                    for i in range(8):
                        word = int.from_bytes(first[4*i:4*i+4], 'little')
                        expect(all(read(CTRL + 8 + 4*i) == word for _ in range(4)), 'stable repeated data reads')
                    report['first_block_sha256'] = hashlib.sha256(first).hexdigest()
                check('native trigger busy completion and eight stable words', sample)

                def another():
                    write(0xa06)  # ACK does not regenerate output.
                    expect(not read() & 0x100 and block() == first, 'ACK only clears interrupt')
                    write(0x804); native([0x00b52023] + delay)
                    expect(block() != first, 'next trigger produces different block')
                    write(0x80c)
                    expect(block() == bytes(32), 'output-clear pulse')
                check('ACK clear, explicit rearm, different block and data clear', another)

                def invalid():
                    write(0x804)
                    before = (read(), block())
                    for off, size in ((0, 1), (0, 2), (1, 4), (4, 4), (0x28, 4), (0x100, 4)):
                        reject(rsp, f'm{CTRL + off:x},{size:x}')
                    for value in (4, 0xc04, 0x2804, 0x4804, 0x8804, 0x840, 0x802, 0x80e):
                        reject(rsp, f'M{CTRL:x},4:' + value.to_bytes(4, 'little').hex())
                    reject(rsp, f'M{CTRL + 8:x},4:ffffffff')
                    write(0x914)  # BUSY/HT_ERROR/INT read-only writes cannot assert them.
                    expect((read(), block()) == before, 'RO write and invalid requests preserve state')
                check('width address reserved manual IRQ and RO atomic boundaries', invalid)

                def cancel():
                    write(0x806)
                    expect(read() & 1, 'busy before cancellation')
                    before = read()
                    reject(rsp, f'M{CTRL:x},4:04080000')  # Clear trigger while busy.
                    expect(read() == before, 'busy rejected write atomic')
                    write(0x800)
                    native(delay)
                    expect(read() == 0x800 and block() == bytes(32), 'disabled transaction cannot complete later')
                check('busy mutation rejected and disable cancels transaction', cancel)

                def reset():
                    write(0x806)
                    qm.command('system_reset')
                    expect(read() == 0x800 and block() == bytes(32), 'warm reset cancels pending')
                    native([0x00b52023] + delay)
                    expect(block() == first, 'warm reset reproduces fixed public seed')
                check('reset cancellation and reproducible public seed', reset)
            report['passed'] = True
        except Exception as exc:
            report.update(passed=False, error=f'{type(exc).__name__}: {exc}')
        json.dump(report, result, indent=2)
        result.write('\n')
    print(json.dumps(report))
    return 0 if report['passed'] else 1


if __name__ == '__main__':
    sys.exit(main())
