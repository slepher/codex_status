#!/usr/bin/env python3
"""Native slot134 config tests; shared RAM remains unpartitioned synthetic RAM."""
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
BASE, CFG, SLOT = 0x20fc1000, 0x2000060c, 0x90015a18


def expect(ok, message):
    if not ok:
        raise AssertionError(message)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('qemu', 'shim', 'output'):
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    qemu, shim, output = args.qemu.resolve(), args.shim.resolve(), args.output.resolve()
    if output.exists() or not output.parent.is_dir() or not Path('/tmp').is_dir() or ',' in str(shim):
        parser.error('requires Linux /tmp, comma-free shim path and new output in existing directory')
    token = 'ap01-em-' + uuid.uuid4().hex
    image = output.parent / (token + '.bin')
    report = dict(synthetic_fixture=True, original_rom_accessed=False,
                  full_rom_boot_verified=False, physical_sram_partition_verified=False,
                  shared_ram_bytes=480 * 1024,
                  shim_sha256=hashlib.sha256(shim.read_bytes()).hexdigest(), checks=[])
    with output.open('x', encoding='utf-8') as result:
        try:
            with image.open('xb') as file:
                file.write(bytes(4096) + b'\x13\x00\x00\x00')
            for enabled in (False, True):
                process = gs = qs = qmp = None
                suffix = 'shim' if enabled else 'strict'
                gdb, qmp_path = (Path('/tmp') / (token + suffix + ext) for ext in ('.gdb', '.qmp'))
                deadline = time.monotonic() + 30
                try:
                    with (output.parent / (token + suffix + '.log')).open('x') as log:
                        machine = 'ap01,boot-mmio=on,boot2-mock=on'
                        if enabled:
                            machine += ',rom-shim=' + str(shim)
                        process = subprocess.Popen(
                            [str(qemu), '-M', machine, '-cpu', 'rv32', '-kernel', str(image), '-S',
                             '-display', 'none', '-serial', 'none', '-monitor', 'none',
                             '-gdb', f'unix:{gdb},server=on,wait=off',
                             '-qmp', f'unix:{qmp_path},server=on,wait=off'],
                            stdin=subprocess.DEVNULL, stdout=log, stderr=log,
                            preexec_fn=probe.child_limits)
                        gs = probe.connect(gdb, process, deadline)
                        qs = probe.connect(qmp_path, process, deadline)
                        rsp, qmp = probe.RSP(gs, deadline), probe.QMP(qs, deadline)
                        rsp.request('?')

                        def write(address, value):
                            expect(rsp.request(f'M{address:x},4:' + value.to_bytes(4, 'little').hex()) == 'OK', 'write')

                        def read(address):
                            return int.from_bytes(bytes.fromhex(rsp.request(f'm{address:x},4')), 'little')

                        def setreg(number, value):
                            expect(rsp.request(f'P{number:x}=' + value.to_bytes(4, 'little').hex()) == 'OK', 'setreg')

                        if not enabled:
                            try:
                                read(SLOT)
                            except RuntimeError as exc:
                                expect(str(exc).startswith('RSP '), 'wrong strict rejection')
                            else:
                                raise AssertionError('strict mode accepted absent slot134')
                            report['checks'].append(dict(name='absent shim leaves slot134 unsupported', passed=True))
                            continue

                        expect(0x90002000 <= read(SLOT) < 0x90011000, 'slot134 does not bind native shim')
                        # Real guest loads the API pointer and executes jalr.
                        for index, word in enumerate((0x0002a283, 0x000280e7, 0x13)):
                            write(BASE + index * 4, word)
                        setreg(2, 0x62fc7000)
                        aliases = (0x20fc0000, 0x22fc0000, 0x60fc0000, 0x62fc0000)
                        offsets = (0, 0x4000, 0x30000, 0x77ffc)
                        samples = {offset: (0xa55a1234 ^ offset) for offset in offsets}
                        for offset, value in samples.items():
                            write(aliases[0] + offset, value)

                        def preserved():
                            for alias in aliases:
                                for offset, value in samples.items():
                                    expect(read(alias + offset) == value, 'EM selection changed shared RAM/alias contents')

                        def call(argument):
                            setreg(5, SLOT)
                            setreg(10, argument)
                            setreg(32, BASE)
                            expect(rsp.request(f'Z0,{BASE + 8:x},4') == 'OK', 'return breakpoint')
                            try:
                                expect(rsp.request('c').startswith(('T', 'S')), 'call stop')
                                raw = bytes.fromhex(rsp.request('g'))
                                expect(int.from_bytes(raw[128:132], 'little') == BASE + 8, 'API did not return')
                                return int.from_bytes(raw[40:44], 'little')
                            finally:
                                rsp.request(f'z0,{BASE + 8:x},4')

                        report['selections'] = []
                        for enum, field, wram, em in ((0, 0, 160, 0), (1, 3, 128, 32), (2, 15, 96, 64)):
                            upper = read(CFG) & ~15
                            expect(call(enum) == 0 and read(CFG) == upper | field, 'SDK EM enum/field mapping')
                            preserved()
                            report['selections'].append(dict(enum=enum, field=field,
                                                             requested_wram_kib=wram, requested_em_kib=em))
                        report['checks'].append(dict(name='native slot134 implements exact SDK enums0/1/2', passed=True))
                        for invalid in (3, 255, 256, 0xffffffff):
                            before = read(CFG)
                            expect(call(invalid) == 1 and read(CFG) == before, 'invalid EM argument mutated configuration')
                            preserved()
                        report['checks'].append(dict(name='invalid arguments return ERROR1 without config/RAM mutation', passed=True))
                        report['checks'].append(dict(name='RAM sentinels retain contents through all four shared aliases', passed=True))
                        qmp.command('system_reset')
                        expect(read(CFG) == 3, 'warm reset did not restore synthetic SRAM_CFG3 reset field')
                        preserved()
                        report['checks'].append(dict(name='warm reset restores config3 and preserves existing shared RAM', passed=True))
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
