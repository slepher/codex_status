#!/usr/bin/env python3
"""Native synthetic LOFCAL transaction tests, never original-ROM boot proof."""
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
    token = 'ap01-rf-cal-' + uuid.uuid4().hex
    image = output.parent / (token + '.bin')
    report = dict(synthetic_fixture=True, original_rom_accessed=False,
                  full_rom_boot_verified=False, analog_rf_verified=False,
                  counter_law='42688+(128-candidate)*64; XTAL40MHz/div0x855 only',
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
                            data = b''.join(struct.pack('<I', w) for w in words + [0x13])
                            expect(rsp.request(f'M{BASE:x},{len(data):x}:' + data.hex()) == 'OK', 'program write')
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
                            reject('m200010ac,4')
                            report['checks'].append(dict(name='default-off profile does not fabricate calibration status', passed=True))
                            continue

                        reject('m200010ac,4')
                        clock()
                        write(MIX + 0xa8, 0x855)

                        def setup_restore():
                            # SDK LOFCAL setup writes and save/restore aperture;
                            # native stores/loads exercise main's real MMIO path.
                            config = {0x014: 0xf0, 0x004: 0x0e, 0x02c: 0x40,
                                      0x030: 0x0cf00002, 0x0b8: 0x13010,
                                      0x0c0: 0x11000, 0x0c4: 0x14b33333,
                                      0x01c: 0x98}
                            for off, value in config.items():
                                regs = run(li(5, MIX) + li(7, value) + [sw(7, off), lw(10, off)])
                                expect(regs[10] == value, 'LOFCAL configuration roundtrip')
                                badwrite(MIX + off, value ^ (4 if off == 0x030 else 0x80000000))
                                expect(read(MIX + off) == value, 'unknown config bit rejection atomic')
                                write(MIX + off, 0)
                            for value in (0x14088889, 0x14b33333, 0x01000000):
                                before = read(MIX + 0xc4)
                                regs = run(li(5, MIX) + li(7, value) + [sw(7, 0xc4), lw(10, 0xc4)])
                                expect(regs[10] == value, 'real first/last channel or LOFCAL literal')
                                write(MIX + 0xc4, before)
                                expect(read(MIX + 0xc4) == before, 'saved c4 cleanup restored')
                            snapshots = (0x600, 0x60c, 0x61c, 0x048, 0x20c,
                                         0x214, 0x218, 0x23c, 0x240, 0x244,
                                         0x0f0, 0x064, 0x058, 0x074)
                            for off in snapshots:
                                before = read(MIX + off)
                                write(MIX + off, before)
                                badwrite(MIX + off, before ^ 1)
                                expect(read(MIX + off) == before, 'snapshot-only register remains unchanged')
                        check('native LOFCAL setup fields and unchanged save/restore registers', setup_restore)

                        def sample(code):
                            words = li(5, MIX) + li(7, code) + [sw(7, 0xa0)]
                            words += li(7, 16) + [sw(7, 0xac), lw(10, 0xac)]
                            words += delay() + [lw(11, 0xac), lw(12, 0xa8)]
                            regs = run(words)
                            expect(regs[10] == 16, 'native immediate poll must be busy/not done')
                            expect(regs[11] == 0x100010, 'native delayed poll done')
                            write(MIX + 0xac, 0x100000)  # RMW done bit is read-only.
                            expect(read(MIX + 0xac) == 0, 'clear trigger drops done')
                            return regs[12] >> 16

                        def varying():
                            counts = [sample(code) for code in (127, 128, 129)]
                            expect(counts == [42752, 42688, 42624], 'input-dependent counter law')
                            report['candidate_counts'] = counts
                        check('native trigger, busy, virtual completion and distinct candidate feedback', varying)

                        def latched():
                            write(MIX + 0xa0, 127); write(MIX + 0xac, 16)
                            write(MIX + 0xa0, 129); write(MIX + 0xa8, 0x1234)
                            expect(read(MIX + 0xac) == 16, 'host debugger does not advance virtual time')
                            run(delay())
                            expect(read(MIX + 0xa8) == (42752 << 16) | 0x1234, 'transaction snapshots input')
                            expect(read(MIX + 0xac) == 0x100010, 'completed')
                            write(MIX + 0xa8, 0xffff0855)
                            expect(read(MIX + 0xa8) == (42752 << 16) | 0x855, 'result cannot be forged')
                            write(MIX + 0xac, 16)
                            expect(read(MIX + 0xac) == 0x100010, 'held trigger does not start a second transaction')
                            write(MIX + 0xac, 0)
                            write(MIX + 0xac, 0x100000)
                            expect(read(MIX + 0xac) == 0, 'done cannot be forged')
                        check('latched candidate/divisor and read-only result/done', latched)

                        def shared_amplitude():
                            write(MIX + 0xa0, 128 | (63 << 16))
                            expect(read(MIX + 0xa0) == 0x3f0080, 'amplitude and candidate coexist')
                            # Native candidate RMW keeps the independent six-bit amplitude.
                            regs = run(li(5, MIX) + li(7, 0x3f007f) + [sw(7, 0xa0), lw(10, 0xa0)])
                            expect(regs[10] == 0x3f007f, 'candidate write preserves amplitude')
                            write(MIX + 0xac, 16)
                            write(MIX + 0xa0, 0x0000007f)
                            run(delay())
                            expect(read(MIX + 0xa8) >> 16 == 42752, 'amplitude has no effect on LOFCAL count')
                            expect(read(MIX + 0xa0) == 127, 'amplitude clear preserves candidate')
                            write(MIX + 0xac, 0)
                            for value in (0x0040007f, 0x0000017f):
                                badwrite(MIX + 0xa0, value)
                                expect(read(MIX + 0xa0) == 127, 'unsupported shared fields atomic')
                        check('shared amplitude boundaries and RMW preserve independent LOFCAL state', shared_amplitude)

                        def invalid():
                            before = [read(MIX + off) for off in (0xa0, 0xa8, 0xac)]
                            for addr, val, width in ((MIX+0xa0,256,4),(MIX+0xac,17,4),
                                                     (MIX+0xac,16,1),(MIX+0xa8,0x855,2)):
                                badwrite(addr,val,width)
                                expect([read(MIX+off) for off in (0xa0,0xa8,0xac)] == before, 'invalid access atomicity')
                            for div in (0,0x854,0x856,0xffff):
                                write(MIX+0xa8,div)
                                badwrite(MIX+0xac,16)
                                expect(read(MIX+0xac)==0 and read(MIX+0xa8)&0xffff==div, 'unsupported divisor cannot start')
                            write(MIX+0xa8,0x855)
                        check('unknown fields, widths and reference profiles reject atomically', invalid)

                        def cancel_reset():
                            write(MIX+0xa0,127); write(MIX+0xac,16)
                            previous=read(MIX+0xa8)
                            write(MIX+0xac,0); run(delay())
                            expect(read(MIX+0xac)==0 and read(MIX+0xa8)==previous, 'cancel does not publish pending result')
                            write(MIX+0xac,16)
                            qmp.command('system_reset')
                            clock(); run(delay())
                            expect(all(read(MIX+off)==0 for off in (0xa0,0xa8,0xac)), 'warm reset cancels timer and state')
                            write(MIX+0xa8,0x855)
                        check('cancel and warm reset prevent deferred completion', cancel_reset)

                        def search():
                            code=128
                            history=[]
                            for step in range(6,-1,-1):
                                count=sample(code); history.append([code,count])
                                if count<42656: code=(code-(1<<step))&255
                                elif count>42720: code=(code+(1<<step))&255
                                else: break
                            expect(42656<=sample(code)<=42720 and 15<=code<=240, 'original threshold search converges')
                            scan=[sample(code+1)]
                            candidate=code+1
                            for _ in range(39):
                                candidate=(candidate-1)&255; scan.append(sample(candidate))
                                if scan[-1]>44256: break
                            expect(scan[-1]>44256 and len(scan)<=40, 'original bounded neighbor scan terminates')
                            expect(all(a<b for a,b in zip(scan,scan[1:])), 'counter responds monotonically to descending codes')
                            report['search']=dict(history=history,code=code,scan_samples=len(scan),final_count=scan[-1])
                        check('original threshold binary search and at-most-39 decrement scan', search)
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
