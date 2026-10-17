#!/usr/bin/env python3
"""Native synthetic Flash API checks; neither ROM nor fixture contains original firmware."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import struct
import subprocess
import sys
import time
import uuid

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location('probe', Path(__file__).with_name('run-qemu-ap01.py'))
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)
RAM, RETURN, MAIL = 0x20fc4000, 0x20fc1000, 0xf10000c0
SLOTS = {329, 358, 359, 365, 367, 368, 369, 371, 374, 377, 379, 384, 385, 387, 389, 392, 393, 401, 402,
         440, 444, 446, 447, 448, 449, 450}


def expect(ok, message):
    if not ok:
        raise AssertionError(message)


class NativeFault(Exception):
    pass


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--qemu', type=Path, required=True)
    parser.add_argument('--rom-shim', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    qemu, shim, output = args.qemu.resolve(), args.rom_shim.resolve(), args.output.resolve()
    if output.exists() or not output.parent.is_dir() or not Path('/tmp').is_dir():
        parser.error('requires Linux /tmp and a new output in an existing directory')
    functions = struct.unpack_from('<512I', shim.read_bytes())
    expect(all(0x90002000 <= functions[s] < 0x90011000 for s in SLOTS), 'requires Flash v9 shim')
    token = 'ap01-flash-' + uuid.uuid4().hex
    image = output.parent / (token + '.bin')
    fixture = output.parent / (token + '.nor.bin')
    log_path = output.parent / (token + '.log')
    image.write_bytes(bytes(4096) + bytes.fromhex('13000000'))
    pattern = bytes((i * 7 + 19) & 255 for i in range(64))
    nor = bytearray(b'\xff' * 0x1000000)
    nor[0xe000:0xe040] = pattern
    with fixture.open('xb') as stream:
        stream.write(nor)
    fixture_hash = hashlib.sha256(nor).hexdigest()
    report = dict(synthetic_fixture=True, original_rom_accessed=False,
                  full_rom_boot_verified=False, checks=[], qemu=str(qemu),
                  qemu_sha256=hashlib.sha256(qemu.read_bytes()).hexdigest(),
                  shim_sha256=hashlib.sha256(shim.read_bytes()).hexdigest(),
                  fixture=str(fixture), fixture_sha256=fixture_hash)
    process = gs = qs = qm = None
    gdb, qmp_path = (Path('/tmp') / (token + ext) for ext in ('.gdb', '.qmp'))
    deadline = time.monotonic() + 30
    with output.open('x', encoding='utf-8') as result:
        try:
            command = [str(qemu), '-M', f'ap01,boot-mmio=on,boot2-mock=on,rom-shim={shim},flash-fixture={fixture}',
                       '-cpu', 'rv32', '-kernel', str(image), '-S', '-display', 'none',
                       '-serial', 'none', '-monitor', 'none',
                       '-gdb', f'unix:{gdb},server=on,wait=off',
                       '-qmp', f'unix:{qmp_path},server=on,wait=off']
            report.update(command=command, log=str(log_path))
            with log_path.open('x') as log:
                process = subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=log,
                                           stderr=log, preexec_fn=probe.child_limits)
                gs = probe.connect(gdb, process, deadline)
                qs = probe.connect(qmp_path, process, deadline)
                rsp, qm = probe.RSP(gs, deadline), probe.QMP(qs, deadline)
                rsp.request('?')

                def put(address, data):
                    expect(rsp.request(f'M{address:x},{len(data):x}:' + data.hex()) == 'OK', 'memory write failed')

                def write(address, value):
                    put(address, value.to_bytes(4, 'little'))

                def get(address, length=4):
                    return bytes.fromhex(rsp.request(f'm{address:x},{length:x}'))

                def read(address):
                    return int.from_bytes(get(address), 'little')

                def setreg(index, value):
                    expect(rsp.request(f'P{index:x}=' + value.to_bytes(4, 'little').hex()) == 'OK', 'register write')

                def regs():
                    return struct.unpack('<33I', bytes.fromhex(rsp.request('g'))[:132])

                def native(slot, *values):
                    expect(read(0x90015800 + 4 * slot) == functions[slot], 'BootROM native pointer differs')
                    arguments = list(values) + [0xa5000000 + i for i in range(len(values), 6)]
                    for index, value in enumerate(arguments):
                        setreg(10 + index, value)
                    setreg(1, RETURN)
                    setreg(32, functions[slot])
                    for _ in range(40):
                        rsp.request('s')
                        registers = regs()
                        if registers[32] == RETURN:
                            return registers[10]
                        if registers[32] == 0:
                            text = qm.command('human-monitor-command', {'command-line': 'info registers'})
                            match = re.search(r'\bmcause\s+([0-9a-fA-F]+)', text)
                            cause = int(match.group(1), 16) if match else None
                            expect(cause in (5, 7), f'unexpected native trap {cause}')
                            raise NativeFault(f'slot{slot}: bus exception{cause}')
                    raise AssertionError('native wrapper did not return within40 steps')

                def reject(address, value):
                    try:
                        reply = rsp.request(f'M{address:x},4:' + value.to_bytes(4, 'little').hex())
                    except RuntimeError as exc:
                        if str(exc).startswith('RSP '):
                            return
                        raise
                    expect(reply.startswith('E'), 'invalid mailbox request accepted')

                def check(name, function):
                    report['active_check'] = name
                    qm.command('system_reset')
                    put(RAM, bytes(256))
                    put(RAM, b'\x01')  # SPI config ioMode1 for strict SFlash_Read.
                    put(RAM + 12, b'\x04')  # SDK sector_size in KiB.
                    function()
                    report['checks'].append(dict(name=name, passed=True))

                def transaction():
                    put(RAM + 0x100, b'\xaa\xbb\xcc\xdd')
                    expect(native(449, RAM + 0x100) == 0, 'Opt_Enter failed')
                    expect(get(RAM + 0x100, 4) == b'\x00\xbb\xcc\xdd', 'AES output width/content')
                    expect(native(447, RAM, RAM + 0x104, 0, 0) == 0, 'State_Save failed')
                    expect(read(RAM + 0x104) == 0x10000, 'saved XIP offset')
                    expect(native(446, RAM, 0x10000, 0, 0) == 0, 'State_Restore failed')
                    expect(native(450, 0) == 0, 'Opt_Exit failed')
                check('native Opt Enter/Save/Restore/Exit and one-byte AES output', transaction)

                def read_paths():
                    expect(native(444, RAM, 0xe000, RAM + 0x200, 64, 0, 0) == 0, 'XIP read')
                    expect(get(RAM + 0x200, 64) == pattern, 'XIP read fabricated data')
                    expect(native(358, 1) == 0, 'Sbus replace')
                    expect(native(379, RAM, 1, 0, 0xe000, RAM + 0x240, 64) == 0, 'SFlash_Read')
                    expect(get(RAM + 0x240, 64) == pattern, 'raw SFlash bytes')
                    expect(native(359) == 0, 'Sbus revoke')
                check('native XIP and Sbus/SFlash reads exact fixture bytes', read_paths)

                def raw_save_restore():
                    # Fixed SDK raw State_Save/Restore leaf sequence. This is
                    # native calls to the modeled APIs, not a call to an
                    # unavailable/decrypted Boot2 implementation.
                    put(RAM, b'\x04')  # QIO; no 32-bit-address capability flag.
                    expect(native(449, RAM + 0x100) == 0, 'raw sequence Opt_Enter')
                    expect(native(367, 0) == 0, 'raw sequence SAHB owner')
                    expect(native(401, RAM) == 0, 'reset continuous read')
                    expect(native(393, RAM) == 0, 'disable burst wrap')
                    expect(native(385, RAM, 1) == 1, 'unsupported32bit flag falsely succeeded')
                    expect(native(377, RAM) == 0, 'QIO QE enable')
                    expect(native(393, RAM) == 0, 'second deburst')
                    saved = native(329, 0, 0)
                    expect(saved == 0x10000, 'raw getter initial offset')
                    expect(native(365, 0, 0, 0) == 0, 'raw clear offset')
                    expect(native(329, 0, 0) == 0, 'offset was not cleared')
                    expect(native(365, saved, 0, 0) == 0, 'raw restore offset')
                    expect(native(402, RAM) == 0, 'restore burst wrap')
                    expect(native(379, RAM, 4, 1, 0, RAM + 0x200, 4) == 0,
                           'restore continuous-read probe')
                    expect(get(RAM + 0x200, 4) == b'\xff' * 4, 'restore read fabricated bytes')
                    expect(native(387, RAM, 4, 1, 0, 32, 0) == 0, 'restore IDbus config')
                    expect(native(450, 0) == 0, 'raw sequence Opt_Exit')
                    expect(native(329, 0, 0) == saved, 'raw sequence changed restored offset')
                    # Exercise the address-width leaf's supported domain
                    # separately; this does not claim the16MiB NOR needs it.
                    put(RAM, b'\x24')
                    expect(native(385, RAM, 1) == 0, 'flagged32bit enable')
                    expect(native(385, RAM, 0) == 0, 'flagged32bit disable')
                check('native raw StateSave/Restore leaves with QIO and address-mode domain', raw_save_restore)

                def invalid_modes():
                    put(RAM, b'\x04')
                    expect(native(379, RAM, 4, 0, 0xe000, RAM + 0x200, 4) == 1,
                           'quad read succeeded without QE')
                    # A rejected QE attempt using a non-QSPI config must not
                    # enable subsequent QSPI reads.
                    put(RAM + 0x80, b'\x01\0\0\0')
                    expect(native(377, RAM + 0x80) == 1, 'invalid QE config accepted')
                    expect(native(379, RAM, 4, 0, 0xe000, RAM + 0x200, 4) == 1,
                           'rejected QE attempt changed mode state')
                    expect(native(377, RAM) == 0, 'valid QE enable')
                    expect(native(379, RAM, 4, 0, 0xe000, RAM + 0x200, 4) == 0, 'valid QIO read')
                    for offset, group, bank in ((0x20000, 0, 0), (0, 1, 0), (0, 0, 1)):
                        expect(native(365, offset, group, bank) == 1, 'invalid offset/group/bank accepted')
                        expect(native(329, 0, 0) == 0x10000, 'rejected offset changed state')
                    expect(native(367, 2) == 1, 'unknown owner accepted')
                    expect(native(385, RAM, 1) == 1, 'unflagged32bit enable accepted')
                    put(RAM, b'\x24')
                    expect(native(385, RAM, 2) == 1, 'invalid32bit boolean accepted')
                    for io, cont, address, length, bank in ((3, 1, 0, 32, 0),
                            (4, 2, 0, 32, 0), (4, 1, 4, 32, 0),
                            (4, 1, 0, 16, 0), (4, 1, 0, 32, 1)):
                        expect(native(387, RAM, io, cont, address, length, bank) == 1,
                               'unsupported IDbus mode accepted')
                        expect(native(329, 0, 0) == 0x10000, 'rejected IDbus changed offset')
                    expect(native(379, RAM, 4, 0, 0xe000, RAM + 0x200, 4) == 0,
                           'rejected mode requests corrupted valid QSPI state')
                    expect(get(RAM + 0x200, 4) == pattern[:4], 'readback after rejected modes')
                check('invalid raw mode requests preserve observable offset/QSPI state', invalid_modes)

                def nor_program():
                    put(RAM + 0x200, b'\xf0\x55\xaa\x00')
                    expect(native(448, RAM, 0x710000, RAM + 0x200, 4, 0, 0) == 0, 'NOR1to0')
                    put(RAM + 0x200, b'\xff' * 4)
                    expect(native(448, RAM, 0x710000, RAM + 0x200, 4, 0, 0) == 1, 'NOR0to1 accepted')
                    expect(native(444, RAM, 0x710000, RAM + 0x240, 4, 0, 0) == 0, 'read programmed')
                    expect(get(RAM + 0x240, 4) == b'\xf0\x55\xaa\x00', 'failed program changed NOR')
                    expect(native(440, RAM, 0x710000, 0x1000, 0, 0) == 0, 'aligned sector erase')
                    expect(native(444, RAM, 0x710000, RAM + 0x240, 4, 0, 0) == 0, 'read erase')
                    expect(get(RAM + 0x240, 4) == b'\xff' * 4, 'erase did not set FF')
                check('native data-only NOR program1to0, reject0to1, sector erase', nor_program)

                def flash_state():
                    return json.loads(qm.command('qom-get', {'path': '/machine', 'property': 'flash-state'}))

                def read_flash(address, length=4):
                    expect(native(444, RAM, address, RAM + 0x400, length, 0, 0) == 0, 'leaf readback')
                    return get(RAM + 0x400, length)

                def leaf_program(address, data=b'\x00\x00\x00\x00'):
                    put(RAM + 0x200, data)
                    expect(native(374, RAM, 1, address, RAM + 0x200, len(data)) == 0, 'native SFlash_Program')

                def erase_leaves():
                    for slot, address, length, parameter in (
                            (384, 0xeb0000, 4096, 0xeb0),
                            (368, 0xeb8000, 32768, 0xeb8000 // 32768),
                            (369, 0xec0000, 65536, 0xec),
                            (371, 0xed0000, 4096, 0xed0fff)):
                        leaf_program(address)
                        leaf_program(address + length - 4)
                        leaf_program(address + length)
                        before = flash_state()
                        expect(native(389, RAM) == 0, 'write enable')
                        expect(native(392, RAM) == 0, 'idle before synchronous leaf')
                        values = (RAM, address, parameter) if slot == 371 else (RAM, parameter)
                        expect(native(slot, *values) == 0, f'erase leaf{slot}')
                        expect(native(392, RAM) == 0, 'leaf returned with pending busy work')
                        expect(read_flash(address) == b'\xff' * 4, 'erase beginning')
                        expect(read_flash(address + length - 4) == b'\xff' * 4, 'erase ending')
                        expect(read_flash(address + length) == bytes(4), 'erase crossed boundary')
                        after = flash_state()
                        expect(after['erase_count'] == before['erase_count'] + 1, 'real erase counter')
                    expect(native(367, 0) == 0 and flash_state()['owner'] == 0, 'SAHB state getter')
                    expect(native(367, 1) == 0 and flash_state()['owner'] == 1, 'IAHB state getter')
                check('native sector block32 block64 inclusive erase mutate exact resource ranges', erase_leaves)

                def resource_program_protection():
                    # Resource partition is EB0000..FF0000 exclusive; Boot2,
                    # firmware, gaps and final reserved64KiB remain protected.
                    expect(native(384, RAM, 0xefe) == 0, 'erase resource test sector')
                    leaf_program(0xefe000, b'\xf0\x55\xaa\x00')
                    before = flash_state()['program_count']
                    put(RAM + 0x200, b'\xff' * 4)
                    expect(native(374, RAM, 1, 0xefe000, RAM + 0x200, 4) == 1, 'raw NOR0to1 rejection')
                    expect(read_flash(0xefe000) == b'\xf0\x55\xaa\x00', 'NOR rejection mutated data')
                    expect(flash_state()['program_count'] == before, 'rejected program counted')
                    for address in (0, 0x10000, 0x720000, 0xeafffc, 0xff0000):
                        old = read_flash(address)
                        expect(native(374, RAM, 1, address, RAM + 0x200, 4) == 1, 'protected raw program')
                        expect(read_flash(address) == old, 'protected bytes changed')
                    for slot, values in ((384, (RAM, 0xeaf)), (384, (RAM, 0xff0)),
                                         (368, (RAM, 0xea8000 // 32768)),
                                         (369, (RAM, 0xff)),
                                         (371, (RAM, 0xeaf000, 0xeb0fff)),
                                         (371, (RAM, 0xfef000, 0xff0fff)),
                                         (371, (RAM, 0xed0001, 0xed0fff)),
                                         (371, (RAM, 0xed0000, 0xed0ffe)),
                                         (384, (RAM, 0xffffffff))):
                        count = flash_state()['erase_count']
                        expect(native(slot, *values) == 1, f'protected/unaligned erase{slot}')
                        expect(flash_state()['erase_count'] == count, 'rejected erase counted')
                    put(RAM + 12, b'\x08')
                    expect(native(384, RAM, 0xeb0) == 1, 'unknown sector geometry accepted')
                    put(RAM + 12, b'\x04')
                    expect(native(374, RAM, 4, 0xefe004, RAM + 0x200, 4) == 1, 'wrong program mode accepted')
                    try:
                        native(374, RAM, 1, 0xefe004, 0x2000a000, 4)
                    except NativeFault:
                        pass
                    else:
                        raise AssertionError('raw program MMIO source accepted')
                check('resource boundary native program and erase protection are atomic', resource_program_protection)

                def errors():
                    for values in ((RAM, 0xfffff0, RAM + 0x200, 64, 0, 0),
                                   (RAM, 0xe000, RAM + 0x200, 4, 1, 0),
                                   (RAM, 0xe000, RAM + 0x200, 4, 0, 1)):
                        expect(native(444, *values) == 1, 'bounds/group/bank accepted')
                    expect(native(448, RAM, 0x10000, RAM + 0x200, 4, 0, 0) == 1, 'non-data program')
                    expect(native(440, RAM, 0x710001, 0x1000, 0, 0) == 1, 'unaligned erase')
                    try:
                        native(444, RAM, 0xe000, 0x2000a000, 4, 0, 0)
                    except NativeFault:
                        pass
                    else:
                        raise AssertionError('invalid output pointer accepted')
                    qm.command('system_reset')
                    write(MAIL, RAM)
                    reject(MAIL + 0x18, 444)  # partial args
                    expect(native(444, RAM, 0xe000, RAM + 0x200, 4, 0, 0) == 0, 'fresh request after partial')
                    reject(MAIL + 0x18, 444)  # no fresh argument writes
                    for i in range(6):
                        write(MAIL + i * 4, 0)
                    reject(MAIL + 0x18, 511)
                check('domain ERROR, invalid RAM, partial/stale/unknown mailbox refusal', errors)

                def reset_retains():
                    put(RAM + 0x200, b'\x5a')
                    expect(native(448, RAM, 0x710000, RAM + 0x200, 1, 0, 0) == 0, 'program before reset')
                    expect(native(449, RAM + 0x100) == 0, 'enter before reset')
                    qm.command('system_reset')
                    expect(native(450, 0) == 1, 'reset retained active transaction')
                    expect(native(444, RAM, 0x710000, RAM + 0x240, 1, 0, 0) == 0, 'read after reset')
                    expect(get(RAM + 0x240, 1) == b'\x5a', 'reset erased programmed byte')
                check('warm reset retains NOR bytes and clears transaction state', reset_retains)
                expect(hashlib.sha256(fixture.read_bytes()).hexdigest() == fixture_hash, 'fixture disk bytes changed')
                report.update(passed=True, fixture_disk_unchanged=True)
        except Exception as exc:
            report.update(passed=False, error=f'{type(exc).__name__}: {exc}')
            raise
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
            json.dump(report, result, indent=2)
            result.write('\n')
    print(json.dumps(report, indent=2))


if __name__ == '__main__':
    main()
