#!/usr/bin/env python3
"""Bounded original AP01 0041 QEMU probe; never asserts full-board boot."""
import argparse
from collections import deque
import hashlib
import json
from pathlib import Path
import re
import socket
import subprocess
import sys
import threading
import time
import uuid
from qemu_scan_mock import EmptyScan, APIS as SCAN_APIS

sys.dont_write_bytecode = True
SHA256 = '972db4c136c7ed9e24a83c07c1a7fd62040ca018b08ca285216d26b1fee3c6b9'
UNSIGNED_OTA_SHA256 = '1bbe8f45359eb7cdb55f337643f3ce28894999ffb21beab92fb62e995cd4854d'


def validate_running_image(data, original_path=None, ota_manifest=None):
    """Accept only original0041 or its explicitly authorized frozen patch."""
    digest = hashlib.sha256(data).hexdigest()
    identity = dict(original_sha256=SHA256, current_sha256=digest,
                    kind='original0041', patch_manifest=None)
    if ota_manifest is not None:
        if original_path is not None:
            raise ValueError('candidate identity options are mutually exclusive')
        import importlib.util
        spec = importlib.util.spec_from_file_location('ota_builder', Path(__file__).with_name('build-ota-channel-0041.py'))
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        built = module.validate_candidate(data, ota_manifest)
        identity.update(kind='0041-independent-ota-channel', original_path=built['original']['path'],
                        patch_manifest=dict(kind=built['format'], path=str(ota_manifest.resolve()),
                            sha256=hashlib.sha256(ota_manifest.read_bytes()).hexdigest(), build_id=built['build_id']))
        return identity
    if original_path is None:
        if len(data) != 6769704 or digest != SHA256:
            raise ValueError('only the exact original0041 image is accepted without --unsigned-ota-candidate ORIGINAL_IMAGE')
        return identity
    if len(data) != 6769704 or digest != UNSIGNED_OTA_SHA256:
        raise ValueError('unsigned OTA candidate must match its frozen SHA256')
    from patch_0041_unsigned_ota import patch_bytes
    original_path = original_path.resolve()
    expected, validation = patch_bytes(original_path.read_bytes())
    if data != expected:
        raise ValueError('candidate differs from patch_bytes(original) recomputation')
    encoded = json.dumps(validation, sort_keys=True, separators=(',', ':')).encode()
    identity.update(kind='0041-unsigned-ota-candidate', original_path=str(original_path),
                    patch_manifest=dict(kind='recomputed-patch_bytes-validation',
                        sha256=hashlib.sha256(encoded).hexdigest(), validation=validation,
                        tool='patch_0041_unsigned_ota.py',
                        tool_sha256=hashlib.sha256(Path(__file__).with_name('patch_0041_unsigned_ota.py').read_bytes()).hexdigest()))
    return identity


LOG_LIMIT = 16 * 1024 * 1024


def child_limits():
    """A guest trap loop must not fill the host disk or create core files."""
    import resource
    resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
    resource.setrlimit(resource.RLIMIT_FSIZE, (LOG_LIMIT, LOG_LIMIT))


class RSP:
    def __init__(self, sock, deadline):
        self.sock, self.deadline = sock, deadline

    def byte(self):
        remaining = self.deadline - time.monotonic()
        if remaining <= 0:
            raise TimeoutError('probe deadline reached')
        self.sock.settimeout(remaining)
        value = self.sock.recv(1)
        if not value:
            raise ConnectionError('GDB connection closed')
        return value

    def request(self, payload):
        raw = payload.encode('ascii')
        packet = b'$' + raw + b'#' + f'{sum(raw) & 255:02x}'.encode()
        self.sock.sendall(packet)
        # ACK mode is retained throughout; retransmit only rejected packets.
        while True:
            marker = self.byte()
            if marker == b'-':
                self.sock.sendall(packet)
            elif marker == b'+':
                break
            else:
                raise ValueError(f'expected RSP ACK, got {marker!r}')
        while True:
            while self.byte() != b'$':
                pass
            encoded = bytearray()
            while True:
                char = self.byte()
                if char == b'#':
                    break
                encoded.extend(char)
            checksum = int(self.byte() + self.byte(), 16)
            if sum(encoded) & 255 != checksum:
                self.sock.sendall(b'-')
                continue
            self.sock.sendall(b'+')
            decoded = bytearray()
            index = 0
            while index < len(encoded):
                char = encoded[index]
                if char == 0x7d:
                    index += 1
                    if index == len(encoded):
                        raise ValueError('truncated RSP escape')
                    decoded.append(encoded[index] ^ 0x20)
                elif char == 0x2a:
                    index += 1
                    if not decoded or index == len(encoded):
                        raise ValueError('invalid RSP repetition')
                    count = encoded[index] - 29
                    if count < 0:
                        raise ValueError('invalid RSP repetition count')
                    decoded.extend([decoded[-1]] * count)
                else:
                    decoded.append(char)
                index += 1
            response = decoded.decode('ascii')
            if response.startswith('E'):
                raise RuntimeError(f'RSP {payload}: {response}')
            return response


