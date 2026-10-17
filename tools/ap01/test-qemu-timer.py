#!/usr/bin/env python3
"""Synthetic shared TIMER/WDG native tests; no original ROM/device access."""
import argparse
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import time
import uuid

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location('probe', Path(__file__).with_name('run-qemu-ap01.py'))
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)
BASE, TIMER = 0x20fc1000, 0x2000a500


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
    token = 'ap01-timer-' + uuid.uuid4().hex
    image = output.parent / (token + '.bin')
    gdb, qmp_path = (Path('/tmp') / (token + ext) for ext in ('.gdb', '.qmp'))
    report = dict(synthetic_fixture=True, original_rom_accessed=False,
                  full_rom_boot_verified=False, checks=[])
    process = gs = qs = qmp = None
    with output.open('x', encoding='utf-8') as result:
        try:
            with image.open('xb') as file:
                file.write(bytes(4096) + b'\x13\x00\x00\x00')
            deadline = time.monotonic() + 30
            with (output.parent / (token + '.log')).open('x') as log:
                process = subprocess.Popen(
                    [str(qemu), '-M', 'ap01,boot-mmio=on,boot2-mock=on,clic-mock=on',
                     '-cpu', 'rv32,x-ap01-clic=on', '-kernel', str(image), '-S',
                     '-display', 'none', '-serial', 'none', '-monitor', 'none',
                     '-gdb', f'unix:{gdb},server=on,wait=off',
                     '-qmp', f'unix:{qmp_path},server=on,wait=off'],
                    stdin=subprocess.DEVNULL, stdout=log, stderr=log,
                    preexec_fn=probe.child_limits)
                gs = probe.connect(gdb, process, deadline)
                qs = probe.connect(qmp_path, process, deadline)
                rsp, qmp = probe.RSP(gs, deadline), probe.QMP(qs, deadline)
                rsp.request('?')

                def write(offset, value, size=4, absolute=False):
                    address = offset if absolute else TIMER + offset
                    expect(rsp.request(f'M{address:x},{size:x}:' + value.to_bytes(size, 'little').hex()) == 'OK', 'MMIO write')

                def read(offset, size=4, absolute=False):
                    address = offset if absolute else TIMER + offset
                    return int.from_bytes(bytes.fromhex(rsp.request(f'm{address:x},{size:x}')), 'little')

                def setreg(number, value):
                    expect(rsp.request(f'P{number:x}=' + value.to_bytes(4, 'little').hex()) == 'OK', 'setreg')

                def pc():
                    raw = bytes.fromhex(rsp.request('g'))
                    return int.from_bytes(raw[128:132], 'little')

                def run(words):
                    for index, word in enumerate(words + [0x13]):
                        write(BASE + index * 4, word, absolute=True)
                    end = BASE + len(words) * 4
                    setreg(32, BASE)
                    expect(rsp.request(f'Z0,{end:x},4') == 'OK', 'breakpoint')
                    try:
                        expect(rsp.request('c').startswith(('T', 'S')) and pc() == end, 'guest continuation')
                    finally:
                        rsp.request(f'z0,{end:x},4')

                def unlock():
                    write(0x9c, 0xbaba, 2)
                    write(0xa0, 0xeb10, 2)

                def protected(offset, value, size=4):
                    unlock()
                    write(offset, value, size)

                def rejected(request):
                    try:
                        rsp.request(request)
                    except RuntimeError as exc:
                        expect(str(exc).startswith('RSP '), 'wrong rejection')
                    else:
                        raise AssertionError('unsupported operation accepted: ' + request)

                def irq(raw):
                    return read(0xe0801000 + 4 * raw, 1, absolute=True) & 1

                write(0x20009014, 0xc000001f, absolute=True)
                write(0x20009014, 0x8000001f, absolute=True)

                def advance(us):
                    start = read(0xe000bff8, absolute=True)
                    setreg(5, 0xe000bff8)
                    setreg(7, start + us)
                    run([0x0002a303, 0xfe736ee3])
                    return read(0xe000bff8, absolute=True) - start

                def check(name, function):
                    report['active_check'] = name
                    function()
                    report['checks'].append(dict(name=name, passed=True))

                def boundaries():
                    expect(read(0x64) == 0 and read(0x68, 2) == 0xffff and read(0x6c, 2) == 0, 'WDG cold state')
                    rejected('M2000a564,4:01000000')
                    rejected('M2000a5a0,2:10eb')
                    write(0x9c, 0xbaba, 2)
                    rejected('M2000a5a0,2:0000')
                    rejected('M2000a568,2:1400')
                    for request in ('M2000a59c,1:ba', 'M2000a59d,2:baba',
                                    'm2000a56c,1', 'm2000a590,4', 'M2000a500,4:00040000'):
                        rejected(request)
                    unlock()
                    expect(read(0x64) == 0, 'read between key pair and protected write')
                    write(0x68, 20, 2)
                    rejected('M2000a564,4:01000000')
                    expect(read(0x68, 2) == 20, 'one-write key window or rejection polluted match')
                    unlock()
                    rejected('M2000a568,4:00000100')  # Unsupported WDT align.
                    write(0x68, 20, 2)
                check('WDG keys, protected writes, widths and unsupported modes', boundaries)

                def wdg():
                    write(0x00, 1 << 8)
                    write(0xbc, 31 << 24)
                    protected(0x98, 1)
                    protected(0x64, 1)
                    advance(5000)
                    count = read(0x6c, 2)
                    expect(4 <= count <= 7 and not irq(54), 'WDG count or early IRQ')
                    protected(0x64, 0)
                    frozen = read(0x6c, 2)
                    advance(10000)
                    expect(read(0x6c, 2) == frozen, 'stopped WDG advanced')
                    protected(0x64, 1)
                    protected(0x98, 1)
                    expect(read(0x6c, 2) == 0, 'feed did not clear counter')
                    advance(25000)
                    expect(irq(54), 'WDG expiry did not assert raw54')
                    protected(0x98, 1)
                    expect(irq(54), 'feed incorrectly cleared latched IRQ')
                    protected(0x80, 1)
                    expect(not irq(54), 'WICR did not lower IRQ')
                    advance(5000)
                    expect(not irq(54), 'feed deadline did not move')
                    protected(0x64, 0)
                    protected(0x80, 1)
                    advance(25000)
                    expect(not irq(54), 'stop did not cancel callback')
                    write(0x00, 2 << 8)
                    write(0xbc, 1 << 24)
                    protected(0x98, 1)
                    protected(0x68, 0xffff, 2)
                    protected(0x64, 1)
                    advance(20000)
                    expect(9 <= read(0x6c, 2) <= 12, 'WDG divider+1/source1k')
                    write(0x00, 5 << 8)
                    frozen = read(0x6c, 2)
                    advance(10000)
                    expect(read(0x6c, 2) == frozen, 'NO clock advanced WDG')
                    protected(0x64, 0)
                check('WDG virtual clock, divider, feed, IRQ, clear and cancellation', wdg)

                def channels():
                    write(0x00, (5 << 8) | 0x22)
                    write(0xbc, 0)
                    write(0x88, 6)
                    write(0x10, 8)
                    write(0x14, 18)
                    write(0x1c, 13)
                    write(0x44, 1)
                    write(0x48, 1)
                    write(0x84, 0x66)
                    advance(5000)
                    expect(not irq(52) and not irq(53), 'timer early IRQ')
                    expect(4 <= read(0x2c) <= 7 and 4 <= read(0x30) <= 7, 'independent timer counters')
                    advance(7000)
                    expect(read(0x38) & 1 and irq(52) and not irq(53), 'channel0 compare+2/IRQ')
                    write(0x44, 0)
                    expect(not irq(52) and read(0x38) & 1, 'mask must preserve pending')
                    write(0x44, 1)
                    expect(irq(52), 'unmask must restore pending IRQ')
                    write(0x78, 1)
                    expect(not irq(52), 'channel0 W1C did not lower IRQ')
                    advance(5000)
                    expect(irq(53), 'channel1 independent compare/IRQ')
                    write(0x84, 0)
                    frozen = read(0x30)
                    advance(5000)
                    expect(read(0x30) == frozen, 'channel stop did not freeze')
                    write(0x7c, 7)
                    write(0x88, 0)
                    write(0x50, 5)
                    write(0x5c, 1)
                    write(0x10, 13)
                    write(0x78, 7)
                    write(0x84, 2)
                    advance(25000)
                    expect(5 <= read(0x2c) < 15 and irq(52), 'preload compare did not reload repeatedly')
                    write(0x78, 7)
                    advance(12000)
                    expect(irq(52) and 5 <= read(0x2c) < 15, 'periodic preload did not reassert after clear')
                    write(0x84, 0x20)
                    expect(read(0x2c) == 0 and read(0x84) == 0, 'counter clear pulse latched or failed')
                    write(0x78, 7)
                check('two timer channels, compare+2, IRQ masks/W1C and periodic preload', channels)

                def reset():
                    write(0x00, (2 << 8) | 0x22)
                    write(0xbc, 0)
                    protected(0x98, 1)
                    protected(0x68, 10, 2)
                    protected(0x64, 3)
                    write(BASE, 0x6f, absolute=True)
                    setreg(32, BASE)
                    expect(rsp.request('Z0,a0000000,4') == 'OK', 'reset entry breakpoint')
                    try:
                        expect(rsp.request('c').startswith(('T', 'S')) and pc() == 0xa0000000,
                               'watchdog did not request a real machine reset')
                    finally:
                        rsp.request('z0,a0000000,4')
                    expect(read(0x70) == 1 and read(0x64) == 0 and read(0x6c, 2) == 0,
                           'watchdog reset status or counter state')
                    expect(read(0x84) == 0 and not irq(52) and not irq(53) and not irq(54),
                           'reset left timer/WDG callbacks or IRQs active')
                    qmp.command('system_reset')
                    expect(read(0x70) == 1, 'warm reset lost watchdog reset evidence')
                check('WDG expiry requests machine reset and preserves WSR evidence', reset)
            report['passed'] = True
        except Exception as exc:
            report.update(passed=False, error=f'{type(exc).__name__}: {exc}')
        finally:
            if qmp is not None:
                qmp.file.close()
            for sock in (gs, qs):
                if sock is not None:
                    sock.close()
            if process is not None:
                if process.poll() is None:
                    process.terminate()
                    try:
                        process.wait(timeout=2)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait(timeout=2)
                for path in (gdb, qmp_path):
                    if path.exists():
                        path.unlink()
            json.dump(report, result, indent=2)
            result.write('\n')
    print(json.dumps(report))
    return 0 if report['passed'] else 1


if __name__ == '__main__':
    sys.exit(main())
