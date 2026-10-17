#!/usr/bin/env python3
"""Native synthetic empty MAC INTC tests; no radio event sources or IRQ delivery."""
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
BASE, MIX = 0x62fc4000, 0x24910000


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
    token = 'ap01-mac-' + uuid.uuid4().hex
    image = output.parent / (token + '.bin')
    report = dict(synthetic_fixture=True, original_rom_accessed=False,
                  full_rom_boot_verified=False, analog_rf_verified=False,
                  radio_sources=False,irq_delivery_implemented=False,checks=[],
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
                    machine += ',mac-empty-mock=on'
                command = [str(qemu), '-M', machine, '-cpu', 'rv32', '-kernel', str(image),
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

                        def run(words):
                            # Trap reaches the terminal breakpoint, rather than
                            # spinning at reset mtvec=0 until the host timeout.
                            end = BASE + (len(words)+3)*4
                            words = li(31,end)+[0x305f9073]+words
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
                            reject('m24910000,4')
                            badwrite(MIX+0x10,1)
                            badwrite(0x24a00034,0xc);reject('m24a00034,4')
                            badwrite(0x24f10388,0);reject('m24f10388,4')
                            report['checks'].append(dict(name='default-off aperture rejects accesses',passed=True))
                            continue

                        def state():
                            return json.loads(qmp.command('qom-get',{'path':'/machine','property':'mac-state'}))

                        def verify_masks(low,high):
                            st=state()
                            expect(st['unmask_low']==low and st['unmask_high']==high,'mask W1S state')
                            expect(st['pending_low']==0 and st['pending_high']==0,'no external radio pending')

                        def empty():
                            regs=run(li(5,MIX)+[lw(10,0),lw(11,4)])
                            expect(regs[10]==regs[11]==0,'native status reads empty')
                            reject('m24910040,4')
                            reject('m24910010,4');reject('m24910014,4')
                            verify_masks(0,0)
                        check('empty status, unavailable index and unsupported SET reads',empty)

                        def masks():
                            expected=[0xff001c00,0x20f5050a]
                            words=li(5,MIX)
                            for bank,value in enumerate(expected):
                                words+=li(7,value)+[sw(7,0x10+bank*4)]
                            run(words);verify_masks(*expected)
                            report['sdk_init_mask_inputs']=expected.copy()
                            for bank in range(2):
                                for bit in range(32):
                                    value=1<<bit
                                    run(li(5,MIX)+li(7,value)+[sw(7,0x10+bank*4)])
                                    expected[bank]|=value
                                    verify_masks(*expected)
                            for bank in range(2):
                                run(li(5,MIX)+li(7,0)+[sw(7,0x10+bank*4)])
                            verify_masks(0xffffffff,0xffffffff)
                            report['all64_mask_inputs_preserved']=True
                        check('SDK init inputs and all64 native W1S mask bits OR without generating events',masks)

                        def dma_platform():
                            before=state()['platform_zero_writes']
                            words=li(5,0x24a00000)+li(7,0xc)+[sw(7,0x34)]
                            words+=li(7,0x1ffff)+[sw(7,0x18)]
                            words+=li(5,0x24f10380)+li(7,0)+[sw(7,8),sw(7,8)]
                            run(words)
                            st=state()
                            expect(st['dma_arbitration']==0xc and st['dma_unmask']==0x1ffff,'observed DMA init writes latched')
                            expect(st['platform_zero_writes']==before+2,'platform zero write counter')
                            for addr in (0x24a00034,0x24a00018,0x24f10388):reject(f'm{addr:x},4')
                            for addr,value in ((0x24a00034,0),(0x24a00034,0xd),(0x24a00018,0),
                                               (0x24a00018,0x1fffe),(0x24f10388,1),
                                               (0x24a00000,0x62fc4000),(0x24a00004,1),(0x24a00008,1)):
                                badwrite(addr,value)
                            for addr in (0x24a00034,0x24a00018,0x24f10388):
                                badwrite(addr,0,1);badwrite(addr,0,2)
                            expect(state()==st,'DMA/platform rejected operations atomic')
                            verify_masks(0xffffffff,0xffffffff)
                            report['dma_platform_fixture']={k:st[k] for k in ('dma_arbitration','dma_unmask','platform_zero_writes')}
                        check('native bounded DMA init/platform zero writes; reads, roots, starts and unknown values reject',dma_platform)


                        def invalid():
                            for off in (0,4,0x40):badwrite(MIX+off,0)
                            for off in (8,0xc,0x18,0x20,0x44):
                                reject(f'm{MIX+off:x},4')
                                badwrite(MIX+off,0)
                            for width in (1,2):
                                badwrite(MIX+0x10,0,width)
                                reject(f'm{MIX:x},{width:x}')
                            badwrite(MIX+0x11,1)
                            reject('m24910001,4')
                            verify_masks(0xffffffff,0xffffffff)
                            expect(read(MIX)==read(MIX+4)==0,'errors leave pending empty')
                        check('RO, unknown, unaligned and non32bit accesses reject atomically',invalid)

                        def wifi_reset():
                            before=state()['wifi_reset_generation']
                            control=read(0x20000540)&~0x10
                            # Native RMW-equivalent low/high pulse, then held high.
                            words=li(5,0x20000540)+li(7,control)+[sw(7,0)]
                            words+=li(7,control|0x10)+[sw(7,0)]
                            run(words)
                            verify_masks(0,0)
                            expect(all(state()[k]==0 for k in ('dma_arbitration','dma_unmask','platform_zero_writes')),'Wi-Fi reset clears DMA/platform fixtures')
                            expect(state()['wifi_reset_generation']==before+1,'one rising edge resets Wi-Fi domain')
                            run(li(5,0x20000540)+li(7,control|0x10)+[sw(7,0)])
                            expect(state()['wifi_reset_generation']==before+1,'held reset does not create another edge')
                            run(li(5,0x20000540)+li(7,control)+[sw(7,0)])
                            run(li(5,MIX)+li(7,0xffffffff)+[sw(7,0x10),sw(7,0x14)])
                            verify_masks(0xffffffff,0xffffffff)
                            report['wifi_reset_generation_after_pulse']=before+1
                        check('native Wi-Fi reset rising edge clears masks once and allows fresh W1S',wifi_reset)


                        def reset():
                            qmp.command('system_reset')
                            verify_masks(0,0)
                            expect(all(state()[k]==0 for k in ('dma_arbitration','dma_unmask','platform_zero_writes')),'warm reset clears DMA/platform fixtures')
                            expect(read(MIX)==read(MIX+4)==0,'reset remains empty')
                            reject('m24910040,4')
                            run(li(5,MIX)+li(7,0x80000001)+[sw(7,0x14)])
                            verify_masks(0,0x80000001)
                        check('warm reset clears masks and preserves explicit no-source behavior',reset)
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
