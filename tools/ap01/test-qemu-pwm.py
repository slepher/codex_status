#!/usr/bin/env python3
"""Native bounded PWMv2 register and stop/start checks; no waveform or IRQ claim."""
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
BASE, MIX = 0x62fc4000, 0x2000a400


def expect(ok, message):
    if not ok:
        raise AssertionError(message)


def li(reg, value):
    high = (value + 0x800) & 0xfffff000
    return [high | reg << 7 | 0x37,
            ((value - high) & 0xfff) << 20 | reg << 15 | reg << 7 | 0x13]


def sw(reg, offset, base=5):
    return (offset >> 5) << 25 | reg << 20 | base << 15 | 2 << 12 | (offset & 31) << 7 | 0x23


def lw(reg, offset, base=5):
    return offset << 20 | base << 15 | 2 << 12 | reg << 7 | 3


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('qemu', 'output'):
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    qemu, output = args.qemu.resolve(), args.output.resolve()
    if output.exists() or not output.parent.is_dir() or not Path('/tmp').is_dir():
        parser.error('requires Linux /tmp and new output in an existing directory')
    token = 'ap01-pwm-' + uuid.uuid4().hex
    image = output.parent / (token + '.bin')
    report = dict(synthetic_fixture=True, original_rom_accessed=False,
                  full_rom_boot_verified=False, analog_rf_verified=False,
                  waveform_verified=False,stop_start_virtual_ns=2000,checks=[],
                  qemu_sha256=hashlib.sha256(qemu.read_bytes()).hexdigest())
    with output.open('x', encoding='utf-8') as result:
        try:
            with image.open('xb') as file:
                file.write(bytes(4096) + b'\x13\x00\x00\x00')
            for enabled in (True,):
                gs = qs = qmp = process = None
                suffix = '-enabled' if enabled else '-strict'
                gdb, qmpp = (Path('/tmp') / (token + suffix + ext) for ext in ('.gdb', '.qmp'))
                deadline = time.monotonic() + 30
                machine = 'ap01,boot-mmio=on,boot2-mock=on'

                command = [str(qemu), '-M', machine, '-cpu', 'rv32,x-ap01-clic=on', '-kernel', str(image),
                           '-S', '-display', 'none', '-serial', 'none', '-monitor', 'none',
                           '-icount', 'shift=0,align=off,sleep=off',
                           '-gdb', f'unix:{gdb},server=on,wait=off', '-qmp', f'unix:{qmpp},server=on,wait=off']
                try:
                    with (output.parent / (token + suffix + '.log')).open('x') as log:
                        process = subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=log,
                                                   stderr=log, preexec_fn=probe.child_limits)
                        gs = probe.connect(gdb, process, deadline)
                        qs = probe.connect(qmpp, process, deadline)
                        rsp, qmp = probe.RSP(gs, deadline), probe.QMP(qs, deadline)
                        rsp.request('?')

                        def read(addr):
                            return int.from_bytes(bytes.fromhex(rsp.request(f'm{addr:x},4')), 'little')

                        def reject(packet):
                            try:
                                rsp.request(packet)
                            except RuntimeError as exc:
                                expect(str(exc).startswith('RSP '), 'wrong rejection')
                            else:
                                raise AssertionError('unsupported request accepted: ' + packet)

                        def badwrite(addr, value, size=4):
                            reject(f'M{addr:x},{size:x}:' + value.to_bytes(size, 'little').hex())

                        def run(words, trap_vector=None):
                            # Trap reaches the terminal breakpoint, rather than
                            # spinning at reset mtvec=0 until the host timeout.
                            end = BASE + (len(words)+3)*4
                            words = li(31,end if trap_vector is None else trap_vector)+[0x305f9073]+words
                            data = b''.join(struct.pack('<I', w) for w in words + [0x13])
                            for offset in range(0,len(data),512):
                                chunk=data[offset:offset+512]
                                expect(rsp.request(f'M{BASE+offset:x},{len(chunk):x}:'+chunk.hex())=='OK','program chunk write')
                            end = BASE + len(words) * 4
                            expect(rsp.request('P20=' + BASE.to_bytes(4, 'little').hex()) == 'OK', 'set PC')
                            expect(rsp.request(f'Z0,{end:x},4') == 'OK', 'breakpoint')
                            try:
                                expect(rsp.request('c').startswith(('S', 'T')), 'continue')
                                raw = bytes.fromhex(rsp.request('g'))
                                expect(int.from_bytes(raw[128:132], 'little') == end, 'unexpected native trap')
                                return [int.from_bytes(raw[i:i+4], 'little') for i in range(0, 132, 4)]
                            finally:
                                rsp.request(f'z0,{end:x},4')

                        def check(name, fn):
                            fn()
                            report['checks'].append(dict(name=name, passed=True))

                        def write(address,value):
                            expect(rsp.request(f'M{address:x},4:'+value.to_bytes(4,'little').hex())=='OK','register write')
                        def delay():
                            return li(6,400)+[0xfff30313,0xfe031ee3]
                        def wait():
                            run(delay()*4)
                        report['transitions']=[]
                        for offset in (0x40,0x80):
                            address=MIX+offset
                            expect(read(address)==0x28000000,'synthetic stopped reset')
                            other=MIX+(0x80 if offset==0x40 else 0x40)
                            untouched=read(other)
                            for target, expected in ((0,[1,1,1,0]),(1<<27,[0,0,0,1])):
                                words=li(5,address)+li(7,target)+[sw(7,0),lw(10,0)]
                                words+=delay()+[lw(11,0),sw(7,0)]+delay()+[lw(12,0)]+delay()+[lw(13,0)]
                                registers=run(words)
                                states=[(registers[i]>>29)&1 for i in range(10,14)]
                                expect(states==expected,'finite2us transition/repeated target restarted deadline')
                                expect(all(((registers[i]>>27)&1)==bool(target) for i in range(10,14)),'STOP_EN latch changed')
                                expect(read(other)==untouched,'group stop/start affected other bank')
                                report['transitions'].append(dict(group_offset=hex(offset),target_stopped=bool(target),native_statuses=states))
                            # RO status writes cannot fake a stopped running group.
                            write(address,0);wait()
                            write(address,1<<29)
                            expect(read(address)==0,'STOP_STS was writable')
                            write(address,1<<27);write(address,0);wait()
                            expect(read(address)==0,'canceled stop completed after reversal')
                        report['checks'].append(dict(name='two independent native2us start/stop transitions, repeated target, ROstatus and reversal',passed=True))
                        for offset in (0x40,0x80):
                            address=MIX+offset
                            write(address,0x88001234);wait()
                            expect(read(address)==0xa8001234,'clock/divider fields or stopped status')
                            for index, off in enumerate((4,8,12,16,20,24,28)):
                                value=(0x12345678^(index*0x11111111))&0xffffffff
                                write(address+off,value)
                                expect(read(address+off)==value,'channel/period/deadtime latch mismatch')
                            for off in (36,44):
                                write(address+off,0x7ff)
                                expect(read(address+off)==0x7ff,'interrupt config latch')
                                badwrite(address+off,0x800)
                            expect(read(address+32)==0,'fabricated PWM interrupt')
                            badwrite(address+32,0)
                            write(address+40,0x7ff)
                            reject(f'm{address+40:x},4')
                            badwrite(address+40,0x800)
                            old=read(address)
                            for invalid in (1<<16,1<<17,1<<18,3<<30):badwrite(address,invalid)
                            expect(read(address)==old,'invalid config mutated bank')
                            for width in (1,2):
                                reject(f'm{address:x},{width:x}');badwrite(address,0,width)
                            for delta in (1,2,3):
                                reject(f'm{address+delta:x},4');badwrite(address+delta,0)
                        for offset in (4,0x3c,0x70,0x7c,0xb0,0xbc,0xc0):
                            reject(f'm{MIX+offset:x},4');badwrite(MIX+offset,0)
                        write(MIX,0x300);expect(read(MIX)==0,'global IRQ fabricated')
                        badwrite(MIX,4)
                        report['checks'].append(dict(name='SDK field latches, threshold channels, strict masks/width/alignment and no fabricated IRQ',passed=True))
                        # Pending transition and all programmed registers are canceled/reset.
                        write(MIX+0x40,0)
                        qmp.command('system_reset')
                        wait()
                        for offset in (0x40,0x80):
                            expect(read(MIX+offset)==0x28000000,'VM reset failed to stop/cancel transition')
                            for off in (4,8,12,16,20,24,28,32,36,44):
                                expect(read(MIX+offset+off)==0,'VM reset retained configuration')
                        report['checks'].append(dict(name='global reset clears both banks and cancels in-flight start',passed=True))
                finally:
                    if qmp is not None: qmp.file.close()
                    for sock in (gs,qs):
                        if sock is not None: sock.close()
                    if process is not None:
                        if process.poll() is None:
                            process.terminate()
                            try: process.wait(timeout=2)
                            except subprocess.TimeoutExpired:
                                process.kill(); process.wait(timeout=2)
                        for path in (gdb,qmpp):
                            if path.exists(): path.unlink()
            report['passed']=True
        except Exception as exc:
            report.update(passed=False,error=f'{type(exc).__name__}: {exc}')
        finally:
            json.dump(report,result,indent=2); result.write('\n')
    print(json.dumps(report))
    return 0 if report['passed'] else 1


if __name__=='__main__':
    sys.exit(main())
