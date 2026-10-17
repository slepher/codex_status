#!/usr/bin/env python3
"""Native synthetic PEC UART API tests; never reads the original AP01 ROM."""
import argparse
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
RAM, PEC, MAIL = 0x62fc4000, 0xa066e000, 0xf1000080


def expect(ok, message):
    if not ok:
        raise AssertionError(message)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--qemu', type=Path, required=True)
    parser.add_argument('--rom-shim', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--pec-uart-mock', action='store_true', help='acknowledge synthetic PEC service tests')
    args = parser.parse_args()
    output, qemu, shim = (p.resolve() for p in (args.output, args.qemu, args.rom_shim))
    if not args.pec_uart_mock or output.exists() or not output.parent.is_dir() or not Path('/tmp').is_dir():
        parser.error('requires --pec-uart-mock, existing Linux /tmp, and new output in existing directory')
    token = 'ap01-pec-' + uuid.uuid4().hex
    image = output.parent / (token + '.bin')
    shim_bytes = shim.read_bytes()
    report = dict(synthetic_fixture=True, original_rom_accessed=False,
                  full_rom_boot_verified=False, original_library_restored=False,
                  shim_sha256=hashlib.sha256(shim_bytes).hexdigest(), checks=[])
    with output.open('x', encoding='utf-8') as result:
        try:
            with image.open('xb') as file:
                file.truncate(0x66f200)
                file.seek(0x1000)
                file.write(b'\x13\x00\x00\x00')
                file.seek(0x66f000)
                file.write(b'PEC:' + bytes(12))
            original_fixture_hash = hashlib.sha256(image.read_bytes()).hexdigest()
            for mode in ('connected', 'disconnected', 'strict'):
                unique = token + '-' + mode
                gdb, qmp_path = (Path('/tmp') / (unique + ext) for ext in ('.gdb', '.qmp'))
                serial_path = output.parent / (unique + '.uart2')
                process = gs = qs = qmp = None
                deadline = time.monotonic() + 30
                machine = 'ap01,boot-mmio=on,boot2-mock=on,rom-shim=' + str(shim)
                if mode != 'strict':
                    machine += ',pec-uart-mock=on'
                command = [str(qemu), '-M', machine, '-cpu', 'rv32', '-kernel', str(image),
                           '-S', '-display', 'none', '-monitor', 'none',
                           '-serial', 'none', '-serial', 'none', '-serial',
                           'file:' + str(serial_path) if mode == 'connected' else 'none',
                           '-gdb', f'unix:{gdb},server=on,wait=off',
                           '-qmp', f'unix:{qmp_path},server=on,wait=off']
                try:
                    with (output.parent / (unique + '.log')).open('x') as log:
                        process = subprocess.Popen(command, stdin=subprocess.DEVNULL,
                                                   stdout=log, stderr=log,
                                                   preexec_fn=probe.child_limits)
                        gs = probe.connect(gdb, process, deadline)
                        qs = probe.connect(qmp_path, process, deadline)
                        rsp, qmp = probe.RSP(gs, deadline), probe.QMP(qs, deadline)
                        rsp.request('?')

                        def check(name, function):
                            try:
                                function()
                                report['checks'].append(dict(mode=mode, name=name, passed=True))
                            except Exception as exc:
                                report['checks'].append(dict(mode=mode, name=name, passed=False, error=str(exc)))
                                raise

                        def put(address, data):
                            expect(rsp.request(f'M{address:x},{len(data):x}:' + data.hex()) == 'OK', 'memory write')

                        def write(address, value):
                            put(address, value.to_bytes(4, 'little'))

                        def get(address, size=4):
                            return bytes.fromhex(rsp.request(f'm{address:x},{size:x}'))

                        def read(address):
                            return int.from_bytes(get(address), 'little')

                        def rejected(packet):
                            try:
                                reply = rsp.request(packet)
                            except RuntimeError as exc:
                                if str(exc).startswith('RSP '):
                                    return
                                raise
                            raise AssertionError(f'request unexpectedly accepted: {packet}: {reply}')

                        def setreg(index, value):
                            expect(rsp.request(f'P{index:x}=' + value.to_bytes(4, 'little').hex()) == 'OK', 'register write')

                        def reg(index):
                            raw = bytes.fromhex(rsp.request('g'))
                            return int.from_bytes(raw[index * 4:index * 4 + 4], 'little')

                        def native(slot, values=(0, 0, 0, 0)):
                            relative = read(PEC + slot)
                            pointer = (PEC + relative) & 0xffffffff
                            expected_pointer = struct.unpack_from('<I', shim_bytes, 0x810 + slot)[0]
                            expect(pointer == expected_pointer and pointer >= 0x90002000, 'native overlay target')
                            for index, value in enumerate(values):
                                setreg(10 + index, value)
                            setreg(1, RAM + 0x400)
                            setreg(2, RAM + 0x1800)
                            setreg(32, pointer)
                            for _ in range(256):
                                pc = reg(32)
                                if pc == RAM + 0x400:
                                    return reg(10)
                                expect(pc != 0, 'native API trapped')
                                stop = rsp.request('s')
                                expect(stop.startswith(('S', 'T')), 'single step stop')
                            raise AssertionError('native wrapper step budget exceeded')

                        def stage(values):
                            for i, value in enumerate(values):
                                write(MAIL + i * 4, value)

                        def direct(slot, values=(0, 0, 0, 0)):
                            stage(values)
                            write(MAIL + 0x10, slot)
                            return read(MAIL + 0x14)

                        def reject_commit(slot, values):
                            stage(values)
                            rejected(f'M{MAIL + 0x10:x},4:' + slot.to_bytes(4, 'little').hex())

                        if mode == 'strict':
                            def strict():
                                expect(get(PEC, 4) == b'PEC:', 'strict header preserved')
                                expect(read(PEC + 0x30) == 0, 'strict table remains original synthetic bytes')
                                rejected(f'm{MAIL:x},4')
                            check('strict mode does not redirect PEC table or map mailbox', strict)
                            continue

                        rx, tx = RAM, RAM + 0x40
                        def config(pin, trailing):
                            return struct.pack('<III4BI', 0, 160000000, 115200, 0, pin, 1, 0, trailing)
                        put(rx, config(11, 20))
                        put(tx, config(12, 0))
                        initial_configs = get(rx, 20), get(tx, 20)

                        def initialize():
                            reject_commit(0x30, (2, 0, 0, 0))
                            expect(native(0x30, (1, 0, 0, 0)) == 0, 'synthetic profile')
                            expect(native(0x88) == 0, 'controller initialization')
                            bad = bytearray(config(11, 20)); bad[4] ^= 1
                            put(RAM + 0x100, bad)
                            reject_commit(0x40, (RAM + 0x100, 0, 0, 0))
                            reject_commit(0x40, (0x2000a000, 0, 0, 0))
                            expect(native(0x40, (rx, 0, 0, 0)) == 0, 'RX initialization')
                            expect(native(0x44, (tx, 0, 0, 0)) == 0, 'TX initialization')
                            expect((get(rx, 20), get(tx, 20)) == initial_configs, 'no fabricated handles/config mutation')
                        check('native configuration and strict invalid config rejection', initialize)

                        def no_rx():
                            expect(native(0x48, (rx, 0, 0, 0)) == 0, 'no RX available')
                            write(RAM + 0x200, 0x12345678)
                            reject_commit(0x4c, (rx, RAM + 0x200, 1, 0))
                            expect(read(RAM + 0x200) == 0x12345678, 'empty RX output unchanged')
                        check('no RX provider is explicit and output untouched', no_rx)

                        def invalid_mailbox():
                            rejected(f'M{MAIL:x},1:01')
                            direct(0x48, (rx, 0, 0, 0))
                            rejected(f'M{MAIL + 0x10:x},4:48000000')
                            write(MAIL, rx)
                            rejected(f'M{MAIL + 0x10:x},4:48000000')
                            reject_commit(0x54, (0, 0, 0, 0))
                            rejected(f'm{PEC + 0x54:x},4')
                            rejected(f'm{PEC + 0x30:x},1')
                            expect(native(0x48, (rx, 0, 0, 0)) == 0, 'rejection does not poison endpoint')
                        check('partial stale argument sets and unknown APIs rejected', invalid_mailbox)

                        def transmit():
                            text = b'PEC native UART2\n'
                            put(RAM + 0x200, b''.join(struct.pack('<I', c) for c in text))
                            reject_commit(0x50, (tx, 0x2000a000, 1, 0))
                            expect(native(0x50, (tx, RAM + 0x200, len(text), 0)) == len(text), 'accepted actual word count')
                            pending = native(0xe8, (tx, 0, 0, 0))
                            if mode == 'connected':
                                expect(pending == 0, 'file backend drained')
                                expect(serial_path.read_bytes() == text, 'actual file backend bytes')
                            else:
                                expect(pending == len(text), 'disconnected backend preserves queue')
                                remaining = 32 - len(text)
                                put(RAM + 0x300, bytes(remaining * 4))
                                expect(native(0x50, (tx, RAM + 0x300, remaining, 0)) == remaining, 'queue fills to bound')
                                reject_commit(0x50, (tx, RAM + 0x200, 1, 0))
                                expect(native(0xe8, (tx, 0, 0, 0)) == 32, 'full queue rejection atomic')
                        check('native TX actual backend or bounded disconnected queue', transmit)

                        def reset():
                            qmp.command('system_reset')
                            while reg(32) != 0xa0000000:
                                expect(time.monotonic() < deadline, 'reset deadline')
                                time.sleep(0.001)
                            reject_commit(0xe8, (tx, 0, 0, 0))
                            native(0x30, (1, 0, 0, 0)); native(0x88)
                            native(0x44, (tx, 0, 0, 0))
                            expect(native(0xe8, (tx, 0, 0, 0)) == 0, 'reset clears queue and endpoint registration')
                        check('reset clears state and retains working API transport', reset)
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
                        for path in (gdb, qmp_path):
                            if path.exists():
                                path.unlink()
            expect(hashlib.sha256(image.read_bytes()).hexdigest() == original_fixture_hash, 'synthetic source mutated')
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
