#!/usr/bin/env python3
"""Launch an isolated AP01 QEMU daemon and pause at the current boot boundary."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import signal
import socket
import subprocess
import sys
import time
import uuid

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location('ap01_probe', Path(__file__).with_name('run-qemu-ap01.py'))
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--qemu', type=Path, required=True)
    parser.add_argument('--image', type=Path, required=True)
    parser.add_argument('--otp-fixture', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    qemu, image, fixture, output = (p.resolve() for p in
                                  (args.qemu, args.image, args.otp_fixture, args.output))
    if output.exists() or not output.parent.is_dir():
        parser.error('output must be a new JSON file in an existing directory')
    source, otp = image.read_bytes(), fixture.read_bytes()
    if len(source) != 6769704 or hashlib.sha256(source).hexdigest() != probe.SHA256:
        parser.error('only the exact original 0041 ROM is accepted')
    if len(otp) != 2048 or ',' in str(fixture):
        parser.error('OTP fixture must be 2048 bytes with no comma in its path')
    token = 'ap01-live-' + uuid.uuid4().hex
    gdb_path, qmp_path = (Path('/tmp') / (token + suffix) for suffix in ('.gdb', '.qmp'))
    pid_path, log_path, startup_path = (output.with_name(output.stem + suffix)
                                      for suffix in ('.pid', '.qemu.log', '.startup.log'))
    if any(p.exists() for p in (pid_path, log_path, startup_path)):
        parser.error('session companion paths must not exist')
    machine = ('ap01,mhint-latch=on,mtvt-latch=on,cache-csr-latches=on,'
               'boot-mmio=on,efuse-shadow=on,psram-mock=on,boot2-mock=on,otp-fixture=' + str(fixture))
    cpu = ('rv32,x-ap01-p=on,xtheadba=on,xtheadbb=on,xtheadbs=on,xtheadcmo=on,'
           'xtheadcondmov=on,xtheadmac=on,xtheadmemidx=on,xtheadmempair=on,xtheadsync=on')
    command = [str(qemu), '-M', machine, '-cpu', cpu, '-kernel', str(image),
               '-S', '-display', 'none', '-serial', 'none', '-monitor', 'none',
               '-gdb', f'unix:{gdb_path},server=on,wait=off',
               '-qmp', f'unix:{qmp_path},server=on,wait=off',
               '-daemonize', '-pidfile', str(pid_path),
               '-D', str(log_path), '-d', 'int,guest_errors']
    report = dict(command=command, original_sha256=probe.SHA256,
                  qemu_sha256=hashlib.sha256(qemu.read_bytes()).hexdigest(),
                  otp_fixture_sha256=hashlib.sha256(otp).hexdigest(),
                  synthetic_external_environment=True, device_equivalence_verified=False,
                  external_mocks=['OTP fixture bank0 read', 'PSRAM request/config and synthetic IDs',
                                  'Boot2 cached JEDEC and unsupported flash-config API',
                                  'virtual XTAL/PLL request and clock state'],
                  full_rom_boot_verified=False, pidfile=str(pid_path),
                  gdb_socket=str(gdb_path), qmp_socket=str(qmp_path), log=str(log_path))
    pid = None
    gs = qs = qmp = None
    success = False
    with output.open('x', encoding='utf-8') as result:
        try:
            with startup_path.open('x') as startup:
                subprocess.run(command, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                               stderr=startup, timeout=10, check=True,
                               preexec_fn=probe.child_limits)
            pid = int(pid_path.read_text().strip())
            if Path(f'/proc/{pid}/exe').resolve() != qemu:
                raise RuntimeError('daemon executable does not match requested QEMU')
            deadline = time.monotonic() + 20
            gs = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            gs.settimeout(3)
            gs.connect(str(gdb_path))
            qs = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            qs.settimeout(3)
            qs.connect(str(qmp_path))
            rsp, qmp = probe.RSP(gs, deadline), probe.QMP(qs, deadline)
            rsp.request('?')
            if rsp.request('Z0,0,2') != 'OK':
                raise RuntimeError('QEMU refused the early trap breakpoint')
            try:
                report['stop_reply'] = rsp.request('c')
            except (TimeoutError, socket.timeout):
                report['stop_reason'] = 'host-run-budget'
            # Detach the transport and explicitly stop using the independent
            # QMP channel; do not leave a guest exception loop running.
            gs.close()
            gs = None
            qmp.deadline = time.monotonic() + 5
            qmp.command('stop')
            text = qmp.command('human-monitor-command', {'command-line': 'info registers'})
            report['registers_hmp'] = text
            report['trap_registers'] = {
                name: '0x' + match.group(1)
                for name in ('pc', 'mepc', 'mcause', 'mtval', 'mtvec')
                if (match := re.search(r'\b' + name + r'\s+([0-9a-fA-F]+)', text))
            }
            report['status'] = qmp.command('query-status')
            os.kill(pid, 0)
            report.update(pid=pid, alive=True, paused=not report['status']['running'])
            success = report['paused']
        except Exception as exc:
            report.update(error=f'{type(exc).__name__}: {exc}', alive=False)
        finally:
            if qmp is not None:
                qmp.file.close()
            for sock in (gs, qs):
                if sock is not None:
                    sock.close()
            if not success:
                # A daemon may have forked before the startup wrapper failed.
                # Only recover the PID belonging to this unique pidfile/exe.
                try:
                    failed_pid = pid or int(pid_path.read_text().strip())
                    proc = Path(f'/proc/{failed_pid}')
                    argv = (proc / 'cmdline').read_bytes().split(b'\0')
                    if (failed_pid > 1 and (proc / 'exe').resolve() == qemu and
                            str(pid_path).encode() in argv):
                        os.kill(failed_pid, signal.SIGTERM)
                        until = time.monotonic() + 2
                        while proc.exists() and time.monotonic() < until:
                            time.sleep(0.01)
                        report['failed_daemon_still_present'] = proc.exists()
                except (OSError, ValueError):
                    pass
            report['source_unchanged'] = hashlib.sha256(image.read_bytes()).hexdigest() == probe.SHA256
            json.dump(report, result, indent=2)
            result.write('\n')
    print(json.dumps({key: report.get(key) for key in
                      ('pid', 'alive', 'paused', 'trap_registers', 'error')}))
    return 0 if success and report['source_unchanged'] else 1


if __name__ == '__main__':
    sys.exit(main())
