#!/usr/bin/env python3
"""Bounded synthetic MTIMER checks; no original ROM or CLIC dispatch claim."""
import argparse
import importlib.util
import json
from pathlib import Path
import re
import subprocess
import sys
import time
import uuid

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location(
    'ap01_probe', Path(__file__).with_name('run-qemu-ap01.py'))
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)
ENTRY = 0xa0000000
RTC = 0x20009014
CMP = 0xe0004000
MTIME = 0xe000bff8


def expect(condition, message):
    if not condition:
        raise AssertionError(message)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--qemu', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    qemu, output = args.qemu.resolve(), args.output.resolve()
    if output.exists() or not output.parent.is_dir() or not Path('/tmp').is_dir():
        parser.error('requires Linux /tmp and a new output in an existing directory')
    token = 'ap01-rtc-' + uuid.uuid4().hex
    image = output.parent / (token + '.bin')
    log_path = output.parent / (token + '.log')
    gdb, qmp_path = (Path('/tmp') / (token + ext) for ext in ('.gdb', '.qmp'))
    # 65536 countdown iterations, then a breakpoint. With icount shift=0,
    # guest instructions advance virtual nanoseconds deterministically.
    words = [0x000102b7, 0xfff28293, 0xfe029ee3, 0x00000013,
             0xc0102373, 0xc81023f3,
             0x20009537, 0x800005b7, 0x01f58593, 0x00b52a23,
             0x34402673, 0x000102b7, 0xfff28293, 0xfe029ee3,
             0x344026f3, 0x01f00593, 0x00b52a23, 0x00000013]
    report = dict(synthetic_fixture=True, original_rom_accessed=False,
                  full_rom_boot_verified=False, clic_dispatch_verified=False,
                  checks=[], fixture=str(image), log=str(log_path))
    process = gs = qs = qmp = None
    with output.open('x', encoding='utf-8') as result:
        try:
            with image.open('xb') as file:
                file.write(bytes(4096) + b''.join(w.to_bytes(4, 'little') for w in words))
            deadline = time.monotonic() + 30
            with log_path.open('x') as log:
                process = subprocess.Popen(
                    [str(qemu), '-M', 'ap01,boot-mmio=on,boot2-mock=on',
                     '-cpu', 'rv32', '-kernel', str(image), '-S',
                     '-icount', 'shift=0,sleep=on', '-display', 'none',
                     '-serial', 'none', '-monitor', 'none',
                     '-gdb', f'unix:{gdb},server=on,wait=off',
                     '-qmp', f'unix:{qmp_path},server=on,wait=off'],
                    stdin=subprocess.DEVNULL, stdout=log, stderr=log,
                    preexec_fn=probe.child_limits)
                gs = probe.connect(gdb, process, deadline)
                qs = probe.connect(qmp_path, process, deadline)
                rsp, qmp = probe.RSP(gs, deadline), probe.QMP(qs, deadline)
                rsp.request('?')

                def read(address):
                    return int.from_bytes(bytes.fromhex(rsp.request(f'm{address:x},4')), 'little')

                def write(address, value):
                    expect(rsp.request(f'M{address:x},4:' + value.to_bytes(4, 'little').hex()) == 'OK',
                           'MMIO write failed')

                def ticks():
                    return read(MTIME) | (read(MTIME + 4) << 32)

                def setreg(number, value):
                    expect(rsp.request(f'P{number:x}=' + value.to_bytes(4, 'little').hex()) == 'OK',
                           'register write failed')

                def run_loop(start=ENTRY, end=ENTRY + 12):
                    setreg(32, start)
                    expect(rsp.request(f'Z0,{end:x},4') == 'OK', 'breakpoint refused')
                    try:
                        expect(rsp.request('c').startswith(('T', 'S')), 'unexpected continue reply')
                        raw = bytes.fromhex(rsp.request('g'))
                        expect(int.from_bytes(raw[128:132], 'little') == end,
                               'countdown did not reach completion breakpoint')
                    finally:
                        rsp.request(f'z0,{end:x},4')

                def mip():
                    text = qmp.command('human-monitor-command', {'command-line': 'info registers'})
                    match = re.search(r'\bmip\s+([0-9a-fA-F]+)', text)
                    expect(match is not None, 'HMP did not expose mip')
                    return int(match.group(1), 16) & 0x80

                def check(name, fn):
                    report['active_check'] = name
                    fn()
                    report['checks'].append(dict(name=name, passed=True))

                def reset_and_mask():
                    expect(read(RTC) == 0x8000000a, 'RTC reset configuration')
                    write(RTC, 0xffffffff)
                    expect(read(RTC) == 0xe00003ff, 'reserved bits must remain zero')
                    expect(ticks() == 0, 'held reset must read zero')
                    run_loop()
                    expect(ticks() == 0, 'held reset advanced')
                check('RTC reset, writable mask and held reset', reset_and_mask)

                def rates():
                    write(CMP + 4, 0xffffffff)
                    write(CMP, 0xffffffff)
                    samples = []
                    for divider in (31, 63):
                        write(RTC, 0xc0000000 | divider)
                        write(RTC, 0x80000000 | divider)
                        start = ticks()
                        run_loop()
                        samples.append(ticks() - start)
                    report['divider_samples'] = samples
                    expect(120 <= samples[0] <= 140, '1MHz icount progression outside instruction budget')
                    expect(abs(samples[0] - samples[1] * 2) <= 2, 'DIV+1 did not halve rate')
                    before = ticks()
                    write(RTC, 63)
                    run_loop()
                    expect(ticks() == before, 'disabled counter advanced')
                check('virtual instruction clock, divider and disable freeze', rates)

                def csr_time():
                    frozen = ticks()
                    for pc in (ENTRY + 16, ENTRY + 20):
                        setreg(32, pc)
                        expect(rsp.request('s').startswith(('T', 'S')), 'CSR step failed')
                    raw = bytes.fromhex(rsp.request('g'))
                    low = int.from_bytes(raw[24:28], 'little')
                    high = int.from_bytes(raw[28:32], 'little')
                    expect((high << 32) | low == frozen, 'time/timeh disagree with MMIO')
                check('CSR time/timeh share the frozen MMIO counter', csr_time)

                def compare():
                    write(RTC, 0xc000001f)
                    write(CMP + 4, 1)
                    write(CMP, 0)
                    expect(read(CMP + 4) == 1 and read(CMP) == 0, 'split compare readback')
                    expect(not mip(), 'future compare asserted MTIP')
                    write(CMP, 10)
                    write(CMP + 4, 0)
                    expect(not mip(), 'compare asserted before deadline')
                    # Enable, sample pending, run, sample pending, and gate in
                    # the guest. Host GDB pauses must not warp to the deadline.
                    run_loop(ENTRY + 24, ENTRY + 68)
                    raw = bytes.fromhex(rsp.request('g'))
                    early = int.from_bytes(raw[48:52], 'little')
                    late = int.from_bytes(raw[52:56], 'little')
                    expect(not early & 0x80, 'guest MTIP asserted before deadline')
                    expect(late & 0x80, 'guest MTIP absent after deadline')
                    expect(mip(), 'elapsed compare did not assert MTIP')
                    write(RTC, 31)
                    expect(mip(), 'gating unexpectedly cleared pending comparator')
                    write(CMP + 4, 1)
                    expect(not mip(), 'future split compare did not clear MTIP')
                    write(CMP, 0)
                    write(CMP + 4, 0)
                    expect(mip(), 'past compare did not assert immediately')
                    write(CMP, 10)
                    write(RTC, 0xc000001f)
                    expect(ticks() == 0 and not mip(), 'reset failed to zero counter and recompute MTIP')
                    run_loop()
                    expect(not mip(), 'cancelled timer fired during held reset')
                check('64-bit compare, deadline, pending gate and reset cancellation', compare)

                def reject_time_write():
                    try:
                        rsp.request(f'M{MTIME:x},4:01000000')
                    except RuntimeError as exc:
                        expect(str(exc).startswith('RSP '), 'unexpected rejection failure')
                    else:
                        raise AssertionError('read-only MTIME accepted write')
                check('MTIME writes rejected', reject_time_write)

                def mac_microseconds():
                    mac = 0x24b00120

                    def rejected(request):
                        try:
                            rsp.request(request)
                        except RuntimeError as exc:
                            expect(str(exc).startswith('RSP '), 'unexpected MAC rejection')
                        else:
                            raise AssertionError('unsupported MAC access accepted: ' + request)

                    def pll():
                        write(0xf1000000, 4)
                        write(0xf1000004, 3)
                        expect(read(0xf1000008) == 0, 'synthetic PLL configuration failed')

                    def udelay():
                        # SDK phyrf_utils.o/original a00192c0..92ee algorithm:
                        # start=MAC; deadline=start+10; signed(now-deadline)<0.
                        # Keep the actual polling and signed comparison in RV32.
                        base = 0x20fc1000
                        words = [0x0002a583, 0x00a58633, 0x0002a683,
                                 0x40c68733, 0xfe074ce3, 0x40b687b3, 0x13]
                        for index, word in enumerate(words):
                            write(base + index * 4, word)
                        setreg(5, mac)
                        setreg(10, 10)
                        end = base + 24
                        expect(rsp.request(f'Z0,{end:x},4') == 'OK', 'MAC delay breakpoint')
                        setreg(32, base)
                        try:
                            expect(rsp.request('c').startswith(('T', 'S')), 'MAC delay continue')
                            raw = bytes.fromhex(rsp.request('g'))
                            registers = [int.from_bytes(raw[n * 4:n * 4 + 4], 'little') for n in range(33)]
                            expect(registers[32] == end, 'MAC delay did not reach continuation')
                            expect(10 <= registers[15] <= 12 and registers[14] < 0x80000000,
                                   'MAC signed deadline loop did not wait10us')
                            return registers[15]
                        finally:
                            rsp.request(f'z0,{end:x},4')

                    rejected(f'm{mac:x},4')
                    pll()
                    for request in (f'm{mac:x},1', f'm{mac:x},2', f'm{mac:x},8',
                                    f'M{mac:x},4:01000000', f'm{mac + 4:x},4'):
                        rejected(request)
                    expect(read(mac) == read(mac), 'paused virtual MAC counter advanced')
                    report['mac_udelay_us'] = udelay()
                    write(CMP, 0xffffffff)
                    write(CMP + 4, 0xffffffff)
                    samples = []
                    for config in (0xc0000027, 0x80000027, 0x8000004f):
                        write(RTC, config | 0x40000000)
                        write(RTC, config)
                        start_mac, start_core = read(mac), ticks()
                        run_loop()
                        samples.append(dict(mac_us=(read(mac) - start_mac) & 0xffffffff,
                                            core_ticks=ticks() - start_core))
                    expect(all(120 <= sample['mac_us'] <= 140 for sample in samples),
                           'core RTC gate/divider changed independent MAC rate')
                    expect(samples[0]['core_ticks'] == 0 and
                           abs(samples[1]['core_ticks'] - samples[2]['core_ticks'] * 2) <= 2,
                           'core gate/divider reference did not behave independently')
                    report['mac_core_independence_samples'] = samples
                    before_reset = read(mac)
                    qmp.command('system_reset')
                    rejected(f'm{mac:x},4')
                    pll()
                    expect(read(mac) == 0 and before_reset > 0,
                           'warm reset did not reset MAC epoch or PLL prerequisite')
                    expect(10 <= udelay() <= 12, 'MAC delay did not resume after reset/PLL')
                check('MAC microsecond counter, real signed udelay, clock independence and reset', mac_microseconds)
            report['passed'] = True
        except Exception as exc:
            report.update(passed=False, error=f'{type(exc).__name__}: {exc}')
            if qmp is not None:
                try:
                    qmp.deadline = time.monotonic() + 3
                    qmp.command('stop')
                    report['failure_registers'] = qmp.command(
                        'human-monitor-command', {'command-line': 'info registers'})
                except Exception as diagnostic:
                    report['diagnostic_error'] = str(diagnostic)
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
