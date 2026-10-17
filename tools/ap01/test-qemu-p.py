#!/usr/bin/env python3
"""Native TCG tests for observed historical P instructions; no original ROM access."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import subprocess
import sys
import time
import uuid

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location('probe', Path(__file__).with_name('run-qemu-ap01.py'))
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)
BASE = 0x20fc1000


def expect(ok, text):
    if not ok:
        raise AssertionError(text)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--qemu', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    qemu, output = args.qemu.resolve(), args.output.resolve()
    if output.exists() or not output.parent.is_dir() or not Path('/tmp').is_dir():
        parser.error('requires Linux /tmp and new output in existing directory')
    token = 'ap01-p-' + uuid.uuid4().hex
    image = output.parent / (token + '.bin')
    report = dict(synthetic_fixture=True, original_rom_accessed=False,
                  full_rom_boot_verified=False,
                  qemu_sha256=hashlib.sha256(qemu.read_bytes()).hexdigest(), checks=[])
    with output.open('x', encoding='utf-8') as result:
        try:
            with image.open('xb') as file:
                file.write(bytes(4096) + b'\x13\x00\x00\x00')
            for enabled in (False, True):
                process = gs = qs = qmp = None
                suffix = 'on' if enabled else 'off'
                gdb, qmp_path = (Path('/tmp') / (token + suffix + ext) for ext in ('.gdb', '.qmp'))
                deadline = time.monotonic() + 30
                try:
                    with (output.parent / (token + suffix + '.log')).open('x') as log:
                        process = subprocess.Popen(
                            [str(qemu), '-M', 'ap01', '-cpu', 'rv32,x-ap01-p=' + suffix,
                             '-kernel', str(image), '-S', '-display', 'none', '-serial', 'none',
                             '-monitor', 'none', '-gdb', f'unix:{gdb},server=on,wait=off',
                             '-qmp', f'unix:{qmp_path},server=on,wait=off'],
                            stdin=subprocess.DEVNULL, stdout=log, stderr=log,
                            preexec_fn=probe.child_limits)
                        gs = probe.connect(gdb, process, deadline)
                        qs = probe.connect(qmp_path, process, deadline)
                        rsp, qmp = probe.RSP(gs, deadline), probe.QMP(qs, deadline)
                        rsp.request('?')

                        def setreg(number, value):
                            expect(rsp.request(f'P{number:x}=' + value.to_bytes(4, 'little').hex()) == 'OK', 'setreg')

                        def getreg(number):
                            raw = bytes.fromhex(rsp.request('g'))
                            return int.from_bytes(raw[number * 4:number * 4 + 4], 'little')

                        def step(word):
                            expect(rsp.request(f'M{BASE:x},4:' + word.to_bytes(4, 'little').hex()) == 'OK', 'write instruction')
                            setreg(32, BASE)
                            expect(rsp.request('s').startswith(('T', 'S')), 'step reply')

                        def illegal(word, label):
                            step(word)
                            text = qmp.command('human-monitor-command', {'command-line': 'info registers'})
                            match = re.search(r'\bmcause\s+([0-9a-fA-F]+)', text)
                            expect(match and int(match.group(1), 16) == 2, label + ': expected illegal')
                            report['checks'].append(dict(name=suffix + ': ' + label, passed=True))

                        encoding = lambda rd, rs1, rs2: 0x0e001077 | rd << 7 | rs1 << 15 | rs2 << 20
                        expect(encoding(15, 15, 15) == 0x0ef797f7, 'observed encoding mismatch')
                        if not enabled:
                            illegal(encoding(15, 15, 15), 'disabled observed instruction')
                            illegal(0xc0e51577, 'disabled observed ADD64')
                            illegal(0xf0a79977, 'disabled observed MULR64')
                            illegal(0xc2c71777, 'disabled observed SUB64')
                            illegal(0xf34a8a77, 'disabled observed MAXW')
                            illegal(0xad890977, 'disabled observed SWAP8')
                            illegal(0xa4d61777, 'disabled observed UMAR64')
                            continue
                        for rd, rs1, rs2, left, right, label in [
                            (5, 6, 7, 0x1234abcd, 0x5678ef01, 'independent operands'),
                            (6, 6, 7, 0x1234abcd, 0x5678ef01, 'rd aliases rs1'),
                            (7, 6, 7, 0x1234abcd, 0x5678ef01, 'rd aliases rs2'),
                            (15, 15, 15, 0x11223344, 0x11223344, 'observed UART all alias'),
                            (5, 6, 6, 0x11223344, 0x11223344, 'sources alias'),
                            (0, 6, 7, 0x1234abcd, 0x5678ef01, 'x0 destination'),
                            (5, 0, 7, 0, 0x5678ef01, 'x0 upper source'),
                            (5, 6, 0, 0x1234abcd, 0, 'x0 lower source')]:
                            if rs1:
                                setreg(rs1, left)
                            if rs2:
                                setreg(rs2, right)
                            step(encoding(rd, rs1, rs2))
                            wanted = (((left & 0xffff) << 16) | (right & 0xffff)) if rd else 0
                            expect(getreg(rd) == wanted and getreg(32) == BASE + 4, label)
                            report['checks'].append(dict(name='on: ' + label, passed=True))
                        illegal(encoding(5, 6, 7) ^ (1 << 25), 'unobserved funct7 rejected')
                        illegal(encoding(5, 6, 7) ^ (1 << 12), 'unobserved funct3 rejected')
                        illegal((encoding(5, 6, 7) & ~0x7f) | 0x3f, 'other draft opcode3f rejected')
                        add64 = lambda rd, rs1, rs2: 0xc0001077 | rd << 7 | rs1 << 15 | rs2 << 20
                        expect(add64(10, 10, 14) == 0xc0e51577, 'observed ADD64 encoding mismatch')
                        for rd, rs1, rs2, left, right, label in [
                            (10, 10, 14, 0, 0x43ad, 'ADD64 exact observed alias'),
                            (4, 6, 8, 0xffffffff, 1, 'ADD64 low carry'),
                            (4, 6, 8, 0xffffffffffffffff, 1, 'ADD64 full overflow wrap'),
                            (6, 6, 8, 0x12345678ffffffff, 0x10, 'ADD64 rd aliases rs1'),
                            (8, 6, 8, 0x10, 0x12345678ffffffff, 'ADD64 rd aliases rs2'),
                            (6, 6, 6, 0x80000000ffffffff, 0x80000000ffffffff, 'ADD64 all pair alias'),
                            (11, 7, 9, 0x12345678ffffffff, 1, 'ADD64 field low bits ignored'),
                            (31, 7, 9, 0x7fffffffffffffff, 1, 'ADD64 highest register pair'),
                            (0, 6, 8, 0xffffffffffffffff, 1, 'ADD64 x0 low discard'),
                            (1, 6, 8, 0x12345678ffffffff, 1, 'ADD64 x0 pair still writes x1'),
                            (4, 0, 8, 0x1234567800000000, 1, 'ADD64 x0 source pair reads x1'),
                            (4, 6, 1, 1, 0xabcdef0100000000, 'ADD64 odd x0 source field')]:
                            for source, value in ((rs1 & ~1, left), (rs2 & ~1, right)):
                                if source:
                                    setreg(source, value & 0xffffffff)
                                setreg(source + 1, value >> 32)
                            step(add64(rd, rs1, rs2))
                            wanted = (left + right) & 0xffffffffffffffff
                            dest = rd & ~1
                            expect(getreg(dest) == (wanted & 0xffffffff if dest else 0) and
                                   getreg(dest + 1) == wanted >> 32 and getreg(32) == BASE + 4,
                                   label)
                            report['checks'].append(dict(name='on: ' + label, passed=True))
                        illegal(add64(4, 6, 8) ^ (1 << 26), 'unimplemented ADD64 funct7 rejected')
                        illegal(add64(4, 6, 8) ^ (1 << 12), 'unimplemented ADD64 adjacent funct3')
                        illegal((add64(4, 6, 8) & ~0x7f) | 0x3f, 'ADD64 other draft opcode3f rejected')
                        mulr64 = lambda rd, rs1, rs2: 0xf0001077 | rd << 7 | rs1 << 15 | rs2 << 20
                        expect(mulr64(18, 15, 10) == 0xf0a79977, 'observed MULR64 encoding mismatch')
                        for rd, rs1, rs2, left, right, label in [
                            (18, 15, 10, 1000, 0x43ad, 'MULR64 exact observed operands'),
                            (4, 6, 8, 0xffffffff, 0xffffffff, 'MULR64 unsigned all ones'),
                            (4, 6, 8, 0x80000000, 2, 'MULR64 high word carry'),
                            (6, 6, 8, 0xffffffff, 3, 'MULR64 low destination aliases rs1'),
                            (6, 7, 8, 0xffffffff, 3, 'MULR64 high destination aliases rs1'),
                            (8, 6, 9, 3, 0xffffffff, 'MULR64 high destination aliases rs2'),
                            (6, 6, 7, 0x12345678, 0xabcdef01, 'MULR64 both destination halves alias sources'),
                            (7, 6, 6, 0x12345678, 0x12345678, 'MULR64 odd destination and sources alias'),
                            (31, 31, 8, 0x80000000, 2, 'MULR64 highest pair and odd source'),
                            (0, 6, 8, 0xffffffff, 2, 'MULR64 x0 low discard x1 high write'),
                            (1, 1, 8, 0xffffffff, 2, 'MULR64 x1 input aliases high destination'),
                            (4, 0, 8, 0, 0xffffffff, 'MULR64 x0 left source'),
                            (4, 6, 0, 0xffffffff, 0, 'MULR64 x0 right source')]:
                            if rs1:
                                setreg(rs1, left)
                            if rs2:
                                setreg(rs2, right)
                            step(mulr64(rd, rs1, rs2))
                            wanted = left * right
                            dest = rd & ~1
                            expect(getreg(dest) == (wanted & 0xffffffff if dest else 0) and
                                   getreg(dest + 1) == wanted >> 32 and getreg(32) == BASE + 4,
                                   label)
                            report['checks'].append(dict(name='on: ' + label, passed=True))
                        illegal(mulr64(4, 6, 8) ^ (1 << 25), 'unimplemented MULR64 adjacent funct7')
                        illegal(mulr64(4, 6, 8) ^ (1 << 12), 'unimplemented MULR64 adjacent funct3')
                        illegal((mulr64(4, 6, 8) & ~0x7f) | 0x3f, 'MULR64 other draft opcode3f rejected')
                        sub64 = lambda rd, rs1, rs2: 0xc2001077 | rd << 7 | rs1 << 15 | rs2 << 20
                        expect(sub64(14, 14, 12) == 0xc2c71777, 'observed SUB64 encoding mismatch')
                        for rd, rs1, rs2, left, right, label in [
                            (14, 14, 12, 0x12345678, 0x4321, 'SUB64 exact observed alias'),
                            (4, 6, 8, 0x100000000, 1, 'SUB64 low borrow'),
                            (4, 6, 8, 0, 1, 'SUB64 full underflow wrap'),
                            (6, 6, 8, 0x1234567800000000, 1, 'SUB64 rd aliases rs1'),
                            (8, 6, 8, 0x1234567800000000, 1, 'SUB64 rd aliases rs2'),
                            (6, 6, 6, 0xffffffffffffffff, 0xffffffffffffffff, 'SUB64 all pairs alias zero'),
                            (11, 7, 9, 0x100000000, 1, 'SUB64 odd fields select even pairs'),
                            (31, 6, 8, 0, 0x8000000000000000, 'SUB64 highest register pair'),
                            (0, 6, 8, 0, 1, 'SUB64 x0 discard x1 high write'),
                            (1, 1, 8, 0x1234567800000000, 1, 'SUB64 x0 source destination alias'),
                            (4, 6, 1, 0, 0x1234567800000000, 'SUB64 odd x0 second source')]:
                            for source, value in ((rs1 & ~1, left), (rs2 & ~1, right)):
                                if source:
                                    setreg(source, value & 0xffffffff)
                                setreg(source + 1, value >> 32)
                            step(sub64(rd, rs1, rs2))
                            wanted = (left - right) & 0xffffffffffffffff
                            dest = rd & ~1
                            expect(getreg(dest) == (wanted & 0xffffffff if dest else 0) and
                                   getreg(dest + 1) == wanted >> 32 and getreg(32) == BASE + 4,
                                   label)
                            report['checks'].append(dict(name='on: ' + label, passed=True))
                        illegal(sub64(4, 6, 8) ^ (1 << 26), 'unimplemented SUB64 funct7 rejected')
                        illegal(sub64(4, 6, 8) ^ (1 << 12), 'unimplemented SUB64 funct3 rejected')
                        illegal((sub64(4, 6, 8) & ~0x7f) | 0x3f, 'SUB64 other draft opcode3f rejected')
                        maxw = lambda rd, rs1, rs2: 0xf2000077 | rd << 7 | rs1 << 15 | rs2 << 20
                        expect(maxw(20, 21, 20) == 0xf34a8a77, 'observed MAXW encoding mismatch')
                        signed = lambda value: value if value < 0x80000000 else value - 0x100000000
                        for rd, rs1, rs2, left, right, label in [
                            (20, 21, 20, 0xfffffffd, 1, 'MAXW exact observed rd rs2 alias'),
                            (4, 6, 8, 0x80000000, 0x7fffffff, 'MAXW signed endpoints'),
                            (4, 6, 8, 0xffffffff, 0x80000000, 'MAXW two negatives'),
                            (4, 6, 8, 0x80000000, 0, 'MAXW negative versus zero'),
                            (4, 6, 8, 0xffffffff, 1, 'MAXW unsigned ordering rejected'),
                            (6, 6, 8, 0x7fffffff, 0x80000000, 'MAXW rd aliases rs1'),
                            (8, 6, 8, 0xfffffffe, 0xffffffff, 'MAXW rd aliases rs2'),
                            (6, 6, 6, 0x80000000, 0x80000000, 'MAXW all registers alias'),
                            (31, 29, 30, 0x12345678, 0x12345678, 'MAXW equal highest destination'),
                            (0, 6, 8, 0x7fffffff, 0xffffffff, 'MAXW x0 destination'),
                            (4, 0, 8, 0, 0xffffffff, 'MAXW x0 left beats negative'),
                            (4, 6, 0, 1, 0, 'MAXW x0 right loses positive'),
                            (4, 0, 0, 0, 0, 'MAXW both sources x0')]:
                            if rs1:
                                setreg(rs1, left)
                            if rs2:
                                setreg(rs2, right)
                            step(maxw(rd, rs1, rs2))
                            wanted = (max(signed(left), signed(right)) & 0xffffffff) if rd else 0
                            expect(getreg(rd) == wanted and getreg(32) == BASE + 4, label)
                            report['checks'].append(dict(name='on: ' + label, passed=True))
                        illegal(maxw(4, 6, 8) ^ (1 << 25), 'unimplemented MAXW adjacent MINW rejected')
                        illegal(maxw(4, 6, 8) ^ (1 << 12), 'unimplemented MAXW adjacent funct3 rejected')
                        illegal((maxw(4, 6, 8) & ~0x7f) | 0x3f, 'MAXW other draft opcode3f rejected')
                        swap8 = lambda rd, rs1: 0xad800077 | rd << 7 | rs1 << 15
                        expect(swap8(18,18)==0xad890977,'observed SWAP8 encoding mismatch')
                        for rd, rs1, source, label in [
                            (18,18,0x12345678,'SWAP8 observed all alias'),
                            (4,6,0x01234567,'SWAP8 independent byte lanes'),
                            (4,6,0x00ff8001,'SWAP8 zero/sign-bit boundaries'),
                            (4,6,0xffffffff,'SWAP8 all ones'),
                            (4,6,0,'SWAP8 all zeros'),
                            (31,30,0x80000001,'SWAP8 highest registers'),
                            (0,6,0x01234567,'SWAP8 x0 destination'),
                            (4,0,0,'SWAP8 x0 source')]:
                            if rs1:
                                setreg(rs1,source)
                            step(swap8(rd,rs1))
                            wanted = (((source & 0x00ff00ff)<<8) | ((source>>8)&0x00ff00ff)) if rd else 0
                            expect(getreg(rd)==wanted and getreg(32)==BASE+4,label)
                            report['checks'].append(dict(name='on: '+label,passed=True))
                        # Unary subf5 is an opcode field, never a source register.
                        illegal(swap8(4,6) ^ (1<<20),'SWAP8 unimplemented adjacent subopcode rejected')
                        illegal(swap8(4,6) ^ (1<<12),'SWAP8 reserved funct3 rejected')
                        illegal((swap8(4,6)&~0x7f)|0x3f,'SWAP8 other draft opcode3f rejected')

                        umar64 = lambda rd, rs1, rs2: 0xa4001077 | rd << 7 | rs1 << 15 | rs2 << 20
                        expect(umar64(14, 12, 13) == 0xa4d61777, 'observed UMAR64 encoding mismatch')
                        for rd, rs1, rs2, acc, left, right, label in [
                            (14, 12, 13, 17, 0x12345678, 0xabcdef01, 'UMAR64 exact observed'),
                            (4, 6, 8, 0xffffffff, 1, 1, 'UMAR64 low carry'),
                            (4, 6, 8, 0xffffffffffffffff, 1, 1, 'UMAR64 modulo64 wrap'),
                            (4, 6, 8, 0xffffffff00000000, 0xffffffff, 0xffffffff, 'UMAR64 full unsigned product and high carry'),
                            (4, 6, 8, 0x123456789abcdef0, 0x80000000, 2, 'UMAR64 unsigned sign boundary'),
                            (5, 6, 8, 0x123456789abcdef0, 0xffffffff, 3, 'UMAR64 odd rd selects same pair'),
                            (6, 6, 8, 0x1122334455667788, 0x80000001, 3, 'UMAR64 rd low aliases rs1'),
                            (6, 8, 7, 0x1122334455667788, 0x12345678, 0xabcdef01, 'UMAR64 rd high aliases rs2'),
                            (7, 6, 7, 0x123456789abcdef0, 0xffffffff, 0x80000000, 'UMAR64 both destination halves alias inputs'),
                            (7, 7, 7, 0x123456789abcdef0, 0xffffffff, 0xffffffff, 'UMAR64 same high input all aliases'),
                            (31, 31, 30, 0x123456789abcdef0, 0xffffffff, 0x80000000, 'UMAR64 highest odd pair'),
                            (0, 6, 8, 0x1234567800000000, 0xffffffff, 0xffffffff, 'UMAR64 x0 pair keeps high result'),
                            (1, 1, 8, 0x1234567800000000, 0x12345678, 0xffffffff, 'UMAR64 x0 pair and high input alias'),
                            (4, 0, 8, 0x123456789abcdef0, 0, 0xffffffff, 'UMAR64 x0 multiplicand preserves accumulator'),
                            (4, 6, 0, 0xffffffffffffffff, 0xffffffff, 0, 'UMAR64 x0 second multiplicand')]:
                            pair = rd & ~1
                            if pair:
                                setreg(pair, acc & 0xffffffff)
                            setreg(pair + 1, acc >> 32)
                            if rs1:
                                setreg(rs1, left)
                            if rs2:
                                setreg(rs2, right)
                            # Capture actual pre-state after aliasing initialization.
                            raw = bytes.fromhex(rsp.request('g'))
                            before = [int.from_bytes(raw[i*4:i*4+4], 'little') for i in range(32)]
                            expected = ((before[pair] | before[pair+1] << 32) +
                                        before[rs1] * before[rs2]) & 0xffffffffffffffff
                            step(umar64(rd, rs1, rs2))
                            raw = bytes.fromhex(rsp.request('g'))
                            after = [int.from_bytes(raw[i*4:i*4+4], 'little') for i in range(32)]
                            wanted = before.copy()
                            if pair:
                                wanted[pair] = expected & 0xffffffff
                            wanted[pair+1] = expected >> 32
                            expect(after == wanted and int.from_bytes(raw[128:132], 'little') == BASE + 4, label)
                            report['checks'].append(dict(name=suffix + ': ' + label, passed=True))
                        illegal(umar64(4, 6, 8) ^ (1 << 25), 'UMAR64 unimplemented adjacent funct7 rejected')
                        illegal(umar64(4, 6, 8) ^ (1 << 12), 'UMAR64 reserved funct3 rejected')
                        illegal((umar64(4, 6, 8) & ~0x7f) | 0x3f, 'UMAR64 other draft opcode3f rejected')

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
            json.dump(report, result, indent=2)
            result.write('\n')
    print(json.dumps(report))
    return 0 if report['passed'] else 1


if __name__ == '__main__':
    sys.exit(main())
