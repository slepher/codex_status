#!/usr/bin/env python3
"""Native synthetic ACAL comparator tests, never original-ROM boot proof."""
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
    token = 'ap01-rf-acal-' + uuid.uuid4().hex
    image = output.parent / (token + '.bin')
    report = dict(synthetic_fixture=True, original_rom_accessed=False,
                  full_rom_boot_verified=False, analog_rf_verified=False,
                  comparator_law='bit12=(amplitude_code>=24); channel-independent synthetic profile',
                  settle_contract='next MAC microsecond tick; same reset epoch as MACtime', checks=[],
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
                            expect(not read(MIX+0xa4)&0x1000, 'default-off exposes no comparator')
                            report['checks'].append(dict(name='default-off profile does not fabricate amplitude feedback', passed=True))
                            continue

                        expect(not read(MIX+0xa4)&0x1000, 'unconfigured comparator absent')
                        clock()
                        write(MIX+0x014, 0x40)

                        def setup_rmw():
                            # Original save-state a00163ea reads +a4 after mode
                            # entry, while c4=0 and no qualified sample exists.
                            regs=run(li(5,MIX)+[lw(10,0xa4),0x30556513,
                                               sw(10,0xa4),lw(11,0xa4)])
                            expect(regs[10]==0x305 and regs[11]==0x305,
                                   'setup preserves config with unqualified output low')
                            write(MIX+0xa4,regs[11]|0x1000)
                            expect(read(MIX+0xa4)==0x305,'unqualified RO output cannot be forged')
                            run(delay())
                            expect(read(MIX+0xa4)==0x305,'time alone cannot qualify a sample')
                        check('native mode-entry configuration snapshot and RMW before stimulus',setup_rmw)
                        write(MIX+0x0c4, 0x14088889)

                        def sample(code):
                            write(MIX+0xa0, (code << 16) | 103)
                            reject('m200010a4,4')
                            regs=run(delay()+li(5,MIX)+[lw(10,0xa4)])
                            return bool(regs[10]&0x1000)

                        def boundaries():
                            expect([sample(c) for c in (0,23,24,25,63)] ==
                                   [False,False,True,True,True], 'comparator threshold')
                        check('native candidate stores, unsettled rejection and settled boundary', boundaries)

                        def original_udelay():
                            # Original a00192c0..92ee waits until the integer MAC
                            # microsecond counter reaches start+1. Preserve that
                            # quantized contract, including its short inner spin.
                            for phase in (0,17,113,499,877):
                                report['current_mac_phase_instructions']=phase
                                words=[0x13]*phase+[0x00000513]+li(5,MIX)+li(7,(25<<16)|103)+[sw(7,0xa0)]
                                words+=li(28,0x24b00000)+[lw(29,0x120,28),0x001e8e93]
                                # Three inner spins match original counter2 including zero test.
                                # Then poll MAC and repeat inner spins if before target.
                                words += [0x00300f93,0xffff8f93,0xfe0f9ee3,
                                          lw(30,0x120,28),0x41df0f33,0xfe0f46e3]
                                words += [lw(10,0xa4)]
                                regs=run(words)
                                expect(regs[10]&0x1000, 'original quantized udelay sample')
                                # Force a fresh changed stimulus on next phase.
                                write(MIX+0xa0,(23<<16)|103)
                        check('original MAC-microsecond udelay1 across clock phases', original_udelay)

                        def readonly():
                            sample(24)
                            before=read(MIX+0xa4)
                            write(MIX+0xa4,before^0x1000)
                            expect(read(MIX+0xa4)==before, 'RO write must not forge comparator')
                            badwrite(MIX+0xa4,before&0xff,1)
                        check('RO comparator ignores writes and rejects wrong width', readonly)

                        def mode_cancel():
                            write(MIX+0xa0,25<<16)
                            write(MIX+0x014,0xc0)
                            run(delay())
                            expect(not read(MIX+0xa4)&0x1000, 'outside ACAL is base config only')
                            write(MIX+0x014,0x40)
                            reject('m200010a4,4')
                            run(delay())
                            expect(read(MIX+0xa4)&0x1000, 'mode reentry requires new settling')
                        check('mode exit cancels feedback and reentry settles afresh', mode_cancel)

                        def reset():
                            write(MIX+0xa0,26<<16)
                            report['mac_tick_before_warm_reset']=read(0x24b00120)
                            expect(report['mac_tick_before_warm_reset']>0,'nonzero virtual epoch before reset')
                            qmp.command('system_reset')
                            clock()
                            expect(read(0x24b00120)==0,'MAC epoch restarts at warm reset')
                            run(delay())
                            expect(not read(MIX+0xa4)&0x1000, 'reset cancels comparator')
                            write(MIX+0x014,0x40)
                            write(MIX+0x0c4,0x14088889)
                            expect(not sample(23), 'new post-reset sample')
                            original_udelay()
                        check('warm reset cancels pending feedback', reset)

                        def search():
                            results=[]
                            for channel_word in (0x14088889,0x14111111,0x1419999a,0x14222222,0x142aaaab,
                                                 0x14333333,0x143bbbbc,0x14444444,0x144ccccd,0x14555555,
                                                 0x145dddde,0x14666666,0x146eeeef,0x14777777,0x14800000,
                                                 0x14888889,0x14911111,0x1499999a,0x14a22222,0x14aaaaab,0x14b33333):
                                write(MIX+0xc4,channel_word)
                                code=32;history=[]
                                for step in (16,8,4,2,1):
                                    high=sample(code);history.append([code,high])
                                    code += -step if high else step
                                if not sample(code):code+=1
                                expect(code==24,'original search result')
                                expect(history==[[32,True],[16,False],[24,True],
                                                 [20,False],[22,False]],'gradient')
                                results.append(code)
                            report['algorithm_results']=results
                        check('21 original five-step searches converge to24', search)
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