def connect(path, process, deadline):
    while time.monotonic() < deadline:
        sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        sock.settimeout(max(0.001, deadline - time.monotonic()))
        try:
            sock.connect(str(path))
            return sock
        except (FileNotFoundError, ConnectionRefusedError):
            sock.close()
            if process.poll() is not None:
                raise RuntimeError(f'QEMU exited with {process.returncode}')
            time.sleep(min(0.01, max(0, deadline - time.monotonic())))
    raise TimeoutError('QEMU socket startup deadline reached')


class QMP:
    def __init__(self, sock, deadline):
        self.sock, self.deadline = sock, deadline
        self.file = sock.makefile('rb')
        self.read()
        self.command('qmp_capabilities')

    def read(self):
        remaining = self.deadline - time.monotonic()
        if remaining <= 0:
            raise TimeoutError('QMP deadline reached')
        self.sock.settimeout(remaining)
        line = self.file.readline(1024 * 1024)
        if not line:
            raise ConnectionError('QMP connection closed')
        return json.loads(line)

    def command(self, execute, arguments=None):
        request = {'execute': execute}
        if arguments is not None:
            request['arguments'] = arguments
        self.sock.sendall(json.dumps(request).encode() + b'\r\n')
        while True:
            response = self.read()
            if 'error' in response:
                raise RuntimeError(f'QMP error: {response["error"]}')
            if 'return' in response:
                return response['return']


def registers(rsp):
    raw = bytes.fromhex(rsp.request('g'))
    if len(raw) < 33 * 4:
        raise ValueError(f'RV32 register packet too short: {len(raw)}')
    return (int.from_bytes(raw[128:132], 'little'),
            int.from_bytes(raw[8:12], 'little'))


