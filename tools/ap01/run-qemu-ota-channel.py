#!/usr/bin/env python3
"""Run the reconstructed0041 OTA candidate with a payload-only loopback socket provider.

This is a QEMU test transport, not a Wi-Fi implementation or physical OTA proof.
"""
import argparse
import hashlib
import importlib.util
import json
import re
from pathlib import Path
import select
import socket
import struct
import subprocess
import sys
import threading
import time
import uuid
import zlib

sys.dont_write_bytecode = True


def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


runner = module('ap01_runner', 'run-qemu-ap01.py')
builder = module('ap01_ota_builder', 'build-ota-channel-0041.py')
scan_mock = module('ap01_scan_mock', 'qemu_scan_mock.py')
APIS = {0xa00268c4: 'socket', 0xa003f446: 'bind', 0xa01a18ce: 'listen',
        0xa01a192a: 'accept', 0xa010d6f6: 'recv', 0xa003949c: 'sendto',
        0xa0026888: 'close', 0xa003f546: 'poll', 0xa010e532: 'setsockopt'}


class BufferedRSP(runner.RSP):
    """Keep the existing checksum/ACK parser; batch transport reads only."""
    def __init__(self, sock, deadline):
        super().__init__(sock, deadline)
        self.pending = b''
        self.position = 0

    def byte(self):
        remaining = self.deadline - time.monotonic()
        if remaining <= 0:
            raise TimeoutError('probe deadline reached')
        if self.position == len(self.pending):
            self.sock.settimeout(remaining)
            self.pending = self.sock.recv(4096)
            self.position = 0
            if not self.pending:
                raise ConnectionError('GDB connection closed')
        result = self.pending[self.position:self.position + 1]
        self.position += 1
        return result


def ram(address, count):
    return count >= 0 and any(base <= address and address + count <= base + length
                             for base, length in [(b, 0x78000) for b in
                                 (0x20fc0000, 0x22fc0000, 0x60fc0000, 0x62fc0000)] +
                                 [(b, 0x400000) for b in (0x88000000, 0xa8000000, 0xd8000000)])


