#!/usr/bin/env python3
"""Native synthetic MACHW reset transactions; no physical timing claim."""
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
    token = 'ap01-machw-' + uuid.uuid4().hex
    image = output.parent / (token + '.bin')
    report = dict(synthetic_fixture=True, original_rom_accessed=False,
                  full_rom_boot_verified=False, analog_rf_verified=False,
                  radio_sources=False,reset_completion_virtual_ns=2000,checks=[],
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

                        def state():
                            return json.loads(qmp.command('qom-get',{'path':'/machine','property':'machw-state'}))

                        if not enabled:
                            reject('m24b08050,4')
                            badwrite(MIX+0x50,1)
                            report['checks'].append(dict(name='default-off reset aperture rejects accesses',passed=True))
                            continue

                        def short_delay():
                            # About802 guest instructions/ns under icount shift0.
                            return li(6,400)+[0xfff30313,0xfe031ee3]

                        def completion():
                            before=state()['generation']
                            words=li(5,MIX)+li(7,1)+[sw(7,0x50),lw(10,0x50)]
                            words+=short_delay()+[lw(11,0x50),sw(7,0x50)]
                            words+=short_delay()+[lw(12,0x50)]
                            words+=short_delay()+[lw(13,0x50)]
                            regs=run(words)
                            expect(regs[10:14]==[1,1,1,0],'busy before2us, finite completion and held1 no restart')
                            expect(state()['generation']==before+1 and not state()['busy'],'one completed generation')
                            report['native_reset_statuses']=regs[10:14]
                        check('native2us reset, busy polls and repeated1 without restart',completion)

                        def invalid():
                            before=state().copy()
                            for value in (0,2,3,0xffffffff):badwrite(MIX+0x50,value)
                            for off in (0,0x4c,0x54,0x58):
                                badwrite(MIX+off,1);reject(f'm{MIX+off:x},4')
                            for width in (1,2):
                                badwrite(MIX+0x50,1,width);reject(f'm{MIX+0x50:x},{width:x}')
                            badwrite(MIX+0x51,1)
                            expect(state()==before,'rejected commands have no state effects')
                        check('zero/unknown commands, unknown offsets and access widths reject atomically',invalid)

                        def cancel():
                            before=state()['generation']
                            expect(rsp.request('M24b08050,4:01000000')=='OK','start reset')
                            expect(read(MIX+0x50)==1,'debugger does not advance virtual time')
                            qmp.command('system_reset')
                            run(short_delay()*3)
                            expect(read(MIX+0x50)==0 and state()['generation']==before,'global reset cancels without completion')
                            expect(rsp.request('M24b08050,4:01000000')=='OK','start Wi-Fi cancel case')
                            control=read(0x20000540)&~0x10
                            words=li(5,0x20000540)+li(7,control)+[sw(7,0)]
                            words+=li(7,control|0x10)+[sw(7,0)]
                            words+=short_delay()*3
                            run(words)
                            expect(read(MIX+0x50)==0 and state()['generation']==before,'Wi-Fi reset cancels without callback')
                            run(li(5,0x20000540)+li(7,control)+[sw(7,0)])
                        check('global and Wi-Fi reset cancel pending commands without generation',cancel)

                        def external_masks():
                            words=li(5,0x24910000)+li(7,0x80000001)+[sw(7,0x10)]
                            words+=li(7,0x40000002)+[sw(7,0x14)]
                            run(words)
                            masks=json.loads(qmp.command('qom-get',{'path':'/machine','property':'mac-state'}))
                            completion()
                            after=json.loads(qmp.command('qom-get',{'path':'/machine','property':'mac-state'}))
                            expect(after['unmask_low']==masks['unmask_low']==0x80000001 and
                                   after['unmask_high']==masks['unmask_high']==0x40000002,'MACHW reset preserves external INTC masks')
                            expect(after['pending_low']==after['pending_high']==0,'no reset-generated interrupt')
                        check('completed MACHW reset preserves external masks and generates no pending IRQ',external_masks)

                        def write(addr, value, size=4):
                            expect(rsp.request(f'M{addr:x},{size:x}:' + value.to_bytes(size, 'little').hex()) == 'OK', 'write')

                        def wait_us(us=8):
                            # Native RV32 execution drives QEMU virtual time (icount1ns/instruction).
                            run(li(6, us*500)+[0xfff30313,0xfe031ee3])

                        def local_fsm():
                            write(0x24b00038, 0x3f)
                            expect(read(0x24b00038) == 0x33, 'ACTIVE acceptance was not synchronous')
                            write(0x24b00038, 0x30)  # repeated accepted target does not restart
                            wait_us()
                            expect(read(0x24b00038) == 0x33 and read(MIX+0x6c) == 0, 'ACTIVE completed with spurious event')
                            badwrite(0x24b00038, 0x20)
                            write(0x24b00038, 3)
                            expect(read(0x24b00038) == 3, 'IDLE did not retain ACTIVE while pending')
                            badwrite(0x24b00038, 0x30)
                            wait_us()
                            expect(read(0x24b00038) == 0 and read(MIX+0x6c) == 4, 'IDLE completion missing GEN2')
                            expect(read(0x24910004) == 0, 'masked INTC54 appeared pending')
                            write(MIX+0x74, 4)
                            write(0x24910014, 1 << 22)
                            expect(read(0x24910004) == 0, 'GEN master gate ignored')
                            write(MIX+0x74, 0x80000004)
                            expect(read(0x24910004) == 1 << 22 and read(0x24910040) == 54, 'GEN did not route through INTC54')
                            expect(read(0xe0801118) & 1, 'raw70 IP absent')
                            expect(not read(0xe0801134) & 1, 'spurious raw77 route')
                            # Real common trap consumes INTC index54 and ACKs GEN2, then mret.
                            handler = 0x62fc6000
                            words = [0x34202a73] + li(5,0x24910000) + [lw(22,0x40)]
                            words += li(5,MIX)+li(7,4)+[sw(7,0x70),0x001a8a93,0x30200073]
                            data = b''.join(struct.pack('<I', w) for w in words)
                            expect(rsp.request(f'M{handler:x},{len(data):x}:'+data.hex())=='OK','handler')
                            write(0xe0801119,1,1)
                            expect(rsp.request('P15=00000000')=='OK','clear handler count')
                            regs = run(li(5,0)+[0x30429073]+li(5,8)+[0x30029073,0x13]+li(5,0)+[0x30029073], handler|3)
                            expect(regs[20] == 0x80000046 and regs[21] == 1 and regs[22] == 54, 'real common trap/mret route mismatch')
                            expect(read(MIX+0x6c)==0 and read(0x24910004)==0,'ACK failed to lower source54')
                            reject('m24910040,4')
                            write(0xe0801119,0,1)
                            report['local_trap'] = dict(mcause=hex(regs[20]),index=regs[22],deliveries=regs[21])
                        check('native ACTIVE/IDLE state completion, GEN gating, INTC54/raw70 common trap and ACK',local_fsm)

                        def local_timers():
                            # Expose the same MAC microsecond clock used by the deadline callback.
                            write(0xf1000000,4);write(0xf1000004,3)
                            expect(read(0xf1000008)==0,'PLL fixture')
                            write(MIX+0x74,0x80000008)
                            for bit, deadline in ((0x100,0x24b00148),(0x40,0x24b00140),(0x80,0x24b00144)):
                                write(MIX+0x8c,0)
                                write(MIX+0x88,0x1c0)
                                now=read(0x24b00120)
                                write(deadline,(now+20)&0xffffffff)
                                wait_us(30)
                                expect(read(MIX+0x84)==0,'disabled timer compare fired')
                                write(MIX+0x8c,bit) # expired latched compare activates once
                                wait_us()
                                expect(read(MIX+0x84)==bit and read(MIX+0x6c)==8,'past deadline/GEN3 missing')
                                write(MIX+0x70,8)
                                expect(read(MIX+0x6c)==8,'GEN ACK incorrectly cleared level timer source')
                                write(MIX+0x88,bit)
                                wait_us()
                                expect(read(MIX+0x84)==0 and read(MIX+0x6c)==0,'one-shot timer autoreloaded')
                                # Rearm while old pending exists; ACK must leave the new schedule alive.
                                expect(read(0x24b00120) < 0x7fffffff, 'fixture exceeded signed half-range')
                                write(deadline,0xffffffff) # modular delta wraps to a past signed value
                                wait_us()
                                expect(read(MIX+0x84)==bit,'signed wrap-past compare did not fire')
                                write(deadline,(read(0x24b00120)+20)&0xffffffff)
                                write(MIX+0x88,bit)
                                wait_us(8)
                                expect(read(MIX+0x84)==0,'future deadline fired early')
                                wait_us(20)
                                expect(read(MIX+0x84)==bit,'ACK canceled newly rearmed compare')
                                write(MIX+0x88,bit)
                                write(deadline,(read(0x24b00120)+20)&0xffffffff)
                                write(MIX+0x8c,0)
                                wait_us(30)
                                expect(read(MIX+0x84)==0 and read(MIX+0x6c)==0,'disable did not cancel schedule')
                                write(MIX+0x8c,bit)
                                wait_us()
                                expect(read(MIX+0x84)==bit,'reenable did not reuse valid past deadline')
                            write(MIX+0x8c,0);write(MIX+0x88,0x1c0)
                            for offset in (0x80,0x84,0x88,0x8c,0x94):
                                expect(read(0x24a00000+offset)==0,'empty DMA completed LLI count')
                                badwrite(0x24a00000+offset,1)
                            badwrite(MIX+0x8c,1)
                            badwrite(MIX+0x88,1)
                            for address in (0x24b00038,0x24b00140,0x24b00144,0x24b00148,MIX+0x6c,MIX+0x84):
                                reject(f'm{address:x},2')
                            # A pending future compare is canceled by MACHW reset, not just its ACK.
                            write(MIX+0x8c,0x100)
                            write(0x24b00148,(read(0x24b00120)+20)&0xffffffff)
                            write(MIX+0x50,1)
                            wait_us(30)
                            expect(read(MIX+0x84)==0 and read(MIX+0x8c)==0 and read(MIX+0x6c)==0,'soft reset did not cancel timer/FSM state')
                        check('native kernel/watchdog/auxiliary one-shot deadlines, enable/cancel/rearm, signed past wrap and level GEN3',local_timers)

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