def capture_display(qmp, output, report):
    state = qmp.command('qom-get', {'path': '/machine', 'property': 'lcd-state'})
    report['lcd_state'] = json.loads(state)
    report['pec_state'] = json.loads(qmp.command('qom-get',
        {'path': '/machine', 'property': 'pec-state'}))
    report['flash_state'] = json.loads(qmp.command('qom-get',
        {'path': '/machine', 'property': 'flash-state'}))
    report['rf_calibration_state'] = json.loads(qmp.command('qom-get',
        {'path': '/machine', 'property': 'rf-calibration-state'}))
    report['mac_state'] = json.loads(qmp.command('qom-get',
        {'path': '/machine', 'property': 'mac-state'}))
    report['machw_state'] = json.loads(qmp.command('qom-get',
        {'path': '/machine', 'property': 'machw-state'}))
    if report['lcd_state']['enabled']:
        frame = output.with_name(output.name + '.ppm')
        if frame.exists():
            raise FileExistsError('refusing to overwrite framebuffer evidence')
        qmp.command('screendump', {'filename': str(frame)})
        report['framebuffer'] = dict(path=str(frame), size=frame.stat().st_size,
                                    sha256=hashlib.sha256(frame.read_bytes()).hexdigest())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--qemu', type=Path, required=True)
    parser.add_argument('--image', type=Path, required=True)
    parser.add_argument('--ota-channel-manifest', type=Path,
                        help='reconstruct an independently built bounded OTA-channel image')
    parser.add_argument('--unsigned-ota-candidate', type=Path, metavar='ORIGINAL_IMAGE',
                        help='explicitly allow frozen candidate after patch_bytes recheck against this original')
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--cpu', default='rv32')
    parser.add_argument('--mhint-latch', action='store_true',
                        help='optional partial CSR model; reset/bit20 are unverified')
    parser.add_argument('--mtvt-latch', action='store_true',
                        help='store MTVT; does not implement CLIC interrupt dispatch')
    parser.add_argument('--boot-mmio', action='store_true',
                        help='SDK register latches only; no complete peripheral model')
    parser.add_argument('--cache-csr-latches', action='store_true',
                        help='MXSTATUS/MHCR/MEXSTATUS storage; no privilege/cache/reset/sleep effects')
    parser.add_argument('--efuse-shadow', action='store_true',
                        help='track software-written eFuse shadow bytes; unknown OTP is rejected')
    parser.add_argument('--otp-fixture', type=Path,
                        help='explicit synthetic/read-only OTP contents (2048 bytes)')
    parser.add_argument('--psram-mock', action='store_true',
                        help='explicit synthetic PSRAM config request/command peripheral')
    parser.add_argument('--boot2-mock', action='store_true',
                        help='synthetic Boot2 cached JEDEC for an 8MiB flash model')
    parser.add_argument('--rom-shim', type=Path,
                        help='explicit native synthetic BootROM leaf image')
    parser.add_argument('--clic-mock', action='store_true',
                        help='restricted non-SHV CLIC timer7 gating; use CPU x-ap01-clic')
    parser.add_argument('--pec-uart-mock', action='store_true',
                        help='replace declared PEC table bindings with a synthetic UART2 API model')
    parser.add_argument('--flash-fixture', type=Path,
                        help='explicit synthetic16MiB NOR, never written back to its source')
    parser.add_argument('--rf-calibration-mock', action='store_true',
                        help='explicit synthetic LO/ACAL/TX/RX feedback; no physical RF equivalence')
    parser.add_argument('--mac-empty-mock', action='store_true',
                        help='explicit empty-radio64-source INTC; no network or fabricated pending IRQ')
    parser.add_argument('--trng-mock', action='store_true',
                        help='deterministic test entropy fixture, never secure or physical randomness')
    parser.add_argument('--ble-unavailable', action='store_true',
                        help='explicit bt_enable API model returns original no-HCI error -ENODEV')
    parser.add_argument('--scan-unavailable', action='store_true',
                        help='explicit SIOCSIWSCAN wrapper returns original negative errno -ENODEV')
    parser.add_argument('--adc-unavailable', action='store_true',
                        help='device lookup for exact adc returns NULL, preserving original setup failure')
    parser.add_argument('--steps', type=int, default=1024)
    parser.add_argument('--timeout', type=float, default=30)
    parser.add_argument('--observe-seconds', type=float, default=0,
                        help='bounded continuous run, then interrupt and capture state; use without trap-handler breakpoint')
    parser.add_argument('--gpio3-input-high', action='store_true',
                        help='explicit external GPIO3 high fixture selects native normal-mode branch')
    parser.add_argument('--scan-empty-mock', action='store_true',
                        help='explicit finite20ms empty scan API model; no association')
    parser.add_argument('--gpio24-input-high', action='store_true',
                        help='explicit released encoder-button external input fixture')
    parser.add_argument('--icount-shift', type=int, choices=range(0, 11),
                        help='explicit synthetic instruction time 2^N ns; independent of host speed')
    parser.add_argument('--icount-sleep', choices=('on', 'off'), default='on',
                        help='icount idle-time policy; on retains host-time pacing during idle')
    parser.add_argument('--run-to-trap', action='store_true',
                        help='run at TCG speed to a GDB breakpoint at mtvec=0; bounded by timeout')
    parser.add_argument('--trace-blocks', action=argparse.BooleanOptionalAction,
                        help='include translated blocks in the bounded QEMU log')
    parser.add_argument('--breakpoint', action='append', default=[],
                        type=lambda value: int(value, 0),
                        help='additional execution boundary VA (repeatable)')
    parser.add_argument('--pass-interrupts', type=int, default=0,
                        help='pass up to N asynchronous stops at original common trap 0x62fc2500')
    parser.add_argument('--pass-ecalls', type=int, default=0,
                        help='pass up to N normal machine ECALLs through the original trap')
    parser.add_argument('--steps-after-boundary', type=int, default=0,
                        help='single-step a bounded number after a continue boundary')
    parser.add_argument('--read-memory', action='append', default=[],
                        help='capture an internal RAM range ADDRESS:LENGTH at the stop (max4096 bytes total)')
    args = parser.parse_args()
    if args.scan_empty_mock and args.scan_unavailable:
        parser.error('scan policies are mutually exclusive')
    if args.observe_seconds and (not args.run_to_trap or not 0 < args.observe_seconds < args.timeout - 3):
        parser.error('--observe-seconds requires run-to-trap and at least3s capture margin')
    if not 1 <= args.steps <= 100000 or not 0 < args.timeout <= 60:
        parser.error('steps must be 1..100000 and timeout must be >0 and <=60')
    if not 0 <= args.pass_interrupts <= 10000 or not 0 <= args.pass_ecalls <= 10000 or (
            (args.pass_interrupts or args.pass_ecalls) and 0x62fc2500 not in args.breakpoint):
        parser.error('pass-interrupts must be 0..10000 and requires breakpoint 0x62fc2500')
    if not 0 <= args.steps_after_boundary <= 1000:
        parser.error('steps-after-boundary must be 0..1000')
    memory_ranges = []
    try:
        for item in args.read_memory:
            address, length = (int(part, 0) for part in item.split(':'))
            if not (0 < length <= 4096 and any(
                    base <= address and address + length <= base + 0x78000
                    for base in (0x20fc0000, 0x22fc0000, 0x60fc0000, 0x62fc0000))):
                raise ValueError('range must fit internal RAM')
            memory_ranges.append((address, length))
        if sum(length for _, length in memory_ranges) > 4096:
            raise ValueError('capture exceeds4096 bytes')
    except ValueError as exc:
        parser.error(f'invalid read-memory: {exc}')
    qemu, image, output = (p.resolve() for p in
                           (args.qemu, args.image, args.output))
    if output.exists() or not output.parent.is_dir():
        parser.error('output must be a new JSON file in an existing directory')
    source = image.read_bytes()
    try:
        image_identity = validate_running_image(source, args.unsigned_ota_candidate, args.ota_channel_manifest)
    except (ValueError, OSError) as exc:
        parser.error(str(exc))
    unique = 'ap01-' + uuid.uuid4().hex
    # /tmp already exists; long workspace paths exceed AF_UNIX's path limit.
    gdb_path, qmp_path = (Path('/tmp') / (unique + suffix)
                          for suffix in ('.gdb', '.qmp'))
    if any(len(str(p).encode()) >= 104 for p in (gdb_path, qmp_path)):
        parser.error('output directory path too long for Unix sockets')
    log_path = output.with_name(output.name + '.qemu.log')
    uart_path = output.with_name(output.name + '.uart.log')
    uart2_path = output.with_name(output.name + '.uart2.log')
    if uart_path.exists() or uart2_path.exists():
        parser.error('UART output must not already exist')
    machine = 'ap01,mhint-latch=on' if args.mhint_latch else 'ap01'
    if args.mtvt_latch:
        machine += ',mtvt-latch=on'
    if args.boot_mmio:
        machine += ',boot-mmio=on'
    if args.cache_csr_latches:
        machine += ',cache-csr-latches=on'
    if args.efuse_shadow:
        if not args.boot_mmio:
            parser.error('--efuse-shadow requires --boot-mmio')
        machine += ',efuse-shadow=on'
    otp_digest = None
    if args.otp_fixture:
        fixture = args.otp_fixture.resolve()
        if not args.efuse_shadow or ',' in str(fixture):
            parser.error('--otp-fixture requires --efuse-shadow and a path without commas')
        otp_data = fixture.read_bytes()
        if len(otp_data) != 0x800:
            parser.error('OTP fixture must contain exactly 2048 bytes')
        otp_digest = hashlib.sha256(otp_data).hexdigest()
        machine += ',otp-fixture=' + str(fixture)
    if args.psram_mock:
        if not args.boot_mmio:
            parser.error('--psram-mock requires --boot-mmio')
        machine += ',psram-mock=on'
    if args.boot2_mock:
        if not args.boot_mmio:
            parser.error('--boot2-mock requires --boot-mmio')
        machine += ',boot2-mock=on'
    if args.rom_shim:
        shim = args.rom_shim.resolve()
        if not args.boot2_mock or ',' in str(shim):
            parser.error('--rom-shim requires --boot2-mock and a path without commas')
        shim_data = shim.read_bytes()
        if not 0x800 < len(shim_data) <= 0x10000:
            parser.error('native ROM shim must contain 2049..65536 bytes')
        machine += ',rom-shim=' + str(shim)
    if args.clic_mock:
        if not args.boot2_mock or 'x-ap01-clic=on' not in args.cpu.split(','):
            parser.error('--clic-mock requires --boot2-mock and CPU x-ap01-clic=on')
        machine += ',clic-mock=on'
    if args.rf_calibration_mock:
        if not args.boot2_mock:
            parser.error('--rf-calibration-mock requires --boot2-mock')
        machine += ',rf-calibration-mock=on'
    if args.mac_empty_mock:
        if not args.boot2_mock:
            parser.error('--mac-empty-mock requires --boot2-mock')
        machine += ',mac-empty-mock=on'
    if args.trng_mock:
        if not args.boot2_mock:
            parser.error('--trng-mock requires --boot2-mock')
        machine += ',trng-mock=on'
    if args.gpio3_input_high:
        if not args.boot2_mock:
            parser.error('--gpio3-input-high requires --boot2-mock')
        machine += ',gpio3-input-high=on'
    if args.gpio24_input_high:
        if not args.boot2_mock:
            parser.error('--gpio24-input-high requires --boot2-mock')
        machine += ',gpio24-input-high=on'
    if args.pec_uart_mock:
        if not args.rom_shim:
            parser.error('--pec-uart-mock requires --rom-shim')
        machine += ',pec-uart-mock=on'
    if args.flash_fixture:
        flash = args.flash_fixture.resolve()
        flash_data = flash.read_bytes()
        if not args.boot2_mock or ',' in str(flash) or len(flash_data) != 0x1000000:
            parser.error('--flash-fixture requires boot2-mock and a16MiB file without commas')
        if flash_data[0x10000:0x10000 + len(source)] != source:
            parser.error('virtual FW0 must contain the exact selected running image at physical10000')
        machine += ',flash-fixture=' + str(flash)
    command = [str(qemu), '-M', machine, '-cpu', args.cpu, '-kernel', str(image),
               '-S', '-display', 'none', '-monitor', 'none',
               '-chardev', f'file,id=uart0,path={uart_path},append=on',
               '-serial', 'chardev:uart0',
               '-serial', 'null',
               '-gdb', f'unix:{gdb_path},server=on,wait=off',
               '-qmp', f'unix:{qmp_path},server=on,wait=off',
               '-d', 'in_asm,int,guest_errors' if args.trace_blocks else 'int,guest_errors']
    if args.icount_shift is not None:
        command += ['-icount', f'shift={args.icount_shift},align=off,sleep={args.icount_sleep}']
    if args.pec_uart_mock:
        command += ['-chardev', f'file,id=uart2,path={uart2_path},append=on',
                    '-serial', 'chardev:uart2']
    report = dict(image_sha256=image_identity["current_sha256"],
                  original_image_sha256=SHA256, running_image=image_identity, image_size=len(source), command=command,
                  execution_mode='single-step', retired_instructions=None,
                  log_limit_bytes=LOG_LIMIT,
                  qemu_sha256=hashlib.sha256(qemu.read_bytes()).hexdigest(),
                  runner_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                  full_rom_boot_verified=False, patches=[], function_stubs=[],
                  assumptions=['zero-filled RAM; no captured Boot2 handoff',
                               'incomplete AP01 functional model; no device equivalence'])
    if args.icount_shift is not None:
        report['assumptions'].append(f'icount synthetic instruction time={2 ** args.icount_shift}ns; not measured E907 timing')
    if args.rf_calibration_mock:
        report['assumptions'].append('synthetic MIX020 selection fixture=0: ROSCAL/RCCAL unselected, physical word meaning/reset unknown; mandatory calibration remains native')
        report['assumptions'].append('synthetic40MHz VCO feedback count=42688+(128-code)*64, finite virtual latency; no physical RF/calibration data')
        report['assumptions'].append('synthetic ACAL comparator threshold24, restricted to original40MHz channel table; not measured analog amplitude')
        report['assumptions'].append('synthetic TX scalar PM raw=(112+candidate-256-2*(trim-11)^2)*count, count1024/4096; TX IQ selectors9200/12400 use independent linear residuals; RX selectorf0000/count20480 uses614 residuals; 2us latency, no physical RF equivalence')
    if args.mac_empty_mock:
        report['assumptions'].append('empty radio INTC:64 unmask W1S sources, no external pending IRQ or wireless connection; empty INDEX read unsupported')
        report['assumptions'].append('virtual MAC profile:60MHz initial clock, timing fields0, RX buffer capability byte9, optional MAC features0, PHY count nibbles1=0x1111/version1.2=0x01020000; no physical chip/reset equivalence')
        report['assumptions'].append('MAC soft-reset request1 completes after synthetic2us and resets modeled core config/time; no external INTC/DMA reset')
        report['assumptions'].append('SYSCTRL24900040 is an independent virtual1MHz free-running32bit stats counter; physical frequency/gates unknown, MAC soft-reset retains it')
    if args.trng_mock:
        report['assumptions'].append('deterministic TRNG test blocks with finite2us busy; predictable, not physical entropy or secure token quality')
    if args.ble_unavailable:
        report['assumptions'].append('bt_enable(callback) API replacement at a00da95e returns -19, matching original no-HCI path; no BLE controller/ready/pairing claim')
        report['ble_unavailable_calls'] = 0
    if args.scan_unavailable:
        report['assumptions'].append('scan API a00761b4 returns -19 like failed SIOCSIWSCAN ioctl; original caller handles failure, no scan results or network association synthesized')
        report['scan_unavailable_calls'] = 0
    if args.adc_unavailable:
        report['assumptions'].append('bflb_device_get_by_name("adc") returns NULL; original ADC setup returns -1 without samples; all other device lookups execute natively')
        report['adc_unavailable_calls'] = 0
    if args.gpio3_input_high:
        report['assumptions'].append('Explicit external GPIO3 input-high fixture; original factory-test selector chooses normal mode. Physical strap level unknown.')
    if args.scan_empty_mock:
        report['assumptions'].append('Explicit scan API model accepts a20ms asynchronous request, reports pending then done, returns -ENOENT with NULL list; no radio/AP results or association')
    if args.gpio24_input_high:
        report['assumptions'].append('Explicit external GPIO24 high fixture means encoder button released; no synthetic input events')
    if args.mhint_latch:
        report['assumptions'].append('MHINT functional latch reset=0; bit20 semantics unknown')
    if args.mtvt_latch:
        report['assumptions'].append('MTVT storage; no SHV dispatch or full CLIC nesting')
    if args.boot_mmio:
        report['assumptions'].append('SDK register subset; unknown MMIO fails')
    if args.cache_csr_latches:
        report['assumptions'].append('MXSTATUS/MHCR/MEXSTATUS storage; no privilege/cache/reset/sleep effects')
    if args.efuse_shadow:
        report['assumptions'].append('eFuse shadow storage; absent fixture OTP transactions rejected')
    if args.otp_fixture:
        report['otp_fixture'] = dict(path=str(fixture), sha256=otp_digest,
                                    device_equivalence_verified=False)
        report['assumptions'].append('OTP read mock uses explicit fixture; not physical device data')
    if args.psram_mock:
        report['assumptions'].append('synthetic PSRAM config peripheral; no physical bus timing')
    if args.boot2_mock:
        report['assumptions'].append('synthetic Boot2 cached JEDEC1840ef with flash fixture, otherwise1740ef')
        report['assumptions'].append('EM selection is a native SDK configuration latch; shared480KiB RAM aliases are not dynamically repartitioned')
        report['assumptions'].append('functional RTC/UART/SPI/DMA; ideal 60Hz TE and output rotation')
        report['assumptions'].append('LCD SPI MISO is an explicit synthetic pull-high sample; no panel ID/readback emulation')
        report['function_stubs'].append(dict(
            symbol='RomDriver_SF_Cfg_Get_Flash_Cfg_Need_Lock', table_slot=320,
            table_address='0x90015d00', code_address='0x90000000',
            result='native fixture FCFG lookup when flash fixture and shim binding exist; otherwise -1 unsupported'))
        report['function_stubs'].append(dict(
            symbol='GLB_Power_On_XTAL_And_PLL_CLK', table_slot=115,
            code_address='0x90000020',
            result='explicit virtual clock request/state; supported only xtal4/pll3'))
        report['function_stubs'].append(dict(
            symbol='GLB_Set_MCU_System_CLK', table_slot=144, code_address='0x90000040',
            result='virtual MCU clock request/state; supported only selector5 after PLL setup'))
        report['function_stubs'].append(dict(
            symbol='HBN_Get_Xtal_Type', table_slot=227, code_address='0x90000060',
            result='reads virtual HBN state; valid marker writes one byte, otherwise ERROR1'))
    trace = deque(maxlen=64)
    if args.rom_shim:
        report['native_rom_shim'] = dict(path=str(shim), size=len(shim_data),
            sha256=hashlib.sha256(shim_data).hexdigest(), original_image_modified=False)
    if args.clic_mock:
        report['assumptions'].append('CLIC non-SHV raw IRQ0..79, fixed NLBITS4/threshold0; no nested priority state')
    if args.pec_uart_mock:
        report['pec_api_replacement'] = dict(enabled=True, original_pec_decrypted=False,
            table_overlay='0xa066e000..0xa066e200', synthetic_uart2=True,
            native_slots=['0x30','0x88','0x40','0x44','0x48','0x4c','0x50','0xe8'])
    if args.flash_fixture:
        report['flash_fixture'] = dict(path=str(flash), size=len(flash_data),
            sha256=hashlib.sha256(flash_data).hexdigest(), synthetic=True,
            mutable_model_range='0x710000..0x720000; in-memory only',
            physical_device_layout_verified=False)
    attempts, process, gdb_sock, qmp_sock, qmp = 0, None, None, None, None
    observation_stop, observation_done = threading.Event(), threading.Event()
    deadline = time.monotonic() + args.timeout
    # Exclusive creation also reserves the output name before spawning.
    with output.open('x', encoding='utf-8') as result, log_path.open('x') as log, uart_path.open('xb'), uart2_path.open('xb'):
        report['uart_log'] = str(uart_path)
        if args.pec_uart_mock:
            report['uart2_log'] = str(uart2_path)
        try:
            version = subprocess.run([str(qemu), '--version'], capture_output=True,
                                     text=True, timeout=max(0.001, deadline - time.monotonic()),
                                     check=True)
            report['qemu_version'] = version.stdout.strip()
            process = subprocess.Popen(command, stdin=subprocess.DEVNULL,
                                       stdout=log, stderr=log, preexec_fn=child_limits)
            gdb_sock = connect(gdb_path, process, deadline)
            qmp_sock = connect(qmp_path, process, deadline)
            rsp = RSP(gdb_sock, deadline)
            qmp = QMP(qmp_sock, deadline)
            scan = EmptyScan(lambda address, count: bytes.fromhex(rsp.request(f'm{address:x},{count:x}')), report) if args.scan_empty_mock else None
            report['qemu_sha256'] = hashlib.sha256(Path(f'/proc/{process.pid}/exe').read_bytes()).hexdigest()
            if args.observe_seconds:
                def observe():
                    if not observation_done.wait(args.observe_seconds):
                        observation_stop.set()
                        try:
                            gdb_sock.sendall(b'\x03')
                        except OSError:
                            pass
                threading.Thread(target=observe, daemon=True).start()
            report['initial_stop'] = rsp.request('?')
            for address in args.breakpoint:
                if not 0 <= address <= 0xffffffff or rsp.request(f'Z0,{address:x},2') != 'OK':
                    raise RuntimeError(f'QEMU refused boundary breakpoint {address:#x}')
            if args.ble_unavailable:
                if not args.run_to_trap or rsp.request('Z0,a00da95e,2') != 'OK':
                    raise RuntimeError('BLE unavailable model requires run-to-trap and its API breakpoint')
            if args.scan_unavailable:
                if not args.run_to_trap or rsp.request('Z0,a00761b4,2') != 'OK':
                    raise RuntimeError('scan unavailable model requires run-to-trap and its API breakpoint')
            if args.adc_unavailable:
                if not args.run_to_trap or rsp.request('Z0,a0025e1e,2') != 'OK':
                    raise RuntimeError('ADC unavailable model requires run-to-trap and its API breakpoint')
            if scan:
                if not args.run_to_trap:
                    raise RuntimeError('empty scan model requires run-to-trap')
                for address in SCAN_APIS:
                    if rsp.request(f'Z0,{address:x},2') != 'OK':
                        raise RuntimeError('cannot install scan API breakpoint')
            if args.run_to_trap:
                if rsp.request('Z0,0,2') != 'OK':
                    raise RuntimeError('QEMU refused trap-boundary breakpoint')
                report['execution_mode'] = 'continue-to-pc-zero'
                pc, sp = registers(rsp)
                trace.append(dict(pc=hex(pc), bytes=rsp.request(f'm{pc:x},4')))
                report['last_step_reply'] = rsp.request('c')
                for _ in range(args.pass_interrupts + args.pass_ecalls + (1024 if scan else 128 if args.ble_unavailable or args.scan_unavailable or args.adc_unavailable else 0)):
                    if observation_stop.is_set():
                        break
                    pc, _ = registers(rsp)
                    if scan and pc in SCAN_APIS:
                        raw = bytes.fromhex(rsp.request('g'))
                        regs = [int.from_bytes(raw[i:i+4], 'little') for i in range(0, 132, 4)]
                        value = scan.handle(pc, regs[10:16])
                        if rsp.request('Pa=' + (value & 0xffffffff).to_bytes(4, 'little').hex()) != 'OK' or rsp.request('P20=' + regs[1].to_bytes(4, 'little').hex()) != 'OK':
                            raise RuntimeError('cannot return explicit scan API result')
                        report['last_step_reply'] = rsp.request('c')
                        continue
                    if args.adc_unavailable and pc == 0xa0025e1e:
                        raw = bytes.fromhex(rsp.request('g'))
                        argument = int.from_bytes(raw[40:44], 'little')
                        if rsp.request(f'm{argument:x},4') == '61646300':
                            ra = int.from_bytes(raw[4:8], 'little')
                            if rsp.request('Pa=00000000') != 'OK' or rsp.request('P20=' + ra.to_bytes(4, 'little').hex()) != 'OK':
                                raise RuntimeError('cannot provide missing ADC device')
                            report['adc_unavailable_calls'] += 1
                        else:
                            rsp.request('z0,a0025e1e,2')
                            rsp.request('s')
                            rsp.request('Z0,a0025e1e,2')
                        report['last_step_reply'] = rsp.request('c')
                        continue
                    provider = ('ble_unavailable_calls' if args.ble_unavailable and pc == 0xa00da95e else
                                'scan_unavailable_calls' if args.scan_unavailable and pc == 0xa00761b4 else None)
                    if provider:
                        if report[provider] >= 8:
                            report['stop'] = {'reason': provider + '-budget'}
                            break
                        raw = bytes.fromhex(rsp.request('g'))
                        ra = int.from_bytes(raw[4:8], 'little')
                        if rsp.request('Pa=edffffff') != 'OK' or rsp.request('P20=' + ra.to_bytes(4, 'little').hex()) != 'OK':
                            raise RuntimeError('cannot return typed -ENODEV from explicit unavailable API')
                        report[provider] += 1
                        report['last_step_reply'] = rsp.request('c')
                        continue
                    if pc != 0x62fc2500:
                        break
                    regs = qmp.command('human-monitor-command', {'command-line': 'info registers'})
                    cause = re.search(r'\bmcause\s+([0-9a-fA-F]+)', regs)
                    if not cause:
                        break
                    cause_number = int(cause.group(1), 16)
                    if cause_number & 0x80000000:
                        counts = report.setdefault('passed_interrupts', {})
                        if sum(counts.values()) >= args.pass_interrupts:
                            break
                        irq = str(cause_number & 0xfff)
                        counts[irq] = counts.get(irq, 0) + 1
                    elif cause_number == 11 and report.get('passed_ecalls', 0) < args.pass_ecalls:
                        report['passed_ecalls'] = report.get('passed_ecalls', 0) + 1
                    else:
                        break
                    rsp.request('z0,62fc2500,2')
                    rsp.request('s')
                    rsp.request('Z0,62fc2500,2')
                    report['last_step_reply'] = rsp.request('c')
                report['stop'] = {'reason': 'observation-complete' if observation_stop.is_set() else 'continue-stop'}
                if args.observe_seconds:
                    report['observation_seconds'] = args.observe_seconds
                    report['guest_uptime_ms'] = int.from_bytes(bytes.fromhex(rsp.request('m62fc6c64,4')), 'little')
                for _ in range(args.steps_after_boundary):
                    pc, sp = registers(rsp)
                    trace.append(dict(pc=hex(pc), bytes=rsp.request(f'm{pc:x},4')))
                    attempts += 1
                    report['last_step_reply'] = rsp.request('s')
            for _ in range(0 if args.run_to_trap else args.steps):
                pc, sp = registers(rsp)
                report.update(pc=hex(pc), sp=hex(sp))
                if pc == 0:
                    report['stop'] = {'reason': 'pc-zero-trap-boundary'}
                    break
                instruction = rsp.request(f'm{pc:x},4')
                if len(bytes.fromhex(instruction)) != 4:
                    raise ValueError('short instruction memory response')
                trace.append(dict(pc=hex(pc), bytes=instruction))
                attempts += 1
                reply = rsp.request('s')
                report['last_step_reply'] = reply
                if not reply.startswith(('S', 'T')):
                    report['stop'] = {'reason': 'unexpected-step-reply', 'reply': reply}
                    break
            else:
                if not args.run_to_trap:
                    report['stop'] = {'reason': 'step-budget'}
            pc, sp = registers(rsp)
            report.update(pc=hex(pc), sp=hex(sp))
            if pc == 0:
                report['stop'] = {'reason': 'pc-zero-trap-boundary'}
            report['registers_hmp'] = qmp.command(
                'human-monitor-command', {'command-line': 'info registers'})
            trap = {}
            for name in ('mepc', 'mcause', 'mtval', 'mtvec'):
                match = re.search(r'\b' + name + r'\s+([0-9a-fA-F]+)', report['registers_hmp'])
                if match:
                    trap[name] = '0x' + match.group(1)
            report['trap_registers'] = trap
            if memory_ranges:
                report['memory_captures'] = [
                    dict(address=hex(address), length=length,
                         hex=rsp.request(f'm{address:x},{length:x}'))
                    for address, length in memory_ranges]
            fault_pc = int(trap.get('mepc', '0'), 16)
            if 0xa0000000 <= fault_pc < 0xa0680000:
                report['fault_code_hex'] = rsp.request(f'm{fault_pc:x},40')
            if 0x62fc0000 <= sp <= 0x63037b00:
                stack = bytes.fromhex(rsp.request(f'm{sp:x},500'))
                report['stack_rom_pointer_candidates'] = [
                    dict(stack_address=hex(sp + i), value=hex(value))
                    for i in range(0, len(stack), 4)
                    if 0xa0000000 <= (value := int.from_bytes(stack[i:i+4], 'little')) < 0xa0680000]
            if 0x62fc0000 <= fault_pc < 0x63037fc0:
                sample = bytes.fromhex(rsp.request(f'm{fault_pc:x},40'))
                offset = source.find(sample)
                report['ram_code_match'] = dict(
                    address=hex(fault_pc), size=len(sample),
                    sha256=hashlib.sha256(sample).hexdigest(),
                    original_file_offset=hex(offset) if offset >= 0 else None,
                    matches_original=offset >= 0 and len(sample) == 64)
        except Exception as exc:
            report['stop'] = {'reason': 'timeout' if isinstance(exc, (TimeoutError, socket.timeout))
                              else 'probe-error', 'type': type(exc).__name__, 'detail': str(exc)}
            # A bounded run can be a busy wait after mtvec was installed.
            # Stop over the independent channel and preserve the actual PC.
            if qmp is not None and process is not None and process.poll() is None:
                try:
                    qmp.deadline = time.monotonic() + 3
                    qmp.command('stop')
                    registers_text = qmp.command('human-monitor-command', {'command-line': 'info registers'})
                    report['registers_hmp'] = registers_text
                    for key, pattern in (('pc', r'\bpc\s+([0-9a-fA-F]+)'),
                                         ('sp', r'\bx2/sp\s+([0-9a-fA-F]+)')):
                        if match := re.search(pattern, registers_text):
                            report[key] = '0x' + match.group(1)
                    report['trap_registers'] = {
                        name: '0x' + match.group(1)
                        for name in ('mepc', 'mcause', 'mtval', 'mtvec')
                        if (match := re.search(r'\b' + name + r'\s+([0-9a-fA-F]+)', registers_text))}
                except Exception as diagnostic_error:
                    report['stop']['diagnostic_error'] = str(diagnostic_error)
        finally:
            if qmp is not None:
                if process is not None and process.poll() is None:
                    try:
                        qmp.deadline = time.monotonic() + 3
                        qmp.command('stop')
                        capture_display(qmp, output, report)
                    except Exception as display_error:
                        report['display_capture_error'] = str(display_error)
                qmp.file.close()
            for sock in (gdb_sock, qmp_sock):
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
                report['qemu_exit_code'] = process.returncode
                for path in (gdb_path, qmp_path):
                    if path.exists():
                        path.unlink()
            observation_done.set()
            uart_bytes = uart_path.read_bytes()
            fatal = re.search(rb'EXCEPTION: [^\r\n]+', uart_bytes)
            if fatal:
                report['first_guest_exception'] = fatal.group().decode('ascii', 'replace')
            report['native_ui_initialized'] = b'lv_ap01_display_process, finish' in uart_bytes
            report['continuous_runtime_verified'] = bool(args.observe_seconds and
                report.get('stop', {}).get('reason') == 'observation-complete' and
                not fatal and report.get('guest_uptime_ms', 0) >= 1000 and
                report['native_ui_initialized'])
            report.update(attempted_steps=attempts, last_pre_step_trace=list(trace),
                          source_unchanged=hashlib.sha256(image.read_bytes()).hexdigest() == image_identity['current_sha256'],
                          qemu_log=str(log_path))
            json.dump(report, result, indent=2)
            result.write('\n')
    print(json.dumps({key: report.get(key) for key in
                      ('attempted_steps', 'pc', 'sp', 'stop', 'full_rom_boot_verified')}))
    return 1 if (not report['source_unchanged'] or
                 (args.observe_seconds and not report.get('continuous_runtime_verified')) or
                 report['stop']['reason'] in ('timeout', 'probe-error', 'unexpected-step-reply')) else 0


if __name__ == '__main__':
    sys.exit(main())
