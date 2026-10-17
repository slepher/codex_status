#!/usr/bin/env python3
"""Native bounded GLB clock ungate and Wi-Fi reset API tests."""
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
BASE, CFG, SLOT = 0x20fc1000, 0x20000580, 0x900159ac


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
    token = 'ap01-clock-gates-' + uuid.uuid4().hex
    image = output.parent / (token + '.bin')
    report = dict(synthetic_fixture=True, original_rom_accessed=False,
                  full_rom_boot_verified=False, physical_clock_gating_verified=False,
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
                                raise AssertionError('strict mode accepted absent slot107')
                            report['checks'].append(dict(name='absent shim leaves slot107 unsupported', passed=True))
                            continue

                        expect(0x90002000 <= read(SLOT) < 0x90011000, 'slot107 does not bind native shim')
                        # Real guest loads the API pointer and executes jalr.
                        for index, word in enumerate((0x0002a283, 0x000280e7, 0x13)):
                            write(BASE + index * 4, word)
                        setreg(2, 0x62fc7000)
                        def call(argument, high=0, slot=SLOT):
                            setreg(5, slot)
                            setreg(10, argument)
                            setreg(11, high)
                            setreg(32, BASE)
                            expect(rsp.request(f'Z0,{BASE + 8:x},4') == 'OK', 'return breakpoint')
                            try:
                                expect(rsp.request('c').startswith(('T', 'S')), 'call stop')
                                raw = bytes.fromhex(rsp.request('g'))
                                expect(int.from_bytes(raw[128:132], 'little') == BASE + 8, 'API did not return')
                                return int.from_bytes(raw[40:44], 'little')
                            finally:
                                rsp.request(f'z0,{BASE + 8:x},4')

                        initial = [read(CFG + i * 4) for i in range(3)]
                        # Start with both supported gates off, preserving all other bits.
                        write(CFG, initial[0] & ~3)
                        expect(call(0x30) == 0 and read(CFG) == initial[0] & ~3, 'DMA1/2 SDK no-op')
                        expect(call(1) == 0 and read(CFG) == (initial[0] & ~3) | 1, 'CPU gate')
                        expect(call(2) == 0 and read(CFG) == initial[0] | 3, 'reserved gate')
                        expect(call(0x33) == 0 and read(CFG) == initial[0] | 3, 'idempotent combined ungate')
                        expect([read(CFG + i * 4) for i in (1, 2)] == initial[1:], 'unrelated gate registers changed')
                        report['checks'].append(dict(name='native uint64 ungate subset and SDK DMA no-ops preserve other gates', passed=True))
                        before = [read(CFG + i * 4) for i in range(3)]
                        for low, high in ((4, 0), (8, 0), (0x40, 0), (0xffffffff, 0), (0, 1), (0x33, 0x80000000)):
                            expect(call(low, high) == 1, 'unsupported clock request accepted')
                            expect([read(CFG + i * 4) for i in range(3)] == before, 'rejected request mutated gate registers')
                        report['checks'].append(dict(name='unsupported low and high uint64 bits reject atomically', passed=True))
                        reset = 0x20000540
                        baseline = read(reset)
                        generation = json.loads(qmp.command("qom-get", {"path": "/machine", "property": "mac-state"}))["wifi_reset_generation"]
                        # Single-step the actual native shim to observe the three MMIO
                        # writes, rather than infer a reset pulse from its final zero.
                        setreg(5, 0x90015964)
                        setreg(10, 4)
                        setreg(32, BASE)
                        pulse = [read(reset)]
                        for _ in range(160):
                            expect(rsp.request('s').startswith(('T', 'S')), 'reset step')
                            value = read(reset)
                            if value != pulse[-1]:
                                pulse.append(value)
                            registers = bytes.fromhex(rsp.request('g'))
                            if int.from_bytes(registers[128:132], 'little') == BASE + 8:
                                expect(int.from_bytes(registers[40:44], 'little') == 0, 'reset return error')
                                break
                        else:
                            raise AssertionError('native reset did not return within160 instructions')
                        expect(pulse == [baseline & ~16, baseline | 16, baseline & ~16], 'missing clear/set/clear Wi-Fi reset pulse')
                        report['reset_register_transitions'] = pulse
                        expect(json.loads(qmp.command("qom-get", {"path": "/machine", "property": "mac-state"}))["wifi_reset_generation"] == generation + 1, 'reset pulse did not reset modeled Wi-Fi domain once')
                        expect(read(reset) == baseline & ~16, 'reset pulse not deasserted/preserved')
                        for invalid in (0, 3, 5, 255, 256, 0xffffffff):
                            expect(call(invalid, slot=0x90015964) == 1 and read(reset) == baseline & ~16, 'invalid reset accepted/mutated')
                        report['checks'].append(dict(name='native Wi-Fi-only reset pulse returns deasserted and rejects unsupported domains', passed=True))
                        qmp.command('system_reset')
                        expect([read(CFG + i * 4) for i in range(3)] == initial, 'warm reset gate defaults changed')
                        report['checks'].append(dict(name='warm reset restores gate defaults', passed=True))
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
