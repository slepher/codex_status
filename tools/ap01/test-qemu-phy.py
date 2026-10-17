#!/usr/bin/env python3
"""Native bounded PHY table/pulse tests; no original ROM or physical radio access."""
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
BASE, MIX = 0x62fc4000, 0x24b08000


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
    token = 'ap01-phy-' + uuid.uuid4().hex
    image = output.parent / (token + '.bin')
    report = dict(synthetic_fixture=True, original_rom_accessed=False,
                  full_rom_boot_verified=False, analog_rf_verified=False,
                  radio_sources=False,checks=[],
                  qemu_sha256=hashlib.sha256(qemu.read_bytes()).hexdigest())
    with output.open('x', encoding='utf-8') as result:
        try:
            with image.open('xb') as file:
                file.write(bytes(4096) + b'\x13\x00\x00\x00')
            for enabled in (False, True):
                gs = qs = qmp = process = None
                suffix = '-enabled' if enabled else '-strict'
                gdb, qmpp = (Path('/tmp') / (token + suffix + ext) for ext in ('.gdb', '.qmp'))
                deadline = time.monotonic() + 18
                machine = 'ap01,boot-mmio=on,boot2-mock=on'
                if enabled:
                    machine += ',mac-empty-mock=on'
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

                        def write(address, value, size=4):
                            expect(rsp.request(f'M{address:x},{size:x}:'+value.to_bytes(size,'little').hex())=='OK','write')

                        table, reset, config = 0x24c0a000, 0x24940018, 0x24c00800
                        if not enabled:
                            for address in (table,table+2044,reset,config):
                                reject(f'm{address:x},4');badwrite(address,0)
                            report['checks'].append(dict(name='mac-empty off rejects PHY table/config/reset access',passed=True))
                            continue

                        expect(all(read(table+i*4)==0 for i in range(512)),'cold table contents not zero')
                        report['checks'].append(dict(name='cold512-word table starts zero',passed=True))
                        for address in (table,table+2044):
                            for width in (1,2):
                                reject(f'm{address:x},{width:x}');badwrite(address,1,width)
                            for delta in (1,2,3):
                                reject(f'm{address+delta:x},4');badwrite(address+delta,1)
                        for address in (table-4,table+2048):
                            reject(f'm{address:x},4');badwrite(address,1)
                        for address in (reset,):
                            for width in (1,2):
                                reject(f'm{address:x},{width:x}');badwrite(address,0,width)
                        for invalid in (1,0x10,0x12,0xffffffff):badwrite(reset,invalid)
                        write(config,0xa5b)
                        badwrite(config,0x1000)
                        expect(read(config)==0xa5b,'invalid PHYcfg write changed state')
                        report['checks'].append(dict(name='table endpoints/width/alignment/aperture and strict config/reset masks',passed=True))

                        # Real RV32 loop copies all512 source words into MMIO, no debugger fill.
                        source=0x62fc8000
                        samples=[((i*0x9e3779b9)^0xa55ac33c)&0xffffffff for i in range(512)]
                        data=b''.join(struct.pack('<I',v) for v in samples)
                        for off in range(0,len(data),512):
                            chunk=data[off:off+512]
                            expect(rsp.request(f'M{source+off:x},{len(chunk):x}:'+chunk.hex())=='OK','source write')
                        words=li(5,source)+li(6,table)+li(7,512)
                        words += [lw(8,0),sw(8,0,6),0x00428293,0x00430313,0xfff38393,0xfe0396e3]
                        run(words)
                        expect([read(table+i*4) for i in range(512)]==samples,'native512-word table copy mismatch')
                        report['table_sha256']=hashlib.sha256(data).hexdigest()
                        report['checks'].append(dict(name='native RV32 loop copies512 words with full content comparison',passed=True))

                        write(0xf1000000,4);write(0xf1000004,3)
                        expect(read(0xf1000008)==0,'PLL initialization')
                        write(0x24900068,0x8000000c)
                        write(0x24940010,0x08000000)
                        write(0x24910014,1<<22)
                        write(0x24b08074,0x80000004)
                        run(li(6,40000)+[0xfff30313,0xfe031ee3])
                        before_clock=[read(0x24b00120),read(0x24900040)]
                        before_state=json.loads(qmp.command('qom-get',{'path':'/machine','property':'mac-state'}))
                        before_regs={a:read(a) for a in (config,0x24900068,0x24940010,0x24b08074)}
                        words=li(5,reset)+li(7,0x11)+[sw(7,0),sw(7,0)]
                        words+=li(7,0)+[sw(7,0)]+li(7,0x11)+[sw(7,0)]+li(7,0)+[sw(7,0)]
                        words+=li(6,5000)+[0xfff30313,0xfe031ee3]
                        run(words)
                        after_state=json.loads(qmp.command('qom-get',{'path':'/machine','property':'mac-state'}))
                        expect(after_state['phy_reset_generation']==before_state['phy_reset_generation']+2,'rising edge pulse count/repeated held value')
                        expect(after_state['unmask_high']==before_state['unmask_high'],'MDM pulse changed MAC INTC masks')
                        expect(read(reset)==0 and all(read(a)==v for a,v in before_regs.items()),'MDM pulse changed config/clocks')
                        after_clock=[read(0x24b00120),read(0x24900040)]
                        expect(all(8<=b-a<=20 for a,b in zip(before_clock,after_clock)),'MDM reset restarted MAC/SYS time or stopped clock')
                        expect([read(table+i*4) for i in range(512)]==samples,'MDM pulse cleared PHY table')
                        report['clock_before_us']=before_clock;report['clock_after_us']=after_clock
                        report['checks'].append(dict(name='two native MDM rising pulses preserve full PHY table/config/MAC state and advancing clocks',passed=True))
                        qmp.command('system_reset')
                        expect(all(read(table+i*4)==0 for i in range(512)),'VM reset did not clear PHY table')
                        expect(read(config)==0 and read(reset)==0,'VM reset did not clear PHYcfg/reset latch')
                        report['checks'].append(dict(name='VM reset clears table and PHY configuration',passed=True))
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
