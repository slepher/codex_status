#!/usr/bin/env python3
"""Native SYSCTRL virtual1MHz counter and independent MAC epoch regression."""
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
BASE, MIX = 0x62fc4000, 0x24900000


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
    token = 'ap01-sysctrl-' + uuid.uuid4().hex
    image = output.parent / (token + '.bin')
    report = dict(synthetic_fixture=True, original_rom_accessed=False,
                  full_rom_boot_verified=False, analog_rf_verified=False,
                  radio_sources=False,sysctrl_virtual_hz=1000000,real_wrap_verified=False,checks=[],
                  qemu_sha256=hashlib.sha256(qemu.read_bytes()).hexdigest())
    with output.open('x', encoding='utf-8') as result:
        try:
            with image.open('xb') as file:
                file.write(bytes(4096) + b'\x13\x00\x00\x00')
            for enabled in (False, True):
                gs = qs = qmp = process = None
                suffix = '-enabled' if enabled else '-strict'
                gdb, qmpp = (Path('/tmp') / (token + suffix + ext) for ext in ('.gdb', '.qmp'))
                deadline = time.monotonic() + 30
                machine = 'ap01,boot-mmio=on,boot2-mock=on'
                if enabled:
                    machine += ',mac-empty-mock=on,clic-mock=on'
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

                        if not enabled:
                            reject('m24900040,4');badwrite(MIX+0x40,0)
                            report['checks'].append(dict(name='default-off SYSCTRL clock aperture rejects access',passed=True))
                            continue

                        def mac_clock():
                            expect(rsp.request('Mf1000000,4:04000000')=='OK','XTAL profile')
                            expect(rsp.request('Mf1000004,4:03000000')=='OK','PLL profile')
                            expect(read(0xf1000008)==0,'explicit MAC PLL960MHz fixture')

                        mac_clock()

                        def delay(count=4096):
                            return li(6,count)+[0xfff30313,0xfe031ee3]

                        def progression():
                            words=li(5,MIX)+[lw(10,0x40)]+delay()+[lw(11,0x40)]
                            words+=delay()+[lw(12,0x40)]
                            regs=run(words)
                            diffs=[(regs[11]-regs[10])&0xffffffff,(regs[12]-regs[11])&0xffffffff]
                            expect(all(8<=d<=9 for d in diffs),'1MHz clock advances with8192 guest instructions')
                            expect(all(0<=v<=0xffffffff for v in regs[10:13]),'32bit output bounds')
                            report['native_sysctrl_ticks']=regs[10:13]
                            report['native_tick_differences']=diffs
                        check('native instruction-time1MHz progress and low32 output',progression)

                        def errors():
                            for value in (0,1,0xffffffff):badwrite(MIX+0x40,value)
                            for width in (1,2):
                                badwrite(MIX+0x40,0,width);reject(f'm{MIX+0x40:x},{width:x}')
                            reject('m24900041,4');badwrite(MIX+0x41,0)
                            # Debugger packets execute no guest instruction time.
                            before=read(MIX+0x40)
                            badwrite(MIX+0x40,0)
                            expect(read(MIX+0x40)==before,'rejected writes preserve clock epoch')
                        check('read-only, narrow and unaligned accesses reject without resetting epoch',errors)

                        def machw_reset():
                            expect(read(0x24b00120)>0,'MAC clock has advanced before soft reset')
                            words=li(5,MIX)+[lw(10,0x40)]
                            words+=li(5,0x24b08000)+li(7,1)+[sw(7,0x50)]
                            # Poll busy at8050 until finite reset completion.
                            words += [lw(8,0x50),0xfe041ee3]
                            words+=li(5,MIX)+[lw(11,0x40)]+li(5,0x24b00000)+[lw(12,0x120)]
                            regs=run(words)
                            expect(2<=((regs[11]-regs[10])&0xffffffff)<=3,'SYSCTRL remains continuous across MACHW reset')
                            expect(regs[12]==0,'MACHW completion resets MAC counter epoch only')
                            report['machw_reset_ticks']={'sysctrl_before':regs[10],'sysctrl_after':regs[11],'mac_after':regs[12]}
                        check('MACHW soft reset preserves SYSCTRL time while clearing MAC epoch',machw_reset)

                        def wifi_reset():
                            run(delay())
                            expect(read(MIX+0x40)>0 and read(0x24b00120)>0,'both counters advanced before Wi-Fi reset')
                            control=read(0x20000540)&~0x10
                            words=li(5,0x20000540)+li(7,control)+[sw(7,0)]
                            words+=li(7,control|0x10)+[sw(7,0)]
                            words+=li(5,MIX)+[lw(10,0x40)]+li(5,0x24b00000)+[lw(11,0x120)]
                            regs=run(words)
                            expect(regs[10:12]==[0,0],'Wi-Fi rising edge resets both independent epochs')
                            run(li(5,0x20000540)+li(7,control)+[sw(7,0)])
                            progression()
                        check('Wi-Fi reset clears both counters and permits subsequent progress',wifi_reset)

                        def global_reset():
                            run(delay())
                            expect(read(MIX+0x40)>0 and read(0x24b00120)>0,'both nonzero before global reset')
                            qmp.command('system_reset')
                            mac_clock()
                            expect(read(MIX+0x40)==0 and read(0x24b00120)==0,'global reset clears both epochs')
                            progression()
                        check('global reset clears both counters with later native progress',global_reset)
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
