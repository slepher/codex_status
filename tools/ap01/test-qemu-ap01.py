#!/usr/bin/env python3
"""Synthetic integration checks; these do not prove original AP01 ROM boot."""
import argparse
import importlib.util
import json
from pathlib import Path
import re
import socket
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


def expect(value, wanted, label):
    if value != wanted:
        raise AssertionError(f'{label}: got {value!r}, expected {wanted!r}')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--qemu', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    output, qemu = args.output.resolve(), args.qemu.resolve()
    if output.exists() or not output.parent.is_dir():
        parser.error('output must be a new JSON file in an existing directory')
    if not Path('/tmp').is_dir():
        parser.error('requires an existing Linux /tmp directory')
    checks = []
    report = {'synthetic_fixture': True, 'full_rom_boot_verified': False,
              'original_rom_accessed': False, 'checks': checks}
    prefix = 'ap01-test-' + uuid.uuid4().hex
    image = output.parent / (prefix + '.bin')
    otp_path = output.parent / (prefix + '.otp.bin')
    otp_data = bytes((i * 17) & 255 for i in range(2048))
    # Exact historical INSB pattern; separate reserved encoding flips bit 22.
    words = [0xac000077 | (5 << 7) | (6 << 15) | (byte << 20)
             for byte in range(4)]
    words += [0xac000077 | (6 << 15),
              0xac000077 | (5 << 7) | (5 << 15) | (2 << 20),
              words[0] ^ (1 << 22)]
    fixture = b'\0' * 0x1000 + b''.join(w.to_bytes(4, 'little') for w in words)

    def check(label, function):
        try:
            function()
            checks.append({'name': label, 'passed': True})
        except Exception as exc:
            checks.append({'name': label, 'passed': False,
                           'error': f'{type(exc).__name__}: {exc}'})
            raise

    with output.open('x', encoding='utf-8') as result:
        try:
            with image.open('xb') as fixture_file:
                fixture_file.write(fixture)
            with otp_path.open('xb') as otp_file:
                otp_file.write(otp_data)
            report['otp_fixture'] = str(otp_path)
            for enabled in (True, False, 'board', 'otp', 'psram', 'boot2'):
                deadline = time.monotonic() + 30
                suffix = enabled if isinstance(enabled, str) else ('enabled' if enabled else 'disabled')
                gdb = Path('/tmp') / (prefix + '-' + suffix + '.gdb')
                qmp_path = Path('/tmp') / (prefix + '-' + suffix + '.qmp')
                log_path = output.parent / (prefix + '-' + suffix + '.log')
                process = gs = qs = qmp = None
                try:
                    with log_path.open('x') as log:
                        process = subprocess.Popen(
                            [str(qemu), '-M', ('ap01,mhint-latch=on,mtvt-latch=on,'
                                               'cache-csr-latches=on,boot-mmio=on,'
                                               'efuse-shadow=on' +
                                               (',otp-fixture=' + str(otp_path)
                                                if enabled == 'otp' else '') +
                                               (',psram-mock=on' if enabled == 'psram' else '') +
                                               (',boot2-mock=on' if enabled == 'boot2' else ''))
                             if enabled in ('board', 'otp', 'psram', 'boot2') else 'ap01', '-cpu',
                             'rv32,x-ap01-p=' + ('on' if enabled else 'off'),
                             '-kernel', str(image), '-S', '-display', 'none',
                             '-serial', 'none', '-monitor', 'none',
                             '-gdb', f'unix:{gdb},server=on,wait=off',
                             '-qmp', f'unix:{qmp_path},server=on,wait=off'],
                            stdin=subprocess.DEVNULL, stdout=log, stderr=log,
                            preexec_fn=probe.child_limits)
                        gs = probe.connect(gdb, process, deadline)
                        qs = probe.connect(qmp_path, process, deadline)
                        rsp, qmp = probe.RSP(gs, deadline), probe.QMP(qs, deadline)
                        rsp.request('?')

                        def setreg(number, value):
                            expect(rsp.request(f'P{number:x}=' +
                                               value.to_bytes(4, 'little').hex()),
                                   'OK', 'register write')

                        def getreg(number):
                            raw = bytes.fromhex(rsp.request('g'))
                            return int.from_bytes(raw[number * 4:number * 4 + 4],
                                                  'little')

                        def step(pc):
                            setreg(32, pc)
                            response = rsp.request('s')
                            if not response.startswith(('T', 'S')):
                                raise AssertionError(f'unexpected stop: {response}')

                        def aliases(addresses):
                            for index, address in enumerate(addresses):
                                data = (0x12345678 ^ index).to_bytes(4, 'little').hex()
                                expect(rsp.request(f'M{address + 0x100:x},4:{data}'),
                                       'OK', 'RAM write')
                                for alias in addresses:
                                    expect(rsp.request(f'm{alias + 0x100:x},4'),
                                           data, 'shared RAM alias')

                        def illegal(pc):
                            # Parent's board may supply a vendor mtvec mode;
                            # mcause is the authoritative illegal-opcode check.
                            step(pc)
                            text = qmp.command('human-monitor-command',
                                               {'command-line': 'info registers'})
                            match = re.search(r'\bmcause\s+([0-9a-fA-F]+)', text)
                            if not match:
                                raise AssertionError('mcause absent from HMP registers: ' + text)
                            expect(int(match.group(1), 16), 2, 'illegal instruction mcause')

                        if enabled in ('board', 'otp', 'psram', 'boot2'):
                            def write(address, value):
                                expect(rsp.request(f'M{address:x},4:' +
                                                   value.to_bytes(4, 'little').hex()),
                                       'OK', 'MMIO write')

                            def read(address):
                                return int.from_bytes(bytes.fromhex(
                                    rsp.request(f'm{address:x},4')), 'little')

                            def rejected(request):
                                try:
                                    reply = rsp.request(request)
                                except RuntimeError as exc:
                                    if str(exc).startswith('RSP '):
                                        return
                                    raise
                                raise AssertionError(f'unsupported access accepted: {request}: {reply}')

                            def register_mask():
                                expect(read(0x2000e500), 0x21, 'MMIO reset')
                                write(0x2000e500, 0xffffffff)
                                expect(read(0x2000e500), 0x3f, 'MMIO write mask')
                            check('boot MMIO reset and mask', register_mask)
                            check('unknown MMIO rejected', lambda: rejected('m20000004,4'))
                            check('uninitialized eFuse shadow rejected', lambda: rejected('m20056000,4'))

                            def shadow():
                                write(0x20056000, 0x89abcdef)
                                expect(read(0x20056000), 0x89abcdef, 'shadow roundtrip')
                            check('eFuse initialized shadow roundtrip', shadow)
                            if enabled != 'otp':
                                check('OTP trigger rejected', lambda: rejected('M20056800,4:10000000'))
                            if enabled == 'board':
                                check('strict PSRAM request rejected', lambda: rejected('M20052000,4:01c081a0'))

                            def clicinfo():
                                initial = read(0xe0800004)
                                expect(initial, (4 << 21) | 80, 'CLICINFO reset')
                                write(0xe0800004, 0xffffffff)
                                expect(read(0xe0800004), initial, 'CLICINFO readonly')
                            check('CLICINFO readonly', clicinfo)

                            # Native guest CSR instructions, not debugger CSR writes.
                            csr_pc = 0x62fc0200
                            csr_words = [(0x7c5 << 20) | (6 << 15) | (1 << 12) | (7 << 7) | 0x73,
                                         (0x7c5 << 20) | (2 << 12) | (8 << 7) | 0x73]
                            for index, word in enumerate(csr_words):
                                write(csr_pc + index * 4, word)

                            def mhint():
                                setreg(6, 0x00108000)
                                step(csr_pc)
                                expect(getreg(7), 0, 'MHINT old reset value')
                                expect(getreg(32), csr_pc + 4, 'CSRRW next PC')
                                step(csr_pc + 4)
                                expect(getreg(8), 0x00108000, 'MHINT latch roundtrip')
                                expect(getreg(32), csr_pc + 8, 'CSRRS next PC')
                            check('MHINT native CSR latch', mhint)

                            def reset_model():
                                qmp.command('system_reset')
                                # system_reset schedules work; wait for reset's
                                # visible PC before checking peripheral state.
                                while getreg(32) != ENTRY:
                                    if time.monotonic() >= deadline:
                                        raise TimeoutError('system_reset did not complete')
                                    time.sleep(0.001)
                                expect(read(0x2000e500), 0x21, 'MMIO reset restoration')
                                rejected('m20056000,4')
                                step(csr_pc + 4)
                                expect(getreg(8), 0, 'MHINT reset restoration')
                            check('system reset restores MMIO CSR and shadow validity', reset_model)
                            if enabled == 'boot2':
                                def unsupported_rom_api():
                                    expect(read(0x200005c4), 0x5a1740ef, 'synthetic cached JEDEC')
                                    expect(read(0x90015d00), 0x90000000, 'ROM flash config slot')
                                    rejected('m90015800,4')
                                    setreg(1, 0x62fc0900)
                                    setreg(10, 0x1740ef)
                                    step(0x90000000)
                                    step(0x90000004)
                                    expect(getreg(10), 0xffffffff, 'explicit unsupported result')
                                    expect(getreg(32), 0x62fc0900, 'unsupported API ABI return')
                                    setreg(1, 0x62fc0900)
                                    setreg(10, 5)
                                    for pc in range(0x90000040, 0x90000050, 4):
                                        step(pc)
                                    expect(getreg(10), 1, 'MCU PLL clock requires PLL initialized')
                                check('Boot2 cached flash ID and unsupported ROM API', unsupported_rom_api)

                                def clock_state():
                                    expect(read(0x900159cc), 0x90000020, 'ROM clock slot')
                                    expect(read(0xf100000c), 0, 'clock initially uninitialized')
                                    setreg(1, 0x62fc0900)
                                    setreg(10, 4)
                                    setreg(11, 3)
                                    for pc in range(0x90000020, 0x90000034, 4):
                                        step(pc)
                                    expect(getreg(10), 0, 'supported clock request')
                                    expect(getreg(32), 0x62fc0900, 'clock API ABI return')
                                    expect(read(0xf100000c), 40000000, 'virtual XTAL frequency')
                                    expect(read(0xf1000010), 960000000, 'virtual WiFi PLL frequency')
                                    expect(read(0xf1000014), 491520000, 'virtual audio PLL frequency')
                                    expect(read(0x2000f10c) & 255, 0x84, 'XTAL type valid marker')
                                    expect(read(0x2000f030) & 3, 1, 'MCU root remains XTAL')
                                    write(0xf1000000, 255)
                                    write(0xf1000004, 3)
                                    before = read(0x2000f030)
                                    expect(read(0xf1000008), 1, 'unsupported clock rejected')
                                    expect(read(0x2000f030), before, 'failed clock request preserves state')
                                    expect(read(0xf100000c), 40000000, 'failed request preserves frequency')
                                    expect(read(0x90015a40), 0x90000040, 'MCU system clock ROM slot')
                                    setreg(1, 0x62fc0900)
                                    setreg(10, 5)
                                    for pc in range(0x90000040, 0x90000050, 4):
                                        step(pc)
                                    expect(getreg(10), 0, 'MCU PLL clock accepted after PLL setup')
                                    expect(getreg(32), 0x62fc0900, 'MCU clock API ABI return')
                                    expect(read(0xf1000028), 320000000, 'virtual MCU frequency')
                                    previous = read(0x2000f030)
                                    write(0xf1000020, 255)
                                    expect(read(0xf1000024), 1, 'unsupported MCU clock rejected')
                                    expect(read(0xf1000028), 320000000, 'invalid request preserves MCU frequency')
                                    expect(read(0x2000f030), previous, 'invalid request preserves MCU root')
                                check('virtual clock API changes state and rejects unsupported input', clock_state)

                                def xtal_getter():
                                    expect(read(0x90015b8c), 0x90000060, 'XTAL getter slot')
                                    destination = 0x62fc0700

                                    def call(pointer):
                                        setreg(1, 0x62fc0900)
                                        setreg(10, pointer)
                                        pc = 0x90000060
                                        for _ in range(20):
                                            step(pc)
                                            pc = getreg(32)
                                            if pc == 0x62fc0900:
                                                return getreg(10)
                                        raise AssertionError('XTAL getter did not return')

                                    write(destination, 0xaabbccdd)
                                    expect(call(destination), 0, 'valid XTAL getter')
                                    expect(read(destination), 0xaabbcc04, 'getter writes one byte')
                                    write(0x2000f10c, 0)
                                    write(destination, 0xaabbccdd)
                                    expect(call(destination), 1, 'invalid XTAL marker rejected')
                                    expect(read(destination), 0xaabbccdd, 'failed getter preserves output')
                                    expect(call(0), 1, 'null XTAL pointer rejected')
                                check('XTAL getter reflects virtual state and preserves failed outputs', xtal_getter)
                            if enabled == 'psram':
                                ctrl = 0x20052000
                                request = 0xa081c201  # WB vendor, CR0, REQ.

                                def grant():
                                    write(ctrl, request)
                                    expect(read(ctrl) & (1 << 17), 0, 'first poll grant pending')
                                    expect(read(ctrl) & (1 << 17), 1 << 17, 'second poll grant')

                                def release():
                                    write(ctrl, request & ~(1 << 16))
                                    expect(read(ctrl) & 0x33000, 0, 'release drops grant and pulses')

                                def request_cycle():
                                    grant()
                                    release()
                                    write(ctrl, request)
                                    release()
                                    expect(read(ctrl) & (1 << 17), 0, 'cancel pending grant')
                                check('PSRAM finite request grant release cancel', request_cycle)

                                def config_write():
                                    grant()
                                    old = read(0x20052014)
                                    write(0x20052010, 0x40003785)
                                    expect(read(0x20052014), old, 'configuration latch not committed')
                                    write(ctrl, request | 0x1000)
                                    expect(read(0x20052014), old, 'pending write leaves status unchanged')
                                    first = read(ctrl)
                                    expect(first & 0x4000, 0, 'write first poll not done')
                                    expect(read(0x20052014), old, 'first poll leaves status unchanged')
                                    second = read(ctrl)
                                    expect(second & 0x5000, 0x4000, 'write second poll done pulse cleared')
                                    expect(read(0x20052014), 0x40003785, 'committed modeled status')
                                check('PSRAM config write commits after two polls', config_write)

                                def config_read():
                                    write(ctrl, request | 0x2000)
                                    expect(read(ctrl) & 0x8000, 0, 'read first poll not done')
                                    expect(read(ctrl) & 0xa000, 0x8000, 'read second poll done pulse cleared')
                                    expect(read(0x20052004) >> 16, 0x3785, 'modeled status readback')
                                    write(0x20052014, 0xffffffff)
                                    expect(read(0x20052014), 0x40003785, 'status readonly')
                                check('PSRAM config read returns modeled status and status readonly', config_read)

                                def reject_command(word=None, packet=None):
                                    before = read(ctrl)
                                    old_status = read(0x20052014)
                                    if packet is None:
                                        packet = f'M{ctrl:x},4:' + word.to_bytes(4, 'little').hex()
                                    rejected(packet)
                                    expect(read(ctrl), before, 'rejection preserves control')
                                    expect(read(0x20052014), old_status, 'rejection preserves status')

                                def invalid_commands():
                                    release()
                                    reject_command(request | 0x1000)  # No grant.
                                    grant()
                                    reject_command(request | 0x3000)  # Dual pulse.
                                    reject_command(packet='M20052001,1:10')
                                    write(ctrl, (request & ~0x700) | 0x300)  # CR1 unsupported.
                                    reject_command((request & ~0x700) | 0x300 | 0x2000)
                                    write(ctrl, request)
                                check('PSRAM unauthorized selector3 dual and partial commands reject atomically', invalid_commands)

                                def synthetic_id():
                                    write(ctrl, request & ~0x700)
                                    write(ctrl, (request & ~0x700) | 0x2000)
                                    expect(read(ctrl) & 0x8000, 0, 'ID0 read pending')
                                    expect(read(ctrl) & 0xa000, 0x8000, 'ID0 read complete')
                                    expect(read(0x20052004) >> 16, 0, 'synthetic ID0 zero')
                                    reject_command((request & ~0x700) | 0x1000)
                                    write(ctrl, request)
                                check('PSRAM synthetic ID0 readonly zero and write rejection', synthetic_id)

                                def reset_pending_psram():
                                    write(ctrl, request | 0x1000)
                                    reset_model()
                                    expect(read(ctrl), 0xa080c002, 'reset restores control')
                                    expect(read(ctrl) & 0x33000, 0, 'reset cancels command and grant')
                                    expect(read(0x20052014), 0x40003382, 'reset restores modeled status')
                                check('PSRAM reset cancels pending command and grant', reset_pending_psram)
                            if enabled == 'otp':
                                def configure():
                                    write(0x20056800, 0x24bf00)

                                def trigger():
                                    write(0x20056800, 0x24bf10)

                                def complete():
                                    first = read(0x20056800)
                                    expect(bool(first & 4), True, 'OTP first read busy')
                                    expect(bool(first & 2), False, 'OTP first read not done')
                                    second = read(0x20056800)
                                    expect(second & 0x16, 2, 'OTP second read done and not busy/trigger')

                                def compare_bank0():
                                    for offset in range(0, 512, 64):
                                        expect(rsp.request(f'm{0x20056000 + offset:x},40'),
                                               otp_data[offset:offset + 64].hex(), 'OTP bank0 fixture')
                                    rejected('m20056200,4')

                                def read_transaction():
                                    rejected('m20056000,4')
                                    configure()
                                    trigger()
                                    complete()
                                    compare_bank0()
                                check('OTP fixture automatic bank0 load', read_transaction)

                                def rejected_transactions():
                                    configure()
                                    original_ctrl = read(0x20056800)
                                    original_shadow = read(0x20056000)
                                    rejected('M20056800,4:18bf2400')
                                    expect(read(0x20056800), original_ctrl, 'program reject preserves CTRL0')
                                    expect(read(0x20056000), original_shadow, 'program reject preserves shadow')
                                    rejected('M20056800,1:10')
                                    expect(read(0x20056800), original_ctrl, 'partial reject preserves CTRL0')
                                    trigger()
                                    rejected('M20056800,4:00bf2400')
                                    complete()
                                    compare_bank0()
                                check('OTP programming partial trigger and busy write rejected', rejected_transactions)

                                def reload_fixture():
                                    write(0x20056000, 0xdeadbeef)
                                    configure()
                                    trigger()
                                    complete()
                                    compare_bank0()
                                    expect(otp_path.read_bytes(), otp_data, 'immutable OTP fixture')
                                check('OTP reload replaces guest mutation from immutable fixture', reload_fixture)

                                def cancel_busy():
                                    configure()
                                    trigger()
                                    reset_model()
                                    expect(read(0x20056800), 0x240003, 'OTP reset cancels transaction')
                                    rejected('m20056000,4')
                                    expect(otp_path.read_bytes(), otp_data, 'reset preserves immutable fixture')
                                check('OTP reset cancels busy and invalidates shadow', cancel_busy)
                        elif enabled:
                            check('internal RAM shared four ways', lambda: aliases(
                                [0x20fc0000, 0x22fc0000, 0x60fc0000, 0x62fc0000]))
                            check('PSRAM shared three ways', lambda: aliases(
                                [0x88000000, 0xa8000000, 0xd8000000]))
                            for byte in range(4):
                                def byte_case(byte=byte):
                                    old, source = 0x12345678, 0xabcdef93
                                    setreg(5, old)
                                    setreg(6, source)
                                    pc = ENTRY + byte * 4
                                    step(pc)
                                    mask = 0xff << (byte * 8)
                                    expect(getreg(5), (old & ~mask) | (0x93 << (byte * 8)),
                                           'INSB result')
                                    expect(getreg(6), source, 'INSB source preservation')
                                    expect(getreg(32), pc + 4, 'INSB next PC')
                                check(f'INSB byte {byte}', byte_case)

                            def zero_case():
                                setreg(6, 0xabcdef93)
                                step(ENTRY + 16)
                                expect(getreg(0), 0, 'x0 destination')
                                expect(getreg(6), 0xabcdef93, 'x0 source preservation')
                                expect(getreg(32), ENTRY + 20, 'x0 next PC')
                            check('INSB x0 destination', zero_case)

                            def alias_case():
                                setreg(5, 0x12345678)
                                step(ENTRY + 20)
                                expect(getreg(5), 0x12785678, 'rd equals rs1')
                                expect(getreg(32), ENTRY + 24, 'alias next PC')
                            check('INSB rd equals rs1', alias_case)
                            check('reserved encoding illegal', lambda: illegal(ENTRY + 24))
                        else:
                            check('disabled extension illegal', lambda: illegal(ENTRY))
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
            report['fixture'] = str(image)
            json.dump(report, result, indent=2)
            result.write('\n')
    print(json.dumps(report))
    return 0 if report['passed'] else 1


if __name__ == '__main__':
    sys.exit(main())
