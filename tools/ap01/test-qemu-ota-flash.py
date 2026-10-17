#!/usr/bin/env python3
"""Native inactive-slot NOR OTA guard and snapshot checks; no original ROM/device."""
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
    token = 'ap01-ota-flash-' + uuid.uuid4().hex
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
    with output.open('x', encoding='utf-8') as result:
        try:
            report['profiles'] = []
            for mode, active1 in (('off', False), ('fw0-active', False), ('fw1-active', True)):
                process = gs = qs = qm = None
                gdb, qmp_path = (Path('/tmp') / (token + mode + ext) for ext in ('.gdb', '.qmp'))
                deadline = time.monotonic() + 20
                expected = bytearray(nor)
                machine = f'ap01,boot-mmio=on,boot2-mock=on,rom-shim={shim},flash-fixture={fixture}'
                if mode != 'off': machine += ',ota-flash-mock=on'
                if active1: machine += ',ota-fw1-active=on'
                command = [str(qemu), '-M', machine, '-cpu', 'rv32', '-kernel', str(image), '-S',
                           '-display', 'none', '-serial', 'none', '-monitor', 'none',
                           '-gdb', f'unix:{gdb},server=on,wait=off', '-qmp', f'unix:{qmp_path},server=on,wait=off']
                try:
                    with (output.parent / (token + mode + '.log')).open('x') as log:
                        process = subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=log, stderr=log, preexec_fn=probe.child_limits)
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

                        put(RAM, bytes(256));put(RAM,b'\x01');put(RAM+12,b'\x04')
                        active = 0x7b0000 if active1 else 0x10000
                        inactive, inactive_end = (0x10000,0x710000) if active1 else (0x7b0000,0xeb0000)
                        inactive_pt, active_pt = (0xe000,0xf000) if active1 else (0xf000,0xe000)
                        expect(native(329,0,0)==active,'active XIP getter')
                        expect(native(449,RAM+0x100)==0,'Opt enter')
                        expect(native(447,RAM,RAM+0x104,0,0)==0 and read(RAM+0x104)==active,'State_Save active offset')
                        expect(native(446,RAM,active,0,0)==0 and native(450,0)==0,'State_Restore active offset')
                        expect(native(365,0,0,0)==0 and native(329,0,0)==0,'clear XIP offset')
                        expect(native(365,active,0,0)==0 and native(329,0,0)==active,'restore selected XIP offset')
                        other=0x10000 if active1 else 0x7b0000
                        expect(native(365,other,0,0)==1 and native(329,0,0)==active,'inactive XIP offset accepted')

                        def contents(address, length=4):
                            expect(native(444,RAM,address,RAM+0x400,length,0,0)==0,'native readback')
                            return get(RAM+0x400,length)
                        def program(address, data, allowed):
                            before=contents(address,len(data));put(RAM+0x200,data)
                            expect(native(448,RAM,address,RAM+0x200,len(data),0,0)==(0 if allowed else 1),'program guard')
                            if allowed: expected[address:address+len(data)]=data
                            expect(contents(address,len(data))==(data if allowed else before),'program changed wrong bytes')
                        def erase(address, allowed):
                            before=contents(address)
                            expect(native(440,RAM,address,0x1000,0,0)==(0 if allowed else 1),'erase guard')
                            if allowed: expected[address:address+0x1000]=b'\xff'*0x1000
                            expect(contents(address)==(b'\xff'*4 if allowed else before),'erase changed wrong region')
                        allowed=mode!='off'
                        for address in (inactive,inactive_end-0x1000,inactive_pt):
                            erase(address,allowed)
                            program(address,b'\xf0\x55\xaa\x00',allowed)
                        program(inactive_end-4,b'\x12\x34\x56\x78',allowed)
                        protected=(0,active_pt,active,active+0x700000-4,0x720000,0xff0000)
                        for address in protected:
                            program(address,bytes(4),False)
                            erase(address&~0xfff,False)
                        # First byte before each OTA aperture is never admitted as part of it.
                        for address in (inactive-4,inactive_pt-4):
                            program(address,bytes(4),False)
                        for address in (0x710000,0xeb0000):
                            erase(address,True);program(address,b'\x5a\xa5\x11\x00',True)
                        snapshot=output.parent/(token+mode+'-snapshot.bin')
                        qm.command('qom-set',{'path':'/machine','property':'flash-snapshot','value':str(snapshot)})
                        expect(snapshot.read_bytes()==expected,'snapshot differs from full expected NOR')
                        try:
                            qm.command('qom-set',{'path':'/machine','property':'flash-snapshot','value':str(snapshot)})
                        except RuntimeError:
                            pass
                        else:
                            raise AssertionError('snapshot overwrote existing path')
                        expect(snapshot.read_bytes()==expected,'failed O_EXCL snapshot changed existing file')
                        qm.command('system_reset')
                        put(RAM,bytes(256));put(RAM,b'\x01');put(RAM+12,b'\x04')
                        expect(native(329,0,0)==active,'reset lost selected active offset')
                        for address in (inactive,inactive_pt,0x710000,0xeb0000):
                            expect(contents(address)==expected[address:address+4],'global reset lost NOR contents')
                        after=output.parent/(token+mode+'-after-reset.bin')
                        qm.command('qom-set',{'path':'/machine','property':'flash-snapshot','value':str(after)})
                        expect(after.read_bytes()==expected,'global reset changed full NOR snapshot')
                        expect(hashlib.sha256(fixture.read_bytes()).hexdigest()==fixture_hash,'input fixture file changed')
                        report['profiles'].append(dict(mode=mode,passed=True,active_offset=hex(active),snapshot=str(snapshot),snapshot_sha256=hashlib.sha256(expected).hexdigest()))
                finally:
                    if qm is not None: qm.file.close()
                    for sock in (gs,qs):
                        if sock is not None:sock.close()
                    if process is not None:
                        if process.poll() is None:
                            process.terminate()
                            try:process.wait(timeout=2)
                            except subprocess.TimeoutExpired:process.kill();process.wait(timeout=2)
                        for path in (gdb,qmp_path):
                            if path.exists():path.unlink()
            report['passed']=True
        except Exception as exc:
            report.update(passed=False,error=f'{type(exc).__name__}: {exc}')
        json.dump(report,result,indent=2);result.write('\n')
    print(json.dumps(report))
    return 0 if report['passed'] else 1


if __name__=='__main__':sys.exit(main())