class Provider:
    def __init__(self, rsp, port, ready, report):
        self.rsp, self.port, self.ready, self.report = rsp, port, ready, report
        self.sockets = {}
        self.next_fd = 0x4000

    def read(self, address, count):
        if count > 65536 or not ram(address, count):
            raise ValueError('provider pointer outside bounded guest RAM')
        return bytes.fromhex(self.rsp.request(f'm{address:x},{count:x}')) if count else b''

    def write(self, address, data):
        if not ram(address, len(data)):
            raise ValueError('provider output outside guest RAM')
        for offset in range(0, len(data), 512):
            chunk = data[offset:offset + 512]
            try:
                result = self.rsp.request(f'M{address + offset:x},{len(chunk):x}:{chunk.hex()}')
            except RuntimeError:
                # RSP's generic exception contains packet bytes, possibly auth data.
                raise RuntimeError('guest RAM write rejected') from None
            if result != 'OK':
                raise RuntimeError('guest RAM write rejected')

    def reg(self, number, value):
        if self.rsp.request(f'P{number:x}={(value & 0xffffffff).to_bytes(4,"little").hex()}') != 'OK':
            raise RuntimeError('register write rejected')

    def allocate(self, sock):
        if len(self.sockets) >= 8:
            sock.close()
            raise RuntimeError('provider socket limit reached')
        fd = self.next_fd
        self.next_fd += 1
        sock.setblocking(False)
        self.sockets[fd] = sock
        return fd

    def handle(self, name, args):
        fd, a1, a2, a3, a4, a5 = args
        if name == 'socket':
            if (fd, a1, a2) != (2, 1, 0):
                raise ValueError('unsupported socket domain/type/protocol')
            return self.allocate(socket.socket(socket.AF_INET, socket.SOCK_STREAM))
        if name == 'poll':
            if not 1 <= a1 <= 8:
                raise ValueError('unsupported provider poll count')
            records = self.read(fd, 24 * a1)
            entries = [struct.unpack_from('<III', records, i * 24) for i in range(a1)]
            if any(f not in self.sockets or events & ~5 for f, events, _ in entries):
                raise ValueError('mixed or unsupported provider poll descriptors')
            reads = [self.sockets[f] for f, events, _ in entries if events & 1]
            writes = [self.sockets[f] for f, events, _ in entries if events & 4]
            rr, ww, xx = select.select(reads, writes, list(set(reads + writes)), 0)
            count = 0
            for i, (f, events, _) in enumerate(entries):
                sock = self.sockets[f]
                bits = (1 if sock in rr else 0) | (4 if sock in ww else 0) | (8 if sock in xx else 0)
                self.write(fd + i * 24 + 8, struct.pack('<I', bits))
                count += bool(bits)
            if count:
                return count
            # Execute the original NuttX timed poll, so the task yields normally.
            # Its nfds=0 path is validated by the original implementation.
            self.reg(10, 0)
            self.reg(11, 0)
            self.reg(12, min(a2, 10))
            self.report['native_poll_yields'] += 1
            return None
        sock = self.sockets[fd]
        if name == 'bind':
            if a2 != 16:
                raise ValueError('unexpected sockaddr size')
            address = self.read(a1, 16)
            if (int.from_bytes(address[:2], 'little') != 2 or
                    int.from_bytes(address[2:4], 'big') != 18080 or address[4:8] != bytes(4)):
                raise ValueError('unexpected OTA bind address/port')
            sock.bind(('127.0.0.1', self.port))
            self.report['host_bound'] = True
            return 0
        if name == 'listen':
            sock.listen(4)
            self.ready.set()
            return 0
        if name == 'accept':
            if a1 or a2:
                raise ValueError('only NULL accept address output supported')
            incoming, _ = sock.accept()
            return self.allocate(incoming)
        if name in ('recv', 'sendto'):
            if a2 > 65536 or a3 != 0x40 or not ram(a1, a2):
                raise ValueError('unsupported transfer bounds/flags')
            if name == 'recv':
                data = sock.recv(a2)
                self.write(a1, data)
                self.report['received_bytes'] += len(data)
                return len(data)
            if a4 or a5:
                raise ValueError('TCP sendto must have NULL destination')
            count = sock.send(self.read(a1, a2))
            self.report['sent_bytes'] += count
            return count
        if name == 'setsockopt':
            if a1 != 1:
                raise ValueError('unsupported socket option level')
            value = self.read(a3, a4)
            if a2 == 11 and a4 == 4:
                sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, struct.unpack('<i', value)[0])
            elif a2 in (10, 14) and a4 == 8:
                sec, usec = struct.unpack('<ii', value)
                if not 0 <= sec <= 60 or not 0 <= usec < 1000000:
                    raise ValueError('invalid guest timeval')
                option = socket.SO_RCVTIMEO if a2 == 10 else socket.SO_SNDTIMEO
                sock.setsockopt(socket.SOL_SOCKET, option, struct.pack('@ll', sec, usec))
                # Nonblocking guest transfers still use the original poll deadlines.
            elif a2 == 12 and a4 == 4:
                sock.setsockopt(socket.SOL_SOCKET, socket.SO_SNDBUF, struct.unpack('<i', value)[0])
            else:
                raise ValueError('unsupported socket option')
            return 0
        if name == 'close':
            sock.close()
            del self.sockets[fd]
            return 0
        raise ValueError('unknown provider operation')


