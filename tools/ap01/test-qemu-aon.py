#!/usr/bin/env python3
"""Native AON cap-code tests against a synthetic kernel and explicit ROM shim."""
import argparse
import hashlib
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


def expect(ok, message):
    if not ok:
        raise AssertionError(message)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('qemu', 'rom-shim', 'output'):
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    qemu, shim, output = (p.resolve() for p in (args.qemu, args.rom_shim, args.output))
    if output.exists() or not output.parent.is_dir() or not Path('/tmp').is_dir():
        parser.error('requires Linux /tmp and a new output in an existing directory')
    token = 'ap01-aon-' + uuid.uuid4().hex
    image = output.parent / (token + '.bin')
    gdb, qmpp = (Path('/tmp') / (token + ext) for ext in ('.gdb', '.qmp'))
    report = dict(synthetic_fixture=True, original_rom_accessed=False,
                  full_rom_boot_verified=False, checks=[],
                  qemu_sha256=hashlib.sha256(qemu.read_bytes()).hexdigest(),
                  shim_sha256=hashlib.sha256(shim.read_bytes()).hexdigest())
    process = gs = qs = qmp = None
    with output.open('x', encoding='utf-8') as result:
        try:
            with image.open('xb') as file:
                file.write(bytes(4096) + b'\x13\x00\x00\x00')
            deadline = time.monotonic() + 30
            command = [str(qemu), '-M', 'ap01,boot-mmio=on,boot2-mock=on,rom-shim=' + str(shim),
                       '-cpu', 'rv32', '-kernel', str(image), '-S', '-display', 'none',
                       '-serial', 'none', '-monitor', 'none', '-icount', 'shift=0,align=off,sleep=off',
                       '-gdb', f'unix:{gdb},server=on,wait=off', '-qmp', f'unix:{qmpp},server=on,wait=off']
            report['command'] = command
            with (output.parent / (token + '.log')).open('x') as log:
                process = subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=log, stderr=log,
                                           preexec_fn=probe.child_limits)
                gs = probe.connect(gdb, process, deadline)
                qs = probe.connect(qmpp, process, deadline)
                rsp, qmp = probe.RSP(gs, deadline), probe.QMP(qs, deadline)
                rsp.request('?')

                def read(addr):
                    return int.from_bytes(bytes.fromhex(rsp.request(f'm{addr:x},4')), 'little')

                def write(addr, value):
                    expect(rsp.request(f'M{addr:x},4:' + value.to_bytes(4, 'little').hex()) == 'OK', 'memory write')

                def setreg(reg, value):
                    expect(rsp.request(f'P{reg:x}=' + value.to_bytes(4, 'little').hex()) == 'OK', 'register write')

                def native(slot, a=0, b=0):
                    target, end = read(0x90015800 + slot * 4), 0x62fc4400
                    expect(0x90000000 <= target < 0x90010000, 'native shim table target')
                    setreg(10, a); setreg(11, b); setreg(1, end)
                    setreg(2, 0x62fc6000); setreg(32, target)
                    expect(rsp.request(f'Z0,{end:x},4') == 'OK', 'breakpoint')
                    try:
                        expect(rsp.request('c').startswith(('S', 'T')), 'native continue')
                        raw = bytes.fromhex(rsp.request('g'))
                        expect(int.from_bytes(raw[128:132], 'little') == end, 'native API trapped')
                        return int.from_bytes(raw[40:44], 'little')
                    finally:
                        rsp.request(f'z0,{end:x},4')

                def check(name, fn):
                    fn()
                    report['checks'].append(dict(name=name, passed=True))

                cap, rtc = 0x2000f884, 0x20009014
                mask = 0x0fff0000

                def initial():
                    expect(read(cap) == 0xb410f8f0, 'documented reset value')
                    expect(native(20) == 16, 'getter reset capIn')
                check('reset register and native getter', initial)

                def disabled():
                    for config in (0, 0xc0000027):
                        write(rtc, config)
                        before = read(cap)
                        expect(native(14, 36, 36) == 1 and read(cap) == before, 'disabled/reset RTC must reject atomically')
                check('disabled and held-reset RTC reject setter', disabled)

                # Establish the explicit 40MHz/320MHz virtual clock profile.
                expect(native(115, 4, 3) == 0, 'PLL profile')
                expect(native(144, 5) == 0, 'MCU 320MHz profile')

                def bounds():
                    write(rtc, 0x80000027)  # XTAL40MHz / 40 = 1MHz.
                    for cin, cout in ((0, 63), (63, 0), (36, 36)):
                        before = read(cap)
                        start = read(0xe000bff8)
                        expect(native(14, cin, cout) == 0, 'valid cap code')
                        elapsed = (read(0xe000bff8) - start) & 0xffffffff
                        expect(100 <= elapsed <= 1000, f'100us bounded delay: {elapsed} ticks')
                        expect(read(cap) == (before & ~mask) | cin << 22 | cout << 16, 'RMW preserves all other fields')
                        expect(native(20) == cin, 'getter returns capIn, not capOut')
                        report.setdefault('delay_ticks_1mhz', []).append(elapsed)
                    before = read(cap)
                    for cin, cout in ((64, 0), (0, 64), (255, 255), (0xffffffff, 0)):
                        expect(native(14, cin, cout) == 1 and read(cap) == before, 'invalid cap code atomic error')
                check('native cap boundaries, preservation, getter and XTAL delay', bounds)

                def mcu_delay():
                    write(rtc, 0xa000013f)  # MCU320MHz / 320 = 1MHz.
                    start = read(0xe000bff8)
                    expect(native(14, 7, 9) == 0, 'MCU-clock setter')
                    elapsed = (read(0xe000bff8) - start) & 0xffffffff
                    expect(100 <= elapsed <= 1000, f'MCU-source 100us bounded delay: {elapsed}')
                    report['mcu_delay_ticks_1mhz'] = elapsed
                check('MCU source and divisor delay', mcu_delay)

                def reset():
                    qmp.command('system_reset')
                    expect(read(cap) == 0xb410f8f0 and native(20) == 16, 'reset restores cap codes')
                check('machine reset restores register and getter', reset)
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
                        process.kill(); process.wait(timeout=2)
                for path in (gdb, qmpp):
                    if path.exists():
                        path.unlink()
            json.dump(report, result, indent=2)
            result.write('\n')
    print(json.dumps(report))
    return 0 if report['passed'] else 1


if __name__ == '__main__':
    sys.exit(main())
