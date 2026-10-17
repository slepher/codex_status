#!/usr/bin/env python3
"""Prove a QEMU-only out-of-line bootstrap hook; NEVER an installable image."""
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
spec = importlib.util.spec_from_file_location(
    'ap01_probe', Path(__file__).with_name('run-qemu-ap01.py'))
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)
ENTRY, BASE, STACK = 0xa0000000, 0x9ffff000, 0x62fdbab0
MARKER = 0x41505831


def sha(data):
    return hashlib.sha256(data).hexdigest()


def addi(rd, rs1, imm):
    return ((imm & 4095) << 20) | (rs1 << 15) | (rd << 7) | 0x13


def store(rs2, rs1, imm):
    return (((imm & 4095) >> 5) << 25) | (rs2 << 20) | (rs1 << 15) | \
           (2 << 12) | ((imm & 31) << 7) | 0x23


def jump(pc, target, scratch):
    delta = target - pc
    hi = (delta + 0x800) >> 12
    lo = delta - (hi << 12)
    return [((hi & 0xfffff) << 12) | (scratch << 7) | 0x17,
            ((lo & 4095) << 20) | (scratch << 15) | 0x67]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--qemu', type=Path, required=True)
    parser.add_argument('--image', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    qemu, image, output = (p.resolve() for p in
                           (args.qemu, args.image, args.output))
    if output.exists() or not output.parent.is_dir() or not Path('/tmp').is_dir():
        parser.error('requires a new output JSON, existing parent directory and Linux /tmp')
    source = image.read_bytes()
    if len(source) != 6769704 or sha(source) != probe.SHA256:
        parser.error('requires exact original 0041 image')
    expected = bytes.fromhex('17c1fdc2130101ab73100134')
    if source[0x1000:0x100c] != expected:
        parser.error('unexpected displaced bootstrap instructions')
    candidate_path = output.with_name(output.stem + '.qemu-only.bin')
    if candidate_path.exists():
        parser.error('candidate must be a new file')
    payload_offset = (len(source) + 3) & ~3
    payload_va = BASE + payload_offset
    # Restore original canonical bootstrap SP and save incoming t0 before use.
    payload_words = [0x62fdc137, addi(2, 2, -1360), store(5, 2, -8),
                     0x34011073, addi(2, 2, -16), 0x415062b7,
                     addi(5, 5, 0x831 - 0x1000), store(5, 2, 0),
                     addi(2, 2, 16)]
    # Marker LUI is rounded up because its low twelve bits are signed.
    payload_words += jump(payload_va + len(payload_words) * 4, ENTRY + 8, 5)
    payload = b''.join(word.to_bytes(4, 'little') for word in payload_words)
    entry_patch = b''.join(word.to_bytes(4, 'little')
                           for word in jump(ENTRY, payload_va, 2))
    # lw t0,-8(sp), executed only after the payload returns with canonical SP.
    landing = (((-8 & 4095) << 20) | (2 << 15) | (2 << 12) | (5 << 7) | 3)
    candidate = bytearray(source)
    candidate[0x1000:0x100c] = entry_patch + landing.to_bytes(4, 'little')
    candidate.extend(b'\0' * (payload_offset - len(candidate)))
    candidate.extend(payload)
    report = dict(installable=False, flashable=False, full_rom_boot_verified=False,
                  original_sha256=sha(source), candidate_sha256=sha(candidate),
                  candidate_path=str(candidate_path), candidate_size=len(candidate),
                  payload_sha256=sha(payload), payload_va=hex(payload_va),
                  payload_file_offset=hex(payload_offset), payload_size=len(payload),
                  entry=hex(ENTRY), return_landing=hex(ENTRY + 8),
                  comparison_pc=hex(ENTRY + 12), canonical_sp=hex(STACK),
                  stack_scratch={'lower': hex(STACK - 16), 'upper_exclusive': hex(STACK),
                                 'marker': hex(MARKER), 'saved_t0': hex(STACK - 8)},
                  changed_bytes=[{'offset': hex(index), 'before': old, 'after': new}
                                 for index, (old, new) in enumerate(zip(source, candidate))
                                 if old != new],
                  appended_range={'offset': hex(len(source)),
                                  'length': len(candidate) - len(source)},
                  entry_patch_hex=bytes(candidate[0x1000:0x100c]).hex(),
                  payload_hex=payload.hex(),
                  limitations=['QEMU-only raw candidate; no OTA header, CRC or length repair',
                               'pre-task bootstrap stack scratch only; no permanent RAM allocation',
                               'native hook/return integer-state proof; no application feature claim'])

    def run(path, label):
        token = 'ap01-hook-' + uuid.uuid4().hex
        gdb_path, qmp_path = (Path('/tmp') / (token + ext) for ext in ('.gdb', '.qmp'))
        log_path = output.with_name(output.stem + '.' + label + '.qemu.log')
        command = [str(qemu), '-M', 'ap01', '-cpu', 'rv32', '-kernel', str(path),
                   '-S', '-display', 'none', '-serial', 'none', '-monitor', 'none',
                   '-gdb', f'unix:{gdb_path},server=on,wait=off',
                   '-qmp', f'unix:{qmp_path},server=on,wait=off']
        process = gs = qs = qmp = None
        deadline = time.monotonic() + 30
        try:
            with log_path.open('x') as log:
                process = subprocess.Popen(command, stdin=subprocess.DEVNULL,
                                           stdout=log, stderr=log, preexec_fn=probe.child_limits)
                gs = probe.connect(gdb_path, process, deadline)
                qs = probe.connect(qmp_path, process, deadline)
                rsp, qmp = probe.RSP(gs, deadline), probe.QMP(qs, deadline)
                rsp.request('?')
                # Nonzero caller state detects lost scratch registers; a zero
                # reset state alone could hide a missing save/restore.
                for reg in range(1, 32):
                    value = (0x13570000 + reg * 0x101).to_bytes(4, 'little').hex()
                    if rsp.request(f'P{reg:x}={value}') != 'OK':
                        raise RuntimeError(f'cannot seed register x{reg}')
                if rsp.request('Z0,a000000c,2') != 'OK':
                    raise RuntimeError('comparison breakpoint rejected')
                stop = rsp.request('c')
                raw = bytes.fromhex(rsp.request('g'))
                pc = int.from_bytes(raw[128:132], 'little')
                if pc != ENTRY + 12:
                    raise AssertionError(f'{label}: unexpected stop PC {pc:#x}: {stop}')
                text = qmp.command('human-monitor-command', {'command-line': 'info registers'})
                match = re.search(r'\bmscratch\s+([0-9a-fA-F]+)', text)
                if not match:
                    raise AssertionError('mscratch missing from HMP output: ' + text)
                return dict(command=command, stop=stop, registers_hex=raw.hex(),
                            mscratch=int(match.group(1), 16), registers_hmp=text,
                            marker_hex=rsp.request(f'm{STACK - 16:x},4'), log=str(log_path))
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
                for sock_path in (gdb_path, qmp_path):
                    if sock_path.exists():
                        sock_path.unlink()

    with output.open('x', encoding='utf-8') as result:
        try:
            with candidate_path.open('xb') as candidate_file:
                candidate_file.write(candidate)
            report['qemu_sha256'] = sha(qemu.read_bytes())
            report['runner_sha256'] = sha(Path(__file__).read_bytes())
            report['qemu_version'] = subprocess.run(
                [str(qemu), '--version'], capture_output=True, text=True,
                check=True, timeout=10).stdout.strip()
            report['baseline'] = run(image, 'baseline')
            report['candidate'] = run(candidate_path, 'candidate')
            baseline, observed = report['baseline'], report['candidate']
            checks = dict(register_packet_equal=baseline['registers_hex'] == observed['registers_hex'],
                          mscratch_equal=baseline['mscratch'] == observed['mscratch'] == STACK,
                          baseline_marker_zero=baseline['marker_hex'] == '00000000',
                          candidate_marker=observed['marker_hex'] == '31585041',
                          unchanged_original_tail=bytes(candidate[0x100c:len(source)]) == source[0x100c:],
                          expected_entry_patch=bytes(candidate[0x1000:0x100c]) ==
                          entry_patch + landing.to_bytes(4, 'little'))
            report['checks'] = checks
            report['passed'] = all(checks.values())
        except Exception as exc:
            report.update(passed=False, error=f'{type(exc).__name__}: {exc}')
        finally:
            report['source_unchanged'] = sha(image.read_bytes()) == probe.SHA256
            report['passed'] = report.get('passed', False) and report['source_unchanged']
            json.dump(report, result, indent=2)
            result.write('\n')
    print(json.dumps({'passed': report['passed'], 'checks': report.get('checks'),
                      'error': report.get('error'), 'installable': False}))
    return 0 if report['passed'] else 1


if __name__ == '__main__':
    sys.exit(main())
