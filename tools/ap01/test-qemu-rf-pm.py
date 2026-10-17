#!/usr/bin/env python3
"""Native synthetic TX/RX PM transaction tests, never original-ROM boot proof."""
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
BASE, MIX = 0x62fc4000, 0x20001000


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


def delay():
    # 8192 guest instructions = 8.192us under icount shift0; no host sleeps.
    return li(6, 4096) + [0xfff30313, 0xfe031ee3]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('qemu', 'output'):
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    qemu, output = args.qemu.resolve(), args.output.resolve()
    if output.exists() or not output.parent.is_dir() or not Path('/tmp').is_dir():
        parser.error('requires Linux /tmp and new output in an existing directory')
    token = 'ap01-rf-pm-' + uuid.uuid4().hex
    image = output.parent / (token + '.bin')
    report = dict(synthetic_fixture=True, original_rom_accessed=False,
                  full_rom_boot_verified=False, analog_rf_verified=False,
                  measurement_law='scalar signed25=(112+candidate-256-2*(trim-11)^2)*count; count1024/4096; explicit synthetic quadratic shape, all21 channels',
                  rx_law='I=sign10(low)*512,Q=(unsigned11(high)-1024)*512; disabled lane0',
                  completion_virtual_ns=2000, checks=[],
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
                    machine += ',rf-calibration-mock=on'
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

                        def write(addr, value, size=4):
                            expect(rsp.request(f'M{addr:x},{size:x}:' + value.to_bytes(size, 'little').hex()) == 'OK', 'write rejected')

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

                        def clock():
                            write(0xf1000000, 4); write(0xf1000004, 3)
                            expect(read(0xf1000008) == 0, '40MHz clock profile')

                        if not enabled:
                            clock()
                            reject('m20001620,4')
                            report['checks'].append(dict(name='default-off does not expose PM measurements', passed=True))
                            continue

                        reject('m20001620,4')
                        clock()
                        def setup(code=256,mode=0x500000,channel=0x14555555,count=1024,trim=11):
                            write(MIX+0x070,trim)
                            write(MIX+0x014,mode)
                            write(MIX+0xc4,channel)
                            write(MIX+0x214,code)
                            write(MIX+0x218,code)
                            write(MIX+0x61c,count<<16)

                        def cancel():
                            write(MIX+0x618,read(MIX+0x618)&~0x20000000)

                        def sample(code,trim=11,count=1024):
                            setup(code,trim=trim,count=count)
                            # Original TXCAL waits10us before PM trigger.
                            words=delay()+delay()+li(5,MIX)+li(7,0x20000000)
                            words += [sw(7,0x618),lw(10,0x618)]
                            words += delay()+[lw(11,0x618),lw(12,0x620)]
                            regs=run(words)
                            expect(not regs[10]&0x10000000,'immediate status cannot be done')
                            expect(regs[11]&0x10000000,'finite delayed completion')
                            raw=regs[12]&0x1ffffff
                            signed=raw-(1<<25) if raw&(1<<24) else raw
                            expect(signed==(112+code-256-2*(trim-11)**2)*count,'signed25 candidate/trim/count law')
                            cancel()
                            return signed

                        def varying():
                            report['candidate_results']=[[code,sample(code)] for code in (0,255,256,257,2047)]
                        check('native pretrigger delay, busy/done, signed25 and candidate endpoints',varying)

                        def scalar4096():
                            report['scalar4096_results']=[[code,trim,sample(code,trim,count=4096)]
                                                          for code,trim in ((0,0),(0,11),(256,11),(2047,0),(2047,11),(2047,15))]
                        check('scalar count4096 scales signed25 safely at domain boundaries',scalar4096)


                        def trim_scan():
                            values=[sample(256,trim=t) for t in range(16)]
                            expect(max(range(16),key=values.__getitem__)==11,'quadratic peak at11')
                            expect(values[10]==values[12] and values[10]<values[11], 'neighbor second peaks')
                            expect(all(a<b for a,b in zip(values[:11],values[1:12])), 'left gradient')
                            expect(all(a>b for a,b in zip(values[11:],values[12:])), 'right gradient')
                            report['trim_scan_results']=values
                        check('trim independently changes synthetic quadratic shape with peak11',trim_scan)


                        def snapshot():
                            setup(256);write(MIX+0x618,0x20000000)
                            write(MIX+0x214,257);write(MIX+0x218,257)
                            write(MIX+0x61c,512<<16)
                            write(MIX+0x070,3)
                            write(MIX+0x618,0x20000000)
                            expect(not read(MIX+0x618)&0x10000000,'held trigger does not finish early')
                            reject('m20001620,4')
                            run(delay())
                            expect(read(MIX+0x620)==112*1024,'input snapshots survive later changes')
                            badwrite(MIX+0x620,0)
                            expect(read(MIX+0x620)==112*1024,'RO result unchanged')
                            write(MIX+0x618,read(MIX+0x618))
                            expect(read(MIX+0x620)==112*1024,'held high does not retrigger')
                            cancel();reject('m20001620,4')
                        check('snapshot inputs, held trigger, unpublished result and RO rejection',snapshot)

                        def invalid():
                            for mode,count,mismatch in ((0x400000,1024,False),(0x600000,1024,False),
                                                        (0x500000,0,False),(0x500000,1023,False),
                                                        (0x500000,1024,True)):
                                setup(256,mode=mode,count=count)
                                if mismatch:write(MIX+0x218,257)
                                before=read(MIX+0x618)
                                badwrite(MIX+0x618,before|0x20000000)
                                expect(read(MIX+0x618)==before,'rejected trigger atomic')
                                run(delay());expect(not read(MIX+0x618)&0x10000000,'no false completion')
                            setup(channel=0)
                            badwrite(MIX+0x618,0x20000000)
                            expect(not read(MIX+0x618)&0x30000000,'unknown channel cannot trigger')
                            setup()
                            write(MIX+0x618,0x10000000)
                            expect(not read(MIX+0x618)&0x30000000,'RO done write cannot forge completion')
                            reject('m20001620,4')
                            badwrite(MIX+0x618,0x20,1)
                        check('unknown modes, count, unequal candidates and widths reject atomically',invalid)

                        def cancel_reset():
                            setup();write(MIX+0x618,0x20000000)
                            cancel();run(delay())
                            expect(not read(MIX+0x618)&0x10000000,'cancel prevents completion')
                            reject('m20001620,4')
                            setup();write(MIX+0x618,0x20000000)
                            qmp.command('system_reset');clock();run(delay())
                            expect(not read(MIX+0x618)&0x30000000,'warm reset cancels trigger and done')
                            reject('m20001620,4')
                            expect(sample(256)==112*1024,'fresh transaction after reset')
                        check('cancel and warm reset discard pending transaction',cancel_reset)

                        def rx_word(i,q,enable_i=True,enable_q=True):
                            return (i&0x3ff)|(0x400 if enable_i else 0)|((q&0x7ff)<<12)|(0x800000 if enable_q else 0)

                        def rx_setup(word,mode=0x1500000,count=20480):
                            setup(21,mode=mode,count=count)
                            # TX candidate fields are unrelated to RX PM.
                            write(MIX+0x218,22)
                            write(MIX+0x614,word)

                        def signed25(raw):
                            raw &= 0x1ffffff
                            return raw-(1<<25) if raw&(1<<24) else raw

                        def rx_sample(i,q,enable_i=True,enable_q=True):
                            rx_setup(rx_word(i,q,enable_i,enable_q))
                            words=li(5,MIX)+li(7,0x601f0000)+[sw(7,0x618),lw(10,0x618)]
                            words+=delay()+[lw(11,0x618),lw(12,0x620),lw(13,0x624)]
                            regs=run(words)
                            expect(not regs[10]&0x10000000 and regs[11]&0x10000000,'RX finite completion')
                            values=(signed25(regs[12]),signed25(regs[13]))
                            expect(values==((i*512 if enable_i else 0),((q-1024)*512 if enable_q else 0)), 'independent RX lanes')
                            cancel()
                            return [v//512 for v in values]

                        def rx_boundaries():
                            report['rx_i_results']=[[i,rx_sample(i,1024)] for i in (-512,-1,0,1,511)]
                            report['rx_q_results']=[[q,rx_sample(0,q)] for q in (0,1023,1024,1025,2047)]
                            expect(rx_sample(-512,0,False,True)==[0,-1024],'I disable only')
                            expect(rx_sample(511,2047,True,False)==[511,0],'Q disable only')
                            expect(rx_sample(511,2047,False,False)==[0,0],'both lanes disabled')
                            metrics=[sum(v*v for v in rx_sample(i,1024)) for i in (-2,-1,0,1,2)]
                            expect(metrics==[4,1,0,1,4],'I quadratic minimum0')
                            metrics=[sum(v*v for v in rx_sample(0,q)) for q in (1022,1023,1024,1025,1026)]
                            expect(metrics==[4,1,0,1,4],'Q quadratic minimum1024')
                        check('RX signed10 and unsigned11 endpoints, enables and independent quadratic minima',rx_boundaries)

                        def rx_snapshot():
                            rx_setup(rx_word(-7,1031));write(MIX+0x618,0x601f0000)
                            reject('m20001620,4');reject('m20001624,4')
                            write(MIX+0x614,rx_word(9,1000))
                            write(MIX+0x014,0x500000);write(MIX+0x61c,1024<<16)
                            write(MIX+0x618,0x601f0000)
                            run(delay())
                            expect(signed25(read(MIX+0x620))==-7*512 and signed25(read(MIX+0x624))==7*512,'RX group and inputs latched')
                            badwrite(MIX+0x620,0);badwrite(MIX+0x624,0)
                            cancel();reject('m20001620,4');reject('m20001624,4')
                            sample(256);reject('m20001624,4')
                        check('RX inputs and group snapshot, held trigger and TX/Q isolation',rx_snapshot)

                        def rx_invalid():
                            for mode,count,control in ((0x1500000,20480,0x6010f000),
                                                       (0x1500000,1024,0x601f0000),
                                                       (0x2500000,20480,0x601f0000),
                                                       (0x3500000,20480,0x601f0000),
                                                       (0x1500000,20480,0x201f0000),
                                                       (0x1500000,20480,0x600f0000),
                                                       (0x1500000,20480,0x601e0000),
                                                       (0x500000,1024,0x601f0000)):
                                rx_setup(rx_word(1,1025),mode=mode,count=count)
                                # Equal TX fields ensure the last rejection is request isolation.
                                write(MIX+0x218,21)
                                before=read(MIX+0x618)
                                badwrite(MIX+0x618,control)
                                expect(read(MIX+0x618)==before,'invalid RX/TX profile is atomic')
                                run(delay());reject('m20001620,4');reject('m20001624,4')
                        check('RX count/mode/control gates and TX request isolation reject atomically',rx_invalid)

                        def rx_cancel_reset():
                            rx_setup(rx_word(-1,1023));write(MIX+0x618,0x601f0000)
                            cancel();run(delay());reject('m20001620,4');reject('m20001624,4')
                            rx_setup(rx_word(-1,1023));write(MIX+0x618,0x601f0000)
                            qmp.command('system_reset');clock();run(delay())
                            reject('m20001620,4');reject('m20001624,4')
                            expect(rx_sample(0,1024)==[0,0],'new RX transaction after reset')
                        check('RX cancellation and warm reset clear both pending outputs',rx_cancel_reset)

                        def iq_setup(gain,phase,i,q,enable_i=True,enable_q=True):
                            setup(21,count=4096)
                            write(MIX+0x218,22)
                            write(MIX+0x074,((gain&63)<<24)|((phase&63)<<16))
                            write(MIX+0x600,rx_word(i,q,enable_i,enable_q))

                        def iq_sample(gain,phase,i,q,selector=0x9200,enable_i=True,enable_q=True):
                            iq_setup(gain,phase,i,q,enable_i,enable_q)
                            control=0x20100000|selector
                            words=li(5,MIX)+li(7,control)+[sw(7,0x618),lw(10,0x618)]
                            words+=delay()+[lw(11,0x618),lw(12,0x620),lw(13,0x624)]
                            regs=run(words)
                            expect(not regs[10]&0x10000000 and regs[11]&0x10000000,'TXIQ finite completion')
                            values=[signed25(regs[12])//512,signed25(regs[13])//512]
                            expect(values==[gain+(i if enable_i else 0),phase+(q-1024 if enable_q else 0)],'TXIQ independent residual inputs')
                            cancel()
                            return values

                        def iq_inputs():
                            report['tx_iq_results']=[iq_sample(*args,selector=selector)
                                                    for selector in (0x9200,0x12400)
                                                    for args in ((-32,31,-512,2047),(31,-32,511,0),
                                                                 (0,0,0,1024),(-3,5,3,1019))]
                            expect(iq_sample(-3,5,3,1019)==[0,0],'independent correction reaches zero metric')
                            expect(iq_sample(2,-2,511,2047,enable_i=False)==[2,1021],'TXIQ I disable')
                            expect(iq_sample(2,-2,511,2047,enable_q=False)==[513,-2],'TXIQ Q disable')
                        check('TXIQ selector9200/12400, signed6/10 and unsigned11 boundaries and zero metric',iq_inputs)

                        def iq_snapshot():
                            iq_setup(-3,5,2,1021);write(MIX+0x618,0x20109200)
                            reject('m20001620,4');reject('m20001624,4')
                            write(MIX+0x074,0);write(MIX+0x600,rx_word(0,1024))
                            write(MIX+0x614,rx_word(7,1031));write(MIX+0x014,0x1500000)
                            write(MIX+0x61c,20480<<16);write(MIX+0x618,0x20109200)
                            run(delay())
                            expect(signed25(read(MIX+0x620))==-512 and signed25(read(MIX+0x624))==1024,'TXIQ snapshot independent of later fields/group')
                            cancel()
                        check('TXIQ coefficient074/600 and group snapshot with held trigger',iq_snapshot)

                        def iq_invalid():
                            for count,selector in ((1024,0x9200),(4096,0x9201),(4096,0x12401),(4096,0xf000)):
                                iq_setup(0,0,0,1024);write(MIX+0x61c,count<<16)
                                before=read(MIX+0x618)
                                badwrite(MIX+0x618,0x20100000|selector)
                                expect(read(MIX+0x618)==before,'invalid TXIQ request atomic')
                                run(delay());reject('m20001620,4');reject('m20001624,4')
                            iq_setup(0,0,0,1024);write(MIX+0x618,0x20109200)
                            cancel();run(delay());reject('m20001620,4');reject('m20001624,4')
                            iq_setup(0,0,0,1024);write(MIX+0x618,0x20109200)
                            qmp.command('system_reset');clock();run(delay())
                            reject('m20001620,4');reject('m20001624,4')
                        check('TXIQ count/selector rejection, cancellation and reset',iq_invalid)


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
