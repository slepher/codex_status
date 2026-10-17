#!/usr/bin/env python3
"""Bounded synthetic UART integration; no original ROM or device access."""
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
spec = importlib.util.spec_from_file_location(
    'ap01_probe', Path(__file__).with_name('run-qemu-ap01.py'))
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)


def require(condition, label):
    if not condition:
        raise AssertionError(label)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--qemu', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    output, qemu = args.output.resolve(), args.qemu.resolve()
    if output.exists() or not output.parent.is_dir() or not Path('/tmp').is_dir():
        parser.error('requires existing Linux /tmp and new output in existing directory')
    token = 'ap01-uart-' + uuid.uuid4().hex
    image = output.parent / (token + '.bin')
    image.write_bytes(bytes(4096) + bytes.fromhex('1300000073001000'))
    report = dict(synthetic_fixture=True, original_rom_accessed=False,
                  full_rom_boot_verified=False, clic_dispatch_verified=False,
                  checks=[], fixture=str(image), qemu=str(qemu),
                  qemu_sha256=hashlib.sha256(qemu.read_bytes()).hexdigest(),
                  sessions=[])
    deadline = time.monotonic() + 30
    with output.open('x', encoding='utf-8') as result:
        try:
            for connected in (True, False):
                suffix = 'file' if connected else 'disconnected'
                gdb, qmp_path = (Path('/tmp') / (token + suffix + ext)
                                 for ext in ('.gdb', '.qmp'))
                log_path = output.parent / (token + suffix + '.log')
                sink = output.parent / (token + '.uart.bin')
                process = gs = qs = qm = None
                try:
                    command = [str(qemu), '-M', 'ap01,boot-mmio=on,boot2-mock=on',
                               '-cpu', 'rv32', '-kernel', str(image), '-S',
                               '-display', 'none', '-monitor', 'none',
                               '-gdb', f'unix:{gdb},server=on,wait=off',
                               '-qmp', f'unix:{qmp_path},server=on,wait=off']
                    command += (['-chardev', f'file,id=uart,path={sink}',
                                 '-serial', 'chardev:uart', '-serial', 'none']
                                if connected else ['-serial', 'none'])
                    report['sessions'].append(dict(command=command, log=str(log_path),
                                                   byte_sink=str(sink) if connected else None))
                    with log_path.open('x') as log:
                        process = subprocess.Popen(command, stdin=subprocess.DEVNULL,
                                                   stdout=log, stderr=log,
                                                   preexec_fn=probe.child_limits)
                        gs = probe.connect(gdb, process, deadline)
                        qs = probe.connect(qmp_path, process, deadline)
                        rsp, qm = probe.RSP(gs, deadline), probe.QMP(qs, deadline)
                        rsp.request('?')

                        def read(offset, uart=0):
                            return int.from_bytes(bytes.fromhex(rsp.request(
                                f'm{0x2000a000 + uart * 256 + offset:x},4')), 'little')

                        def write(offset, value, uart=0, size=4):
                            reply = rsp.request(f'M{0x2000a000 + uart * 256 + offset:x},{size:x}:'
                                                + value.to_bytes(size, 'little').hex())
                            require(reply == 'OK', f'write {offset:x}: {reply}')

                        def reject(offset, value=None):
                            address = 0x2000a000 + offset
                            request = (f'm{address:x},4' if value is None else
                                       f'M{address:x},4:' + value.to_bytes(4, 'little').hex())
                            try:
                                response = rsp.request(request)
                            except RuntimeError as exc:
                                if str(exc).startswith('RSP '):
                                    return
                                raise
                            require(response.startswith('E'), f'unsupported access accepted: {request}')

                        def check(name, function):
                            report['active_check'] = name
                            function()
                            report['checks'].append(dict(name=name, passed=True))

                        def reset_state():
                            for uart in (0, 1):
                                require(read(0, uart) == 0, 'synthetic TX reset')
                                require(read(0x24, uart) == 0xfff, 'reset IRQ mask')
                                require(read(0x2c, uart) == 0, 'reset IRQ enable')
                                require(read(0x84, uart) == 32, 'reset TX free/RX empty')
                                require(read(0x30, uart) == 0, 'reset idle')
                        check(suffix + ': reset state on both UARTs', reset_state)

                        if connected:
                            def transmit():
                                write(0, 1)
                                payload = b'AP01 UART\r\n\x00\xff'
                                for byte in payload:
                                    write(0x88, byte, size=1)
                                require(sink.read_bytes() == payload, 'chardev bytes differ')
                                require(read(0x84) == 32 and read(0x30) == 0, 'TX did not drain')
                                require(read(0x20) & 1, 'actual drain did not set TX END')
                            check('file backend receives exact FIFO bytes', transmit)

                            def interrupts():
                                write(0x2c, 5)
                                write(0x24, 0xffa)
                                require(read(0x2c) == 5 and read(0x24) == 0xffa, 'IRQ latches')
                                require(read(0x20) & 5 == 5, 'END/FIFO status absent')
                                write(0x28, 1)
                                require(read(0x20) == 4 and read(0x28) == 0, 'W1C END')
                                write(0x84, 31 << 16)
                                require(read(0x20) & 4, 'free32 must exceed threshold31')
                                write(0x24, 0xfff)
                                require(read(0x24) == 0xfff, 'IRQ mask restore')
                            check('interrupt mask/enable, W1C and threshold', interrupts)

                            def unsupported():
                                require(read(0x84) & 0x3f00 == 0, 'phantom RX input')
                                reject(0x8c)
                                before = read(0)
                                reject(0, before | 2)
                                require(read(0) == before, 'CTS reject changed config')
                                reject(0x80, 1)
                                require(read(0x80) == 0, 'DMA reject changed state')
                                reject(0x60)
                            check('RX empty, CTS/DMA/unknown fail closed', unsupported)
                        else:
                            def bounded_fifo():
                                write(0, 1)
                                write(0x84, 31 << 16)
                                write(0x88, 0x41)
                                require(read(0x84) & 63 == 31, 'disconnected byte lost')
                                require(read(0x20) & 4 == 0, 'threshold not recomputed')
                                for byte in range(31):
                                    write(0x88, byte)
                                require(read(0x84) & 63 == 0 and read(0x30) == 1, 'full/busy state')
                                require(read(0x20) & 1 == 0, 'disconnected TX falsely completed')
                                reject(0x88, 0x42)
                                write(0x80, 4)
                                require(read(0x84) & 63 == 32 and read(0x30) == 0, 'FIFO clear')
                                write(0x88, 0x55)
                                qm.command('system_reset')
                                reset_state()
                            check('disconnected FIFO bounded, threshold, clear and reset', bounded_fifo)
                finally:
                    if qm:
                        try:
                            qm.command('quit')
                        except Exception:
                            pass
                    for sock in (gs, qs):
                        if sock:
                            sock.close()
                    if process:
                        try:
                            process.wait(timeout=2)
                        except subprocess.TimeoutExpired:
                            process.kill()
                            process.wait(timeout=2)
                    for path in (gdb, qmp_path):
                        path.unlink(missing_ok=True)
            report['passed'] = True
        except Exception as exc:
            report['passed'] = False
            report['error'] = f'{type(exc).__name__}: {exc}'
            raise
        finally:
            json.dump(report, result, indent=2)
            result.write('\n')
    print(json.dumps(report, indent=2))


if __name__ == '__main__':
    main()