def client(port, ready, done, stop, report, deadline, upload=None, auth_key=None):
    tests = [('health', b'GET /health HTTP/1.1\r\n\r\n'),
             ('wrong_token', b'POST /firmware HTTP/1.1\r\nAuthorization: Bearer invalid\r\nContent-Length: 0\r\nX-ROM-CRC32: 00000000\r\n\r\n'),
             ('truncated', b'GET /health HTTP/1.1\r\nHost:')]
    if upload is not None:
        header = ('POST /firmware HTTP/1.1\r\nAuthorization: Bearer ' + auth_key +
                  f'\r\nContent-Length: {len(upload)}\r\nX-ROM-CRC32: {zlib.crc32(upload):08x}\r\n\r\n').encode('ascii')
        tests.append(('upload', header))
    try:
        if not ready.wait(max(0, deadline - time.monotonic())):
            raise TimeoutError('guest did not listen before deadline')
        if stop.is_set():
            raise TimeoutError('guest stopped before client requests')
        for name, request in tests:
            with socket.create_connection(('127.0.0.1', port), timeout=3) as sock:
                sock.settimeout(.25)
                request_deadline = deadline if name == 'upload' else min(deadline, time.monotonic() + 25)
                for block in (request, upload if name == 'upload' else b''):
                    view = memoryview(block)
                    sent = 0
                    while sent < len(view):
                        if stop.is_set() or time.monotonic() >= request_deadline:
                            raise TimeoutError('request send deadline reached')
                        try:
                            count = sock.send(view[sent:sent + 16384])
                        except socket.timeout:
                            continue
                        if count == 0:
                            raise ConnectionError('host TCP closed during request')
                        sent += count
                        if name == 'upload' and block is upload:
                            report['upload_body_sent'] = sent
                if name == 'truncated':
                    sock.shutdown(socket.SHUT_WR)
                data = bytearray()
                while len(data) < 8192:
                    if stop.is_set() or time.monotonic() >= request_deadline:
                        raise TimeoutError('request stopped or deadline reached')
                    try:
                        chunk = sock.recv(1024)
                    except socket.timeout:
                        continue
                    if not chunk:
                        break
                    data.extend(chunk)
                response = bytes(data).decode('ascii', 'replace')
                report['responses'].append(dict(test=name, response=response))
                if name == 'upload' and not response.startswith('HTTP/1.1 200'):
                    raise RuntimeError('guest rejected authenticated upload; see response')
    except Exception as exc:
        report['client_error'] = f'{type(exc).__name__}: {exc}'
    finally:
        done.set()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('qemu', 'image', 'manifest', 'flash-fixture', 'rom-shim', 'output', 'profile'):
        parser.add_argument('--' + name, type=Path, required=True)
    parser.add_argument('--port', type=int, default=18080)
    parser.add_argument('--timeout', type=float, default=60)
    parser.add_argument('--upload-image', type=Path)
    parser.add_argument('--upload-manifest', type=Path)
    parser.add_argument('--auth-key-file', type=Path)
    parser.add_argument('--active-slot', type=int, choices=(0, 1), default=0)
    parser.add_argument('--gpio3-input-high', action='store_true', help='explicit virtual normal-mode strap')
    args = parser.parse_args()
    if not 1 <= args.timeout <= 240 or not 1024 <= args.port <= 65535:
        parser.error('host debugging timeout1..240 and unprivileged port required')
    if any((args.upload_image, args.upload_manifest, args.auth_key_file)) and not all((args.upload_image, args.upload_manifest, args.auth_key_file)):
        parser.error('upload-image, upload-manifest and auth-key-file must be supplied together')
    output = args.output.resolve()
    paths = [output] + [output.with_name(output.name + suffix) for suffix in ('.qemu.log', '.uart.log', '.uart2.log')]
    snapshot = output.with_suffix('.flash.bin')
    if not output.parent.is_dir() or any(p.exists() for p in paths) or (args.upload_image and snapshot.exists()):
        parser.error('new output/log names in an existing directory required')
    data = args.image.read_bytes()
    manifest = builder.validate_candidate(data, args.manifest)
    upload = auth_key = upload_manifest = None
    if args.upload_image:
        upload = args.upload_image.read_bytes()
        upload_manifest = builder.validate_candidate(upload, args.upload_manifest)
        auth_key = args.auth_key_file.read_text(encoding='ascii').strip()
        if len(auth_key) != 64 or any(c not in '0123456789abcdefABCDEF' for c in auth_key):
            parser.error('auth key must contain exactly64 hexadecimal characters')
        digest = hashlib.sha256(auth_key.encode('ascii')).hexdigest()
        if any(m['auth_key_sha256'] != digest for m in (manifest, upload_manifest)):
            parser.error('auth key does not match running/upload build identities')
    flash = args.flash_fixture.read_bytes()
    active_offset = 0x10000 if args.active_slot == 0 else 0x7b0000
    if len(flash) != 0x1000000 or flash[active_offset:active_offset + len(data)] != data:
        parser.error('NOR fixture must contain the exact running candidate in the selected slot')
    payload_start = 0x9ffff000 + struct.unpack_from('<I', Path(manifest['payload']['path']).read_bytes(), 16)[0]
    code_start, code_end = payload_start + 256, payload_start + manifest['payload']['bytes']
    profile = json.loads(args.profile.read_text())['command']
    machine = profile[profile.index('-M') + 1]
    fields = [part for part in machine.split(',') if not part.startswith(('rom-shim=', 'flash-fixture=', 'ota-flash-mock=', 'ota-fw1-active='))]
    fields += ['rom-shim=' + str(args.rom_shim.resolve()), 'flash-fixture=' + str(args.flash_fixture.resolve())]
    if upload is not None:
        fields.append('ota-flash-mock=on')
    if args.active_slot == 1:
        fields.append('ota-fw1-active=on')
    if args.gpio3_input_high and 'gpio3-input-high=on' not in fields:
        fields = [part for part in fields if not part.startswith('gpio3-input-high=')]
        fields.append('gpio3-input-high=on')
    if any(',' in str(p.resolve()) for p in (args.rom_shim, args.flash_fixture)):
        parser.error('fixture paths cannot contain commas')
    unique = '/tmp/ap01-ota-' + uuid.uuid4().hex
    gdb, qmp_path = Path(unique + '.gdb'), Path(unique + '.qmp')
    command = [str(args.qemu.resolve()), '-M', ','.join(fields), '-cpu', profile[profile.index('-cpu') + 1],
               '-kernel', str(args.image.resolve()), '-S', '-display', 'none', '-monitor', 'none',
               '-chardev', f'file,id=uart0,path={paths[2]},append=on', '-serial', 'chardev:uart0',
               '-serial', 'null', '-chardev', f'file,id=uart2,path={paths[3]},append=on', '-serial', 'chardev:uart2',
               '-gdb', f'unix:{gdb},server=on,wait=off', '-qmp', f'unix:{qmp_path},server=on,wait=off',
               '-icount', 'shift=2,align=off,sleep=on', '-d', 'guest_errors']
    report = dict(passed=False, command=command, transport='payload-only host loopback TCP, not radio',
                  image_sha256=hashlib.sha256(data).hexdigest(), build_id=manifest['build_id'],
                  qemu_sha256=hashlib.sha256(args.qemu.read_bytes()).hexdigest(),
                  runner_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                  scan_mock_sha256=hashlib.sha256(Path(__file__).with_name('qemu_scan_mock.py').read_bytes()).hexdigest(),
                  manifest_sha256=hashlib.sha256(args.manifest.read_bytes()).hexdigest(),
                  flash_fixture_sha256=hashlib.sha256(flash).hexdigest(),
                  provider_code_range=[hex(code_start), hex(code_end)], device_accessed=False,
                  full_rom_boot_verified=False, physical_ota_verified=False, responses=[],
                  received_bytes=0, sent_bytes=0, native_poll_yields=0, native_api_passes=0,
                  provider_calls={}, host_bound=False)
    report['host_timeout_seconds'] = args.timeout
    report['host_endpoint'] = dict(address='127.0.0.1', port=args.port, guest_port=18080)
    report.update(active_slot=args.active_slot, active_offset=hex(active_offset),
                  upload_requested=upload is not None, native_ff13_calls=[], reset_requested=False)
    if upload is not None:
        report.update(upload_sha256=hashlib.sha256(upload).hexdigest(),
                      upload_build_id=upload_manifest['build_id'], upload_bytes=len(upload), upload_body_sent=0)
    report['assumptions'] = [
        'Payload-only host loopback socket API provider; no radio or physical Wi-Fi equivalence',
        'bt_enable returns typed -ENODEV; original BLE failure branch remains native',
        'Explicit empty scan API transaction:20ms guest ticks pending, then completion and no AP results; no association/radio',
        'Exact adc device lookup returns NULL; other device lookups execute natively, no ADC samples supplied']
    report['adc_unavailable_calls'] = 0
    deadline = time.monotonic() + args.timeout
    process = rsp = qmp = provider = worker = None
    sockets = []
    ready, done, stop = threading.Event(), threading.Event(), threading.Event()
    try:
        with paths[1].open('xb') as log:
            process = subprocess.Popen(command, stdout=log, stderr=log, preexec_fn=runner.child_limits)
            report['qemu_sha256'] = hashlib.sha256(Path(f'/proc/{process.pid}/exe').read_bytes()).hexdigest()
            sockets = [runner.connect(p, process, deadline) for p in (gdb, qmp_path)]
            rsp = BufferedRSP(sockets[0], deadline)
            qmp = runner.QMP(sockets[1], deadline)
            provider = Provider(rsp, args.port, ready, report)
            def scan_read(address, count):
                if not (0 <= count <= 64 and (ram(address, count) or
                        0x9ffff000 <= address and address + count <= 0xa0fff000)):
                    raise ValueError('scan API pointer outside bounded RAM/XIP')
                return bytes.fromhex(rsp.request(f'm{address:x},{count:x}'))
            empty_scan = scan_mock.EmptyScan(scan_read, report)
            breakpoints = [0, 0xa00da95e, *scan_mock.APIS, 0xa0025e1e, 0xa0055a14, 0xa019750e, *APIS]
            for address in breakpoints:
                if rsp.request(f'Z0,{address:x},2') != 'OK':
                    raise RuntimeError('cannot install API breakpoint')
            worker = threading.Thread(target=client, args=(args.port, ready, done, stop, report, deadline, upload, auth_key), daemon=True)
            worker.start()
            ff13_returns = {}
            create_returns = {}
            serve_entries = set()
            def watchdog():
                while not stop.wait(.02):
                    try:
                        uart = paths[2].read_bytes() if paths[2].exists() else b''
                    except OSError:
                        # WSL drvfs can transiently reject concurrent file reads.
                        uart = b''
                    fatal = re.search(rb'EXCEPTION: [^\r\n]*MTVAL: [0-9a-fA-F]{8}', uart)
                    if fatal:
                        report['first_guest_exception'] = fatal.group().decode('ascii', 'replace')
                    completed = done.is_set() and (upload is None or report.get('client_error'))
                    if time.monotonic() >= deadline - .5:
                        report['deadline_stop_requested'] = True
                    if fatal or completed or time.monotonic() >= deadline - .5 or any(p.exists() and p.stat().st_size >= runner.LOG_LIMIT for p in paths[1:]):
                        try:
                            sockets[0].sendall(b'\x03')
                        except OSError:
                            pass
                        return
            threading.Thread(target=watchdog, daemon=True).start()
            while time.monotonic() < deadline:
                rsp.request('c')
                raw = bytes.fromhex(rsp.request('g'))
                regs = struct.unpack_from('<33I', raw)
                pc, ra = regs[32], regs[1]
                args6 = regs[10:16]
                report['pc'], report['sp'] = hex(pc), hex(regs[2])
                if report.get('deadline_stop_requested'):
                    raise TimeoutError('total execution deadline')
                if report.get('first_guest_exception'):
                    raise RuntimeError('native guest exception; see first_guest_exception')
                if done.is_set() and (upload is None or report.get('client_error')):
                    report['stop_reason'] = 'client completed'
                    break
                if pc == 0:
                    raise RuntimeError('guest trap PC0')
                if pc in create_returns:
                    item = create_returns.pop(pc)
                    item['result'] = struct.unpack('<i', struct.pack('<I', regs[10]))[0]
                    item['tid'] = int.from_bytes(provider.read(item['tid_pointer'], 4), 'little')
                    rsp.request(f'z0,{pc:x},2')
                    continue
                if pc in serve_entries:
                    report['serve_entry_calls'] = report.get('serve_entry_calls', 0) + 1
                    report['serve_argument'] = hex(regs[10])
                    report['serve_entry_sp'] = hex(regs[2])
                    report['payload_api_min_sp'] = hex(regs[2])
                    report['observed_payload_stack_descent_bytes'] = 0
                    report['stack_observation_scope'] = 'serve entry minus minimum SP at selected payload socket API calls; lower bound, not full high-water or native callee usage'
                    serve_entries.remove(pc)
                    rsp.request(f'z0,{pc:x},2')
                    continue
                if pc == 0xa019750e and code_start <= ra < code_end:
                    item = dict(caller=hex(ra), tid_pointer=regs[10], entry=hex(regs[12]), stack_bytes=regs[13])
                    report.setdefault('thread_create', []).append(item)
                    if not code_start <= regs[12] < code_end or len(report['thread_create']) > 4:
                        raise RuntimeError('unexpected payload thread creation')
                    create_returns[ra] = item
                    serve_entries.add(regs[12])
                    for address in (ra, regs[12]):
                        if rsp.request(f'Z0,{address:x},2') != 'OK':
                            raise RuntimeError('cannot observe native thread entry/return')
                if pc == 0xa0055a14 and regs[10] == 0xff04:
                    report['stop_reason'] = 'guest requested reset'
                    report['reset_requested'] = True
                    report['reset_caller'] = hex(ra)
                    report['reset_payload_caller'] = code_start <= ra < code_end
                    report['premature_reset'] = not (code_start <= ra < code_end and
                        any(call.get('result') == 0 and call['payload_caller'] for call in report['native_ff13_calls']))
                    if upload is not None:
                        # close(fd) already ran; let the host consume its EOF/response
                        # before assessing success while the guest is held at reset.
                        worker.join(timeout=max(0, min(1, deadline - time.monotonic())))
                        upload_replied = any(r['test'] == 'upload' and r['response'].startswith('HTTP/1.1 200')
                                             for r in report['responses'])
                        report['premature_reset'] = report['premature_reset'] or not upload_replied
                        qmp.command('qom-set', {'path': '/machine', 'property': 'flash-snapshot', 'value': str(snapshot)})
                        snapshot_data = snapshot.read_bytes()
                        if len(snapshot_data) != 0x1000000:
                            raise RuntimeError('invalid NOR snapshot size')
                        report['flash_snapshot'] = dict(path=str(snapshot), bytes=len(snapshot_data),
                            sha256=hashlib.sha256(snapshot_data).hexdigest())
                    break
                if pc in ff13_returns:
                    index = ff13_returns.pop(pc)
                    report['native_ff13_calls'][index]['result'] = struct.unpack('<i', struct.pack('<I', regs[10]))[0]
                    rsp.request(f'z0,{pc:x},2')
                    continue
                if pc == 0xa0055a14 and regs[10] == 0xff13:
                    if len(report['native_ff13_calls']) >= 16:
                        raise RuntimeError('native ff13 observation budget exceeded')
                    report['native_ff13_calls'].append(dict(caller=hex(ra), payload_caller=code_start <= ra < code_end))
                    if code_start <= ra < code_end:
                        if rsp.request(f'Z0,{ra:x},2') != 'OK':
                            raise RuntimeError('cannot observe native ff13 return')
                        ff13_returns[ra] = len(report['native_ff13_calls']) - 1
                if pc == 0xa00da95e:
                    provider.reg(10, -19)
                    provider.reg(32, ra)
                    report['ble_unavailable_calls'] = report.get('ble_unavailable_calls', 0) + 1
                    continue
                if pc in scan_mock.APIS:
                    provider.reg(10, empty_scan.handle(pc, args6))
                    provider.reg(32, ra)
                    continue
                if pc == 0xa0025e1e:
                    # Lookup names are immutable XIP strings or guest RAM.
                    pointer = regs[10]
                    if ram(pointer, 4) or 0x9ffff000 <= pointer <= 0xa0ffeffc:
                        name_bytes = bytes.fromhex(rsp.request(f'm{pointer:x},4'))
                        if name_bytes == b'adc\0':
                            report['adc_unavailable_calls'] += 1
                            if report['adc_unavailable_calls'] > 64:
                                raise RuntimeError('ADC-unavailable call budget exceeded')
                            provider.reg(10, 0)
                            provider.reg(32, ra)
                            continue
                name = APIS.get(pc)
                eligible = code_start <= ra < code_end
                if name and eligible and 'serve_entry_sp' in report:
                    entry_sp = int(report['serve_entry_sp'], 16)
                    if entry_sp - 8192 <= regs[2] <= entry_sp:
                        minimum = min(int(report['payload_api_min_sp'], 16), regs[2])
                        report['payload_api_min_sp'] = hex(minimum)
                        report['observed_payload_stack_descent_bytes'] = entry_sp - minimum
                if name and eligible and (name == 'socket' or args6[0] in provider.sockets or
                        name == 'poll' and args6[1] and struct.unpack('<I', provider.read(args6[0], 4))[0] in provider.sockets):
                    report['provider_calls'][name] = report['provider_calls'].get(name, 0) + 1
                    result = provider.handle(name, args6)
                    if result is not None:
                        provider.reg(10, result)
                        provider.reg(32, ra)
                        continue
                # Only this entry instruction is stepped with its breakpoint removed.
                # Every original-ROM caller continues through the real implementation.
                if pc in breakpoints:
                    rsp.request(f'z0,{pc:x},2')
                    rsp.request('s')
                    rsp.request(f'Z0,{pc:x},2')
                    report['native_api_passes'] += 1
            else:
                raise TimeoutError('total execution deadline')
            report['registers'] = qmp.command('human-monitor-command', {'command-line': 'info registers'})
            report['guest_ticks'] = int.from_bytes(provider.read(0x62fc6c64, 4), 'little')
            report['guest_uptime_ms'] = report['guest_ticks']
            responses = report['responses']
            report['passed'] = len(responses) == 3 and not report['reset_requested'] and not report.get('client_error') and responses[0]['response'].startswith('HTTP/1.1 200') and responses[1]['response'].startswith('HTTP/1.1 401') and responses[2]['response'].startswith(('HTTP/1.1 400', 'HTTP/1.1 500'))
            if upload is not None:
                report['passed'] = (len(responses) == 4 and not report.get('client_error') and
                    responses[0]['response'].startswith('HTTP/1.1 200') and
                    responses[1]['response'].startswith('HTTP/1.1 401') and
                    responses[2]['response'].startswith(('HTTP/1.1 400', 'HTTP/1.1 500')) and
                    responses[3]['response'].startswith('HTTP/1.1 200') and
                    report['reset_requested'] and not report.get('premature_reset', True) and
                    report.get('reset_payload_caller', False) and bool(report.get('flash_snapshot')) and
                    report['upload_body_sent'] == len(upload) and
                    any(call.get('result') == 0 and call['payload_caller'] for call in report['native_ff13_calls']))
    except Exception as exc:
        report['error'] = f'{type(exc).__name__}: {exc}'
        if qmp and time.monotonic() < deadline:
            try:
                report['registers'] = qmp.command('human-monitor-command', {'command-line': 'info registers'})
            except Exception:
                pass
        if provider and time.monotonic() < deadline:
            try:
                report['guest_uptime_ms'] = int.from_bytes(provider.read(0x62fc6c64, 4), 'little')
            except Exception:
                pass
    finally:
        stop.set()
        ready.set()
        if worker:
            worker.join(timeout=1)
        if provider:
            for sock in provider.sockets.values():
                sock.close()
        if process:
            process.terminate()
            try:
                process.wait(timeout=2)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=2)
        for sock in sockets:
            sock.close()
        for path in (gdb, qmp_path):
            path.unlink(missing_ok=True)
        report['source_unchanged'] = hashlib.sha256(args.image.read_bytes()).hexdigest() == report['image_sha256']
        report['flash_fixture_unchanged'] = hashlib.sha256(args.flash_fixture.read_bytes()).hexdigest() == report['flash_fixture_sha256']
        report['uart_paths'] = [str(p) for p in paths[2:]]
        uart = paths[2].read_bytes() if paths[2].exists() else b''
        report['native_ui_started'] = b'lv_ap01_display_process, start' in uart
        fatal = re.search(rb'EXCEPTION: [^\r\n]+', uart)
        if fatal:
            report['first_guest_exception'] = fatal.group().decode('ascii', 'replace')
        report['passed'] = bool(report['passed'] and not report.get('first_guest_exception') and
            report['source_unchanged'] and report['flash_fixture_unchanged'])
        with output.open('x') as stream:
            json.dump(report, stream, indent=2)
            stream.write('\n')
    print(json.dumps(dict(passed=report['passed'], output=str(output), error=report.get('error'))))
    return 0 if report['passed'] else 1


if __name__ == '__main__':
    sys.exit(main())
