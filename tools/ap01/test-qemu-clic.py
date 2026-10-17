#!/usr/bin/env python3
"""Synthetic common CLIC trap checks; no original ROM/device access."""
import argparse
import importlib.util
import json
from pathlib import Path
import re
import subprocess
import sys
import threading
import time
import uuid

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location('probe', Path(__file__).with_name('run-qemu-ap01.py'))
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)
BASE, HANDLER = 0x20fc1000, 0x20fc2000


def expect(ok, message):
    if not ok:
        raise AssertionError(message)


def csr_write(csr, register=5):
    return (csr << 20) | (register << 15) | 0x1073


def csr_read(csr, register=6):
    return (csr << 20) | (register << 7) | 0x2073


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--qemu', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    qemu, output = args.qemu.resolve(), args.output.resolve()
    if output.exists() or not output.parent.is_dir() or not Path('/tmp').is_dir():
        parser.error('requires Linux /tmp and new output in existing directory')
    token = 'ap01-clic-' + uuid.uuid4().hex
    image = output.parent / (token + '.bin')
    report = dict(synthetic_fixture=True, original_rom_accessed=False,
                  full_rom_boot_verified=False, common_trap_only=True, checks=[])
    with output.open('x', encoding='utf-8') as result:
        try:
            with image.open('xb') as file:
                file.write(bytes(4096) + b'\x13\x00\x00\x00')
            invalid = subprocess.run(
                [str(qemu), '-M', 'ap01,boot-mmio=on,boot2-mock=on,clic-mock=on',
                 '-cpu', 'rv32,x-ap01-clic=off', '-kernel', str(image), '-S',
                 '-display', 'none', '-serial', 'none', '-monitor', 'none'],
                stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                timeout=3, preexec_fn=probe.child_limits)
            expect(invalid.returncode != 0 and b'x-ap01-clic' in invalid.stderr,
                   'clic-mock without CPU capability did not fail explicitly')
            report['checks'].append(dict(name='machine CLIC requires matching CPU capability', passed=True))
            for enabled in (False, True):
                process = gs = qs = ts = qmp = None
                suffix = 'on' if enabled else 'off'
                gdb, qmp_path = (Path('/tmp') / (token + suffix + ext) for ext in ('.gdb', '.qmp'))
                qtest_path = Path('/tmp') / (token + suffix + '.qtest')
                log_path = output.parent / (token + suffix + '.log')
                deadline = time.monotonic() + 30
                try:
                    with log_path.open('x') as log:
                        process = subprocess.Popen(
                            [str(qemu), '-M', 'ap01,boot-mmio=on,boot2-mock=on' +
                             (',clic-mock=on' if enabled else ''),
                             '-cpu', 'rv32,x-ap01-clic=' + suffix, '-kernel', str(image), '-S',
                             '-icount', 'shift=0,sleep=on', '-display', 'none', '-serial', 'none',
                             '-monitor', 'none', '-gdb', f'unix:{gdb},server=on,wait=off',
                             '-qtest', f'unix:{qtest_path},server=on,wait=off',
                             '-qmp', f'unix:{qmp_path},server=on,wait=off'],
                            stdin=subprocess.DEVNULL, stdout=log, stderr=log,
                            preexec_fn=probe.child_limits)
                        gs = probe.connect(gdb, process, deadline)
                        qs = probe.connect(qmp_path, process, deadline)
                        ts = probe.connect(qtest_path, process, deadline)
                        rsp, qmp = probe.RSP(gs, deadline), probe.QMP(qs, deadline)
                        rsp.request('?')

                        def write(address, value, size=4):
                            expect(rsp.request(f'M{address:x},{size:x}:' + value.to_bytes(size, 'little').hex()) == 'OK',
                                   'memory write rejected')

                        def read(address, size=4):
                            return int.from_bytes(bytes.fromhex(rsp.request(f'm{address:x},{size:x}')), 'little')

                        def setreg(number, value):
                            expect(rsp.request(f'P{number:x}=' + value.to_bytes(4, 'little').hex()) == 'OK',
                                   'register write rejected')

                        def reg(number):
                            raw = bytes.fromhex(rsp.request('g'))
                            return int.from_bytes(raw[number * 4:number * 4 + 4], 'little')

                        def run(words):
                            for index, word in enumerate(words + [0x13]):
                                write(BASE + index * 4, word)
                            end = BASE + len(words) * 4
                            setreg(32, BASE)
                            expect(rsp.request(f'Z0,{end:x},4') == 'OK', 'breakpoint rejected')
                            try:
                                expect(rsp.request('c').startswith(('S', 'T')), 'unexpected stop reply')
                                expect(reg(32) == end, 'guest did not reach expected continuation')
                            finally:
                                rsp.request(f'z0,{end:x},4')

                        def setcsr(csr, value):
                            setreg(5, value)
                            run([csr_write(csr)])

                        def getcsr(csr):
                            run([csr_read(csr)])
                            return reg(6)

                        def rejected(address, value, size=1):
                            try:
                                write(address, value, size)
                            except RuntimeError as exc:
                                expect(str(exc).startswith('RSP '), 'wrong rejection')
                            else:
                                raise AssertionError(f'unsupported CLIC write accepted at {address:x}')

                        def check(name, fn):
                            report['active_check'] = suffix + ': ' + name
                            fn()
                            report['checks'].append(dict(name=report['active_check'], passed=True))

                        def mtvec_modes():
                            setcsr(0x305, HANDLER)
                            setcsr(0x305, HANDLER | 3)
                            expect(getcsr(0x305) == (HANDLER | 3 if enabled else HANDLER),
                                   'mode3 flag boundary')
                            before = getcsr(0x305)
                            setcsr(0x305, HANDLER | 2)
                            expect(getcsr(0x305) == before, 'reserved mode2 changed mtvec')
                            setcsr(0x305, HANDLER | 7)
                            expect(getcsr(0x305) == before, 'unaligned mode3 changed mtvec')
                        check('mtvec feature flag, mode2 and 64-byte alignment', mtvec_modes)
                        if not enabled:
                            continue

                        def controller():
                            write(0xe0800000, 8)
                            write(0xe0800008, 0)
                            rejected(0xe0800000, 4, 4)
                            rejected(0xe0800008, 1, 4)
                            rejected(0xe0800000, 4, 1)
                            rejected(0xe0800000, 4, 2)
                            rejected(0xe0800001, 1, 1)
                            rejected(0xe0800002, 1, 2)
                            rejected(0xe0800008, 1, 1)
                            rejected(0xe0800008, 1, 2)
                            rejected(0xe0800009, 1, 1)
                            rejected(0xe080000a, 1, 2)
                            expect(read(0xe0800000) == 8 and read(0xe0800008) == 0,
                                   'rejected partial control write changed committed state')
                            write(0xe0800000, 0, 1)
                            write(0xe0800000, 8, 2)
                            write(0xe0800008, 0, 2)
                            for irq in (80,):
                                source = 0xe0801000 + 4 * irq
                                for byte in range(4):
                                    rejected(source + byte, 1 if byte != 3 else 0xf0, 1)
                                rejected(source, 0x00000100, 2)
                                rejected(source, 0xf0000101, 4)
                                rejected(source, 0, 4)
                                try:
                                    read(source)
                                except RuntimeError as exc:
                                    expect(str(exc).startswith('RSP '), 'wrong source-bound read rejection')
                                else:
                                    raise AssertionError('unsupported IRQ80 read accepted')
                            rejected(0xe080101e, 1)
                            rejected(0xe080101e, 2)
                            write(0xe080101f, 0xff, 1)
                            expect(read(0xe080101f, 1) == 0xf0, 'CTL mask')
                            write(0x20009014, 0xc000001f)
                            write(0xe0004000, 0)
                            write(0xe0004004, 0)
                            expect(read(0xe080101c, 1) == 1, 'IP7 does not expose pending')
                            expect(not getcsr(0x344) & 0x80, 'IE7=0 did not gate MTIP')
                        check('bounded CLIC register support and timer pending', controller)

                        # Handler snapshots trap entry, moves compare into the
                        # future, counts delivery, then executes real mret.
                        handler = [csr_read(0x342, 16), csr_read(0x341, 17),
                                   csr_read(0x300, 18), 0xe00042b7, 0xfff00313,
                                   0x0062a023, 0x0062a223, 0x00198993, 0x30200073]
                        for index, word in enumerate(handler):
                            write(HANDLER + index * 4, word)
                        setreg(19, 0)

                        def gates_and_trap():
                            setcsr(0x304, 0x80)
                            setcsr(0x300, 8)
                            run([0x13, 0x13])
                            expect(reg(19) == 0, 'IE7=0 delivered an interrupt')
                            setcsr(0x300, 0)
                            write(0xe080101d, 1, 1)
                            expect(getcsr(0x344) & 0x80, 'IE7=1 did not expose MTIP')
                            run([0x13, 0x13])
                            expect(reg(19) == 0, 'MIE=0 delivered an interrupt')
                            setcsr(0x304, 0)
                            setcsr(0x300, 8)
                            run([0x13, 0x13])
                            expect(reg(19) == 0, 'mie7=0 delivered an interrupt')
                            # Restore mie7 inside the guest; the immediately
                            # following PC must be saved as mepc and resumed.
                            setreg(5, 0x80)
                            run([csr_write(0x304), 0x13, 0x13])
                            expect(reg(19) == 1, 'restored gates did not deliver exactly once')
                            expect(reg(16) == 0x80000007, 'mcause is not timer interrupt7')
                            expect(reg(17) == BASE + 4, 'mepc did not identify interrupted continuation')
                            entry_status = reg(18)
                            expect(not entry_status & 8 and entry_status & 0x80,
                                   'trap entry did not move MIE into MPIE')
                            expect(getcsr(0x300) & 8, 'mret did not restore MIE')
                            expect(not read(0xe080101c, 1), 'handler compare update did not clear IP7')
                            report['trap'] = dict(mcause=hex(reg(16)), mepc=hex(reg(17)),
                                                  entry_mstatus=hex(entry_status), deliveries=reg(19))
                        check('independent IE7/MIE/mie7 gates, trap, compare update and mret', gates_and_trap)

                        def external_traps_and_wfi():
                            report['external_traps'] = []
                            # External IDs deliberately run with all mie bits
                            # clear. Only individual CLICIE and global MIE gate.
                            setcsr(0x300, 0)
                            setcsr(0x304, 0)
                            write(0xe080101d, 0, 1)
                            for irq in (31, 60, 66, 79):
                                source = 0xe0801000 + irq * 4
                                # Handler snapshots real raw cause and clears
                                # this synthetic edge latch before mret.
                                handler = [csr_read(0x342, 16), csr_read(0x341, 17),
                                           csr_read(0x300, 18), 0xe08012b7,
                                           ((source & 0xfe0) << 20) |
                                           ((source & 0x1f) << 7) | 0x28023,
                                           0x00198993, 0x30200073]
                                for index, word in enumerate(handler):
                                    write(HANDLER + index * 4, word)
                                setreg(19, 0)
                                write(source + 2, 2, 1)
                                write(source, 1, 1)
                                write(source + 1, 0, 1)
                                setcsr(0x300, 8)
                                run([0x13, 0x13])
                                expect(reg(19) == 0, f'IRQ{irq} ignored CLICIE=0')
                                setcsr(0x300, 0)
                                write(source + 1, 1, 1)
                                run([0x13, 0x13])
                                expect(reg(19) == 0, f'IRQ{irq} ignored MIE=0')
                                setreg(5, 8)
                                run([csr_write(0x300), 0x13, 0x13])
                                expect(reg(19) == 1 and reg(16) == 0x80000000 | irq,
                                       f'IRQ{irq} did not deliver raw cause with mie11=0')
                                expect(reg(17) == BASE + 4 and not read(source, 1),
                                       f'IRQ{irq} mret/edge clear failed')
                                report['external_traps'].append(dict(irq=irq, mcause=hex(reg(16)),
                                                                     mepc=hex(reg(17)), mie=0))

                                # With no pending IRQ, WFI leaves PC at its
                                # continuation while VM stays running; an
                                # execution breakpoint there cannot fire until
                                # wakeup. Inject after independently observing
                                # that state through QMP (qtest only writes IP).
                                for index, word in enumerate((0x10500073, 0x13)):
                                    write(BASE + index * 4, word)
                                setreg(32, BASE)
                                expect(rsp.request(f'Z0,{BASE + 4:x},4') == 'OK', 'WFI breakpoint')
                                replies = {}

                                def resume():
                                    try:
                                        replies['stop'] = rsp.request('c')
                                    except Exception as exc:
                                        replies['error'] = str(exc)
                                thread = threading.Thread(target=resume, daemon=True)
                                thread.start()
                                try:
                                    limit = min(deadline, time.monotonic() + 3)
                                    observed = False
                                    while time.monotonic() < limit:
                                        state = qmp.command('query-status')
                                        text = qmp.command('human-monitor-command',
                                                           {'command-line': 'info registers'})
                                        pc = re.search(r'\bpc\s+([0-9a-fA-F]+)', text)
                                        if state['running'] and pc and int(pc.group(1), 16) == BASE + 4:
                                            observed = True
                                            break
                                        time.sleep(0.001)
                                    expect(observed and thread.is_alive(), f'IRQ{irq} did not halt in WFI')
                                    ts.settimeout(max(0.001, deadline - time.monotonic()))
                                    ts.sendall(f'writeb 0x{source:x} 0x1\n'.encode())
                                    reply = bytearray()
                                    while not reply.endswith(b'\n'):
                                        byte = ts.recv(1)
                                        expect(byte and len(reply) < 128, 'invalid qtest response')
                                        reply.extend(byte)
                                    expect(reply == b'OK\n', f'qtest IP injection: {reply!r}')
                                    thread.join(timeout=max(0.001, min(3, deadline - time.monotonic())))
                                    expect(not thread.is_alive() and 'error' not in replies,
                                           f'IRQ{irq} failed WFI wakeup: {replies}')
                                    expect(reg(32) == BASE + 4 and reg(19) == 2,
                                           f'IRQ{irq} WFI did not trap/mret once')
                                    expect(reg(16) == 0x80000000 | irq and reg(17) == BASE + 4,
                                           f'IRQ{irq} WFI wrong raw cause/mepc')
                                    report['external_traps'][-1]['wfi_wakeup_verified'] = True
                                finally:
                                    if thread.is_alive():
                                        qmp.command('stop')
                                        thread.join(timeout=1)
                                    rsp.request(f'z0,{BASE + 4:x},4')
                                setcsr(0x300, 0)
                                write(source + 1, 0, 1)
                        check('raw external IRQ31/60/66/79 gates, trap/mret and WFI wake with mie11=0', external_traps_and_wfi)

                        def retention_gpio_reset():
                            expect(read(0x20010000) == 0, 'cold retention RAM is not zero')
                            write(0x20010000, 0x12345678)
                            gpio = 0x200008c4
                            write(gpio, 1 << 25)
                            expect(read(gpio) & (1 << 24), 'GPIO set pulse did not set latch')
                            write(gpio, 1 << 26)
                            expect(not read(gpio) & (1 << 24), 'GPIO clear pulse did not clear latch')
                            setcsr(0x300, 0)
                            write(0xe080101d, 1, 1)
                            write(0xe0004000, 0)
                            write(0xe0004004, 0)
                            expect(read(0xe080101c, 1) == 1 and read(0xe080101d, 1) == 1,
                                   'reset precondition must have enabled pending timer')
                            qmp.command('system_reset')
                            expect(read(0x20010000) == 0x12345678, 'warm reset lost retention RAM')
                            expect(read(0xe080101d, 1) == 0 and read(0xe080101c, 1) == 0,
                                   'reset did not clear IE7/pending')
                            expect(read(0xe0004000) == 0xffffffff and read(0xe0004004) == 0xffffffff,
                                   'reset did not restore synthetic compare-max')
                            expect(reg(32) == 0xa0000000, 'CPU reset did not restore reset PC')
                            expect(not getcsr(0x300) & 8, 'CPU reset did not clear global MIE')
                            # QEMU target/riscv/cpu.c riscv_cpu_reset_hold
                            # explicitly resets PC/MIE/mcause, not mtvec/mie.
                            # Record this generic-CPU boundary, not a claim
                            # about physical BL616 reset behavior.
                            report['warm_reset_mtvec'] = hex(getcsr(0x305))
                        check('GPIO set/clear, cold/warm retention and CLIC/CPU reset', retention_gpio_reset)
                finally:
                    if qmp is not None:
                        if sys.exc_info()[0] is not None:
                            try:
                                qmp.deadline = time.monotonic() + 2
                                qmp.command('stop')
                                report['failure_registers'] = qmp.command('human-monitor-command',
                                                                         {'command-line': 'info registers'})
                            except Exception as exc:
                                report['diagnostic_error'] = str(exc)
                        qmp.file.close()
                    for sock in (gs, qs, ts):
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
                        for path in (gdb, qmp_path, qtest_path):
                            if path.exists():
                                path.unlink()
            report['passed'] = True
        except Exception as exc:
            report.update(passed=False, error=f'{type(exc).__name__}: {exc}')
        finally:
            json.dump(report, result, indent=2)
            result.write('\n')
    print(json.dumps(report))
    return 0 if report['passed'] else 1


if __name__ == '__main__':
    sys.exit(main())
